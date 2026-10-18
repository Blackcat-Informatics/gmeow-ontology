# Referenced artifacts — issue #1762

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `crates/gmeow-cli/src/commands/prove.rs` | yes | repo | file | 1685 | SPDX: AGPL-3.0-only |
| `crates/gmeow-cli/src/commands/reasoning_report.rs` | yes | repo | file | 783 | SPDX: AGPL-3.0-only |
| `crates/logic/src/external_evidence.rs` | yes | repo | file | 2292 | SPDX: AGPL-3.0-only |
| `crates/logic/src/physical/numeric.rs` | yes | repo | file | 400 | SPDX: AGPL-3.0-only |
| `crates/logic/src/physical/plan.rs` | yes | repo | file | 1299 | SPDX: AGPL-3.0-only |
| `/home/paudley/stage/root/bin/ghprsq` | yes | absolute | file | 776 | — |

### History: `crates/gmeow-cli/src/commands/prove.rs`

- `1c3bc726a fix(logic): preserve native source diagnostics through proof preparation`
- `a9892dc72 feat(logic): catalogue reasoning contracts in classical proof tasks`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `05bab4429 fix: compile gmeow-cli and gmeow-lsp against the purrdf 3 read API (#1786)`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/gmeow-cli/src/commands/reasoning_report.rs`

- `41009bed6 fix(diagnostics): preserve failure classes in prover report projections`
- `f033b9f3a docs(logic): clarify diagnostic contracts and native terminology`
- `c3123db1a docs(helpers): explain source and diagnostic regression contracts`
- `c74f5e804 fix(helpers): share RDF emission and diagnostic registration`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/logic/src/external_evidence.rs`

- `41009bed6 fix(diagnostics): preserve failure classes in prover report projections`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/logic/src/physical/numeric.rs`

- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/logic/src/physical/plan.rs`

- `a0b0deb02 Execute native node-kind integrity checks`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `80a142415 refactor(1346): relocate the structured-term arena into crates/term-arena`
- `c3d5f1e9c feat(1609): reasoner materializes math:DimensionalInhomogeneity from the logic:Formula ASTs`
- `6744be6f4 feat(1601): RDF 1.2 quoted-triple goal-argument grammar for triple-term PURREMB retrieval`
- `4d40eae01 feat(1428): ledger builtin domain gaps with per-kind identity through the reasoning spine`
- `1b130738b feat(1428): native exact-ℚ bilinear squared-distance builtin across all three engines`
- `7914b385c fix(logic): content-address goal-directed answer IRIs; lock latent engine invariants`
- `41b266c90 feat(logic): harden structured-term engine, add compound function terms (#1295)`

## Identifiers (files containing each at HEAD)

- `gmeow_errors::Diag`: 461
- `gmeow_errors::Result`: 452
- `Diag`: 726
- `ArtifactProtocolError`: 3
- `EvalError`: 4
- `ce1c916a7d7820dddc9c0f20648ea8cd9ac76c49`: 0
- `main`: 2059
- `ce1c916a7`: 0

## Workspace packages named

- `gmeow-build-inputs`
- `gmeow-cli`
- `gmeow-logic`
- `helper-census`

## URLs

None.
