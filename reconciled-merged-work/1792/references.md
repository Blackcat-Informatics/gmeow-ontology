# Referenced artifacts — issue #1792

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `crates/logic/src/reason/refute/casesplit/execution.rs` | yes | repo | file | 528 | SPDX: AGPL-3.0-only |
| `families.rs` | NO | repo | — | — | — |

### History: `crates/logic/src/reason/refute/casesplit/execution.rs`

- `a159730f8 perf(logic): integrate the certified native pipeline`

## Identifiers (files containing each at HEAD)

- `PreparedClassAnalysis::evaluate`: 0
- `FamilyState::round`: 0
- `onProperty`: 622
- `onClass`: 158
- `prepare`: 290
- `evaluate`: 441
- `native_support`: 5
- `scan.ingest`: 2
- `prepare_schema`: 2

## Workspace packages named

- `gmeow-logic`

## URLs

None.
