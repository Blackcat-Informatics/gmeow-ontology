# Issue #1770: feat: certify and precompile the complete canon for consumer proof

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Problem

Composing the installed `gmeow prove` operation with the complete embedded canon currently makes every request stop before external prover execution. The exact current bundle produces 327 classical-FOF admission blockers:

- 266 `UNSUPPORTED_CLASS_EXPRESSION` (213 uncertified class-expression lowerings; 53 restriction/enumeration reachability cases)
- 51 `CONTEXT_ERASURE_FORBIDDEN` (standpoint/modal translation required)
- 7 `UNSUPPORTED_SELECTED_OWNER` (Contract owners lack a classical execution contract)
- 2 `RULE_TERM_DISTINCT_REQUIRES_BINDING_SEMANTICS`
- 1 `COMPILER_ERROR_IN_SELECTED_THEORY` (the probabilistic profile carries a measure without a declared probability model)

This is the concrete unsupported-grounding set behind the full-canon acceptance criterion in #1768. Silently dropping these units would violate the source-admission contract and cannot be justified by bounded-corpus evidence.

## Scope

- give every selected canonical unit a typed, certified FOF disposition;
- implement the missing class-expression, standpoint/modal, contract-owner, distinct-body, and probabilistic-profile semantics, or give an explicit typed proof-task selection that excludes a unit for a sound documented reason;
- preserve standpoint, provenance, complements, loss evidence, and stage contracts;
- produce the canon FOF base once in the O3/full-LTO producer and carry it in `gmeow.gts`, rather than recompiling the flattened canon on every consumer invocation;
- compose caller premises/goals with that prepared native base in memory, with no serialization/reparse step;
- make `gmeow prove` reach the selected native/external prover against the complete admitted canon from a blinded cwd;
- report the exact bundle and prepared-canon identities.

## Acceptance

- complete-canon admission has zero blockers for the supported proof profile;
- the #1755 navigation obligation works through the canonical carrier and its ordinary-domain-triple form fails with the intended grounding diagnostic;
- focused product tests cover GMEOW proof behavior without duplicating PurRDF or external-prover conformance;
- runtime evidence records elapsed time, peak memory, and intermediate bytes for prepared-canon load plus request composition.

Depends on #1712, #1737, and #1738; blocks the full semantic completion of #1768. Principles 4, 7, 12, 17, 18, and 19 apply.


## Comments (17)

### paudley — 2026-10-16T00:54:10Z

Current source audit for the historical probability-model blocker:

The authored logic module already declares `FullIndependence` as a
`ProbabilityModel`. The compiler's structural admission recognizes a declared
model under both RDF typing and native `logic:instanceOf`; it only consults the
selected default graph. This does not select an independence model for an actual
probabilistic query, which still requires its explicit model declaration.

The combined candidate strengthens the existing compiler regression to cover both
typing surfaces, require the emitted contract, and retain its uncertainty facet.
A new synthetic named-graph control requires the hard contract diagnostic when
the only model is outside the selected graph. Together with the existing absent-
model and native structural-typing controls, the selection passed 4/4. Source lint
and producer-independent strict Clippy also passed.

These test-only changes do not establish a new full-canon blocker count. The
historical 327-unit inventory belongs to its original bundle/selection and must
be remeasured on the authenticated new product; it is not a reason to invent a
model or erase graph context. The complete prepared-canon, request composition,
grounding-diagnostic, native/external proof and blinded consumer requirements
remain open. Current production CLI routing still compiles caller files, so the
one-file distribution evidence alone cannot establish those requirements.

The existing optimized producer build continues for the authenticated bootstrap.
No new PR is being opened, and no full proof or issue completion is claimed.


### paudley — 2026-10-16T16:18:21Z

# Complete canon proof preparation

The full contract is issue #1770, including its comment and dependencies. The
historical blocker count is not a current measurement. Principles 4, 7, 12,
17, 18 and 19 govern implementation. Existing source restrictions retain source
identity and dependency certificates; extend that path rather than create a
second class compiler. The production CLI still compiles caller inputs and does
not establish complete-canon acceptance.

## Task 1: Complete certified source-class laws

Lower canonical Boolean classes, enumeration and restrictions through the
existing formula core. Retain exact selected support, scoped blank identity,
dependency refusal, datatype semantics, context and loss evidence. Begin with
Boolean membership and resource enumeration, then cover remaining constructors
required by the authenticated canon. Synthetic tests exercise actual source
admission and FOF projection, including malformed lists, excluded support,
dependencies, empty/singleton lists and unsupported value/context controls.
Commit and push coherent verified changes; partial constructor coverage does not
complete this task or the issue.

