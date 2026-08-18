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
use glob::{GlobError, MatchOptions, glob_with};
use itertools::Itertools;
use methodify::methodify;
use rewriter::{Backup, LineColumn, Rewriter, Span};
use std::{
    env,
    fs::{read_to_string, write},
    io,
    ops::Range,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Condvar, LazyLock, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread,
};

mod resolve_project_file;
use resolve_project_file::resolve_project_file;

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
syntax. If a rustfmt.toml file is found in a current or parent
directory, and the file has a `max_width` or `comment_width`
key, the `--max-width` option is applied automatically.

rustdoc-prettier supports glob patterns. Example:

    rustdoc-prettier '**/*.rs'

References

- https://prettier.io/docs/en/options.html
- https://rust-lang.github.io/rustfmt/?version=main&search=
";

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
    let Some(mut opts) = process_args()? else {
        return Ok(());
    };
    if opts.max_width.is_none() {
        opts.max_width = rustfmt_max_width()?;
    }

    check_if_prettier_is_installed().with_context(|| "failed to run `prettier`")?;

    let mut backups = Vec::new();
    let mut handles = Vec::new();
    // smoelius: Split off `opts.patterns` so that its contents are not cloned before each call to
    // `thread::spawn`.
    for pattern in opts.patterns.split_off(0) {
        let mut found = false;
        let match_options = MatchOptions {
            require_literal_leading_dot: true,
            ..MatchOptions::new()
        };
        for result in glob_with(&pattern, match_options)? {
            let Some(path) = result
                .map_err(GlobError::into)
                .ignore_not_found(|| format!("failed while reading `{pattern}`"))?
            else {
                continue;
            };
            let Some(backup) = Backup::new(&path)
                .treat_einval_as_not_found_on_macos()
                .ignore_not_found(|| format!("failed while backing up `{}`", path.display()))?
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
            .ignore_not_found(|| String::from("failed while disabling backup"))?;
    }
    Ok(())
}

fn process_args() -> Result<Option<Options>> {
    let mut opts = Options::default();
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--help" || arg == "-h" {
            println!("{HELP}");
            return Ok(None);
        } else if arg == "--max-width" {
            let Some(arg) = iter.next() else {
                bail!("missing argument to --max--width");
            };
            let width = arg.parse()?;
            opts.max_width = Some(width);
        } else if let Some(arg) = arg.strip_prefix("--max-width=") {
            let width = arg.parse()?;
            opts.max_width = Some(width);
        } else if arg == "--version" || arg == "-V" {
            version();
            return Ok(None);
        } else if arg.to_lowercase().ends_with(".rs") {
            opts.patterns.push(arg);
        } else {
            if arg == "--check" {
                opts.check = true;
            }
            opts.args.push(arg);
        }
    }
    Ok(Some(opts))
}

fn version() {
    const RUSTDOC_PRETTIER_VERSION: &str = env!("CARGO_PKG_VERSION");
    let node_version = program_version("node").unwrap_or_else(|_| String::from("??"));
    let prettier_version = program_version("prettier").unwrap_or_else(|_| String::from("??"));
    println!(
        "rustdoc-prettier {RUSTDOC_PRETTIER_VERSION} (node {node_version}, prettier {prettier_version})"
    );
}

fn rustfmt_max_width() -> Result<Option<usize>> {
    let current_dir = current_dir_wc()?;
    let Some(path) =
        resolve_project_file(&current_dir).with_context(|| "failed to find `rustfmt.toml` file")?
    else {
        return Ok(None);
    };
    let contents = read_to_string_wc(path)?;
    let table = contents.parse::<toml::Table>()?;
    let Some(max_width) = table
        .get("max_width")
        .or_else(|| table.get("comment_width"))
    else {
        return Ok(None);
    };
    let Some(max_width_i64) = max_width.as_integer() else {
        bail!("`max_width`/`comment_width` is not an integer");
    };
    let max_width = usize::try_from(max_width_i64)?;
    Ok(Some(max_width))
}

fn check_if_prettier_is_installed() -> Result<()> {
    program_version("prettier").map(|_| ())
}

fn program_version(program: &str) -> Result<String> {
    let output = Command::new(program).arg("--version").output_wc()?;
    if !output.status.success() {
        bail!(
            "`{program} --version` exited {}",
            exit_status_to_string(output.status)
        );
    }
    str::from_utf8(output.stdout.trim_ascii_end())
        .map(ToOwned::to_owned)
        .map_err(Into::into)
}

fn format_file(opts: Options, path: impl AsRef<Path>) -> Result<()> {
    let check = opts.check;
    #[allow(clippy::disallowed_methods)]
    let Some(contents) = read_to_string(&path)
        .ignore_not_found(|| format!("failed while reading `{}`", path.as_ref().display()))?
    else {
        return Ok(());
    };

    let chunks = chunk(&contents);
    let characteristics = chunks
        .iter()
        .map(|chunk| chunk.characteristics)
        .collect::<Vec<_>>();

    let (sender, receiver) = sync_channel::<Prettier>(*N_THREADS);
    let handle = thread::spawn(move || prettier_spawner(&opts, &characteristics, &sender));

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
            .treat_einval_as_not_found_on_macos()
            .ignore_not_found(|| format!("failed while writing `{}`", path.as_ref().display()))?;
    }

    join_anyhow(handle)?;

    Ok(())
}

