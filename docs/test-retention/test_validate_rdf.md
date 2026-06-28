# Retention: `tests/test_validate_rdf.py`

**Category:** Python CLI surface

## What it tests

Acceptance tests for the repo-free ``gmeow validate <data>`` RDF path.

## Why it cannot move to Rust today

Drives the Python `gmeow`/`gmeow-dev` Typer CLI via `CliRunner`/subprocess — behavior that does not exist outside Python.

## What is needed to move it to Rust

Port the command surface to a Rust binary (`clap`) with `assert_cmd`/`trycmd` integration tests, then delete this file.