## Task 2: Complete selected nonclass semantics

Inspect the current standpoint/modal, Contract, rule distinct-body and probability
paths. Implement their typed certified execution contracts or a sound explicit
proof-task selection. Test positive production semantics and negative erasure
controls. The existing probability tests establish selected structural typing,
not a current complete-canon blocker count.

## Task 3: Produce and carry the prepared native canon

Compile the complete admitted base once in the authenticated O3/full-LTO producer;
embed it in gmeow.gts with exact bundle/prepared identities and fail-closed loading.
Use the native carrier, without text serialization/reparse. Producer execution
remains subject to the unresolved authority conflict: its current fingerprinting
reads host CPU/environment data forbidden by the user. Do not invoke it locally
or weaken authentication. Current hosted CI can provide existing evidence.

## Task 4: Compose and execute installed consumer requests

Wire gmeow prove to the prepared native base, compose caller premises/goals in
memory, and invoke the chosen native/external solver from a blinded cwd. Exercise
the canonical navigation obligation and its ordinary-domain grounding rejection.
Record load/composition elapsed, intermediate bytes, and permitted peak-memory
evidence; do not add banned host probes. Complete-canon admission must show zero
blockers for the supported profile, with exact identities.

## Task 5: Qualify, review, publish and integrate

Use focused synthetic checks during development and required normal commit/push
hooks. Reserve one full qualification for settled source; report unrun or failed
gates precisely. Open the complete issue PR, incorporate available CodeRabbit
feedback, and merge through ghprsq under standing user authorization. No runtime
dependency, fallback parser or silent exclusion is authorized.

## Current process limitations and retained evidence

No independent-agent API is available in this session. Local implementation and
analysis proceed; no independent review or completion audit is claimed. Required
independent completion judgment remains unverified. Existing prior-art coverage
records two counted repeated-unshared-reasoning-analysis instances and one
consumer-artifact-path-coupling instance; unsearched recurrence remains unknown.
The previous PR's evidence remains in the sibling Stage directory and its remote
audit ref. Reused the current checkout; did not add a new worktree. Five clean
ancestor worktrees/branches were removed; fourteen/fifteen remain respectively.


### paudley — 2026-10-16T16:56:59Z

# Current acceptance

Authoritative plan: plan.md. Full issue remains incomplete.

| Criterion | Evidence / current state |
| --- | --- |
| Boolean/resource-enumeration source laws | Committed and pushed f9dbb7ae7. 62/62 selected TPTP/list/identity controls passed in fof-tests-repaired.log (60 compiler cases and 2 xtask controls). Includes shared owner provenance, nested restriction/nominal, blank identity, singleton arity, invalid empty/malformed lists and refusal controls. |
| Remaining selected class expressions | Not complete; datatype, qualified cardinality and remaining constructors require certified semantics. |
| Modal/context, Contract, distinct-body semantics | Not yet verified complete. |
| Selected probability model typing | Earlier merged synthetic controls, as recorded in issue comment. Not a new canon measurement. |
| Prepared canon in authenticated bundle | Not complete. Local producer fingerprinting conflicts with user host-probing ban; no execution or auth weakening authorized. |
| Installed blinded full-canon proof / negative grounding | Not run. Consumer currently compiles caller input. |
| Load/composition elapsed, memory, intermediate bytes and identities | Not established. |
| Full gate / hosted CI | Merged base CI run 38927612921 failed producer authentication after compilation; lint and rust-prebuild passed. Quality is an aggregate failure; remaining gates skipped. Filed #1825 with exact job evidence and source diagnosis. No local full gate run for this issue. |
| Producer identity repair | Isolated and pushed in PR #1827 on the 1825 worktree, with the related #1826 docs failure fix. Final stable-source identity selection passed 3/3 in producer-identity-final.log. Earlier producer-identity-tests.log executed only the old test during overlapping hook stashing and is not repair coverage. Strict prebuild Clippy passed. The duplicate uncommitted producer patch was removed from this checkout after publication; no unrelated work was changed. Hosted producer remains unqualified. |
| Independent review | Unavailable: no delegation API. No independent verdict claimed. |
| Normal commit/push hooks | Class commit hooks passed, including source purity, formatting and issue-reference policy; class-commit-retry.log. Normal branch push succeeded. |

Initial make fmt refused the stale target symlink after worktree move. Relocated
the existing managed cache to the wrapper-required new worktree path, preserving
its contents; make fmt then completed successfully. No flags, profiles, caps or
host probes were added. Focused tests use the existing nextest-synthetic target,
which removes corpus credentials and does not produce fixtures.

