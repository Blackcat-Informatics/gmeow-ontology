// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the native foundation evaluator.
//!
//! These tests build N-Quads inputs mirroring the `conformance/logic/cases/foundation/`
//! cases, run [`evaluate`], and assert (a) the materialized quad SET against the
//! goldens, and (b) the content-addressed provenance on representative quads.  Full
//! end-to-end explanation-golden parity is validated after the runner is rewired
//! (Task 2); here we pin the quad set + provenance recipe.

use super::*;

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE_P: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_FUNCTIONAL: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";

/// Build a `WorldStore` from N-Quads text.
fn store_from(nquads: &str) -> WorldStore {
    let store = WorldStore::new();
    store.load_nquads(nquads).expect("valid N-Quads");
    store
}

/// Run the evaluator and return its quads.
fn run(nquads: &str, policy: AntiRigidityPolicy) -> Vec<FoundationQuad> {
    evaluate(&store_from(nquads), policy).expect("evaluate must succeed")
}

/// The set of `(subject, predicate, object_n3, graph)` tuples for a quad list — the
/// materialized quad SET, ignoring provenance.
fn quad_set(
    quads: &[FoundationQuad],
) -> std::collections::BTreeSet<(String, String, String, String)> {
    quads
        .iter()
        .map(|q| {
            (
                q.subject.clone(),
                q.predicate.clone(),
                q.object.clone(),
                q.graph.clone(),
            )
        })
        .collect()
}

/// Parse an `expected/materialized.nq` golden into the same `(s, p, o_n3, g)` set.
///
/// The golden lines are N-Quads `<s> <p> <o> <g> .` — the object is already in N3
/// `<iri>` form, which is exactly how [`FoundationQuad::object`] is stored.
fn golden_set(nq: &str) -> std::collections::BTreeSet<(String, String, String, String)> {
    let mut out = std::collections::BTreeSet::new();
    for line in nq.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Strip trailing " ." and split into the four angle-bracketed terms.
        let body = line.strip_suffix('.').unwrap_or(line).trim();
        let terms: Vec<&str> = split_nq_terms(body);
        assert_eq!(terms.len(), 4, "golden line must have 4 terms: {line:?}");
        let s = strip_angle(terms[0]).to_owned();
        let p = strip_angle(terms[1]).to_owned();
        let o = terms[2].to_owned(); // keep N3 form
        let g = strip_angle(terms[3]).to_owned();
        out.insert((s, p, o, g));
    }
    out
}

/// Split a `<a> <b> <c> <d>` line into its angle-bracketed terms (all IRIs here).
fn split_nq_terms(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            // `&body[start..=i]` would panic with an opaque out-of-bounds slice if
            // the '<' were never closed; assert with a clear message instead.
            assert!(
                i < bytes.len(),
                "malformed N-Quads term: unclosed '<' at byte {start} in {body:?}"
            );
            // include the '>'
            out.push(&body[start..=i]);
        }
        i += 1;
    }
    out
}

