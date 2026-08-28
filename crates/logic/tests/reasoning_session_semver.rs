// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC6 (governance) — the "remains semver-governed" guarantee as an executable drift pin.
//!
//! The public session surface is `#[non_exhaustive]` (a compile-time additive guarantee)
//! plus a descriptor-hash drift pin: any change to the engine descriptor — or to the
//! content-addressed [`SessionIdentity`] built from a FIXED input — moves a golden BLAKE3
//! hex, forcing a DELIBERATE version bump and checkpoint re-bless rather than a silent
//! semantic drift that leaves existing checkpoints spuriously valid.

use gmeow_logic::runtime::{EngineContract, ReasoningSession};

mod session_common;
use session_common::*;

/// Golden engine-descriptor hash. A drift here is a deliberate engine version bump.
/// Re-blessed for the broader chase-termination-class ladder (joint / super-weak /
/// model-summarizing acyclicity certifiers + the authored-existential-rule surface),
/// which extends the forward reasoning/certificate contract folded into this descriptor.
/// Re-blessed again for the stage-2 certifier hardening: the partial-order `combine` meet
/// (incomparable JA∥SWA meet to their glb), the budgeted MSA critical-instance fixpoint
/// (Exhausted → conservative refuse), the fail-fast authored-rule reader, and the
/// certifier perf rewrites — all move the `physical/chase.rs` / `reason/dl.rs` content
/// digest folded into this descriptor. Re-blessed once more for the round-3 certifier
/// hardening: the reordered soundness differential, the atomic authored-rule reader
/// (non-resource ref / duplicate slot hard-fails), and the MSA critical-instance size cap
/// all move the `physical/chase.rs` / `reason/dl.rs` content digest. Re-blessed for the
/// functional-characteristic carrier migration: the foundation chase now derives
/// `functionalProperty(?P,?P)` from the canonical `logic:PropertyCharacteristicAssertion`
/// carrier (a new Datalog rule alongside the `owl:FunctionalProperty` marker rule), and
/// `reason/dl.rs` unions the carrier into the functional-clash reader — both move the folded
/// program/`reason/dl.rs` content digest. Re-blessed for the key carrier migration: `reason/dl.rs`
/// now reads keys from the canonical `logic:KeyAssertion` carrier (`logic:keyClass` +
/// `logic:keyProperty`) unioned into the key-agreement clash reader and coverage inventory
/// alongside the `owl:hasKey` list, so the datatype/single-property key survives removal of the
/// `owl:hasKey` slice declaration — moving the `reason/dl.rs` content digest folded here.
/// Re-blessed once more for the stage-2 `cargo fmt` pass over the branch-modified reasoning
/// core: reformatting `reason/dl.rs` / `physical/chase.rs` (behaviour-preserving — `reason-verify`
/// stays green) moves the raw-source content digest folded into this descriptor. Re-blessed again
/// for the merge of origin/main PR 1385: the public bilinear-form distance API (`bilinear_sqdist` /
/// `compare_sqdist` / `BilinearFormError`) on the runtime engine surface (`physical/builtin_eval.rs`,
/// `physical/mod.rs`) also folds into the runtime engine-source content digest, so the golden below
/// is the merged value (both this branch's fmt and PR 1385's bilinear API move the digest).
/// Re-blessed for the fragment-certified refutation kernel: `reason/refute.rs` is registered
/// as a new load-bearing `NATIVE_CONTRACT_COMPONENTS` engine component (the unified beyond-Horn
/// decider the `reason/dl.rs` decide path now invokes), so the native contract hash folded into
/// this descriptor moves. The kernel is inert on this input (it registers no family sub-decider
/// yet), so no reasoning verdict changes — only the source-content digest.
/// Re-blessed for Family 5 (the FIRST real refutation sub-decider): `reason/refute/datatype.rs`
/// is a new engine source module registered in `SUB_DECIDERS`, and `reason/dl.rs` now consults it
/// for datatype value-space coverage (promoting the facet / cardinality / oneOf families exactly
/// when the subsolver decides). Both move the native-contract source-content digest folded into
/// this descriptor. This DOES change reasoning verdicts on the datatype value-space fragment
/// (previously-withheld W3C-divergence cases are now soundly decided), but NOT on this fixed
/// datatype-free input, so the fixed-input session verdict is unchanged. (Value below
/// is the post-`cargo fmt` state of the Family 5 branch — the behaviour-preserving format
/// pass over `reason/refute.rs` folds into the raw source-content digest.)
/// Re-blessed for Family 2/6a/7 (the counting / arithmetic-feasibility sub-decider):
/// `reason/refute/counting.rs` is a new engine source module registered in `SUB_DECIDERS`,
/// and `reason/dl.rs` now consults it for cardinality / inverse-functional / `owl:hasSelf`
/// coverage (promoting those families and narrowing their class-definition / refutation-shape
/// withholds exactly when the sub-decider decides). Both move the native-contract
/// source-content digest folded into this descriptor. This DOES change reasoning verdicts on
/// the counting fragment (previously-withheld W3C-divergence cardinality / IFP / hasSelf cases
/// are now soundly decided), but NOT on this fixed edge-only input, so the fixed-input session
/// verdict is unchanged.
/// Re-blessed for Family 1/3/6b (+ entangled Family 4) — the bounded case-split /
/// complement / union-disjoint / malformed-list sub-decider: `reason/refute/casesplit.rs`
/// is a new engine source module registered in `SUB_DECIDERS`, and `reason/refute.rs`
/// (its `mod` + registry entry) and `reason/dl.rs` (the coverage coordination — the
/// refutation-shape withholds for complement / union / oneOf / malformed list are now
/// narrowed by `!casesplit::decides`) both change. `reason/refute.rs` and `reason/dl.rs`
/// are folded into the native contract hash, so the engine-descriptor digest moves. This
/// DOES change reasoning verdicts on the case-split fragment (previously-withheld
/// W3C-divergence complement / union-disjoint / disjointUnion / malformed-list cases are
/// now soundly decided), but NOT on this fixed edge-only input, so the fixed-input session
/// verdict is unchanged.
/// Re-blessed for Task 6b — the kernel's decidability surface as first-class content:
/// `reason/refute.rs` gains the shipped registry API (`RefutationPattern`,
/// `decided_fragments`, `retained_boundaries`) and a live production consumer
/// (`production_boundary_findings`), the blanket `#![allow(dead_code)]` is removed, and
/// `reason/dl.rs` folds a family-scoped kernel withhold into a new
/// `DlVerdict::boundary_findings`. All fold into the native contract source-content
/// digest, so the engine descriptor moves. The kernel stays inert on this fixed
/// edge-only input (its steady state is `NoDeciderEngaged`, which emits nothing), so no
/// reasoning verdict changes — only the source-content digest.
/// Re-blessed once more when the coverage-gate determinism/refusal `#[cfg(test)]` tests were
/// added to `reason/refute.rs`: `native_contract_hash()` `include_str!`s the whole file, so its
/// byte content moves the engine descriptor; no reasoning verdict changes.
/// Re-blessed once more for the G12 refutation-kernel helper consolidation: `resource_key`,
/// `world_key`, `is_rational_tower`, and `parse_rational` moved from
/// `reason/refute/{casesplit,counting,datatype}.rs` into a single canonical definition in
/// `reason/refute.rs` (folded via `include_str!` into `native_contract_hash()`), so the raw
/// source-content digest moves; this is a pure refactor and no reasoning verdict changes.
/// Re-blessed once more for the DL existential-chase materialization backstop: `reason/dl.rs`
/// (the `≥n` obligation bound + chase-incomplete withhold) and `physical/chase.rs` (the
/// budget-bounded `join_atoms`/`head_satisfied` working set) are folded via `include_str!`
/// into `native_contract_hash()`, so the raw source-content digest moves; the change only
/// turns a previously-OOM super-polynomial materialization into a sound INCOMPLETE withhold
/// and no reasoning verdict on any decided input changes.
/// Re-blessed once more for the RDF 1.2 quoted-triple goal-argument grammar: `query_ir.rs`
/// gains a `<<( s p o )>>` term and `QTerm::Triple`, and the flat/generic lowering, plan
/// hash, reference oracle, probabilistic, and counterfactual surfaces gain the exhaustive
/// arm — all folded via `include_str!` into `backward_source_hash()`, so the raw
/// source-content digest moves. The fixed edge-only input carries no triple term, so the new
/// arm never fires and no reasoning verdict changes.
/// Re-blessed once more for the reasoner-derived `math:` dimensional-homogeneity gate:
/// `EvalRule` gains `constraint_tag` (`rule_ir.rs`), `QBuiltin` gains `DimEqual`/
/// `DimProduct` (`query_ir.rs`), `physical/plan.rs`'s `hash_builtin`/`canonical_rule_hash`
/// gain the new discriminators, `physical/seminaive.rs`'s `apply_builtins` gains the
/// constraint-tagged violation-emitting Filter inversion, `physical/builtin_eval.rs` gains
/// the dimension-resolving `CellResolver::dimension` probe, and `relational_core.rs` gains
/// the `logic:Constraint` → violation-`EvalRule` lowering — all folded via `include_str!`
/// into BOTH `native_contract_hash()` (`forward_contract_hash`) and `backward_source_hash`
/// (`rule_ir.rs`/`query_ir.rs`/`physical/plan.rs`/`physical/seminaive.rs`/
/// `physical/builtin_eval.rs` are members of both source lists), so the raw source-content
/// digest moves on both axes. The fixed edge-only input authors no `logic:Constraint`, so
/// no new rule ever fires and no reasoning verdict on this fixed input changes.
/// Re-blessed once more when `physical/builtin_eval.rs`'s cell loaders were hardened to
/// require EXACTLY one target for each functional dimension/Gram/vector cell property
/// (the new `exactly_one_iri_object`) rather than silently taking the first of a
/// multi-valued cell: `builtin_eval.rs` is folded via `include_str!` into both
/// `native_contract_hash()` (`forward_contract_hash`) and `backward_source_hash`, so the
/// raw source-content digest moves on both axes. The change only makes an already-malformed
/// multi-valued cell decline instead of mis-decoding, so no reasoning verdict on any
/// well-formed input — including this fixed edge-only input, which authors no dimension
/// cell — changes.
/// Re-blessed once more for the `math:` expression-identity reasoned gate
/// (`math_expression.rs`): `physical/lower.rs`'s `math_expression_structural_keys` and
/// `structural_digest` lose their `#[allow(dead_code)]` now that a live caller
/// (`math_expression::check_math_expression_findings`, dispatched from `verify.rs`)
/// exists — `physical/lower.rs` is a member of `BACKWARD_SOURCE`, folded via
/// `include_str!` into `backward_source_hash()`, so the raw source-content digest moves.
/// No lowering behavior changes (only an attribute), so no reasoning verdict on any
/// input — including the fixed edge-only input below — changes.
/// Re-blessed once more for new `structural_digest`/`lower_math_expression` property
/// tests (`interning_tests`-shaped additions to the existing `physical::lower::tests`
/// module — alpha-equivalence, injectivity, and interning coverage, plus the
/// `reference-ast-act.ttl` `math:structuralKey` placeholder reconciliation):
/// `physical/lower.rs` is a member of `BACKWARD_SOURCE`, folded via `include_str!` into
/// `backward_source_hash()`, so the raw source-content digest moves even though only
/// `#[cfg(test)]` content was added — no lowering behavior changes, so no reasoning
/// verdict on any input, including the fixed edge-only input below, changes.
/// Re-blessed once more for widening `reason::math_gate`'s module visibility and
/// `dimension_gate_markers`' fn visibility from `pub(crate)` to `pub` (so a completeness
/// harness in `crates/pipeline` can call it directly): this changes those files' raw
/// bytes, and both are `BACKWARD_SOURCE` members folded into `backward_source_hash()`; a
/// trivial `cargo fmt` rewrap of one test line in `physical/lower.rs` (also a
/// `BACKWARD_SOURCE` member) moves it further. No reasoning behavior changes on any
/// input — attribute/whitespace-only source moves.
/// Re-blessed once more for removing a stray comment reference from `physical/lower.rs`
/// (also a `BACKWARD_SOURCE` member, so its raw bytes move again): comment-only, no
/// reasoning behavior change on any input.
/// Re-blessed once more for two lowering-correctness fixes in `physical/lower.rs` (a
/// `BACKWARD_SOURCE` member): (1) the leaf
/// fallback in `lower_math_node_dispatch` no longer silently accepts a named node
/// carrying an unrecognized `math:` type as an opaque IRI leaf — it now HARD-FAILS with
/// `MathLoweringError::UnrecognizedExpressionType` (which gained a `types: Vec<String>`
/// field) unless the node carries NO `math:` type at all or the recognized
/// `math:SymbolReference` constant-operand type; and (2)
/// `math_expression_structural_keys` now seeds any still-unvisited `math:`-expression-
/// typed node after the root-seeded traversal (`MathGraph::expression_typed_nodes` /
/// `reachable_expression_nodes`), so a fully closed cyclic component with no externally
/// referenced member is still reached and its `math:CyclicExpressionGraph` guard fires.
/// Both DO change reasoning verdicts on `math:`-authored inputs that hit these paths
/// (a previously-silently-accepted ill-typed leaf or rootless cycle is now soundly
/// rejected), but NOT on this fixed edge-only input (which authors no `math:` expression
/// graph at all), so the fixed-input session verdict is unchanged.
/// Re-blessed once more for a phantom-variant removal (see the fixed-input golden below
/// too): `physical/lower.rs`'s `intern_bound_checked_math` duplicate helper and its two
/// `DeBruijnDistanceOverflow` / `DeBruijnSlotOverflow` variants (unreachable by
/// construction — `lower_math_binding` pushes exactly one declaration per binder frame and
/// every descent is depth-bounded by `MAX_MATH_EXPRESSION_DEPTH`) are deleted, and the
/// call site now reuses the shared `intern_bound_checked` helper, panicking on the
/// now-provably-unreachable error case instead of laundering it into a `math:` conformance
/// failure. `physical/lower.rs` is a `BACKWARD_SOURCE` member, so this moves the
/// native-contract source-content digest folded into this descriptor; the fixed edge-only
/// input (authoring no `math:` expression graph) has an unchanged reasoning verdict.
/// Re-blessed once more for the α-equivalence-class term (`math:alphaEquivalenceClass`)
/// reaching a production call site: `physical/lower.rs` (a `BACKWARD_SOURCE` member) drops
/// the `#[allow(dead_code)]` on
/// `alpha_class_iri` / `alpha_class_iri_for_digest`, now called from
/// `math_expression.rs`'s `check_structural_key_drift` (which is itself NOT a
/// `BACKWARD_SOURCE` member, so its own new `classify_structural_key_usage` /
/// `math:MalformedStructuralKey` logic does not independently move this hash). This moves
/// the native-contract source-content digest folded into this descriptor; the fixed
/// edge-only input (authoring no `math:` expression graph) has an unchanged reasoning
/// verdict.
/// Re-blessed once more for the REAL, generating `structural_digest`/
/// `lower_math_expression` α-equivalence property-test module (`physical::lower::tests::
/// interning`), which replaces a five-string hardcoded-suffix-table example test with a
/// `proptest` generator driven through the real `MathGraph`/`lower_math_expression`
/// pipeline (bound-variable renaming, nested shadowing, slot arity/order, injectivity,
/// and cross-dag determinism): `physical/lower.rs` is a `BACKWARD_SOURCE` member, so this
/// test-only content moves the native-contract source-content digest folded into this
/// descriptor; no lowering behavior changes, so the fixed edge-only input (authoring no
/// `math:` expression graph) has an unchanged reasoning verdict.
/// Re-blessed for the origin/main merge into this branch: this branch's ADDITIVE engine
/// sources — the W4b browser reasoner `reason::reason_closure_dataset` (wrapping the
/// unchanged native chase) and the W4 `conjecture_eval` orchestration module — combine with
/// main's `math:` dimension-gate sources, so the merged source-content digest is a new value
/// (neither this branch's nor main's). No reasoning verdict on the fixed edge-only input
/// changes (all additions are inert on it).
/// Re-blessed for the enactment gate becoming live: `reason/enactment.rs` and
/// `relational_core.rs` are folded engine-source axes, and the gate stopped being a stub
/// (it now compiles `logic/module.ttl`'s 25 failure-class-bearing `logic:Constraint`s into
/// violation rules and chases them), so their bytes — and the folded descriptor — move. The
/// fixed edge-only input authors no enactment record, so no reasoning verdict on it changes;
/// only the identity moved.
/// Re-blessed again for the enactment gate's law-identity fix: every violation rule now
/// heads on `logic:violatedLaw <the law>` instead of the shared
/// `rdf:type logic:EnactmentIntegrityViolation` marker, because a head tuple forty-four laws
/// share is one the chase keeps once — a record breaking two laws reported one of them and
/// silently lost the rest. `relational_core.rs` is a folded engine-source axis, so its bytes
/// and the folded descriptor move. The fixed edge-only input authors no enactment record, so
/// no reasoning verdict on it changes; only the identity moved.
/// Re-blessed once more when `reason_closure_dataset`'s axiom-to-RDF lowering was extracted
/// into the public `inferred_axioms_to_dataset` so the agent-facing MCP `reason_graph` tool
/// could lower a BUDGETED closure through the SAME code path the unbudgeted one uses (R4
/// forbids exposing an unbudgeted chase to an agent loop, and two lowerings would have let a
/// budgeted and an unbudgeted closure of the same size serialize differently). `reason/mod.rs`
/// is one of the folded native-contract source axes, so the descriptor moves with the file;
/// the extraction is a pure refactor — `reason_closure_dataset` now calls the extracted
/// function and no rule, no chase step, and no verdict changed, so the fixed edge-only input's
/// reasoning verdict is unchanged.
/// Re-blessed once more when the hash-consed structured-term arena was relocated out of
/// this runtime into the reasoner-free `gmeow-term-arena` crate: `EXTERNAL_BACKWARD_SOURCE`
/// (`runtime.rs`) `include_str!`s that crate's `src/` tree into `backward_source_hash`, so
/// moving `physical/term_dag.rs` + `physical/term_key.rs` to `term-arena/src/` — and
/// splitting the atom dictionary into `interner.rs` and the term rendering into
/// `display.rs` — changes the folded source-content digest on that axis. The relocation is
/// byte-for-byte behaviour-preserving (the same netstring fold, the same de-Bruijn
/// encoding, the same interning constructors), so no reasoning verdict on any input
/// changes.
/// Re-blessed once more for the public STRUCTURED proof view (`proof_tree.rs`): reading a
/// checked proof term as a step TREE requires `physical/proof.rs`'s `ProofShape` decoder and
/// its `classify` entry to be `pub(crate)` (a second decode of the `App` proof framing would
/// be a forked duplicate of the one place it is parsed), and `physical/proof.rs` is folded via
/// `include_str!` into `backward_source_hash`, so the raw source-content digest moves. The
/// change is visibility-only — no constructor, checker rule, or minting recipe is touched — so
/// no reasoning verdict on any input changes. (`proof_tree.rs` itself is a downstream READER of
/// an already-decided proof and is classified in `NOT_BACKWARD_SOURCE` alongside
/// `goal_directed.rs`, so it adds nothing to the digest.)
/// Re-blessed once more when the DL existential chase stopped treating an `owl:Thing`
/// qualification as a real class qualifier. `reason/dl.rs` is a native-contract component, so
/// normalizing `≥n p.⊤` to the unqualified obligation moves the engine descriptor. This one is
/// a genuine SEMANTIC repair, not a source-churn re-bless: carrying `owl:Thing` into the rule
/// head added a `?witness rdf:type owl:Thing` conjunct that nothing ever asserts, so the
/// restricted chase's head-satisfaction probe could never match, blocking never fired, and a
/// witness was invented even for a subject that already had its filler — one asserted value
/// read back as two and collided with the `≤1` restriction on the same property.
/// Re-blessed once more for the structural key routing through the arena seam:
/// `physical/lower.rs` and `term_arena.rs` are both `BACKWARD_SOURCE` members, so
/// `math_expression_structural_keys` calling `term_arena::intern_math_root` — and the
/// removal of the uncalled `MathGraphInterning` Turtle-bytes wrapper beside it, the
/// `alpha_class_iri` wrapper deletion, the alpha-class materializer moving onto the asserted
/// graph, the unconditional typed-rejection emitter, and the lowering now accepting both
/// authored `math:literalValue` idioms and the abstract expression base as an operand — the
/// last only where it is genuinely undecomposed, so a value-bearing node can no longer be
/// interned on its name — moves the backward-source digest. The published digest bytes are unchanged (`Arena::key` returns `TermDag::key`
/// verbatim, and both routes now fold through the single `fold_content_key`), so no
/// reasoning verdict moves with it.
/// Re-blessed on integrating main, and for the same structural reason as the earlier
/// integration note above: BOTH sides had moved this golden away from the merge base, so
/// neither branch's value is correct for the merged engine. The descriptor folds this
/// branch's enactment-gate registration and law-identity fix together with main's term-arena
/// relocation and `proof.rs` visibility change, producing a THIRD value that is not a choice
/// between the two. It was recomputed from the merged engine rather than resolved by taking a
/// side — taking a side here would pin a hash that no build actually produces. Every
/// contributing change is individually verdict-preserving on the fixed edge-only input, so
/// only the identity moved.
/// Re-blessed once more for the RDF 1.2 statement-metadata lowering: `statement_lowering.rs`
/// is a NEW folded engine-source axis (the reasoning-session contract hashes the bytes of
/// every engine source, and the lowering is one), and `reason/refute.rs` moved with the
/// nested-triple-term boundary record. The fixed edge-only input carries no RDF 1.2
/// statement metadata at all, so its reasoning verdict is unchanged; only the identity moved.
/// Re-blessed on integrating main. BOTH sides had moved this golden away from the merge
/// base, so neither branch's value is correct for the merged engine and taking a side would
/// pin a hash no build produces. Recomputed from the merged engine, which folds this branch's
/// expression-identity work together with main's RDF 1.2 statement-metadata lowering. Every
/// contributing change is individually verdict-preserving on the fixed edge-only input, so
/// only the identity moved.
/// Re-blessed once more for the abstract expression base joining the root population, so a
/// `math:structuralKey` authored on `math:MathematicalExpression` — the property's own declared
/// domain — is compared against a computed digest instead of skipped. `physical/lower.rs` is a
/// `BACKWARD_SOURCE` member; the fixed edge-only input carries no `math:` expression graph, so
/// only the identity moved.
/// Re-blessed once more for binder arity: a `math:BindingExpression` binds its variable over
/// its INDEXED operand sequence, which is what the slice authors ("its body through indexed
/// math:argumentSlot cells"; a `math:ModelFormula` is "a binder over indexed math:ArgumentSlot
/// operands"). The one-operand case still interns the bare body, so a `math:` binder and its
/// alpha-equivalent `logic:` quantifier still collapse to ONE node and no existing digest
/// moved; only `physical/lower.rs`'s bytes did, and it is a BACKWARD_SOURCE member.
/// Re-blessed once more for a COMMENT correction in `reason/dl.rs`: the notes on
/// `refutation_shape_withholds` and `cardinality_on_datatype_property` claimed the committed
/// bundle asserts only exact `cardinality 1` and qualified cardinalities, which was untrue
/// (`math:compilesToLogicFormula` carries two plain `owl:minCardinality "1"` `rdfs:domain`
/// companions) and is now stated correctly. `native_contract_hash()` `include_str!`s the
/// whole file, so the raw-source content digest folded into this descriptor moves. No engine
/// capability, withhold, or decider registration changed — the diff is comment lines only, so
/// no reasoning verdict on any input changes.
/// Re-blessed once more for a FURTHER `reason/dl.rs` comment correction on the same two
/// notes: the previous wording still claimed those two `math:compilesToLogicFormula`
/// companions were the committed bundle's ONLY plain cardinality restrictions, which the
/// `logic:` grounding-surface demonstrators (`ex:minMemberRestriction` /
/// `ex:maxLeadRestriction`) falsify. Both notes now state the REACH condition that actually
/// keeps the withholds quiet (never in a class-definition position; never `owl:onProperty`
/// an `owl:DatatypeProperty`) instead of an unqualified corpus census.
/// `native_contract_hash()` `include_str!`s the whole file, so the raw-source content digest
/// folded into this descriptor moves. No engine capability, withhold, or decider
/// registration changed — the diff is comment lines only, so no reasoning verdict on any
/// input changes.
/// Re-blessed once more on integrating main into this branch, for the same structural reason
/// as the two integration notes above: both sides had again moved this golden away from the
/// merge base — main by the RDF 1.2 statement-metadata lowering, this branch by the two
/// `reason/dl.rs` comment corrections — and `native_contract_hash()` `include_str!`s the whole
/// of `dl.rs`, so the merged contract text is the UNION of both sides' bytes and its digest is
/// a third value that is neither side's. It was recomputed from the merged engine rather than
/// resolved by taking a side. Every contributing change is individually verdict-preserving on
/// the fixed edge-only input, so only the identity moved.
/// Re-blessed once more for the leave-one-out canonical-subsumption lowering in
/// `reason/mod.rs`: a probe spelled `logic:subClassOf`/`logic:subPropertyOf` is now lowered
/// to the `rdfs:` spelling the fixed calculus matches — the SAME EDB-boundary lowering
/// `reason/rl.rs` already performs — so it is answered by the analytic
/// `TransitiveReachability` index instead of falling through to a per-axiom incremental
/// fork plus a full finite-DL augmentation that could only ever answer "not re-derived"
/// (no fixed rule head is spelled `logic:`). `native_contract_hash()` `include_str!`s the
/// whole of `reason/mod.rs`, so the raw-source content digest folded into this descriptor
/// moves. No rule, decider, or profile capability changed, and the fixed edge-only input
/// authors no subsumption edge in either spelling, so its reasoning verdict is unchanged.
/// Re-blessed once more on integrating main into this branch, for the same structural reason
/// as every integration note above: BOTH sides had again moved this golden away from the merge
/// base — main by the two `reason/dl.rs` comment corrections and the leave-one-out canonical
/// subsumption lowering in `reason/mod.rs`, this branch by the expression-identity, structural
/// key, and binder-arity work in `physical/lower.rs` — and the descriptor folds the bytes of
/// every engine source. The merged contract text is therefore the UNION of both sides' bytes
/// and its digest is a THIRD value that is neither side's; taking a side would pin a hash no
/// build produces. Recomputed from the merged engine. Every contributing change is
/// individually verdict-preserving on the fixed edge-only input, so only the identity moved.
/// Re-blessed once more for the canonical-subsumption lowering at the FORWARD EDB boundary:
/// `reason/mod.rs`'s `build_edb_facts` — the single typed-fact-set construction the whole
/// native path shares (the shipped closure, the `DlVerdict`, `gmeow entails`, every
/// incremental session) — now pushes each quad under every spelling the new shared
/// `edb_predicate_spellings` gives it, so a `logic:subClassOf` / `logic:subPropertyOf`
/// taxonomy also enters the EDB under the `rdfs:` spelling the fixed EL/DL rules match.
/// `reason/rl.rs` was already doing exactly this privately and now calls the shared helper
/// instead of its own copy. `reason/mod.rs`, `reason/rl.rs` and `reason/el.rs` are all folded
/// into `native_contract_hash()` by `include_str!`, so the raw-source content digest moves.
/// This DOES change reasoning verdicts on canonically-spelled subsumption (previously the
/// authored edge sat inert in the EDB and derived nothing), but the change is
/// semantics-preserving in the direction that matters: the added facts are the RDFS
/// PROJECTION of asserted axioms (asserted, never derived), the authored canonical edge is
/// kept, and no rule, decider, or profile capability changed. The fixed input here is
/// edge-only and authors no subsumption edge in either spelling, so its reasoning verdict is
/// unchanged — only the identity moved.
/// Re-blessed once more for the abstract expression base LEAVING the accepted population.
/// `math:MathematicalExpression` with no concrete form beneath it is the slice's abstract
/// root; it names no production the lowering can walk and carries no content of its own, so
/// it is now `math:UnrecognizedExpressionType` in an expression position instead of interning
/// on its own node IRI. Keying it on that IRI made the structural digest a LABEL — two
/// independent authorings of one expression over undecomposed operands never reached one key
/// — and keying it on a shared opaque constant would have made two DIFFERENT undecomposed
/// operands interchangeable. `physical/lower.rs` is a `BACKWARD_SOURCE` member; the fixed
/// edge-only input carries no `math:` expression graph, so only the identity moved.
/// Re-blessed once more for reaching the datatype value-space sub-decider's facet
/// analysis from production coverage. Two wirings changed in `reason/dl.rs` (a folded
/// engine component). First, `reason/refute/datatype.rs`'s obligation discovery now
/// follows the asserted `rdfs:subClassOf` chain from an individual's types to a
/// datatype-property restriction, which is how a production ontology authors a value
/// restriction (an anonymous superclass filler on a named class, never a direct
/// `rdf:type` on the individual) — without the step the decider engaged on no
/// production obligation at all. Second, coverage now asks the sub-decider a
/// PER-OBLIGATION question (`definitively_evaluated_obligations`) rather than the
/// whole-case `decided`, whose predicate allowlist is false as soon as ordinary
/// domain vocabulary is present and so could never widen coverage on a real bundle.
/// Membership in an intersection of several constraining datatypes is also decided
/// now — it is the exact pointwise conjunction of membership in each conjunct —
/// while emptiness/cardinality under an intersection stays an honest obstruction.
/// This DOES change reasoning verdicts: an `xsd:` facet a literal actually satisfies
/// is now DECIDED instead of reported as an out-of-fragment construct, and a literal
/// that violates one produces a value-space clash the kernel materializes. The fixed
/// edge-only input carries no datatype facet, so only the identity moved.
/// Re-blessed once more for the canonical `logic:` CLASS-EXPRESSION lowering. One shared
/// table (`reason/mod.rs`'s `CALCULUS_VOCABULARY`) now maps the canonical restriction
/// vocabulary — `logic:Restriction` and its slots, plus the `logic:subClassOf` /
/// `logic:equivalentClass` anchors that attach a body to the class it constrains — onto the
/// W3C spelling the FIXED calculi name by specification. It is consumed at the typed-EDB
/// boundary (`edb_predicate_spellings`, which ADDS the projection) and at every raw-dataset
/// scan waist (`reason/dl.rs`'s `quads_by_subject` / `raw_resource_facts` and the three
/// `reason/refute/*` per-quad scans, which normalize). `reason/dl.rs`, `reason/mod.rs` and
/// `reason/refute.rs`'s module tree fold into the native contract hash, so the descriptor
/// moves. This DOES change reasoning verdicts: a class-expression body authored in the
/// canonical vocabulary previously reached only the derived SHACL surface and contributed
/// nothing to the DL/EL closure; it is now read, so `gmeow entails` decides over it. The H2
/// class-definition cardinality withhold moves with it — it is now narrowed by
/// `counting::class_definition_counting_residual`, which applies the engine's existing exact
/// `cardinality 1` carve-out to the EFFECTIVE per-class/per-property bound, so a `min 1` +
/// `max 1` pair spelled as two restriction nodes is decided exactly as the one-node `= 1`
/// spelling already was. Every other bound (one-sided, effective minimum ≥ 2, collapsed)
/// stays an honest gap, and the W3C divergence corpus is unchanged
/// (`webont-description-logic-035` still withholds). The fixed edge-only input carries no
/// class expression, so its reasoning verdict is unchanged.
/// Re-blessed for the purrdf substrate-identity fold — which subsumes the earlier RL cutover
/// that first moved the RL lane onto purrdf's `entail` chase and dropped `reason/rl_rules.rs`
/// from `native_contract_hash`'s component list. `native_contract_hash()` now folds a
/// deterministic `purrdf`-PROVIDED engine identity (`purrdf::datalog::cache::CALCULUS_VERSION`
/// plus the OWL 2 RL and datatype-entailment `calculus_program` `contract_hash`es) alongside
/// the native component source, so a purrdf pin bump that changes the moved lanes (the RL
/// chase, the DL `entail` services, the datatype value space) moves the engine seal even
/// though no native source byte moved. The native contract hash is one of the folded
/// engine-descriptor axes, so this descriptor moves. No native rule, decider, or profile
/// capability changed and the fixed edge-only input's reasoning verdict is unchanged — only
/// the identity moved to reflect the shared purrdf substrate.
/// Re-blessed once more for folding the public DL service façade (`reasoner_services.rs`) into
/// `native_contract_hash`'s component list: the OWL 2 Direct-Semantics service surface
/// (consistency, classification, realization, profile certification, module extraction) is now
/// part of the engine identity, so a change to how the façade wraps `purrdf::entail`'s services
/// moves the descriptor — a consumer holding a DL-service verdict can refuse one minted under a
/// different façade contract. This is a deliberate contract widening; no rule or decider
/// capability changed.
/// Re-blessed once more for the process-independent whole-bundle import cache. The shared
/// term arena's `ContentKey` now derives serde so that exact content identity can cross the
/// cache boundary; dense arena handles remain non-serializable and every constructor,
/// interning key, ordering, and decision path is unchanged. `term-arena/src/lib.rs` is an
/// `EXTERNAL_BACKWARD_SOURCE` member, so the raw-source pin moves even though adding derives
/// cannot change a reasoning answer. The fixed edge-only input's verdict is unchanged.
/// Re-blessed once more, EXTENDING the canonical `logic:` lowering from the
/// restriction body to the full slice-authorable TYPING + class-axiom vocabulary:
/// `CALCULUS_VOCABULARY` (`reason/mod.rs`) grows from the restriction slots to also carry
/// `logic:Class`/`ObjectProperty`/`NamedIndividual`/the property-characteristic types/
/// `disjointWith`/`inverseOf`/`unionOf`/`oneOf`/`sameAs`/`Thing`/`Nothing`/… — every
/// slice-authorable construct the DL and counting/case-split refuters read by name — and the
/// raw-dataset object position is now normalized on that table at `raw_resource_facts`,
/// `quads_by_subject` (all IRI objects, not only `rdf:type`), `scan_coverage`, and RL's
/// `encode_generic_edb`, so a marker or filler authored as `logic:` (`?P rdf:type
/// logic:TransitiveProperty`, `logic:someValuesFrom logic:Nothing`) reaches the fixed calculi.
/// The native contract hash `include_str!`s `reason/mod.rs`, `reason/dl.rs` and `reason/rl.rs`,
/// so the descriptor moves. This DOES change reasoning verdicts for a slice authored in the
/// canonical typing vocabulary (previously dark to the DL/EL/RL closure, now read), but every
/// currently-`owl:`-authored input is untouched (`calculus_term` is identity on an `owl:` IRI),
/// so the whole shipped corpus and the W3C divergence corpus are unchanged. The fixed edge-only
/// input carries no typing axiom, so its reasoning verdict is unchanged.
/// Re-blessed once more when the four fragment-completeness/boundary description strings in
/// `reason/refute.rs` were aligned from their `owl:`-prefixed spelling to the `OWL X` prose spelling
/// the slice `logic:fragmentCompletenessBound`/`logic:expressivenessBoundary` mirrors now carry (the
/// authored surface reached literal zero `owl:` tokens). `native_contract_hash()` `include_str!`s
/// `reason/refute.rs`, so its byte content moves the descriptor; no reasoning verdict changes.
/// Re-blessed once more when the reasoner's fixed calculus-vocabulary table was exposed through
/// a public `reason::calculus_vocabulary()` accessor, so the grounding cross-check reads the
/// engine's own table instead of a hand-copied 49-row mirror.
/// `native_contract_hash()` `include_str!`s `reason/mod.rs`, so adding the accessor moves the
/// descriptor by byte content alone; the `CALCULUS_VOCABULARY` data, every calculus lowering, and
/// all reasoning verdicts are unchanged.
/// Re-blessed once more when `CALCULUS_VOCABULARY` gained the two property domain/range anchors
/// (`logic:domain`→`rdfs:domain`, `logic:range`→`rdfs:range`), so a slice-authored canonical
/// `logic:domain`/`logic:range` reasoning axiom lowers onto the fixed `rdfs:` spelling the DL/RL
/// domain-range rules match — previously it passed through unnormalized and went dark. The table
/// grows 49→51 rows and `native_contract_hash()` `include_str!`s `reason/mod.rs`, so the descriptor
/// moves; every currently-`rdfs:`-authored domain/range input is untouched (`calculus_term` is
/// identity on an `rdfs:` IRI), so the shipped corpus's reasoning verdicts are unchanged.
/// Re-blessed after the finite-DL reader stopped treating `graph/logic` flat IR and
/// `graph/relational-core` reified fields as OWL syntax. The object/meta boundary changes the
/// shipped coverage verdict deliberately while leaving ordinary authored OWL/logic inputs intact.
/// Re-blessed after leave-one-out fast-family dispatch began normalizing canonical `logic:`
/// predicates and characteristic objects through the same fixed-calculus table. This removes
/// full finite-DL rebuilds for canonical disjointness/inverse/characteristic probes without
/// changing their verdicts.
/// Re-blessed after integrating current main with the complete PurRDF reasoning-substrate
/// cutover. Both sides changed folded engine-source axes, so the merged descriptor is a third
/// value measured from the combined source rather than either side's stale pin. The measurement
/// also includes the correction of `goal_directed.rs`'s public module documentation from the
/// retired native resolver to the actual PurRDF resolver/proof-checker path; that file is a raw
/// source-content identity component even though the correction does not change a verdict.
const GOLDEN_ENGINE_DESCRIPTOR_HASH: &str =
    "496c51b86e8e17a1d484bcc43be8359215faea883872391d8581163fa97ff79d";

