// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Native OWL 2 RL entailment harness for the migrated reasoning pytest cluster (issue #896).
//!
//! ## What this replaces
//! The `python` CI lane (~45 min) was dominated by OWL/EL/DL reasoning tests that each rebuilt
//! a reasoned graph via the OWL-2-RL chase (`gmeow_tools.native_rl_rdflib.native_rl_closure`).
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

use gmeow_logic::reason::{rl_closure, RlClosure};
use gmeow_rdf::oxigraph::rdf_quad_from_oxigraph;
use gmeow_rdf::{RdfQuad, RdfTerm, VecRdfStore};
use oxigraph::io::{RdfFormat, RdfParser};

/// The gmeow ontology namespace (`config.NAMESPACE` = `ONTOLOGY_IRI + "/"`).
pub const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
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
        for quad in RdfParser::from_format(RdfFormat::Turtle).for_reader(bytes.as_slice()) {
            let quad =
                quad.unwrap_or_else(|e| panic!("Turtle parse failed for {}: {e}", path.display()));
            quads.push(rdf_quad_from_oxigraph(&quad));
        }
    }
    quads
}

/// An RL closure of the named slice modules plus injected `abox` quads — the native twin of the
/// Python `_materialize(*modules, abox)` pattern. `slices` are `<group>/<name>` ids; the relevant
/// `module.ttl` files (small TBox) plus the tiny A-Box close in seconds, Docker-free.
pub fn scoped_closure(slices: &[&str], abox: &[RdfQuad]) -> RlClosure {
    let mut paths: Vec<String> = slices.iter().map(|s| module(s)).collect();
    paths.sort();
    let mut quads = turtle_quads(&paths);
    quads.extend_from_slice(abox);
    let store = VecRdfStore::with_quads(quads);
    rl_closure(&store).expect("scoped OWL 2 RL closure should succeed")
}

/// An RL closure of arbitrary Turtle source files (repo-relative) plus injected `abox` — for the
/// few tests that parse a non-`module.ttl` source (mapping/equivalence files, examples).
pub fn scoped_closure_files(rel_paths: &[&str], abox: &[RdfQuad]) -> RlClosure {
    let paths: Vec<String> = rel_paths.iter().map(|s| (*s).to_owned()).collect();
    let mut quads = turtle_quads(&paths);
    quads.extend_from_slice(abox);
    let store = VecRdfStore::with_quads(quads);
    rl_closure(&store).expect("scoped OWL 2 RL closure should succeed")
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
    // hasParent ∘ hasParent ⊑ hasAncestor (#38): inject a two-step parent chain over the
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

// ── Migrated from tests/test_reasoning_entailments.py ───────────────────────────────────────
// The native twins of the `_materialize(module, *abox)` positive-entailment tests (#38). The
// three `reasoning_cases` monkeypatch tests (two-axis / two-kind / run_all order) are NOT migrated
// — they exercise the Python Docker-orchestration layer (`gmeow_tools.reasoning_cases`), an
// independent live Python impl with no Rust twin (retain-with-reason, see MIGRATION-LEDGER.md).

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
        iri_quad(&dist, RDF_TYPE, &gmeow("ScalarQuantity")),
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

// ── Migrated from tests/test_mereology.py (#76) ─────────────────────────────────────────────
// The three `_materialize(*modules, abox=...)` propagation tests. The three structural tests
// (`_universal_part_properties_*`, `_existing_part_like_relations_*`, `_no_winner_or_cardinality_*`)
// run over the ASSERTED merged graph with no closure — they are TBox-well-formedness checks that
// belong to the #867 slicetest structural migration, not this reasoning migration; left in place.

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
