// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance tests for the S6b truth-maintenance derivation graph.
//!
//! These run against the Rust derivation graph **directly**, with
//! content-addressed goldens — not a second forward chase (the native positive
//! materialization). Every assertion is over the structure built here.

use std::collections::BTreeSet;

use super::*;

// ── Fixture reifier IRIs (content keys) ────────────────────────────────────────
//
// Stand-in reifier IRIs for the fixture facts. In production these are minted by
// `mint_reifier`; here we use stable opaque IRIs so the test reads clearly. The
// derivation ids are computed from these + the rule IRI by the golden-pinned
// recipe, so they ARE content-addressed.

const R_A: &str = "https://blackcatinformatics.ca/gmeow/reifier/aaaa";
const R_B: &str = "https://blackcatinformatics.ca/gmeow/reifier/bbbb";
const R_C: &str = "https://blackcatinformatics.ca/gmeow/reifier/cccc";
const R_F: &str = "https://blackcatinformatics.ca/gmeow/reifier/ffff";
const RULE_R1: &str = "https://blackcatinformatics.ca/logic/rules/r1";

const UNIT_A: &str = "https://blackcatinformatics.ca/slice/A";
const UNIT_C: &str = "https://blackcatinformatics.ca/slice/C";

fn fk(s: &str) -> FactKey {
    FactKey(s.to_owned())
}
fn uk(s: &str) -> UnitKey {
    UnitKey(s.to_owned())
}
fn unit_set(units: &[&str]) -> BTreeSet<UnitKey> {
    units.iter().map(|u| uk(u)).collect()
}
fn no_rules() -> BTreeSet<String> {
    BTreeSet::new()
}

// ── RuleApplication content addressing (golden) ────────────────────────────────

/// The derivation id is `mint_derivation_id(rule_iri, sorted(premise reifiers))`.
/// Pinned to the Python-oracle recipe (sha1 over `rule\nsorted-joined-premises`).
#[test]
fn rule_application_derivation_id_golden() {
    let app = RuleApplication::new(RULE_R1, [fk(R_B), fk(R_C)]);
    assert_eq!(
        app.derivation_id(),
        "https://blackcatinformatics.ca/gmeow/derivation/4a9f5bbc9e6b7a5925e0c9f9248c274b0697b5ad",
        "derivation-id golden mismatch"
    );
}

/// Premise order does not change the content-addressed derivation id (the premises
/// are sorted both in `RuleApplication::new` and in `mint_derivation_id`).
#[test]
fn rule_application_derivation_id_order_independent() {
    let fwd = RuleApplication::new(RULE_R1, [fk(R_B), fk(R_C)]);
    let rev = RuleApplication::new(RULE_R1, [fk(R_C), fk(R_B)]);
    assert_eq!(fwd, rev, "RuleApplication must be order-independent");
    assert_eq!(fwd.derivation_id(), rev.derivation_id());
}

// ── Acceptance: deletion survival ──────────────────────────────────────────────
//
// Fact F is derivable two ways:
//   (1) asserted in slice A, AND
//   (2) derivable via rule r1 from premises in slice C (B and C are asserted in C).
// Remove slice A's assertion -> F SURVIVES (rule-based justification still holds).
// Remove the rule's premises too -> F no longer derivable.

fn survival_fixture() -> DerivationGraph {
    let mut g = DerivationGraph::new();
    // Premises B and C are asserted by slice C.
    g.add_assertion(fk(R_B), uk(UNIT_C));
    g.add_assertion(fk(R_C), uk(UNIT_C));
    // F is asserted by slice A (justification 1).
    g.add_assertion(fk(R_F), uk(UNIT_A));
    // F is also derivable via r1 from {B, C} (justification 2).
    g.add_derivation(fk(R_F), RuleApplication::new(RULE_R1, [fk(R_B), fk(R_C)]))
        .expect("r1 firing is not self-referential");
    g
}