The first commit attempt failed because stagectl open removed the prior local
/.stage/ exclusion. The issue-reference gate documents that untracked ignored
local tool records are outside authored product source and tracked files remain
scanned. Restored that local artifact classification in common Git info/exclude;
no hook, policy scanner, source exclusion, invocation or verification setting was
changed. All Stage evidence stays intact for selected ghprsq audit archival;
the failed attempt remains in class-commit.log. Normal retry passed every hook.


### paudley — 2026-10-16T17:23:21Z

# Partition and local self laws

Extended the existing source-bound class-law path, without a second parser or
runtime dependency. Disjoint unions retain the membership cover plus every
pairwise overlap exclusion. Duplicate source members are a set, so they do not
create self-disjointness. All original list statements remain selected support,
and dependencies use the existing refusal propagation.

Local self restrictions now pass through the shared native restriction compiler
and define membership by p(x,x). Native XSD boolean interpretation admits true/1;
false, untyped, invalid and scoped flags remain refused. This is local class
membership, not a global reflexivity assertion.

The first focused run passed 68/69. Its one failure was an incorrect new positive
test expectation for whitespace-padded true. PurRDF's Boolean lexical parser and
the existing native reasoner both reject that lexical. RDF 1.2 Concepts defines
the Boolean lexical space as true/false/1/0:
https://www.w3.org/TR/rdf12-concepts/#section-Datatypes
Moved the whitespace case to the negative product checks and added uppercase
refusal; no parser, assertion, gate or admitted lexical space was weakened.

The corrected source passed 69/69 compiler TPTP/list/restriction tests; nextest
85045e6f-74cf-4848-bae9-69b4e3017cda, 38.09s build, 0.032s execution. The tests
inspect actual source admission, native compiler retention, exact class bindings,
cover membership and every pairwise constraint, source singleton/set semantics,
and existing malformed/context/dependency controls. Corpus purity passed and no
corpus was produced. Strict producer-independent workspace/all-target Clippy
also passed with -D warnings and its existing four consumer exclusions.

Evidence: class-constructors-tests.log (historical failure),
class-constructors-corrected-tests.log, class-constructors-clippy.log and normal
class-constructors-commit.log. Full authenticated canon, qualified cardinality,
datatypes, contextual owners, prepared bundle carriage and installed consumer
composition remain required by the original issue; no complete blocker count or
full issue acceptance is claimed.


### paudley — 2026-10-16T18:02:21Z


## Qualified object cardinality unit

All 74 selected compiler/list/restriction/FOF tests passed (qualified-cardinality-corrected-tests.log). Strict producer-independent workspace/all-target Clippy passed with warnings denied (qualified-cardinality-clippy.log); four corpus-dependent consumers remain outside that diagnostic target. Normal signed commit hooks passed (qualified-cardinality-commit.log). Qualified bounds preserve selected qualifier dependencies, conjunctive independent counts, native XSD integer values and denotational pairwise inequality. Zero-minimum laws retain dependency validation without an unused ground binding. Malformed/unsupported datatype bounds and unrepresentable counts fail explicitly. Existing unsupported-fixture controls now use datatype restrictions, rather than supported object bounds; their refusal and document-selection assertions remain intact. No full local gate, full canon certification or installed prepared-base acceptance is claimed.

The Boolean, enumeration, disjoint-union and self-restriction commits are already pushed. This cardinality commit advances the same complete-canon task; modal, datatype and prepared bundle/consumer composition requirements remain open.


### paudley — 2026-10-16T18:35:30Z

# Complete owner accounting

Non-rule owners excluded by document/root/graph selection now retain explicit OutsideSelection rows rather than disappearing from the source admission. Selected rejected owners report their extraction failure. Emitted owner IR carrying hard compiler errors also remains malformed instead of being described only as missing classical semantics. Required unsupported semantics still block; no contract catalogue, distinct-body or context execution was silently selected away.

All 59 TPTP product tests passed in owner-inventory-corrected-tests.log. New controls exercise explicit root exclusion, source-document receipts, an actually rejected unknown preset and emitted IR with hard compiler diagnostics. The first run passed 57/58: the new rejected-owner fixture mistakenly used an unknown model value, which this reader emits rather than rejecting. Replaced it with the reader's explicit unknown-preset rejection and added the separate emitted-error control. That historical failed run remains in owner-inventory-tests.log.

Strict producer-independent workspace/all-target Clippy passed with warnings denied in 1m12s (owner-inventory-clippy.log), using the target's existing four corpus-dependent consumer exclusions. All normal signed commit hooks passed for 1414e7498 (owner-inventory-commit.log). These checks do not establish complete-canon acceptance, a new historical blocker count, a prepared bundle, installed proof execution or a full gate.


