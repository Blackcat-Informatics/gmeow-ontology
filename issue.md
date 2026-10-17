# Issue #1803: Split gmeow/purrdf responsibilities: generic engines in purrdf, logic: semantics in gmeow

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Rule (agreed)
Split by **generic mechanism versus gmeow semantics**, not strictly by RDF versus beyond-RDF:

- **purrdf owns** the generic, logic-agnostic engines and decision procedures, wherever their theory is general:
  - RDF parsing and serialization, SPARQL and its pushdown;
  - XSD/OWL datatype value spaces;
  - Datalog and existential-chase execution;
  - chase termination certification (WA/JA/SWA/MSA/RJA, Datalog-first);
  - the OWL-Direct DL hypertableau with absorption, proofs and certificates;
  - module extraction.
- **gmeow owns** the `logic:` language, its semantics and its compilation onto those engines:
  - modal and contextual worlds;
  - grounding and correspondence laws;
  - world-scoped refutation ledgers and provenance policy;
  - the slices and the `gmeow.gts` / `gmeow` CLI deliverables.
- **Each overlap gets exactly one owner.** The inferior copy is deleted (greenfield), never kept as a fallback.
- **CLAUDE.md** currently says "the native `logic:` reasoner is the single reasoning authority". It should read: the authority on `logic:` **semantics**, executed on purrdf engines.

## Boundary requirements
The split only pays if crossing it is cheap and lossless:
1. **Proofs bound to source premises,** so gmeow ledgers can cite purrdf derivations. purrdf already has `DlProof`, `ChaseProof` and `ServiceProof`.
2. **World-scoped input:** named graphs as worlds.
3. **Budgets and stop signals** (`StopSignal` exists).
4. **Shared-store or delta seams,** so production does not copy a ~690k-fact store per call. These may need adding upstream.
5. **purrdf release cadence** that keeps up with boundary moves (gmeow pins `purrdf = "3"`).

