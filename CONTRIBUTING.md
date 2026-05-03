# Contributing to Midnight X

## Clean-room rule

This project is a clean-room reimplementation of the Midnight Commander style
two-pane file manager. **Do not read, paste, or reference GNU Midnight
Commander source code** in code, issues, PRs, or design discussion. The man
page, screenshots, and observed behavior of the running tool are fine.

## Commit style

Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`).

**Do not** add AI-tool attribution to commits or committed files. No
`Co-Authored-By: Claude ...` trailers. No "Generated with Claude Code" lines.

## Code style

- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- `#![forbid(unsafe_code)]` is enforced workspace-wide.
- Library crates do not use `anyhow`; they define explicit error enums.

## Tests

- All new behavior is covered by tests.
- Snapshot tests use `insta`; review with `cargo insta review`.
- Run `cargo test --workspace` before opening a PR.
