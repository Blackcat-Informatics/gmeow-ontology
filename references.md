# Referenced artifacts — issue #1757

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `scripts/test-corpus-producer-scanner.sh` | yes | repo | file | 483 | SPDX: AGPL-3.0-only |
| `crates/pipeline/src/scheduler.rs` | yes | repo | file | 1651 | SPDX: AGPL-3.0-only |
| `Cargo.lock` | yes | repo | file | 6134 | — |
| `generated/projections/lang/gmn-codebook.cbor` | NO | repo | — | — | — |
| `slices/grounding/math/module.ttl` | yes | repo | file | 16547 | SPDX: CC-BY-4.0 |
| `diag.rs` | NO | repo | — | — | — |
| `diag.tests.rs` | NO | repo | — | — | — |
| `.github/workflows/ci.yml` | yes | repo | file | 1823 | — |
| `crates/action-cache/src/executable.rs` | yes | repo | file | 307 | SPDX: AGPL-3.0-only |
| `crates/action-cache/src/executable.tests.rs` | yes | repo | file | 188 | SPDX: AGPL-3.0-only |
| `crates/gmeow-dev-cli/src/dev_producer.rs` | yes | repo | file | 181 | SPDX: AGPL-3.0-only |
| `crates/gmeow-dev-cli/tests/make_gate_contract.rs` | yes | repo | file | 1888 | SPDX: AGPL-3.0-only |
| `origin/main...HEAD` | NO | repo | — | — | — |
| `crates/logic/src/reason/leave_one_out.rs` | yes | repo | file | 433 | SPDX: AGPL-3.0-only |
| `crates/logic/src/reason/mod.rs` | yes | repo | file | 1116 | SPDX: AGPL-3.0-only |
| `crates/build-inputs/src/resolution.rs` | yes | repo | file | 323 | SPDX: AGPL-3.0-only |
| `slices/grounding/logic/module.ttl` | yes | repo | file | 24074 | — |
| `fr.po/zh.po` | NO | repo | — | — | — |
| `crates/logic/src/physical/chase.rs` | yes | repo | file | 3918 | SPDX: AGPL-3.0-only |
| `crates/logic/src/physical/seminaive/joint.rs` | yes | repo | file | 1776 | SPDX: AGPL-3.0-only |
| `crates/logic/src/physical/seminaive/joint/input/admission.rs` | yes | repo | file | 1357 | SPDX: AGPL-3.0-only |
| `crates/logic/src/reason/value.rs` | yes | repo | file | 340 | SPDX: AGPL-3.0-only |
| `crates/logic/src/physical/effects/value_flow.rs` | yes | repo | file | 3439 | SPDX: AGPL-3.0-only |
| `crates/logic/src/physical/chase.tests.rs` | yes | repo | file | 2160 | SPDX: AGPL-3.0-only |
| `crates/logic/src/physical/seminaive/property/datatype.tests.rs` | yes | repo | file | 320 | SPDX: AGPL-3.0-only |
| `crates/logic/src/reason/artifacts.tests.rs` | yes | repo | file | 710 | SPDX: AGPL-3.0-only |
| `fuzz/Cargo.lock` | yes | repo | file | 1524 | — |
| `application/vnd.github.raw` | NO | repo | — | — | — |
| `repos/Blackcat-Informatics/gmeow-ontology/contents/slices/grounding/math/module.ttl` | NO | repo | — | — | — |
| `.stage/term-literal-typing-language-on-words/plan.md` | NO | repo | — | — | — |
| `.stage/perf-logic-finish-merged-pipeline/plan.md` | NO | repo | — | — | — |
| `/bin/bash` | yes | absolute | file | 7040 | — |

### History: `scripts/test-corpus-producer-scanner.sh`

