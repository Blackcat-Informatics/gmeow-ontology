# Acceptance index

## Current ownership/CLI findings

The first live ownership selection FAILED (raw/source-policy-seals-v2.log): six
controls passed, the two new raw-writer fixtures had not installed the existing
owning-profile fixture that arms the seal, and the two live seals refused an
unowned inventory::collect! macro encountered in the existing registration proc
macro. The fixture setup is corrected. The resolver now admits that qualified
registry declaration as a parsed type capability (verified against the locked
inventory 0.3.24 implementation), while continuing to scan nested type macros.
New controls reject bare/foreign collect! and a missing source include inside
the registry type. The combined recheck with the complete slicetest synthetic
inventory is running in raw/source-policy-soleness-focused.log. No live pass is
claimed yet.

The CLI failure-path selection FAILED at build admission because the mandatory
generated/dist/gmeow.gts producer artifact is absent
(raw/diag-cli-focused-current.log). The read-only build script correctly refused;
no test or build hook produced a corpus and no dummy bundle or alternate path
was substituted. The temporary Make scope included the CLI without changing
compiler/profile/features or resource settings. CLI controls remain unverified
until the genuine producer succeeds.

The soleness test repair replaces all real grounding-module projection and
whole-shape loading in that contract with two tiny synthetic SPARQL laws and
explicit temporary data. It retains pinned/unpinned rejection and a correct sole
finding positive control through run_conformance_cell. The now-unused corpus
shape loader is removed from this test file. Plan: synthetic-soleness-plan.md;
issue plan comment 6203444131. This change is uncommitted and awaiting checks.

## Refreshed diagnostic and ownership evidence

The integrated external-evidence/numeric selection PASSED: nextest
d141520e-75bd-4720-8aa0-4242f5cf8f57, 27 passed / 2249 skipped, 0.124s execution,
raw/diag-migration-focused-current.log. Prover CLI checks are not yet refreshed.
The named migrated surfaces now use Diag error positions; successful String
values remain legitimate. The newer helper-census string-error sites and the input-specific termination analysis remain. The latter has nested Result<_, String> outcomes as well as a stored closable Result; it requires evidence-preserving typed propagation, not a scanner workaround.
make lint passed in raw/diag-native-lint.log before the ownership changes below.

The first-party ownership-query prototype passed make source-identity-test,
including the three new source_policy controls (32 inventory tests plus existing
other targets; raw/source-policy-resolver.log and source-policy-resolver-final.log).
The live production seals and their actual writer/generated-fold negative controls
are now running in raw/source-policy-seals-v2.log. Its previous attempt stopped at
corpus purity because literal synthetic fixture strings lacked the existing
adjacent input markers; the inert fixture strings now carry those truthful markers.
No producer ran from those tests and no scanner exemption was added. The full
corpus-purity recheck passed in raw/source-policy-corpus-purity.log.

The query reuses the existing module/cfg resolver, distinguishes shared production
ownership from test-only ownership, and refuses ambiguous path/cfg_attr branches.
Cargo roots include declared lib/bin/build controllers and conventional auto bins.
Unmanaged-temp-dir scanning is unchanged and still sees all test bodies. The
composed-cfg blanker now retains any(test, feature=...) production branches.
No ownership/live-seal pass or full qualification is claimed yet.

## Native authored-default repair: focused pass

raw/authored-native-green-v2.log: nextest
1b981437-983b-4041-88a9-efad97234113, 2 passed / 1031 skipped, 0.100s execution.
The actual canonical-authored boundary now preserves native reifier bindings,
then the mandated GTS profile emits/imports both default and named logic graphs
and the consumer recovers the exact typed rule (dynamic predicate, directional
literal, negation, distinct guard and scope). The negative ordinary-quad quote
still fails closed with the offending row. Both legacy fixture ingestion helpers
use that same partition-aware guard. The first green attempt failed compilation
because those two test-only callers still named the old helper; repaired normally.
This is narrow synthetic evidence, not the full producer verdict. source-load
v14 and the wider current integration still require qualification.

## Current diagnostic integration

The exact previously committed ce1c916a7 diagnostic repair has been applied to
this combined candidate. It remains uncommitted here. Focused external-evidence
and numeric controls are running in raw/diag-migration-focused-current.log;
legacy branch passes are historical only. The newer helper-census error channels
remain open. Source edits from this integration invalidate broad claims from
older test inventory builds; the native authored-default check remains a narrow
reproducer, not a full qualification.

