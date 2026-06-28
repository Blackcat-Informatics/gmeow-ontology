# Retention: `tests/test_cli.py`

**Category:** Python CLI surface

## What it tests

The public `gmeow` and maintenance `gmeow-dev` command-line apps end-to-end via
Typer's `CliRunner`: argument parsing, subcommand wiring, exit codes, rendered
output, `GTS_SNAPSHOT_FILE` injection, and `--lang` selection. It also covers the
wheel-resolution path where the CLI runs without the repo (snapshot-only).

## Why it cannot move to Rust today

The CLI is implemented in Python (`gmeow_tools.cli` / `cli_dev`, a Typer app).
`CliRunner` exercises the actual Python entrypoint, option resolution, and
Rich/console rendering. There is no Rust binary to drive, so the behavior under
test does not exist outside Python. A Rust unit test cannot observe Typer's
parsing or the Python command dispatch.

## What is needed to move it to Rust

Port the CLI to a Rust binary (e.g. `clap`-based `gmeow`/`gmeow-dev`) — the
standing rust-first goal. Once the command surface is Rust, replace these with
`assert_cmd`/`trycmd` integration tests in the CLI crate and delete this file.
Until the CLI is Rust, this is the only place command behavior is verified.
