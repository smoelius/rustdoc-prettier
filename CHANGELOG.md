# Changelog

## 0.5.1

- Improve error messages; don't require `assert_cmd` to install ([#66](https://github.com/smoelius/rustdoc-prettier/pull/66))

## 0.5.0

- FEATURE: Raise an error when no matching files are found ([#64](https://github.com/smoelius/rustdoc-prettier/pull/64))
- FEATURE: Properly handle `--check` ([#61](https://github.com/smoelius/rustdoc-prettier/pull/61))

## 0.4.0

- FEATURE: Search parent directories for a rustfmt.toml file ([6cce81f](https://github.com/smoelius/rustdoc-prettier/commit/6cce81f36a307dd66e7c40427f2ce7fde0a4c2b3))
- Upgrade `rewriter` to version 0.2 ([878f571](https://github.com/smoelius/rustdoc-prettier/commit/878f571073d5075122778fda12aaced76939adf5))

## 0.3.0

- FEATURE: Spawn no more than `std::threads::available_parallelism() - 1` threads at any time ([#32](https://github.com/smoelius/rustdoc-prettier/pull/32))

## 0.2.0

- Improve help message ([055aebc](https://github.com/smoelius/rustdoc-prettier/commit/055aebccef6a09ee5ac0ef14383f592a23bf6360), [8cf6c6f](https://github.com/smoelius/rustdoc-prettier/commit/8cf6c6f26fe8a4346a5b0569ede4552ae61f89f8), and [8217aeb](https://github.com/smoelius/rustdoc-prettier/commit/8217aebcd2d230276e67e42257a6b9d345451d67))
- FEATURE: Support glob patterns ([5c33ddf](https://github.com/smoelius/rustdoc-prettier/commit/5c33ddfde42e8d807a9b553748a0b89b99df7e71))

## 0.1.1

- Improve error messages ([#3](https://github.com/smoelius/rustdoc-prettier/pull/3))
- If the current directory contains a rustfmt.toml file with a `max_width` key, use it to generate a `--max-width` option ([#4](https://github.com/smoelius/rustdoc-prettier/pull/4))

## 0.1.0

- Initial release
