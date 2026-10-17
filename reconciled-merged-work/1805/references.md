# Referenced artifacts — issue #1805

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `crates/logic-compile/src/projections/rdf.rs` | yes | repo | file | 1887 | SPDX: AGPL-3.0-only |
| `graph/logic` | NO | repo | — | — | — |
| `logic:rule/_000001/body/0000` | NO | repo | — | — | — |
| `frontend.rs` | NO | repo | — | — | — |

### History: `crates/logic-compile/src/projections/rdf.rs`

- `a159730f8 perf(logic): integrate the certified native pipeline`
- `bf18f2e7c style: rustfmt the purrdf 1.1.0 call-site migrations`
- `06f896c18 fix(logic-compile): carry the serialize diagnostic as a Diag, not a String`
- `984928540 fix(logic-compile): surface the IRI refusal instead of panicking blind`
- `8e5fa7ae0 fix(provenance): harden citation and Wikidata validation`
- `6aeb64e5b fix(1682): complete the owl: retirement across readers, full-IRI constructs, characteristic projection, and translations`
- `9f766d825 feat(1682): teach the frontend + shape derivation the canonical logic: typing vocabulary`
- `4441dac56 fix(1443): name the failure class on the finding, collapse the duplicates, and let the reasoner read the canonical restriction`
- `3c0935c32 refactor(1200): share one TripleSink::add_annotation across the grounding views`
- `64047b962 feat(1200): project lifted annotations back out with carrier tag (ExactPreservation round-trip)`

## Identifiers (files containing each at HEAD)

- `gmeow.gts`: 456

## Workspace packages named

None.

## URLs

None.
