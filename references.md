# Referenced artifacts — issue #1746

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `crates/logic/src/obligations.rs` | yes | repo | file | 931 | SPDX: AGPL-3.0-only |
| `agent/fol-tptp-export` | NO | repo | — | — | — |

### History: `crates/logic/src/obligations.rs`

- `720eef2b4 fix: retain graph and wire identity throughout diagnostics`
- `d2437b90f refactor(logic): retain obligation diagnostics in the shared ledger`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `41ec956a4 deps: adopt purrdf 3.0 and enforce a major-3 substrate floor (#1784)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `830c937f7 style(762): apply rustfmt + i18n trailing-newline normalization`
- `13393e5bf feat(762): first-class useWhen AdviceGuidance carrier + per-cell advice-coverage`
- `3f340a16f feat(762): bind advisory-constraint message to avoidWhen prose via verify gate`
- `2b64001ee refactor(validate): make advisory rules data-matching logic:Constraints`
- `48b8474dd feat(validate): harvest accepted recommendation candidates into advisory findings`

## Identifiers (files containing each at HEAD)

- `UNION`: 389

## Workspace packages named

None.

## URLs

None.
