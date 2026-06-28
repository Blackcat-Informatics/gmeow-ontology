# Retention: `tests/test_git_merge_driver.py`

**Category:** Python CLI surface

## What it tests

Regression tests for the generated bundle merge driver (#532).

## Why it cannot move to Rust today

Drives the Python `gmeow`/`gmeow-dev` Typer CLI via `CliRunner`/subprocess — behavior that does not exist outside Python.

## What is needed to move it to Rust

Port the command surface to a Rust binary (`clap`) with `assert_cmd`/`trycmd` integration tests, then delete this file.