### paudley — 2026-10-16T18:45:04Z

# Complete canon proof preparation

The full contract is issue #1770, including its comment and dependencies. The
historical blocker count is not a current measurement. Principles 4, 7, 12,
17, 18 and 19 govern implementation. Existing source restrictions retain source
identity and dependency certificates; extend that path rather than create a
second class compiler. The production CLI still compiles caller inputs and does
not establish complete-canon acceptance.

## Task 1: Complete certified source-class laws

Lower canonical Boolean classes, enumeration and restrictions through the
existing formula core. Retain exact selected support, scoped blank identity,
dependency refusal, datatype semantics, context and loss evidence. Begin with
Boolean membership and resource enumeration, then cover remaining constructors
required by the authenticated canon. Synthetic tests exercise actual source
admission and FOF projection, including malformed lists, excluded support,
dependencies, empty/singleton lists and unsupported value/context controls.
Commit and push coherent verified changes; partial constructor coverage does not
complete this task or the issue.

## Task 2: Complete selected nonclass semantics

Inspect the current standpoint/modal, Contract, rule distinct-body and probability
paths. Implement their typed certified execution contracts or a sound explicit
proof-task selection. Test positive production semantics and negative erasure
controls. The existing probability tests establish selected structural typing,
not a current complete-canon blocker count.

ReasoningContract/ReasoningPreset declarations use the compiler's existing
metatheory boundary: extract_contracts explicitly excludes configuration syntax
from domain axioms. The classical proof profile must catalogue these units with
their native configuration, exact selected support facts and a typed reason;
declaring a configuration neither executes it nor asserts its grammar as object
axioms. A catalogue alone must not authorize an empty proof. Retain rejected
owners, hard extraction errors, scope refusals and excluded-support failures.
Use a versioned shared profile identity for this explicit proof-task disposition.
The initial object-fact prototype is not qualifying evidence for this final
boundary and will not be committed. Other selected owner families still require
their real execution semantics or sound explicit task selection.

## Task 3: Produce and carry the prepared native canon

Compile the complete admitted base once in the authenticated O3/full-LTO producer;
embed it in gmeow.gts with exact bundle/prepared identities and fail-closed loading.
Use the native carrier, without text serialization/reparse. Producer execution
remains subject to the unresolved authority conflict: its current fingerprinting
reads host CPU/environment data forbidden by the user. Do not invoke it locally
or weaken authentication. Current hosted CI can provide existing evidence.

## Task 4: Compose and execute installed consumer requests

Wire gmeow prove to the prepared native base, compose caller premises/goals in
memory, and invoke the chosen native/external solver from a blinded cwd. Exercise
the canonical navigation obligation and its ordinary-domain grounding rejection.
Record load/composition elapsed, intermediate bytes, and permitted peak-memory
evidence; do not add banned host probes. Complete-canon admission must show zero
blockers for the supported profile, with exact identities.

## Task 5: Qualify, review, publish and integrate

Use focused synthetic checks during development and required normal commit/push
hooks. Reserve one full qualification for settled source; report unrun or failed
gates precisely. Open the complete issue PR, incorporate available CodeRabbit
feedback, and merge through ghprsq under standing user authorization. No runtime
dependency, fallback parser or silent exclusion is authorized.

## Current process limitations and retained evidence

No independent-agent API is available in this session. Local implementation and
analysis proceed; no independent review or completion audit is claimed. Required
independent completion judgment remains unverified. Existing prior-art coverage
records two counted repeated-unshared-reasoning-analysis instances and one
consumer-artifact-path-coupling instance; unsearched recurrence remains unknown.
The previous PR's evidence remains in the sibling Stage directory and its remote
audit ref. Reused the current checkout; did not add a new worktree. Five clean
ancestor worktrees/branches were removed; fourteen/fifteen remain respectively.


### paudley — 2026-10-16T19:23:27Z

# Reasoning-contract proof-task disposition

Committed a9892dc72. ReasoningContract/ReasoningPreset declarations retain their native configuration and exact selected configuration-support facts in an explicit ConfigurationDeclaration receipt with a reason. The classical object-theory task neither executes every declared discipline nor asserts configuration grammar as domain axioms. This follows the existing is_facet_config_predicate boundary and LOGIC-CONTRACT.md. Profile v2 is shared by compiler and CLI from one exported identity. No selected nonconfiguration owner, context or datatype interpretation is silently removed.

