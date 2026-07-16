// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Native OWL 2 RL entailment harness for the migrated reasoning pytest cluster.
//!
//! ## What this replaces
//! The `python` CI lane (~45 min) was dominated by OWL/EL/DL reasoning tests that each rebuilt
//! a reasoned graph via the OWL-2-RL chase (the former `native_rl_rdflib.native_rl_closure`
//! rdflib adapter over `gmeow_logic.rl_closure_nt`, now deleted — its last reasoning consumer
//! migrated here).
//! The per-slice entailment tests follow a single shape — parse the relevant slice `module.ttl`
//! files, inject a tiny test A-Box, close under RL, assert a derived triple is present (and a
//! contrasting one absent). This harness is the native twin of that
//! `gmeow_tools` `_materialize(module, *abox)` pattern: [`scoped_closure`] parses the same
//! `module.ttl` files, injects the same A-Box, and runs the native RL chase
//! ([`gmeow_logic::reason::rl_closure`]) over that **small, scoped** input — seconds, Docker-free,
//! once per test instead of a full-ontology chase.
//!
//! ## RL lane, not EL/DL
//! The Python suites close under OWL **2 RL** (`gmeow_logic.rl_closure_nt`, the 4-ary
//! predicate-as-DATA reformulation in [`gmeow_logic::reason::rl`]). `reason_native`'s EL/DL
//! calculus is a *different* rule set (its ternary encoding cannot express the RL meta-rules that
//! quantify over the property position — see the `reason/rl.rs` header) and does not derive these
//! property-chain / `owl:equivalentClass` classification entailments. Fidelity to the migrated
//! assertions therefore requires the **RL** closure (`rl_closure`), never `reason_all`.
//!
//! ## Single-world flattening
//! `_materialize` parses every module into one default graph and closes "in one world". Turtle
//! has no named graphs, so the parsed quads already land in the single default world; the injected
//! A-Box ([`iri_quad`]) lives there too — so RL rules fire across the whole scoped TBox.

use std::path::PathBuf;

use gmeow_logic::reason::{RlClosure, rl_closure};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, parse_dataset};

/// The gmeow ontology namespace (`config.NAMESPACE` = `ONTOLOGY_IRI + "/"`).
pub const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The canonical mathematics grounding namespace.
pub const MATH: &str = "https://blackcatinformatics.ca/math/";
/// The example/test A-Box namespace used by the migrated competency tests (`tests.EX`).
pub const EX: &str = "https://example.org/test/";
/// `rdf:type`.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:subClassOf`.
pub const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// `https://blackcatinformatics.ca/gmeow/<local>`.
pub fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// `https://blackcatinformatics.ca/math/<local>`.
pub fn math(local: &str) -> String {
    format!("{MATH}{local}")
}

/// `https://example.org/test/<local>`.
pub fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

/// `https://example.org/mereology/<local>` — the A-Box namespace the mereology tests use.
pub fn exm(local: &str) -> String {
    format!("https://example.org/mereology/{local}")
}

/// Repo root (`CARGO_MANIFEST_DIR` = `<repo>/crates/logic`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The canonical terms file of a slice, by `<group>/<name>` (e.g. `extensions/genealogy`).
/// Mirrors `gmeow_tools.slices.Slice.module_path` (`slices/<g>/<n>/module.ttl`).
pub fn module(slice: &str) -> String {
    format!("slices/{slice}/module.ttl")
}

/// Parse the given Turtle files (repo-relative paths) into default-world quads. HARD-FAIL
/// (panic) if a file is missing or unparsable — no skip, no optional fallback (NO-OPTIONALITY):
/// a missing reasoning corpus file is a build error, not a silently-skipped test.
fn turtle_quads(rel_paths: &[String]) -> Vec<RdfQuad> {
    let root = repo_root();
    let mut quads = Vec::new();
    for rel in rel_paths {
        let path = root.join(rel);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("missing ontology source {}: {e}", path.display()));
        // Parse through the canonical native codec directly into the frozen IR;
        // its `RdfQuad`s feed the scoped closure builder below.
        let dataset = parse_dataset(&bytes, "text/turtle", None)
            .unwrap_or_else(|e| panic!("Turtle parse failed for {}: {e}", path.display()));
        quads.extend(dataset.owned_quads());
    }
    quads
}

fn dataset_from_quads(quads: Vec<RdfQuad>) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid scoped test dataset")
}

/// An RL closure of the named slice modules plus injected `abox` quads — the native twin of the
/// Python `_materialize(*modules, abox)` pattern. `slices` are `<group>/<name>` ids; the relevant
/// `module.ttl` files (small TBox) plus the tiny A-Box close in seconds, Docker-free.
pub fn scoped_closure(slices: &[&str], abox: &[RdfQuad]) -> RlClosure {
    let mut paths: Vec<String> = slices.iter().map(|s| module(s)).collect();
    paths.sort();
    let mut quads = turtle_quads(&paths);
    quads.extend_from_slice(abox);
    let dataset = dataset_from_quads(quads);
    rl_closure(dataset.as_ref()).expect("scoped OWL 2 RL closure should succeed")
}

/// An RL closure of arbitrary Turtle source files (repo-relative) plus injected `abox` — for the
/// few tests that parse a non-`module.ttl` source (mapping/equivalence files, examples).
pub fn scoped_closure_files(rel_paths: &[&str], abox: &[RdfQuad]) -> RlClosure {
    let paths: Vec<String> = rel_paths.iter().map(|s| (*s).to_owned()).collect();
    let mut quads = turtle_quads(&paths);
    quads.extend_from_slice(abox);
    let dataset = dataset_from_quads(quads);
    rl_closure(dataset.as_ref()).expect("scoped OWL 2 RL closure should succeed")
}

