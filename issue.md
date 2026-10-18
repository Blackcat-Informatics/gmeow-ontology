# Issue #1762: refactor: complete Diag migration for prover and physical execution errors

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Problem

`make check` currently fails `crate-check` on remaining first-party `Result<_, String>` error channels. The live repository-static gate requires `gmeow_errors::Diag` as the sole first-party error substrate, but production paths still expose string errors in the prover/external-evidence and physical numeric execution surfaces.

Representative current findings include:

- `crates/gmeow-cli/src/commands/prove.rs`
- `crates/gmeow-cli/src/commands/reasoning_report.rs`
- `crates/logic/src/external_evidence.rs`
- `crates/logic/src/physical/numeric.rs`
- `crates/logic/src/physical/plan.rs`

This is part of #1336's unified diagnostic substrate and currently blocks `make check` for unrelated work.

## Required implementation

Replace each first-party string error with a typed `gmeow_errors::Diag` carrying a stable code, grade, source context, and the evidence available at the failure site. Preserve external process stderr/stdout as diagnostic payload rather than using it as the error type. Do not suppress the repo-static rule or introduce a parallel local error enum that loses the shared diagnostic fields.

## Acceptance criteria

- No production-reachable `Result<_, String>` remains in the reported surfaces.
- Prover discovery/execution failures retain command, artifact, version, exit-status, stdout/stderr and path evidence where applicable.
- External model parsing and numeric preparation failures retain their typed operation/locus and original detail.
- Existing public CLI rendering and exit behavior remains deterministic.
- Focused failure-path tests assert stable diagnostic codes and evidence.
- `make crate-check` no longer reports the string-error rule.

Principles 4, 7, 18. Parent: #1336.


## Comments (5)

### paudley — 2026-09-22T04:50:28Z