## Retry findings

The retry has terminated with exit 2. Sync failed in the authored-default fold:
the guard re-flattened valid native rdf:reifies bindings and misclassified their
quoted objects as ordinary base quads. The diagnostic labels its helper stage
stage-gts-sink, but the traced call is the source-load self-description boundary.
An expanded native-rule GTS control reproduces the exact failure in
raw/authored-native-red.log. The repair now refolds canonical RDF into the native
dataset before inspecting only its ordinary quad partition. Unrepresentable
ordinary quotes still fail closed, with their actual row in the diagnostic.
The test also reads the shipped authored-default rule and the existing graph/logic
rule back as exact typed IR, and a negative control preserves the refusal.
Focused recheck is running in raw/authored-native-green.log. source-load's stage
version is bumped to v14. No third full gate has been started: the existing
static/asset/diagnostic prerequisites and this repair must settle first.

The 107 crate/static findings comprise 97 repo-static diagnostics (including
54 String-error findings and 5 diagnostic binding findings) plus 10 browser
capability attestations bound to obsolete purrdf 2.0.0 rather than the actual
3.0.0 lock. Browser assets must be rebuilt and genuinely re-attested; no stamp
edit or comparison weakening is an acceptable remedy.

The producer compiled successfully in the required pipeline profile in 36m27s
and staged dist/bin/gmeow-dev. The changed-input refusal from the first attempt
did not recur. Full retry is nevertheless FAILED: crate-check reports 107
static findings (raw/current-static-violations.txt), including extracted test
files counted as production and remaining first-party String error channels.
The test-census defects have existing owners 1761/1781; the diagnostic migration
has owner 1762. No filename suppression or new GTS producer declaration for a
synthetic test is an acceptable repair. i18n-lint passed. The sync/pipeline
operation is still running and its final result is unknown.

## Full qualification retry

The repaired candidate is now running the mandatory fresh full gate:
make check CHECK_ARGS='--timings-json .stage/perf-logic-finish-merged-pipeline/raw/native-full-gate-retry-timings.json'.
Log: raw/native-full-gate-retry.log. The first full run remains FAILED; this is
the explicitly reported required retry after its findings were repaired, not
a waived gate or an inferred pass. Source edits are settled for this attempt.
The three affected producer stage versions were bumped for the changed native
encoding and observation artifact contracts.

Settled focused controls: raw/corpus-observation-focused-v3.log, nextest
e77e4757-aaa3-4e28-adc6-71c2a5ef9f9a, 18 passed / 3459 skipped,
0.055 seconds execution. The new positive/negative list observations and PO
catalog observation control passed; synthetic goal-directed behavior passed.
Corpus-purity, helper ownership and final lint all passed. Actual corpus
observations and the real production verdict remain unqualified until this
full run succeeds. No new commit, push, merge or issue closure is claimed.

## Current qualification status

UNQUALIFIED. The first combined full gate failed helper ownership; its producer
compiled for 43m35s but refused a receipt because source inputs changed during
the build. Both commands have terminated. A fresh full gate is required after
the repairs below settle; no second full gate has started.

Helper ownership subsequently passed (raw/shared-home-hygiene-final.log).
The shared-home selection executed 1110 cases: 1109 passed and one shape-oracle
negative fixture failed (raw/shared-home-focused-v3.log). That execution also
ran an existing forbidden whole-module compiler test: its pass is INVALID corpus
acceptance. The fixture failure was a false negative: $this is focus-bound by
SHACL. The corrected positive control and genuinely unscoped negative control,
plus malformed-list admission, all passed (raw/shape-oracle-final.log, 7 cases).

The corpus-purity scanner missed mutation through recursive output vectors.
Its new inert renamed-helper regression rejects that path and accepts explicit
temporary synthetic inputs. The stronger scan exposed the whole-module list
compiler, authored goal-directed compiler/evaluator tests, and all-catalog PO
parsing/coverage. Those operations now execute only in explicit producer stages;
tests hydrate exact authenticated observations and retain their assertions.
The complete source-purity gate passes (raw/list-corpus-purity-v2.log).
New synthetic observation controls and affected goal-directed controls are
RUNNING in raw/corpus-observation-focused.log; lint and helper hygiene are
RUNNING in raw/corpus-observation-lint.log and
raw/corpus-observation-helper-hygiene.log. These changes remain uncommitted.