/// Find the single quad matching `(subject, predicate, object_n3)` in named graph
/// `graph`.  Panics unless EXACTLY one quad matches: matching is graph-scoped and
/// uniqueness-checked so a multi-world regression that emits the same `(s, p, o)`
/// in more than one graph cannot be silently masked by a first-match lookup.
fn find<'a>(
    quads: &'a [FoundationQuad],
    graph: &str,
    subject: &str,
    predicate: &str,
    object_n3: &str,
) -> &'a FoundationQuad {
    let matches: Vec<&'a FoundationQuad> = quads
        .iter()
        .filter(|q| {
            q.graph == graph
                && q.subject == subject
                && q.predicate == predicate
                && q.object == object_n3
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one quad {subject} {predicate} {object_n3} in graph {graph}, \
         found {}; have: {quads:#?}",
        matches.len()
    );
    matches[0]
}

/// Whether a violation `(subject, label_local)` is present.
fn has_violation(quads: &[FoundationQuad], subject: &str, label: &str) -> bool {
    let pred = format!("{LOGIC}violation");
    let obj = format!("<{LOGIC}{label}>");
    quads
        .iter()
        .any(|q| q.subject == subject && q.predicate == pred && q.object == obj)
}

// ── Discipline: StereotypeCardinality (0 and >1 stereotypes) ────────────────────

#[test]
fn stereotype_cardinality_zero_and_two() {
    // Mirrors conformance exactly-one-stereotype: Anchor=Kind, NoStereo (subClassOf
    // Anchor, no stereotype), TwoStereo=Kind+Role.
    let base = "https://example.org/foundation/exactly-one-stereotype";
    let nq = format!(
        "<{base}/Anchor> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/NoStereo> <{LOGIC}subClassOf> <{base}/Anchor> <{base}/schema> .\n\
         <{base}/TwoStereo> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/TwoStereo> <{RDF_TYPE_P}> <{LOGIC}Role> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    // 0 stereotypes: NoStereo is a class (subClassOf subject) with no meta-class.
    assert!(
        has_violation(&quads, &format!("{base}/NoStereo"), "StereotypeCardinality"),
        "NoStereo (0 stereotypes) must fire StereotypeCardinality"
    );
    // >1 stereotype: TwoStereo carries Kind AND Role.
    assert!(
        has_violation(
            &quads,
            &format!("{base}/TwoStereo"),
            "StereotypeCardinality"
        ),
        "TwoStereo (2 stereotypes) must fire StereotypeCardinality"
    );
    // Anchor has exactly one stereotype → no cardinality violation.
    assert!(
        !has_violation(&quads, &format!("{base}/Anchor"), "StereotypeCardinality"),
        "Anchor (exactly one stereotype) must NOT fire StereotypeCardinality"
    );
}

// ── Discipline: MixIden (Kind under Kind) ───────────────────────────────────────

#[test]
fn mixiden_kind_under_kind() {
    let base = "https://example.org/foundation/identity-overlap-mixiden";
    let nq = format!(
        "<{base}/Animal> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/Dog> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/Dog> <{LOGIC}subClassOf> <{base}/Animal> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    // Dog is a Kind with a Kind proper-ancestor (Animal) → MixIden.
    assert!(
        has_violation(&quads, &format!("{base}/Dog"), "MixIden"),
        "Dog (Kind under Kind) must fire MixIden"
    );
}

// ── S6b: chase → derivation graph wiring ─────────────────────────────────

#[test]
fn derivation_graph_chase_wiring_and_survival() {
    use crate::derivation_graph::FactKey;
    use std::collections::BTreeSet;

    // Same fixture as `mixiden_kind_under_kind`: a single-world schema whose facts
    // are all asserted by that one world, plus derived helper/violation facts.
    let base = "https://example.org/foundation/dg-wiring";
    let nq = format!(
        "<{base}/Animal> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/Dog> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/Dog> <{LOGIC}subClassOf> <{base}/Animal> <{base}/schema> .\n"
    );
    let store = store_from(&nq);
    let quads =
        evaluate(&store, AntiRigidityPolicy::WitnessObligation).expect("evaluate must succeed");
    let graph = super::derivation_graph(&store, AntiRigidityPolicy::WitnessObligation)
        .expect("derivation_graph must succeed");

    // The graph records exactly one fact per materialized quad reifier.
    let reifiers: BTreeSet<FactKey> = quads
        .iter()
        .map(|q| FactKey(super::quad_reifier(q).unwrap()))
        .collect();
    assert_eq!(
        graph.len(),
        reifiers.len(),
        "one fact per materialized quad"
    );

    // With nothing removed, every materialized fact is derivable (every derived
    // fact's premises trace back to the asserted base in the same world).
    let all = graph.all_derivable();
    assert_eq!(
        all, reifiers,
        "full closure derives every materialized quad"
    );

    // The derived MixIden violation fact is present and is derivable (not asserted).
    let viol = FoundationQuad {
        graph: format!("{base}/schema"),
        subject: format!("{base}/Dog"),
        predicate: format!("{LOGIC}violation"),
        object: format!("<{LOGIC}MixIden>"),
        rule_iri: String::new(),
        source_quad_ids: vec![],
        derivation_id: String::new(),
    };
    let viol_key = FactKey(super::quad_reifier(&viol).unwrap());
    assert!(
        all.contains(&viol_key),
        "the MixIden violation must be derivable"
    );

    // Remove the sole asserting world unit → the whole closure collapses (no base
    // assertion survives, so nothing — derived or asserted — remains derivable).
    let mut removed_units = BTreeSet::new();
    removed_units.insert(crate::derivation_graph::UnitKey(format!("{base}/schema")));
    let surviving = graph.survives(&removed_units, &BTreeSet::new());
    assert!(
        surviving.is_empty(),
        "removing the only asserting world collapses the whole closure, got {} facts",
        surviving.len()
    );

    // Determinism: a second run over a freshly-built store yields an identical graph
    // (content-addressed, runtime-id independent — preserves the order-stable fold).
    let store2 = store_from(&nq);
    let graph2 = super::derivation_graph(&store2, AntiRigidityPolicy::WitnessObligation)
        .expect("second derivation_graph must succeed");
    assert_eq!(
        graph, graph2,
        "derivation graph must be deterministic across runs"
    );
    assert_eq!(graph.content_digest(), graph2.content_digest());
}

// ── Discipline: FreeRole (bare Role) ────────────────────────────────────────────

#[test]
fn freerole_bare_role() {
    let base = "https://example.org/foundation/free-role";
    let nq = format!("<{base}/Wanderer> <{RDF_TYPE_P}> <{LOGIC}Role> <{base}/schema> .\n");
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    // Wanderer is an anti-rigid sortal (Role) with no rigid ancestor → FreeRole.
    assert!(
        has_violation(&quads, &format!("{base}/Wanderer"), "FreeRole"),
        "bare Role must fire FreeRole"
    );
    // And, being a non-Kind sortal with no Kind ancestor, also MixIden.
    assert!(
        has_violation(&quads, &format!("{base}/Wanderer"), "MixIden"),
        "bare Role (non-Kind sortal, no Kind ancestor) must fire MixIden"
    );
}

// ── Discipline: MixRig (SubKind under Role) — the parity anchor ──────────────────

const MIXRIG_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/foundation/mixrig-kind-under-role/expected/materialized.nq"
);

#[test]
fn mixrig_full_quad_set_matches_golden() {
    let base = "https://example.org/foundation/mixrig-kind-under-role";
    let nq = format!(
        "<{base}/HonorsStudent> <{RDF_TYPE_P}> <{LOGIC}SubKind> <{base}/schema> .\n\
         <{base}/HonorsStudent> <{LOGIC}subClassOf> <{base}/Student> <{base}/schema> .\n\
         <{base}/Student> <{RDF_TYPE_P}> <{LOGIC}Role> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    let got = quad_set(&quads);
    let want = golden_set(MIXRIG_GOLDEN);
    assert_eq!(
        got, want,
        "mixrig materialized quad SET must match the golden byte-for-byte"
    );
}

// ── Discipline: RelComp (under-mediated relator) ────────────────────────────────

#[test]
fn relcomp_under_mediated() {
    let base = "https://example.org/foundation/relcomp-under-mediated";
    let nq = format!(
        // Employment mediates two distinct roles → two ends → well-formed.
        "<{base}/Employment> <{RDF_TYPE_P}> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Employment> <{LOGIC}mediates> <{base}/employee> <{base}/schema> .\n\
         <{base}/Employment> <{LOGIC}mediates> <{base}/employer> <{base}/schema> .\n\
         \
         <{base}/Marriage> <{RDF_TYPE_P}> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Marriage> <{LOGIC}mediates> <{base}/spouse> <{base}/schema> .\n\
         <{base}/spouse> <{RDF_TYPE_P}> <{OWL_FUNCTIONAL}> <{base}/schema> .\n\
         \
         <{base}/Partnership> <{RDF_TYPE_P}> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Partnership> <{LOGIC}mediates> <{base}/partner> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    // Marriage mediates a single FUNCTIONAL role → one end → RelComp.
    assert!(
        has_violation(&quads, &format!("{base}/Marriage"), "RelComp"),
        "Marriage (one functional role → one end) must fire RelComp"
    );
    // Employment mediates two distinct roles → two ends → no RelComp.
    assert!(
        !has_violation(&quads, &format!("{base}/Employment"), "RelComp"),
        "Employment (two distinct roles) must NOT fire RelComp"
    );
    // Partnership mediates a single NON-functional role → two ends → no RelComp.
    // The entity-count reading: one non-functional role already reaches two entities.
    assert!(
        !has_violation(&quads, &format!("{base}/Partnership"), "RelComp"),
        "Partnership (one non-functional role → two ends) must NOT fire RelComp"
    );
}

// ── Discipline: RelComp on production `subClassOf logic:Relator` relators ────────

/// Production relators are typed `a logic:Kind ; rdfs:subClassOf logic:Relator` (a
/// class-edge to the meta-class), NOT the direct `a logic:Relator` the base
/// [`relcomp_under_mediated`] case uses.  These assertions pin that the discipline is
/// live on that production shape (previously it was wholly dormant on it): it fires on
/// an under-mediated subclass relator, clears on a well-mediated one, and never fires
/// on the `logic:Relator` meta-class itself.
#[test]
fn relcomp_on_subclass_relator() {
    let base = "https://example.org/foundation/relcomp-subclass-relator";
    let nq = format!(
        // Enrollment: a Kind subClassOf logic:Relator mediating two distinct roles.
        "<{base}/Enrollment> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/Enrollment> <{LOGIC}subClassOf> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Enrollment> <{LOGIC}mediates> <{base}/student> <{base}/schema> .\n\
         <{base}/Enrollment> <{LOGIC}mediates> <{base}/course> <{base}/schema> .\n\
         \
         <{base}/Devotion> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/Devotion> <{LOGIC}subClassOf> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Devotion> <{LOGIC}mediates> <{base}/devotee> <{base}/schema> .\n\
         <{base}/devotee> <{RDF_TYPE_P}> <{OWL_FUNCTIONAL}> <{base}/schema> .\n\
         \
         <{base}/Membership> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/schema> .\n\
         <{base}/Membership> <{LOGIC}subClassOf> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Membership> <{LOGIC}mediates> <{base}/memberParty> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    // Under-mediated subclass relator (single functional role) fires RelComp — the
    // discipline now reaches the production `subClassOf logic:Relator` shape.
    assert!(
        has_violation(&quads, &format!("{base}/Devotion"), "RelComp"),
        "Devotion (subClassOf logic:Relator, one functional role) must fire RelComp"
    );
    // Well-mediated subclass relator (two distinct roles) does NOT fire.
    assert!(
        !has_violation(&quads, &format!("{base}/Enrollment"), "RelComp"),
        "Enrollment (subClassOf logic:Relator, two distinct roles) must NOT fire RelComp"
    );
    // A subclass relator mediating a single NON-functional role reaches two entities →
    // no RelComp (the symmetric-relator case, e.g. an agreement bound by its parties).
    assert!(
        !has_violation(&quads, &format!("{base}/Membership"), "RelComp"),
        "Membership (one non-functional role → two ends) must NOT fire RelComp"
    );
    // The Relator meta-class itself is never a concreteRelator (its subclasses make
    // hasLogicSubclass hold, suppressing it), so it never fires RelComp.
    assert!(
        !has_violation(&quads, &format!("{LOGIC}Relator"), "RelComp"),
        "logic:Relator (the meta-class) must never fire RelComp"
    );
}

/// A concrete relator that SPECIALISES a mediated relator inherits its ancestor's
/// mediation roles (mediation propagates down subClassOf), so it does NOT trip
/// RelComp even though it declares no logic:mediates of its own.  Here the inherited
/// role is a single NON-functional party role — the symmetric-relator pattern (a
/// contract bound by its parties) — which reaches two entities on its own.
#[test]
fn relcomp_mediation_propagates_to_subclass() {
    let base = "https://example.org/foundation/relcomp-propagation";
    let nq = format!(
        // Agreement: an abstract relator mediating a single non-functional party role.
        "<{base}/Agreement> <{RDF_TYPE_P}> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Agreement> <{LOGIC}mediates> <{base}/hasParty> <{base}/schema> .\n\
         <{base}/Contract> <{LOGIC}subClassOf> <{base}/Agreement> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_violation(&quads, &format!("{base}/Contract"), "RelComp"),
        "Contract (⊑ Agreement) must inherit its ancestor's non-functional party role \
         (two ends) and NOT fire RelComp"
    );
    // Agreement itself is abstract here (Contract is its subclass) → not concrete → exempt.
    assert!(
        !has_violation(&quads, &format!("{base}/Agreement"), "RelComp"),
        "Agreement (non-leaf) is not a concreteRelator and must not fire RelComp"
    );
}

// ── Anti-rigidity policies ───────────────────────────────────────────────────────

/// Two-world input: alice exists in both, Employee (a Role) typed instance carol.
fn anti_rigidity_input() -> (String, String) {
    let base = "https://example.org/foundation/anti-rig".to_owned();
    let nq = format!(
        // worldA: Employee is a Role; carol is an Employee (instance).
        "<{base}/Employee> <{RDF_TYPE_P}> <{LOGIC}Role> <{base}/worldA> .\n\
         <{base}/carol> <{RDF_TYPE_P}> <{base}/Employee> <{base}/worldA> .\n\
         <{base}/carol> <{base}/livesIn> <{base}/Berlin> <{base}/worldB> .\n"
    );
    (base, nq)
}

#[test]
fn anti_rigidity_witness_obligation_emits_discharge() {
    let (base, nq) = anti_rigidity_input();
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    let pred = format!("{LOGIC}dischargeObligation");
    assert!(
        quads.iter().any(|q| q.subject == format!("{base}/carol")
            && q.predicate == pred
            && q.object == format!("<{base}/Employee>")
            && q.graph == format!("{base}/worldA")),
        "witness-obligation must emit dischargeObligation for carol in worldA"
    );
}

#[test]
fn anti_rigidity_schema_only_emits_nothing() {
    let (_base, nq) = anti_rigidity_input();
    let quads = run(&nq, AntiRigidityPolicy::SchemaOnly);
    let discharge = format!("{LOGIC}dischargeObligation");
    let witness = format!("{LOGIC}witnessRequiredViolation");
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate == discharge || q.predicate == witness),
        "schema-only must emit no instance-level obligation facts"
    );
}

#[test]
fn anti_rigidity_witness_required_fires_when_not_discharged() {
    // Single world: carol is typed Employee (Role) and there is no counter-world.
    let base = "https://example.org/foundation/anti-rig-strict";
    let nq = format!(
        "<{base}/Employee> <{RDF_TYPE_P}> <{LOGIC}Role> <{base}/worldA> .\n\
         <{base}/carol> <{RDF_TYPE_P}> <{base}/Employee> <{base}/worldA> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessRequired);
    let pred = format!("{LOGIC}witnessRequiredViolation");
    assert!(
        quads
            .iter()
            .any(|q| q.subject == format!("{base}/carol") && q.predicate == pred),
        "witness-required must fire when no counter-world discharges the obligation"
    );
}

#[test]
fn anti_rigidity_witness_required_discharged_by_counter_world() {
    // carol typed Employee in worldA, exists (untyped Employee) in worldB → discharged.
    let base = "https://example.org/foundation/anti-rig-discharged";
    let nq = format!(
        "<{base}/Employee> <{RDF_TYPE_P}> <{LOGIC}Role> <{base}/worldA> .\n\
         <{base}/carol> <{RDF_TYPE_P}> <{base}/Employee> <{base}/worldA> .\n\
         <{base}/carol> <{base}/livesIn> <{base}/Berlin> <{base}/worldB> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessRequired);
    let pred = format!("{LOGIC}witnessRequiredViolation");
    assert!(
        !quads
            .iter()
            .any(|q| q.subject == format!("{base}/carol") && q.predicate == pred),
        "a counter-world must discharge the witness-required obligation"
    );
}

// ── Cross-world rigidity ─────────────────────────────────────────────────────────

#[test]
fn cross_world_rigidity_fires_two_worlds() {
    // Mirrors conformance cross-world-rigidity: alice is a Person (Kind=rigid) in
    // worldA and still exists in worldB but is NOT typed Person there → violation in B.
    let base = "https://example.org/foundation/cross-world-rigidity";
    let nq = format!(
        "<{base}/Person> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/worldA> .\n\
         <{base}/alice> <{RDF_TYPE_P}> <{base}/Person> <{base}/worldA> .\n\
         <{base}/bob> <{RDF_TYPE_P}> <{base}/Person> <{base}/worldA> .\n\
         <{base}/alice> <{base}/livesIn> <{base}/Berlin> <{base}/worldB> .\n\
         <{base}/bob> <{RDF_TYPE_P}> <{base}/Person> <{base}/worldB> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    let pred = format!("{LOGIC}rigidityViolation");
    // alice: typed Person in A, exists in B but not typed Person → violation in B.
    assert!(
        quads.iter().any(|q| q.subject == format!("{base}/alice")
            && q.predicate == pred
            && q.object == format!("<{base}/Person>")
            && q.graph == format!("{base}/worldB")),
        "alice must get a rigidityViolation in worldB"
    );
    // bob persists as Person into worldB → no violation.
    assert!(
        !quads
            .iter()
            .any(|q| q.subject == format!("{base}/bob") && q.predicate == pred),
        "bob (persists) must NOT get a rigidityViolation"
    );
}

#[test]
fn cross_world_rigidity_clean_single_world() {
    let base = "https://example.org/foundation/single";
    let nq = format!(
        "<{base}/Person> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/worldA> .\n\
         <{base}/alice> <{RDF_TYPE_P}> <{base}/Person> <{base}/worldA> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    let pred = format!("{LOGIC}rigidityViolation");
    assert!(
        !quads.iter().any(|q| q.predicate == pred),
        "single world admits no cross-world rigidity violation"
    );
}

// ── Modal evaluation (□/◇ standard translation) ─────────────────────────────────

/// Build a modal frame `F = op B` over `logic:epistemicallyPossible` from `w0`, with
/// `B`'s ground atom `(a, knows, b)`, `R`-edges `R(w0,w1) ∧ R(w0,w2)`, and the atom
/// fact `a knows b` in each named graph listed in `atom_worlds`.
///
/// `op` is `"necessarily"` (□) or `"possibly"` (◇).  Everything is all-IRI and every
/// quad lives in a named graph so the chase echoes it as an ordinary `FoundationQuad`.
fn modal_frame_nq(op: &str, atom_worlds: &[&str]) -> String {
    let base = "https://example.org/foundation/modal";
    let frame = format!("{base}/frame");
    let epi = format!("{LOGIC}epistemicallyPossible");
    let mut nq = format!(
        "<{base}/F> <{LOGIC}{op}> <{base}/B> <{frame}> .\n\
         <{base}/F> <{LOGIC}overAccessibility> <{epi}> <{frame}> .\n\
         <{base}/F> <{LOGIC}modalEvalWorld> <{base}/w0> <{frame}> .\n\
         <{base}/B> <{LOGIC}atomSubject> <{base}/a> <{frame}> .\n\
         <{base}/B> <{LOGIC}atomPredicate> <{base}/knows> <{frame}> .\n\
         <{base}/B> <{LOGIC}atomObject> <{base}/b> <{frame}> .\n\
         <{base}/w0> <{epi}> <{base}/w1> <{frame}> .\n\
         <{base}/w0> <{epi}> <{base}/w2> <{frame}> .\n"
    );
    for w in atom_worlds {
        nq.push_str(&format!("<{base}/a> <{base}/knows> <{base}/b> <{base}/{w}> .\n"));
    }
    nq
}

#[test]
fn modal_box_holds_when_body_true_in_all_accessible() {
    // □(a knows b) over epistemicallyPossible from w0, with w1 and w2 both accessible
    // and the atom present in BOTH → the necessity holds.
    let base = "https://example.org/foundation/modal";
    let quads = run(
        &modal_frame_nq("necessarily", &["w1", "w2"]),
        AntiRigidityPolicy::WitnessObligation,
    );
    assert!(
        quads.iter().any(|q| q.subject == format!("{base}/F")
            && q.predicate == format!("{LOGIC}modalNecessityHolds")
            && q.object == format!("<{base}/B>")
            && q.graph == format!("{base}/w0")),
        "□ must hold when the body atom is present in every accessible world"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate == format!("{LOGIC}modalNecessityFails")),
        "no □-failure when the body holds everywhere accessible"
    );
}

#[test]
fn modal_box_fails_with_witness() {
    // Atom present in w1 only; w2 is accessible but lacks it → □ fails, and the least
    // absent accessible world (w2) is recorded as the counterexample world.
    let base = "https://example.org/foundation/modal";
    let quads = run(
        &modal_frame_nq("necessarily", &["w1"]),
        AntiRigidityPolicy::WitnessObligation,
    );
    assert!(
        quads.iter().any(|q| q.subject == format!("{base}/F")
            && q.predicate == format!("{LOGIC}modalNecessityFails")
            && q.object == format!("<{base}/B>")
            && q.graph == format!("{base}/w0")),
        "□ must fail when an accessible world lacks the body atom"
    );
    assert!(
        quads.iter().any(|q| q.subject == format!("{base}/F")
            && q.predicate == format!("{LOGIC}modalCounterexampleWorld")
            && q.object == format!("<{base}/w2>")
            && q.graph == format!("{base}/w0")),
        "the least absent accessible world (w2) must be the counterexample world"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate == format!("{LOGIC}modalNecessityHolds")),
        "no □-holds verdict when the necessity fails"
    );
}

#[test]
fn modal_diamond_holds_when_body_true_in_some() {
    // ◇(a knows b): the atom is present in w1 only, but one accessible world suffices.
    let base = "https://example.org/foundation/modal";
    let quads = run(
        &modal_frame_nq("possibly", &["w1"]),
        AntiRigidityPolicy::WitnessObligation,
    );
    assert!(
        quads.iter().any(|q| q.subject == format!("{base}/F")
            && q.predicate == format!("{LOGIC}modalPossibilityHolds")
            && q.object == format!("<{base}/B>")
            && q.graph == format!("{base}/w0")),
        "◇ must hold when the body atom is present in some accessible world"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate == format!("{LOGIC}modalPossibilityFails")),
        "no ◇-failure when the body holds in some accessible world"
    );
}

// ── Policy parsing: unknown is a hard error ─────────────────────────────────────

#[test]
fn unknown_policy_is_err() {
    assert!(AntiRigidityPolicy::from_str("definitely-not-a-policy").is_err());
    assert_eq!(
        AntiRigidityPolicy::from_str("witness-obligation").unwrap(),
        AntiRigidityPolicy::WitnessObligation
    );
    assert_eq!(
        AntiRigidityPolicy::from_str("schema-only").unwrap(),
        AntiRigidityPolicy::SchemaOnly
    );
    assert_eq!(
        AntiRigidityPolicy::from_str("witness-required").unwrap(),
        AntiRigidityPolicy::WitnessRequired
    );
}

// ── Negative control: instances / plain objects get NO markers ──────────────────

#[test]
fn negative_control_instances_get_no_markers() {
    // alice is a plain instance (typed by a Person, which has no logic stereotype),
    // Berlin is a plain object.  Neither should acquire any derived foundation marker.
    let base = "https://example.org/foundation/neg";
    let nq = format!(
        "<{base}/Person> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/worldA> .\n\
         <{base}/alice> <{RDF_TYPE_P}> <{base}/Person> <{base}/worldA> .\n\
         <{base}/alice> <{base}/livesIn> <{base}/Berlin> <{base}/worldA> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    // alice carries no hasMetaClass/isClass/violation marker (it is an instance).
    let alice = format!("{base}/alice");
    for q in &quads {
        if q.subject == alice {
            assert!(
                q.rule_iri == ASSERT_RULE_IRI,
                "alice should only have asserted quads, got derived: {q:?}"
            );
        }
    }
    // Berlin never appears as a derived subject.
    let berlin = format!("{base}/Berlin");
    assert!(
        quads
            .iter()
            .filter(|q| q.subject == berlin)
            .all(|q| q.rule_iri == ASSERT_RULE_IRI),
        "Berlin (plain object) must acquire no derived markers"
    );
}

// ── Provenance recipe lock ───────────────────────────────────────────────────────

#[test]
fn provenance_isclass_derivation_matches_recipe() {
    // Lock the derivation-ID recipe usage: isClass(HonorsStudent) is derived from
    // hasMetaClass(HonorsStudent, SubKind) (single source), under logic:rule/anonymous.
    let base = "https://example.org/foundation/mixrig-kind-under-role";
    let nq = format!(
        "<{base}/HonorsStudent> <{RDF_TYPE_P}> <{LOGIC}SubKind> <{base}/schema> .\n\
         <{base}/HonorsStudent> <{LOGIC}subClassOf> <{base}/Student> <{base}/schema> .\n\
         <{base}/Student> <{RDF_TYPE_P}> <{LOGIC}Role> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    let honors = format!("{base}/HonorsStudent");
    let is_class = find(
        &quads,
        &format!("{base}/schema"),
        &honors,
        &format!("{LOGIC}isClass"),
        &format!("<{honors}>"),
    );

    // The quality-ordered tiebreak (most-direct derivation) now selects the single
    // asserted `subClassOf(HonorsStudent, Student)` fact as the source for
    // `isClass(HonorsStudent)`, rather than the previously-selected
    // `hasMetaClass(HonorsStudent, SubKind)` derived fact.  The shallower derivation
    // (depth-1 asserted source) wins over the depth-2 chain through hasMetaClass.
    let student = format!("{base}/Student");
    let expected_source = triple_reifier(&honors, &format!("{LOGIC}subClassOf"), &student).unwrap();
    let expected_deriv = mint_derivation_id(ANON_RULE_IRI, &[expected_source.as_str()]);

    assert_eq!(
        is_class.rule_iri, ANON_RULE_IRI,
        "isClass must be stamped logic:rule/anonymous"
    );
    assert_eq!(
        is_class.source_quad_ids,
        vec![expected_source.clone()],
        "isClass source must be the subClassOf reifier (quality-ordered tiebreak: \
         most-direct / shallowest derivation wins)"
    );
    assert_eq!(
        is_class.derivation_id, expected_deriv,
        "isClass derivation_id must equal mint_derivation_id(anon, [subClassOf reifier]); \
         this is the captured golden 86f96b359743809b25d976426054cbe3d13283d1"
    );
    // Pin the captured golden value too (quality-ordered tiebreak authority).
    assert_eq!(
        is_class.derivation_id,
        "https://blackcatinformatics.ca/gmeow/derivation/86f96b359743809b25d976426054cbe3d13283d1"
    );
}

#[test]
fn provenance_rigidity_and_obligation_recipes() {
    // Cross-world rigidity: alice rigidityViolation Person in worldB; witness is the
    // worldA typing reifier; source_quad_ids empty.
    let base = "https://example.org/foundation/cross-world-rigidity";
    let nq = format!(
        "<{base}/Person> <{RDF_TYPE_P}> <{LOGIC}Kind> <{base}/worldA> .\n\
         <{base}/Employee> <{RDF_TYPE_P}> <{LOGIC}Role> <{base}/worldA> .\n\
         <{base}/alice> <{RDF_TYPE_P}> <{base}/Person> <{base}/worldA> .\n\
         <{base}/bob> <{RDF_TYPE_P}> <{base}/Person> <{base}/worldA> .\n\
         <{base}/carol> <{RDF_TYPE_P}> <{base}/Employee> <{base}/worldA> .\n\
         <{base}/alice> <{base}/livesIn> <{base}/Berlin> <{base}/worldB> .\n\
         <{base}/bob> <{RDF_TYPE_P}> <{base}/Person> <{base}/worldB> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    let alice = format!("{base}/alice");
    let rig = find(
        &quads,
        &format!("{base}/worldB"),
        &alice,
        &format!("{LOGIC}rigidityViolation"),
        &format!("<{base}/Person>"),
    );
    assert!(
        rig.source_quad_ids.is_empty(),
        "rigidity leaf has empty sources"
    );
    assert_eq!(rig.rule_iri, RIGIDITY_RULE_IRI);
    assert_eq!(
        rig.derivation_id,
        "https://blackcatinformatics.ca/gmeow/derivation/ceefd40b66aebab96918cbc046bf074b29be43dc",
        "rigidity derivation_id must match the captured golden"
    );

    // Anti-rigidity: carol dischargeObligation Employee in worldA.
    let carol = format!("{base}/carol");
    let obl = find(
        &quads,
        &format!("{base}/worldA"),
        &carol,
        &format!("{LOGIC}dischargeObligation"),
        &format!("<{base}/Employee>"),
    );
    assert!(
        obl.source_quad_ids.is_empty(),
        "obligation leaf has empty sources"
    );
    assert_eq!(obl.rule_iri, ANTI_RIGIDITY_RULE_IRI);
    assert_eq!(
        obl.derivation_id,
        "https://blackcatinformatics.ca/gmeow/derivation/43210e798cea3220fe86ab2953919105e538d814",
        "anti-rigidity obligation derivation_id must match the captured golden"
    );
}

// ── Golden quad-set parity for the remaining single-world cases ──────────────────

const EXACTLY_ONE_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/foundation/exactly-one-stereotype/expected/materialized.nq"
);
const FREE_ROLE_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/foundation/free-role/expected/materialized.nq"
);
const IDENTITY_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/foundation/identity-overlap-mixiden/expected/materialized.nq"
);
const RELCOMP_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/foundation/relcomp-under-mediated/expected/materialized.nq"
);
// Holonic emergence (C2): the input.nq seed facts and the full
// materialized golden are read straight from the conformance case, so this Rust
// golden and the conformance harness assert the SAME bytes.
const HOLONIC_EMERGENCE_INPUT: &str =
    include_str!("../../../../conformance/logic/cases/holonic/emergence/input.nq");
const HOLONIC_EMERGENCE_GOLDEN: &str =
    include_str!("../../../../conformance/logic/cases/holonic/emergence/expected/materialized.nq");
// Holonic autonomy/integration duality (C4): the input.nq seed facts and the
// full materialized golden are read straight from the conformance case, so this Rust golden
// and the conformance harness assert the SAME bytes.
const HOLONIC_AGENCY_INPUT: &str =
    include_str!("../../../../conformance/logic/cases/holonic/holon-integrity/input.nq");
const HOLONIC_AGENCY_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/holonic/holon-integrity/expected/materialized.nq"
);
// Holonic downward-constraint governance (C3 corpus closure): the governance case and
// rules already existed; these constants wire the Rust golden to the same conformance case
// so the golden-parity test covers it.
const HOLONIC_GOVERNANCE_INPUT: &str =
    include_str!("../../../../conformance/logic/cases/holonic/downward-constraint/input.nq");
const HOLONIC_GOVERNANCE_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/holonic/downward-constraint/expected/materialized.nq"
);
// Holonic-level coherence (C5): position-based level coherence rule + HolonicLevelIncoherence.
const HOLONIC_LEVEL_INPUT: &str =
    include_str!("../../../../conformance/logic/cases/holonic/holonic-level/input.nq");
const HOLONIC_LEVEL_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/holonic/holonic-level/expected/materialized.nq"
);
// Holonic agent-goal-holarchy (C6): the named Principle-15 CONSUMER of the Holons epic —
// an AI agent's goal/action trajectory as a holarchy that applies the C1–C4 kernel at once.
// Read straight from the conformance case's input.nq world facts and run through the foundation
// lowering directly — the same world-fact bytes the harness materializes over for this
// foundation-profile case (the harness additionally loads input.logic.ttl, which this unit test omits).
const HOLONIC_AGENT_GOAL_INPUT: &str =
    include_str!("../../../../conformance/logic/cases/holonic/agent-goal-holarchy/input.nq");

