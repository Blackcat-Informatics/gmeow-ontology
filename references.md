# Referenced artifacts — issue #1825

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `crates/docs-print/src/doc_render.rs` | yes | repo | file | 620 | SPDX: AGPL-3.0-only |
| `docs-print/src/lib.rs` | NO | repo | — | — | — |
| `crates/xtask/src/producer.rs` | yes | repo | file | 1202 | SPDX: AGPL-3.0-only |

### History: `crates/docs-print/src/doc_render.rs`

- `43a43a19d feat(1628): render slice documents into mdbook + PDF, unify the link authority`

### History: `crates/xtask/src/producer.rs`

- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `22842c8c8 fix: close optimized producer admission gaps`
- `e049a22a2 fix: preserve optimized producer authority across checkouts`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`

## Identifiers (files containing each at HEAD)

None named.

## Workspace packages named

- `docs-print`
- `xtask`

## URLs

- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/38927612921/job/117063071573
