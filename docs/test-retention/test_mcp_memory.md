# Retention: `tests/test_mcp_memory.py`

**Category:** Python tool algorithm

## What it tests

The MCP grounded-memory triad — `store_claim`, `recall`, `revise_belief` — and
specifically that suppression is honored on every recall path (the D2 gate). It
drives `gts.examples.agent_memory.Memory`: store/round-trip, token-overlap
recall, belief revision, and the audit view.

## Why it cannot move to Rust today

The memory implementation is live Python (`gts.examples.agent_memory.Memory`, a
Python example module in the `gts` package — not a binding). The token-overlap
recall ranking and the suppression-on-recall gate are Python algorithm behavior;
there is no Rust port whose tests could subsume these. (The MCP *read*-surface —
`lookup_term`/`llms` — IS Rust, via `crates/pipeline` `McpView` + `export.rs`,
and its server-wrapper tests were deleted; the memory triad is the part still
running on Python.)

## What is needed to move it to Rust

Port `gts.examples.agent_memory.Memory` (recall ranking + suppression gate) into
the `gts` Rust crate, cover it with crate tests over the same scenarios, and
delete this file. Until the memory engine is Rust, this is the only guard on the
D2 suppression-on-recall invariant.