#[test]
fn golden_quad_sets_match_for_single_world_cases() {
    let cases: [(&str, &str, &str); 8] = [
        (
            "exactly-one-stereotype",
            EXACTLY_ONE_GOLDEN,
            "<https://example.org/foundation/exactly-one-stereotype/Anchor> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Kind> <https://example.org/foundation/exactly-one-stereotype/schema> .\n\
             <https://example.org/foundation/exactly-one-stereotype/NoStereo> <https://blackcatinformatics.ca/logic/subClassOf> <https://example.org/foundation/exactly-one-stereotype/Anchor> <https://example.org/foundation/exactly-one-stereotype/schema> .\n\
             <https://example.org/foundation/exactly-one-stereotype/TwoStereo> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Kind> <https://example.org/foundation/exactly-one-stereotype/schema> .\n\
             <https://example.org/foundation/exactly-one-stereotype/TwoStereo> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Role> <https://example.org/foundation/exactly-one-stereotype/schema> .\n",
        ),
        (
            "free-role",
            FREE_ROLE_GOLDEN,
            "<https://example.org/foundation/free-role/Wanderer> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Role> <https://example.org/foundation/free-role/schema> .\n",
        ),
        (
            "identity-overlap-mixiden",
            IDENTITY_GOLDEN,
            "<https://example.org/foundation/identity-overlap-mixiden/Animal> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Kind> <https://example.org/foundation/identity-overlap-mixiden/schema> .\n\
             <https://example.org/foundation/identity-overlap-mixiden/Dog> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Kind> <https://example.org/foundation/identity-overlap-mixiden/schema> .\n\
             <https://example.org/foundation/identity-overlap-mixiden/Dog> <https://blackcatinformatics.ca/logic/subClassOf> <https://example.org/foundation/identity-overlap-mixiden/Animal> <https://example.org/foundation/identity-overlap-mixiden/schema> .\n",
        ),
        (
            "relcomp-under-mediated",
            RELCOMP_GOLDEN,
            "<https://example.org/foundation/relcomp-under-mediated/Employment> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Relator> <https://example.org/foundation/relcomp-under-mediated/schema> .\n\
             <https://example.org/foundation/relcomp-under-mediated/Employment> <https://blackcatinformatics.ca/logic/mediates> <https://example.org/foundation/relcomp-under-mediated/employee> <https://example.org/foundation/relcomp-under-mediated/schema> .\n\
             <https://example.org/foundation/relcomp-under-mediated/Employment> <https://blackcatinformatics.ca/logic/mediates> <https://example.org/foundation/relcomp-under-mediated/employer> <https://example.org/foundation/relcomp-under-mediated/schema> .\n\
             <https://example.org/foundation/relcomp-under-mediated/Marriage> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Relator> <https://example.org/foundation/relcomp-under-mediated/schema> .\n\
             <https://example.org/foundation/relcomp-under-mediated/Marriage> <https://blackcatinformatics.ca/logic/mediates> <https://example.org/foundation/relcomp-under-mediated/spouse> <https://example.org/foundation/relcomp-under-mediated/schema> .\n\
             <https://example.org/foundation/relcomp-under-mediated/spouse> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#FunctionalProperty> <https://example.org/foundation/relcomp-under-mediated/schema> .\n",
        ),
        (
            "holonic-emergence",
            HOLONIC_EMERGENCE_GOLDEN,
            HOLONIC_EMERGENCE_INPUT,
        ),
        (
            "holonic-autonomy-integration",
            HOLONIC_AGENCY_GOLDEN,
            HOLONIC_AGENCY_INPUT,
        ),
        (
            "holonic-governance",
            HOLONIC_GOVERNANCE_GOLDEN,
            HOLONIC_GOVERNANCE_INPUT,
        ),
        ("holonic-level", HOLONIC_LEVEL_GOLDEN, HOLONIC_LEVEL_INPUT),
    ];

    for (name, golden, input) in cases {
        let quads = run(input, AntiRigidityPolicy::WitnessObligation);
        assert_eq!(
            quad_set(&quads),
            golden_set(golden),
            "quad set mismatch for case {name}"
        );
    }
}

