# Explicit local types

`explicit_local_types` is a deny-by-default Dylint lint for ordinary `let`
bindings, including destructuring, delayed initialization, and `let ... else`.
It requires complete annotations for nameable, non-primitive types:

```rust
let mut start = 0;
let enabled = true;
let name = "foo";
let user: User = load_user()?;
let client: Arc<HttpClient> = build_client();
let handlers: Vec<Box<dyn Handler>> = make_handlers();
let state: Option<AppState> = get_state();
```

## Policy

- Inference is allowed for booleans, characters, integers, floats, `&str`
  (including mutable string slices), unit, and the never type.
- Strings, collections, arrays, tuples, slices, references other than `&str`,
  pointers, and user-defined types require annotations, even with constructors.
- `_` and nested type placeholders such as `Vec<_>` do not count as complete
  annotations. Elided and placeholder lifetimes remain allowed.
- Types containing closures, function items, coroutines, or opaque `impl Trait`
  are exempt because their exact types cannot be written in a local annotation.
  This includes iterator adapters containing closures and async blocks.
- Discarded values (`let _ = ...`) and macro-generated bindings are exempt.
- `if let`, `while let`, `for`, match arms, and parameters are outside this rule.
  Their binding patterns do not support ordinary `let` type annotations.
- Standard `#[allow(explicit_local_types)]` and `#[expect(explicit_local_types)]`
  attributes can document intentional exceptions.

Suggestions show inferred types but are not machine-applicable: compiler-rendered
paths may need imports or qualification changes.

## Setup and validation

From the repository root, install the tools once:

```sh
cargo install cargo-dylint dylint-link --version 6.0.4 --locked
```

Run the repository lint and its regression tests:

```sh
cargo dylint --all -- --locked --all-targets
(cd lints/explicit-local-types && cargo test --locked)
```

The lint uses its own workspace, lockfile, and pinned nightly toolchain. Rustup
installs that toolchain and its compiler components when needed. Normal project
builds and Clippy continue to use stable Rust. Dylint is a separate command;
`cargo clippy` does not load this rule. CI runs both.

The UI tests check accepted code and compare rejected-code diagnostics against
`ui/fail.stderr`. Update that snapshot only after reviewing intentional changes.
Run formatting inside this directory as well as at the repository root.
