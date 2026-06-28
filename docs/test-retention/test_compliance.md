# Retention: `tests/test_compliance.py`

**Category:** Validator / governance surface

## What it tests

Compliance-report tests (#285).

## Why it cannot move to Rust today

A governance/validator surface (constitution gate, compliance render, statement+Jena oracle) whose engine is Rust but whose orchestration is Python.

## What is needed to move it to Rust

Move the orchestration into the bundled Rust validator / native path, then delete this file.