/// An IRI-subject / IRI-object quad in the single default world (matches the parsed Turtle).
pub fn iri_quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o))
}

/// Strip an optional surrounding `<…>` so closure terms compare against bare IRIs regardless of
/// how `RlTriple` renders each position (subject/predicate are bare, object is `<iri>`-wrapped).
fn unwrap_iri(term: &str) -> &str {
    term.strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or(term)
}

/// `true` iff the closure contains the IRI triple `s p o`.
pub fn contains(closure: &RlClosure, s: &str, p: &str, o: &str) -> bool {
    closure.triples.iter().any(|t| {
        unwrap_iri(&t.subject) == s && unwrap_iri(&t.predicate) == p && unwrap_iri(&t.object) == o
    })
}

/// `true` iff the closure types `individual` as `class` (`individual a class`).
pub fn has_type(closure: &RlClosure, individual: &str, class: &str) -> bool {
    contains(closure, individual, RDF_TYPE, class)
}

/// Assert that an individual typed `x_class` is inferred to be a `target_class` (the common
/// "X ⊑ Observation"-style subsumption): inject `ex:<local> a gmeow:<x_class>` over `slices` and
/// check `gmeow:<target_class>` is derived. `local` keeps each test's A-Box individual distinct.
pub fn assert_specialises(slices: &[&str], local: &str, x_class: &str, target_class: &str) {
    let ind = ex(local);
    let abox = vec![iri_quad(&ind, RDF_TYPE, &gmeow(x_class))];
    let closure = scoped_closure(slices, &abox);
    assert!(
        has_type(&closure, &ind, &gmeow(target_class)),
        "ex:{local} a {target_class} ({x_class} ⊑ {target_class})"
    );
}

/// Assert `s p o` is ENTAILED but not asserted: present in `closure`, absent from `asserted` (the
/// injected A-Box). The native twin of the Python "absent before reasoning, present after"
/// contrast (`*_is_entailed_not_asserted`).
pub fn assert_entailed(closure: &RlClosure, asserted: &[RdfQuad], s: &str, p: &str, o: &str) {
    let asserted_here = asserted.iter().any(|q| {
        matches!(&q.subject, RdfTerm::Iri(qs) if qs == s)
            && q.predicate == p
            && matches!(&q.object, RdfTerm::Iri(qo) if qo == o)
    });
    assert!(
        !asserted_here,
        "{s} {p} {o} is authored in the A-Box; it cannot be 'entailed not asserted'"
    );
    assert!(
        contains(closure, s, p, o),
        "{s} {p} {o} must be ENTAILED by the OWL 2 RL closure (authored nowhere)"
    );
}

// ── Smoke gate ────────────────────────────────────────────────────────────────────────────
// Prove the scoped closure is genuinely reasoning before any real migration relies on it: a
// known property-chain entailment is present (positive) AND a known non-entailment is absent
// (negative) — so a silently-empty / mis-parsed closure cannot pass every positive check by
// vacuity.

#[test]
fn smoke_property_chain_entailment_and_negative() {
    // hasParent ∘ hasParent ⊑ hasAncestor: inject a two-step parent chain over the
    // genealogy module's TBox (the same module the Python ancestry test parses).
    let (a, b, c, unrelated) = (ex("a"), ex("b"), ex("c"), ex("unrelated"));
    let abox = vec![
        iri_quad(&a, &gmeow("hasParent"), &b),
        iri_quad(&b, &gmeow("hasParent"), &c),
    ];
    let closure = scoped_closure(&["extensions/genealogy"], &abox);

    // Positive: the grandparent ancestry edge is derived, authored nowhere.
    assert_entailed(&closure, &abox, &a, &gmeow("hasAncestor"), &c);

    // Negative: no spurious ancestry edge to an individual in no chain.
    assert!(
        !contains(&closure, &a, &gmeow("hasAncestor"), &unrelated),
        "ex:a must NOT have a spurious hasAncestor to the unrelated ex:unrelated"
    );

    // Non-trivial closure (guards against an empty/mis-parsed module silently passing).
    assert!(
        closure.triples.len() > 10,
        "the scoped RL closure should be non-trivial; got {}",
        closure.triples.len()
    );
}

// ── Native twins of the retired reasoning-entailments pytest cluster ─────────────
// The native twins of the `_materialize(module, *abox)` positive-entailment tests. The
// three `reasoning_cases` monkeypatch tests (two-axis / two-kind / run_all order) have no
// native twin — they exercised the Python Docker-orchestration oracle layer, which was
// retired together with the Docker ELK/HermiT cross-check lane (superseded by the native
// purrdf-entail crosscheck).

#[test]
fn ancestry_is_derived_not_asserted() {
    // hasParent ∘ hasParent ⊑ hasAncestor (transitive sub-property), DERIVED.
    let (a, b, c) = (ex("a"), ex("b"), ex("c"));
    let abox = vec![
        iri_quad(&a, &gmeow("hasParent"), &b),
        iri_quad(&b, &gmeow("hasParent"), &c),
    ];
    let closure = scoped_closure(&["extensions/genealogy"], &abox);
    // The grandparent edge is asserted nowhere yet is entailed.
    assert_entailed(&closure, &abox, &a, &gmeow("hasAncestor"), &c);
    // Parentage feeds ancestry; the transitive inverse closes descendants too.
    assert!(
        contains(&closure, &a, &gmeow("hasAncestor"), &b),
        "ex:a hasAncestor ex:b (parentage ⊑ ancestry)"
    );
    assert!(
        contains(&closure, &c, &gmeow("hasDescendant"), &a),
        "ex:c hasDescendant ex:a (transitive inverse)"
    );
}