#[test]
fn deletion_survival_full_graph_derives_everything() {
    let g = survival_fixture();
    let all = g.all_derivable();
    let expected: BTreeSet<FactKey> = [fk(R_B), fk(R_C), fk(R_F)].into_iter().collect();
    assert_eq!(
        all, expected,
        "all four facts derivable with nothing removed"
    );
}

#[test]
fn deletion_survival_remove_slice_a_f_survives() {
    let g = survival_fixture();
    // Remove slice A's assertion. F's asserted justification dies, but the
    // r1-from-{B,C} justification still holds (B, C asserted by slice C).
    let surviving = g.survives(&unit_set(&[UNIT_A]), &no_rules());
    let expected: BTreeSet<FactKey> = [fk(R_B), fk(R_C), fk(R_F)].into_iter().collect();
    assert_eq!(
        surviving, expected,
        "F must SURVIVE removal of slice A via its rule justification"
    );
    assert!(surviving.contains(&fk(R_F)), "F survives");
}

#[test]
fn deletion_survival_remove_a_and_premises_f_dies() {
    let g = survival_fixture();
    // Remove slice A AND slice C (the premises B, C). Now F has neither a surviving
    // assertion nor a satisfiable rule firing.
    let surviving = g.survives(&unit_set(&[UNIT_A, UNIT_C]), &no_rules());
    assert!(
        surviving.is_empty(),
        "with slice A and the premises removed, NOTHING is derivable, got {surviving:?}"
    );
    assert!(!surviving.contains(&fk(R_F)), "F is no longer derivable");
}

#[test]
fn deletion_survival_remove_rule_f_falls_back_to_assertion() {
    let g = survival_fixture();
    // Remove the rule r1 but keep slice A. F's rule justification dies, but its
    // asserted-in-A justification survives.
    let mut removed_rules = BTreeSet::new();
    removed_rules.insert(RULE_R1.to_owned());
    let surviving = g.survives(&BTreeSet::new(), &removed_rules);
    assert!(
        surviving.contains(&fk(R_F)),
        "F survives via slice-A assertion"
    );
    // And removing the rule AND slice A kills F (B, C still asserted in C).
    let surviving2 = g.survives(&unit_set(&[UNIT_A]), &removed_rules);
    assert!(
        !surviving2.contains(&fk(R_F)),
        "F dies with both rule and A gone"
    );
    let expected2: BTreeSet<FactKey> = [fk(R_B), fk(R_C)].into_iter().collect();
    assert_eq!(surviving2, expected2);
}

// ── Acceptance: self-attestation guard ─────────────────────────────────────────
//
// A generated/analysis fact cannot appear as a premise of its own RuleApplication.

#[test]
fn self_attestation_direct_self_premise_rejected() {
    let mut g = DerivationGraph::new();
    let app = RuleApplication::new(RULE_R1, [fk(R_F), fk(R_B)]); // F among its own premises
    let err = g
        .add_derivation(fk(R_F), app)
        .expect_err("a fact listing itself as a premise must be rejected");
    assert!(
        err.message().contains("self-attestation"),
        "error must name the guard: {err}"
    );
    assert!(g.is_empty(), "no justification recorded on rejection");
}

