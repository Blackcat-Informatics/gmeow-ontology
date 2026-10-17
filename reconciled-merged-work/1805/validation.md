# Acceptance index

## Current main reconciliation

The native RDF 1.2 rule migration was applied to the combined branch and is now
present on main fb73513b8 through eca80ae00 / PR 1802. The original isolated copy
predates the checked PurRDF constructor/error APIs and newer canonical terms.
Its full staged/unstaged snapshots are retained in pre-sync-*.patch and the stash
identified by pre-sync-stash.txt. The worktree fast-forwarded cleanly to main;
the older copy was not reapplied over newer source. No duplicate implementation
PR is needed. Main retains native rule atoms, every scope dimension, ordered
term-identity guard carriers and the production-profile GTS regression.

Current native rule/statement/GTS checks passed all ten selected cases using the
producer-independent inventory in the 1792 worktree, recorded in its
raw/main-native-rule-regressions.log. That execution consumes no corpus. Original full-corpus
qualification remains pending on the shared generation/fixture repair PR 1830.
No whole-corpus or independent completion pass is asserted.

## Historical development evidence

Plan: plan.md. Base: combined branch 13a44f7a9.

- Native rule writer and reader: implemented, further validation running.
- Source/consumer migration: implemented across canonical logic/lang, fixtures and consumer tests; no classic question-mark-variable authoring remains under crates/slices/tests/docs.
- IR byte identity and statement non-assertion: first two native controls passed (native-rule-focused.log); expanded current pass is pending.
- GMEOW-profile GTS statement-layer round trip: implemented, not yet executed successfully.
- Formatting: passed. Current focused compiler/consumer run: migrated-consumers-v2.log. First migrated run failed compilation on a checked constructor Result; fixed with ordinary error propagation.
- Full qualification and independent review: NOT RUN / NOT VERIFIED.
- Completion, PR readiness, merge: NOT MET.

## Expanded execution and remediation

migrated-consumers-v3.log: nextest executed 967 cases, 945 passed and 22 failed
(1.331 seconds execution after a 3m10 build). The native typed-rule round trip and
the production-profile GTS carrier-root/write/import/read round trip both passed.
The latter retains the complete directional literal, dynamic predicate, scope,
negation and guard. No test produced the ontology corpus.

Nine compiler failures: seven CLIF/CGIF metadata round trips lost native statement
rows, one SSSOM metadata fixture lacked its now-mandatory preservation declaration,
and one intentional SHACL-AF golden drifted from value inequality to !sameTerm.
The two-line golden change was reviewed against shacl-af.snap.new and accepted;
the transient .snap.new was removed. CLIF/CGIF writers now use the shared complete
default statement view, and their readers decode the already-emitted triple form.
The SSSOM fixture explicitly authors its preservation rather than weakening the
production invariant.

Five gate cases used the old N-Quads parser on the newly readable Turtle fixture.
The fixture now goes through an explicit tiny Turtle-to-native-N-Quads conversion
before the unchanged production N-Quads entry point; its no-wiring counterexample
removes the actual prefixed declaration. Eight remaining failures were authenticated
corpus consumers selected without the required manifest. They remain mandatory in
full qualification; no fallback or test-side producer was added.

Further exactness inspection found atom-local scopes were omitted by the previous
rule writer. One scope emitter now serves scoped axioms, rules and rule atoms.
The reader preserves atom scopes while source-owned rule components stay out of
standalone scoped assertions. The typed round-trip control now exercises all six
atom scope dimensions. A compile error from hashing the owned Subject adapter was
fixed by indexing the exact native TermId, preserving source identity without a
new string or adapter hash representation.

Latest corpus-free compiler/consumer run: migrated-consumers-v5.log, 952 executed,
951 passed, one failed (0.563 seconds execution). All dialects, native scoped rule
atoms, gate controls, GTS carriage and the reviewed golden pass. The sole failure
expected an explicit xsd:string field in the correspondence IR although its input
authored RdfLiteral::simple with datatype None. The IR correctly preserves that
native value. The two exact native assertions now expect the authored simple
literal; the separate effective-datatype cell assertions remain unchanged.
alignment-fixture-final.log is the focused recheck, pending.

The combined optimized producer is building in raw/native-producer-build.log of
the combined Stage directory. The single full qualification has started there:
make check CHECK_ARGS='--timings-json .stage/perf-logic-finish-merged-pipeline/raw/native-full-gate-timings.json'.
Output is raw/native-full-gate.log. It owns materialization plus the complete gate;
no second manual corpus-production run or tests-side construction was added.
Both production/full qualification are RUNNING, not passed.

Research: W3C RDF 1.2 Turtle grammar permits only an IRI or blank node in a triple-term subject; variables therefore require actual resource carriers, not literal subjects or inline property lists. The current source-reader helper already inspects native reifier quads. Rule guards use ordered argument carriers: native distinct_pairs_satisfied compares RDF term identity, while procedural logic:termDistinct means SPARQL value inequality. The draft quotation of that relation was removed before publication. The SHACL-AF guard now lowers to !sameTerm, preserving the same identity contract. Definitions and existing French/Chinese coats for the five directly affected structural terms move together.

The current patch applies cleanly to the combined branch with git apply -p0 and was applied there without overwriting its uncommitted numeric-range changes. The patch retains this isolated development copy; combined full qualification has not run. The one-use initial source migration report/preview is historical, preceding the corrected guard representation.

Stage evidence remains local, excluded under repository-local tool-state policy, and retained for explicit ghprsq archival. The root checkout and sibling worktrees remain preserved.
