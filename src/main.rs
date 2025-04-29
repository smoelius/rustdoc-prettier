//! # rustdoc-prettier
//!
//! Format `//!` and `///` comments with prettier

use anyhow::{Result, anyhow, bail, ensure};
use assert_cmd::output::OutputError;
use glob::glob;
use itertools::Itertools;
use rewriter::{Backup, LineColumn, Rewriter, Span};
use std::{
    env,
    fs::{read_to_string, write},
    io::Write,
    ops::Range,
    path::Path,
    process::{Child, Command, Stdio, exit},
    sync::{
        Condvar, LazyLock, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread,
};

mod resolve_project_file;
use resolve_project_file::resolve_project_file;

#[derive(Clone, Default)]
struct Options {
    max_width: Option<usize>,
    patterns: Vec<String>,
    args: Vec<String>,
}

#[derive(Debug)]
struct Chunk {
    lines: Range<usize>,
    characteristics: Characteristics,
    docs: String,
}

/// Describes doc comments that need formatting
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Characteristics {
    indent: usize,
    kind: DocKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DocKind {
    Inner,
    Outer,
}

static N_THREADS: LazyLock<usize> = LazyLock::new(|| {
    std::cmp::max(
        1,
        thread::available_parallelism()
            .unwrap()
            .get()
            .saturating_sub(1),
    )
});

static CTRLC: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    ctrlc::set_handler(|| CTRLC.store(true, Ordering::SeqCst))?;
    let mut opts = process_args()?;
    if opts.max_width.is_none() {
        opts.max_width = rustfmt_max_width()?;
    }

    let mut backups = Vec::new();
    let mut handles = Vec::new();
    // smoelius: Split off `opts.patterns` so that its contents are not cloned before each call to
    // `thread::spawn`.
    for pattern in opts.patterns.split_off(0) {
        for result in glob(&pattern)? {
            let path = result?;
            let backup = Backup::new(&path)?;
            backups.push(backup);
            let opts = opts.clone();
            handles.push(thread::spawn(|| format_file(opts, path)));
        }
    }

    for handle in handles {
        join_anyhow(handle)?;
    }
    for mut backup in backups {
        backup.disable()?;
    }
    Ok(())
}

fn process_args() -> Result<Options> {
    let mut opts = Options::default();
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--help" || arg == "-h" {
            help();
        } else if arg == "--max-width" {
            let Some(arg) = iter.next() else {
                bail!("missing argument to --max--width");
            };
            let width = arg.parse()?;
            opts.max_width = Some(width);
        } else if let Some(arg) = arg.strip_prefix("--max-width=") {
            let width = arg.parse()?;
            opts.max_width = Some(width);
        } else if arg.to_lowercase().ends_with(".rs") {
            opts.patterns.push(arg);
        } else {
            opts.args.push(arg);
        }
    }
    Ok(opts)
}

#[rustfmt::skip]
const HELP: &str = "\
Usage: rustdoc-prettier [ARGS]

Arguments ending with `.rs` are considered source files and are
formatted. All other arguments are forwarded to `prettier`, with
one exception. An option of the form:

    ---max-width <N>

is converted to options of the form:

    --prose-wrap always --print-width <M>

where `M` is `N` minus the sum of the widths of the indentation,
the `//!` or `///` syntax, and the space that might follow that
syntax. If a rustfmt.toml file with a `max_width` key is found
in the current directory or a parent directory, the
`--max-width` option is applied automatically.

rustdoc-prettier supports glob patterns. Example:

    rustdoc-prettier '**/*.rs'

References

- https://prettier.io/docs/en/options.html
- https://rust-lang.github.io/rustfmt/?version=master&search=
";

fn help() -> ! {
    println!("{HELP}");
    exit(0);
}

fn rustfmt_max_width() -> Result<Option<usize>> {
    let current_dir = env::current_dir()?;
    let Some(path) = resolve_project_file(&current_dir)? else {
        return Ok(None);
    };
    let contents = read_to_string(path)?;
    let table = contents.parse::<toml::Table>()?;
    let Some(max_width) = table.get("max_width") else {
        return Ok(None);
    };
    let Some(max_width_i64) = max_width.as_integer() else {
        bail!("`max_width` is not an integer");
    };
    let max_width = usize::try_from(max_width_i64)?;
    Ok(Some(max_width))
}

fn format_file(opts: Options, path: impl AsRef<Path>) -> Result<()> {
    let contents = read_to_string(&path)?;

    let chunks = chunk(&contents);
    let characteristics = chunks
        .iter()
        .map(|chunk| chunk.characteristics)
        .collect::<Vec<_>>();

    let (sender, receiver) = sync_channel::<Child>(*N_THREADS);
    let handle = thread::spawn(move || prettier_spawner(&opts, characteristics, &sender));

    let mut rewriter = Rewriter::new(&contents);

    for chunk in chunks {
        if CTRLC.load(Ordering::SeqCst) {
            bail!("Ctrl-C detected");
        }

        let docs = format_chunk(&receiver, &chunk)?;

        let start = LineColumn {
            line: chunk.lines.start,
            column: 0,
        };
        let end = LineColumn {
            line: chunk.lines.end,
            column: 0,
        };
        let span = Span::new(start, end);

        rewriter.rewrite(&span, &docs);
    }

    let contents = rewriter.contents();

    write(path, contents)?;

    join_anyhow(handle)?;

    Ok(())
}