// ── Determinism / quality-ordered tiebreak test ───────────────────────────────

/// Verify that the per-round winner-selection tiebreak is quality-ordered and
/// independent of the order in which candidates are folded into the round map.
///
/// The total order is `(max_src_depth, sum_src_depth, sorted_sources)` — smaller wins.
/// This test proves:
///   1. **Depth dominates lex order**: a shallower (lower max-depth) candidate beats a
///      deeper one even when the deeper one has a lexicographically smaller sorted_sources.
///   2. **Sum-depth tiebreaks at equal max-depth**: among candidates with the same
///      max-depth, the one closer to asserted axioms (lower sum) wins.
///   3. **Lex-min sorted_sources as final tiebreaker** (all depth fields equal).
///   4. **All three levels are enumeration-order-independent**: forward, reverse, and a
///      permuted order must all yield the same winner for each level.
///
/// Self-contained — no STRATA or WorldStore dependency.
#[test]
fn first_wins_tiebreak_prefers_most_direct_derivation_order_independent() {
    /// A minimal stand-in for [`RoundCandidate`]'s comparison key.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct FakeCand {
        max_depth: u32,
        sum_depth: u64,
        sorted_sources: Vec<String>,
        label: &'static str, // for assertion messages only
    }

    /// Fold a slice of candidates using the same `and_modify` logic as `chase_world`,
    /// returning a clone of the winning candidate.
    fn fold(cands: &[FakeCand]) -> FakeCand {
        let mut winner: Option<FakeCand> = None;
        for c in cands {
            match &winner {
                None => winner = Some(c.clone()),
                Some(w) => {
                    let c_key = (c.max_depth, c.sum_depth, &c.sorted_sources);
                    let w_key = (w.max_depth, w.sum_depth, &w.sorted_sources);
                    if c_key < w_key {
                        winner = Some(c.clone());
                    }
                }
            }
        }
        winner.unwrap()
    }

    // ── Level 1: depth dominates lex order ──────────────────────────────────
    //
    // `shallow` has max_depth=1, `deep` has max_depth=2 but a lex-smaller sorted_sources.
    // Expected winner: `shallow` (depth beats lex).
    let shallow = FakeCand {
        max_depth: 1,
        sum_depth: 1,
        sorted_sources: vec!["urn:z".to_owned()], // lex-larger
        label: "shallow",
    };
    let deep = FakeCand {
        max_depth: 2,
        sum_depth: 2,
        sorted_sources: vec!["urn:a".to_owned()], // lex-smaller — but loses on depth
        label: "deep",
    };
    let pool1 = vec![shallow.clone(), deep.clone()];
    let pool1_rev = vec![deep.clone(), shallow.clone()];
    assert_eq!(fold(&pool1).label, "shallow", "fwd: depth must beat lex");
    assert_eq!(
        fold(&pool1_rev).label,
        "shallow",
        "rev: depth must beat lex"
    );

    // ── Level 2: sum-depth tiebreak at equal max-depth ───────────────────────
    //
    // `asserted_rooted` has max=1, sum=1; `chain_rooted` has max=1, sum=3.
    // Both have the same max-depth; sum-depth picks `asserted_rooted`.
    let asserted_rooted = FakeCand {
        max_depth: 1,
        sum_depth: 1,
        sorted_sources: vec!["urn:m".to_owned()], // lex-larger
        label: "asserted_rooted",
    };
    let chain_rooted = FakeCand {
        max_depth: 1,
        sum_depth: 3,
        sorted_sources: vec!["urn:a".to_owned()], // lex-smaller — but loses on sum
        label: "chain_rooted",
    };
    let pool2 = vec![asserted_rooted.clone(), chain_rooted.clone()];
    let pool2_rev = vec![chain_rooted.clone(), asserted_rooted.clone()];
    assert_eq!(
        fold(&pool2).label,
        "asserted_rooted",
        "fwd: sum-depth must beat lex at equal max-depth"
    );
    assert_eq!(
        fold(&pool2_rev).label,
        "asserted_rooted",
        "rev: sum-depth must beat lex at equal max-depth"
    );

    // ── Level 3: lex-min sorted_sources as final tiebreaker ─────────────────
    //
    // All three candidates have the same max-depth and sum-depth; only lex order decides.
    let cands3: Vec<FakeCand> = vec![
        FakeCand {
            max_depth: 0,
            sum_depth: 0,
            sorted_sources: vec!["urn:a".to_owned(), "urn:c".to_owned()],
            label: "ac",
        },
        FakeCand {
            max_depth: 0,
            sum_depth: 0,
            sorted_sources: vec!["urn:a".to_owned(), "urn:b".to_owned()], // ← lex smallest
            label: "ab",
        },
        FakeCand {
            max_depth: 0,
            sum_depth: 0,
            sorted_sources: vec!["urn:b".to_owned(), "urn:d".to_owned()],
            label: "bd",
        },
    ];
    let cands3_rev: Vec<FakeCand> = cands3.iter().cloned().rev().collect();
    let cands3_perm: Vec<FakeCand> = vec![cands3[2].clone(), cands3[0].clone(), cands3[1].clone()];
    assert_eq!(
        fold(&cands3).label,
        "ab",
        "fwd: lex-min must win when depths equal"
    );
    assert_eq!(
        fold(&cands3_rev).label,
        "ab",
        "rev: lex-min must win when depths equal"
    );
    assert_eq!(
        fold(&cands3_perm).label,
        "ab",
        "perm: lex-min must win when depths equal"
    );

    // ── Combined: depth 1 shallower beats lex-smaller depth 2, order-independent ─
    //
    // Three candidates with mixed depth and lex order; expected winner is the sole
    // depth-1 candidate (smallest max-depth), regardless of enumeration order.
    let c_depth1 = FakeCand {
        max_depth: 1,
        sum_depth: 1,
        sorted_sources: vec!["urn:z1".to_owned()], // lex-largest
        label: "depth1",
    };
    let c_depth2a = FakeCand {
        max_depth: 2,
        sum_depth: 2,
        sorted_sources: vec!["urn:a1".to_owned()], // lex-smallest — but depth 2
        label: "depth2a",
    };
    let c_depth2b = FakeCand {
        max_depth: 2,
        sum_depth: 3,
        sorted_sources: vec!["urn:b1".to_owned()],
        label: "depth2b",
    };
    let pool4 = vec![c_depth1.clone(), c_depth2a.clone(), c_depth2b.clone()];
    let pool4_rev: Vec<FakeCand> = pool4.iter().cloned().rev().collect();
    let pool4_perm: Vec<FakeCand> = vec![c_depth2b.clone(), c_depth1.clone(), c_depth2a.clone()];
    assert_eq!(
        fold(&pool4).label,
        "depth1",
        "combined fwd: shallowest wins"
    );
    assert_eq!(
        fold(&pool4_rev).label,
        "depth1",
        "combined rev: shallowest wins"
    );
    assert_eq!(
        fold(&pool4_perm).label,
        "depth1",
        "combined perm: shallowest wins"
    );
}

