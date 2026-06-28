# Retention: `tests/test_feedback_surfaces.py`

**Category:** Validator / governance surface

## What it tests

The `gmeow-dev feedback` surface fold loop (#654).

## Why it cannot move to Rust today

A governance/validator surface (constitution gate, compliance render, statement+Jena oracle) whose engine is Rust but whose orchestration is Python.

## What is needed to move it to Rust

Move the orchestration into the bundled Rust validator / native path, then delete this file.
