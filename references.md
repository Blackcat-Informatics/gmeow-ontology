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
| `/home/paudley/Active/gmeow-ontology/.worktrees/1803-split-gmeow-purrdf-responsibilities/.stage/split-gmeow-purrdf-responsibilities` | yes | absolute | dir | — | — |

### History: `Cargo.lock`

- `a8ffd250e fix(logic): pin sound native length-space exhaustion`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `7db11af84 fix(pipeline): select recorded worlds after complete object assembly`
- `645a9db7a chore(deps): bump rustls (#1824)`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `1d5d7f36d chore(logic): remove unused corpus loader dependencies`
- `a631f73e6 perf(1757): make native reasoning fit and finish on the production corpus (#1791)`
- `7cfc7049f feat(dev): helper census: one home per job, PurRDF's where it exists (#1785)`
- `41ec956a4 deps: adopt purrdf 3.0 and enforce a major-3 substrate floor (#1784)`

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

## URLs

- https://blackcatinformatics.ca/gmeow/graph/
- https://github.com/Blackcat-Informatics/purrdf/issues/566
- https://blackcatinformatics.ca/gmeow/RecordedConsumerFailure
