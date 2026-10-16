# Current publication and merge instruction

SUPERSEDING USER DIRECTION: "Please ghprsq this branch now, we'll create issues
for anything that comes up in CI, but we need to get this code pushed and
merged." Integrate now with CI pending, using ghprsq and its normal guards.
Do not claim the pending CI or production/consumer acceptance passed. File or
update issues for observed CI failures afterward. Keep unfinished issues open.

Published source: eca80ae008d24dccf5b4cd19011e0c7acf28d3b8 on the existing
issue-1757-production-verdict branch and PR 1802. Normal pre-commit verification
passed. The local branch now tracks that same existing PR branch; no branch or
worktree was added. Its current open.json branch binding was repaired while
preserving the original setup history.

Latest user direction: "If it passes, merge it. We have so much work ahead of us
that we'll revist any issues." Merge through ghprsq when required current-head
CI passes. Keep unfinished issue acceptance open; do not invent an independent
audit or bypass a failing or pending check. This supersedes an additional
independent-completion-review prerequisite for this immediate merge.

CI run 38924824685 is pending. Lint, CLA and the CodeRabbit status check are green;
the latter is not a fresh current-head review submission. The numeric-range
finding has the published fix and two current consumer regressions in reply
4296418846; its thread is resolved.

An additional 143 chase/joint controls passed in raw/reconciliation-review-controls.log,
bringing this reconciliation to 207 passing focused tests. Strict source Clippy
passed. The actual new source commit has a clean Git merge preview against
fetched main; this is not combined-runtime qualification.

Local producer fingerprinting has not been invoked. Remaining 1768/1770 and other
logic requirements stay open. No side worktree or branch has been deleted.

## Earlier current-state and historical snapshots

# Current resumed integration

See branch-reconciliation.md for the accepted consolidation order, refreshed
81-issue inventory, side-branch findings and preservation/cleanup boundaries.
The missing 1774 correspondence/retraction behavior is now ported; its focused
run passed 40/40 in raw/reconciliation-consumers.log. Another 22/22 native
artifact, source-lookup, RDF 1.2 carrier and certificate controls passed in
raw/reconciliation-native-carrier.log; both numeric lowering/validation controls
also passed. Strict producer-independent Clippy passed in
raw/reconciliation-clippy.log. No merge is qualified.

fof-property-reflection.md records the latest 103 passing FOF/artifact controls
and strict source Clippy/lint. These checks do not qualify the later retraction
change, complete canon, authenticated producer or four excluded consumers.
Normal pre-commit verification has been restored; source publication remains
pending the normal commit hook and push. No side worktree or branch has been deleted.

Producer authentication remains unauthorized under the no-probing restriction.
The current source and older CI must not be conflated. The full original logic
issue/PR goal, including 1768/1770 and newer consumer logic requests, is unfinished.

## Earlier evidence snapshots

Latest continuation: see fof-source-class-laws.md for source-bound restriction
laws, fatal ambiguity diagnostics, quantified primitive-class profile laws and
31 passing focused controls, strict producer-independent Clippy and repository lint.
The complete objective remains unfinished.
See fof-document-selection.md for the reproduced document-
selection defect, repaired contributor accounting, 20 passing selected controls,
passing strict producer-independent Clippy and repository lint,
and current forge inventory/review evidence. See fof-admission-integrity.md for the reproduced and repaired
proof-admission integrity defect and passing FOF controls. See
native-service-cli-diagnostics.md for shared service/CLI diagnostics, controller
package-ID repair and the pending producer-fingerprinting authorization question.
correspondence-operation-failures.md records native law execution diagnostics and
the fallible production compiler callback. constructor-list-grammar.md records the preceding list-grammar fix
and original feedback regression. purrdf-fallible-native-pipeline.md records the
Git-main native/pipeline port; its failing list-feedback status is historical.
The math/GMN handoff remains historical evidence for the preceding layers.

## PurRDF Git-main projection-emission migration continuation

This goal turn made source and evidence progress. The full objective remains active.

TripleSink now immediately propagates native storage failures through every
intern/push/list/codec operation. It uses the upstream fallible collection builder
at purrdf_core::collections::try_build_rdf_list without changing cell identities
or emission order. No storage failure is deferred, ignored or converted to a panic,
and no weaker alternate emitter was added. Malformed subjects and inconsistent cell counts
return typed semantic refusals. Native freeze, serialization and UTF-8 failures
retain their original sources in the shared diagnostic envelope.

RDF OWL DL/EL, gUFO and canonical projections, formula/term/contract/context
emitters, presentation emitters, report/A-Box emission, correspondence graph and
text wrappers, and their actual production callers now propagate the same Result.
RdfProjectionResult::into_text is a fallible terminal boundary. The top compiler
no longer converts these diagnostics into detail strings. Semantic OverclaimError
remains a distinct typed cause with the existing compiler projection code; pure
semantic guards retain their own type. The existing unsupported-report refusal
control now also requires the exact typed cause and code. General cardinality
values remain upstream exact integers; the nonnegative comparison no longer
assumes i128. Action-policy hydration now preserves native insertion/freeze
failures and installs only successful datasets. Test success construction is
explicitly unwrapped; semantic assertions are retained. The tiny openEHR union
control also requires the now-fallible native union to succeed.

raw/purrdf-main-fallible-clippy-v6.log: terminal FAIL exit 2. The first projection
migration inventory exposed four production typing/API issues plus seven test
handle issues, all subsequently repaired.
raw/purrdf-main-fallible-clippy-v7.log: terminal FAIL exit 2. Projection type
errors cleared; it exposed one ignored action-policy builder result and fifteen
ignored synthetic fixture results, all subsequently repaired.
raw/purrdf-main-fallible-clippy-v8.log: terminal FAIL exit 2. The compiler reached
new dependency layers; an openEHR integration-test union Result was subsequently
fixed. No earlier logic-compile library/unit-test failure remained in this log.
raw/purrdf-main-fallible-clippy-v9.log: terminal FAIL exit 2. There are zero
logic-compile compiler diagnostics, including integration targets. Remaining
reported errors: math-lift/src/sink.rs two; lang-bridge/src/gmn1_codec.rs one;
gmn1_witness.rs one; gmn1_codec.tests.rs four; gmn1_digest.tests.rs one. Passing
compilation of a layer is not a unit-test execution or full workspace pass.
raw/purrdf-main-fallible-lint-v7.log: terminal PASS exit 0 on final source.
make fmt v18 and git diff --check passed. No runtime/unit tests executed; the
workspace inventory still cannot compile. No corpus producer, full gate, browser
rebuild, commit, push, message posting or merge occurred.

