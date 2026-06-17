// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the native foundation evaluator.
//!
//! These tests build N-Quads inputs mirroring the `conformance/logic/cases/foundation/`
//! cases, run [`evaluate`], and assert (a) the materialized quad SET against the
//! goldens, and (b) the content-addressed provenance on representative quads.  Full
//! end-to-end explanation-golden parity is validated after the runner is rewired
//! (issue #636 Task 2); here we pin the quad set + provenance recipe.

use super::*;

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE_P: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

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

const MIXRIG_GOLDEN: &str =
    include_str!("../../../../conformance/logic/cases/foundation/mixrig-kind-under-role/expected/materialized.nq");

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
        "<{base}/Employment> <{RDF_TYPE_P}> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Employment> <{LOGIC}mediates> <{base}/Employee> <{base}/schema> .\n\
         <{base}/Employment> <{LOGIC}mediates> <{base}/Employer> <{base}/schema> .\n\
         <{base}/Marriage> <{RDF_TYPE_P}> <{LOGIC}Relator> <{base}/schema> .\n\
         <{base}/Marriage> <{LOGIC}mediates> <{base}/Spouse1> <{base}/schema> .\n"
    );
    let quads = run(&nq, AntiRigidityPolicy::WitnessObligation);
    // Marriage mediates only one relatum → RelComp.
    assert!(
        has_violation(&quads, &format!("{base}/Marriage"), "RelComp"),
        "Marriage (one relatum) must fire RelComp"
    );
    // Employment mediates two distinct relata → no RelComp.
    assert!(
        !has_violation(&quads, &format!("{base}/Employment"), "RelComp"),
        "Employment (two relata) must NOT fire RelComp"
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

// ── Policy parsing: unknown is a hard error ─────────────────────────────────────

#[test]
fn unknown_policy_is_err() {
    assert!(AntiRigidityPolicy::from_str("definitely-not-a-policy").is_err());
    assert_eq!(
        AntiRigidityPolicy::from_str("witness-obligation"),
        Ok(AntiRigidityPolicy::WitnessObligation)
    );
    assert_eq!(
        AntiRigidityPolicy::from_str("schema-only"),
        Ok(AntiRigidityPolicy::SchemaOnly)
    );
    assert_eq!(
        AntiRigidityPolicy::from_str("witness-required"),
        Ok(AntiRigidityPolicy::WitnessRequired)
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

    // The single source is the reifier of hasMetaClass(HonorsStudent, SubKind).
    let expected_source = triple_reifier(
        &honors,
        &format!("{LOGIC}hasMetaClass"),
        &format!("{LOGIC}SubKind"),
    )
    .unwrap();
    let expected_deriv = mint_derivation_id(ANON_RULE_IRI, &[expected_source.as_str()]);

    assert_eq!(
        is_class.rule_iri, ANON_RULE_IRI,
        "isClass must be stamped logic:rule/anonymous"
    );
    assert_eq!(
        is_class.source_quad_ids,
        vec![expected_source.clone()],
        "isClass source must be the hasMetaClass reifier"
    );
    assert_eq!(
        is_class.derivation_id, expected_deriv,
        "isClass derivation_id must equal mint_derivation_id(anon, [hasMetaClass reifier]); \
         this is the captured golden c42fdeaffa9a306d4dbdf207299be6b2a2f4e3f4"
    );
    // Pin the captured golden value too (from the live Python oracle).
    assert_eq!(
        is_class.derivation_id,
        "https://blackcatinformatics.ca/gmeow/derivation/c42fdeaffa9a306d4dbdf207299be6b2a2f4e3f4"
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

const EXACTLY_ONE_GOLDEN: &str =
    include_str!("../../../../conformance/logic/cases/foundation/exactly-one-stereotype/expected/materialized.nq");
const FREE_ROLE_GOLDEN: &str = include_str!(
    "../../../../conformance/logic/cases/foundation/free-role/expected/materialized.nq"
);
const IDENTITY_GOLDEN: &str =
    include_str!("../../../../conformance/logic/cases/foundation/identity-overlap-mixiden/expected/materialized.nq");
const RELCOMP_GOLDEN: &str =
    include_str!("../../../../conformance/logic/cases/foundation/relcomp-under-mediated/expected/materialized.nq");

#[test]
fn golden_quad_sets_match_for_single_world_cases() {
    let cases: [(&str, &str, &str); 4] = [
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
             <https://example.org/foundation/relcomp-under-mediated/Employment> <https://blackcatinformatics.ca/logic/mediates> <https://example.org/foundation/relcomp-under-mediated/Employee> <https://example.org/foundation/relcomp-under-mediated/schema> .\n\
             <https://example.org/foundation/relcomp-under-mediated/Employment> <https://blackcatinformatics.ca/logic/mediates> <https://example.org/foundation/relcomp-under-mediated/Employer> <https://example.org/foundation/relcomp-under-mediated/schema> .\n\
             <https://example.org/foundation/relcomp-under-mediated/Marriage> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Relator> <https://example.org/foundation/relcomp-under-mediated/schema> .\n\
             <https://example.org/foundation/relcomp-under-mediated/Marriage> <https://blackcatinformatics.ca/logic/mediates> <https://example.org/foundation/relcomp-under-mediated/Spouse1> <https://example.org/foundation/relcomp-under-mediated/schema> .\n",
        ),
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
