# rustdoc-prettier

Format `//!` and `///` comments with [`prettier`]

## Installation

```sh
cargo install rustdoc-prettier
```

`rustdoc-prettier` requires `prettier` to be installed independently, e.g.:

```sh
npm install -g prettier
```

## Usage

```
rustdoc-prettier [ARGS]
```

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

## "No such file or directory" errors

`rustdoc-prettier` tries to tolerate "No such file or directory" errors by emitting a warning and continuing. Such errors can arise when `rustdoc-prettier` tries to format a file that was removed by another process, for example.

## Notes

`rustdoc-prettier` parses source code manually. It does not use [`rustdoc-json`]. There are two reasons for this:

1. `rustdoc-json` provides the [span] of the commented code, but not of the comment itself. To the best of my knowledge, there is no easy way to extract `rustdoc` comments using `rustdoc-json`'s output.
2. `rustdoc-json` does not output [span]s for items that come [from macro expansions or inline assembly]. However, there are legitimate reasons to want to format such comments.

[`prettier`]: https://prettier.io/
[`rustdoc-json`]: https://crates.io/crates/rustdoc-json
[from macro expansions or inline assembly]: https://github.com/rust-lang/rust/blob/e190983bd3cefdec262c63dc8d47f76d965fea65/src/rustdoc-json-types/lib.rs#L65-L66
[span]: https://rust-lang.github.io/rfcs/2963-rustdoc-json.html#span
