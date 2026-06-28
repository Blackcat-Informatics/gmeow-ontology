# Retention: `tests/test_bundle_selfsufficient.py`

**Category:** Python CLI surface

## What it tests

The bundle is self-sufficient: transpile runs from the wheel, no repo (#bundle).

## Why it cannot move to Rust today

Drives the Python `gmeow`/`gmeow-dev` Typer CLI via `CliRunner`/subprocess — behavior that does not exist outside Python.

## What is needed to move it to Rust

Port the command surface to a Rust binary (`clap`) with `assert_cmd`/`trycmd` integration tests, then delete this file.
