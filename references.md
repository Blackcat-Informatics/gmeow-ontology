# Referenced artifacts — issue #1803

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `logic/src/reason/refute/casesplit.rs` | NO | repo | — | — | — |
| `logic/src/physical/chase.rs` | NO | repo | — | — | — |
| `reason/value.rs` | NO | repo | — | — | — |
| `property/datatype/extent.rs` | NO | repo | — | — | — |
| `Cargo.lock` | yes | repo | file | 6388 | — |
| `graph/imports` | NO | repo | — | — | — |
| `graph/logic` | NO | repo | — | — | — |
| `graph/rl_default` | NO | repo | — | — | — |
| `docs/RUST-OPTIMIZATION.md` | yes | repo | file | 327 | SPDX: CC-BY-4.0 |
| `.stage/split-gmeow-purrdf-responsibilities/plan.md` | NO | repo | — | — | — |
| `crates/logic/src/reasoner_services.rs` | yes | repo | file | 300 | SPDX: AGPL-3.0-only |
| `crates/logic/src/reasoner_services.tests.rs` | yes | repo | file | 410 | SPDX: AGPL-3.0-only |
| `crates/reason-wasm/tests/WITNESS.conjecture.nq` | yes | repo | file | 127 | — |
| `crates/reason-wasm/tests/WITNESS.reason.nq` | yes | repo | file | 7 | — |
| `.../conformance_music_performance.rs` | NO | repo | — | — | — |
| `fuzz/Cargo.lock` | yes | repo | file | 1552 | — |
| `fuzz/Cargo.toml` | yes | repo | file | 63 | SPDX: AGPL-3.0-only |
| `slices/extensions/music/docs.md` | yes | repo | file | 386 | — |
| `slices/extensions/music/module.ttl` | yes | repo | file | 9388 | — |
| `.../src/projections/shapes.procedural_tests.rs` | NO | repo | — | — | — |
| `.../conformance_cases/conformance_profiles.rs` | NO | repo | — | — | — |
| `slices/core/documentation/docs.md` | yes | repo | file | 69 | SPDX: CC-BY-4.0 |
| `slices/core/documentation/module.ttl` | yes | repo | file | 1383 | SPDX: CC-BY-4.0 |
| `slices/core/documentation/shapes.ttl` | yes | repo | file | 68 | SPDX: CC-BY-4.0 |
| `slices/core/documentation/tests/structural.ttl` | yes | repo | file | 285 | SPDX: CC-BY-4.0 |
| `slices/core/profiles/docs.md` | yes | repo | file | 87 | SPDX: CC-BY-4.0 |
| `slices/core/profiles/module.ttl` | yes | repo | file | 223 | — |
| `slices/core/profiles/tests/structural.ttl` | yes | repo | file | 164 | SPDX: CC-BY-4.0 |
| `slices/grounding/lang/module.ttl` | yes | repo | file | 12764 | SPDX: CC-BY-4.0 |
| `slices/grounding/lang/tests/structural.ttl` | yes | repo | file | 441 | SPDX: CC-BY-4.0 |
| `slices/grounding/logic/module.ttl` | yes | repo | file | 23966 | — |
| `slices/grounding/logic/tests/structural.ttl` | yes | repo | file | 1968 | SPDX: CC-BY-4.0 |
| `/home/paudley/Active/gmeow-ontology/.worktrees/1803-split-gmeow-purrdf-responsibilities/.stage/split-gmeow-purrdf-responsibilities` | yes | absolute | dir | — | — |
| `/home/paudley/Active/gmeow-ontology/.worktrees/1803-split-gmeow-purrdf-responsibilities/.stage/split-gmeow-purrdf-responsibilities.` | yes | absolute | dir | — | — |

### History: `Cargo.lock`

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `bd2e35b8c fix(logic): preserve native datatype lexical admission`
- `a8ffd250e fix(logic): pin sound native length-space exhaustion`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `7db11af84 fix(pipeline): select recorded worlds after complete object assembly`
- `645a9db7a chore(deps): bump rustls (#1824)`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `1d5d7f36d chore(logic): remove unused corpus loader dependencies`
- `a631f73e6 perf(1757): make native reasoning fit and finish on the production corpus (#1791)`