Earlier entries below are historical evidence, not a claim of current readiness.

Current corpus-purity recheck passes (raw/corpus-purity-final.log), including
mutable reborrows into another same-file helper as well as recursion. Current
helper ownership passes (raw/corpus-observation-helper-hygiene-v2.log). Moving
coverage parsing into the producer exposed one existing duplicate boxed-error
shim; it was removed in favor of native typed Diag errors. The generated debt
diff was inspected in full: the old five-member shim group becomes its existing
four-member remainder, and fixed-hasher copies fall from 705 to 704. No ceiling,
variant, checker exception or dependency version was added.

Focused compilation caught an obsolete RdfQuad import and the moved helper's
boxed-error conversion assumptions. Both are repaired. The current run is
raw/corpus-observation-focused-v3.log. Earlier focused attempts failed and do
not qualify these changes. Final lint including new files is running in
raw/corpus-observation-lint-final.log.

Fresh published-head CI evidence: raw/hosted-b797-generation-b-failed.log.
Generation (b) reached stage-docs-render and failed with valid/unsupported
reasoning (16394 retained obstructions). The run is for b797a315e, not the current
dirty candidate. The complete review surface was refreshed, with all review,
thread and comment connections reporting hasNextPage=false. Only the canonical
numeric-range thread remains unresolved; its current-source repair is not yet
committed or qualified. No current-source completion claim is inferred from
the three resolved threads.

Current work: restricted-chase blocking soundness and current PR #1802 review
remediation. Plan: remediation-plan.md. Comparison base: origin/main.

| Criterion | Current evidence | State |
| --- | --- | --- |
| Source exactness and vocabulary identity in restricted blocking | Settled focused nextest: 152 passed; formerly false data-alias certificate rejected, valid grounded inverses retained | Focused pass; independent/full proof review open |
| Distinct head guards in binary blocking | Two-witness inverse cycle refused by binary and statement certifiers; finite execution allowance exhausts | Focused pass |
| Valid single-witness inverse certificates retained | Existing binary and native-family positive RJA examples passed | Focused pass |
| Overflow-safe finite write bound | Saturating products implemented | Inspection only |
| anyURI facets and enumeration | Datatype-plan tests passed for length/min/max and collapsed enumeration equality, preserving unknown foreign/nonmatching values | Focused pass |
| CI lint defects | Helper EOF and three source process comments repaired; make lint exit 0, raw/lint-local.log | Passed |
| Production reasoning verdict | Last hosted fixture log ends unsupported in stage-docs-render; current combined source has not run production | Prior failure; current open |
| Native artifact regression | Actual native fixture repair integrated as 13a44f7a9; missing/mismatched evidence rejected; typed artifact checks pass | Focused pass; two authenticated verifier cases open |
| Numeric math ranges and compiler projection | Current uncommitted union/rank repair passes actual native SHACL consumer controls | Focused pass; source qualification/commit open |
| Full local gate and fixtures | No qualifying current execution; native rule-projection defect is in active remediation | Open |
| Two cold generations and exact comparison | Not established by current CI | Open |
| Independent implementation/completion review | Collaboration APIs unavailable; no independent audit claimed | Open |
| Merge | Required checks and semantic acceptance incomplete | Not ready |

`make fmt` passed after the current Rust edits. The earlier development check was
`make nextest-synthetic NEXTEST_FILTER='package(gmeow-logic) & (test(physical::chase::tests) | test(physical::seminaive::joint) | test(any_uri))'`.
Nextest run fd9ab4ba-45b1-422a-8c50-6e2ed7f8eb7c: 124 passed, 2132 skipped,
2.502 seconds execution after a 4m10s test-profile build. Exit 0. This consumes no
corpus fixture and is not the shipping gate. Cargo also reported manifest-level
unused dependency warnings for csv and flate2; no suppression was added.