- `0f286b1c8 fix(audit): preserve native findings through report consumers`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/pipeline/src/scheduler.rs`

- `0327fb993 feat(pipeline): consume recorded reasoning inputs without a producer`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `a43f4b013 perf(pipeline): schedule stages on completion (#1738)`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `620406270 perf(cache): unify content-addressed fixture products (Principles 4, 7)`
- `5454baa17 perf(test): make DAG cache reuse bounded and auditable`
- `967889855 perf(pipeline): cache bounded DAG products and reuse one closure`
- `e35d138f3 docs(pipeline): point the two doc links at targets rustdoc can resolve`
- `da8957732 perf(pipeline): bound the carrier's lifetime, and derive the pin`

### History: `Cargo.lock`

- `2bfd0abc4 refactor: use native owners for mathematical projection helpers`
- `3e2a0e559 Implement mathematical target codecs and preservation ledgers`
- `6c7471802 Select the complete native shared-view integration`
- `41e48a592 Align substrate admission and reporting with shared-view delivery`
- `83d2e5754 Accept shared dataset views in native DL services`
- `ec550989f refactor: use native BLAKE3 for content identities`
- `ff2429d5b Use the native JSON Schema engine for instance validation`
- `f70b889c4 feat: submit source packet theories through the governed candidate tool`
- `f21d94925 fix: qualify external corpus quality profiles`
- `0514c5dcf build: consume qualified native causal proof support`

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

### History: `.github/workflows/ci.yml`

- `cc47395c7 fix: preserve main acceptance runs and lower stale helper debt`
- `a6098f3c7 perf: avoid repeated value-flow telemetry scans`
- `334eddd3c perf: expose cold producer phase boundaries in CI`
- `bc70ca583 fix(ci): fetch locked inputs before the offline helper census`
- `edef801c6 fix(ci): require helper hygiene in the initial scheduling wave`
- `b25d5c510 chore(deps): bump actions/download-artifact from 8.0.1 to 8.0.2 (#1823)`
- `4ea374012 chore(deps): bump taiki-e/install-action from 2.87.19 to 2.87.26 (#1822)`
- `3315f8531 chore(deps): bump actions/upload-artifact from 7.0.1 to 7.0.2 (#1821)`
- `b4d185f33 chore(deps): bump dorny/test-reporter from 3.0.0 to 3.2.0 (#1819)`
- `38f568b04 chore(deps): bump taiki-e/install-action from 2.86.1 to 2.87.19 (#1775)`

### History: `crates/action-cache/src/executable.rs`

- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `b6c4441ba fix(ci): admit transferred optimized producers`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `22842c8c8 fix: close optimized producer admission gaps`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`

### History: `crates/action-cache/src/executable.tests.rs`

- `9279fffdc fix: bind macro ownership to selected Rust editions`
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

- `bc70ca583 fix(ci): fetch locked inputs before the offline helper census`
- `edef801c6 fix(ci): require helper hygiene in the initial scheduling wave`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `daf8552c1 fix(1768): harden one-file build acceptance`
- `4f3411112 fix(1768): ship one-file prover consumer`
- `b6c4441ba fix(ci): admit transferred optimized producers`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `0bddf6ad9 fix(pipeline): isolate finalized fixture selectors from sync candidates`
- `22842c8c8 fix: close optimized producer admission gaps`
- `4ef37a98e perf: persist producer fixture actions across CI runs`

### History: `crates/logic/src/reason/leave_one_out.rs`

- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `0996b01d0 fix(reason): retain property-chain probes on native path`
- `197c1922c perf: certify incremental reasoning probes`
- `28817b370 fix(logic): preserve alternate calculus spellings`
- `8557d7e43 perf(logic): batch certified slice probes`

### History: `crates/logic/src/reason/mod.rs`

- `ec550989f refactor: use native BLAKE3 for content identities`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `197c1922c perf: certify incremental reasoning probes`
- `28817b370 fix(logic): preserve alternate calculus spellings`
- `8557d7e43 perf(logic): batch certified slice probes`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `7aa92ca5a $(cat \ 	<<'EOF' fix(logic): route modal evaluation through reason_program (#1695)`
- `ec13e5721 fix(logic): preserve modal derivation joins (#1695)`

### History: `crates/build-inputs/src/resolution.rs`

- `8557d7e43 perf(logic): batch certified slice probes`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `slices/grounding/logic/module.ttl`

- `08eae0721 fix: preserve language and direction on word properties`
- `77bd7a3da fix: enforce finite numeric radial-field and measure values`
- `c1f95a865 feat(validate): enforce alignment projection purity`
- `89bbc5f9f feat(logic): derive the callable catalog from canonical signatures`
- `539ecc681 fix(logic): ground native builtin evaluation refusals`
- `ee262a12f fix(logic): preserve annotation metadata through integrity constraints`
- `84b05f060 fix(diagnostics): bind source and recorded-consumer failures to ontology classes`
- `8ba5c655d docs(logic): bind probability model requirements to query execution`
- `94b5979d9 fix(logic): preserve context and relation coordinates in atomic applications`
- `12380e2df fix(logic): remove facet string errors and ground service refusals`

### History: `crates/logic/src/physical/chase.rs`

- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `d3ca06032 fix(logic): keep restricted blocking on exact native premises`
- `22193fc44 fix(logic): preserve restricted-trigger proof conditions`
- `a95adedf4 fix(1796): read a Thing filler as any successor in restricted triggers`
- `1f80fdcb0 perf(1796): trace why a restricted trigger closure does not block`
- `c3e9669f0 perf(1796): name the members of a remaining restricted cycle`
- `d677fbe1b fix(1796): read joint acyclicity off the weak-acyclicity position graph`
- `e0cda306c perf(1796): report restricted-candidate sizes before the trigger checks`
- `ce276d4e8 perf(1796): skip restricted-trigger closures that cannot reach the head`

### History: `crates/logic/src/physical/seminaive/joint.rs`

- `a0b0deb02 Execute native node-kind integrity checks`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `22193fc44 fix(logic): preserve restricted-trigger proof conditions`
- `96b1ac63a fix(1757): name the obstruction that withholds completed native reads`
- `61fc549fb fix(1757): admit definition writes that only re-derive asserted statements`
- `3238773a2 fix(1757): admit clash commitments under a source-bound contextual grammar`
- `bce830888 fix(1757): name the write that changes a selected definition grammar`
- `12e5c4f8a feat(1796): apply Datalog-first to every chase executor and producer`
- `0e9a889a4 feat(1796): run the restricted chase Datalog-first`

### History: `crates/logic/src/physical/seminaive/joint/input/admission.rs`

- `ec550989f refactor: use native BLAKE3 for content identities`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `22193fc44 fix(logic): preserve restricted-trigger proof conditions`
- `4c6c7844e perf(1796): check restricted triggers only inside joint-acyclicity components`
- `4e7c7f2a7 fix(1795): bind termination specializations from closed witness-free relations`
- `7a285ccfb fix(1795): encode termination statements per atom with fresh-null keys`
- `51b7abd40 fix(1789): specialize termination analysis without combinatorial products`
- `14e32bf7e wip(1789): project immutable join solutions before specializing`
- `0c82e4d08 fix(1789): size the input-specific termination certificate for production`

### History: `crates/logic/src/reason/value.rs`

- `bbe250ab8 fix(logic): delegate rational lexical admission to the native parser`
- `bd2e35b8c fix(logic): preserve native datatype lexical admission`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `9fa28c6fe fix(logic): keep rejected decimals outside rational interpretation`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `551dee071 fix(logic): compare admitted anyURI enumeration values after collapse`
- `22193fc44 fix(logic): preserve restricted-trigger proof conditions`
- `2a942b70f fix(1757): admit xsd:anyURI as an intrinsic primitive datatype`
- `8b5f885c0 perf(1757): trace and restructure the native reasoner critical path`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/logic/src/physical/effects/value_flow.rs`

- `4733616a6 fix: reject root tracing aliases during source admission`
- `4b1b774f8 perf: materialize constrained value-flow columns directly`
- `190919e9f perf: start seeded value-flow joins at constrained atoms`
- `ec550989f refactor: use native BLAKE3 for content identities`
- `bb4c44cf6 perf(logic): skip excluded rectangle predicates before lookup`
- `ac5ed846c perf(logic): avoid inverse lookups for fixed predicates`
- `a6098f3c7 perf: avoid repeated value-flow telemetry scans`
- `ec2222790 fix(logic): preserve portable browser reasoning identities and timing`
- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `22193fc44 fix(logic): preserve restricted-trigger proof conditions`

### History: `crates/logic/src/physical/chase.tests.rs`

- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `b797a315e test(logic): pin native closure mismatch diagnostics`
- `d3ca06032 fix(logic): keep restricted blocking on exact native premises`
- `22193fc44 fix(logic): preserve restricted-trigger proof conditions`
- `a95adedf4 fix(1796): read a Thing filler as any successor in restricted triggers`
- `d677fbe1b fix(1796): read joint acyclicity off the weak-acyclicity position graph`
- `c7c5a20c8 fix(1796): read witness-family summaries per rule on the position rungs`
- `da8d326b4 fix(1796): let exact count-1 witness families climb the polynomial ladder`
- `7a285ccfb fix(1795): encode termination statements per atom with fresh-null keys`
- `a465c9970 feat(1796): certify restricted joint acyclicity`

### History: `crates/logic/src/physical/seminaive/property/datatype.tests.rs`

- `bbe250ab8 fix(logic): delegate rational lexical admission to the native parser`
- `bd2e35b8c fix(logic): preserve native datatype lexical admission`
- `9d992189d refactor(logic): delegate anyURI values to the native substrate`
- `9fa28c6fe fix(logic): keep rejected decimals outside rational interpretation`
- `551dee071 fix(logic): compare admitted anyURI enumeration values after collapse`
- `22193fc44 fix(logic): preserve restricted-trigger proof conditions`
- `2a942b70f fix(1757): admit xsd:anyURI as an intrinsic primitive datatype`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `crates/logic/src/reason/artifacts.tests.rs`

- `eca80ae00 refactor: integrate pinned PurRDF APIs and native reasoning repairs`
- `b797a315e test(logic): pin native closure mismatch diagnostics`
- `13a44f7a9 fix(logic): build artifact fixtures from native execution`
- `41ec956a4 deps: adopt purrdf 3.0 and enforce a major-3 substrate floor (#1784)`
- `a159730f8 perf(logic): integrate the certified native pipeline`

### History: `fuzz/Cargo.lock`

- `2bfd0abc4 refactor: use native owners for mathematical projection helpers`
- `3e2a0e559 Implement mathematical target codecs and preservation ledgers`
- `6c7471802 Select the complete native shared-view integration`
- `41e48a592 Align substrate admission and reporting with shared-view delivery`
- `83d2e5754 Accept shared dataset views in native DL services`
- `ec550989f refactor: use native BLAKE3 for content identities`
- `0514c5dcf build: consume qualified native causal proof support`
- `a0b0deb02 Execute native node-kind integrity checks`
- `8ad7b3120 feat(logic): retain cancellation through recorded native services`
- `28d77401b fix(logic): preserve native refusals and correct music data roles`

## Identifiers (files containing each at HEAD)

- `a159730f8`: 0
- `NativeReasoningSession::retract`: 0
- `b1676af1a`: 0
- `ghprsq`: 4
- `prepare_reasoning_input`: 84
- `SourceView`: 3
- `reason_all`: 72
- `tracing`: 55
- `ValueFlow::summarize`: 0
- `Domain`: 339
- `FlowSummary::publish_rectangle`: 0
- `head_domains`: 0
- `publish_head_conditions`: 0
- `perf`: 358
- `rule_index`: 5
- `visit`: 148
- `FlowRule`: 6
- `analysis_body`: 5
- `parse_cardinality`: 5
- `CardinalityEqual`: 3
- `Other`: 135
- `maximum_one_equality_requires_both_qualifiers_and_keeps_every_premise`: 1
- `FlowRule::native_witnesses`: 0
- `witness_frontier`: 22
- `StatementRule::rja_exact`: 0
- `RjaDatalog`: 1
- `rja_exact`: 4
- `rja_trigger_blocked`: 1
- `consumer.distinct`: 1
- `distinct_pairs_satisfied`: 8
- `finite_writes`: 2
- `saturating_mul`: 10
- `limit`: 365
- `facet_length`: 3
- `closure`: 743
- `nonblocking`: 3
- `distinct_pairs`: 48
- `StatementRule::from_ordinary`: 3
- `RjaProgram`: 2
- `LiteralValue::same_value`: 0
- `csv`: 26
- `flate2`: 5
- `b797a315e`: 0
- `live_repo_static_passes`: 2
- `xsd`: 1695
- `case`: 1607

## Workspace packages named

- `gmeow-build-inputs`
- `gmeow-dev-cli`
- `gmeow-logic`
- `gmeow-ontology`
- `helper-census`

## URLs

- https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1802#discussion_r4183954775
- https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1802#discussion_r4183954796
- https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1802#discussion_r4183954803
- https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1802#discussion_r4183954834
- https://github.com/Blackcat-Informatics/gmeow-ontology/issues/1757
- https://docs.coderabbit.ai/guides/review-instructions
- https://app.coderabbit.ai/learnings
- https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1802#discussion_r4286519283
- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39023512214/job/117375201363
- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39034161155/job/117405796015
- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39034161155/job/117405796084
- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39070050658/job/117506113659
- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39070050658/job/117506113637
- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39070050658/job/117506113612
- https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39403720279