Live forge refresh retained in raw/pr1802-purrdf-migration-review.json. PR1802 is
OPEN at b797a315e09cf6a118e0ca60975007a65f43a138, with generation(b), fixture-prefix
and quality failures; those checks do not qualify current local source. Its latest
CodeRabbit review (2026-10-15T18:52:00Z) requests two changes. Current local
chase.tests.rs has formatted certify_restricted_joint_acyclic calls without the
horizontal trailing commas. Current reason/artifacts.tests.rs already asserts
that mismatched.validate_native_closure returns the specific missing-native-witness
closure diagnostic. Both comments are locally addressed, not published/resolved.

Next actionable migration: math-lift sink and all actual RunFrame/R/ONNX/proof
emitter callers; GMN model hydration and canonical comparison/witness helpers,
whose current infallible APIs still panic at freeze. Gmn0Model::to_dataset feeds
canonical_nquads, gmn0_canonically_equal, gmn_metrics and CLI output. Propagate
actual errors throughout their callers rather than replacing native failure with
unmapped residue or false semantic equality. Continue full original logic/prover/
producer/PR qualification scope afterward. Source migration is not completion of
that scope, and prior corpus/CI/browser evidence remains stale for the Git pin.

## PurRDF Git-main reader-view migration continuation

The exact official pin remains 7734b013d19b68600ad24c4e8aa5a40955a8e89e.
Current direct TOML verification confirms identical sets of 31 PurRDF packages
and one exact Git source in root/fuzz lockfiles, with no registry duplicates.

Reader projections now return shared diagnostic Results from every storage
operation, preserving graph scope, original annotations/reifiers and quoted
terms. Recursive term remapping publishes entries only after successful native
interning. Freeze no longer panics. Actual validation, carrier, slice and explicit
fixture-producer callers propagate failure; the producer's OnceLock installs only
a fully constructed reader view. Existing semantic controls retain all assertions.
Presentation arities and blank scopes use checked conversion from upstream exact
integers into their existing native index domains. Synthetic fixture construction
now explicitly requires successful fallible builder operations. SPARQL projection
controls retain upstream DatasetHandle instead of converting away its admission.

raw/purrdf-main-fallible-clippy-v4.log: terminal FAIL exit 2; 105 logic-compile
lib-test errors, 18 production errors, math-lift and GTS fixture failures. The
reader-view, frontend exact integer and foundation ChapterProjection errors from
v3 are absent. This is compiler evidence only, not a runtime test pass.
raw/purrdf-main-fallible-clippy-v5.log: terminal FAIL exit 2; 19 logic-compile
lib-test errors, including the same 18 production errors and one DatasetHandle
facade-export path error. The path was then corrected to the upstream-defined
purrdf::backend::DatasetHandle; that final path correction is not compiler-checked.
GTS fixture errors and the prior 105 fixture-error inventory are absent in v5.
Math-lift still fails at its two old infallible sink boundaries.

raw/purrdf-main-fallible-lint-v5.log: terminal PASS exit 0. v4 failed while source
was being edited concurrently and is not a clean qualification result. Formatting
v11 and git diff --check passed. v12 formats the final handle-path correction.
No unit tests ran: the workspace inventory remains compilation-blocked. No corpus
producer, full gate, browser assets, commit, push, PR update or merge was performed.

Next: migrate TripleSink and its RDF/presentation/report/correspondence callers,
then math-lift's actual emitter graph. Propagate native errors immediately rather
than panicking, stringifying the storage cause, swallowing Results or collecting
a deferred failure. The semantic OverclaimError must remain distinguishable from
storage failures. Continue remaining full-goal semantics/prover/diagnostic and
qualification work after source migration; the earlier list-feedback regression
and authenticated full producer acceptance remain unresolved.

# Acceptance index

Current candidate: the dirty combined 1757 worktree. Published PR 1802 remains at
b797a315e; no current uncommitted repair is qualified by that head's CI. Source
changes remain uncommitted. No new PR, merge, issue closure or completed-goal
claim has been made. Principles 2, 3, 4, 7, 12, 17, 18 and 19 apply.

## Resumed goal: upstream source pin and fallible API migration

The user paused the broader goal, then authorized pinning PurRDF Git main.
The exact live main revision 7734b013d19b68600ad24c4e8aa5a40955a8e89e now owns
all 31 selected PurRDF packages in both production and fuzz lockfiles. The direct
requirements retain major 3 and select the official repository plus complete rev.
See purrdf-main-pin.md for source-gate changes, checks, and authorization.
The earlier 95/96 run and all earlier producer/browser/CI evidence predate this pin;
they do not qualify it. The user subsequently resumed the full goal.

The first Git-pin test build is terminal exit 2 before test execution. Its
three diagnostic-renderer type errors exposed the newly fallible native builder.
The renderer now uses an associated error on its shared Sink: text remains
Infallible, native insertion retains the actual storage error through Diag.
append_gmeow_findings, dataset construction, ABox annotations, meta-finding
projection, and their actual pipeline callers now return/propagate those failures.
A synthetic sink refusal control requires the original failing predicate and
immediate stop; existing annotation parity remains selected for later execution.

The first strict Clippy inventory is terminal FAIL in
raw/purrdf-main-fallible-clippy.log. It found the additional unchecked ABox append
(subsequently repaired) and four importer interning helpers in foundation-corpus
that still return bare IDs. The next full producer-independent Clippy inventory
is raw/purrdf-main-fallible-clippy-v2.log; source lint is
raw/purrdf-main-fallible-lint.log. These checks are pending, not passes.
No producer, fixture materialization, current-substrate runtime test pass,
commit, push, or merge has occurred.

The second Clippy inventory is terminal exit 2. It got past the errors crate and
reported the next migration sites, including 177 logic-compile lib-test errors;
raw/purrdf-main-fallible-clippy-v2.log retains the complete diagnostics. This is
not a workspace pass or runtime evidence. The foundation importer now propagates
142 calls through 17 fallible methods using its existing io::Result boundary;
io::Error::other retains the actual StorageError, including freeze failures.
Missing required record fields now return InvalidData instead of panicking, with
one five-case synthetic regression. No actual repository corpus is built in tests.

Relational-core term construction, native identity and native/text projection
now propagate native insertion errors. Its terminal text API returns Result and
all eleven test callers still require successful projection before their existing
semantic assertions. NumericLiteral uses the upstream exact Integer for its
zero/one comparisons; the coordinate's numeric value is not narrowed to a machine
integer. rcIndex still requires a representable non-negative native position.

The third Clippy inventory is terminal exit 2 in
raw/purrdf-main-fallible-clippy-v3.log. It no longer reports those relational-core
or NumericLiteral errors, but still has 130 logic-compile lib-test errors in
other source/projection readers and emitters. Its one foundation finding was
type-complexity after the Result addition; this was repaired with the concrete
ChapterProjection record, without suppressing the lint. That last adjustment is
not yet compiler-qualified. Fresh source lint is
raw/purrdf-main-fallible-lint-v3.log (terminal PASS, exit 0). Formatting and diff checks pass;
unit/runtime validation and the broader native/corpus acceptance remain unrun
on this Git substrate. The next core batch is TripleSink and its projection
callers: storage failures must propagate immediately, not become panics or be
flattened into an OverclaimError string. The all-logic goal remains active.

