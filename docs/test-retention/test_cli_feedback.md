# Retention: `tests/test_cli_feedback.py`

**Category:** Python CLI surface

## What it tests

CLI wiring for the ``gmeow-dev feedback`` diagnostics-output knobs (#662).

## Why it cannot move to Rust today

Drives the Python `gmeow`/`gmeow-dev` Typer CLI via `CliRunner`/subprocess — behavior that does not exist outside Python.

## What is needed to move it to Rust

Port the command surface to a Rust binary (`clap`) with `assert_cmd`/`trycmd` integration tests, then delete this file.
