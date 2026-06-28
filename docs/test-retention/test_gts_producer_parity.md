# Retention: `tests/test_gts_producer_parity.py`

**Category:** Python tool algorithm

## What it tests

Parity gate for the native ``RDF → GTS`` producer cutover (#819 Task 8).

## Why it cannot move to Rust today

A live Python tool algorithm; the test asserts its computed output, which no Rust crate covers yet.

## What is needed to move it to Rust

Port the tool to a Rust crate with crate tests/goldens over the same scenarios, then delete this file.