## Current continuation: program-aware obligation discharge

UNQUALIFIED. The original actual-program regression is retained as FAIL in
raw/program-obligation-baseline.log. The current checker now consumes mandatory
native program/input/engine/world writer evidence and produces typed outcomes.
It no longer accepts a flattened graph and a caller-supplied predicate set.

The first integrated run compiled, then failed 22/41 tests (19 passed), nextest
b5748324-a13a-4da3-ae6d-7e957d83399f, raw/program-reachability-controls-v2.log.
Ten verifier cases require the absent authenticated fixture selector; these
were an invalid synthetic selection, not qualifying verifier acceptance. The
remaining failures exposed an incorrect assumption that demanded proof evidence
already retained every governance field. The candidate now registers selected
governance fields at source admission and after each committed suffix, including
zero-budget runs. Scalar declaration fields preserve their original RDF kinds;
blank normalization must not turn an invalid forbidden predicate into an IRI.
The corrected focused selection passed 77/77 in 1.196s, terminal exit 0,
raw/program-reachability-controls-v4.log, nextest
a3dde38a-d124-4f1b-a62c-c059f6743277 (workspace compilation 6m19s). It covers
the original four-case regression, source kinds, mixed typing, schema-induced
and open writes, source/import/program/engine changes, world isolation, original
blank identity, finite/budgeted alternatives and mandatory receipt/schema fields.

A subsequent adversarial control reproduced a second defect: a zero-budget run
with an original invalid cardinality operator incorrectly syntactically discharged
an unrelated predicate because its still-unobserved source-admission inventory
was empty. The plain source/no-operator arm remains a valid independent syntactic
proof. raw/program-reachability-admission-baseline.log retains the failure. The
candidate now retains whether original native operators require admission and
requires actual complete source-owner observations before negative discharge.
The native role classifier is shared with the existing construct observer; no
source reconstruction or weaker fallback was added. This repair passed its
control in the broader current-source run below.

The current-source broader run completed 413 cases: 405 PASS, eight FAIL,
18.862s execution after 8m46s compilation including lock wait;
raw/program-reachability-native-final.log, nextest
bac7243b-2d1f-4fdc-a145-40d86c40d5d3. All selected obligation, identity and receipt
controls passed, including the reproduced zero-budget source-admission defect.
Six failures expected private execute() to return Err even where the public
native fold now retains typed source/value obstructions. Their remediation checks
the actual public result, incomplete coverage, unknown information, and the
specific typed obstruction; it must not merely accept an arbitrary returned value.
A nominal test expected diagnostic prose that was replaced with a typed original
source refusal; its update checks exact source issue kinds/owners/fields.
The eighth list-feedback case receives NonStratifiable before the expected
termination/bounded execution behavior. Its original bounded requirements remain
unchanged and unresolved; no broader successful verdict is claimed.

The native finalizer already observes the complete committed source suffix before
publishing a nonzero-budget cut (families.rs::finish). A new focused control now
exercises a committed invalid schema head at that boundary, rather than inventing
another runtime census. The affected consumer/control selection is pending in
raw/native-admission-consumer-controls.log. Source strict producer-independent
Clippy passed (3m20s, handle 32659, raw/program-reachability-clippy-final.log), as
did source lint (handle 49191, raw/program-reachability-lint-final.log), before
these later test-only changes. No production behavior was weakened to satisfy a
private error-message expectation.

The affected follow-up ran 40 controls: 36 PASS and four FAIL, nextest
8b61c86a-ff90-461c-b0cb-483d0f90be0a (1.505s). The original/committed operator
budget controls pass. Four tests now prove the public native result is incomplete
and retains typed source/value obstructions, rather than assuming a private Err.
The malformed-key loop's final undefined-comparison case still produces an immediate
typed Physical error; its separate assertion now preserves that exact route.
The nominal tests retain exact typed original-source list issues and pass.

Two value-comparison cases revealed a real producer-cause loss: emit_head set only
admission_blocked when native value comparison was undefined. The completed-read
roles survived but no typed cause or operands reached the consumer. The candidate
now returns guard obstruction evidence, retains actual native premise proofs in a
world-scoped producer ledger, validates its supports, and folds it into native
verdict gaps. It preserves independent positive writes and does not throw away the
whole execution to recreate an old private Err. New controls require receipt
round-trip and foreign-support refusal. This production delta is compiling and
unqualified in raw/native-producer-obstruction-controls.log; lint/Clippy logs with
the same prefix are pending. The original list-feedback bounded requirement remains
red and unchanged.

The helper ratchet reported one obsolete duplication entry. Its prescribed writer
removed only that group (351 to 350), with zero added groups or copy-count raises;
raw/program-reachability-helper-debt-before.json and
raw/program-reachability-helper-ratchet.log retain the actual transition. The full
helpers-hygiene retry passed, terminal exit 0, recorded in
raw/native-producer-obstruction-helpers-v2.log.

The initial producer-obstruction controls and Clippy both failed compilation:
the new receipt test called to_ntriples on the String already returned by the
projection API. The test now passes that original serialized carrier directly
to parse_reasoning_graph. No assertion or production behavior was removed.
The corrected strict producer-independent Clippy passed, terminal exit 0,
raw/native-producer-obstruction-clippy-v2.log (1m15s). The corrected focused
test run finished with terminal exit 2: 96 run, 95 PASS, one FAIL in 1.720s,
after 5m47s compilation. raw/native-producer-obstruction-controls-v2.log records
nextest 91f0ab88-ab04-4d2f-a480-f8e93859a75f. Both original undefined-value
controls now pass, including receipt round-trip and foreign-proof rejection.
All selected obligations and native receipt controls pass. The only failure is
the unchanged list-feedback existential-cycle test, whose preparation still
returns NonStratifiable before its required termination/bounded behavior.

A temporary diagnostic at JointInput::prepare's actual cycle branch
prints the existing DependencyCycle without changing admission or execution.
The first one-test run (60112, now terminal exit 2) rejected an inline cfg(test)
statement under canonical source ownership; it did not execute the regression.
The cfg annotation was removed, retaining a temporary diagnostic on the actual
failure path. The corrected prescribed-workspace run finished with terminal exit
2: one test run, one failed, nextest 1187b0bb-2950-4b83-a655-6514d5a445f2,
recorded in raw/native-list-feedback-cycle-diagnostic-v2.log. It captured the
membership -> authored seed -> authored existential positive return path and
the completed source-admission edge back to membership. The temporary print
was removed. The original bounded assertions remain unchanged and unresolved.