The native declaration is boxed so this uncommon metadata payload does not enlarge every admission row. Its support walk uses native predicate/subject indexes, visits each support role of a shared node once and shares the compiler's configuration-field vocabulary. Referenced closure entries and expanded facet types must have selected support. Scoped declarations and hard compiler errors remain refusals. A catalogue alone cannot authorize a vacuous proof.

The reader now rejects conflicting values for all ten single-valued facets, conflicting closure values for the same key, and multiple keys or values inside one closure entry. Equal repeated values and set-valued facets remain valid. Errors are attached to the native owner and cannot be laundered through partial emitted IR.

Validation:

- contract-final-tests.log: 67/67 TPTP tests passed before the final closure-field guard.
- contract-cardinality-tests.log: 13/13 contract and owner tests passed after that guard, including both multiple-field entry cases.
- contract-native-regressions.log: 9/9 existing native reader tests passed on final source, covering compatibility, missing closure values, unknown presets, selected model typing and no configuration leakage into domain axioms.
- contract-final-clippy.log: strict producer-independent workspace/all-target Clippy passed with warnings denied, 1m32s. Its four corpus-dependent consumer exclusions remain unchanged.
- contract-metatheory-commit.log: all normal signed commit hooks passed. Build-profile flags, dependencies, authentication seals and corpus boundaries were unchanged. The semantic proof-task profile is explicitly versioned as v2.

Historical contract-declarations-tests.log passed 63 tests for the discarded object-fact prototype. It is not acceptance for this feature. The metatheory revision passed 64 tests in contract-metatheory-tests.log before the subsequent reader-conflict controls. Final acceptance uses the scoped evidence above; no unchanged test run is represented as covering later code.

No full local gate, consumer runtime test, complete-canon blocker count or installed prepared-base acceptance is claimed. Datatype and remaining contextual/distinct-body semantics, authenticated native canon preparation, bundle carriage and blinded consumer composition remain required by the full issue. No generic prover/engine implementation was added; this change implements GMEOW's configuration-role boundary.


### paudley — 2026-10-16T19:43:35Z

feat(logic): extend certified source laws and contract admission

Related to #1770; the issue remains open.

The source-admission compiler now lowers Boolean classes, resource enumerations, disjoint partitions, local-self restrictions and qualified object cardinalities into certified source laws. It retains complete owner accounting and represents reasoning-contract configuration as typed metatheory, with explicit conflicting-value failures and a shared versioned proof profile.

Principles 4, 7, 17, 18 and 19: canonical source ownership, explicit support evidence and refusal of unsupported semantics remain intact.

Validation: focused compiler/source-admission tests and strict producer-independent workspace/all-target Clippy passed, as indexed in the selected Stage validation report. Normal commit hooks passed. No full local gate or installed full-canon acceptance is claimed. Hosted CI will qualify this published branch; failures will be tracked visibly under the user's explicit merge-now instruction.

Related to #1770. This PR does not complete or close that issue: remaining datatype/context/distinct-body semantics, authenticated prepared native canon, installed caller composition and full-canon performance/acceptance evidence remain outstanding.

Merge authorization: "Please ghprsq this branch now, we'll create issues for anything that comes up in CI, but we need to get this code pushed and merged."

Integration: clean merge-tree against main; producer identity/document prerequisite changes do not alter this source-admission implementation. No conflicts. Existing evidence retains its recorded scope; no combined full gate is claimed.

Authoritative plan: .stage/certify-and-precompile-the-complete/plan.md. Selected Stage directory: .stage/certify-and-precompile-the-complete. See validation.md and merge-now-decision.md for incomplete original criteria and explicit publication authority. Independent reviewer APIs are unavailable; no independent completion audit is claimed.

Defect-Class: none


### paudley — 2026-10-16T20:10:36Z

# Complete-source admission publication

The producer now inventories its original compiled catalog before OPT/example
augmentation. Every document, formula root, rule and semantic owner is selected;
blocked and malformed dispositions remain explicit. The compile-logic artifact is
mandatory in the snapshot reasoning archive, and both changed stage implementation
identities invalidate stale publications. Bundle::canon_proof_admission reads the
required archived observation with no source or corpus producer path. Its byte
limit is an explicit caller input to the existing authenticated archive selector.

This is an admission observation, not the prepared native proof base. Hydration
does not restore SourceAdmission's private live-compiler commitment. Original
prepared-canon composition, context/distinct-body/datatype semantics and installed
full-canon acceptance remain required by the original issue.

Source contribution lookup now builds one invocation-local reverse index over
canonical native node/graph and statement/carrier keys. Document positions are
deduplicated within each contribution; document paths are borrowed once, and query
results retain the old owned, sorted path-set contract. No RDF, public ledger,
selection policy, compiler profile, dependency or host setting changed. The index
is transient; no cumulative carrier cache is added.