#[test]
fn location_propagates_through_containment() {
    // locatedAt ∘ containedInPlace ⊑ locatedAt: in your room means in your city.
    let (thing, room, city) = (ex("thing"), ex("room"), ex("city"));
    let abox = vec![
        iri_quad(&thing, &gmeow("locatedAt"), &room),
        iri_quad(&room, &gmeow("containedInPlace"), &city),
    ];
    let closure = scoped_closure(&["core/places"], &abox);
    assert!(
        contains(&closure, &thing, &gmeow("locatedAt"), &city),
        "ex:thing locatedAt ex:city (location through containment)"
    );
}

#[test]
fn suborganization_is_transitive() {
    // subOrganizationOf is transitive — a team is part of the parent company.
    let (team, div, corp) = (ex("team"), ex("div"), ex("corp"));
    let abox = vec![
        iri_quad(&team, &gmeow("subOrganizationOf"), &div),
        iri_quad(&div, &gmeow("subOrganizationOf"), &corp),
    ];
    let closure = scoped_closure(&["core/organization"], &abox);
    assert!(
        contains(&closure, &team, &gmeow("subOrganizationOf"), &corp),
        "ex:team subOrganizationOf ex:corp (transitive)"
    );
}

#[test]
fn proximity_measurement_is_a_measurement() {
    // ProximityMeasurement ⊑ Measurement is asserted and survives materialization.
    let (commute, home, dist) = (ex("commute"), ex("home"), ex("dist"));
    let abox = vec![
        iri_quad(&commute, RDF_TYPE, &gmeow("ProximityMeasurement")),
        iri_quad(&commute, &gmeow("proximityTo"), &home),
        iri_quad(&commute, &gmeow("observationResult"), &dist),
        iri_quad(&dist, RDF_TYPE, &math("Quantity")),
    ];
    let closure = scoped_closure(&["core/places"], &abox);
    // The asserted subClassOf is preserved through materialization.
    assert!(
        contains(
            &closure,
            &gmeow("ProximityMeasurement"),
            RDFS_SUBCLASSOF,
            &gmeow("Measurement")
        ),
        "ProximityMeasurement ⊑ Measurement preserved"
    );
    // And the instance is typed in both the asserted and reasoned graph.
    assert!(
        has_type(&closure, &commute, &gmeow("ProximityMeasurement")),
        "ex:commute a ProximityMeasurement (asserted)"
    );
    assert!(
        has_type(&closure, &commute, &gmeow("Measurement")),
        "ex:commute a Measurement (derived via cax-sco)"
    );
}

// ── Migrated from tests/test_mereology.py ─────────────────────────────────────────────
// The three `_materialize(*modules, abox=...)` propagation tests. The three structural tests
// (`_universal_part_properties_*`, `_existing_part_like_relations_*`, `_no_winner_or_cardinality_*`)
// run over the ASSERTED merged graph with no closure — they are TBox-well-formedness checks that
// belong to the slicetest structural migration, not this reasoning migration; left in place.

#[test]
fn specialized_part_relations_entail_generic_parthood() {
    let (room, building) = (exm("room"), exm("building"));
    let (team, division) = (exm("team"), exm("division"));
    let (talk, session) = (exm("talk"), exm("session"));
    let (message, mime_part) = (exm("message"), exm("mimePart"));
    let abox = vec![
        iri_quad(&room, &gmeow("containedInPlace"), &building),
        iri_quad(&team, &gmeow("subOrganizationOf"), &division),
        iri_quad(&talk, &gmeow("subEventOf"), &session),
        iri_quad(&message, &gmeow("hasBodyPart"), &mime_part),
    ];
    let closure = scoped_closure(
        &[
            "core/kernel",
            "core/places",
            "core/organization",
            "core/events",
            "extensions/email",
        ],
        &abox,
    );
    assert!(
        contains(&closure, &room, &gmeow("partOf"), &building),
        "containedInPlace ⊑ partOf"
    );
    assert!(
        contains(&closure, &team, &gmeow("partOf"), &division),
        "subOrganizationOf ⊑ partOf"
    );
    assert!(
        contains(&closure, &talk, &gmeow("partOf"), &session),
        "subEventOf ⊑ partOf"
    );
    assert!(
        contains(&closure, &message, &gmeow("hasPart"), &mime_part),
        "hasBodyPart ⊑ hasPart"
    );
}

#[test]
fn member_of_propagates_through_suborganization() {
    let (alex, team, division, company) =
        (exm("alex"), exm("team"), exm("division"), exm("company"));
    let abox = vec![
        iri_quad(&alex, &gmeow("memberOf"), &team),
        iri_quad(&team, &gmeow("subOrganizationOf"), &division),
        iri_quad(&division, &gmeow("subOrganizationOf"), &company),
    ];
    let closure = scoped_closure(&["core/kernel", "core/organization"], &abox);
    assert!(
        contains(&closure, &alex, &gmeow("memberOf"), &division),
        "memberOf propagates through subOrganizationOf (one hop)"
    );
    assert!(
        contains(&closure, &alex, &gmeow("memberOf"), &company),
        "memberOf propagates through subOrganizationOf (two hops)"
    );
}

