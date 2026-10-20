# Referenced artifacts — issue #1652

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `slices/core/slice-quality-rubric/module.ttl:569` | yes | repo | file | 3922 | SPDX: CC-BY-4.0 |
| `crates/gmeow-dev-cli/src/dev_slice_quality.rs:1588` | yes | repo | file | 2752 | SPDX: AGPL-3.0-only |
| `crates/gmeow-cli/src/commands.rs:4967` | yes | repo | file | 8678 | SPDX: AGPL-3.0-only |
| `.coderabbit.yaml` | NO | repo | — | — | — |
| `slices/core/slice-quality-rubric/module.ttl` | yes | repo | file | 3922 | SPDX: CC-BY-4.0 |
| `crates/gmeow-dev-cli/src/dev_slice_quality.rs` | yes | repo | file | 2752 | SPDX: AGPL-3.0-only |
| `crates/gmeow-cli/src/commands.rs` | yes | repo | file | 8678 | SPDX: AGPL-3.0-only |

### History: `slices/core/slice-quality-rubric/module.ttl:569`

- `cbbe21256 fix: terminate the quality applicability range declaration`
- `08eae0721 fix: preserve language and direction on word properties`
- `c19400ad9 feat: add explicit external corpus quality profiles`
- `89bbc5f9f feat(logic): derive the callable catalog from canonical signatures`
- `18e18ec7a feat(743): harvest axis honesty — fail closed, excuse deliberate non-assertions, docs+skill`
- `d702024b5 feat(slice-quality): definitional-harvest coverage axis (prose → executable logic:)`
- `be45dd27f fix(1682): declare the used rdf: prefix in slices that omitted it`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `9c62939dc i18n(1376): author the fr + cmn renderings the three below-floor slices need`
- `50764e97c fix(1443): author the bridge's justification, re-pin the codebook, drop an absorbed relocation`

### History: `crates/gmeow-dev-cli/src/dev_slice_quality.rs:1588`

- `a159730f8 perf(logic): integrate the certified native pipeline`
- `8e5fa7ae0 fix(provenance): harden citation and Wikidata validation`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `7cb205556 fix(medium): derive the model-facing delta from the ontology, not a literal`
- `c5b122ff3 fix: make each read record prove itself, and pin the residue carve-out`
- `ee96aeeab fix: stop the gate littering /tmp, and gate the pattern that caused it`
- `1055d73c6 perf(gate): stop recomputing what sync already produced`
- `14afc2558 fix(slice-quality): compute transport in u64, and keep one axis-attribution mechanism`
- `53f5470a4 fix(docs,dev-cli,slice-quality,xtask): close the open review findings`
- `183f06964 fix(slice-quality): make the transport solver an actual max flow, and give the preview the same one`

### History: `crates/gmeow-cli/src/commands.rs:4967`

- `f70b889c4 feat: submit source packet theories through the governed candidate tool`
- `f21d94925 fix: qualify external corpus quality profiles`
- `c19400ad9 feat: add explicit external corpus quality profiles`
- `10c6e175f refactor(logic): retain native DL certificates and proofs`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `7cfc7049f feat(dev): helper census: one home per job, PurRDF's where it exists (#1785)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `8e5fa7ae0 fix(provenance): harden citation and Wikidata validation`
- `2357a77a1 feat(1666): cut goal-directed reasoning onto PurRDF order-sorted datalog`
- `6d90e838b fix(1666): address review findings and re-bless the drifted engine-identity goldens`

### History: `slices/core/slice-quality-rubric/module.ttl`

- `cbbe21256 fix: terminate the quality applicability range declaration`
- `08eae0721 fix: preserve language and direction on word properties`
- `c19400ad9 feat: add explicit external corpus quality profiles`
- `89bbc5f9f feat(logic): derive the callable catalog from canonical signatures`
- `18e18ec7a feat(743): harvest axis honesty — fail closed, excuse deliberate non-assertions, docs+skill`
- `d702024b5 feat(slice-quality): definitional-harvest coverage axis (prose → executable logic:)`
- `be45dd27f fix(1682): declare the used rdf: prefix in slices that omitted it`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `9c62939dc i18n(1376): author the fr + cmn renderings the three below-floor slices need`
- `50764e97c fix(1443): author the bridge's justification, re-pin the codebook, drop an absorbed relocation`

### History: `crates/gmeow-dev-cli/src/dev_slice_quality.rs`

- `a159730f8 perf(logic): integrate the certified native pipeline`
- `8e5fa7ae0 fix(provenance): harden citation and Wikidata validation`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `7cb205556 fix(medium): derive the model-facing delta from the ontology, not a literal`
- `c5b122ff3 fix: make each read record prove itself, and pin the residue carve-out`
- `ee96aeeab fix: stop the gate littering /tmp, and gate the pattern that caused it`
- `1055d73c6 perf(gate): stop recomputing what sync already produced`
- `14afc2558 fix(slice-quality): compute transport in u64, and keep one axis-attribution mechanism`
- `53f5470a4 fix(docs,dev-cli,slice-quality,xtask): close the open review findings`
- `183f06964 fix(slice-quality): make the transport solver an actual max flow, and give the preview the same one`

### History: `crates/gmeow-cli/src/commands.rs`

- `f70b889c4 feat: submit source packet theories through the governed candidate tool`
- `f21d94925 fix: qualify external corpus quality profiles`
- `c19400ad9 feat: add explicit external corpus quality profiles`
- `10c6e175f refactor(logic): retain native DL certificates and proofs`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `7cfc7049f feat(dev): helper census: one home per job, PurRDF's where it exists (#1785)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `8e5fa7ae0 fix(provenance): harden citation and Wikidata validation`
- `2357a77a1 feat(1666): cut goal-directed reasoning onto PurRDF order-sorted datalog`
- `6d90e838b fix(1666): address review findings and re-bless the drifted engine-identity goldens`

## Identifiers (files containing each at HEAD)

- `main`: 2093
- `countKindStructuralAxiom`: 6
- `range`: 1073

## Workspace packages named

- `gmeow-cli`
- `gmeow-dev-cli`
- `gmeow-ontology`

## URLs

- https://discord.gg/coderabbit