// ── Typed/contextual mereology + holon kernel (C1) ───────────────────

/// Whether a binary `(subject, predicate_local, object_iri)` fact is present
/// (object compared in N3 `<iri>` form).
fn has_binary(quads: &[FoundationQuad], subject: &str, local: &str, object_iri: &str) -> bool {
    let pred = format!("{LOGIC}{local}");
    let obj = format!("<{object_iri}>");
    quads
        .iter()
        .any(|q| q.subject == subject && q.predicate == pred && q.object == obj)
}

/// Whether a self-atom marker `(subject, predicate_local, subject)` is present.
fn has_marker(quads: &[FoundationQuad], subject: &str, local: &str) -> bool {
    has_binary(quads, subject, local, subject)
}

#[test]
fn holon_projection_middle_of_chain_only() {
    // Engine ⊏ Car ⊏ Fleet.  Car is both a proper part (of Fleet) and a whole
    // (has Engine) → isHolon.  Fleet (root) and Engine (leaf) are not holons.
    let base = "https://example.org/foundation/holon-projection";
    let nq = format!(
        "<{base}/Engine> <{LOGIC}properPartOf> <{base}/Car> <{base}/schema> .\n\
         <{base}/Car> <{LOGIC}properPartOf> <{base}/Fleet> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    assert!(
        has_marker(&quads, &format!("{base}/Car"), "isHolon"),
        "Car (part AND whole) must project to isHolon"
    );
    assert!(
        !has_marker(&quads, &format!("{base}/Fleet"), "isHolon"),
        "Fleet (root whole) must NOT be a holon"
    );
    assert!(
        !has_marker(&quads, &format!("{base}/Engine"), "isHolon"),
        "Engine (atomic leaf) must NOT be a holon"
    );
    // overlap: a proper part overlaps its whole (both directions).
    assert!(
        has_binary(
            &quads,
            &format!("{base}/Engine"),
            "overlaps",
            &format!("{base}/Car")
        ),
        "Engine overlaps Car (part overlaps whole)"
    );
    assert!(
        has_binary(
            &quads,
            &format!("{base}/Car"),
            "overlaps",
            &format!("{base}/Engine")
        ),
        "overlaps is symmetric: Car overlaps Engine"
    );
}

#[test]
fn holon_two_node_chain_has_no_holon() {
    // A ⊏ B: A is a leaf, B is a root.  Neither is both a part and a whole, so the
    // reflexive overlap (a thing overlaps its own whole) must NOT spuriously fire
    // isHolon for either.
    let base = "https://example.org/foundation/holon-two-node";
    let nq = format!("<{base}/A> <{LOGIC}properPartOf> <{base}/B> <{base}/schema> .\n");
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    assert!(
        !has_marker(&quads, &format!("{base}/A"), "isHolon"),
        "A (leaf) must not be a holon in a 2-node chain"
    );
    assert!(
        !has_marker(&quads, &format!("{base}/B"), "isHolon"),
        "B (root) must not be a holon in a 2-node chain"
    );
}

#[test]
fn holonic_level_coherence_is_position_based_and_profile_scoped() {
    let base = "https://example.org/holonic/holonic-level";
    let quads = run(HOLONIC_LEVEL_INPUT, AntiRigidityPolicy::WitnessObligation);
    // Profiled holon WITH a HolonicPosition → coherent, not charged.
    assert!(
        !has_violation(
            &quads,
            &format!("{base}/Bracket"),
            "HolonicLevelIncoherence"
        ),
        "Bracket (profiled holon occupying a HolonicPosition) must NOT fire HolonicLevelIncoherence"
    );
    // Profiled holon in the instantiation tower but with NO position → fires (non-conflation:
    // logic:instanceOf / orderedType do not supply a holonic position).
    assert!(
        has_violation(
            &quads,
            &format!("{base}/Gearbox"),
            "HolonicLevelIncoherence"
        ),
        "Gearbox (profiled holon, no position, only instanceOf) MUST fire HolonicLevelIncoherence"
    );
    // Unprofiled holon → never charged (profile-relativity).
    assert!(
        !has_violation(&quads, &format!("{base}/Sprite"), "HolonicLevelIncoherence"),
        "Sprite (unprofiled holon) must NOT fire HolonicLevelIncoherence"
    );
}

#[test]
fn multiply_positioned_marks_entity_with_two_distinct_positions() {
    // ME9: an entity occupying TWO distinct logic:HolonicPositions is the structural
    // signature of a DAG node on multiple paths → logic:multiplyPositioned fires, grounding
    // that a path-relative depth band exists.  An entity with a single position does NOT fire.
    let base = "https://example.org/foundation/multiply-positioned";
    let nq = format!(
        "<{base}/posA> <{LOGIC}positionEntity> <{base}/Sensor> <{base}/schema> .\n\
         <{base}/posB> <{LOGIC}positionEntity> <{base}/Sensor> <{base}/schema> .\n\
         <{base}/posC> <{LOGIC}positionEntity> <{base}/Bolt> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    assert!(
        has_marker(&quads, &format!("{base}/Sensor"), "multiplyPositioned"),
        "Sensor (two distinct HolonicPositions) must be multiplyPositioned"
    );
    assert!(
        !has_marker(&quads, &format!("{base}/Bolt"), "multiplyPositioned"),
        "Bolt (a single HolonicPosition) must NOT be multiplyPositioned"
    );
}

#[test]
fn weak_supplementation_singleton_part_under_profile_fires() {
    // W1 is declared under a MereologyProfile and has exactly ONE proper part P1
    // with no disjoint co-part → weak supplementation is violated.
    let base = "https://example.org/foundation/weak-supplementation";
    let nq = format!(
        "<{base}/W1> <{LOGIC}underMereologyProfile> <{base}/MP> <{base}/schema> .\n\
         <{base}/P1> <{LOGIC}properPartOf> <{base}/W1> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    assert!(
        has_marker(&quads, &format!("{base}/W1"), "supplementationScoped"),
        "W1 declared under a profile must be supplementation-scoped"
    );
    assert!(
        has_violation(&quads, &format!("{base}/W1"), "WeakSupplementation"),
        "W1 (lone proper part, under profile) must fire WeakSupplementation"
    );
}

#[test]
fn weak_supplementation_two_disjoint_parts_under_profile_clean() {
    // W2 under a profile has two disjoint proper parts (Pa, Pb share no part) →
    // each has a disjoint co-part, so weak supplementation holds (no violation).
    let base = "https://example.org/foundation/weak-supplementation-clean";
    let nq = format!(
        "<{base}/W2> <{LOGIC}underMereologyProfile> <{base}/MP> <{base}/schema> .\n\
         <{base}/Pa> <{LOGIC}properPartOf> <{base}/W2> <{base}/schema> .\n\
         <{base}/Pb> <{LOGIC}properPartOf> <{base}/W2> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    assert!(
        has_binary(
            &quads,
            &format!("{base}/Pa"),
            "disjoint",
            &format!("{base}/Pb")
        ),
        "Pa and Pb (no shared part) must be disjoint"
    );
    assert!(
        has_marker(&quads, &format!("{base}/W2"), "supplementationScoped"),
        "W2 declared under a profile must be supplementation-scoped"
    );
    assert!(
        !has_violation(&quads, &format!("{base}/W2"), "WeakSupplementation"),
        "W2 (two disjoint parts) must NOT fire WeakSupplementation"
    );
}

#[test]
fn weak_supplementation_inert_without_profile() {
    // W3 has a lone proper part but is NOT declared under any profile → parthood is
    // profiled, so no WeakSupplementation obligation applies.
    let base = "https://example.org/foundation/weak-supplementation-unprofiled";
    let nq = format!("<{base}/P3> <{LOGIC}properPartOf> <{base}/W3> <{base}/schema> .\n");
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    assert!(
        !has_marker(&quads, &format!("{base}/W3"), "supplementationScoped"),
        "W3 not under a profile must not be supplementation-scoped"
    );
    assert!(
        !has_violation(&quads, &format!("{base}/W3"), "WeakSupplementation"),
        "W3 (lone part, NO profile) must NOT fire WeakSupplementation"
    );
}

#[test]
fn quality_role_without_generic_fires_and_is_inert_with_generic() {
    // YAMATO quality constraint: a logic:Quality playing a logic:qualityRole with NO
    // logic:genericQuality fires QualityRoleWithoutGeneric — a frame-relative value
    // standing without the frame-independent structure it refines (Principle 11 in
    // role terms).  Adding the generic quality fails the NAF complement → no violation.
    let base = "https://example.org/foundation/quality-role-without-generic";
    let nq = format!("<{base}/Q> <{LOGIC}qualityRole> <{base}/systolicRole> <{base}/schema> .\n");
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_violation(&quads, &format!("{base}/Q"), "QualityRoleWithoutGeneric"),
        "Q (qualityRole, no genericQuality) must fire QualityRoleWithoutGeneric"
    );

    let nq_ok = format!(
        "<{base}/Q> <{LOGIC}qualityRole> <{base}/systolicRole> <{base}/schema> .\n\
         <{base}/Q> <{LOGIC}genericQuality> <{base}/pressure> <{base}/schema> .\n"
    );
    let quads_ok = run(&nq_ok, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_violation(&quads_ok, &format!("{base}/Q"), "QualityRoleWithoutGeneric"),
        "Q with a genericQuality must NOT fire QualityRoleWithoutGeneric"
    );
}