/// Consumer audit: the disjunctive OR-of-AND is PRESERVED, not collapsed. A fact provable
/// two INDEPENDENT ways (distinct rules AND distinct premise sets) must retain BOTH `Derived`
/// alternatives in `justifications_of` — a single-winner collapse would lose the disjunction
/// deletion-survival depends on. The companion assertion pins the self-attestation hard-fail.
#[test]
fn two_independent_derivations_preserve_the_or_of_and() {
    let mut g = DerivationGraph::new();
    // Two DISTINCT firings of F: rule r1 from {B, C}, and rule ext from {A}. Different rule
    // AND different premises — not the same premises reordered (which would dedup to one).
    let app1 = RuleApplication::new(RULE_R1, [fk(R_B), fk(R_C)]);
    let app2 = RuleApplication::new(RULE_EXT, [fk(R_A)]);
    assert_ne!(app1, app2, "the two derivations must be genuinely distinct");
    g.add_derivation(fk(R_F), app1.clone())
        .expect("r1 firing is not self-referential");
    g.add_derivation(fk(R_F), app2.clone())
        .expect("ext firing is not self-referential");

    let alternatives = g
        .justifications_of(&fk(R_F))
        .expect("F must have recorded justifications");
    assert!(
        alternatives.contains(&Justification::Derived(app1)),
        "the r1-from-{{B,C}} derivation must be retained"
    );
    assert!(
        alternatives.contains(&Justification::Derived(app2)),
        "the ext-from-{{A}} derivation must be retained"
    );
    let derived_count = alternatives
        .iter()
        .filter(|j| matches!(j, Justification::Derived(_)))
        .count();
    assert_eq!(
        derived_count, 2,
        "BOTH disjuncts survive — the OR-of-AND is not collapsed to a single winner"
    );

    // A premise list containing the fact itself is a hard self-attestation error.
    let self_app = RuleApplication::new(RULE_R1, [fk(R_F), fk(R_B)]);
    let err = g
        .add_derivation(fk(R_F), self_app)
        .expect_err("a fact listing itself as a premise must be rejected");
    assert!(
        err.message().contains("self-attestation"),
        "the error must name the self-attestation guard: {err}"
    );
}

#[test]
fn self_attestation_independent_facts_allowed() {
    // F derived from B (not itself) is fine, even if B is also derivable from F:
    // mutual support across DISTINCT facts is permitted in the structure.
    let mut g = DerivationGraph::new();
    g.add_derivation(fk(R_F), RuleApplication::new(RULE_R1, [fk(R_B)]))
        .expect("F<-B is not self-referential");
    g.add_derivation(fk(R_B), RuleApplication::new(RULE_R1, [fk(R_F)]))
        .expect("B<-F is not self-referential");
    // But with NO independent base, the mutual cycle is NOT derivable (least
    // fixpoint never bootstraps a cycle from nothing).
    assert!(
        g.all_derivable().is_empty(),
        "a pure F<->B cycle with no base must derive nothing"
    );
    // Ground the cycle: assert F. Now both become derivable.
    g.add_assertion(fk(R_F), uk(UNIT_A));
    let all = g.all_derivable();
    assert!(
        all.contains(&fk(R_F)) && all.contains(&fk(R_B)),
        "grounded cycle derives both"
    );
}

// ── Acceptance: runtime-id independence (golden) ───────────────────────────────
//
// Build the SAME logical graph two ways: once inserting facts/justifications in
// one order, once in a different order (simulating different interner-id
// assignments). The content-addressed derivation ids and the whole-graph digest
// must be IDENTICAL — numeric runtime ids never enter the hashes.

fn build_graph_order_1() -> DerivationGraph {
    let mut g = DerivationGraph::new();
    g.add_assertion(fk(R_A), uk(UNIT_A));
    g.add_assertion(fk(R_B), uk(UNIT_C));
    g.add_assertion(fk(R_C), uk(UNIT_C));
    g.add_derivation(fk(R_F), RuleApplication::new(RULE_R1, [fk(R_B), fk(R_C)]))
        .unwrap();
    g
}

fn build_graph_order_2() -> DerivationGraph {
    // Reverse insertion order + reversed premise order: a different "interner-id"
    // assignment in spirit. Must produce a byte-identical graph.
    let mut g = DerivationGraph::new();
    g.add_derivation(fk(R_F), RuleApplication::new(RULE_R1, [fk(R_C), fk(R_B)]))
        .unwrap();
    g.add_assertion(fk(R_C), uk(UNIT_C));
    g.add_assertion(fk(R_B), uk(UNIT_C));
    g.add_assertion(fk(R_A), uk(UNIT_A));
    g
}

