# Referenced artifacts — issue #1438

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `generated/projections/math/` | NO | repo | — | — | — |
| `.coderabbit.yaml` | NO | repo | — | — | — |
| `crates/` | yes | repo | dir | — | — |
| `crates/pipeline/src/stages/math_producers.rs` | yes | repo | file | 297 | SPDX: AGPL-3.0-only |

### History: `crates/`

- `23d45d144 fix: preserve manifest-only profile debt discovery`
- `e813ba91e feat: expose source-backed projection debt to consumers`
- `7f94b54fd test: cover anonymous requesters and named right references`
- `aa3f2cdfc fix: intersect conjunctive validation node kinds`
- `39891b157 test: use the native shape loader text interface`
- `72ce6f22f feat: define governed claim lifecycle and erasure doctrine`
- `5bbb43a03 fix: route imported template model types`
- `c290ab573 fix: execute compound native numeric integrity laws`
- `0776b650a test: verify authored finance and music inverse-cycle termination`
- `6592e22b6 test: separate authenticated verifier and native inventory coverage`

### History: `crates/pipeline/src/stages/math_producers.rs`

- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `a801fcfb4 fix(1443): make the content key content-addressed, the registry single, and the join shippable`
- `972776b08 refactor(1346): retire r_bridge_lift; the last amnesic witnesses`
- `04ff03298 feat(1443): admit the math example ABox to the bundle; reach the identity gate from the CLI`
- `208e92b78 feat(1346): fold the three lifted graphs into gmeow.gts`
- `a7a1b4f4e feat(grounding): close external mathematics coverage gaps`
- `bab4c18f5 feat(math): fold the p-value tri-slice round-trip into the bundle via a native producer`
- `cf38fb773 feat(math): fold the probability-model seam into gmeow.gts via a native producer`
- `6b55f35cd feat(pipeline): declare and runtime-verify stage graph/blob attachments; fanoutExtracts as data`

## Identifiers (files containing each at HEAD)

- `renderingConvention`: 30
- `renderingPreservation`: 31
- `main`: 2097

## Workspace packages named

- `gmeow-math`
- `gmeow-ontology`

## URLs

- https://discord.gg/coderabbit