fn chunk(contents: &str) -> Vec<Chunk> {
    let mut line_curr = 1;
    let mut chunks = Vec::new();
    for (key, key_line_pairs) in &contents
        .lines()
        .map(preprocess_line)
        .chunk_by(|&(key, _)| key)
    {
        let lines = key_line_pairs.map(|(_key, line)| line).collect::<Vec<_>>();
        let line_prev = line_curr;
        line_curr += lines.len();
        if let Some(characteristics) = key {
            chunks.push(Chunk {
                lines: line_prev..line_curr,
                characteristics,
                docs: lines.iter().map(|line| format!("{line}\n")).collect(),
            });
        }
    }
    chunks
}

fn preprocess_line(line: &str) -> (Option<Characteristics>, &str) {
    let indent = line.chars().take_while(char::is_ascii_whitespace).count();
    let unindented = &line[indent..];
    let (characteristics, suffix) = if let Some(suffix) = unindented.strip_prefix("//!") {
        (
            Characteristics {
                indent,
                kind: DocKind::Inner,
            },
            suffix,
        )
    } else if let Some(suffix) = unindented.strip_prefix("///") {
        (
            Characteristics {
                indent,
                kind: DocKind::Outer,
            },
            suffix,
        )
    } else {
        return (None, "");
    };

    // smoelius: Skip at most one whitespace character after the `//!` or `///`.
    let i = suffix
        .chars()
        .next()
        .and_then(|c| {
            if c.is_whitespace() {
                Some(c.len_utf8())
            } else {
                None
            }
        })
        .unwrap_or(0);

    (Some(characteristics), &suffix[i..])
}

/// Spawns a `prettier` instance for each element of `characteristics`, and sends the instance over
/// `sender`
///
/// Note that `characteristics` influences the arguments passed to `prettier`. So the `prettier`
/// instances must be consumed in the same order in which they were spawned.
#[allow(clippy::unnecessary_wraps)]
fn prettier_spawner(
    opts: &Options,
    characteristics: Vec<Characteristics>,
    sender: &SyncSender<Child>,
) -> Result<()> {
    for characteristics in characteristics {
        let mut used_parallelism = lock_used_parallelism_for_incrementing();
        let mut command = Command::new("prettier");
        command.arg("--parser=markdown");
        if let Some(max_width) = opts.max_width {
            command.arg("--prose-wrap=always");
            command.arg(format!(
                "--print-width={}",
                max_width.saturating_sub(characteristics.indent + 4)
            ));
        }
        command.args(&opts.args);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn().expect("failed to spawn `prettier`");
        // smoelius: The next send should never fail. The channel is created with a capacity of
        // `N_THREADS`, and no more than `N_THREADS` children exist at any time.
        sender.try_send(child).unwrap_or_else(|error| {
            panic!(
                "tried to send more than {} children on channel: {error:?}",
                *N_THREADS
            )
        });
        *used_parallelism += 1;
    }
    Ok(())
}

fn format_chunk(receiver: &Receiver<Child>, chunk: &Chunk) -> Result<String> {
    let mut prettier = receiver.recv()?;
    let mut stdin = prettier
        .stdin
        .take()
        .ok_or_else(|| anyhow!("child has no stdin"))?;

    stdin.write_all(chunk.docs.as_bytes())?;
    drop(stdin);

    let output = prettier.wait_with_output()?;
    ensure!(
        output.status.success(),
        "prettier exited abnormally: {}",
        OutputError::new(output)
    );

    decrement_used_parallelism();

    let docs = String::from_utf8(output.stdout)?;

    Ok(postprocess_docs(chunk.characteristics, &docs))
}

static USED_PARALLELISM: Mutex<usize> = Mutex::new(0);
static USED_PARALLELISM_CONDVAR: Condvar = Condvar::new();

fn lock_used_parallelism_for_incrementing() -> MutexGuard<'static, usize> {
    let used_parallelism = USED_PARALLELISM.lock().unwrap();
    USED_PARALLELISM_CONDVAR
        .wait_while(used_parallelism, |used_parallelism| {
            *used_parallelism >= *N_THREADS
        })
        .unwrap()
}

fn decrement_used_parallelism() {
    let mut used_parallelism = USED_PARALLELISM.lock().unwrap();
    *used_parallelism -= 1;
    USED_PARALLELISM_CONDVAR.notify_one();
}

fn postprocess_docs(characteristics: Characteristics, docs: &str) -> String {
    let Characteristics { indent, kind, .. } = characteristics;
    docs.lines()
        .map(|line| {
            format!(
                "{:indent$}{}{}{}\n",
                "",
                match kind {
                    DocKind::Inner => "//!",
                    DocKind::Outer => "///",
                },
                if line.is_empty() { "" } else { " " },
                line,
            )
        })
        .collect()
}

fn join_anyhow<T>(handle: thread::JoinHandle<Result<T>>) -> Result<T> {
    handle
        .join()
        .map_err(|error| anyhow!("{error:?}"))
        .and_then(std::convert::identity)
}

#[cfg(test)]
mod test {
    use std::fs::read_to_string;

    #[test]
    fn readme_contains_help() {
        let readme = read_to_string("README.md").unwrap();
        // smoelius: Skip the first two lines, which give the usage.
        let help = super::HELP
            .split_inclusive('\n')
            .skip(2)
            .collect::<String>();
        assert!(readme.contains(&help));
    }
}