#[test]
fn measurement_frame_missing_fires_and_is_inert_with_frame() {
    // YAMATO quality constraint: a measurement bearing a logic:unit with NO
    // logic:referenceFrame fires MeasurementFrameMissing — a value expressed in a unit
    // without its frame (Principle 11).  The rule keys on the IRI-valued unit witness,
    // not the literal logic:measuredValue (the foundation chase is all-IRI).  Adding the
    // frame fails the NAF complement → no violation.
    let base = "https://example.org/foundation/measurement-frame-missing";
    let nq = format!("<{base}/M> <{LOGIC}unit> <{base}/mmHg> <{base}/schema> .\n");
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_violation(&quads, &format!("{base}/M"), "MeasurementFrameMissing"),
        "M (unit, no referenceFrame) must fire MeasurementFrameMissing"
    );

    let nq_ok = format!(
        "<{base}/M> <{LOGIC}unit> <{base}/mmHg> <{base}/schema> .\n\
         <{base}/M> <{LOGIC}referenceFrame> <{base}/clinicalFrame> <{base}/schema> .\n"
    );
    let quads_ok = run(&nq_ok, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_violation(&quads_ok, &format!("{base}/M"), "MeasurementFrameMissing"),
        "M with a referenceFrame must NOT fire MeasurementFrameMissing"
    );
}

#[test]
fn holonic_emergence_tri_valued_verdicts_and_non_propagation() {
    // One whole (Car, with proper parts Engine + Wheel) under one reduction theory
    // (AdditiveTheory, whose basis carries HasMass but NOT Drivable) drives all three
    // emergence verdicts, and the emergent property must NOT propagate to the parts.
    let base = "https://example.org/foundation/holonic-emergence";
    let nq = format!(
        "<{base}/Engine> <{LOGIC}properPartOf> <{base}/Car> <{base}/schema> .\n\
         <{base}/Wheel> <{LOGIC}properPartOf> <{base}/Car> <{base}/schema> .\n\
         <{base}/Car> <{LOGIC}bearsProperty> <{base}/HasMass> <{base}/schema> .\n\
         <{base}/Engine> <{LOGIC}bearsProperty> <{base}/HasMass> <{base}/schema> .\n\
         <{base}/Car> <{LOGIC}bearsProperty> <{base}/Drivable> <{base}/schema> .\n\
         <{base}/Car> <{LOGIC}bearsProperty> <{base}/Numinous> <{base}/schema> .\n\
         <{base}/AdditiveTheory> <{LOGIC}reductionBasis> <{base}/HasMass> <{base}/schema> .\n\
         <{base}/AssessMass> <{LOGIC}assessmentWhole> <{base}/Car> <{base}/schema> .\n\
         <{base}/AssessMass> <{LOGIC}assessmentProperty> <{base}/HasMass> <{base}/schema> .\n\
         <{base}/AssessMass> <{LOGIC}assessmentReductionTheory> <{base}/AdditiveTheory> <{base}/schema> .\n\
         <{base}/AssessDrive> <{LOGIC}assessmentWhole> <{base}/Car> <{base}/schema> .\n\
         <{base}/AssessDrive> <{LOGIC}assessmentProperty> <{base}/Drivable> <{base}/schema> .\n\
         <{base}/AssessDrive> <{LOGIC}assessmentReductionTheory> <{base}/AdditiveTheory> <{base}/schema> .\n\
         <{base}/AssessNuminous> <{LOGIC}assessmentWhole> <{base}/Car> <{base}/schema> .\n\
         <{base}/AssessNuminous> <{LOGIC}assessmentProperty> <{base}/Numinous> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);

    // Aggregate: the theory's basis carries HasMass AND a proper part bears it.
    assert!(
        has_binary(
            &quads,
            &format!("{base}/AssessMass"),
            "assessmentVerdict",
            &format!("{LOGIC}Aggregate")
        ),
        "mass (theory-reduced, borne by a part) must be Aggregate"
    );
    // Emergent: borne by the whole, theory declared, but not part-reducible — a genuine
    // negation-as-failure over the aggregate derivation, never a theory-free default.
    assert!(
        has_binary(
            &quads,
            &format!("{base}/AssessDrive"),
            "assessmentVerdict",
            &format!("{LOGIC}Emergent")
        ),
        "drivability (borne by the whole, not reduced by the theory) must be Emergent"
    );
    // Unknown: the whole bears the property but the assessment declares no reduction
    // theory, so the reducibility question cannot be posed (ME9's third value).
    assert!(
        has_binary(
            &quads,
            &format!("{base}/AssessNuminous"),
            "assessmentVerdict",
            &format!("{LOGIC}EmergenceUnknown")
        ),
        "a theory-free assessment of a borne property must be EmergenceUnknown"
    );
    // The three verdicts are mutually exclusive — no assessment carries two.
    assert!(
        !has_binary(
            &quads,
            &format!("{base}/AssessDrive"),
            "assessmentVerdict",
            &format!("{LOGIC}Aggregate")
        ),
        "the emergent assessment must NOT also be Aggregate"
    );
    // NON-PROPAGATION: the emergent property never reaches the parts, and is not
    // entailed by the parts' properties — non-inheritance is structural.
    assert!(
        !has_binary(
            &quads,
            &format!("{base}/Engine"),
            "bearsProperty",
            &format!("{base}/Drivable")
        ),
        "emergent Drivable must NOT propagate down properPartOf to Engine"
    );
    assert!(
        !has_binary(
            &quads,
            &format!("{base}/Wheel"),
            "bearsProperty",
            &format!("{base}/Drivable")
        ),
        "emergent Drivable must NOT propagate down properPartOf to Wheel"
    );
    assert!(
        !has_binary(
            &quads,
            &format!("{base}/Engine"),
            "bearsProperty",
            &format!("{base}/Numinous")
        ),
        "the Numinous (Unknown) property must NOT propagate to Engine"
    );
}

#[test]
fn holonic_downward_constraint_tri_valued_verdicts_and_non_transitivity() {
    // The C3 (revised by ME9) downward-constraint governance, driven from the
    // SAME bytes the conformance harness asserts (the downward-constraint case input.nq).  One
    // holarchy (ex:Department ▷ ex:Team ▷ ex:Member) and one governance regime (ex:HouseRegime,
    // whose logic:activationBasis carries ex:OnCallState but NOT ex:IdleState) drive all three
    // logic:ConstraintVerdict values; the would-be binding on ex:GovWaived is DEFEATED because
    // ex:Team bears the declared override token (ex:CharterWaiver).  Crucially, the verdict must
    // NOT cascade down logic:properPartOf to the grandchild ex:Member — non-transitivity is the
    // C3 guarantee, and golden parity alone cannot pin that NEGATIVE fact.
    let base = "https://example.org/foundation/holonic-governance";
    let quads = run(
        HOLONIC_GOVERNANCE_INPUT,
        AntiRigidityPolicy::WitnessObligation,
    );

    // The three verdicts, each on its own logic:DownwardConstraint reifier.
    let expect: [(&str, &str); 3] = [
        ("GovActive", "ConstraintBinding"), // regime activates OnCallState, no override
        ("GovWaived", "ConstraintOverridden"), // binding defeated by the CharterWaiver token
        ("GovIdle", "ConstraintUnknown"),   // IdleState not in the regime's activationBasis
    ];
    let all_verdicts = [
        "ConstraintBinding",
        "ConstraintOverridden",
        "ConstraintUnknown",
    ];
    for (constraint, verdict) in expect {
        assert!(
            has_binary(
                &quads,
                &format!("{base}/{constraint}"),
                "constraintVerdict",
                &format!("{LOGIC}{verdict}")
            ),
            "{constraint} must receive {verdict}"
        );
        // Mutual exclusivity: the constraint carries NO other verdict of the trio.
        for other in all_verdicts.iter().filter(|v| **v != verdict) {
            assert!(
                !has_binary(
                    &quads,
                    &format!("{base}/{constraint}"),
                    "constraintVerdict",
                    &format!("{LOGIC}{other}")
                ),
                "{constraint} must NOT also receive {other}"
            );
        }
    }

    // NON-TRANSITIVITY (the C3 guarantee): governance does NOT cascade down
    // logic:properPartOf.  ex:Member is a proper part of ex:Team (hence TRANSITIVELY a proper
    // part of ex:Department), but no logic:DownwardConstraint names it as its constraintTarget,
    // so it receives NO logic:constraintVerdict of any value — every verdict rule is gated on an
    // explicit constraintWhole / constraintTarget reification, never on the properPartOf closure.
    for verdict in all_verdicts {
        assert!(
            !has_binary(
                &quads,
                &format!("{base}/Member"),
                "constraintVerdict",
                &format!("{LOGIC}{verdict}")
            ),
            "downward constraint must NOT cascade to the grandchild ex:Member (got {verdict})"
        );
    }
}

#[test]
fn holonic_agency_four_valued_verdicts() {
    // The C4 holon autonomy/integration duality, driven from the SAME bytes
    // the conformance harness asserts (the case input.nq).  One declared
    // logic:HolonicAgencyProfile (KoestlerProfile: command ⇒ self-assertion, subordination
    // ⇒ integration) drives all four logic:AgencyVerdict values, and the two Janus markers
    // are co-equal (Principle 9).  A second, basis-free profile (VoidProfile) pins AgencyUnknown's
    // SECOND trigger — a declared profile that names no basis — distinct from the holon-bears-
    // nothing trigger (AssessInert).
    let base = "https://example.org/foundation/holonic-autonomy-integration";
    let quads = run(HOLONIC_AGENCY_INPUT, AntiRigidityPolicy::WitnessObligation);

    // The four verdicts (each on its own assessment) plus the basis-free AgencyUnknown: the SAME
    // both-capacity holon (Captain) is HolonIntegral under KoestlerProfile yet AgencyUnknown under
    // the basis-free VoidProfile (AssessSilentCaptain) — agency is profile-relative.
    let expect: [(&str, &str); 5] = [
        ("AssessCaptain", "HolonIntegral"),
        ("AssessPrivate", "AutonomyDeficient"),
        ("AssessWarlord", "IntegrationDeficient"),
        ("AssessInert", "AgencyUnknown"),
        ("AssessSilentCaptain", "AgencyUnknown"),
    ];
    let all_verdicts = [
        "HolonIntegral",
        "AutonomyDeficient",
        "IntegrationDeficient",
        "AgencyUnknown",
    ];
    for (assess, verdict) in expect {
        assert!(
            has_binary(
                &quads,
                &format!("{base}/{assess}"),
                "agencyVerdict",
                &format!("{LOGIC}{verdict}")
            ),
            "{assess} must receive {verdict}"
        );
        // Mutual exclusivity: the assessment carries NO other verdict (the 2×2 partitions).
        for other in all_verdicts.iter().filter(|v| **v != verdict) {
            assert!(
                !has_binary(
                    &quads,
                    &format!("{base}/{assess}"),
                    "agencyVerdict",
                    &format!("{LOGIC}{other}")
                ),
                "{assess} must NOT also receive {other}"
            );
        }
    }

    // Principle 9 co-equality: the BALANCED holon carries BOTH markers, derived by identical
    // rules — neither face is privileged.  AssessPrivate carries only integrative (a part
    // with no autonomy); AssessWarlord only self-assertive (a whole refusing to integrate).
    assert!(
        has_marker(&quads, &format!("{base}/AssessCaptain"), "selfAssertive")
            && has_marker(&quads, &format!("{base}/AssessCaptain"), "integrative"),
        "the integral assessment must carry BOTH co-equal Janus markers"
    );
    assert!(
        has_marker(&quads, &format!("{base}/AssessPrivate"), "integrative")
            && !has_marker(&quads, &format!("{base}/AssessPrivate"), "selfAssertive"),
        "the autonomy-deficient assessment integrates but does not self-assert"
    );
    assert!(
        has_marker(&quads, &format!("{base}/AssessWarlord"), "selfAssertive")
            && !has_marker(&quads, &format!("{base}/AssessWarlord"), "integrative"),
        "the integration-deficient assessment self-asserts but does not integrate"
    );

    // Dogfooding C1: the mid-chain holon ex:Captain co-fires logic:isHolon.
    assert!(
        has_marker(&quads, &format!("{base}/Captain"), "isHolon"),
        "the mid-chain ex:Captain must co-fire the C1 holon projection"
    );

    // Basis-free trigger: ex:AssessSilentCaptain assesses the SAME both-capacity holon (Captain)
    // under the basis-free ex:VoidProfile.  No basis means NEITHER marker can derive, so the
    // verdict is Unknown — not deficient (deficiency needs the mirror marker to hold) and not
    // integral.  This proves the verdict is PROFILE-RELATIVE: the identical holon that is
    // HolonIntegral under KoestlerProfile is AgencyUnknown here, purely because the profile
    // declares no basis to reason over.
    assert!(
        !has_marker(
            &quads,
            &format!("{base}/AssessSilentCaptain"),
            "selfAssertive"
        ) && !has_marker(
            &quads,
            &format!("{base}/AssessSilentCaptain"),
            "integrative"
        ),
        "a basis-free profile must evidence NEITHER Janus marker, even for a both-capacity holon"
    );

    // B1 well-formedness guard: a malformed assessment (no logic:agencyProfile) receives NO
    // verdict, because every verdict rule re-binds agencyHolon AND agencyProfile.  Here the
    // holon bears BOTH capacities, so a guard-free rule would wrongly emit HolonIntegral.
    let malformed = format!(
        "{HOLONIC_AGENCY_INPUT}\
         <{base}/AssessMalformed> <{LOGIC}agencyHolon> <{base}/Captain> <{base}/schema> .\n"
    );
    let mq = run(&malformed, AntiRigidityPolicy::WitnessObligation);
    for verdict in all_verdicts {
        assert!(
            !has_binary(
                &mq,
                &format!("{base}/AssessMalformed"),
                "agencyVerdict",
                &format!("{LOGIC}{verdict}")
            ),
            "a profile-less assessment must receive NO verdict (got {verdict})"
        );
    }
}

#[test]
fn holonic_agent_goal_holarchy_non_propagation_and_non_transitivity() {
    // C6: the named Principle-15 CONSUMER — an AI agent's goal/action trajectory as
    // a holarchy, applying the C1–C4 kernel at once.  The flagship composite previously had only
    // golden-parity coverage; this test PINS the two structural guarantees golden parity cannot
    // express as positive facts: NON-PROPAGATION (C2 — an emergent plan property never inherits
    // down to sub-goals) and NON-TRANSITIVITY (C3 — a downward constraint never cascades down
    // logic:properPartOf to a grandchild sub-goal).  Positive anchors across all four composed
    // families come first, so the test cannot pass vacuously on an inert case.
    let base = "https://example.org/holonic/agent-goal-holarchy";
    let quads = run(
        HOLONIC_AGENT_GOAL_INPUT,
        AntiRigidityPolicy::WitnessObligation,
    );

    // ── Liveness: at least one positive derivation from each composed family fires ──
    // C1 holon projection: mid-chain goals/actions are holons; the root goal and leaf are not.
    for holon in ["RetrieveContext", "AnswerQuery", "ToolPhase"] {
        assert!(
            has_marker(&quads, &format!("{base}/{holon}"), "isHolon"),
            "mid-chain {holon} must co-fire the C1 holon projection"
        );
    }
    for non_holon in ["ShipAssistant", "SearchQuery"] {
        assert!(
            !has_marker(&quads, &format!("{base}/{non_holon}"), "isHolon"),
            "the root/leaf {non_holon} must NOT be a holon"
        );
    }
    // C2 emergence, C3 constraint, C4 agency: one verdict each proves the family is engaged.
    assert!(
        has_binary(
            &quads,
            &format!("{base}/AssessCoherence"),
            "assessmentVerdict",
            &format!("{LOGIC}Emergent")
        ),
        "plan coherence (borne by the whole, not theory-reduced) must be Emergent"
    );
    assert!(
        has_binary(
            &quads,
            &format!("{base}/GovGrounded"),
            "constraintVerdict",
            &format!("{LOGIC}ConstraintBinding")
        ),
        "the grounded-only governance must bind the retrieval sub-goal"
    );
    assert!(
        has_binary(
            &quads,
            &format!("{base}/IntegralRetrieve"),
            "agencyVerdict",
            &format!("{LOGIC}HolonIntegral")
        ),
        "the both-capacity retrieval sub-agent must be HolonIntegral"
    );

    // ── NON-PROPAGATION (C2): the emergent ex:PlanCoherence stays on the root goal and never
    // inherits down logic:properPartOf to any sub-goal — non-inheritance is structural. ──
    for sub_goal in ["AnswerQuery", "RetrieveContext", "SearchQuery"] {
        assert!(
            !has_binary(
                &quads,
                &format!("{base}/{sub_goal}"),
                "bearsProperty",
                &format!("{base}/PlanCoherence")
            ),
            "emergent PlanCoherence must NOT propagate down to the sub-goal {sub_goal}"
        );
    }

    // ── NON-TRANSITIVITY (C3): ex:SearchQuery is a proper part of ex:RetrieveContext (the
    // constraint target), hence transitively of ex:AnswerQuery (the constraint whole), but no
    // logic:DownwardConstraint names it, so it receives NO constraintVerdict of any value —
    // governance does not cascade down the properPartOf closure. ──
    for verdict in [
        "ConstraintBinding",
        "ConstraintOverridden",
        "ConstraintUnknown",
    ] {
        assert!(
            !has_binary(
                &quads,
                &format!("{base}/SearchQuery"),
                "constraintVerdict",
                &format!("{LOGIC}{verdict}")
            ),
            "downward constraint must NOT cascade to the grandchild sub-goal ex:SearchQuery (got {verdict})"
        );
    }
}

// ── Property characteristics (H4) ────────────────────────────────────────────────

const OWL_TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_SYMMETRIC: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const OWL_IRREFLEXIVE: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
const OWL_ASYMMETRIC: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";

/// Whether a plain edge `(subject, predicate_iri, object_iri)` is materialized.
fn has_edge(quads: &[FoundationQuad], subject: &str, predicate: &str, object: &str) -> bool {
    let obj = format!("<{object}>");
    quads
        .iter()
        .any(|q| q.subject == subject && q.predicate == predicate && q.object == obj)
}

#[test]
fn characteristic_transitive_closure_via_owl_marker() {
    let b = "https://example.org/char/transitive";
    let p = format!("{b}/before");
    let nq = format!(
        "<{p}> <{RDF_TYPE_P}> <{OWL_TRANSITIVE}> <{b}/g> .\n\
         <{b}/A> <{p}> <{b}/B> <{b}/g> .\n\
         <{b}/B> <{p}> <{b}/C> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_edge(&quads, &format!("{b}/A"), &p, &format!("{b}/C")),
        "transitive closure must derive A→C"
    );
}

#[test]
fn characteristic_transitive_closure_via_logic_record() {
    // The canonical carrier: a central record, not a marker on the property itself.
    let b = "https://example.org/char/record";
    let p = format!("{b}/before");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}transitiveProperty> <{b}/g> .\n\
         <{b}/A> <{p}> <{b}/B> <{b}/g> .\n\
         <{b}/B> <{p}> <{b}/C> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_edge(&quads, &format!("{b}/A"), &p, &format!("{b}/C")),
        "a logic:characterizes/characteristicSort record must drive closure like an owl: marker"
    );
}