The 95/96 result and remaining full-goal boundaries were posted through the
Stage 2 workflow to issue 1747 comment 6208211304 and PR 1802 comment 6208215395.
Both posts explicitly identify the changes as uncommitted/unpublished and do not
close the issue, resolve review threads, or claim production qualification.

Source lint passed on the preceding integrated source (handle 1085,
raw/program-reachability-lint-v2.log), before the retained-field repair. It does
not qualify that later delta. The earliest integrated build failed on one unused
import, raw/program-reachability-obligations.log, and is not a behavior pass.

The complete prepared-program identity prerequisite passed five focused controls
(raw/program-identity-focused-v2.log), 180 affected native/certificate consumers
(raw/program-identity-native-consumers.log), strict Clippy and source lint
(raw/program-identity-clippy.log and raw/program-identity-lint.log). These checks
precede the new static analysis and do not clear the obligation regression.
The implementation/acceptance contract is program-obligation-remediation.md.
All full-goal requirements, production refusal, publication and merge remain open.

## Preceding continuation: native bootstrap and runtime review repairs

UNQUALIFIED overall. The full goal still covers all logic issues and PRs. This
section owns live statuses; running/pending statements below Recorded history
describe their own earlier observations and must not restart old jobs.

The language-rule migration changed the original module's BLAKE3 to
9127a8f94adc3dad1c8ca96fa54c6f6a995b6f737a62352ef99a2b2d0bbe2bb3.
The browser pin still named 7572ef1ea3b7e7403c0770bd825c181d54a651e23663a4f206dceefa7569ccbe.
This was an actual source-identity mismatch, not a resource diagnosis. The pin
now matches the measured original bytes. The existing read-only wasm-codebook
verifier also invokes the exact native decoder used by both browser consumers,
retaining its typed refusal, after authenticating and comparing the artifact.
Receipt equality alone cannot approve a packet the actual consumers reject.

| Scope | Latest result | Evidence |
| --- | --- | --- |
| Existing native codebook controls and original-byte pin | 4/4 PASS, 0.031s, terminal exit 0 | raw/native-codebook-source-pin-controls.log; nextest 1f473685-b1e2-4c1b-89b0-8b25d8a683c4 |
| Source lint after the decoder preflight | PASS, terminal exit 0 | raw/native-codebook-preflight-lint.log |
| Native probability-model declaration and graph scope | 4/4 PASS, 0.018s, terminal exit 0 | raw/probability-model-native-scope-controls.log; nextest 8561645e-6675-4c38-9788-cf45144937ba |
| Current-source producer-independent Clippy | PASS, terminal exit 0; warnings denied, final runtime refinement | raw/review-optimizations-final-clippy.log; terminal handle 94145 |
| Latest source lint and helper ownership | Both PASS, terminal exit 0; final runtime refinement | raw/review-optimizations-final-lint.log; raw/review-optimizations-helper-hygiene.log; terminal handles 7583 and 61073 |
| Runtime review repairs | 135/135 PASS, terminal exit 0, 2.982s | raw/review-optimizations-final-controls.log; nextest c129000f-b2b2-46f9-9680-d1bbfb3bbebb |
| Preceding optimized producer build | PASS, 26m54s, terminal exit 0; built before the runtime repairs and does not qualify their new source identity | raw/source-pinned-producer-build.log; handle 80767 is terminal |
| Explicit bundle/codebook bootstrap | FAIL, terminal exit 2; stage-docs-render refuses unsupported native verdict, 16575 obstructions | raw/source-pinned-bootstrap.log; handle 39900 is terminal |
| Earlier standalone producer build | Intentionally interrupted for this required source correction, terminal exit 130 | raw/current-world-certificate-producer-build.log; handle 19663 is terminal |
| Per-world certificate integration after runtime repairs | Refreshed 45/45 PASS, terminal exit 0, 10.766s | raw/review-optimizations-certificate-consumers.log; nextest dff7ae54-1c8b-4b71-bcf5-5fc081315a3c |
| Four browser packages | Earlier preparation completed, but current-source freshness and all parity gates remain open | raw/browser-independent-rebuild.log; handle 6462 is terminal |
| Full qualification | Two failed attempts remain FAIL; no third full gate has started | raw/native-full-gate-retry.log and earlier retained logs |

The actual new decoder-verifier operation still needs its authenticated produced
codebook/selector before runtime acceptance. No test or verifier constructs that
corpus. No bundle, manifest, codebook, receipt or stamp has been fabricated.
The pin and preflight are uncommitted and unpushed with the combined candidate.

Next: settle the applied incorporated-PR runtime repairs and their affected
checks, then investigate the actual unsupported production verdict. Artifact,
fixture-selector and all-six browser asset acceptance remain incomplete; no
terminal output is approved by the failed bootstrap. Preserve the required
full qualification and parity, and
the original CI/cold-generation, review and ghprsq merge requirements. A build
or narrow test pass is not the production verdict.

The separate consumer-proof intake is in consumer-proof-intake.md. No prepared
canon or full-canon consumer proof has been implemented or qualified by this
continuation; all 1768/1770 requirements remain in scope. PR consolidation is
recorded in pr-consolidation.md. No new PR or worktree was created.

The current canonical source already types FullIndependence as ProbabilityModel.
The historical missing-model blocker does not justify inventing a new model or
assuming independence. Existing compiler controls now cover both RDF/native
structural typings, retain the compiled uncertainty facet, and require a hard
contract error when the only model is in an unselected named graph. These are
synthetic tests only; no production source or source pin changed in this pass.
They do not measure the current complete-canon blocker count. The corrected
producer subsequently completed; bootstrap handle 39900 then failed at the
docs gate with an unsupported reasoning verdict. Both handles are terminal.
The bounded source adjudication and passing controls were posted to issue 1770
comment 6205528402. The source-data coupling identified by the pin correction is
recorded in remediation-plan.md as an unimplemented dataflow observation, with
required authentication and full-inventory acceptance preserved.

The bootstrap has completed stage-parse-sources (1146ms), stage-compile-logic
(12888ms), stage-source-load (47255ms), stage-mappings (63231ms) and stage-slice-
brief (110455ms), plus several export branches. This is real-corpus source-load
acceptance through the repaired quotation guard. It is not a completed
generation, cold comparison or full-pipeline speedup. Conformance completed in
359899ms and reason in 1735830ms. GTS composition (16951ms), verify attestation
(48735ms) and validation (52613ms) completed before the docs gate refused the
unsupported native verdict. There is no completed generation or qualified
terminal bundle/fixture selector/browser parity. Stage completion is not a
conclusive reasoning certificate.