#[test]
fn runtime_id_independence_graph_digest_golden() {
    let g1 = build_graph_order_1();
    let g2 = build_graph_order_2();

    // The two graphs are structurally equal (BTreeMap/BTreeSet content order).
    assert_eq!(g1, g2, "insertion order must not change the graph");

    // The content digest is identical and golden-pinned.
    let d1 = g1.content_digest();
    let d2 = g2.content_digest();
    assert_eq!(d1, d2, "content digest must be runtime-id independent");

    // The F derivation id is identical across both orderings.
    let app1 = RuleApplication::new(RULE_R1, [fk(R_B), fk(R_C)]);
    let app2 = RuleApplication::new(RULE_R1, [fk(R_C), fk(R_B)]);
    assert_eq!(app1.derivation_id(), app2.derivation_id());
    assert_eq!(
        app1.derivation_id(),
        "https://blackcatinformatics.ca/gmeow/derivation/4a9f5bbc9e6b7a5925e0c9f9248c274b0697b5ad",
        "F derivation id golden mismatch"
    );
}

// ── Acceptance: incremental == clean rebuild ───────────────────────────────────
//
// Over a fixture with CYCLIC core dependencies (Y<->Z mutually derive) AND an
// ext+core product, incrementally applying add/modify/delete yields a graph whose
// surviving-fact set + derivation ids EXACTLY equal a clean rebuild's.

// Reifier IRIs for the incremental fixture.
const R_X: &str = "https://blackcatinformatics.ca/gmeow/reifier/xxxx"; // core, base
const R_Y: &str = "https://blackcatinformatics.ca/gmeow/reifier/yyyy"; // core, cyclic
const R_Z: &str = "https://blackcatinformatics.ca/gmeow/reifier/zzzz"; // core, cyclic
const R_P: &str = "https://blackcatinformatics.ca/gmeow/reifier/pppp"; // ext product
const RULE_CORE: &str = "https://blackcatinformatics.ca/logic/rules/core";
const RULE_EXT: &str = "https://blackcatinformatics.ca/logic/rules/ext";
const UNIT_CORE: &str = "https://blackcatinformatics.ca/slice/core";
const UNIT_EXT: &str = "https://blackcatinformatics.ca/slice/ext";

/// The FINAL desired state, built from scratch ("clean rebuild").
///
/// - X asserted by core (the base that grounds the cycle).
/// - Y is derivable two ways: from {X} (grounds the cycle, non-cyclic) AND from
///   {Z} (the cyclic back-edge). Z is derivable from {Y}. So {Y, Z} are mutually
///   dependent (a cyclic CORE dependency) yet grounded by X via Y's {X} firing.
/// - P (ext product) derived from {Y} via the ext rule, AND asserted by ext.
fn rebuild_final() -> DerivationGraph {
    let mut g = DerivationGraph::new();
    g.add_assertion(fk(R_X), uk(UNIT_CORE));
    // Y has two firings: the grounding {X} firing and the cyclic {Z} firing.
    g.add_derivation(fk(R_Y), RuleApplication::new(RULE_CORE, [fk(R_X)]))
        .unwrap();
    g.add_derivation(fk(R_Y), RuleApplication::new(RULE_CORE, [fk(R_Z)]))
        .unwrap();
    // Z is derived only from Y (cyclic dependency on Y).
    g.add_derivation(fk(R_Z), RuleApplication::new(RULE_CORE, [fk(R_Y)]))
        .unwrap();
    g.add_derivation(fk(R_P), RuleApplication::new(RULE_EXT, [fk(R_Y)]))
        .unwrap();
    g.add_assertion(fk(R_P), uk(UNIT_EXT));
    g
}

