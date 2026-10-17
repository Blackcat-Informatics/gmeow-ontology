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
| `Cargo.lock` | yes | repo | file | 6387 | — |
| `graph/imports` | NO | repo | — | — | — |
| `graph/logic` | NO | repo | — | — | — |
| `graph/rl_default` | NO | repo | — | — | — |

### History: `Cargo.lock`

- `645a9db7a chore(deps): bump rustls (#1824)`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `1d5d7f36d chore(logic): remove unused corpus loader dependencies`
- `a631f73e6 perf(1757): make native reasoning fit and finish on the production corpus (#1791)`
- `7cfc7049f feat(dev): helper census: one home per job, PurRDF's where it exists (#1785)`
- `41ec956a4 deps: adopt purrdf 3.0 and enforce a major-3 substrate floor (#1784)`
- `1510a48a3 perf: reuse authenticated validation state (Principles 4, 7, 18)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `f6a422059 build: bump GMEOW to 0.3.0 with PurRDF 2.0.0`
- `22842c8c8 fix: close optimized producer admission gaps`

## Identifiers (files containing each at HEAD)

- `gmeow.gts`: 456
- `gmeow`: 8973
- `DlProof`: 1
- `ChaseProof`: 1
- `ServiceProof`: 1
- `StopSignal`: 4
- `entail::owl_dl`: 1
- `Reasoner::consistency`: 2
- `DlCertificate`: 3
- `casesplit`: 12
- `anyURI`: 47
- `DlReasoner::consistency`: 2
- `false`: 1359
- `incomplete`: 631
- `consistent`: 1477
- `rl_default`: 0
- `dateTimeStamp`: 5
- `unsupported`: 636
- `oneOf`: 162
- `hasValue`: 94
- `members`: 939
- `sameAs`: 175
- `differentFrom`: 77
- `eb1fd88b0283a8a53efea6374243c4e5c2ff3161`: 0
- `purrdf`: 1107

## Workspace packages named

None.

## URLs

None.