The incorporated list-traversal finding now has synthetic baseline controls in
input/admission_tests.rs: exhaustive three-node directed graphs and first-cell
selections, 512 heads sharing one branching rest-only cycle, and the existing
inclusive 262144-row bound with atomic refusal beyond it. Baseline selection
passed all three controls in 0.444s, nextest
8c56b8e0-aa05-4b73-9192-d8685fbc654a, terminal exit 0 on handle 22374;
raw/list-sharing-baseline-controls-v2.log. Per-case durations were 0.016s for
the exhaustive graph/member combinations, 0.322s for the boundary and 0.384s
for the shared cycle. These are single synthetic observations, not a production
speedup or a stable comparative benchmark. The first selection failed compilation
on two &&str constructor arguments; explicit borrowed-item destructuring fixed
those test errors. No production code has changed during the running bootstrap.
For that baseline, format, source lint and strict Clippy completed with exit 0;
raw/list-sharing-test-lint.log and raw/list-sharing-test-clippy.log. The review
accounting and measured baseline were published through Stage 2 to issue 1757
comment 6205990419 and PR 1802 comment 6205990499. The two runtime performance
fixes have since been applied; those baseline checks do not qualify them in advance.

The three-file runtime repair was prepared in
raw/review-optimizations-candidate.patch. git apply --check completed with exit 0;
the actual patch was read in full. It uses the existing upstream SCC primitive
and shared immutable reachable-cell sets, checks the same full association limit
before publication, and precomputes scheduled minting capability before the
standalone/joint generative transitions. No new cutoff, profile or dependency.
It was applied after the bootstrap became terminal. The final source also
discovers reachable nodes once before SCC construction, avoiding analysis of
unreachable incoming rest-only components. Original source facts remain intact.

The broader pre-change synthetic baseline now passes 134/134 in 1.883s, nextest
d433dc81-20ed-4128-bdd9-07966042014b, terminal exit 0;
raw/review-optimizations-broad-baseline.log. Selection includes all physical chase
and joint tests, retained-proof admission/drain controls, witness identity and
budget cuts, plus empty nominal guards, reverse-restriction satisfaction,
distinct XML literals and unknown value interpretations. The first applied
candidate passed the same 134/134 in 2.075s, nextest
e75a6821-63ef-4608-8728-9d0fac9c3b78. Baseline and candidate logs are separate;
the baseline is not a later-source verdict.
Bootstrap handle 39900 subsequently terminated with exit 2: stage-reason finished
in 1735830ms, but its native verdict remained unsupported. GTS composition,
verify attestation and validation actions completed before stage-docs-render
correctly refused that verdict (16575 obstructions). This is FAIL, not a complete
generation or a conclusive reasoning certificate. No third full gate has run.

After that authoritative terminal result, the runtime repair was applied
successfully. make fmt and git diff --check pass. The first candidate passed
134/134 in raw/review-optimizations-candidate-controls.log. Strict Clippy refused
the ambiguous inline Arc initialization; an explicit shared-empty binding fixed
that warning without suppression and the next Clippy run passed. Source review
then added the reachable-union refinement plus a fourth synthetic control:
incoming rest-only components cannot become head membership. The final 135-case
selection passes in 2.982s, nextest c129000f-b2b2-46f9-9680-d1bbfb3bbebb.
Strict producer-independent Clippy, source lint and helpers-hygiene all terminate
with exit 0 in the raw/review-optimizations-final-* and
raw/review-optimizations-helper-hygiene.log files. The affected source/minimum/
per-world certificate consumers were separately refreshed: 45/45 PASS in 10.766s,
nextest dff7ae54-1c8b-4b71-bcf5-5fc081315a3c. That includes the demonstrator-class
consumer's refusal of copied/wrong certificates. No producer or full gate was
restarted. All handles from this continuation are terminal.

Single pre/post observations (seconds), not stable benchmarks or production gain:

| Control | Broad baseline | First applied candidate | Final refinement |
| --- | --- | --- | --- |
| Exhaustive graph/member combinations | 0.037 | 0.047 | 0.067 |
| Inclusive bound and atomic refusal | 0.400 | 0.120 | 0.165 |
| Shared branching rest-only cycle | 0.462 | 0.016 | 0.027 |
| Complete selection | 1.883 (134 cases) | 2.075 (134 cases) | 2.982 (135 cases) |

No aggregate selection or full-pipeline speedup is established by these observations.

The official crates.io Git index was refreshed through its GitHub API:
raw/purrdf-registry-after-bootstrap-api.jsonl still lists 3.0.1 as the latest
stable non-yanked purrdf. The raw-domain fetch timed out (terminal handle 78000)
and sparse-index browser access failed; the successful API capture supplies the
actual entries. The boundary issue/comments are refreshed in
raw/upstream-boundary-after-bootstrap.json. Prior captured ancestry puts the
three upstream boundary fixes after rust-v3.0.1, so a version bump alone does
not supply them. No dependency, profile, selected world or verdict gate changed.
The actual bootstrap refusal and refreshed boundary evidence were published to
issue 1803 comment 6206336506. This is the preceding producer's native result,
not production validation of the subsequent runtime repairs.

The complete six-PR review transfer is in incorporated-pr-review-debt.md. Both
performance findings have local repairs and passing focused/consumer checks;
publication, current-head review and actual production admission remain required.
They are not resolved by commit ancestry. Coverage gaps from skipped and limited
bot reviews are retained, and the historical RJA tuple-bridge integration note
is adjudicated against the newer exact-source blocking contract.

The final runtime repair results and the production qualification limit were
published through Stage 2 to issue 1757 comment 6206378369 and PR 1802 comment
6206382138, using runtime-review-repairs-progress.md. No public review thread,
issue or superseded PR was closed; repairs remain uncommitted and unpushed.

## Recorded history and attributed evidence

The sections below preserve previous observations and their original raw logs.
The live handles and current verdicts above supersede earlier pending statuses.

## Qualification

UNQUALIFIED. Two full-gate attempts failed. No third full gate has started.

The retry log is raw/native-full-gate-retry.log. Its producer built/staged in
36m27s, but crate-check failed 107 findings and source-load failed at the
canonical authored-default quotation guard. Required downstream Rust gates,
validation, reasoning and fixture acceptance did not qualify. The first attempt
also failed and its 43m35s producer refused a changed-input receipt.

The 107 retry findings comprised 97 repo-static findings (54 string-error sites,
5 kind/failure-class bindings and the extracted-test classification findings)
plus 10 browser attestations still stamped purrdf 2.0.0 against lock 3.0.0.
The current source-policy run now passes live_repo_static_passes: all 97 repo-static findings are cleared. The later 19 genuine diagnostic findings were repaired without suppressions or ceiling raises. Browser attestations have not yet been fully rebuilt/rechecked.
The real browser assets must be rebuilt/re-attested, never stamp-edited.

## Current evidence by requirement

