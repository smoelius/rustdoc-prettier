//! # rustdoc-prettier
//!
//! Format `//!` and `///` comments with prettier

use anyhow::{Context, Result, anyhow, bail, ensure};
use elaborate::std::{
    env::current_dir_wc,
    fs::read_to_string_wc,
    io::WriteContext,
    process::{ChildContext, CommandContext, ExitStatusContext},
    thread::available_parallelism_wc,
};
use glob::{GlobError, glob};
use itertools::Itertools;
use rewriter::{Backup, LineColumn, Rewriter, Span};
use std::{
    env,
    fs::{read_to_string, write},
    io,
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

trait IgnoreNotFound<T> {
    fn ignore_not_found(self, what: impl Fn() -> String) -> io::Result<Option<T>>;
}

impl<T> IgnoreNotFound<T> for io::Result<T> {
    fn ignore_not_found(self, what: impl Fn() -> String) -> io::Result<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                if error.kind() == io::ErrorKind::NotFound {
                    let what = what();
                    eprintln!("Warning: failed while {what}: {error}");
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }
}

#[derive(Clone, Default)]
struct Options {
    /// Preferred maximum width of a formatted line
    max_width: Option<usize>,
    /// Source files to format
    patterns: Vec<String>,
    /// Whether `args` includes `--check` and thus files should not be overwritten
    check: bool,
    /// Arguments to pass to `prettier`
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
        available_parallelism_wc().unwrap().get().saturating_sub(1),
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
        let mut found = false;
        for result in glob(&pattern)? {
            let Some(path) = result
                .map_err(GlobError::into_error)
                .ignore_not_found(|| format!("reading `{pattern}`"))?
            else {
                continue;
            };
            let Some(backup) = Backup::new(&path)
                .ignore_not_found(|| format!("backing up `{}`", path.display()))?
            else {
                continue;
            };
            backups.push(backup);
            let opts = opts.clone();
            handles.push(thread::spawn(|| format_file(opts, path)));
            found = true;
        }
        ensure!(found, "found no files matching pattern: {pattern}");
    }

    for handle in handles {
        join_anyhow(handle)?;
    }
    for mut backup in backups {
        let _: Option<()> = backup
            .disable()
            .ignore_not_found(|| String::from("disabling backup"))?;
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
            if arg == "--check" {
                opts.check = true;
            }
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
    let current_dir = current_dir_wc()?;
    let Some(path) = resolve_project_file(&current_dir)? else {
        return Ok(None);
    };
    let contents = read_to_string_wc(path)?;
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
    let check = opts.check;
    #[allow(clippy::disallowed_methods)]
    let Some(contents) = read_to_string(&path)
        .ignore_not_found(|| format!("reading `{}`", path.as_ref().display()))?
    else {
        return Ok(());
    };

    let chunks = chunk(&contents);
    let characteristics = chunks
        .iter()
        .map(|chunk| chunk.characteristics)
        .collect::<Vec<_>>();

    let (sender, receiver) = sync_channel::<Child>(*N_THREADS);
    let handle = thread::spawn(move || prettier_spawner(opts, characteristics, &sender));

    let mut rewriter = Rewriter::new(&contents);

    for chunk in chunks {
        if CTRLC.load(Ordering::SeqCst) {
            bail!("Ctrl-C detected");
        }

        let docs = format_chunk(&receiver, &chunk).with_context(|| {
            format!(
                "failed to format {}:{:?}",
                path.as_ref().display(),
                chunk.lines
            )
        })?;

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

    if !check {
        #[allow(clippy::disallowed_methods)]
        write(&path, contents)
            .ignore_not_found(|| format!("writing `{}`", path.as_ref().display()))?;
    }

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
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn prettier_spawner(
    opts: Options,
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
        let child = command.spawn_wc().expect("failed to spawn `prettier`");
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

    stdin.write_all_wc(chunk.docs.as_bytes())?;
    drop(stdin);

    let output = prettier.wait_with_output_wc()?;
    ensure!(
        output.status.success(),
        "prettier exited {}",
        output
            .status
            .code_wc()
            .map(|code| format!("with code {code}"))
            .unwrap_or(String::from("abnormally"))
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
    use elaborate::std::fs::read_to_string_wc;

    #[test]
    fn readme_contains_help() {
        let readme = read_to_string_wc("README.md").unwrap();
        // smoelius: Skip the first two lines, which give the usage.
        let help = super::HELP
            .split_inclusive('\n')
            .skip(2)
            .collect::<String>();
        assert!(readme.contains(&help));
    }
}
