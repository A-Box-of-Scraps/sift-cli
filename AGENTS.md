# Repository Guidelines

Sift is a CLI for indexing and querying files, designed for LLMs and AI coding agents.
Its goal is to serve a similar discovery role to rg (content search) while returning more relevant results through indexing.

## Project Structure

Here is an overview of the project:

```
.cargo/config.toml
docs/
  IDEA.md       # Design decisions and proposals
  TODO.MD       # Remaining work
src/
tests/
.gitignore
.gitattributes
AGENTS.md
Cargo.toml
Cargo.lock
LICENSE
README.md
```

## Development

- Do not add comments unless they explain unexpected or complex behavior, or when documentation is explicitly requested by the user. In all cases, keep them concise.

## Validation

Validate changes with:

```sh
cargo fmt -q   # Format changes direcly instead of checking first and then fixing formatting issues.
cargo clippy -q --all-targets -- -D warnings
cargo test -q
```

## Commits & Pull Requests

Follow the Conventional Commits specification for commit messages.
Pull request summaries should include the related issue(s), a brief description of the changes, and how the changes were tested.