The baseline selected all 68 existing TPTP tests and passed (origin-index-baseline.log).
Initial implementation passed all 72 selected tests (canon-admission-tests.log):
the 68 existing cases, original-scan/index differential, two publication tests and
inert archive wiring. These are correctness executions, not performance benchmarks;
no elapsed speedup, peak-memory result or current whole-canon blocker count is
claimed. Final typed-diagnostic and direct bundle-reader controls passed with all
73 selected tests in canon-admission-repaired-tests.log. The first final build
failed on a test-only diagnostic function module path; that failure remains in
canon-admission-final-tests.log, not acceptance evidence. The corrected run is
the final test evidence. The full local producer/gate remains unrun under
the standing host-probing prohibition; no admission/authentication weakening is
authorized or implemented. Hosted exact-source production remains required.

Strict producer-independent workspace/all-target Clippy passed with warnings
denied in canon-admission-clippy.log (3m05s); its existing four corpus-dependent
consumer exclusions are unchanged.

Committed and pushed as 41d204773 with every normal commit hook passing
(canon-admission-commit.log). PR 1832 publishes this increment without closing the
full original issues. stagectl created the PR but could not parse the resulting
URL as JSON; the existing PR was resolved through stagectl pr, without repeating
creation. The source worktree is clean. Exact-source hosted production and current
CodeRabbit review are pending; no merge-ready verdict is claimed.

The missing/malformed/unsupported report paths use the shared diagnostic substrate.
The producer and consumer retain the decoder's typed diagnostic instead of
replacing its code with text. No independent review is claimed: delegation APIs
are unavailable; current-head CodeRabbit review will be requested through the PR.

Separately, issue 1768 was found closed despite its last status comment explicitly
saying complete-canon semantics remained open. It was reopened, with source-path
evidence and preserved distribution acceptance, in comment 6223532303. This does
not represent the original consumer proof contract as completed by a report.


### paudley — 2026-10-16T22:50:29Z

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


### paudley — 2026-10-16T23:46:47Z

# Combined repair qualification

The existing three-PR stack is pushed: pipeline bc230b3f6, helper 213e0bd2a, proof 9cd02cd77. Actual-base synchronization was clean. No new branch, worktree or PR was created, and no final integration or main push occurred.

The replacement deletes CertifiedAnswer, RawDerivation, the explanation Turtle parser and its three obsolete formatters. Production callers preserve the complete native certificate and selected proof; native question preparation supports independent checking. The sink reads the authenticated Reasoning payload already retained by the snapshot, preserving modal worlds and nested/directional terms without another parser or longer original-carrier retention.

Combined qualification: 136/136 focused synthetic tests passed in 31.540s, including the live source-only repository seal. Strict producer-independent workspace/all-target Clippy passed in 1m38. The complete helpers-hygiene target passed on the final combined head. Exact logs: combined-repair-focused-tests.log, combined-repair-clippy.log, combined-repair-helpers-final.log. Corpus was not produced. The earlier overly broad selection was interrupted during compilation and is not counted as a pass.

Deleting the parser reduced fixed-hasher copies from 702 to 701. The authoritative helper ledger records that decrease only. A pipeline-only ledger regeneration would add or raise other unresolved helper debt, so that generated change was discarded; the helper repairs remain a required part of final integration. No scanner, exemption, dependency or semantic admission was weakened. A misplaced untracked review report was moved into Stage and its identical root reply duplicate removed; neither entered source. Normal commit hooks passed after that correction. Pushes succeeded.

CodeRabbit explicitly reviewed executable-source head 9086140cf, reporting no actionable findings and 88.71% documentation coverage. The only subsequent source delta is the measured one-line ledger decrease. Linked-issue/scope checks were skipped; they do not establish full issue completion. The separate pipeline review request was rate limited and is recorded as unavailable.

Hosted final-source production, authenticated consumers and full acceptance remain pending. No local optimized producer, fingerprint, full gate or machine probe ran. All PRs remain open; final integration uses ghprsq after actual acceptance. Full canon semantics, prepared canon, installed proof composition and measured production performance remain open under the standing plan. Principles 4, 6, 7, 17 and 18 apply.


### paudley — 2026-10-17T00:11:29Z

# Formula context admission

The existing FOF path silently accepts scope annotations on formula roots and owned formula children. The before-fix product run proves the defect: two tests failed because a context-bearing source produced Complete rather than Blocked; explicit inContext translation and independent document selection passed. Exact evidence: formula-context-before-fix-tests.log.

