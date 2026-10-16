# Referenced artifacts — issue #1757

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `scripts/test-corpus-producer-scanner.sh` | yes | repo | file | 416 | SPDX: AGPL-3.0-only |
| `crates/pipeline/src/scheduler.rs` | yes | repo | file | 1642 | SPDX: AGPL-3.0-only |
| `Cargo.lock` | yes | repo | file | 6367 | — |
| `generated/projections/lang/gmn-codebook.cbor` | NO | repo | — | — | — |
| `.github/workflows/ci.yml` | yes | repo | file | 1790 | — |
| `crates/action-cache/src/executable.rs` | yes | repo | file | 305 | SPDX: AGPL-3.0-only |
| `crates/action-cache/src/executable.tests.rs` | yes | repo | file | 187 | SPDX: AGPL-3.0-only |
| `crates/gmeow-dev-cli/src/dev_producer.rs` | yes | repo | file | 181 | SPDX: AGPL-3.0-only |
| `crates/gmeow-dev-cli/tests/make_gate_contract.rs` | yes | repo | file | 1844 | SPDX: AGPL-3.0-only |
| `origin/main...HEAD` | NO | repo | — | — | — |
| `crates/logic/src/reason/leave_one_out.rs` | yes | repo | file | 420 | SPDX: AGPL-3.0-only |
| `crates/logic/src/reason/mod.rs` | yes | repo | file | 1119 | SPDX: AGPL-3.0-only |
| `crates/build-inputs/src/resolution.rs` | yes | repo | file | 323 | SPDX: AGPL-3.0-only |
| `slices/grounding/logic/module.ttl` | yes | repo | file | 24093 | — |
| `fr.po/zh.po` | NO | repo | — | — | — |

### History: `scripts/test-corpus-producer-scanner.sh`

- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/pipeline/src/scheduler.rs`

- `a43f4b013 perf(pipeline): schedule stages on completion (#1738)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `620406270 perf(cache): unify content-addressed fixture products (Principles 4, 7)`
- `5454baa17 perf(test): make DAG cache reuse bounded and auditable`
- `967889855 perf(pipeline): cache bounded DAG products and reuse one closure`
- `e35d138f3 docs(pipeline): point the two doc links at targets rustdoc can resolve`
- `da8957732 perf(pipeline): bound the carrier's lifetime, and derive the pin`
- `ee96aeeab fix: stop the gate littering /tmp, and gate the pattern that caused it`
- `8887a1ce6 feat(pipeline): gate medium dictionaries on measured two-part MDL cost`

### History: `Cargo.lock`

- `41ec956a4 deps: adopt purrdf 3.0 and enforce a major-3 substrate floor (#1784)`
- `1510a48a3 perf: reuse authenticated validation state (Principles 4, 7, 18)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `f6a422059 build: bump GMEOW to 0.3.0 with PurRDF 2.0.0`
- `22842c8c8 fix: close optimized producer admission gaps`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`
- `f01e5fc7c feat(deps): repin purrdf to the 1.1.0 crates.io release (#1645)`
- `a319905e6 fix(pipeline): stabilize release history and GTS sink (#1699)`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `dd60b195b chore(1666): re-pin purrdf to merged 3aa4ba51 substrate`

### History: `.github/workflows/ci.yml`

- `38f568b04 chore(deps): bump taiki-e/install-action from 2.86.1 to 2.87.19 (#1775)`
- `31053008b ci: budget measured cold producer tails`
- `b6c4441ba fix(ci): admit transferred optimized producers`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `0bddf6ad9 fix(pipeline): isolate finalized fixture selectors from sync candidates`
- `4ef37a98e perf: persist producer fixture actions across CI runs`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`
- `723ba5f3e fix(ci): date-pin the nightly so one run cannot build with two compilers`
- `4140e2698 chore(deps): bump actions/checkout from 7.0.0 to 7.0.1`
- `c55e06206 chore(deps): bump Swatinem/rust-cache from 2.9.1 to 2.9.2`

### History: `crates/action-cache/src/executable.rs`

- `b6c4441ba fix(ci): admit transferred optimized producers`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `22842c8c8 fix: close optimized producer admission gaps`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`

### History: `crates/action-cache/src/executable.tests.rs`

- `b6c4441ba fix(ci): admit transferred optimized producers`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/gmeow-dev-cli/src/dev_producer.rs`

- `1510a48a3 perf: reuse authenticated validation state (Principles 4, 7, 18)`
- `b6c4441ba fix(ci): admit transferred optimized producers`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `22842c8c8 fix: close optimized producer admission gaps`
- `e049a22a2 fix: preserve optimized producer authority across checkouts`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`

### History: `crates/gmeow-dev-cli/tests/make_gate_contract.rs`

- `daf8552c1 fix(1768): harden one-file build acceptance`
- `4f3411112 fix(1768): ship one-file prover consumer`
- `b6c4441ba fix(ci): admit transferred optimized producers`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `0bddf6ad9 fix(pipeline): isolate finalized fixture selectors from sync candidates`
- `22842c8c8 fix: close optimized producer admission gaps`
- `4ef37a98e perf: persist producer fixture actions across CI runs`
- `e049a22a2 fix: preserve optimized producer authority across checkouts`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`
- `723ba5f3e fix(ci): date-pin the nightly so one run cannot build with two compilers`

### History: `crates/logic/src/reason/leave_one_out.rs`

- `0996b01d0 fix(reason): retain property-chain probes on native path`
- `197c1922c perf: certify incremental reasoning probes`
- `28817b370 fix(logic): preserve alternate calculus spellings`
- `8557d7e43 perf(logic): batch certified slice probes`

### History: `crates/logic/src/reason/mod.rs`

- `197c1922c perf: certify incremental reasoning probes`
- `28817b370 fix(logic): preserve alternate calculus spellings`
- `8557d7e43 perf(logic): batch certified slice probes`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `7aa92ca5a $(cat \ 	<<'EOF' fix(logic): route modal evaluation through reason_program (#1695)`
- `ec13e5721 fix(logic): preserve modal derivation joins (#1695)`
- `8e5fa7ae0 fix(provenance): harden citation and Wikidata validation`
- `09a0106c8 fix(logic): route modal evaluation through reasoner`
- `2357a77a1 feat(1666): cut goal-directed reasoning onto PurRDF order-sorted datalog`
- `c403cf754 fix(1682): lower canonical logic:domain/logic:range to the reasoner`

### History: `crates/build-inputs/src/resolution.rs`

- `8557d7e43 perf(logic): batch certified slice probes`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `slices/grounding/logic/module.ttl`

- `8557d7e43 perf(logic): batch certified slice probes`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `0e80c11a0 feat(validation): per-document term authority and executable recovery legs (#1650)`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `c403cf754 fix(1682): lower canonical logic:domain/logic:range to the reasoner`
- `bda21d6aa fix(1682): flip merge-reintroduced owl: to logic: in core slices`
- `47a735ce1 fix(1682): use full IRIs for the property-type filter in the gpc SPARQL target`
- `6aeb64e5b fix(1682): complete the owl: retirement across readers, full-IRI constructs, characteristic projection, and translations`
- `767e3a91c feat(1682): flip every slice module.ttl from owl: to canonical logic: authoring`
- `4f62c2a44 feat(1682): declare the canonical logic: grounding vocabulary and correspondence laws`

## Identifiers (files containing each at HEAD)

- `a159730f8`: 0
- `NativeReasoningSession::retract`: 0
- `b1676af1a`: 0
- `ghprsq`: 3
- `prepare_reasoning_input`: 80
- `SourceView`: 2
- `reason_all`: 71
- `tracing`: 37
- `ValueFlow::summarize`: 0
- `Domain`: 321
- `FlowSummary::publish_rectangle`: 0
- `head_domains`: 1
- `publish_head_conditions`: 1
- `perf`: 343
- `rule_index`: 4
- `visit`: 128
- `FlowRule`: 5
- `analysis_body`: 4
- `parse_cardinality`: 0
- `CardinalityEqual`: 2
- `Other`: 114
- `maximum_one_equality_requires_both_qualifiers_and_keeps_every_premise`: 1
- `FlowRule::native_witnesses`: 0
- `witness_frontier`: 19

## Workspace packages named

- `gmeow-build-inputs`
- `gmeow-dev-cli`
- `gmeow-logic`

## URLs

None.
