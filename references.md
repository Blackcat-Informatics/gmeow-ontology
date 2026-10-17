# Referenced artifacts — issue #1843

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `slices/grounding/logic/module.ttl` | yes | repo | file | 23966 | — |
| `slices/grounding/logic/tests/structural.ttl` | yes | repo | file | 1968 | SPDX: CC-BY-4.0 |
| `crates/logic-compile/src/frontend.rs` | yes | repo | file | 6706 | SPDX: AGPL-3.0-only |
| `relational_core.rs` | NO | repo | — | — | — |

### History: `slices/grounding/logic/module.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `84b05f060 fix(diagnostics): bind source and recorded-consumer failures to ontology classes`
- `8ba5c655d docs(logic): bind probability model requirements to query execution`
- `94b5979d9 fix(logic): preserve context and relation coordinates in atomic applications`
- `12380e2df fix(logic): remove facet string errors and ground service refusals`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `f06f59e5c fix(slices): state logic:message values as the plain strings its range declares`
- `8557d7e43 perf(logic): batch certified slice probes`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `0e80c11a0 feat(validation): per-document term authority and executable recovery legs (#1650)`

### History: `slices/grounding/logic/tests/structural.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `93f7e2ba9 fix(logic,sensory): author the merged-in taxonomy canonically instead of raising a ceiling`
- `21bce4ced docs: finish the fixture-pair burn-down; repair three structural files`
- `a85b6bdd8 fix(logic,slicetest): let the canonical subsumption edge reach the reasoner and the test surfaces`
- `bef1ec5e3 feat(logic): make the HTN refinement, the pin and the saturation witness real`
- `3db4bf421 feat(logic): give the kernel laws relational bodies, and stop them masking each other`
- `43d066ba5 test(1638): signature guards restore the kernel's test reach`
- `6012e6a31 refactor(1638): re-home orphan vocabulary and move the provenance emitter onto the logic: spine`
- `70aa6ca4e test(1579): re-point vacuous owl:FunctionalProperty competency asks to the logic: carrier`
- `7b81beed3 test(1579): reconcile structural competency assertions with the functional deprecation`

### History: `crates/logic-compile/src/frontend.rs`

- `d89debcda fix(logic): separate configuration declarations from probabilistic execution`
- `1c3bc726a fix(logic): preserve native source diagnostics through proof preparation`
- `f033b9f3a docs(logic): clarify diagnostic contracts and native terminology`
- `12380e2df fix(logic): remove facet string errors and ground service refusals`
- `a9892dc72 feat(logic): catalogue reasoning contracts in classical proof tasks`
- `f9dbb7ae7 feat(logic): certify Boolean source-class membership laws`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `0ecbb06b8 fix(1787): carry every logic: list to graph/logic in one encoding (#1788)`
- `41ec956a4 deps: adopt purrdf 3.0 and enforce a major-3 substrate floor (#1784)`
- `a159730f8 perf(logic): integrate the certified native pipeline`

## Identifiers (files containing each at HEAD)

- `RcViolationGap::UnsupportedConsequent`: 1

## Workspace packages named

None.

## URLs

- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39051285193/job/117456045300
- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39051285193/job/117456045340