/// Warns about and converts a macOS `EINVAL` error into an [`io::ErrorKind::NotFound`] error
///
/// Creating a file in a directory that is concurrently removed fails with `ENOENT` on Linux, but
/// can fail with `EINVAL` on macOS.
#[methodify]
fn treat_einval_as_not_found_on_macos<T>(result: io::Result<T>) -> io::Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            // `tempfile` wraps the underlying error in a type that reports neither its
            // `raw_os_error` nor it as a `source`. But the error's kind is preserved, and `EINVAL`
            // maps to `InvalidInput`.
            if cfg!(target_os = "macos") && error.kind() == io::ErrorKind::InvalidInput {
                eprintln!("Warning: treating {error} as not found");
                Err(io::Error::new(io::ErrorKind::NotFound, error))
            } else {
                Err(error)
            }
        }
    }
}

#[methodify]
fn ignore_not_found<T>(
    result: io::Result<T>,
    context: impl FnOnce() -> String,
) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            let context = context();
            if error.kind() == io::ErrorKind::NotFound {
                eprintln!("Warning: {context}: {error}");
                Ok(None)
            } else {
                Err(error).context(context)
            }
        }
    }
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

struct Prettier {
    child: Child,
    decrement_used_parallelism: DecrementUsedParallelism,
}

/// Spawns a `prettier` instance for each element of `characteristics`, and sends the instance over
/// `sender`
///
/// Note that `characteristics` influences the arguments passed to `prettier`. So the `prettier`
/// instances must be consumed in the same order in which they were spawned.
#[allow(clippy::unnecessary_wraps)]
fn prettier_spawner(
    opts: &Options,
    characteristics: &[Characteristics],
    sender: &SyncSender<Prettier>,
) -> Result<()> {
    for &characteristics in characteristics {
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
        // smoelius: The `sender` channel is created with a capacity of `N_THREADS`, and no more
        // than `N_THREADS` children exist at any time. For these reasons, the next `try_send`
        // should fail only if prettier exits. In that case, we should unwind gracefully so that an
        // error message returned elsewhere can be displayed to the user.
        *used_parallelism += 1;
        let prettier = Prettier {
            child,
            decrement_used_parallelism: DecrementUsedParallelism,
        };
        drop(used_parallelism);
        sender
            .try_send(prettier)
            .with_context(|| "failed to send `prettier`")?;
    }
    Ok(())
}

fn format_chunk(receiver: &Receiver<Prettier>, chunk: &Chunk) -> Result<String> {
    let Prettier {
        mut child,
        decrement_used_parallelism,
    } = receiver.recv()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("child has no stdin"))?;

    stdin.write_all_wc(chunk.docs.as_bytes())?;
    drop(stdin);

    let output = child.wait_with_output_wc()?;
    ensure!(
        output.status.success(),
        "prettier exited {}",
        exit_status_to_string(output.status)
    );

    drop(decrement_used_parallelism);

    let docs = String::from_utf8(output.stdout)?;

    Ok(postprocess_docs(chunk.characteristics, &docs))
}

fn exit_status_to_string(status: ExitStatus) -> String {
    status
        .code_wc()
        .map(|code| format!("with code {code}"))
        .unwrap_or(String::from("abnormally"))
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

struct DecrementUsedParallelism;

impl Drop for DecrementUsedParallelism {
    fn drop(&mut self) {
        decrement_used_parallelism();
    }
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
mod tests {
    use super::*;
    use elaborate::std::fs::read_to_string_wc;
    use std::sync::{
        Mutex,
        mpsc::{Receiver, sync_channel},
    };

    #[test]
    fn readme_contains_help() {
        let readme = read_to_string_wc("README.md").unwrap();
        // smoelius: Skip the first two lines, which give the usage.
        let help = HELP.split_inclusive('\n').skip(2).collect::<String>();
        assert!(readme.contains(&help));
    }

    // smoelius: `used_parallelism_is_decremented_when_format_chunk_fails` and
    // `used_parallelism_is_decremented_when_queued_prettier_is_dropped` both use
    // `USED_PARALLELISM`. Ensure the two tests do not interfere with each other.
    static USED_PARALLELISM_TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn used_parallelism_is_decremented_when_format_chunk_fails() {
        let _guard = USED_PARALLELISM_TEST_MUTEX.lock().unwrap();
        assert_eq!(*USED_PARALLELISM.lock().unwrap(), 0);

        let chunk = chunk("///  Needs formatting\n").remove(0);
        let receiver = spawn_prettier_instance(&chunk);

        assert!(format_chunk(&receiver, &chunk).is_err());
        assert_eq!(*USED_PARALLELISM.lock().unwrap(), 0);
    }

    #[test]
    fn used_parallelism_is_decremented_when_queued_prettier_is_dropped() {
        let _guard = USED_PARALLELISM_TEST_MUTEX.lock().unwrap();
        assert_eq!(*USED_PARALLELISM.lock().unwrap(), 0);

        let chunk = chunk("///  Needs formatting\n").remove(0);
        let receiver = spawn_prettier_instance(&chunk);

        // Dropping a queued child must decrement `USED_PARALLELISM`.
        drop(receiver);
        assert_eq!(*USED_PARALLELISM.lock().unwrap(), 0);
    }

    /// Spawns a `prettier` instance to check `chunk` and returns a [`Receiver`] from which it can
    /// be retrieved.
    fn spawn_prettier_instance(chunk: &Chunk) -> Receiver<Prettier> {
        let opts = Options {
            args: vec![String::from("--check")],
            ..Options::default()
        };
        let (sender, receiver) = sync_channel(1);
        prettier_spawner(&opts, &[chunk.characteristics], &sender).unwrap();
        assert_eq!(*USED_PARALLELISM.lock().unwrap(), 1);
        receiver
    }
}
