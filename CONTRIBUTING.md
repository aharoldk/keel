# Contributing to Keel

Thanks for your interest in contributing. This document covers how to set up a
development environment and how we review changes.

## Development setup

Requires Rust 1.77+, Node 20+, and the [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
npm run install:all
npm run dev
```

Then open `examples/demo-workspace`.

## Tests

```bash
npm test
```

That runs the UI tests and `cargo test --workspace --features keel/cli`.

## Pull requests

- Open pull requests against `main`.
- Keep changes focused. One concern per pull request is easier to review.
- Match the style of the surrounding code. Do not add comments unless the
  surrounding code needs them to stay understandable.
- Run `npm test` before opening the pull request.

## Reporting issues

Use the issue templates. Include the Keel version, your OS, and steps to
reproduce. For crashes, attach the relevant log output.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). By
participating, you agree to abide by it.