#[test]
fn event_location_propagates_through_spatial_containment() {
    let (meeting, room, building, city) =
        (exm("meeting"), exm("room"), exm("building"), exm("city"));
    let abox = vec![
        iri_quad(&meeting, &gmeow("eventLocation"), &room),
        iri_quad(&room, &gmeow("containedInPlace"), &building),
        iri_quad(&building, &gmeow("containedInPlace"), &city),
    ];
    let closure = scoped_closure(&["core/kernel", "core/places", "core/events"], &abox);
    assert!(
        contains(&closure, &meeting, &gmeow("eventLocation"), &building),
        "eventLocation propagates through spatial containment (one hop)"
    );
    assert!(
        contains(&closure, &meeting, &gmeow("eventLocation"), &city),
        "eventLocation propagates through spatial containment (two hops)"
    );
}

// ── Migrated from tests/test_competency.py (the entailment-dependent competency questions) ───
// The competency QUERY tests (`_query_terms`) answer on the ASSERTED merged graph via SPARQL
// property paths (`rdfs:subClassOf*`) and pay no closure cost — they stay in pytest (now fast),
// pending the slicetest migration. Only the two genuinely entailment-dependent contrasts
// migrate here: the ancestry property-chain answer (already covered by
// `ancestry_is_derived_not_asserted` above) and the PlaceNaming `equivalentClass` classification.

#[test]
fn place_naming_is_entailed_not_asserted() {
    // PlaceNaming ≡ NameUsage ⊓ ∃usageNamed.Place (the first owl:equivalentClass defined class):
    // a NameUsage that names a gmeow:Place is CLASSIFIED a PlaceNaming — the type is
    // entailed, authored nowhere (Principle 6 reuse, Principle 8 reasoning-centric).
    let (usage, place, toponym) = (ex("usage"), ex("place"), ex("toponym"));
    let (person_usage, person) = (ex("personUsage"), ex("person"));
    let abox = vec![
        // a name-usage that names a Place → should classify as a PlaceNaming
        iri_quad(&usage, RDF_TYPE, &gmeow("NameUsage")),
        iri_quad(&usage, &gmeow("usageNamed"), &place),
        iri_quad(&place, RDF_TYPE, &gmeow("Place")),
        iri_quad(&usage, &gmeow("usageAppellation"), &toponym),
        iri_quad(&toponym, RDF_TYPE, &gmeow("PlaceName")),
        // a name-usage that names a Person → must NOT classify as a PlaceNaming
        iri_quad(&person_usage, RDF_TYPE, &gmeow("NameUsage")),
        iri_quad(&person_usage, &gmeow("usageNamed"), &person),
        iri_quad(&person, RDF_TYPE, &gmeow("Person")),
    ];
    let closure = scoped_closure(&["core/names"], &abox);
    // Entailed: the place-naming usage is classified a PlaceNaming (authored nowhere).
    assert_entailed(&closure, &abox, &usage, RDF_TYPE, &gmeow("PlaceNaming"));
    // Negative: a name-usage that does NOT name a Place is NOT classified a PlaceNaming.
    assert!(
        !has_type(&closure, &person_usage, &gmeow("PlaceNaming")),
        "a NameUsage naming a Person must NOT be classified a PlaceNaming"
    );
}

// ── Migrated from tests/test_sensory.py ─────────────────────────────────────────
// The former `native_rl_closure` tests parse the sensory + observation modules, inject a SensoryObservation
// A-Box, and assert the OWL-RL entailment (specialization, equivalentClass inheritance, property
// chains, contested-coexistence). The structural tests (`load_merged_graph`, no closure — the
// subProperty / inverseOf / equivalentClass *asserted* TBox checks) stay in pytest.

#[test]
fn sensory_observation_specialises_observation() {
    // SensoryObservation ⊑ Observation, inferred under OWL RL.
    let so1 = ex("so1");
    let abox = vec![iri_quad(&so1, RDF_TYPE, &gmeow("SensoryObservation"))];
    let closure = scoped_closure(&["core/observations", "extensions/sensory"], &abox);
    assert!(
        has_type(&closure, &so1, &gmeow("Observation")),
        "so1 a Observation (SensoryObservation ⊑ Observation)"
    );
}

#[test]
fn sensor_specialises_agent() {
    // Sensor ⊑ Agent, inferred under OWL RL.
    let sensor1 = ex("sensor1");
    let abox = vec![iri_quad(&sensor1, RDF_TYPE, &gmeow("Sensor"))];
    let closure = scoped_closure(
        &["core/kernel", "core/observations", "extensions/sensory"],
        &abox,
    );
    assert!(
        has_type(&closure, &sensor1, &gmeow("Agent")),
        "sensor1 a Agent (Sensor ⊑ Agent)"
    );
}

#[test]
fn sensory_quantity_specializes_math_quantity() {
    // SensoryQuantity ⊑ math:Quantity: the domain result keeps the grounding authority.
    let sq1 = ex("sq1");
    let abox = vec![iri_quad(&sq1, RDF_TYPE, &gmeow("SensoryQuantity"))];
    let closure = scoped_closure(&["core/observations", "extensions/sensory"], &abox);
    assert!(
        has_type(&closure, &sq1, &math("Quantity")),
        "sq1 a math:Quantity (SensoryQuantity ⊑ math:Quantity)"
    );
}

