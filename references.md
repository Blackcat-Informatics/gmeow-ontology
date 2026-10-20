# Referenced artifacts — issue #1657

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `examples/closed-form-functions` | NO | repo | — | — | — |
| `crates/validate/tests/example_sweep.rs:44` | NO | repo | — | — | — |
| `tests/structural.ttl` | NO | repo | — | — | — |
| `.coderabbit.yaml` | NO | repo | — | — | — |
| `agent/issue-1657-stage-1` | NO | repo | — | — | — |
| `slices/grounding/math/module.ttl:14834` | yes | repo | file | 16526 | SPDX: CC-BY-4.0 |
| `crates/logic-compile/src/frontend.rs` | yes | repo | file | 6764 | SPDX: AGPL-3.0-only |
| `frontend.rs:4369-4378` | NO | repo | — | — | — |
| `frontend.rs:1748` | NO | repo | — | — | — |
| `frontend.rs:1820` | NO | repo | — | — | — |
| `crates/validate/tests/example_sweep.rs` | NO | repo | — | — | — |
| `slices/grounding/math/module.ttl` | yes | repo | file | 16526 | SPDX: CC-BY-4.0 |

### History: `slices/grounding/math/module.ttl:14834`

- `f8824d0e9 fix: reject contradictory exact math projection judgments`
- `0e09c13e7 docs: distinguish finite numeric and literal value branches`
- `77bd7a3da fix: enforce finite numeric radial-field and measure values`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a794c70b3 fix(slices): qualify datatype-property counts by a data range, not owl:Thing`
- `b10a1de8a feat(1682): complete owl:->logic: slice-authoring migration closeout`
- `566b1302e docs(1682): correct stale OWL wording in slice constraint headers`
- `6aeb64e5b fix(1682): complete the owl: retirement across readers, full-IRI constructs, characteristic projection, and translations`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `db033e9c3 fix(1443): attribute the findings that had no class, and restore eight bounds that vanished`

### History: `crates/logic-compile/src/frontend.rs`

- `fa80f5825 fix: project language datatypes and closed range unions (Principles 4, 7, 17, 18)`
- `a0b0deb02 Execute native node-kind integrity checks`
- `d89debcda fix(logic): separate configuration declarations from probabilistic execution`
- `1c3bc726a fix(logic): preserve native source diagnostics through proof preparation`
- `f033b9f3a docs(logic): clarify diagnostic contracts and native terminology`
- `12380e2df fix(logic): remove facet string errors and ground service refusals`
- `a9892dc72 feat(logic): catalogue reasoning contracts in classical proof tasks`
- `f9dbb7ae7 feat(logic): certify Boolean source-class membership laws`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `0ecbb06b8 fix(1787): carry every logic: list to graph/logic in one encoding (#1788)`

### History: `slices/grounding/math/module.ttl`

- `f8824d0e9 fix: reject contradictory exact math projection judgments`
- `0e09c13e7 docs: distinguish finite numeric and literal value branches`
- `77bd7a3da fix: enforce finite numeric radial-field and measure values`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a794c70b3 fix(slices): qualify datatype-property counts by a data range, not owl:Thing`
- `b10a1de8a feat(1682): complete owl:->logic: slice-authoring migration closeout`
- `566b1302e docs(1682): correct stale OWL wording in slice constraint headers`
- `6aeb64e5b fix(1682): complete the owl: retirement across readers, full-IRI constructs, characteristic projection, and translations`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `db033e9c3 fix(1443): attribute the findings that had no class, and restore eight bounds that vanished`

## Identifiers (files containing each at HEAD)

- `main`: 2090
- `measureResult`: 20
- `PositiveInfinity`: 47
- `example_sweep`: 3
- `NON_CONFORMANT`: 1
- `d56923da8`: 0
- `termApplication`: 23
- `termDistinct`: 30
- `termEqual`: 32
- `termGreater`: 14
- `termGreaterEqual`: 9
- `termIn`: 191
- `termIndex`: 181
- `termIri`: 157
- `termIsBlankOrIri`: 6
- `termIsIri`: 11
- `termIsLiteral`: 11
- `termLess`: 10
- `termLessEqual`: 9
- `termLiteral`: 67
- `termLiteralDatatype`: 49
- `termRegex`: 21
- `termSequenceMarker`: 20
- `termStrStarts`: 17
- `termVariable`: 142
- `ConstraintComponent::Datatype`: 17
- `withRestrictions`: 48

## Workspace packages named

- `gmeow-ontology`

## URLs

- https://discord.gg/coderabbit