The shared frontend already implements inContext and typed modal standard translation; reimplementing that machinery would create a second owner. Preserve those native formulas. Index other context requirements once over the existing graph-local source edges and propagate them to formula ancestors, including atom-routed roots and shared subtrees. A selected source whose context is not represented must refuse the whole FOF problem instead of silently becoming unconditional. Retain named-graph/document isolation and malformed source outcomes. Advance the shared source profile to v3 with an accurate context contract; retain no v2 execution fallback. This repair does not by itself implement every remaining contextual execution contract required by the full issue.

Regressions cover the source scope coordinates and both atomic/compound routing, scope hidden below an exclusive owner, mixed explicit context and unsupported metadata, valid explicit contexts and all six typed modal accessibility families, an independent document selection, and named-graph isolation. Run the full TPTP admission/publication selection with source-graph/formula-reader controls and strict producer-independent Clippy, then normal hooks/push. No corpus construction, local optimized producer or host probing is authorized.

Commit and publish this coherent repair in the existing proof branch/PR. Full canon semantics, prepared native canon, installed composition and hosted acceptance remain required by the standing plan. Principles 4, 6, 7, 17, 18 and 19 apply.


### paudley — 2026-10-17T00:41:16Z

# Source context survives FOF admission

The actual before-fix product run returned Complete for context-bearing formula roots and owned children; both required-refusal tests failed. Explicit inContext and independent-document controls passed. formula-context-before-fix-tests.log records the false-complete defect.

The admission builder now indexes untranslated context requirements once from the existing native source graph and propagates them to formula ancestors. Graph-local identities prevent named-graph scope from contaminating default-graph sources. Both formula and atom-routed roots honor the result. A selected contextual claim that has no typed translation blocks the complete FOF problem; it cannot become an unconditional sentence or vanish as owned support. No new AST, reparser or fallback evaluator was added.

The existing frontend remains the owner of explicit inContext and typed modal standard translation. Positive controls preserve those emitted context arguments and all six typed accessibility families for necessity and possibility. Negative controls cover the other scope coordinates on roots, descendants and mixed explicit/unsupported contexts. Independent selected documents and named graphs remain isolated. The shared source profile is v3, with no old-profile execution fallback.

All 103 TPTP/source-graph/formula-reader cases passed in 0.132s; formula-context-fixed-tests.log. Strict producer-independent workspace/all-target Clippy passed in 4m19; formula-context-clippy.log. Normal commit hooks passed for e82713091; formula-context-commit.log. Actual helper base edef801c6 was synchronized cleanly, changing only the workflow and its contract tests, and creating combined head 6870f384d. The executable compiler code and its 103-case evidence remain unchanged. Final combined qualification passed strict Clippy (9.10s), all 20 CI gate contracts (0.015s), and the complete helper-hygiene target. Exact logs: context-helper-combined-clippy.log, context-helper-combined-contract-tests.log and context-helper-combined-helpers.log. Push succeeded; context-helper-combined-push.log. No new worktree, branch or PR was created.

This closes an unsound proof-admission path. It does not claim complete contextual execution, zero whole-canon blockers, a prepared native canon, installed consumer proof acceptance or full hosted qualification. Those remain required by the standing full plan. No corpus was produced by tests, no local optimized producer/fingerprint or machine probe ran, and no measured pipeline speedup is claimed. Principles 4, 6, 7, 17, 18 and 19 apply.

Hosted helper follow-through required one setup correction: both earlier helper jobs failed offline metadata on absent adler2 v2.0.1. Actual base bc70ca583 explicitly fetches the locked workspace graph before the unchanged offline checker. Final combined head 2e6956703 synchronized that base cleanly; strict Clippy passed (1.94s), all 20 CI contracts passed (0.010s), and push succeeded. Exact logs: context-helper-fetch-combined-clippy.log, context-helper-fetch-combined-tests.log and context-helper-fetch-combined-push.log. The fresh hosted locked fetch succeeded and its checker is live; no hosted gate pass is claimed yet.


### paudley — 2026-10-17T01:32:04Z

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


### paudley — 2026-10-17T02:21:44Z

Contextual application replacement — 94b5979d9

The red production-source/FOF control proved scoped unary p(a) at w and ordinary binary p(w,a) emitted an identical sentence. Native finite-context lowering also inferred a world from any ternary atom.

Replace that encoding with the canonical logic:contextualApplication family: context, reified relation and original arguments are separate coordinates. The original world-prepending translation and ternary-guessing decoder are deleted; there is no compatibility fallback. Shared native validation rejects malformed family uses, including binary RDF assertions/axioms and binary class properties. Principal-predicate metadata retains the original relation. Native modal evaluation consumes the explicit coordinates. Source profile v4 and compiler/snapshot cache versions bind the semantic change. Canonical ontology documentation and all seven French/Chinese annotations accompany it. Principles 4, 7, 12, 17, 18 and 19.

