<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-test-budget

`gmeow-test-budget` is the Rust suite's post-run per-test duration gate. It
parses the JUnit XML emitted by `cargo nextest` and fails when any test in the
default/ci profile exceeds the configured wall-time budget.

## Contract

- Default budget: 25 seconds per test.
- Input: `target/nextest/ci/junit.xml` unless a path argument is supplied.
- Override: `GMEOW_TEST_BUDGET_SECS`, for local experiments only.
- Implementation: standard-library XML attribute scan, no Python and no XML
  dependency.

## Checks

```bash
make rust-test
make rust-gate
```
