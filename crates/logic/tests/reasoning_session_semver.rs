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
const GOLDEN_ENGINE_DESCRIPTOR_HASH: &str =
    "42bb1f0982885c07b9ba016aa84e39c9d25740d78e5c98573458b8f9623cecf9";

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
/// Re-blessed once more for the term-arena relocation (see the engine-descriptor golden
/// above): the backward-source digest is one of the seven folded identity axes and moves
/// with the arena's new crate-relative source paths, while the fixed edge-only input's
/// reasoning verdict is unchanged.
/// Re-blessed once more for the public structured proof view (see the engine-descriptor
/// golden above): the backward-source digest is one of the seven folded identity axes and
/// moves with `physical/proof.rs`'s `pub(crate)` decoder visibility, while the fixed
/// edge-only input's reasoning verdict is unchanged.
/// Re-blessed on integrating main, for the same reason as the engine descriptor above: both
/// sides had moved this golden away from the merge base, so the merged identity is a third
/// value recomputed from the merged engine rather than a choice between the two sides.
/// Re-blessed once more for the same reason as the engine descriptor above: the RDF 1.2
/// statement-metadata lowering adds a folded engine-source axis and the nested-triple-term
/// boundary moved `reason/refute.rs`, so the fixed-input session identity moves with them.
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
const GOLDEN_SESSION_DESCRIPTOR_HASH: &str =
    "9e3d2ab3324cc52331b95c0bc9fb5eac73969372de3c770053d429dec523134f";

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