## Current overlap inventory
| Overlap | gmeow today | purrdf home | Status |
|---|---|---|---|
| OWL DL class-model consistency | `logic/src/reason/refute/casesplit.rs`: partial case-split; refuses defined classes and domain/range, about 21k production obstructions (#1757, PR #1802) | `entail::owl_dl`: SHOIQ(D) hypertableau, absorption, `Reasoner::consistency` with `DlCertificate`/`DlProof` | **Next:** benchmark on the production class-model worlds, then delegate and delete `casesplit` |
| Existential chase and termination ladder | `logic/src/physical/chase.rs` and the joint materializer (WA/JA/SWA/MSA/RJA, Datalog-first, #1796) | `purrdf-datalog` (seminaive, chase, WA only) | #1797 tracks lifting RJA/Datalog-first; the full engine move follows |
| Datatype semantics | `reason/value.rs` named-datatype membership; `property/datatype/extent.rs`; `anyURI` added in #1802 | `purrdf-xsd` (value-space datatypes; lexical-space types are only constants today) | Upstream lexical-space datatype membership, then drop gmeow's layer |
| Class-producer per-round re-evaluation | joint family round re-runs the class model every round | goes away with delegation | Folded into the class-model item |

Further overlaps found along the way get added here, not left implicit.

## Comments (14)

### paudley — 2026-10-06T02:25:16Z

## Benchmark: the class-model worlds on the locked purrdf 3.0.0 hypertableau

This is the "benchmark on the production class-model worlds" step from the overlap table. It goes through gmeow's own `DlReasoner::consistency` seam, using the crates.io `purrdf-entail 3.0.0` that `Cargo.lock` selects. The inputs are the class worlds dumped from the producer after the corpus repairs in #1804. Each run was capped at 60 min and 16 GB.

| World | Answer | Completeness | Boundaries | Time |
|---|---|---|---|---|
| Fixture worlds (36: one example, 35 W3C) | 34 answered (26 true, 8 false; the example clash fixture answers `false`). Checked against each case's expected verdict: 33 match W3C. That includes `webont-description-logic-035`, which the native reasoner records as `incomplete`. `webont-Thing-005` answers `consistent` where W3C says inconsistent: `owl:Thing owl:oneOf (s)` is satisfiable under Direct Semantics with a one-element domain, and only inconsistent under the RDF-Based Semantics that case is written for. That is a semantics divergence, not an engine error | 33 decided; one decided-within-boundaries | `fs2rdf-domain-range-expression-ar`: `data-range`, `unrecognized-term` | at most a few ms each |
| W3C `webont-I5.26-006`, `-007` | **refused** at open: `Malformed: cyclic OWL class expression`. This is an OWL Full-only construct; W3C says consistent | none | none | n/a |
| `graph/imports` | true | decided-within-boundaries | `data-range` | 24 ms |
| `graph/logic` | none | timed out | none reported | over 60 min |
| `graph/rl_default` | none | timed out | none reported | over 60 min |

- The `graph/imports` boundary comes entirely from `rdfs:range xsd:dateTimeStamp`. purrdf 3.0.0 does not decide that datatype exactly, while `xsd:date`, `xsd:dateTime` and `xsd:integer` all decide cleanly. Filed upstream as Blackcat-Informatics/purrdf#451.
- `graph/logic` and `rl_default` do not decide within the producer budget on the released engine.

**Consequence for the delegation row:** delegating the class model to the released hypertableau cannot make the production verdict conclusive yet. Two things block it: the speed of the engine on the two large worlds, and the `dateTimeStamp` boundary. `casesplit` stays in place until a purrdf release decides these worlds. Deleting it now would replace an honest `unsupported` verdict with timeouts.


### paudley — 2026-10-06T03:31:22Z

## Follow-up: ABox summarization, and the source of every boundary

**Structure.** Neither large world has any role assertion between individuals. The individuals carry only types and data:

| World | Typed individuals | Distinct assertion signatures | Couplers |
|---|---|---|---|
| `graph/logic` | 3,047 | 50 | `oneOf` 17, `hasValue` 18, `members` 10 |
| `graph/rl_default` | 16,887 | 698 | `sameAs` 420, `differentFrom` 2, `hasValue` 25, `oneOf` 18 |

**Summarization.** I kept one representative per identical assertion signature. Any individual that a nominal, a coupler or another axiom names was kept verbatim (862 and 3,032 respectively, most of them punned class terms). Without role edges this is sound and complete for consistency. Results on the locked purrdf 3.0.0:

| Summarized world | Individuals | Result |
|---|---|---|
| `graph/imports` | 722 → 5 | true in 2 ms; `data-range` |
| `graph/logic` | 3,047 → 893 | true, but in **53 min**; `data-range`, `property-chain`, `unrecognized-term` |
| `graph/rl_default` | 16,887 → 3,673 | no answer in 60 min |

Summarization does not bring `graph/logic` within budget, so the cost is driven by the TBox and not by the individuals.

**Each boundary, reduced to a one-axiom probe:**
- `data-range`: `rdfs:range xsd:dateTimeStamp`, filed as Blackcat-Informatics/purrdf#451.
- `property-chain`: any `owl:propertyChainAxiom`, of which `graph/logic` has 9. Filed as Blackcat-Informatics/purrdf#452.
- `unrecognized-term`, from two separate sources:
  - `rdfs:range rdf:langString`, filed as Blackcat-Informatics/purrdf#453;
  - `rdf:type rdf:Statement` on 232 rule-atom nodes. That one is a gmeow defect: the `logic:` rule projection uses RDF 1.1 classic reification. Filed as #1805. The RDF 1.2 `rdf:reifies` triple-term form decides with no boundary on the same engine.

**Where delegation stands.** Even with all four boundary causes fixed, the released engine does not decide `graph/logic` or `rl_default` within the producer budget.


### paudley — 2026-10-16T00:13:54Z

Refreshed the upstream delivery state against the current forge and official
registry index:

- PurRDF issues #451 and #453 close against
  `866765b3e88bb6ae5108e387708d29a85c101b17`; #452 closes against
  `eb1fd88b0283a8a53efea6374243c4e5c2ff3161`. These are no longer open upstream
  defects as the earlier benchmark discussion recorded.
- Both closure commits are strictly after `rust-v3.0.1`: 783 and 800 commits ahead,
  respectively, with zero commits behind. The official crates.io Git index still
  lists 3.0.1 as its newest non-yanked `purrdf` entry. GMEOW locks 3.0.0, and a
  registry update to 3.0.1 would not bring in these fixes.
- This is publication evidence, not a new performance or semantic benchmark.
  The old large-world timings are not measurements of the current upstream
  implementation. Exact production-world decisions, source-bound proofs,
  world/budget/stop semantics and store seams still need actual consumer evidence
  before delegation and deletion can qualify.

No dependency, compiler profile, fallback, selected world or verification gate
was changed during this check. The current GMEOW source qualification and normal
optimized producer build continue; no production completion is claimed.


### paudley — 2026-10-16T02:00:42Z

The source-pinned generated-profile bootstrap has now terminated with a real production refusal. Source loading completed in 47.255s, conformance in 359.899s and native reasoning in 1,735.830s (28m55.830s). GTS composition, verification attestation and validation actions completed, but `stage-docs-render` correctly rejected the retained native result: `input=valid`, `evaluation=unsupported`, 16,575 unsupported constructs. This is a failed generation, not a conclusive reasoning certificate.

The production warnings still include class-expression equivalence, nominal set-equality admission, domain/range and blocked completed reads. Their world-specific log counts are not additive and do not replace the final preservation inventory. These timings describe the current GMEOW native path on the locked 3.0.0 substrate; they are not measurements of the latest PurRDF implementation.

The official crates.io Git index was refreshed through the GitHub API and still lists 3.0.1 as the latest stable non-yanked `purrdf`. The previously captured ancestry places the three closed upstream boundary fixes after that release. A version bump to 3.0.1 alone therefore does not establish those capabilities or the required production-world decisions.

The declared ownership split remains in force: benchmark the actual upstream decision procedure on the exact production inputs, preserve source-bound proofs/worlds/budgets/store semantics, then delegate and remove the inferior copy when its acceptance is demonstrated. The unsupported result remains visible. No world selection, verdict guard, dependency source, compiler profile or mandatory gate was weakened.

Separately, the incorporated list-traversal and empty-generative-round review findings now have local runtime repairs and 135/135 passing affected synthetic controls. Those performance repairs do not claim to resolve this native DL capability boundary. Full generation, asset parity, local/hosted qualification, final review and merge remain open.

Principles 4, 7, 17 and 18.


### paudley — 2026-10-16T22:50:27Z

# Native DL evidence boundary

Commit 10c6e175f removes CertifiedAnswer entirely. Every DL service returns the
unchanged PurRDF Certified value instead of calling into_parts and copying only
six observations. The complete certificate, work/search-shape/preparation/storage
observations and any selected ServiceProof now cross the GMEOW boundary intact.
There is no compatibility alias, duplicate certificate or new generic engine.
The actual consistency/classification/realization CLI callers read answer and
certificate directly, preserving their existing grading rules.

Explicit recording and native proof-context construction use the same typed
GMEOW refusal adapter. The boundary control checks a retained proof and full
certificate against a separately prepared consumer context and the exact source,
and rejects a different source. Unrecorded selection retains absence and cannot
fabricate an empty checking context. No-model, malformed and execution categories
and original causes remain distinct. Default execution and dependency identity
7734b013 are unchanged. Principles 4, 6, 7 and 18 apply.

All nine selected service tests passed in 0.056s. Strict producer-independent
workspace/all-target Clippy passed with warnings denied in 7m06s. All normal
signed commit hooks passed. Logs: dl-native-certificate-tests.log,
dl-native-certificate-clippy.log and dl-native-certificate-commit.log. Push output
is in dl-native-certificate-push.log.

Four authenticated consumer packages, including the CLI, are excluded by the
local producer-independent targets; their final type/runtime qualification remains
hosted. No local optimized producer, full gate or machine probe ran. This is a
prerequisite for the owner split in 1803 and the full 1770 plan, not completed
class-engine delegation, a prepared native canon, zero admission blockers, an
installed full-canon proof, or measured pipeline speedup.


### paudley — 2026-10-16T23:52:09Z

# Remove the obsolete decimal retry

Actual source inspection found that LiteralValue::from_parts retries a rejected xsd:decimal through rational(), which accepts slash syntax. Consequently malformed decimal lexical forms such as 1/2 acquire the value of a valid owl:rational. This is a semantic admission defect, not a performance-only cleanup. The comment justifying the retry assumes a fixed decimal scale, but the pinned PurRDF value parser already returns its arbitrary-scale exact Decimal.

Delete the retry so PurRDF remains the single decimal lexical authority. Retain rejected native literals as opaque observations; do not invent numeric membership or equality. Leave the explicitly separate OWL rational reader and unsupported lexical spaces unchanged. No dependency bump is needed: both local and remote PurRDF main are newer, but the exact pinned implementation already supplies this capability. Principles 4, 6, 7, 17 and 18 apply.

Verification exercises the real NativeValues membership/equality path for malformed fractional/exponent/empty-magnitude decimal syntax, a valid scale-80 exact decimal, and the actual datatype-enumeration consumer: a malformed decimal cannot satisfy the rational half enumeration, while valid decimal half is included and quarter excluded. Run the surrounding synthetic native datatype/refutation/program controls and strict producer-independent Clippy, normal commit hooks, then publish the coherent fix in the existing proof branch and PR. Hosted corpus and full-canon qualification remain required; this does not close the whole engine-ownership issue.


### paudley — 2026-10-16T23:59:25Z

# Decimal interpretation keeps one native authority

Removed the obsolete Err(xsd:decimal) retry from the shared native literal interpreter. PurRDF rejects slash syntax for decimal, but GMEOW retried the rejected text through the OWL rational reader and thereby treated 1/2 typed as decimal as a valid half. That could satisfy a rational value enumeration and establish numeric membership/equality without a valid decimal interpretation.

The pinned PurRDF parser already returns arbitrary-scale exact Decimal values, so the fixed-scale justification was stale. The replacement deletes six production lines and retains the normal uninterpreted-literal path on native rejection. No alternate parser, compatibility fallback, runtime dependency or dependency bump is introduced. Exact native values, RDF term identity and unsupported observations remain distinct. The remaining GMEOW OWL rational reader is still used for its explicitly selected domain and was not removed wholesale.

Regressions exercise the real NativeValues path with malformed decimal spellings, a valid scale-80 decimal, and the DatatypePlan enumeration consumer. A malformed decimal cannot become the rational half; valid decimal half remains included and quarter excluded. All 27 selected value/datatype/refutation cases passed in 0.180s; decimal-native-owner-tests.log. The initial filter missed the nested program-level modules, so their actual selection ran separately: all 19 passed in 0.625s; decimal-native-owner-program-tests.log. These are 46 passing focused cases, not a whole-canon qualification. Strict producer-independent workspace/all-target Clippy passed in 2m10; decimal-native-owner-clippy.log. All normal commit hooks and push passed for 9fa28c6fe; decimal-native-owner-commit.log and decimal-native-owner-push.log. Source is clean. Hosted new-source acceptance and updated independent review remain pending.

Source/recipe assessment: the pipeline action key includes the exact producer build fingerprint, including called workspace code, so this logic change invalidates stale products without inventing a second cache or weakening authentication. Reasoning already declares recomputation. No local optimized producer, fingerprint, full gate or machine probe ran. No production speedup or full canon/engine-ownership completion is claimed. Principles 4, 6, 7, 17 and 18 apply.


### paudley — 2026-10-17T03:22:01Z

# One owner for generic decision procedures

The complete contract is issue 1803 and its comments. Preserve all logic issue/PR scope. Principles 4, 7, 12, 17, 18 and 19, the repository goals/baseline, and docs/RUST-OPTIMIZATION.md govern this work. GMEOW owns language, worlds, grounding and source-bound evidence; PurRDF owns generic engines. Replaced copies must be deleted, without a fallback or weakened verdict. The separate stable proof PR 1836 is waiting on CI and review; this worktree keeps that qualification source unchanged.

## Task 1: Measure actual recorded production inputs

Recovered stage-source-load, stage-statements and stage-compile-logic contributions from the existing purrdf-3 action store. All declare the pipeline profile, one identical recorded build, stable-prefix persistent contributions, and the current native cache codec. The code may only deserialize their recorded StageKeyContexts and call the strict read-only native PipelineCache getter. Require exact verified receipt equality, all three unique producers and shared build identity. Never derive a current context/fingerprint, run a DAG/fixture producer, recreate a missing contribution or weaken a codec/hash check. The cache reader already checks the complete restored product. No test may call the consumer or rebuild corpus.

Add one explicit Make-driven example consumer, excluded from Cargo test execution. Reuse the sole production object-EDB assembler, not a copied world reconstruction. List actual nonempty worlds first, then explicitly select the production class-model worlds. Measure native hydration, assembly, preparation, decision and proof replay separately under the unchanged bench profile and locked substrate. Retain native certificate observations, exact input action/receipt/product identities and actual checked proofs. Human certificate Debug output is report-only, never parsed into semantic authority. No process/host probes, environment inspection, caps or compiler/profile changes.

Acceptance: real recorded inputs hydrate through digest and structure checks; missing/edited/wrong-profile/wrong-producer inputs hard-fail with original causes and no writes/recomputation; list reflects the real assembled carrier; selected absent worlds refuse. Native decisions retain completion/boundary/budget/stop observations and their source-checked proofs. A frozen historical input benchmark does not establish current complete-canon qualification. Record the exact source selection and report refusals honestly.

## Task 2: Delegate class reasoning through the native engine

Use measured source/world evidence to choose the real owner path. Preserve every original selected premise and its GMEOW grounding/translation evidence, empty-domain selection, shared-world/store semantics, per-operation budgets and stop signals. Native Certified values remain intact. Add missing generic engine seams upstream rather than implement another generic decision procedure in GMEOW. The current pinned and current upstream public constructors still require RdfDataset; with_stop and with_proofs are separate. Verify the required combined/store seam before claiming readiness. Production unknown/unsupported cannot become a false conclusive answer. Complete class-world qualification and proof controls precede deletion of the inferior casesplit engine and its per-round re-evaluation path.

## Task 3: Complete the remaining overlap inventory

Lift generic existential chase and the full termination ladder/Datalog-first scheduling into purrdf-datalog under the owning work. Consume it and remove the duplicate generic implementation while preserving native GMEOW world/contract/evidence policy. Complete native datatype membership in its PurRDF owner, then remove the GMEOW overlap; the already-landed decimal retry deletion is not the whole item. Maintain the live overlap inventory and clarify architecture documentation: GMEOW owns logic semantics executed on PurRDF engines. Dependency changes must use measured exact upstream identities and preserve gates. No new runtime dependency, shim or silent fallback.

## Task 4: Qualify, review and integrate

Use actual consumer operations and focused meaningful controls. Keep benchmark execution outside tests and source lint/corpus-purity seals intact. Run appropriate source/Clippy/helper gates and normal signed hooks, publish concrete evidence to the owning issue/PR and incorporate current-source reviews where possible. Full hosted production/consumer qualification remains required; no local optimized producer/fingerprint is authorized. Final merges use ghprsq with exact Stage evidence, preserving sibling work and the full remaining goal. Missing independent tooling must be reported, never represented as a passed audit.


### paudley — 2026-10-17T03:53:29Z

# Recorded world consumer: execution evidence

The consumer is committed and pushed as `0327fb993`. Strict affected-example
Clippy passed with warnings denied; normal commit and push hooks passed.
The first bench-profile compilation completed in 25m 39s. This is compilation,
not a native reasoning measurement.

Three existing pipeline-profile contributions sharing one recorded build were
hydrated through the strict native read-only cache. Exact action, receipt,
product and blob identities are retained in the logs and input locator. The
first list operation spent 933ms, 2ms and 3637ms hydrating source-load, statements
and compile-logic respectively, then 2774ms assembling the object-level carrier.
It produced no corpus and wrote nothing to the selected input store.

The actual assembled assertion census is:

| World | Assertions |
| --- | ---: |
| default | 179177 |
| demo/jointly-acyclic | 22 |
| demo/model-summarizing-acyclic | 18 |
| demo/super-weakly-acyclic | 22 |
| examples | 13436 |
| imports | 8099 |
| logic | 52366 |
| relational-core | 377397 |
| statements | 114 |

Named-world IRIs use the `https://blackcatinformatics.ca/gmeow/graph/`
prefix. Named projection used the native graph operation, including its RDF 1.2
statement-layer selection rules.

| Selected world | Prepare | Decision | Proof replay | Native answer | Retained boundaries |
| --- | ---: | ---: | ---: | --- | --- |
| imports | 15ms | 13ms | 11ms | True / DecidedWithinBoundaries | DataRange |
| logic | 259ms | 180ms | 209ms | True / DecidedWithinBoundaries | DataRange, UnrecognizedTerm |

Each decision retained its original native certificate and one proof run, which
replayed successfully against the same dataset and Consistency question.
Boundary-bearing answers are not unrestricted consistency claims and do not
establish GMEOW complete-canon admission. There is no measured speedup ratio
against the old class engine on these exact inputs.

All six actual refusal controls terminated with a failure exit: empty CLI
arguments, missing receipt, wrong producer profile, wrong producer stage,
altered receipt payload and duplicate source selector. The missing receipt
retained its original I/O cause. No control invoked a producer or repaired the
cache. Logs are `control-*.log`; edited selectors are separate Stage files,
never mutations of the input store.

The default operation exposed a defect in this new benchmark: its authored-default
shortcut selected 180062 assertions before complete object-level filtering,
whereas the actual assembled default has 179177. The broader input refused with
the original native role-hierarchy InvalidProperty failure at term 6745. That
refusal is not a result for the actual assembled default world. The shortcut
must be deleted, not retained as a fallback.

Active correction: [PurRDF 566](https://github.com/Blackcat-Informatics/purrdf/issues/566)
replaces the named-only projection with one native default-or-named graph
projection, migrating existing callers without a legacy alias. Its isolated
implementation and graph-isolation controls are in progress. After integration,
the GMEOW benchmark will project every selected world from the full production
assembler and rerun the actual default world. No class-engine deletion or full
engine migration is claimed yet.


### paudley — 2026-10-17T04:23:39Z

The recorded-world benchmark previously selected the authored default before
object-level filtering: 180062 assertions instead of the assembled world's
179177. It now completes the production object-world assembly first, then uses
the native graph projection for every selected world. The broader default
shortcut is removed, and the full EDB is released before native reasoning.

Pin PurRDF to `af8936e6f22c70dcbc3f3dc1224be887aba51cc5`, the paired native
implementation in Blackcat-Informatics/purrdf#567. Migrate all named graph
callers to the unified API without retaining the old method or an alias. Recorded
input metadata is checked before hydration; the read-only native cache remains
the sole product admission authority. Native certificates and question-bound
proof replay remain intact.

Advances #1803. This PR supplies the recorded-input consumer and graph API
integration; the complete class-engine delegation and deletion remain open in
that issue. The upstream PR must land before this dependent PR is merged.

Validation: the corrected consumer type-check passed against the new API; normal
signed commit hooks passed. The earlier authenticated world listing, imports and
logic proof replays and six refusal controls are recorded against the earlier
consumer commit, not claimed as executions of this corrected default path.
Strict producer-independent workspace Clippy and the optimized corrected-default
operation are running on the final pin. Full hosted qualification is pending.
No test or local producer rebuilt a corpus. The benchmark is an explicit
consumer excluded from Cargo test execution. Its recorded historical inputs
do not establish complete admission of today's canonical sources.


### paudley — 2026-10-17T05:13:34Z

# Recorded-input consumer qualification

## Current acceptance update

PurRDF PR 567 merged through ghprsq as ff302c091db9b7aebc497221512549e82a7ab729;
the reviewed af8936e6 source remains the exact dependency pin. Its task worktree
and branch are cleaned up. The source shortcut and retired graph API are deleted.

646bf451f fixes CodeRabbit's listing finding by explicitly flagging zero-count
worlds as unselectable. Focused type-check, normal signed hooks and push passed;
the review reply is published and the thread resolved. Doc comments now explain
the recorded consumer boundaries. No independent completion auditor is available;
owner assessment must not be represented as independent judgment.

Final executable observations: native-final-list.log lists nine real worlds,
default 179177, imports 8099, logic 52366. native-final-imports.log records native
True/DecidedWithinBoundaries with DataRange, one replayed proof and prepare/decide/
replay times 27/36/29ms. native-final-logic.log records the same bounded answer
with DataRange and UnrecognizedTerm, one replayed proof, 152/132/97ms. Selection
took 287ms and 260ms respectively. These concurrent observations establish no
speedup ratio or unrestricted canonical admission.

The corrected default run completed selection of exactly 179177 assertions,
then native preparation REFUSED invalid object property at private term 9336.
native-projection-default.log contains no verdict or proof replay. The earlier
180062-row shortcut refusal is historical, not the actual default result.
Six final-control-*.log operations passed refusal expectations on the current
executable: empty arguments, missing selector (original I/O cause retained),
duplicate producer, wrong profile, unexpected producer and altered receipt.
No operation rebuilt corpus, derived a current fingerprint, or repaired cache.

Hosted run 39002241472 passed producer-build, rust-prebuild, lint and helpers,
but generation (both lanes) and fixture-prefix correctly refused a missed fuzz
dependency pin. Downstream jobs skipped and quality failed. The fuzz manifest
is corrected; make maint-pin-purrdf changed only its 31 native source references
and left the production lock unchanged. A TOML comparison passed for all 31
PurRDF package identities across both locks. Full hosted repair qualification
remains pending. See fuzz-pin-resolution.log and fuzz-pin-commit.log.

The full logic goal and issue 1803 remain open, including full class delegation
and engine deletion. The historical input store is preserved. Its authenticated
products do not establish that current sources have completed the production
pipeline. Native proof and certificate values remain authoritative in memory;
JSON Debug observations are never reconstructed into a proof authority.


### paudley — 2026-10-17T05:17:18Z

# Recorded native world results

Explicit read-only consumers of the same three authenticated historical products.
The implementation is 646bf451f, with the af8936e6 native pin. The later fuzz-only
dependency alignment does not change these executable paths. All worlds use the
complete production object EDB before native graph selection. No corpus producer,
current fingerprint, cache repair, profile change or host probe ran. Principles
4 and 7. Raw results are native-final-*.log; the corrected default operation is
native-projection-default.log. Exact input identities remain in
recorded-input-locator.json and each operation's hydration observations.

World suffixes below use https://blackcatinformatics.ca/gmeow/graph/.
Times are prepare / decide / source-bound proof replay in milliseconds. These
concurrently observed runs are not a controlled speedup comparison.

| World | Assertions | Native result | Boundaries | Times (ms) | Replayed runs |
| --- | ---: | --- | --- | --- | ---: |
| default | 179177 | Preparation refused | Invalid object property, private term 9336 | no decision | 0 |
| imports | 8099 | True / DecidedWithinBoundaries | DataRange | 27 / 36 / 29 | 1 |
| logic | 52366 | True / DecidedWithinBoundaries | DataRange, UnrecognizedTerm | 152 / 132 / 97 | 1 |
| examples | 13436 | True / DecidedWithinBoundaries | DataRange, UnrecognizedTerm | 28 / 6 / 55 | 1 |
| relational-core | 377397 | True / DecidedWithinBoundaries | DataRange | 921 / 654 / 515 | 1 |
| statements | 114 | True / Decided | none | 0 / 0 / 0 | 1 |
| demo/jointly-acyclic | 22 | True / Decided | none | 0 / 0 / 0 | 1 |
| demo/model-summarizing-acyclic | 18 | True / Decided | none | 0 / 0 / 0 | 1 |
| demo/super-weakly-acyclic | 22 | True / Decided | none | 0 / 0 / 0 | 1 |

All eight named selections exited successfully and retained their native
certificates and proof contexts. A bounded result is not unrestricted consistency.
The default refusal is malformed-input admission, not False or an Unknown
decision. None of these OWL Direct operations evaluates GMEOW canonical admission
or authenticates current complete sources. Native delegation must preserve GMEOW
class/source/world/domain semantics before deleting the still-active class
implementation. Dropping unsupported premises is not a repair.


### paudley — 2026-10-17T08:08:59Z

fix(logic): preserve native evidence and portable reasoning identities

Advances #1803, #1761 and #1762. The complete logic goal remains active; this integration does not close the full class-engine delegation issue.

The recorded-input consumer assembles the complete production object EDB before selecting a world through the sole native graph projection. Default, IRI and scoped blank graphs preserve independent base, reifier and annotation slots. The source shortcut and retired named-only API are removed. Production and fuzz locks select the reviewed native implementation merged through PurRDF PR 567.

Source and recorded-consumer diagnostics retain ontology failure classes and original causes through report serialization. New terms carry their required annotations and paired translations. All six browser packages share the pinned build recipe. Browser observations use the required monotonic host clock; native observations retain the standard clock. Semantic identity uses canonical PurRDF rule hashes and calculus version, preserving distinct operation contracts and their actual limits. No execution limits, budgets or optimization contracts changed (Principles 4, 7 and 17).

The incomplete conjecture-only refresher is deleted without an alias. Its replacement verifies the selected seven-fact closure, both conjecture lifecycles and determinism before refreshing both attestations. Tests consume those attestations read-only. The unused historical-codebook restore prototype was removed after the original packet correctly failed current-source admission.

Validation: 41 affected native controls passed in 1.212 seconds, including semantic identity, native parity, failure classes/causes, report projections and numeric execution. All four optimized WebAssembly Node controls passed byte-for-byte against native attestations. Current main compiler changes were synchronized before the final native refresh and browser build. The unchanged query package passed seven Node controls. Helper hygiene and translation lint pass; translation lint reports 427 existing warnings. Normal signed commit hooks and push passed. The final two-line ledger/translation delta changes no qualified Rust/kernel input.

CodeRabbit reviewed 673132cd5 with low merge risk and 88.10 percent doc coverage. Its listing finding is resolved; the sole later minor translation finding is corrected in 9a4d8af8a. The helper census retains the same group and allowance under the replacement refresher name. No merge conflict required discarding either side. No unavailable independent completion audit is represented as passed.

Full hosted CI remains pending, not green. The previous run had killed/canceled producer lanes; the latest helper failure was a stale renamed census member and is repaired locally. The user explicitly requested ghprsq integration now with subsequent CI findings tracked as issues; validation.md retains that instruction verbatim. Full shipped-browser qualification, translation-floor assessment, consumer CLI acceptance and complete-canon acceptance remain open. Historical recorded inputs establish no current-canon verdict. The recorded default selects 179177 assertions then refuses invalid property 9336; no premise was dropped to obtain a verdict.

Authoritative plan: .stage/split-gmeow-purrdf-responsibilities/plan.md. Selected evidence: .stage/split-gmeow-purrdf-responsibilities. Integration is against origin/main 9cb2cb1fc with the compiler change included in the qualified branch.

Defect-Class: https://blackcatinformatics.ca/gmeow/RecordedConsumerFailure


### paudley — 2026-10-17T08:17:35Z

PR 1837 merged through ghprsq as 11c04b49d4fc4d1c594f70b2b7462d128c41f000.
Local affected acceptance passed 41 native controls, four browser controls,
helper hygiene and translation lint; the two review findings are resolved.
Full hosted production acceptance remains pending, not passed.

GitHub automatically closed this broader issue on integration. Reopening it:
the PR deliberately advances the complete contract and does not satisfy full
class-engine delegation/deletion, generic chase/datatype overlap removal, or
complete-canon qualification. Those tasks and the full logic goal remain active.
The native default-world preparation refusal is still explicitly recorded; no
premise or diagnostic boundary was removed to manufacture an answer.

Source and selected Stage evidence are archived separately by ghprsq. This
status is a scope/ownership correction, not a claim of complete issue acceptance.