### History: `docs/RUST-OPTIMIZATION.md`

- `3bcbc1007 docs: clarify partial validation telemetry (Principles 4, 7)`
- `1510a48a3 perf: reuse authenticated validation state (Principles 4, 7, 18)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `e049a22a2 fix: preserve optimized producer authority across checkouts`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`
- `108d350d2 docs(1668): rename gmeow-rdf references to purrdf in documentation`
- `a76168cc6 build(gate): one producer, one run — remove make regen as an entry point`
- `6bb22778d build(gate): schedule check on accurate dependencies, and move breadth lanes to CI`
- `ab0ed267e fix(1211): strip tracker references from published-crate metadata, and hoist shared dependencies to the workspace`
- `92e42bea0 docs(1402): sweep all `make sync` references to `make regen`/`make check-sync``

### History: `crates/logic/src/reasoner_services.rs`

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `816a604cd fix(logic): prepare exact questions for native proof checking`
- `10c6e175f refactor(logic): retain native DL certificates and proofs`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `3e65bdc23 fix(reason): resolve gmeow-logic and pipeline rustdoc intra-doc links`
- `a8a253643 feat(1666): pin and prove budget-ceiling honesty on the purrdf reasoning surfaces`
- `cebabb4e7 feat(1666): adopt purrdf DL reasoning services with honest certificates + CLI`

### History: `crates/logic/src/reasoner_services.tests.rs`

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `816a604cd fix(logic): prepare exact questions for native proof checking`
- `10c6e175f refactor(logic): retain native DL certificates and proofs`
- `f033b9f3a docs(logic): clarify diagnostic contracts and native terminology`
- `12380e2df fix(logic): remove facet string errors and ground service refusals`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/reason-wasm/tests/WITNESS.conjecture.nq`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `bbe250ab8 fix(logic): delegate rational lexical admission to the native parser`
- `bd2e35b8c fix(logic): preserve native datatype lexical admission`
- `a8ffd250e fix(logic): pin sound native length-space exhaustion`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `673132cd5 test(reason): bind parity attestations to the integrated compiler source`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `b133d9aa0 chore(reason): re-bless the engine descriptor and conjecture witness`

### History: `crates/reason-wasm/tests/WITNESS.reason.nq`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `bbe250ab8 fix(logic): delegate rational lexical admission to the native parser`
- `bd2e35b8c fix(logic): preserve native datatype lexical admission`
- `a8ffd250e fix(logic): pin sound native length-space exhaustion`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `673132cd5 test(reason): bind parity attestations to the integrated compiler source`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `c299578f5 feat(1211): close R2's remaining verbs and retire three vendored engines`

### History: `fuzz/Cargo.lock`

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `bd2e35b8c fix(logic): preserve native datatype lexical admission`
- `a8ffd250e fix(logic): pin sound native length-space exhaustion`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `271463204 fix(deps): align fuzz workspace with the native graph projection pin`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `1d5d7f36d chore(logic): remove unused corpus loader dependencies`
- `7cfc7049f feat(dev): helper census: one home per job, PurRDF's where it exists (#1785)`
- `41ec956a4 deps: adopt purrdf 3.0 and enforce a major-3 substrate floor (#1784)`