#[test]
fn functional_marker_derives_functional_property_edge_via_owl_marker() {
    // The deprecated OWL type marker still lifts `functionalProperty(?P, ?P)` into the
    // chase — the derivation source raw external/conformance graphs and the OWL grounding
    // VIEW continue to supply after the slice source declarations are removed.
    let b = "https://example.org/char/functional-marker";
    let p = format!("{b}/spouse");
    let nq = format!("<{p}> <{RDF_TYPE_P}> <{OWL_FUNCTIONAL}> <{b}/g> .\n");
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_edge(&quads, &p, &format!("{LOGIC}functionalProperty"), &p),
        "owl:FunctionalProperty must derive functionalProperty(spouse, spouse)"
    );
}

#[test]
fn functional_carrier_derives_functional_property_edge_via_logic_record() {
    // The canonical carrier: a central `logic:PropertyCharacteristicAssertion` record naming
    // the property functional must drive the SAME `functionalProperty(?P, ?P)` marker the OWL
    // type marker does — the derivation source that survives the `owl:FunctionalProperty`
    // slice-source removal, keeping the relator-mediation entity-count identical.
    let b = "https://example.org/char/functional-record";
    let p = format!("{b}/spouse");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}functionalProperty> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_edge(&quads, &p, &format!("{LOGIC}functionalProperty"), &p),
        "a logic:characterizes/characteristicSort functionalProperty record must derive \
         functionalProperty(spouse, spouse) like the owl: marker"
    );
}

#[test]
fn characteristic_symmetric_mirror() {
    let b = "https://example.org/char/symmetric";
    let p = format!("{b}/counter");
    let nq = format!(
        "<{p}> <{RDF_TYPE_P}> <{OWL_SYMMETRIC}> <{b}/g> .\n\
         <{b}/A> <{p}> <{b}/B> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_edge(&quads, &format!("{b}/B"), &p, &format!("{b}/A")),
        "symmetric mirror must derive B→A"
    );
}

#[test]
fn characteristic_irreflexive_self_edge_violates() {
    let b = "https://example.org/char/irreflexive";
    let p = format!("{b}/counter");
    let nq = format!(
        "<{p}> <{RDF_TYPE_P}> <{OWL_IRREFLEXIVE}> <{b}/g> .\n\
         <{b}/A> <{p}> <{b}/A> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_violation(&quads, &format!("{b}/A"), "IrreflexivityViolation"),
        "an irreflexive property holding of a self-pair must fire IrreflexivityViolation"
    );
}

#[test]
fn characteristic_symmetric_irreflexive_cycle_violates() {
    // counterGoal shape: symmetric + irreflexive. A self-edge is a violation; a plain
    // A↔B pair (mirror) is NOT (distinct endpoints).
    let b = "https://example.org/char/countergoal";
    let p = format!("{b}/counterGoal");
    let nq = format!(
        "<{p}> <{RDF_TYPE_P}> <{OWL_SYMMETRIC}> <{b}/g> .\n\
         <{p}> <{RDF_TYPE_P}> <{OWL_IRREFLEXIVE}> <{b}/g> .\n\
         <{b}/A> <{p}> <{b}/B> <{b}/g> .\n\
         <{b}/S> <{p}> <{b}/S> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_edge(&quads, &format!("{b}/B"), &p, &format!("{b}/A")),
        "symmetric mirror still derives B→A"
    );
    assert!(
        has_violation(&quads, &format!("{b}/S"), "IrreflexivityViolation"),
        "a goal that is its own counter-goal must fire IrreflexivityViolation"
    );
    assert!(
        !has_violation(&quads, &format!("{b}/A"), "IrreflexivityViolation"),
        "a symmetric pair between DISTINCT goals is not an irreflexivity violation"
    );
}

