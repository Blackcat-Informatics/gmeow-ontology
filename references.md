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

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `bbe250ab8 fix(logic): delegate rational lexical admission to the native parser`
- `bd2e35b8c fix(logic): preserve native datatype lexical admission`
- `a8ffd250e fix(logic): pin sound native length-space exhaustion`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `673132cd5 test(reason): bind parity attestations to the integrated compiler source`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `b133d9aa0 chore(reason): re-bless the engine descriptor and conjecture witness`
- `265f486d4 test(logic): reconcile reasoning-witness attestations for program-path modal routing (#1695)`

### History: `crates/reason-wasm/tests/WITNESS.reason.nq`

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
- `sameAs`: 175
- `differentFrom`: 77
- `eb1fd88b0283a8a53efea6374243c4e5c2ff3161`: 0
- `purrdf`: 1113
- `af8936e6f22c70dcbc3f3dc1224be887aba51cc5`: 0

## Workspace packages named

- `gmeow-ontology`
- `gmeow-slicetest`

## URLs

- https://blackcatinformatics.ca/gmeow/graph/
- https://github.com/Blackcat-Informatics/purrdf/issues/566
- https://blackcatinformatics.ca/gmeow/RecordedConsumerFailure
