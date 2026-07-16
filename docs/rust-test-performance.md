<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Rust test-suite performance

Rust test wall time is optimization evidence, not a correctness gate. Neither
`make rust-test`, `make rust-gate`, nor CI fails because an individual test
crosses an elapsed-time threshold. Wall-clock timings vary with host load,
filesystem state, cache warmth, and concurrent worktrees, so a single contended
sample is not a stable policy signal.

Nextest still applies a 60-second slow-timeout with two consecutive periods before
termination. That is only a runaway or hang backstop; it is not a performance
budget. Architecture-scoped overrides extend the backstop for known exhaustive
paths without turning their elapsed time into a target.

## Scheduling

Nextest uses the CPUs available to the process. There is no fixed global or
test-group concurrency cap. Tests that start their own full-width worker pools may
use `threads-required = "num-cpus"` to prevent nested oversubscription; this
reservation scales with the machine instead of throttling a 32+ CPU development
server to a small constant.

The default profile keeps a small architectural carve-out for exhaustive
whole-repository checks whose contracts are already covered by focused tests and
dedicated drift gates. `make maint-rust-heavy` runs that complete lane. A test
must never move off the default profile merely because one wall-clock sample was
slow.

## Optimization evidence

Use evidence that identifies actual work:

- Criterion comparisons through `make bench` and `make bench-compare`.
- Allocation counts, retained bytes, rows scanned, candidate rows, and other
  deterministic cost measures.
- Repeated before/after samples of the same production path, with cache state and
  host contention recorded.
- Profiles that locate redundant parsing, indexing, serialization, joins, or I/O.
- Semantic parity tests and byte-identical generated artifacts after optimization.

JUnit timing remains available as contextual CI output, but it does not decide
whether the suite passes. Correctness failures, snapshot drift, lints, and genuine
runaway termination remain hard failures.
