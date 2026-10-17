## Problem

`make check` currently fails `crate-check` on remaining first-party `Result<_, String>` error channels. The live repository-static gate requires `gmeow_errors::Diag` as the sole first-party error substrate, but production paths still expose string errors in the prover/external-evidence and physical numeric execution surfaces.

Representative current findings include:

- `crates/gmeow-cli/src/commands/prove.rs`
- `crates/gmeow-cli/src/commands/reasoning_report.rs`
- `crates/logic/src/external_evidence.rs`
- `crates/logic/src/physical/numeric.rs`
- `crates/logic/src/physical/plan.rs`

This is part of #1336's unified diagnostic substrate and currently blocks `make check` for unrelated work.

## Required implementation

Replace each first-party string error with a typed `gmeow_errors::Diag` carrying a stable code, grade, source context, and the evidence available at the failure site. Preserve external process stderr/stdout as diagnostic payload rather than using it as the error type. Do not suppress the repo-static rule or introduce a parallel local error enum that loses the shared diagnostic fields.

## Acceptance criteria

- No production-reachable `Result<_, String>` remains in the reported surfaces.
- Prover discovery/execution failures retain command, artifact, version, exit-status, stdout/stderr and path evidence where applicable.
- External model parsing and numeric preparation failures retain their typed operation/locus and original detail.
- Existing public CLI rendering and exit behavior remains deterministic.
- Focused failure-path tests assert stable diagnostic codes and evidence.
- `make crate-check` no longer reports the string-error rule.

Principles 4, 7, 18. Parent: #1336.

