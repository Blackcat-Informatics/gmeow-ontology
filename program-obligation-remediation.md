# Program-aware obligation discharge

Scope: the complete 1746/1747 obligation contract within the existing combined
candidate and the still-active all-logic goal. This is not a replacement for the
native-production verdict, complete-canon proof or any other issue's acceptance.
The refreshed 1747 body/comments are in raw/issue1747-current.json. The first
Stagectl fetch timed out; its successful retry supplies the issue evidence.

## Observations and reproduced failure

obligations.rs obtains syntactic heads only from foundation::head_predicate_iris.
verify.rs passes its flattened reasoned store and a global derived-predicate set,
losing the actual compiled program and world ownership at this seam. The new
synthetic control constructs a real typed formula program, executes reason_program,
requires a valid conclusive native closure and checks both syntactic-only and
finite-closure declarations. The current syntactic-only case incorrectly returns
just an inventory note, even with a matching authored head. Its baseline failure
is in raw/program-obligation-baseline.log. This is an actual regression, not a
zero-case check or an upstream engine-conformance test.

Runtime-refined effects cannot alone repair the syntactic arm: they can erase a
writer whose body is currently unsatisfied, whereas a matching authored head must
still prevent syntactic discharge. Native schema/dynamic writers must also be
accounted for; a textual Horn-head union would remain incomplete.

## Required implementation

1. Retain the complete typed-program digest already computed for preparation-cache
   lookup, including for uncached programs and native publication. Bind it into
   the native source-template/input contract without another hash, lowering,
   serialization round trip or corpus materialization. This prerequisite is now
   implemented; its targeted controls are pending. The execution witness source
   contract remains distinct from the whole selected-input identity.
2. Build one reusable, content-addressed syntactic writer analysis for the actual
   selected program/source/import/world/engine scope. Reuse compiled IR and native
   flow layouts. Preserve authored head possibilities independently of current
   body satisfaction, propagate potential schema-induced consequences, retain
   unknown dynamic writes conservatively and account for rejected/lost units.
   Keep this profile distinct from runtime body-reachability pruning. Do not
   weaken runtime selection, substitute a second source compiler or infer absence
   merely from an empty current relation. Include the foundation catalog once.
3. Carry the complete analysis as mandatory native evidence for the selected
   assessment. Keep metadata compact; do not retain source datasets, a second
   serialized program or all intermediate value-flow state. The existing native
   publication authenticates the evidence, not a self-reported graph label.
4. Wire the actual verifier/public assessment through retained native evidence.
   Preserve each world's original ownership and document-scoped identities;
   flattened query presentation cannot be the authority for obligation scope.
   Reuse the one governance validator and preserve malformed-declaration failures,
   candidate attribution, complete inventories and typed receipts. Refuse missing,
   stale, foreign or unsupported analysis before publishing a negative proof.
5. Preserve the alternative-proof semantics: potential reachability means unknown,
   never violation. A certified finite closure may discharge despite a potential
   writer; a real derived forbidden edge supplies violation. Unsupported declared
   discharge conditions remain hard errors. Do not turn an inconclusive result
   into a successful empty assessment or equate rdf:type/instanceOf globally.

The four-case control requires unknown only for a nonfiring head under the
syntactic-only condition. A real derived counterexample is a violation even when
the declaration requests only syntactic discharge; the declaration chooses a
sufficient negative proof, not permission to ignore a positive counterexample.
The initial draft's seed=true/syntactic-only expectation was corrected against
the issue's explicit semantics before that case ran. The observed baseline
failure remains the first seed=false/syntactic-only case.

## Native integration constraints

The complete preparation digest is now forwarded through PreparedProgram and its
publication into JointTemplate's v2 source contract. Five preparation/identity
controls pass (1c8057bf-95c6-4b7e-85f8-c81d602c68d7), as do 180 affected native/
certificate consumers. Source lint and strict Clippy pass at that prerequisite.
The obligation regression itself is still red; none of this clears 1747.

JointInput's runtime world_effects are body-refined and unsuitable as the sole
syntactic inventory. Applicable schema selection also depends on current bodies,
so retaining just that subset could miss a law activated by an authored metadata
head. Do not fix this by seeding every internal DL rule head: an unconstrained
internal type head could invent arbitrary property markers. Preserve the actual
runtime selection/termination path; this is a distinct required static analysis.

A reusable static plan should consume already compiled FlowRule/PreparedPropertyRule
metadata for all native schema laws, source/chase rules and family effects. Avoid
constructing another executable, termination certificate or source compiler.
Factor the existing property-flow metadata builder rather than copying it. Reuse
the core flow layouts and bounded template ownership; keep the expanded analysis
run-local, not a persistent corpus cache. Per-world source-head seeds come from
prepared authored rules and source producers owned by that world, plus its modal/
contextual envelopes. They are abstract possible patterns, never fake input facts.
Refine the full static schema plan over these possibilities, and retain the
authored fixed heads even when their own bodies do not currently fire. Capture
finite predicate envelopes plus an explicit unbounded-predicate state, with the
complete examined producer inventory. Scope source selection/list immutability
per world; a foreign world's metadata writer must not widen the local theory.

NativeFamilyLedger retains demanded native proofs and their exact source
ownership; it does not implicitly retain every assertion. The first integrated
run exposed this distinction. The candidate now registers governance declaration
and field proofs at initial source admission and after each committed suffix,
including a zero-budget run, without interpreting malformed declarations during
the separate WorldConsistency operation. The assessment consumes this evidence per world,
not rediscover the program from the flattened verify graph. The shared governance
validator can consume a small native projection of just its declaration/field
rows; no whole-corpus transport or RDF-text serialization/reparse is needed.
Keep malformed governance validation at the selected assessment boundary rather
than silently changing the meaning of the separate WorldConsistency claim.
The public assessment should require the native result/analysis, with no weaker
foundation-only overload. Validate program/input/engine/world framing and the
parent publication's authority before any negative proof. Preserve all supported
positive counterexamples even when a separate family or budget is incomplete.

## Verification and publication

The source-identity control uses two equal executable rule sets with different
typed axiom content, requiring different native input contracts and preserved
identity on publication. The main regression covers firing and nonfiring custom
heads under syntactic-only versus finite-closure conditions. Add unreachable,
schema-induced/dynamic, malformed/excluded, mixed-typing, import/world/blank-scope
and changed-program/receipt controls through the real native caller. Tests use
only tiny synthetic inputs; no test produces the repository corpus.

Use focused Make checks, strict warnings, lint and affected source-policy gates.
The required final full producer/gate/cold CI and independent/current-head review
remain mandatory after the known production refusal is addressed. Collaboration
APIs are unavailable here; local analysis is not an independent completion audit.
Publish actual results and source changes through Stage 2; do not resolve or close
the issue merely because the custom-head regression has been repaired.

Principles 4, 7, 9, 17, 18 and 19.