/// Golden `SessionIdentity.descriptor_hash` over the fixed input below. A drift here is a
/// deliberate session-identity contract bump (it also moves whenever the engine, program,
/// contract, or annotation framing changes — the full seven-axis fold).
/// Re-blessed for the fragment-certified refutation kernel component registration (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded axes, so
/// the fixed-input session identity moves with it even though the reasoning verdict is unchanged.
/// Re-blessed again for Family 5 (the datatype value-space sub-decider) for the same reason: the
/// native contract hash is one of the seven folded axes and moves with the new engine source, while
/// the fixed datatype-free input's reasoning verdict is unchanged. (Post-`cargo fmt` value,
/// tracking the engine-descriptor golden above.)
/// Re-blessed again for Family 2/6a/7 (the counting / arithmetic-feasibility sub-decider) for the
/// same reason: the native contract hash is one of the seven folded axes and moves with the new
/// engine source module, while the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for Family 1/3/6b (+ entangled Family 4) — the case-split / complement /
/// union-disjoint / malformed-list sub-decider — for the same reason: the native contract hash
/// (folding the changed `reason/refute.rs` + `reason/dl.rs`) is one of the seven folded axes and
/// moves with the new engine source, while the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed for Task 6b for the same reason as the engine-descriptor golden above:
/// the native contract hash is one of the seven folded identity axes and moves with the
/// changed `reason/refute.rs` + `reason/dl.rs` engine source, while the fixed edge-only
/// input's reasoning verdict is unchanged.
/// Re-blessed once more when the coverage-gate determinism/refusal `#[cfg(test)]` tests were
/// added to `reason/refute.rs`: `native_contract_hash()` `include_str!`s the whole file, so its
/// byte content (folded into the session identity axis) moves; the fixed edge-only input's
/// reasoning verdict is unchanged and the engine descriptor hash is untouched.
/// Re-blessed once more for the G12 refutation-kernel helper consolidation (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded
/// identity axes and moves with the changed `reason/refute.rs` engine source, while the
/// fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for the DL existential-chase materialization backstop (see the
/// engine-descriptor golden above): the native contract hash folds the changed
/// `reason/dl.rs` + `physical/chase.rs` source, so the session identity moves with it, while
/// the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for the RDF 1.2 quoted-triple goal-argument grammar (see the
/// engine-descriptor golden above): the backward-source digest is one of the seven folded
/// axes and moves with the changed `query_ir`/`physical` source, while the fixed edge-only
/// input's reasoning verdict is unchanged.
/// Re-blessed once more for the reasoner-derived `math:` dimensional-homogeneity gate (see
/// the engine-descriptor golden above): the native contract hash is one of the seven folded
/// identity axes and moves with the changed `rule_ir.rs`/`query_ir.rs`/`physical/plan.rs`/
/// `physical/seminaive.rs`/`relational_core.rs` engine source, while the fixed edge-only
/// input (authoring no `logic:Constraint`) has an unchanged reasoning verdict.
/// Re-blessed once more when `physical/builtin_eval.rs`'s cell loaders were hardened to
/// require exactly one target per functional cell property (see the engine-descriptor
/// golden above): `builtin_eval.rs` is one of the folded source axes, so the fixed-input
/// session identity moves with it, while the fixed edge-only input's reasoning verdict is
/// unchanged.
/// Re-blessed once more for the `math:` expression-identity reasoned gate (see the
/// engine-descriptor golden above): `backward_source_hash` is one of the seven folded
/// identity axes and moves with the changed `physical/lower.rs` engine source
/// (a dropped `#[allow(dead_code)]`, no behavior change), while the fixed edge-only
/// input's reasoning verdict is unchanged.
/// Re-blessed once more for new `structural_digest`/`lower_math_expression` property
/// tests (see the engine-descriptor golden above): the native contract hash is one of the
/// seven folded identity axes and moves with the changed `physical/lower.rs` engine source
/// (test-only content), while the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for widening `reason::math_gate`'s module visibility and
/// `dimension_gate_markers`'s fn visibility to `pub` (see the engine-descriptor golden
/// above): the native contract hash folds the changed `reason/math_gate.rs`/`reason/mod.rs`
/// (visibility widening) and `physical/lower.rs` (fmt rewrap) engine source, while the
/// fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for removing a stray comment reference from `physical/lower.rs`
/// (see the engine-descriptor golden above): comment-only source move, no reasoning
/// verdict change.
/// Re-blessed once more for two lowering-correctness fixes (see the engine-descriptor
/// golden above): the native contract hash is one of the seven folded identity axes and
/// moves with the changed `physical/lower.rs` engine source, while the fixed edge-only
/// input (authoring no `math:` expression graph) has an unchanged reasoning verdict.
/// Re-blessed once more for a phantom-variant removal (see the engine-descriptor golden
/// above for the mechanism): `physical/lower.rs` is a `BACKWARD_SOURCE` member, so
/// deleting the unreachable `DeBruijnDistanceOverflow` / `DeBruijnSlotOverflow` variants
/// and their duplicate `intern_bound_checked_math` helper moves the native contract hash,
/// one of the seven folded identity axes, while the fixed edge-only input (authoring no
/// `math:` expression graph) has an unchanged reasoning verdict.
/// Re-blessed once more for the α-equivalence-class term reaching a production call site
/// (see the engine-descriptor golden above for the mechanism): `physical/lower.rs`'s
/// dropped `#[allow(dead_code)]` on the alpha-class minting helpers moves the native
/// contract hash, one of the seven folded
/// identity axes, while the fixed edge-only input (authoring no `math:` expression graph)
/// has an unchanged reasoning verdict.
/// Re-blessed once more for the real generating α-equivalence property-test module
/// (see the engine-descriptor golden above for the mechanism): `physical/lower.rs`'s
/// test-only `physical::lower::tests::interning` addition moves the native contract hash,
/// one of the seven folded identity axes, while the fixed edge-only input (authoring no
/// `math:` expression graph) has an unchanged reasoning verdict.
/// Re-blessed once more for the enactment-kernel gate: `reason/mod.rs` is one of the
/// folded engine-source axes and registering `reason/enactment.rs` changed its bytes, so
/// the native contract hash — and with it the engine descriptor and the fixed-input
/// session identity — moves. The fixed edge-only input authors no enactment record, so
/// its reasoning verdict is unchanged; only the identity moved.
/// Re-blessed again on integrating main, and for a different reason than the ones above: BOTH
/// sides had already moved this golden away from the merge base, so neither branch's value is
/// correct for the merged engine. The descriptor folds this branch's enactment-gate
/// registration together with main's own source changes, producing a third value that is not a
/// choice between the two. It was recomputed from the merged engine rather than resolved by
/// taking a side — taking a side here would pin a hash that no build actually produces, and the
/// test would then fail for everyone on a value that looked deliberate.
/// Re-blessed once more for the same reason as the engine descriptor above: the enactment
/// gate stopped being a stub, moving two folded engine-source axes and therefore the
/// fixed-input session identity with them.
/// Re-blessed again for the same reason as the engine descriptor above: the violation
/// rules' head tuple now names the law that drew the conclusion, moving `relational_core.rs`
/// and therefore the fixed-input session identity with it.
/// Re-blessed once more for the `inferred_axioms_to_dataset` extraction (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded
/// identity axes and moves with the changed `reason/mod.rs` source, while the fixed edge-only
/// input's reasoning verdict is unchanged.
/// Re-blessed once more for the term-arena relocation (see the engine-descriptor golden
/// above): the backward-source digest is one of the seven folded identity axes and moves
/// with the arena's new crate-relative source paths, while the fixed edge-only input's
/// reasoning verdict is unchanged.
/// Re-blessed once more for the public structured proof view (see the engine-descriptor
/// golden above): the backward-source digest is one of the seven folded identity axes and
/// moves with `physical/proof.rs`'s `pub(crate)` decoder visibility, while the fixed
/// edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for the structural key routing through the public arena facade
/// (see the engine-descriptor golden above): the backward-source digest is one of the seven
/// folded identity axes and moves with `physical/lower.rs` + `term_arena.rs`, while the
/// fixed edge-only input (authoring no `math:` expression graph) has an unchanged
/// reasoning verdict.
/// Re-blessed on integrating main, for the same reason as the engine descriptor above: both
/// sides had moved this golden away from the merge base, so the merged identity is a third
/// value recomputed from the merged engine rather than a choice between the two sides.
/// Re-blessed once more for the same reason as the engine descriptor above: the RDF 1.2
/// statement-metadata lowering adds a folded engine-source axis and the nested-triple-term
/// boundary moved `reason/refute.rs`, so the fixed-input session identity moves with them.
/// Re-blessed on integrating main. BOTH sides had moved this golden away from the merge
/// base, so neither branch's value is correct for the merged engine and taking a side would
/// pin a hash no build produces. Recomputed from the merged engine, which folds this branch's
/// expression-identity work together with main's RDF 1.2 statement-metadata lowering. Every
/// contributing change is individually verdict-preserving on the fixed edge-only input, so
/// only the identity moved.
/// Re-blessed once more for the abstract expression base joining the root population, so a
/// `math:structuralKey` authored on `math:MathematicalExpression` — the property's own declared
/// domain — is compared against a computed digest instead of skipped. `physical/lower.rs` is a
/// `BACKWARD_SOURCE` member; the fixed edge-only input carries no `math:` expression graph, so
/// only the identity moved.
/// Re-blessed once more for binder arity: a `math:BindingExpression` binds its variable over
/// its INDEXED operand sequence, which is what the slice authors ("its body through indexed
/// math:argumentSlot cells"; a `math:ModelFormula` is "a binder over indexed math:ArgumentSlot
/// operands"). The one-operand case still interns the bare body, so a `math:` binder and its
/// alpha-equivalent `logic:` quantifier still collapse to ONE node and no existing digest
/// moved; only `physical/lower.rs`'s bytes did, and it is a BACKWARD_SOURCE member.
/// Re-blessed once more for the `reason/dl.rs` comment correction (see the engine-descriptor
/// golden above): the native contract hash is one of the seven folded identity axes and
/// `native_contract_hash()` `include_str!`s the whole file, so a comment-only edit moves the
/// raw-source content digest and with it this fixed-input session identity. No engine
/// capability changed and the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for the FURTHER `reason/dl.rs` comment correction (see the
/// engine-descriptor golden above — the two notes now state the reach condition rather than
/// an unqualified "only plain cardinality restrictions in the bundle" census): the native
/// contract hash is one of the seven folded identity axes and `native_contract_hash()`
/// `include_str!`s the whole file, so a comment-only edit moves the raw-source content
/// digest and with it this fixed-input session identity. No engine capability changed and
/// the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more on integrating main into this branch, for the same reason as the
/// engine descriptor above: both sides had again moved this golden away from the merge base,
/// so the merged fixed-input session identity is a third value recomputed from the merged
/// engine rather than a choice between the two sides.
/// Re-blessed once more for the leave-one-out canonical-subsumption lowering (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded
/// identity axes and `native_contract_hash()` `include_str!`s the whole of `reason/mod.rs`,
/// so this fixed-input session identity moves with it. The fixed edge-only input authors no
/// subsumption edge in either spelling, so its reasoning verdict is unchanged.
/// Re-blessed once more on integrating main into this branch, for the same reason as the
/// engine descriptor above: both sides had again moved this golden away from the merge base,
/// so the merged fixed-input session identity is a THIRD value recomputed from the merged
/// engine rather than a choice between the two sides.
/// Re-blessed once more for the canonical-subsumption lowering at the FORWARD EDB boundary
/// (see the engine-descriptor golden above): `build_edb_facts` now pushes each quad under
/// every spelling `edb_predicate_spellings` gives it, and the native contract hash is one of
/// the seven folded identity axes, so this fixed-input session identity moves with it. The
/// fixed edge-only input authors no subsumption edge in either spelling, so its reasoning
/// verdict is unchanged.
/// Re-blessed once more for the abstract expression base LEAVING the accepted population
/// (see the engine-descriptor golden above): the native contract hash is one of the seven
/// folded identity axes and `physical/lower.rs` is a `BACKWARD_SOURCE` member, so this
/// fixed-input session identity moves with it. The fixed edge-only input carries no `math:`
/// expression graph, so its reasoning verdict is unchanged.
/// Re-blessed once more for reaching the datatype value-space sub-decider's facet analysis
/// from production coverage (see the engine-descriptor golden above): the native contract
/// hash is one of the seven folded identity axes and `native_contract_hash()`
/// `include_str!`s the whole of `reason/dl.rs`, so this fixed-input session identity moves
/// with it. The fixed edge-only input carries no datatype facet, so its reasoning verdict
/// is unchanged.
/// Re-blessed once more for the canonical `logic:` class-expression lowering (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded
/// identity axes and `native_contract_hash()` `include_str!`s the whole of `reason/mod.rs`
/// and `reason/dl.rs`, so this fixed-input session identity moves with it. The fixed
/// edge-only input authors no class expression in either spelling, so its reasoning verdict
/// is unchanged.
/// Re-blessed for the purrdf substrate-identity fold — which subsumes the earlier RL cutover
/// that dropped the `reason/rl_rules.rs` component and edited `reason/mod.rs` — for the same
/// reason as the engine-descriptor golden above: the native contract hash is one of the seven
/// folded session-identity axes and moves with the folded `purrdf`-provided engine identity
/// (the datalog `CALCULUS_VERSION` plus the OWL 2 RL and datatype-entailment calculus
/// `contract_hash`es), while the fixed edge-only input's reasoning verdict (native EL/DL) is
/// unchanged.
/// Re-blessed once more for folding the public DL service façade (`reasoner_services.rs`) into
/// `native_contract_hash`'s component list (see the engine-descriptor golden above): the native
/// contract hash is one of the seven folded session-identity axes, so this fixed-input session
/// identity moves with it. The fixed edge-only input exercises no DL service, so its reasoning
/// verdict is unchanged.
/// Re-blessed once more for the whole-bundle import cache (see the engine-descriptor note
/// above): the backward-source digest is one of the seven folded identity axes and moves
/// when `ContentKey` gains serialization derives. Those derives do not change the fixed
/// edge-only program, its canonical terms, or its reasoning verdict.
/// Re-blessed once more, extending the canonical `logic:` lowering to the full
/// typing + class-axiom vocabulary (see the engine-descriptor golden above): the native
/// contract hash is one of the seven folded identity axes and `native_contract_hash()`
/// `include_str!`s the whole of `reason/mod.rs`, `reason/dl.rs` and `reason/rl.rs`, so this
/// fixed-input session identity moves with it. The fixed edge-only input authors no typing
/// axiom in either spelling, so its reasoning verdict is unchanged. (The engine-descriptor value
/// also folds the follow-up fix that the seven property-characteristic markers lower from their
/// canonical LOWER-camel `logic:` spelling — `logic:transitiveProperty` — onto the upper-camel
/// `owl:TransitiveProperty` the RL characteristic rules match, per
/// `adapter::OWL_CHARACTERISTIC_TO_LOGIC`.)
/// Re-blessed once more for the public `reason::calculus_vocabulary()` accessor (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded identity
/// axes and it `include_str!`s `reason/mod.rs`, so this fixed-input session identity moves with the
/// added accessor even though the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for the two property domain/range calculus anchors (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded identity
/// axes and it `include_str!`s `reason/mod.rs`, so this fixed-input session identity moves with the
/// 49→51-row table even though the fixed edge-only input carries no domain/range axiom and its
/// reasoning verdict is unchanged.
/// Re-blessed for the compiled-program graph boundary above; the fixed edge-only input contains
/// neither internal graph, but its engine-contract identity deliberately moves with the boundary.
/// Re-blessed for the canonical leave-one-out fast-family dispatch above. The fixed edge-only
/// input has no such TBox axiom, so its verdict is unchanged while engine identity moves.
/// Re-blessed from the merged engine for the same combined PurRDF-cutover/current-main source
/// identity described above. The fixed edge-only input's verdict is unchanged; its session
/// identity deliberately follows the newly measured engine descriptor.
const GOLDEN_SESSION_DESCRIPTOR_HASH: &str =
    "bf1bd2c84470b585564de80cf8e7aa257fd523557af53c02b1f68b4d20295585";

#[test]
fn semver_engine_descriptor_hash_is_pinned() {
    let actual = EngineContract::current().descriptor_hash;
    assert_eq!(
        actual.len(),
        64,
        "descriptor hash is a 64-hex BLAKE3 address"
    );
    assert_eq!(
        actual, GOLDEN_ENGINE_DESCRIPTOR_HASH,
        "the engine descriptor drifted — bump the version and re-bless checkpoints"
    );
}

#[test]
fn semver_fixed_session_identity_descriptor_hash_is_pinned() {
    // A fixed, deterministic input: a fixed EDB, program, contract, and annotation. The
    // minted data-generation and all seven identity axes are pure functions of these, so
    // the folded descriptor_hash is a stable golden.
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b")]);
    let session =
        ReasoningSession::open(&edb, &projection_program(), &contract, &annotation).expect("open");
    let actual = &session.identity().descriptor_hash;
    assert_eq!(
        actual.len(),
        64,
        "descriptor hash is a 64-hex BLAKE3 address"
    );
    assert_eq!(
        actual, GOLDEN_SESSION_DESCRIPTOR_HASH,
        "the fixed-input session identity drifted — a deliberate contract bump is required"
    );
}