/// Reach the same final state by an add/modify/delete *sequence* over an initial
/// graph, mirroring how an incremental engine would patch the graph.
fn incremental_to_final() -> DerivationGraph {
    let mut g = DerivationGraph::new();

    // --- initial build (a DIFFERENT, stale state) ---
    g.add_assertion(fk(R_X), uk(UNIT_CORE));
    // stale: Y derived from {X} only (missing the cyclic {Z} firing — will be
    // "modified" to ADD the back-edge).
    g.add_derivation(fk(R_Y), RuleApplication::new(RULE_CORE, [fk(R_X)]))
        .unwrap();
    // stale: Z derived from {X, Y} (wrong premises — will be "modified" to {Y}).
    g.add_derivation(fk(R_Z), RuleApplication::new(RULE_CORE, [fk(R_X), fk(R_Y)]))
        .unwrap();
    // stale: P derived from {Z} (wrong premise — will be "modified" to {Y}); plus a
    // STALE extra ext assertion that we will DELETE and re-add to its correct unit.
    g.add_derivation(fk(R_P), RuleApplication::new(RULE_EXT, [fk(R_Z)]))
        .unwrap();
    let stale_unit = uk("https://blackcatinformatics.ca/slice/stale-ext");
    g.add_assertion(fk(R_P), stale_unit.clone());

    // --- incremental MODIFY (add a firing): Y now also derivable from {Z} ---
    g.replace_derivations(
        &fk(R_Y),
        vec![
            RuleApplication::new(RULE_CORE, [fk(R_X)]),
            RuleApplication::new(RULE_CORE, [fk(R_Z)]),
        ],
    )
    .unwrap();

    // --- incremental MODIFY: Z's premises {X, Y} -> {Y} ---
    g.replace_derivations(&fk(R_Z), vec![RuleApplication::new(RULE_CORE, [fk(R_Y)])])
        .unwrap();

    // --- incremental MODIFY: P's ext derivation premises {Z} -> {Y} ---
    g.replace_derivations(&fk(R_P), vec![RuleApplication::new(RULE_EXT, [fk(R_Y)])])
        .unwrap();

    // --- incremental DELETE the stale ext assertion, ADD the correct one ---
    g.remove_assertion(&fk(R_P), &stale_unit);
    g.add_assertion(fk(R_P), uk(UNIT_EXT));

    g
}

#[test]
fn incremental_equals_clean_rebuild() {
    let rebuilt = rebuild_final();
    let incremental = incremental_to_final();

    // The graphs are structurally identical.
    assert_eq!(
        incremental, rebuilt,
        "incremental graph must equal clean rebuild"
    );
    // Same content digest (derivation ids fold in).
    assert_eq!(
        incremental.content_digest(),
        rebuilt.content_digest(),
        "incremental and rebuild content digests must match"
    );

    // Same surviving-fact set with nothing removed (the cycle is grounded by X).
    let all_rebuild = rebuilt.all_derivable();
    let all_incr = incremental.all_derivable();
    let expected: BTreeSet<FactKey> = [fk(R_X), fk(R_Y), fk(R_Z), fk(R_P)].into_iter().collect();
    assert_eq!(all_rebuild, expected, "rebuild derives the full closure");
    assert_eq!(all_incr, expected, "incremental derives the full closure");

    // Same survival behaviour under a deletion: remove the core base X. The cycle
    // {Y, Z} loses its only ground, so Y and Z die; P survives via its ext
    // assertion only.
    let removed = unit_set(&[UNIT_CORE]);
    let surv_rebuild = rebuilt.survives(&removed, &no_rules());
    let surv_incr = incremental.survives(&removed, &no_rules());
    assert_eq!(surv_rebuild, surv_incr, "survival sets must match");
    let expected_surv: BTreeSet<FactKey> = [fk(R_P)].into_iter().collect();
    assert_eq!(
        surv_rebuild, expected_surv,
        "removing core base X collapses the cycle; only P (ext-asserted) survives"
    );
}