| Requirement | Evidence and scope | Current state |
| --- | --- | --- |
| Restricted-blocking source exactness, distinct guards, anyURI facets/enumeration | Prior settled focused selection: 152 passed, raw logs retained in this Stage | Focused pass; full production verdict open |
| Native rule RDF 1.2 encoding and exact carriers/scopes/literals | Native compiler/consumer checks previously covered 952 selected cases as 951 plus corrected SSSOM case | Focused only; uncommitted native migration |
| Canonical authored-default guard and production GTS round trip | raw/authored-native-green-v2.log; nextest 1b981437-983b-4041-88a9-efad97234113: 2 passed, 0.100s | PASS for explicit synthetic default/named graph rule and ordinary-quote refusal |
| Actual native artifact fixture repair | Committed/pushed 1782 repair integrated as 13a44f7a9; typed absent/mismatched controls pass | Exact authenticated corpus consumer acceptance still open |
| Math numeric union/rank ranges | Actual emitted validation-shape consumer controls previously passed | Uncommitted; CodeRabbit math thread remains open until published/qualified |
| List/goal-directed/translation corpus operations outside tests | New producer observations and inert scanner controls; raw/corpus-observation-focused-v3.log, nextest e77e4757-aaa3-4e28-adc6-71c2a5ef9f9a: 18 passed | Synthetic controls PASS; actual producer observations unqualified |
| Shape-oracle positive/negative capture and list admission | raw/shape-oracle-final.log: 7 passed | PASS for affected behavior |
| Shared fixed-hash/list helper homes | raw/corpus-observation-helper-hygiene-v2.log | Prior dirty-source pass; ownership-query/diagnostic integration changes require recheck |
| Recovered external-evidence/numeric diagnostic integration | Exact ce1c916a7 patch applied; raw/diag-migration-focused-current.log, nextest d141520e-75bd-4720-8aa0-4242f5cf8f57: 27 passed, 0.124s | Focused PASS; newer helper/termination-analysis repairs also pass their controls and live static seal |
| Prover CLI failure-path checks | raw/diag-cli-focused-current.log: build refused absent mandatory producer bundle | FAILED admission; controls unrun; no substituted bundle |
| First-party module/cfg ownership query | make source-identity-test: 1 unit + 6 extraction + 32 inventory tests passed before registry capability addition | Earlier focused pass; PASS after registry addition: 1 unit + 6 extraction + 33 inventory cases (40 total), raw/source-policy-resolver-registry.log |
| Production seals and test-like production filenames/shared ownership | raw/source-policy-seals-v2.log: 6 pass, 4 fail; missing synthetic profile setup and unresolved inventory registry capability were repaired | Both required live seals and snapshot-publication seal PASSED in raw/source-policy-soleness-settled.log; subsequent shared-context recheck running |
| Synthetic slicetest soleness | Replaced real-module projection/whole-shape loader with two explicit synthetic laws, both same component code; retains positive, pinned and unpinned teeth | PASS: all 40 cases, including stale/invalid/deleted file cache control, in raw/source-policy-soleness-settled.log |
| Corpus-producer prohibition | raw/source-policy-corpus-purity.log PASS after truthful adjacent markers on inert source-string fixtures; current focused target also passed purity before starting | No corpus build admitted; affected source check remains required after later changes |
| Lint/format | raw/diag-native-lint.log PASS before ownership/soleness changes; normal make fmt applied afterward | PASS in raw/integrated-source-proof-lint.log before the later diagnostic factory edit; no terminal result was captured for the integrated proof and helper runs |
| Independent completion review and full local qualification | No qualifying current source result | OPEN |
| Hosted qualification/cold comparison | Published b797 generation failed stage-docs-render with unsupported verdict and 16394 obstructions | Historical failure; current generation unrun |
| ghprsq integration | No qualifying full gate/production verdict/review | NOT READY |

## Checks and next actions

- raw/integrated-source-proof-focused.log had no terminal result. The replacement raw/typed-admission-and-proof-current.log ran 345 tests: 344 passed, including the complete live static seal, with one stale extracted-test caller-file assertion. The corrected errors package then passed all 120 tests in raw/diag-caller-and-factory-settled.log. Earlier raw/source-policy-soleness-focused.log covers the prototype affected
  production seal controls. Previous live refusal was the missing capability for
  the existing qualified inventory::collect! type-registration macro. The new
  capability parses its type, scans nested type macros, and has bare/foreign/missing
  include negative controls. No file/name exception was added.
- raw/source-policy-resolver-registry.log: source-identity tests after that capability.
- The later typed ownership-error adapter retains the original InputError/TOML
  cause under the existing repo-static policy code, instead of mislabeling a
  semantic refusal as I/O. Its new negative control passed in raw/source-policy-soleness-settled.log.
- Diagnostic/binding findings are cleared by the current live static result. Resolve browser-asset findings and the remaining Clippy source failures; settle all known source blockers
  before the required full-gate retry. No waived acceptance or weakened census.
- The shared ownership query now resolves once per complete check invocation
  and is passed to every production seal, without a cross-invocation cache. Keep unmanaged temporary-directory scanning over ALL code.
- 1792 performance evidence remains in its original Stage/worktree. Its 1250/2500
  cases measured roughly 17x/35x improvements including retained-proof validation;
  the 5000 series completed too (295.94s baseline, 1.6917s optimized). The exact lookup patch is now integrated here and its evidence/class controls
  passed in the replacement selection. The timing series does not establish full-pipeline speedup.

The old 1110-case run included a forbidden whole-module compiler test. Its pass
is INVALID corpus acceptance. The prototype owned-source queries and tiny
synthetic soleness tests never compile the real ontology. Missing selectors
continue to fail closed; no test or build hook produced a corpus.

## Plans and publication

Current plans: remediation-plan.md, static-census-plan.md,
synthetic-soleness-plan.md. Public updates: native boundary 6203158640,
diagnostic integration 6203118780, static ownership plan 6203178715,
synthetic soleness plan 6203444131. The complete published PR review surface was
captured with every pagination hasNextPage=false. Only the math range thread was
open at that capture; its repair remains unpublished.

Earlier detailed snapshots are preserved in
validation-history-before-source-policy.md and the raw logs. They do not compete
with this index's current status. Stage evidence stays outside source and must
be selected explicitly for ghprsq audit archival.

## Continuation evidence

The latest continuation cannot resume handles 95506, 23435 and 81680: write_stdin reports Unknown process id. The proof/helper logs have no terminal results, so those checks remain UNVERIFIED rather than passed or still running. The lint log ends with every hook Passed; no later source is qualified by it. Do not probe processes or the host to reinterpret these missing handles. Re-run affected checks only after the current coherent diagnostic fixes settle.

Inspection found a separate information-flow bug in Diag::of_kind: it read code/grade/message but dropped DiagKind::failure_class. The factory now copies that authored binding into the live diagnostic. The end-to-end kind/ledger/JSON/Finding control passed, and an unbound kind keeps None. All 120 errors tests passed after correcting the extracted-test caller-file assertion. This change is uncommitted; full production acceptance remains open.