#[test]
fn sensory_observation_el_axioms_stay_consistent() {
    // A fully-propertied SensoryObservation survives materialization (EL consistency).
    let (so2, sensor2, room1, sq2) = (ex("so2"), ex("sensor2"), ex("room1"), ex("sq2"));
    let abox = vec![
        iri_quad(&so2, RDF_TYPE, &gmeow("SensoryObservation")),
        iri_quad(&so2, &gmeow("vantage"), &sensor2),
        iri_quad(&so2, &gmeow("sensoryObservationOf"), &room1),
        iri_quad(
            &so2,
            &gmeow("sensoryProperty"),
            &gmeow("observablePropertyTemperature"),
        ),
        iri_quad(&so2, &gmeow("sensoryResult"), &sq2),
        iri_quad(&sensor2, RDF_TYPE, &gmeow("Sensor")),
        iri_quad(&room1, RDF_TYPE, &gmeow("Place")),
        iri_quad(&sq2, RDF_TYPE, &gmeow("SensoryQuantity")),
    ];
    let closure = scoped_closure(
        &[
            "core/kernel",
            "core/places",
            "core/observations",
            "extensions/sensory",
        ],
        &abox,
    );
    assert!(
        has_type(&closure, &so2, &gmeow("SensoryObservation")),
        "so2 remains a SensoryObservation after materialization"
    );
}

#[test]
fn sensory_quantity_frame_inheritance() {
    // isResultOf ∘ hasReferenceFrame ⊑ hasReferenceFrame: a SensoryQuantity result inherits the
    // observation's reference frame (isResultOf is the inverse of the result property).
    let (so3, sq3, frame_si) = (ex("so3"), ex("sq3"), ex("frameSI"));
    let abox = vec![
        iri_quad(&so3, RDF_TYPE, &gmeow("SensoryObservation")),
        iri_quad(&so3, &gmeow("sensoryResult"), &sq3),
        iri_quad(&so3, &gmeow("hasReferenceFrame"), &frame_si),
        iri_quad(&sq3, RDF_TYPE, &gmeow("SensoryQuantity")),
        iri_quad(&frame_si, RDF_TYPE, &gmeow("ReferenceFrame")),
    ];
    let closure = scoped_closure(
        &["core/observations", "core/places", "extensions/sensory"],
        &abox,
    );
    assert!(
        contains(&closure, &sq3, &gmeow("hasReferenceFrame"), &frame_si),
        "sq3 inherits the observation's reference frame via the property chain"
    );
}

#[test]
fn has_sensory_quantity_property_chain() {
    // The flat shortcut hasSensoryQuantity is derived from hasSensoryObservation ∘ sensoryResult.
    let (room2, so4, sq4) = (ex("room2"), ex("so4"), ex("sq4"));
    let abox = vec![
        iri_quad(&room2, RDF_TYPE, &gmeow("Place")),
        iri_quad(&room2, &gmeow("hasSensoryObservation"), &so4),
        iri_quad(&so4, &gmeow("sensoryResult"), &sq4),
        iri_quad(&so4, RDF_TYPE, &gmeow("SensoryObservation")),
        iri_quad(&sq4, RDF_TYPE, &gmeow("SensoryQuantity")),
    ];
    let closure = scoped_closure(&["core/observations", "extensions/sensory"], &abox);
    assert!(
        contains(&closure, &room2, &gmeow("hasSensoryQuantity"), &sq4),
        "room2 hasSensoryQuantity sq4 (flat shortcut chain)"
    );
}

#[test]
fn contested_sensory_readings_coexist() {
    // Two sensors observing the same feature with different results COEXIST (Principle 9): both
    // observations survive and both sensors are inferred Agents — no clash. (The decimal
    // quantityValue literals are decoration for the assertions and are omitted.)
    let (so_a, sensor_a, sq_a) = (ex("soA"), ex("sensorA"), ex("sqA"));
    let (so_b, sensor_b, sq_b) = (ex("soB"), ex("sensorB"), ex("sqB"));
    let room3 = ex("room3");
    let temp = gmeow("observablePropertyTemperature");
    let mut abox = Vec::new();
    for (so, sensor, sq) in [(&so_a, &sensor_a, &sq_a), (&so_b, &sensor_b, &sq_b)] {
        abox.extend([
            iri_quad(so, RDF_TYPE, &gmeow("SensoryObservation")),
            iri_quad(so, &gmeow("vantage"), sensor),
            iri_quad(so, &gmeow("sensoryObservationOf"), &room3),
            iri_quad(so, &gmeow("sensoryProperty"), &temp),
            iri_quad(so, &gmeow("sensoryResult"), sq),
            iri_quad(sensor, RDF_TYPE, &gmeow("Sensor")),
            iri_quad(sq, RDF_TYPE, &gmeow("SensoryQuantity")),
        ]);
    }
    abox.push(iri_quad(&room3, RDF_TYPE, &gmeow("Place")));
    let closure = scoped_closure(
        &[
            "core/kernel",
            "core/places",
            "core/observations",
            "extensions/sensory",
        ],
        &abox,
    );
    // Both observations survive; neither is contradicted.
    assert!(has_type(&closure, &so_a, &gmeow("SensoryObservation")));
    assert!(has_type(&closure, &so_b, &gmeow("SensoryObservation")));
    // Both sensors are inferred Agents.
    assert!(has_type(&closure, &sensor_a, &gmeow("Agent")));
    assert!(has_type(&closure, &sensor_b, &gmeow("Agent")));
}

