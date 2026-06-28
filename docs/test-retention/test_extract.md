# Retention: `tests/test_extract.py`

**Category:** Python tool algorithm

## What it tests

Tests for the license-policy guard on module extraction.

## Why it cannot move to Rust today

A live Python tool algorithm; the test asserts its computed output, which no Rust crate covers yet.

## What is needed to move it to Rust

Port the tool to a Rust crate with crate tests/goldens over the same scenarios, then delete this file.