## Typed admission continuation

Implemented all 15 helper-census string-return repairs using the existing shared Diag carrier. The internal TOML helpers retain the original structured parser error and actual line. Ledger admission preserves that cause and regex causes. Request/schema/home/inventory failures use one registered, graded admission kind; actual I/O retains the original std::io::Error with path context. The shared diagnostic-kind macro has an explicit source expression so original causes remain traversable. No suppressions or error-string aliases were added. The existing first-party errors crate is a new direct dependency of this tool; only that edge changed in its root-lock entry.

The helper operational input contract and refusal class are authored in canonical logic source with complete paired French/Chinese annotation coats. The live binding inventory includes the new code. Native coverage/class admission, snapshot refusal bindings and typed inconclusive termination-analysis observations remain part of this candidate.

The first compile caught five self-test callers still expecting String/PartialEq errors. They now render only at the report boundary and pattern-match successful home resolution. That failed attempt is raw/helper-diag-focused.log, not a pass.

Current checks:
- make fmt passed: raw/helper-diag-fmt-v2.log.
- make lint passed every hook with terminal exit 0: raw/typed-admission-lint.log.
- The explicit helper/errors/joint-input/native-proof/contextual/snapshot/repo-static selection finished: nextest 46d0b3ba-352b-4cfb-859b-e9c6d8cc9eaa, 345 run / 344 pass / 1 stale caller-file assertion failure. The errors rerun passed 120/120, nextest 42df8131-5acb-4105-af55-19cc41eac838. No production-source fix was needed for that stale assertion.
- The initial helper-hygiene run refused stale debt after measured fixed-hasher copies fell 704 to 702. The debt was lowered with the prescribed shrink-only writer. The replacement make helpers-hygiene passed with terminal exit 0, raw/helper-hygiene-lowered-current.log. No ceiling was raised.
- Four codebook-independent browser engines are rebuilding through their normal Make targets: raw/browser-independent-rebuild.log, handle 6462. Packages only; no parity or substrate readiness claim.

The generated native codebook and bundle are absent. GMN/validator builds require verify-wasm-codebook, which consumes the exact authenticated stage-mappings selector and refuses absence or mismatch; verification produces nothing. Resolve this bootstrap ordering through the explicit authenticated producer. No placeholder, stamp edit, substituted bundle or test reconstruction is admitted. No third full gate has started.

## Clippy source repairs and pending builds

The source-only clippy-prebuild target uses the same producer-independent workspace selection as rust-prebuild. It retains -D warnings and uses --keep-going to collect independent failures without changing failure status. The normal full-workspace clippy gate is unchanged and still mandatory.

Repaired six build-input nested conditional/match warnings, the source-body byte-slice warning, duplicate must_use on Iterator returns, an unnecessary boolean branch, borrowed-string and slice-clone test warnings, and one controller nested conditional. The existing source-identity suite passed all 40 cases after the build-input edits: raw/source-identity-after-clippy.log, terminal exit 0.

Boxed the dominant ReceiptDocument and mapping-pattern Item payloads instead of suppressing large_enum_variant. Receipt decoding returns the same owned receipt and JSON format; mapping flattening still returns cloned Atom values. Parser construction, guard borrowing and all discovered test constructors use the new ownership layout. Affected receipt/controller/compiler/projection tests are selected in raw/clippy-structural-consumers.log, handle 21542; they are waiting for the current build directory lock, not passed.

The current Clippy inventory is raw/source-clippy-independent-inventory.log, handle 28472. It has no terminal result yet. The prior three diagnostic runs failed on successively exposed build-input, receipt-layout and controller warnings and are historical failed attempts. Source edits addressed those exact sites before this current inventory.

Four browser package builds remain in raw/browser-independent-rebuild.log, handle 6462; no terminal result. The latest all-source lint is raw/after-clippy-source-lint.log. Published status: issue 1757 comment 6204314279 and PR 1802 comment 6204316045, with every pending/full-acceptance boundary explicit. No new commit, push or merge occurred.

## Native layout and frontier repairs

The compiler/projection/receipt/controller selection passed all 417 cases in 0.701s, nextest 0037a5ef-f43b-402f-969d-8d998cd52d6f, raw/clippy-structural-consumers.log, terminal exit 0. This covers the first layout repair batch; it does not qualify the subsequent native logic edits. The second all-source lint also passed with terminal exit 0 before that native batch.

The broader Clippy inventory then exposed 25 logic findings. Native evidence/outcome and closure enums now box only their dominant payloads, keeping serde wire values unchanged in design; actual native wire/validation controls remain required. Conditional/match, contains, byte/slice and borrow findings were repaired without allowances. Artifact checker work counts use a named units/evaluations/budget record. Context request and attributed-claim indexes have named types.

Removed the redundant 11-argument from_schedule adapter; both callers now use the existing with_layout implementation directly. The relational constructor moves its freshly prepared layout rather than cloning it. Source-owned template rules travel with their existing immutable-definition contract. Family rounds and final observations take explicit borrowed evidence contexts, preserving the exact snapshot, committed rows, registry, completion frontier, observed reads and terminal; no selection or budget changed. Property witness authority carries its existing admission basis.

The first native compile caught seven tests coupled to the old private builder and one now-unused mut binding. The tests now call the existing with_sources entry point, which selects exactly the same Forward operation. The mut was removed. No behavior assertion or failure control was removed. The failed compile logs are raw/native-clippy-after-inventory.log and raw/native-layout-and-frontier-consumers.log; neither is a pass.

Current Clippy is raw/native-clippy-callers-settled.log, handle 26061, queued behind the preceding compile. Native consumers must be rerun after that check settles; the prior handle 98884 still needs its terminal result captured. No full producer is running and no third full gate has begun. Query/reason browser package preparation completed within the four-engine build; MCP packages remain underway. These packages require a current-source freshness check before copying and still carry no parity verdict.

## Latest native consumer verdict and fixture diagnosis

raw/native-frontier-and-pipeline-settled.log finished with 408 run, 405 passed and 3 failed (nextest f3dc44c6-e454-47e6-b2ad-8f4ffff59643, 2.598s). This was not a pass. Its synthetic scheduler, conformance profile/gating, source-owned existential, joint frontier and most native proof consumers passed.

The three failures were traced rather than relaxed:
- The quoted-data fixture placed a quoted triple in subject position, rejected by native RDF 1.2 freeze before any class analysis. It now places the same unasserted Nothing payload in a legal quoted object and keeps the outside-selection and empty-world assertions. That correction passed in the probe selection.
- The identity test expected an obsolete blanket refusal in the presence of an unrelated subclass axiom. Actual shared schema execution completed the identity family with no obstruction; counting.rs explicitly delegates equality consequences to those producers. The replacement requires a nonvacuous Complete identity outcome and Consistent decision, then adds a second equal-value subject plus differentFrom and requires Inconsistent. It does not introduce a broader global consistency claim.
- The existential/min-cardinality test omitted the explicit ObjectProperty marker required by schema.rs minimum_laws for unqualified counts and someValuesFrom. Its actual probe completed with only selected-domain witnesses, not the expected property witness. The positive fixture now declares that operation role and retains every original exact-bound, source-proof, multihead witness and zero-budget assertion.

