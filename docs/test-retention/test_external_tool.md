# Retention: `tests/test_external_tool.py`

**Category:** Python CLI surface

## What it tests

Tests for wrapping external gate tools as canonical findings (#662).

## Why it cannot move to Rust today

Drives the Python `gmeow`/`gmeow-dev` Typer CLI via `CliRunner`/subprocess — behavior that does not exist outside Python.

## What is needed to move it to Rust

Port the command surface to a Rust binary (`clap`) with `assert_cmd`/`trycmd` integration tests, then delete this file.