// ── Migrated from tests/test_places.py (the coordinate/geometry observation chains) ─────────
// The other ~128 test_places.py tests are structural / SHACL / fixture checks (no closure) and
// stay in pytest. Only these two property-chain entailments migrate here.

#[test]
fn coordinate_observation_chain_fires() {
    // hasCoordinateObservation ∘ coordinateResult ⊑ hasCoordinates.
    let (place, obs, coords) = (ex("testPlace"), ex("testObs"), ex("testCoords"));
    let abox = vec![
        iri_quad(&place, RDF_TYPE, &gmeow("Place")),
        iri_quad(&place, &gmeow("hasCoordinateObservation"), &obs),
        iri_quad(&obs, RDF_TYPE, &gmeow("CoordinateObservation")),
        iri_quad(&obs, &gmeow("coordinateResult"), &coords),
        iri_quad(&coords, RDF_TYPE, &gmeow("GeoCoordinates")),
    ];
    let closure = scoped_closure(&["core/places", "core/observations"], &abox);
    assert!(
        contains(&closure, &place, &gmeow("hasCoordinates"), &coords),
        "place hasCoordinates via hasCoordinateObservation ∘ coordinateResult"
    );
}

#[test]
fn geometry_observation_chain_fires() {
    // hasCoordinateObservation ∘ geometryResult ⊑ hasGeometry.
    let (place, obs, geom) = (ex("testPlace2"), ex("testObs2"), ex("testGeom2"));
    let abox = vec![
        iri_quad(&place, RDF_TYPE, &gmeow("Place")),
        iri_quad(&place, &gmeow("hasCoordinateObservation"), &obs),
        iri_quad(&obs, RDF_TYPE, &gmeow("CoordinateObservation")),
        iri_quad(&obs, &gmeow("geometryResult"), &geom),
        iri_quad(&geom, RDF_TYPE, &gmeow("Geometry")),
    ];
    let closure = scoped_closure(&["core/places", "core/observations"], &abox);
    assert!(
        contains(&closure, &place, &gmeow("hasGeometry"), &geom),
        "place hasGeometry via hasCoordinateObservation ∘ geometryResult"
    );
}

/// `owl:Nothing` — the empty class; any subject typed `owl:Nothing` in a closure is an
/// inconsistency witness (the RL twin of an unsatisfiable individual).
pub const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

/// `true` iff no subject in `closure` is typed `owl:Nothing` (the RL consistency check: the
/// scoped TBox + A-Box closes without deriving a clash). The native twin of the Python
/// `not any(graph.subjects(RDF.type, OWL.Nothing))` assertion.
pub fn is_consistent(closure: &RlClosure) -> bool {
    !closure
        .triples
        .iter()
        .any(|t| unwrap_iri(&t.predicate) == RDF_TYPE && unwrap_iri(&t.object) == OWL_NOTHING)
}

// ── Migrated from tests/test_sensory_environment.py ────────────────────────────────────
// The five entailment tests. `test_mental_reference_frame_requires_host` (a hosted
// MentalReferenceFrame consistency + ReferenceFrame classification check) migrates here as
// `mental_reference_frame_hosted_instance_is_consistent`. The remaining structural/mapping
// tests stay in pytest.

#[test]
fn sensory_environment_el_axioms_fire() {
    // A SensoryEnvironment is inferred from environmentAtLocation's domain.
    let (env1, place1) = (ex("env1"), ex("place1"));
    let abox = vec![
        iri_quad(&env1, &gmeow("environmentAtLocation"), &place1),
        iri_quad(&place1, RDF_TYPE, &gmeow("Place")),
    ];
    let closure = scoped_closure(&["extensions/sensory-environment", "core/places"], &abox);
    assert!(
        has_type(&closure, &env1, &gmeow("SensoryEnvironment")),
        "env1 a SensoryEnvironment (environmentAtLocation domain)"
    );
}

#[test]
fn sensory_perception_specialises_standpoint_claim() {
    // SensoryPerception ⊑ StandpointClaim and ⊑ Observation.
    let perc1 = ex("perc1");
    let abox = vec![iri_quad(&perc1, RDF_TYPE, &gmeow("SensoryPerception"))];
    let closure = scoped_closure(
        &[
            "extensions/sensory-environment",
            "core/observations",
            "core/standpoint",
        ],
        &abox,
    );
    assert!(
        has_type(&closure, &perc1, &gmeow("StandpointClaim")),
        "perc1 a StandpointClaim"
    );
    assert!(
        has_type(&closure, &perc1, &gmeow("Observation")),
        "perc1 a Observation"
    );
}

#[test]
fn mental_reference_frame_specialises_reference_frame() {
    // MentalReferenceFrame ⊑ ReferenceFrame.
    let mrf1 = ex("mrf1");
    let abox = vec![iri_quad(&mrf1, RDF_TYPE, &gmeow("MentalReferenceFrame"))];
    let closure = scoped_closure(&["extensions/sensory-environment", "core/places"], &abox);
    assert!(
        has_type(&closure, &mrf1, &gmeow("ReferenceFrame")),
        "mrf1 a ReferenceFrame (MentalReferenceFrame ⊑ ReferenceFrame)"
    );
}