The temporary computation print was removed. The obsolete private withholds helper became unused and was removed instead of suppressed. The intermediate raw/native-three-contract-fixtures.log failed compilation on that now-removed helper; current three-case replacement is raw/native-three-fixtures-settled.log, handle 41168, not yet terminal. The probe log raw/native-three-failure-probes.log has one pass/two failures and is diagnostic evidence only.

The Clippy cascade also exposed two conformance conditionals, four pipeline source findings, one synthetic carrier slice-clone assertion, one developer CLI borrow and one orphan test doc comment. Those exact findings were repaired. No allowance, cap-lints setting or gate removal was used. The current native-layout-helper-hygiene log completed all checks with terminal exit 0.

## PR consolidation and latest continuation

The live PR audit is recorded in pr-consolidation.md. Six other project PR heads
are already ancestors of PR 1802; PR 1794 retains a distinct five-file patch.
That missing per-world certificate publication and demonstrator enforcement is
now integrated in this uncommitted candidate, together with the valid CodeRabbit
scope-documentation correction. Source review showed that the remaining
existential fixture also expected logic:instanceOf despite the actual Minimum
witness emitter recording rdf:type. Both membership checks now require that exact
production predicate; all source proof, witness identity and zero-budget checks
remain. The previous three-case run ended 2 passed/1 failed, not a pass.

Before the certificate integration, clippy-prebuild passed with terminal exit 0
in raw/source-clippy-post-fixture-cleanup.log, and make lint passed with terminal
exit 0 in raw/settled-source-lint.log. These do not qualify subsequent source
edits. Current focused consumers and Clippy are
raw/world-certificate-integration-consumers.log (handle 19124) and
raw/world-certificate-source-clippy.log (handle 98026). The prior four-case
witness correction run is raw/native-witness-predicate-settled.log (handle 81290).
No source is committed, pushed or merged in this continuation. Browser handle
6462 remains live; no full producer or third full gate has been started.

Latest terminal results supersede those pending statuses:
- The exact three native fixture repairs and minimum-witness control passed
  4/4 in raw/native-witness-predicate-settled.log, nextest
  c7e51c9d-9670-4837-9983-1d4e028f0d66, 0.114s, terminal exit 0. This predates
  the certificate publication integration.
- After that integration, clippy-prebuild passed with warnings denied in
  raw/world-certificate-source-clippy.log, terminal exit 0. make lint also passed
  in raw/world-certificate-source-lint.log, terminal exit 0.
- The four codebook-independent browser package builds finished successfully,
  raw/browser-independent-rebuild.log, terminal exit 0. They are candidates;
  later source edits still require normal freshness builds and all parity gates.
- make producer-build is now running as handle 19663 in
  raw/current-world-certificate-producer-build.log. It is the normal authenticated
  optimized build, not a test/debug substitute. No corpus production or third
  full gate has been started. Do not restart this build while its handle is live.
- The focused source-rule/minimum/published-demonstrator consumers remain live
  as handle 19124, raw/world-certificate-integration-consumers.log. The current
  output shows workspace compilation, not a runtime test verdict.

That focused run subsequently finished with terminal exit 0: 45/45 passed in
10.913s, nextest 9ec37d1e-44af-48b5-abda-66fb100dd1c0. Both the actual
multiworld certificate publication and producer rejection of copied joint classes
passed. The optimized producer build remains live as handle 19663. Browser handle
6462 is terminal and must not be polled or restarted as if still running.

Publication: the candid consolidation update was posted to issue 1790 comment
6204950044 and PR 1794 comment 6204950420 through stagectl. At posting time the
focused runtime result was still pending; the terminal verdict is recorded here.

## Refreshed upstream boundary

The live issue inventory still contains 81 open issues; the broader objective is
not reduced to this integration. raw/open-issues-live.json records the current
number/title/label inventory, and earlier raw/open-issues.json contains the full
body/comment captures. raw/upstream-boundary-issue-current.json refreshes the
complete 1803 body and its two benchmark comments.

The old upstream boundary status has changed: issues 451 and 453 close against
866765b3e88bb6ae5108e387708d29a85c101b17, and 452 closes against
eb1fd88b0283a8a53efea6374243c4e5c2ff3161. Their closure events are captured in
raw/purrdf451-closures.json, purrdf452-closures.json and purrdf453-closures.json.
These commits are strictly ahead of rust-v3.0.1 (783 and 800 commits respectively,
no behind commits). The direct official rust-lang/crates.io-index capture
raw/purrdf-registry-index-current.json lists 3.0.1 as the newest non-yanked entry.
Thus a registry bump from the locked 3.0.0 to 3.0.1 would not supply these fixes.
The direct crates.io API request returned 403 and browser/index access was
unavailable; those failures are not evidence of absence. The official Git index
capture and actual release ancestry establish the publication distinction.

No dependency or fallback has been changed. This is not a new production-world
benchmark or proof that delegation is ready. Fresh upstream performance, exact
source-premise/world/budget/store seams, and actual consumer integration remain
required. Do not keep describing all three upstream defects as still open, or
claim that their closure means the locked GMEOW engine contains them.

The official index check also covers purrdf-entail, purrdf-xsd and
purrdf-datalog; their highest stable, non-yanked version is likewise 3.0.1.
Each complete index response is retained in raw/*-registry-index-current.json.
The updated upstream publication distinction was posted to issue 1803 comment
6205012144. No upstream checkout or release was changed.

## Consumer-proof scope retained

The broader consumer-proof objective remains unfulfilled. Current routing in
gmeow-cli/src/lib.rs dispatches Prove directly to commands::prove. Its
load_theory reads only the caller paths, materializes their CompositeDatasetView,
and runs PreparedLogicSource::new/into_compiled for each invocation. Thus the
current production caller does not establish the complete embedded-canon or
producer-precompiled-base acceptance requirements. This is a source observation,
not a new installed-binary runtime result. The complete refreshed issue 1770
contract is retained in raw/issue1770-current.json.

The historical merged distribution work provides a one-file build/embedding
contract, not proof that the current operation consumes every embedded axiom.
The existing tiny external-prover protocol tests do not cover that requirement.
Current class/context admission still explicitly refuses uncertified units;
neither refusal nor stub-prover success qualifies full-canon proof. The historical
327-blocker count is not a current measured count. All typed semantic disposition,
prepared-base, identity, native request composition and blinded installed-binary
requirements remain in the active goal. Do not close 1768 or 1770 from the
1757/certificate validation alone.