Complete diagnostic migration for prover and physical execution (#1762)

Summary: replace first-party string error channels with the existing shared Diag substrate, preserving typed cause and evidence. Principles 4, 7, 18.

Baseline: no deferrals or stubs; expensive Make gates run once after coherent edits; tests never produce the corpus. Standing goals: Rust-first, one canonical diagnostic substrate, maximum information retention, and no silent degradation.

Completeness contract:
| Requirement | Task |
| --- | --- |
| Remove production string errors and error tuples from reported surfaces | 1–3 |
| Preserve command, artifact, version, status, stdout/stderr, path | 2 |
| Preserve numeric operation, source/rule locus, original detail | 1 |
| Preserve external model parsing details and dispositions | 3 |
| Preserve structured evidence in both live Diag and serialized reports | 1–3 |
| Deterministic CLI rendering and exit behavior | 2, 4 |
| Stable-code/evidence failure tests | 4 |
| crate-check no longer reports string errors | 4 |

0. Fetch main and create isolated .worktrees/issue-1762-diag branch/worktree; preserve unrelated work.
1. Migrate every production numeric preparation and execution failure return to `gmeow_errors::Result` with typed Diag kinds. A cloneable typed preparation payload may support cached diagnostic reconstruction, but never becomes a parallel function error type: `Diag` owns propagation at every failure-returning boundary. Each cached retrieval mints a fresh Diag and attaches the requesting rule. Preserve operation, operands, datatype, rule locus and original scalar detail. Validate with malformed literal, missing binding, arithmetic failure and repeatable cached-error cases; then commit with hooks, push, and post the validation/hash to #1762.
2. Define one evidence-preserving diagnostic-to-report conversion contract. Extend the serialized report diagnostic payload with grade, context and typed machine fields used by these surfaces; retain deterministic text and exit behavior. Migrate source discovery/read/parse, problem publication, digest, executable discovery/version and execution. Every process failure retains argv, executable/problem identity, available status/signal, timeout/truncation, and both streams' partial bytes/digests/counts. Bounded capture and child cleanup remain mandatory. Validate missing/unexecutable prover, failing/empty/truncated version, timeout/nonzero exit, partial stream failure and source failures; run shared-behavior integration review; commit with hooks, push, and post the validation/hash.
3. Migrate external artifact/model failure propagation to `gmeow_errors::Result`. Convert `ArtifactProtocolError` and `EvalError` into typed diagnostic payloads or non-error outcome classifications, with Diag owning every failure-returning boundary. Invalid, unsupported, structural-only and resource-limited outcomes remain distinct typed decisions; no caller infers them from prose. Preserve parser/model locus and original detail. Validate malformed/unsupported models and evaluation-budget exhaustion; run integration review; commit with hooks, push, and post the validation/hash.
4. Audit all named surfaces, dependent callers, local error enums and error tuples. Add report serialization/exit regressions and assert code, grade, typed evidence, retained causes and stable ordering. Run `make fmt`, `make lint`, `make nextest-synthetic NEXTEST_FILTER='package(gmeow-logic) | package(gmeow-cli)'`, then `make crate-check`; run one complete `make check` after all changes. Repair every in-scope failure. Run the deferral scan and verify the deficiency ledger before PR creation; commit any final test/audit unit with hooks, push, and post its evidence.
5. Open a non-draft PR closing #1762 and publish the plan/evidence. Address every human and CodeRabbit thread under stage-2, repeating validation/commit/push/issue updates per logical remediation. Merge current origin/main into the branch, re-run required evidence, write squash notes, and merge only with `/home/paudley/stage/root/bin/ghprsq`. Verify PR merge, issue closure and main before removing only this worktree.

Enhancement audit: adopted the medium/high-payoff transformation of one evidence-preserving Diag-to-report conversion contract across all named surfaces. Adopted reuse of DiagKind/source context/process receipts, machine-readable operation and artifact identities, and failure injection for partial capture/repeated cached failures. Declined a new diagnostic framework because the existing canonical substrate already supplies codes, grades, provenance and serialization targets; extending its projection is sufficient. Declined ontology changes because this issue migrates internal failure carriers and mints no semantic vocabulary. No requirement is deferred or stubbed.


### paudley — 2026-09-22T08:56:51Z

Implementation commit `ce1c916a7d7820dddc9c0f20648ea8cd9ac76c49` is pushed on `issue-1762-diag-migration`.

Implemented:
- migrated prover discovery, source admission, problem publication, version probing, subprocess execution, and process cleanup to typed `Diag` failures;
- retained executable/problem identities, argv, deadline, status/signal, stdout/stderr bytes, digests, counts, truncation state, artifacts, and lower diagnostic causes;
- migrated external evidence parsing/checking to typed invalid, unsupported, and resource classifications with operation, locus, source text, parser input, observed use, and limits;
- migrated numeric preparation/execution to `Diag` with typed rule, operation, operands, datatype, and scalar detail, including fresh diagnostics from cached rejected plans;
- preserved the existing non-failure report wire shape and `logic fragments` rendering code.

Validation:
- `make fmt`: passed.
- `make lint`: passed.
- focused `gmeow-cli` nextest selection: 12/12 passed.
- focused `gmeow-logic` nextest selection: 15/15 passed.
- current `gmeow-dev crate-check` differential: clean `main` reports 38 `Result<_, String>` findings; this branch reports zero and adds no new repo-static violation. The command still exits nonzero on unrelated baseline policy findings.
- focused Clippy is blocked before the touched crates by six existing `gmeow-build-inputs` collapsible-if/match findings.
- deficiency ledger: clean.
- mechanical deferral scan: clean.

Full `make check` is blocked by the snapshot path: after the producer completed and `stage-archive-blobs` published its authenticated receipt, `stage-snapshot` ran for more than three hours without publishing the next receipt and was terminated. That exceeds the operational ceiling and is being handled under #1757 before PR creation.

Principles 4, 7, 18.


### paudley — 2026-10-15T21:57:12Z

The existing implementation commit `ce1c916a7` has been recovered and its exact patch applied cleanly to the current combined qualification candidate for PR #1802. The newer native RDF 1.2 and corpus-purity changes remain intact. No replacement implementation or gate suppression was introduced.

The full-gate retry exposed 54 string-error findings on current source, versus the 38 covered by the historical migration. Focused external-evidence and numeric checks are now running against the integrated source; the older branch passes are not current qualification. The newer helper-census channels, production test-module classification, native diagnostic failure-class bindings and browser substrate attestations remain concrete gate blockers. There has been no qualifying full-gate pass, new commit, issue closure or merge.

Next steps: settle those known source prerequisites, verify the affected CLI and diagnostic consumers, then perform the required qualification and review before ordinary commits and ghprsq integration. Principles 4, 7, 18.


### paudley — 2026-10-16T22:14:30Z

# Diagnostic source-seal repair

The current source-only repository guard found two defects while qualifying the
already-integrated diagnostic migration: route_facet_value returned Result<(),
String>, and logic.dl-service had no canonical failure class. The original run
executed 29 cases: 28 passed and live_repo_static_passes failed on those two
findings. See diag-current-qualification.log.

Contract facet routing now appends directly to the source compiler's existing
Diagnostic observations. The obsolete string-return path and caller conversion
are removed. Conflicting singleton values retain INVALID_CONTRACT_FACET, Error
severity, the exact contract owner and both conflicting values; source admission
continues to reject them. Repeated values and set-valued facets retain their
previous semantics. This is the compiler observation channel, not a new runtime
error abstraction or a suppressed refusal.

DlService now names DirectSemanticsServiceRefusal as its canonical failure class.
Its existing code, grade, typed no-model/malformed/execution disposition and
original upstream error remain intact. The owning logic slice declares that
operational class and its enforcing boundary, with all seven English prose
fields and paired French/Chinese translations. No authored shape or external
semantic authority was introduced. Principles 4, 6, 7, 17 and 18 apply.

Commit 12380e2df is pushed in PR 1835 and the source tree is clean. All 45 focused
cases passed in 17.824s after 7m34s compilation, including the live repository
source seal. Strict producer-independent workspace/all-target Clippy passed with
warnings denied in 4m11s. Every normal signed commit hook passed. Logs:
diag-repair-qualification.log, diag-repair-clippy.log, diag-repair-commit.log and
diag-repair-push.log. The new head has live producer/lint/Rust-prebuild CI.

Hosted ontology/translation and full consumer qualification remain pending.
No local producer, corpus production, full gate or machine probe ran.
The prior helper repair's hosted optimized producer, lint and Rust prebuild all
passed on c3123db1a. Its downstream generation failed with the known recovery-leg
error; generation-current.log retains that evidence and PR 1830 owns the repair.

No full gate or translation/ontology corpus validation is inferred from the
source seal. This repairs two current diagnostic source-seal defects without
claiming full issue 1762 qualification or full native semantic completeness.


### paudley — 2026-10-17T01:32:01Z

# Preserve native source diagnostics through proof preparation

SourceCatalog converted a document-mapping Diag to LogicParseError(String), then
to a generic stage failure. That discarded the original kind, grade, focus and
evidence. Prepared source construction, frontend/dialect parsing, round-trip
drivers and the prover also depended on this string-only error API.

The wrapper is deleted throughout the Rust workspace, without an alias. The
compiler, prepared source, CLIF/CGIF/XCL readers and their real callers return
the shared Diag. Native failures propagate unchanged with operation/document
context. The callback regression preserves its original FormulaAdmission kind,
code, grade, focus and tags, and verifies that failure publishes no receipt.

Registered source-parse and source-read kinds retain the original live foreign
error, selected format and exact path. Arc retains the original cause while
satisfying the existing diagnostic-kind cloning contract. Empty source remains
a typed frontend refusal. File reads use the actual I/O result rather than a
separate existence check. The actual consumer prover uses its existing preserved-
diagnostic adapter, retaining outer reason codes, source identities and native
cause evidence. A consumer empty-selected-theory regression exercises this path.

Conformance, developer feedback/certification, transcode, slice reasoning,
pipeline examples, the round-trip drivers and the explicit fixture-producer
example move with this API. Obsolete identity/error adapters are deleted. The
explicit fixture producer was compiled, never invoked from a test. No runtime
dependency, fallback, profile weakening or successful program-identity change.

Validation: native-source-diag-synthetic-tests.log records 138/138 passing cases
in 0.108s. These cover source admission/FOF, all three Common Logic dialects,
conformance admission controls, source mapping and typed parser/I/O failures.
After removing the last four obsolete identity adapters, all three affected
pipeline round-trip cases passed again in 0.243s:
native-source-diag-final-roundtrip-tests.log. Strict producer-independent
workspace/all-target Clippy passed with warnings denied in 2m12:
native-source-diag-final-clippy.log. Complete helper hygiene passed:
native-source-diag-final-helpers.log. No ledger ceiling or gate was weakened.

The first expanded selection accidentally included the corpus-dependent
corpus_logic_module_common_logic_roundtrips case. It correctly FAILED CLOSED
without GMEOW_TEST_FIXTURE_MANIFEST; 138 other cases passed. Exact failure:
native-source-diag-integrated-tests.log. The corrected local diagnostic selection
excludes that authenticated consumer only; its test and CI requirement remain
unchanged. The corpus observation and new embedded consumer CLI regression are
not locally qualified. No test built corpus and no local optimized producer,
fingerprint or full gate ran.

Earlier compile attempts exposed the diagnostic macro's field/Clone contract,
old tuple-field consumers, the explicit fixture example and obsolete identity
adapters. Their failed logs are retained. These were corrected in source without
suppression, hook bypass or a compatibility wrapper.

Published as 1c3bc726a. Normal signed hooks and push passed:
native-source-diag-commit.log and native-source-diag-push.log. Hosted acceptance
is not implied by local checks.

Full hosted production/consumer acceptance, current-head review, canon semantics,
prepared native canon, blinded installed proof and production-world measurements
remain required. Neither 1762 nor 1770 is complete, and the full logic issue/PR
goal and plan.md remain in force. No pipeline speedup or final merge is claimed.
Principles 4, 7, 12 and 18.