#[test]
fn cyclic_core_without_base_is_not_self_justifying() {
    // Y<->Z mutual derivation with NO asserted base must derive nothing — the
    // truth-maintenance least fixpoint never bootstraps a cycle.
    let mut g = DerivationGraph::new();
    g.add_derivation(fk(R_Y), RuleApplication::new(RULE_CORE, [fk(R_Z)]))
        .unwrap();
    g.add_derivation(fk(R_Z), RuleApplication::new(RULE_CORE, [fk(R_Y)]))
        .unwrap();
    assert!(
        g.all_derivable().is_empty(),
        "an ungrounded cycle must derive nothing"
    );
}

// ── Construction from a foundation chase ───────────────────────────────────────

#[test]
fn from_foundation_quads_builds_assertions_and_derivations() {
    use crate::foundation::{ASSERT_RULE_IRI, FoundationQuad};

    // An asserted quad and a derived quad whose source is the asserted quad's
    // reifier. We compute the asserted reifier via the public helper so the
    // derived quad's premise references the right key.
    let asserted = FoundationQuad {
        graph: "http://world/base".to_owned(),
        subject: "http://ex/a".to_owned(),
        predicate: "http://ex/p".to_owned(),
        object: "<http://ex/b>".to_owned(),
        rule_iri: ASSERT_RULE_IRI.to_owned(),
        source_quad_ids: vec![],
        derivation_id: String::new(),
    };
    let asserted_reifier = crate::foundation::quad_reifier(&asserted).unwrap();

    let derived = FoundationQuad {
        graph: "http://world/base".to_owned(),
        subject: "http://ex/a".to_owned(),
        predicate: "http://ex/q".to_owned(),
        object: "<http://ex/c>".to_owned(),
        rule_iri: "https://blackcatinformatics.ca/logic/rule/anonymous".to_owned(),
        source_quad_ids: vec![asserted_reifier.clone()],
        derivation_id: String::new(),
    };
    let derived_reifier = crate::foundation::quad_reifier(&derived).unwrap();

    let g = super::from_foundation_quads(&[asserted.clone(), derived]).unwrap();

    // The asserted quad has an Asserted justification keyed on its world IRI.
    let aj = g
        .justifications_of(&FactKey(asserted_reifier.clone()))
        .unwrap();
    assert!(aj.iter().any(|j| matches!(
        j,
        Justification::Asserted { unit } if unit.as_str() == "http://world/base"
    )));

    // The derived quad has a Derived justification with the asserted reifier as a premise.
    let dj = g
        .justifications_of(&FactKey(derived_reifier.clone()))
        .unwrap();
    assert!(dj.iter().any(|j| matches!(
        j,
        Justification::Derived(app)
            if app.premises.len() == 1 && app.premises[0].as_str() == asserted_reifier
    )));

    // Both facts derivable; removing the asserting world kills both.
    assert_eq!(g.all_derivable().len(), 2);
    let removed = unit_set(&["http://world/base"]);
    assert!(g.survives(&removed, &no_rules()).is_empty());
}

#[test]
fn from_foundation_quads_rejects_self_referential_derivation() {
    use crate::foundation::FoundationQuad;
    // A derived quad that lists its OWN reifier as a source must be rejected.
    let mut q = FoundationQuad {
        graph: "http://world/base".to_owned(),
        subject: "http://ex/a".to_owned(),
        predicate: "http://ex/q".to_owned(),
        object: "<http://ex/c>".to_owned(),
        rule_iri: "https://blackcatinformatics.ca/logic/rule/anonymous".to_owned(),
        source_quad_ids: vec![],
        derivation_id: String::new(),
    };
    let self_reifier = crate::foundation::quad_reifier(&q).unwrap();
    q.source_quad_ids = vec![self_reifier];
    let err =
        super::from_foundation_quads(&[q]).expect_err("self-referential quad must be rejected");
    assert!(err.message().contains("self-attestation"), "got: {err}");
}