### History: `fuzz/Cargo.toml`

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `bd2e35b8c fix(logic): preserve native datatype lexical admission`
- `a8ffd250e fix(logic): pin sound native length-space exhaustion`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `271463204 fix(deps): align fuzz workspace with the native graph projection pin`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `41ec956a4 deps: adopt purrdf 3.0 and enforce a major-3 substrate floor (#1784)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `4999a1f48 fix purrdf pin`
- `f6a422059 build: bump GMEOW to 0.3.0 with PurRDF 2.0.0`

### History: `slices/extensions/music/docs.md`

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `41a933fee feat(769): factor claim modality into six orthogonal axes + lossless projection (ME3 of #766) (#1070)`
- `0d7988874 feat: notation projection layer (fixes #318) (#563)`
- `275dbb71f feat(music): timbre & sensory bridge (closes #317) (#546)`
- `46d59c140 docs: describe GMEOW and remove ticket references (#538)`
- `ec09a3d6a feat(music): genre and analysis standpoint layer (#524)`
- `b96e98e0f feat(music): instruments, configurations, and playing-technique vocabulary (#314) (#493)`
- `79c510e60 feat(music): performance events & participation (#313) (#487)`
- `ebfd70280 feat(music): add M6 performance layer for form, process and indeterminacy (#312) (#467)`
- `3e0adfb26 feat(music): add structure graph layer (MusicalSegment, ToneEvent, Voice, SegmentTransformation) (#462)`

### History: `slices/extensions/music/module.ttl`

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `b10a1de8a feat(1682): complete owl:->logic: slice-authoring migration closeout`
- `be45dd27f fix(1682): declare the used rdf: prefix in slices that omitted it`
- `6aeb64e5b fix(1682): complete the owl: retirement across readers, full-IRI constructs, characteristic projection, and translations`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `ee79a0c4d fix: give the 67 slice module.ttl files their trailing newline (#1658)`
- `f6538dc2e fix(1442): state the boundary in the carrier and versions definitions`
- `dca7f51d7 fix(1442): document the promoted terms and scope-note the carriers`
- `192f46bfd fix(1442): document the relocated carriers so their new slices hold their floors`
- `0727cf359 fix(1442): re-home logic's central records to the slices that own their terms`

### History: `slices/core/documentation/docs.md`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `04997b76f feat(documentation): add core documentation slice owning the doc* projection vocabulary`

### History: `slices/core/documentation/module.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `e7be15f66 fix(docs): retain undecided native reasoning as observations`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `14e49b9d1 feat(1628): logic:-ground the DocumentedDocument taxonomy edge (slice-quality-gate)`
- `80eff19e3 feat(1628): render canonical slice Markdown into the site (graft + child pages + search/LLM)`
- `7af443fbb feat(1628): first-class DocMarkdownDocument model + SourceToPageMap link authority`
- `51301d337 fix(1551): author all migration backing in logic: namespace (OWL banned in slices)`
- `fc78fa8ce feat(1551): retire 87 grounded shapes.ttl blocks; back+self-annotate 39 kept residues`
- `106c44c51 feat(1551): ground ~36 more shapes; fix oracle logic:formalizes provenance mis-read`
- `01d81c238 feat(1551): logic:-author declarative shape backing across core/lang/math/extensions slices (WIP)`

### History: `slices/core/documentation/shapes.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `fc78fa8ce feat(1551): retire 87 grounded shapes.ttl blocks; back+self-annotate 39 kept residues`
- `04997b76f feat(documentation): add core documentation slice owning the doc* projection vocabulary`

### History: `slices/core/documentation/tests/structural.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `8a4f5ce9c test(docs,slice-quality): harden coverage/rubric assertions per reviewer findings`
- `899855eff feat(documentation): project coverage dimensions and earned DocMaturity into graph/documentation`
- `6f556e35d feat(documentation): add DocMaturity FCA lattice and coverage-dimension vocabulary`
- `04997b76f feat(documentation): add core documentation slice owning the doc* projection vocabulary`

### History: `slices/core/profiles/docs.md`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `46d59c140 docs: describe GMEOW and remove ticket references (#538)`
- `af73a8135 feat(docs): docs ship WITH the ontology — three tiers, gmeow describe, GTS-bundled guides, 56/56 full (#325) (#392)`
- `b8f08fb51 feat(slices): the slice architecture — every module a slice, every generated artifact under one gated root (closes #287) (#328)`

### History: `slices/core/profiles/module.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `6aeb64e5b fix(1682): complete the owl: retirement across readers, full-IRI constructs, characteristic projection, and translations`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `ca4887953 refactor(1618): lower subclass type-checks via a/rdfs:subClassOf* SPARQL path; remove materialize_subclass_type_closure pre-pass`
- `3b6c36792 fix(slices): ground the drain's OWL/RDFS residue in logic: + flagship stereotypes`
- `520c58a49 feat(shapes): wave A drain — ground 36 oracle-proven blocks into owning slices`
- `48a0df5fb feat(pipeline): promote the frame and open-value guards to hard fatal (#1517)`
- `c7e5e1654 feat(logic): project the remaining domain procedural constraints (suppression/inference/music/lang/finance/…)`
- `c3edeac7c feat(694): finish logic stereotype source retirement tail (#981)`
- `f7a6eba09 feat(819): gmeow-rdf as the optimal IR & dataflow stream — loss flips, native SHACL-on-IR, Python→Rust cutovers (#825)`

### History: `slices/core/profiles/tests/structural.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `a3a66b195 purge(cleanup): retire migration/retention scaffolding, repoint gate citations, re-bless goldens`
- `ddca2a570 refactor(slices): make saRationale state the reason, not the test`
- `6bc1646c0 test(validate): retire the run_shacl / _graph_nt black-box SHACL test seam`
- `41a933fee feat(769): factor claim modality into six orthogonal axes + lossless projection (ME3 of #766) (#1070)`
- `c3edeac7c feat(694): finish logic stereotype source retirement tail (#981)`
- `c53fd7e25 chore(867): cull pytest — migrate 6 slices' structural tests to slicetest cells (batch 4) (#925)`

### History: `slices/grounding/lang/module.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a794c70b3 fix(slices): qualify datatype-property counts by a data range, not owl:Thing`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `b10a1de8a feat(1682): complete owl:->logic: slice-authoring migration closeout`
- `566b1302e docs(1682): correct stale OWL wording in slice constraint headers`
- `6aeb64e5b fix(1682): complete the owl: retirement across readers, full-IRI constructs, characteristic projection, and translations`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `31fb9e01f feat(lang): profile stage-order uniqueness + single-value cardinality bounds (PR #1683 CR)`
- `db033e9c3 fix(1443): attribute the findings that had no class, and restore eight bounds that vanished`

### History: `slices/grounding/lang/tests/structural.ttl`

- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `d1df6d516 test(lang): analysis ?version binding, undeclared-pipeline fixture, Formula-typed integrity (PR #1683 CR)`
- `7ae671089 feat(lang): canonicalization and analyzer profile family`
- `70aa6ca4e test(1579): re-point vacuous owl:FunctionalProperty competency asks to the logic: carrier`
- `136512136 test(1537): glossary conformance twins + spec-executable agreement gate`
- `48aa8c6d8 fix(grounding): close final review gaps (Principles 4, 7, 17)`
- `c152ef7c5 feat(grounding): close logic math lang authority gaps (#1555)`
- `84e35c78a fix(gap): purge dev-process references from canonical carrier and design docs (Gap 1 of #1475 remediation)`
- `e3d610f3c feat(lang): re-declare gmnCorrNormalToGmn as a lossless SectionRetraction (Part of #1475)`
- `7c0f5e176 refactor(lang): derive gmeow:gmnRingWithin from a Denning information-flow lattice (Part of #1475)`

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

## Identifiers (files containing each at HEAD)

- `gmeow.gts`: 456
- `gmeow`: 8978
- `DlProof`: 1
- `ChaseProof`: 1
- `ServiceProof`: 1
- `StopSignal`: 4
- `entail::owl_dl`: 1
- `Reasoner::consistency`: 2
- `DlCertificate`: 3
- `casesplit`: 12
- `anyURI`: 46
- `DlReasoner::consistency`: 2
- `false`: 1359
- `incomplete`: 631
- `consistent`: 1477
- `rl_default`: 0
- `dateTimeStamp`: 5
- `unsupported`: 636
- `oneOf`: 162
- `hasValue`: 94
- `members`: 940
- `sameAs`: 176
- `differentFrom`: 77
- `eb1fd88b0283a8a53efea6374243c4e5c2ff3161`: 0
- `purrdf`: 1113
- `af8936e6f22c70dcbc3f3dc1224be887aba51cc5`: 0
- `ee262a12f`: 0

## Workspace packages named

- `gmeow-ontology`
- `gmeow-slicetest`

## URLs

- https://blackcatinformatics.ca/gmeow/graph/
- https://github.com/Blackcat-Informatics/purrdf/issues/566
- https://blackcatinformatics.ca/gmeow/RecordedConsumerFailure
