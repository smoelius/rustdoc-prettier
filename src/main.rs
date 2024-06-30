//! # rustdoc-prettier
//!
//! Format `//!` and `///` comments with prettier

use anyhow::{anyhow, bail, ensure, Result};
use itertools::Itertools;
use std::{
    env,
    fs::{read_to_string, write},
    io::Write,
    ops::Range,
    process::{exit, Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
    },
    thread,
};

mod backup;
use backup::Backup;

mod offset_based_rewriter;

mod offset_calculator;

mod rewriter;
use rewriter::Rewriter;

mod span;
use span::{LineColumn, Span};

#[derive(Clone, Default)]
struct Options {
    max_width: Option<usize>,
    paths: Vec<String>,
    args: Vec<String>,
}

#[derive(Debug)]
struct Chunk {
    lines: Range<usize>,
    characteristics: Characteristics,
    docs: String,
}

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

static CTRLC: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    ctrlc::set_handler(|| CTRLC.store(true, Ordering::SeqCst))?;
    let mut opts = process_args()?;
    let paths = opts.paths.split_off(0);
    let backups = paths
        .iter()
        .map(|path| Backup::new(path).map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;
    let mut handles = Vec::new();
    for path in paths {
        let opts = opts.clone();
        handles.push(thread::spawn(|| format_file(opts, path)));
    }
    for handle in handles {
        join(handle)?;
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
            opts.paths.push(arg);
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

```
    ---max-width <N>
```

is converted to options of the form:

```
    --prose-wrap always --print-width <M>
```

where `M` is `N` minus the width of the indentation, of the
`//!` or `///` syntax, and of the space that might follow that
syntax. See: https://prettier.io/docs/en/options.html";

fn help() -> ! {
    println!("{HELP}");
    exit(0);
}

fn format_file(opts: Options, path: String) -> Result<()> {
    let contents = read_to_string(&path)?;

    let chunks = chunk(&contents);
    let characteristics = chunks
        .iter()
        .map(|chunk| chunk.characteristics)
        .collect::<Vec<_>>();

    let (sender, receiver) = sync_channel::<Child>(0);
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

        rewriter.rewrite(span, &docs);
    }

    let contents = rewriter.contents();

    write(path, contents)?;

    join(handle)?;

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
fn prettier_spawner(
    opts: &Options,
    characteristics: Vec<Characteristics>,
    sender: &SyncSender<Child>,
) -> Result<()> {
    let children = characteristics
        .into_iter()
        .map(|characteristics| {
            let mut command = Command::new("prettier");
            command.arg("--parser=markdown");
            if let Some(max_width) = opts.max_width {
                command.arg("--prose-wrap=always");
                command.arg(&format!(
                    "--print-width={}",
                    max_width.saturating_sub(characteristics.indent + 4)
                ));
            }
            command.args(&opts.args);
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command.spawn().expect("failed to spawn `prettier`")
        })
        .collect::<Vec<_>>();
    for child in children {
        sender.send(child)?;
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
        "prettier exited abnormally: {output:#?}"
    );

    let docs = String::from_utf8(output.stdout)?;

    Ok(postprocess_docs(chunk.characteristics, &docs))
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

fn join<T>(handle: thread::JoinHandle<Result<T>>) -> Result<T> {
    handle
        .join()
        .map_err(|error| anyhow!("{error:?}"))
        .and_then(std::convert::identity)
}
