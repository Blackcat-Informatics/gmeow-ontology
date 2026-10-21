# Referenced artifacts — issue #1717

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `slices/math` | NO | repo | — | — | — |
| `slices/core/observations/tests/structural.ttl:136` | yes | repo | file | 463 | SPDX: CC-BY-4.0 |
| `slices/core/quality/module.ttl:121` | yes | repo | file | 126 | — |
| `examples/` | NO | not here; exists in sibling(s): biocode-2026-09-30, cf-config, llama-cpp-rs-logit-loom, purrassist, purrdf (+1 more) | — | — | — |
| `slices/core/observations/tests/structural.ttl` | yes | repo | file | 463 | SPDX: CC-BY-4.0 |
| `slices/grounding/math/module.ttl` | yes | repo | file | 16547 | SPDX: CC-BY-4.0 |
| `slices/core/quality/module.ttl` | yes | repo | file | 126 | — |

### History: `slices/core/observations/tests/structural.ttl:136`

- `b81825fd4 feat(ontology): model policy findings and reader disclosure`
- `f2955b0a0 feat: record encounters, attention requests and consultations (Principles 4, 7, 9, 11, 17)`
- `74b08e87a Add source-bound refinement outcomes and reading patterns (Principles 4, 9, 19)`
- `3894fba10 fix(1442): resolve the six non-grounding forbidden tier crossings`
- `fe5ae1f6a fix(1442): resolve 7 forbidden crossings via gate-policy + term promotion`
- `70aa6ca4e test(1579): re-point vacuous owl:FunctionalProperty competency asks to the logic: carrier`
- `7b81beed3 test(1579): reconcile structural competency assertions with the functional deprecation`
- `48aa8c6d8 fix(grounding): close final review gaps (Principles 4, 7, 17)`
- `c152ef7c5 feat(grounding): close logic math lang authority gaps (#1555)`
- `a3a66b195 purge(cleanup): retire migration/retention scaffolding, repoint gate citations, re-bless goldens`

### History: `slices/core/quality/module.ttl:121`

- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `c152ef7c5 feat(grounding): close logic math lang authority gaps (#1555)`
- `5fc0bd072 feat(1080): seed generic-quality vocab + disambiguate data quality`
- `a69a2bfb4 feat(694): per-slice owl/gUFO→logic: source retirement — first batch + logic:Disposition (#864)`
- `f7a6eba09 feat(819): gmeow-rdf as the optimal IR & dataflow stream — loss flips, native SHACL-on-IR, Python→Rust cutovers (#825)`
- `4f38e8489 Enforce graph-box role coverage across ontology surfaces (#642)`
- `6d6782fbe docs(slices): bring all core slices to Tier-1 documentation depth (#602)`
- `e080a6311 feat(slices): per-term rdfs:isDefinedBy → owning-slice IRI (closes #329, Principles 7, 8, 15, 16) (#333)`
- `b8f08fb51 feat(slices): the slice architecture — every module a slice, every generated artifact under one gated root (closes #287) (#328)`

### History: `slices/core/observations/tests/structural.ttl`

- `b81825fd4 feat(ontology): model policy findings and reader disclosure`
- `f2955b0a0 feat: record encounters, attention requests and consultations (Principles 4, 7, 9, 11, 17)`
- `74b08e87a Add source-bound refinement outcomes and reading patterns (Principles 4, 9, 19)`
- `3894fba10 fix(1442): resolve the six non-grounding forbidden tier crossings`
- `fe5ae1f6a fix(1442): resolve 7 forbidden crossings via gate-policy + term promotion`
- `70aa6ca4e test(1579): re-point vacuous owl:FunctionalProperty competency asks to the logic: carrier`
- `7b81beed3 test(1579): reconcile structural competency assertions with the functional deprecation`
- `48aa8c6d8 fix(grounding): close final review gaps (Principles 4, 7, 17)`
- `c152ef7c5 feat(grounding): close logic math lang authority gaps (#1555)`
- `a3a66b195 purge(cleanup): retire migration/retention scaffolding, repoint gate citations, re-bless goldens`

### History: `slices/grounding/math/module.ttl`

- `da59fdeaa fix: retain projection output modes and admit expression requests`
- `5b21f45d2 fix: preserve canonical math numbers and projection boundaries`
- `3e2a0e559 Implement mathematical target codecs and preservation ledgers`
- `f8824d0e9 fix: reject contradictory exact math projection judgments`
- `0e09c13e7 docs: distinguish finite numeric and literal value branches`
- `77bd7a3da fix: enforce finite numeric radial-field and measure values`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a794c70b3 fix(slices): qualify datatype-property counts by a data range, not owl:Thing`
- `b10a1de8a feat(1682): complete owl:->logic: slice-authoring migration closeout`
- `566b1302e docs(1682): correct stale OWL wording in slice constraint headers`

### History: `slices/core/quality/module.ttl`

- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `c152ef7c5 feat(grounding): close logic math lang authority gaps (#1555)`
- `5fc0bd072 feat(1080): seed generic-quality vocab + disambiguate data quality`
- `a69a2bfb4 feat(694): per-slice owl/gUFO→logic: source retirement — first batch + logic:Disposition (#864)`
- `f7a6eba09 feat(819): gmeow-rdf as the optimal IR & dataflow stream — loss flips, native SHACL-on-IR, Python→Rust cutovers (#825)`
- `4f38e8489 Enforce graph-box role coverage across ontology surfaces (#642)`
- `6d6782fbe docs(slices): bring all core slices to Tier-1 documentation depth (#602)`
- `e080a6311 feat(slices): per-term rdfs:isDefinedBy → owning-slice IRI (closes #329, Principles 7, 8, 15, 16) (#333)`
- `b8f08fb51 feat(slices): the slice architecture — every module a slice, every generated artifact under one gated root (closes #287) (#328)`

## Identifiers (files containing each at HEAD)

- `main`: 2101
- `module.ttl`: 559
- `Prior`: 43
- `Posterior`: 20
- `TensorComputationGraph`: 29
- `affect`: 237
- `observations`: 492
- `diagnostics`: 479
- `quality`: 895
- `observationResult`: 154
- `AbstractIndividualType`: 172
- `QualityAssessment`: 35
- `QualityDimension`: 18

## Workspace packages named

None.

## URLs

None.