Validation:
- Exact failing reproducer: context-arity-before-fix.log.
- First expanded run: 275/277 passed; retained memo expectation and real binary-assertion admission failures: context-application-tests.log. Both corrected.
- Same 277 source/FOF/native IR/RDF/Common Logic/modal/temporal cases then passed in 0.305s: context-application-settled-tests.log. These include actual native contextual query evaluation and ternary-impersonation refusal. No test produced corpus.
- Strict producer-independent workspace/all-target Clippy passed with warnings denied: context-application-final-clippy.log.
- Complete helper hygiene passed: context-application-final-helpers.log.
- Full source lint passed before and after final translation wording: context-application-lint.log and context-application-final-lint.log.
- Normal signed commit hooks and push passed. No hook bypass.

Full hosted generation, authenticated consumers, ontology materialization/translation-floor qualification and current independent review remain unproven. This is an incremental fix, not completion of issues 1770/1768 or full canon semantics, prepared native canon, blinded installed consumer acceptance or production performance. Prior review request was rate-limited, not completed. No local optimized producer, fingerprint or host probe ran.

Latest prerequisite CI readback: pipeline run 38988869011 passed producer-build, lint and Rust prebuild. Generation (a) and fixture-prefix both failed after explicit runner shutdown signals; generation (b) remains live. The generation log demonstrated stage-docs-render completed before the shutdown. These failures do not establish a source defect or successful full qualification. Exact prefix log: context-root-fixture-prefix-failure.log.


### paudley — 2026-10-17T02:22:18Z

fix(logic): preserve canon admission, native proofs and contextual applications

Preserve original-source canon admission and exact selection identity in the reasoning archive. Decoded observations remain inert. Use one document-contribution index and refuse untranslated contextual scope rather than silently asserting it unconditionally.

Return native PurRDF Certified values, including certificate/proof/checking context and typed refusals, from DL services. Prepare the exact selected question. Remove the obsolete answer wrapper and rejected-decimal-to-rational retry. Valid native exact decimals remain supported.

Replace the string-only frontend parse error and identity adapters with the shared diagnostic boundary through actual CLI, pipeline, dialect and conformance consumers. Preserve native kind/grade/focus/evidence and original parser/read causes.

Replace ambiguous world-prepended atoms with canonical contextualApplication coordinates. Remove the old translation and ternary-guessing decoder. Ordinary predicate arity is preserved; malformed family uses fail explicitly. Update source/cache versions and canonical documentation with French/Chinese annotations. Principles 4, 6, 7, 12, 17, 18 and 19.

Validation and integration:
277 focused source/FOF/native IR/RDF/Common Logic/modal/temporal tests pass after the final context fix. Strict producer-independent workspace/all-target Clippy, full helper hygiene, source lint, normal signed hooks and push pass. Prior unchanged implementation evidence includes 138 source diagnostic cases plus three final round-trip cases, 46 datatype controls and native proof/preparation controls. No test produces corpus. No local optimized producer, fingerprint or host probe ran.

The actual PR base is paudley/1834-repair-shared-helper-ownership-and (PR 1835), incorporating pipeline PR 1830. The fetched base has no commits missing from this head. The candidate merge is clean and contains the tested head tree; no conflict resolution or base synchronization is required. This integrates into the existing helper branch, not directly into main.

Hosted generation, authenticated consumers, translation-floor qualification and current independent completion review remain incomplete. The earlier review request ended rate-limited; green outer bot status is not current-head review evidence. Pipeline prerequisite generation (a) and fixture-prefix failed following explicit runner shutdown signals, with generation (b) still live at readback. Existing source/pipeline work is being integrated under the user's explicit instruction: "Please ghprsq this branch now, we'll create issues for anything that comes up in CI, but we need to get this code pushed and merged." No check is relabeled as passed and no independent audit is fabricated.

This increment advances issues 1770, 1768, 1762 and 1803; it does not close them. Complete canon semantics, prepared native canon, blinded installed consumer proofs and production-world performance remain under the authoritative plan. No compatibility path, test weakening or additional runtime dependency is retained.

Authoritative plan: .stage/certify-and-precompile-the-complete/plan.md
Selected Stage directory: .stage/certify-and-precompile-the-complete
Current evidence: context-application-report.md, validation.md and review-debt.md in that directory. Historical failed and successful receipts are retained.

Defect-Class: none