#[test]
fn mental_reference_frame_hosted_instance_is_consistent() {
    // A hosted MentalReferenceFrame instance is consistent under OWL 2 RL, and the
    // instance is classified a ReferenceFrame (MentalReferenceFrame ⊑ ReferenceFrame). The native
    // twin of the retained sensory-environment host-consistency test — parses the sensory-environment + places
    // modules, injects the host A-Box, closes under RL, and asserts BOTH the consistency arm (no
    // subject typed owl:Nothing) and the subsumption entailment.
    let (host1, mrf1) = (ex("host1"), ex("mrf1"));
    let abox = vec![
        iri_quad(&host1, RDF_TYPE, &gmeow("Agent")),
        iri_quad(&mrf1, RDF_TYPE, &gmeow("MentalReferenceFrame")),
        iri_quad(&mrf1, &gmeow("isHostedBy"), &host1),
    ];
    let closure = scoped_closure(&["extensions/sensory-environment", "core/places"], &abox);
    // Consistency: the ontology + hosted instance derives no owl:Nothing clash.
    assert!(
        is_consistent(&closure),
        "ontology + hosted MentalReferenceFrame instance must be consistent (no owl:Nothing)"
    );
    // Entailment: the hosted instance is classified a ReferenceFrame.
    assert!(
        has_type(&closure, &mrf1, &gmeow("ReferenceFrame")),
        "mrf1 a ReferenceFrame (MentalReferenceFrame ⊑ ReferenceFrame)"
    );
}

#[test]
fn frame_inheritance_via_coordinate_matrix() {
    // A CoordinateMatrix result inherits the observation's reference frame (the same
    // isResultOf ∘ hasReferenceFrame chain as the sensory-quantity frame test).
    let (obs1, matrix1, frame) = (ex("obs1"), ex("matrix1"), ex("frameCIEXYZ"));
    let abox = vec![
        iri_quad(&obs1, RDF_TYPE, &gmeow("SensoryObservation")),
        iri_quad(&obs1, &gmeow("observationResult"), &matrix1),
        iri_quad(&obs1, &gmeow("hasReferenceFrame"), &frame),
        iri_quad(&matrix1, RDF_TYPE, &gmeow("CoordinateMatrix")),
        iri_quad(&frame, RDF_TYPE, &gmeow("ReferenceFrame")),
    ];
    let closure = scoped_closure(
        &[
            "extensions/sensory-environment",
            "core/observations",
            "core/places",
        ],
        &abox,
    );
    assert!(
        contains(&closure, &matrix1, &gmeow("hasReferenceFrame"), &frame),
        "matrix1 inherits the observation's reference frame via the property chain"
    );
}

// ── Migrated from tests/test_observations.py ──────────────────────────
// The universal-claim-construct subsumptions, the isResultOf/frame property chains, the
// SpatialMeasurement/CoordinateObservation entailments, and the EL-consistency survivals. The
// Stream/property structural tests (`load_merged_graph`, no closure) stay in pytest.
// `test_sensory_observation_specialises_observation` is omitted — its twin already exists above.

#[test]
fn observation_el_axioms_fire() {
    // A fully-propertied Observation survives materialization (EL consistency).
    let (obs1, agent1, place1) = (ex("obs1"), ex("agent1"), ex("place1"));
    let abox = vec![
        iri_quad(&obs1, RDF_TYPE, &gmeow("Observation")),
        iri_quad(&obs1, &gmeow("vantage"), &agent1),
        iri_quad(&obs1, &gmeow("observedFeature"), &place1),
        iri_quad(&agent1, RDF_TYPE, &gmeow("Agent")),
        iri_quad(&place1, RDF_TYPE, &gmeow("Place")),
    ];
    let closure = scoped_closure(&["core/observations"], &abox);
    assert!(has_type(&closure, &obs1, &gmeow("Observation")));
}

#[test]
fn measurement_specialises_observation() {
    assert_specialises(&["core/observations"], "m1", "Measurement", "Observation");
}

#[test]
fn standpoint_claim_specialises_observation() {
    assert_specialises(
        &["core/observations"],
        "c1",
        "StandpointClaim",
        "Observation",
    );
}

#[test]
fn name_usage_specialises_observation() {
    assert_specialises(
        &["core/observations", "core/names"],
        "nu1",
        "NameUsage",
        "Observation",
    );
}

#[test]
fn identity_facet_specialises_observation() {
    assert_specialises(
        &["core/observations", "core/gender"],
        "if1",
        "IdentityFacet",
        "Observation",
    );
}

#[test]
fn rights_statement_specialises_observation() {
    assert_specialises(
        &["core/observations", "core/rights"],
        "rs1",
        "RightsStatement",
        "Observation",
    );
}

#[test]
fn kin_relationship_specialises_observation() {
    assert_specialises(
        &["core/observations", "extensions/genealogy"],
        "kr1",
        "KinRelationship",
        "Observation",
    );
}

#[test]
fn spatial_measurement_infers_observation() {
    assert_specialises(
        &["core/observations", "core/places"],
        "sm1",
        "SpatialMeasurement",
        "Observation",
    );
}

#[test]
fn is_result_of_provenance_chain() {
    // isResultOf is the inverse of observationResult: q1 isResultOf obs1 ⇒ obs1 observationResult q1.
    let (obs1, q1) = (ex("obs1"), ex("q1"));
    let abox = vec![
        iri_quad(&obs1, RDF_TYPE, &gmeow("Measurement")),
        iri_quad(&q1, RDF_TYPE, &math("Quantity")),
        iri_quad(&q1, &gmeow("isResultOf"), &obs1),
    ];
    let closure = scoped_closure(&["core/observations"], &abox);
    assert!(
        contains(&closure, &obs1, &gmeow("observationResult"), &q1),
        "obs1 observationResult q1 (inverse of isResultOf)"
    );
}