#[test]
fn characteristic_asymmetric_mutual_pair_violates() {
    let b = "https://example.org/char/asymmetric";
    let p = format!("{b}/strictlyBefore");
    let nq = format!(
        "<{p}> <{RDF_TYPE_P}> <{OWL_ASYMMETRIC}> <{b}/g> .\n\
         <{b}/A> <{p}> <{b}/B> <{b}/g> .\n\
         <{b}/B> <{p}> <{b}/A> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_violation(&quads, &format!("{b}/A"), "AsymmetryViolation")
            || has_violation(&quads, &format!("{b}/B"), "AsymmetryViolation"),
        "an asymmetric property holding both ways must fire AsymmetryViolation"
    );
}

#[test]
fn characteristic_acyclic_cycle_violates_without_materializing_closure() {
    // linkNext shape: acyclic + NOT transitive. A cycle A→B→C→A fires AcyclicityViolation
    // on each node; the closure (e.g. A→C) is NOT materialized (one-step semantics kept).
    let b = "https://example.org/char/acyclic";
    let p = format!("{b}/linkNext");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}acyclicProperty> <{b}/g> .\n\
         <{b}/A> <{p}> <{b}/B> <{b}/g> .\n\
         <{b}/B> <{p}> <{b}/C> <{b}/g> .\n\
         <{b}/C> <{p}> <{b}/A> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_violation(&quads, &format!("{b}/A"), "AcyclicityViolation"),
        "a node that reaches itself through an acyclic property must fire AcyclicityViolation"
    );
    assert!(
        !has_edge(&quads, &format!("{b}/A"), &p, &format!("{b}/C")),
        "an acyclic property must NOT be transitively closed (no A→C edge materialized)"
    );
}

#[test]
fn characteristic_acyclic_violation_witness_is_on_the_cycle() {
    // A's lexicographically-smallest outgoing edge (A->B) is a DEAD END; the cycle
    // runs A->Z->A. The violation provenance must witness the on-cycle A->Z edge,
    // never the decoy A->B edge that opens no cycle.
    let b = "https://example.org/char/acyclic-witness";
    let p = format!("{b}/linkNext");
    let rec = format!("{b}/rec");
    let a = format!("{b}/A");
    let dead = format!("{b}/B"); // sorts before Z, dead end
    let cyc = format!("{b}/Z"); // on the cycle
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}acyclicProperty> <{b}/g> .\n\
         <{a}> <{p}> <{dead}> <{b}/g> .\n\
         <{a}> <{p}> <{cyc}> <{b}/g> .\n\
         <{cyc}> <{p}> <{a}> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    let pred = format!("{LOGIC}violation");
    let obj = format!("<{LOGIC}AcyclicityViolation>");
    let viol = quads
        .iter()
        .find(|q| q.subject == a && q.predicate == pred && q.object == obj)
        .expect("A reaches itself and must fire an AcyclicityViolation");
    let on_cycle = triple_reifier(&a, &p, &cyc).unwrap();
    let decoy = triple_reifier(&a, &p, &dead).unwrap();
    assert!(
        viol.source_quad_ids.contains(&on_cycle),
        "witness must be the on-cycle A->Z edge, got {:?}",
        viol.source_quad_ids
    );
    assert!(
        !viol.source_quad_ids.contains(&decoy),
        "witness must NOT be the dead-end A->B edge, got {:?}",
        viol.source_quad_ids
    );
}

#[test]
fn characteristic_acyclic_chain_is_clean() {
    // A non-cyclic chain A→B→C fires no violation.
    let b = "https://example.org/char/acyclic-clean";
    let p = format!("{b}/linkNext");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}acyclicProperty> <{b}/g> .\n\
         <{b}/A> <{p}> <{b}/B> <{b}/g> .\n\
         <{b}/B> <{p}> <{b}/C> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_violation(&quads, &format!("{b}/A"), "AcyclicityViolation")
            && !has_violation(&quads, &format!("{b}/B"), "AcyclicityViolation")
            && !has_violation(&quads, &format!("{b}/C"), "AcyclicityViolation"),
        "an acyclic chain must fire no AcyclicityViolation"
    );
}

#[test]
fn relatum_distinctness_coincident_roles_violate() {
    // Commitment shape: committedAgent and commitmentBeneficiary must be distinct.
    let b = "https://example.org/distinct";
    let cls = format!("{b}/Commitment");
    let r1 = format!("{b}/committedAgent");
    let r2 = format!("{b}/commitmentBeneficiary");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}distinctnessTarget> <{cls}> <{b}/g> .\n\
         <{rec}> <{LOGIC}distinctnessRole> <{r1}> <{b}/g> .\n\
         <{rec}> <{LOGIC}distinctnessRole> <{r2}> <{b}/g> .\n\
         <{b}/c1> <{RDF_TYPE_P}> <{cls}> <{b}/g> .\n\
         <{b}/c1> <{r1}> <{b}/alice> <{b}/g> .\n\
         <{b}/c1> <{r2}> <{b}/alice> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_violation(&quads, &format!("{b}/c1"), "RelatumDistinctnessViolation"),
        "a commitment whose agent and beneficiary coincide must fire RelatumDistinctnessViolation"
    );
}

#[test]
fn relatum_distinctness_distinct_roles_are_clean() {
    let b = "https://example.org/distinct-clean";
    let cls = format!("{b}/Commitment");
    let r1 = format!("{b}/committedAgent");
    let r2 = format!("{b}/commitmentBeneficiary");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}distinctnessTarget> <{cls}> <{b}/g> .\n\
         <{rec}> <{LOGIC}distinctnessRole> <{r1}> <{b}/g> .\n\
         <{rec}> <{LOGIC}distinctnessRole> <{r2}> <{b}/g> .\n\
         <{b}/c1> <{RDF_TYPE_P}> <{cls}> <{b}/g> .\n\
         <{b}/c1> <{r1}> <{b}/alice> <{b}/g> .\n\
         <{b}/c1> <{r2}> <{b}/bob> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_violation(&quads, &format!("{b}/c1"), "RelatumDistinctnessViolation"),
        "a commitment with distinct agent and beneficiary must be clean"
    );
}

#[test]
fn relatum_distinctness_malformed_role_count_hard_fails() {
    // A RelatumDistinctnessAssertion naming only ONE role is malformed. The native pass
    // must fail closed — exactly as the SHACL projector rejects the same record — never
    // silently drop it (no-optionality: the two enforcement halves of one axiom must
    // agree on malformed input).
    let b = "https://example.org/distinct-malformed";
    let cls = format!("{b}/Commitment");
    let r1 = format!("{b}/committedAgent");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{RDF_TYPE_P}> <{LOGIC}RelatumDistinctnessAssertion> <{b}/g> .\n\
         <{rec}> <{LOGIC}distinctnessTarget> <{cls}> <{b}/g> .\n\
         <{rec}> <{LOGIC}distinctnessRole> <{r1}> <{b}/g> .\n"
    );
    let err = evaluate(&store_from(&nq), AntiRigidityPolicy::WitnessObligation)
        .expect_err("a one-role distinctness assertion must hard-fail, not be skipped");
    assert!(
        err.message().contains("exactly two") && err.message().contains(&rec),
        "error must name the malformed record and the two-role requirement, got: {err}"
    );
}

#[test]
fn characteristic_unmarked_property_is_inert() {
    // The counterpartOf shape: a plain (unmarked) property must NOT be closed — proving
    // the pass fires ONLY on declared characteristics (non-transitivity is respected).
    let b = "https://example.org/char/unmarked";
    let p = format!("{b}/counterpartOf");
    let nq = format!(
        "<{b}/A> <{p}> <{b}/B> <{b}/g> .\n\
         <{b}/B> <{p}> <{b}/C> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_edge(&quads, &format!("{b}/A"), &p, &format!("{b}/C")),
        "an unmarked property must not be transitively closed"
    );
}

#[test]
fn characteristic_carrier_agreement_holds_when_both_present() {
    // A DL-projectable logic: record WITH its OWL projection: carriers agree, no drift.
    let b = "https://example.org/char/agree";
    let p = format!("{b}/before");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{p}> <{RDF_TYPE_P}> <{OWL_TRANSITIVE}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}transitiveProperty> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_violation(&quads, &p, "CharacteristicCarrierDisagreement"),
        "a logic: record whose OWL projection is present must not fire a disagreement"
    );
}

#[test]
fn characteristic_carrier_agreement_fires_when_owl_marker_dropped() {
    // The canonical logic: record is present but its OWL projection is missing — the
    // carriers have drifted, so the property-characteristic agreement gate must fire.
    let b = "https://example.org/char/drift";
    let p = format!("{b}/before");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}transitiveProperty> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_violation(&quads, &p, "CharacteristicCarrierDisagreement"),
        "a DL-projectable logic: record without its OWL marker must fire a disagreement"
    );
}

#[test]
fn characteristic_carrier_agreement_ignores_logic_only_sorts() {
    // Irreflexive/asymmetric are logic:-only by design (DL-clean, EL-safe): a record for
    // them carries no OWL projection and must NOT be read as a carrier disagreement.
    let b = "https://example.org/char/logiconly";
    let p = format!("{b}/counterGoal");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}irreflexiveProperty> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_violation(&quads, &p, "CharacteristicCarrierDisagreement"),
        "a logic:-only irreflexive record must not fire a carrier disagreement"
    );
}

#[test]
fn functional_carrier_materializes_marker_and_clears_disagreement() {
    // A functional property carried ONLY by the canonical central record — NO
    // owl:FunctionalProperty / logic:functionalProperty rdf:type marker in the input, exactly
    // as the slices now author it after the owl:FunctionalProperty source declarations were
    // removed. The closure materializes the marker (?P rdf:type logic:functionalProperty) from
    // the carrier, so the carrier-agreement cross-check sees the canonical record and its
    // derived projection AGREE and raises NO disagreement.
    let b = "https://example.org/char/functional-carrier-only";
    let p = format!("{b}/hasLead");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}functionalProperty> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        has_edge(
            &quads,
            &p,
            RDF_TYPE_P,
            &format!("{LOGIC}functionalProperty")
        ),
        "the closure must materialize the logic:functionalProperty type marker from the carrier"
    );
    assert!(
        !has_violation(&quads, &p, "CharacteristicCarrierDisagreement"),
        "a carrier-only functional property must not fire a disagreement once the marker is \
         materialized"
    );
}

#[test]
fn transitive_carrier_without_marker_still_fires_disagreement() {
    // The functional marker is materialized from its carrier, but transitive/symmetric stay
    // DUAL-authored: the closure never materializes a transitive marker, so a transitive record
    // WITHOUT its hand-authored owl:TransitiveProperty marker must STILL fire a disagreement.
    // This guards against the functional materialization accidentally clearing the other sorts.
    let b = "https://example.org/char/transitive-carrier-only";
    let p = format!("{b}/before");
    let rec = format!("{b}/rec");
    let nq = format!(
        "<{rec}> <{LOGIC}characterizes> <{p}> <{b}/g> .\n\
         <{rec}> <{LOGIC}characteristicSort> <{LOGIC}transitiveProperty> <{b}/g> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    assert!(
        !has_edge(
            &quads,
            &p,
            RDF_TYPE_P,
            &format!("{LOGIC}transitiveProperty")
        ),
        "the closure must NOT materialize a transitive marker (transitive stays dual-authored)"
    );
    assert!(
        has_violation(&quads, &p, "CharacteristicCarrierDisagreement"),
        "a transitive carrier without its owl: marker must still fire a disagreement"
    );
}