The first compilation caught an unused import and a now-unused atom-normalization
helper introduced by the ordinary-rule refactor. Both were removed before the
passing execution above. The first lint caught generated Stage process documents
as authored project content. Repository policy explicitly places local tool state
in .git/info/exclude; /.stage/ is now locally excluded, remains wholly untracked,
and all evidence is preserved here for explicit ghprsq audit archival. No tracked
source exclusion, hook change, or lint suppression was added. stagectl's default
stage_tracked=true does not override this repository's prohibition on shipping
tracker/process provenance. Source cleanup and Stage archival are distinct.

The former September test sessions are no longer registered. Earlier unit passes,
producer builds and interrupted runs do not qualify this new code or the current
combined production verdict.

## Publication

Normal commit 22193fc44 completed and was pushed successfully to the existing
issue-1757-production-verdict branch (PR #1802). The PR description now distinguishes
the current focused checks from historical production observations. Review updates
were published to the PR and issue through the Stage workflow, with a fresh bot
review requested. No unresolved thread was marked resolved without reassessment.

- PR update: https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1802#issuecomment-6198648046
- Issue update: https://github.com/Blackcat-Informatics/gmeow-ontology/issues/1757#issuecomment-6198648449

After that earlier unit, the source worktree had no intended uncommitted product changes;
Stage evidence remains local and must be included in the explicit ghprsq archive
before any future cleanup. No full-gate or merge-readiness claim is made.

## Settled development pass

Normal commits 13a44f7a9 (native artifact fixtures), d3ca06032 (exact restricted
premises), 551dee071 (anyURI equality), and 1d5d7f36d (unused dependencies/locks)
were pushed successfully to the existing PR branch. No hook was bypassed.

The counterexample in raw/vocabulary-counterexample.log exhausted its explicit
40-step allowance but received RestrictedJointlyAcyclic before the repair. Its
inverse requires a distinct ordinary data URI that is never produced; the value
quotient fabricated that join. Structural value flow remains conservative, while
blocking now reads raw data constants and only canonicalizes fixed operators.
Marker-alternative body facts are not assumed to have one chosen exact spelling.
Relaxed producer heads and unknown/zero count summaries cannot seed a blocking
proof; the variable-count regression executes the selected count-1 expansion.

Final combined focused command and full output are in raw/combined-focused-final.log.
It selects chase, joint admission, any_uri, all corpus-free artifact cases,
qualified-range frontend cases, and the actual generated numeric-range validator.
Nextest d240e531-3bac-4fe6-b5bb-3141da4549f5: exit 0, 152 passed, 3801 skipped,
1.811 seconds execution. make fmt and make lint passed; raw/settled-lint.log.
Both real artifact-verifier cases remain present and require the authenticated
producer-selected law fixture. They have not passed yet.

The unused csv/flate2 findings were verified against the implementation and the
existing loader removal. Normal Cargo resolution pruned the two direct edges and
regenerated the existing fuzz lock, including its already-authored tracing and
term-hash dependencies. PurRDF selections and all compiler settings are unchanged.
The first concurrent lint observed Cargo.lock changing during dependency resolution;
the settled lint passed after lock generation completed. No shell policy changed.

Canonical math ranges and their compiler/consumer regressions remain uncommitted
pending the required full qualification. Their focused checks are real, but they
are not published as a completed ontology change. Native rule projection is in
its isolated 1805 worktree; its plan and issue update are published. The entire
goal, production gate, fresh reviews, cold generation proof and merge remain open.

## Native rule migration in progress

Normal commit b797a315e was pushed successfully. It removes the reviewed horizontal
call punctuation and pins the actual native closure mismatch diagnostic. Its two
affected controls passed (raw/test-review-refinements.log, nextest
6aa7c004-ecab-4bb4-bd52-59c584e5e0d4). No fresh review has yet assessed this head.

The uncommitted native rule migration was applied from the isolated 1805 worktree
without overwriting the numeric-range work. It migrates source rule atoms and the
writer/reader to native reifiers, keeping guards as ordered variable arguments.
RDF term identity differs from procedural SPARQL value inequality; the SHACL-AF
guard now uses !sameTerm. Five directly affected structural definitions and their
existing French/Chinese translations move together. A production-profile tiny GTS
round-trip control is added. Focused compiler/consumer tests are running in the
isolated tree; raw/native-migration-lint.log is the combined hygiene run. Neither
run nor the uncommitted source is currently a completed qualification.

The expanded isolated run subsequently executed 967 cases: 945 passed, 22 failed.
The native rule round trip and the actual GMEOW-profile GTS round trip passed.
Seven compiler dialect round trips exposed base-only metadata scans; the native
statement view and quoted-term decoding now repair CLIF/CGIF. The other compiler
failures were one mandatory preservation declaration missing from a tiny SSSOM
fixture and the deliberate !sameTerm snapshot change, both corrected without
weakening production checks. Five gate fixture failures now convert tiny authored
Turtle to the production reader's native N-Quads. Eight selected corpus consumers
lacked their authenticated manifest and remain unqualified until explicit
production/full gate. The latest isolated pass is migrated-consumers-v5.log.

Atom-local scopes now use the same writer as rules/scoped axioms, and their reader
keeps source-owned rule components out of standalone assertion extraction. The
round-trip control covers all six dimensions. The combined source includes these
repairs and retains the earlier numeric ranges. make producer-build is running
normally with its standard recipe in raw/native-producer-build.log; no new profile
or override was introduced. raw/native-migration-settled-lint.log is the new lint.
Canonical changes, full gate, external completion audit and merges remain open.

Latest expanded run: migrated-consumers-v5.log, 952 executed / 951 passed / 1 failed,
0.563 seconds execution. Every changed dialect, native scoped rule, GTS carriage,
gate and golden control passed. The sole failure is the SSSOM fixture's native
literal expectation: it authors datatype None but expected Some(xsd:string). The
native IR correctly retains the original field, so the two exact assertions now
expect the authored simple literal; effective-datatype cell checks remain intact.
The isolated focused recheck is alignment-fixture-final.log, pending.

The single full qualification is now RUNNING on the combined source:
make check CHECK_ARGS='--timings-json .stage/perf-logic-finish-merged-pipeline/raw/native-full-gate-timings.json'.
Output: raw/native-full-gate.log. Existing standard make producer-build continues
in raw/native-producer-build.log; the gate uses the same authenticated producer
through its ordinary builder election. No separate corpus regeneration was run.
The full run has not passed. Canonical changes remain uncommitted until it does.

## Full-gate finding: native helper ownership

The full gate reached helpers-hygiene and failed: fixed-hasher-everywhere 716
against 705, rdf-collection 27 against 19. Compliance-report was skipped because
its prerequisite failed. The original producer/full gate remains running; changes
made after its start invalidate any current-source qualification claim from it.
No second full gate has been launched.

Remediation uses the existing purrdf_hash::fixed::FixedState in the touched native
value cache, formula memo/admission set, chase marker set and joint input tables.
The source-owned rule-atom inventory uses its existing native TermId BTreeSet.
The IR list walker no longer carries a separate random visited set. The copy
count is 704, and the debt generator's complete diff was inspected: its ONLY
change lowers fixed-hasher-everywhere 705 to 704. No ceiling or group was raised.

Eight counted list copies now delegate to PurRDF: native compiler source walk,
IR cell builder, three RDF projection builders, EDOAL builder, shape-oracle
walk, and the synthetic carrier control. The IR projection walk also delegates
and removes its separate default-hasher set; it was not a counted list hit.
Cell names, member kinds, graph scopes,
and actionable malformed-list diagnostics remain explicit. Native source lists
still read the complete default statement layer. The compiler now directly names
the already-linked purrdf-core workspace package to reach its public collection
algorithms (the facade does not re-export them). Root and fuzz locks add only that
existing edge; package versions/features and compiler settings are unchanged.
The initial census after seven removals showed 20 list copies; moving the final
IR cell builder through the home restored the existing 19 ceiling.

The final isolated fixture recheck passed: alignment-fixture-final.log, nextest
39718902-43d4-4a3d-bc65-2d6cb3c38bec, 1 passed / 933 skipped (0.062 seconds).
Together with migrated-consumers-v5.log this covers all 952 earlier selected
cases, but the subsequent shared-home changes require their own checks.

Current affected execution: raw/shared-home-focused-v3.log. Earlier attempts
caught a missing closure parameter annotation and then an obsolete RDF_NS import;
both are fixed without suppressions. The required helper gate is running in
raw/shared-home-hygiene-final.log. Neither is a reported pass yet. Required full
qualification remains FAILED/UNQUALIFIED; after repairs settle, the mandatory
repository gate conflicts with the single-run workflow budget and its necessary
repeat must be reported explicitly rather than silently waived or looped.