#[test]
fn observation_frame_inheritance_property_chain() {
    // inverse(observationResult) ∘ hasReferenceFrame ⊑ hasReferenceFrame: a result inherits the
    // observation's reference frame.
    let (obs1, coords1, frame) = (ex("obs1"), ex("coords1"), ex("frameWGS84"));
    let abox = vec![
        iri_quad(&obs1, &gmeow("observationResult"), &coords1),
        iri_quad(&obs1, &gmeow("hasReferenceFrame"), &frame),
        iri_quad(&coords1, RDF_TYPE, &gmeow("GeoCoordinates")),
        iri_quad(&frame, RDF_TYPE, &gmeow("ReferenceFrame")),
    ];
    let closure = scoped_closure(&["core/observations", "core/places"], &abox);
    assert!(
        contains(&closure, &coords1, &gmeow("hasReferenceFrame"), &frame),
        "coords1 inherits the observation's reference frame"
    );
}

#[test]
fn frame_inheritance_via_quantity() {
    // A Quantity result inherits the observation's reference frame.
    let (obs1, q1, frame) = (ex("obs1"), ex("q1"), ex("frameSI"));
    let abox = vec![
        iri_quad(&obs1, RDF_TYPE, &gmeow("Measurement")),
        iri_quad(&obs1, &gmeow("observationResult"), &q1),
        iri_quad(&obs1, &gmeow("hasReferenceFrame"), &frame),
        iri_quad(&q1, RDF_TYPE, &math("Quantity")),
        iri_quad(&frame, RDF_TYPE, &gmeow("ReferenceFrame")),
    ];
    let closure = scoped_closure(&["core/observations", "core/places"], &abox);
    assert!(
        contains(&closure, &q1, &gmeow("hasReferenceFrame"), &frame),
        "q1 inherits the observation's reference frame"
    );
}

#[test]
fn stream_el_axiom_stays_consistent() {
    // A Stream with streamOf survives materialization.
    let (stream1, entity1) = (ex("stream1"), ex("entity1"));
    let abox = vec![
        iri_quad(&stream1, RDF_TYPE, &gmeow("Stream")),
        iri_quad(&stream1, &gmeow("streamOf"), &entity1),
        iri_quad(&entity1, RDF_TYPE, &gmeow("Entity")),
    ];
    let closure = scoped_closure(&["core/observations"], &abox);
    assert!(has_type(&closure, &stream1, &gmeow("Stream")));
}

#[test]
fn coordinate_observation_infers_spatial_measurement() {
    // CoordinateObservation ⊑ SpatialMeasurement (⊑ Observation).
    let co1 = ex("co1");
    let abox = vec![iri_quad(&co1, RDF_TYPE, &gmeow("CoordinateObservation"))];
    let closure = scoped_closure(&["core/observations", "core/places"], &abox);
    assert!(
        has_type(&closure, &co1, &gmeow("SpatialMeasurement")),
        "co1 a SpatialMeasurement"
    );
    assert!(
        has_type(&closure, &co1, &gmeow("Observation")),
        "co1 a Observation (transitively)"
    );
}

#[test]
fn coordinate_observation_frame_inheritance() {
    // A CoordinateObservation's result inherits the observation's reference frame.
    let (co2, coords2, frame) = (ex("co2"), ex("coords2"), ex("frameWGS84"));
    let abox = vec![
        iri_quad(&co2, RDF_TYPE, &gmeow("CoordinateObservation")),
        iri_quad(&co2, &gmeow("coordinateResult"), &coords2),
        iri_quad(&co2, &gmeow("hasReferenceFrame"), &frame),
        iri_quad(&coords2, RDF_TYPE, &gmeow("GeoCoordinates")),
        iri_quad(&frame, RDF_TYPE, &gmeow("ReferenceFrame")),
    ];
    let closure = scoped_closure(&["core/observations", "core/places"], &abox);
    assert!(
        contains(&closure, &coords2, &gmeow("hasReferenceFrame"), &frame),
        "coords2 inherits the coordinate-observation's reference frame"
    );
}

#[test]
fn coordinate_observation_el_axioms_stay_consistent() {
    // A fully-propertied CoordinateObservation survives materialization.
    let (co3, agent3, place3) = (ex("co3"), ex("agent3"), ex("place3"));
    let abox = vec![
        iri_quad(&co3, RDF_TYPE, &gmeow("CoordinateObservation")),
        iri_quad(&co3, &gmeow("vantage"), &agent3),
        iri_quad(&co3, &gmeow("observedFeature"), &place3),
        iri_quad(&agent3, RDF_TYPE, &gmeow("Agent")),
        iri_quad(&place3, RDF_TYPE, &gmeow("Place")),
    ];
    let closure = scoped_closure(&["core/observations", "core/places"], &abox);
    assert!(has_type(&closure, &co3, &gmeow("CoordinateObservation")));
}

// ── Migrated from tests/test_quality.py ─────────────────────────────────────────────────────
// QualityAssessment ⊑ Observation — the same subsumption shape as the observations twins. The
// assessedEntity/Place A-Box in the Python test is decoration; the subsumption fires from the
// type alone (cax-sco). The remaining test_quality.py tests are structural (asserted graph).

#[test]
fn quality_assessment_specialises_observation() {
    assert_specialises(
        &["core/quality", "core/observations"],
        "qa1",
        "QualityAssessment",
        "Observation",
    );
}
