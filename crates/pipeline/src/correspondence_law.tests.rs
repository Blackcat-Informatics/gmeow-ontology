// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "http://example.org/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

fn read_query(name: &str) -> String {
    let path = format!("generated/queries/{name}");
    let bytes = query_artifacts()
        .get(&path)
        .unwrap_or_else(|| panic!("producer-selected stage-mappings carries no {path}"));
    String::from_utf8(bytes.clone()).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn query_artifacts() -> &'static BTreeMap<String, Vec<u8>> {
    static QUERIES: OnceLock<BTreeMap<String, Vec<u8>>> = OnceLock::new();
    QUERIES.get_or_init(|| {
        crate::fixture::stage_artifacts(&repo_root(), 1, "stage-mappings")
            .expect("load producer-selected query artifacts read-only")
    })
}

// ── The real committed SIOC fixture: the three CompleteOver cells round-trip exactly. ──
#[test]
fn sioc_section_law_discharged_on_the_complete_over_cells() {
    let get_rq = read_query("sioc.rq");
    let put_rq = read_query("sioc.put.rq");
    // The exact three recoverable source atoms (per the shipped CompleteOver up-lift).
    let seed = SeedGraph::from_iri_atoms(
        "sioc-complete-over".to_owned(),
        vec![
            (
                format!("{EX}t1"),
                RDF_TYPE.to_owned(),
                format!("{GMEOW}Thread"),
            ),
            (
                format!("{EX}m1"),
                format!("{GMEOW}partOfThread"),
                format!("{EX}th1"),
            ),
            (
                format!("{EX}r1"),
                format!("{GMEOW}inReplyTo"),
                format!("{EX}p1"),
            ),
        ],
    );
    let outcome = discharge_section_law(&get_rq, &put_rq, std::slice::from_ref(&seed));
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "the three SIOC CompleteOver cells must discharge the section law\n{outcome:#?}"
    );
    assert!(outcome.countermodel.is_none());
}

// ── Inter-leg carrier round-trips non-IRI terms (literal / blank node). ──
//
// These are GMEOW law-adapter regressions: a non-IRI intermediate must reach the
// inverse leg with its exact identity, and generated blanks compare by isomorphism.
// The carrier stays native between legs; serialization is not part of execution.

// get mints a constant LITERAL object into the forward image; put matches that literal and
// reconstructs the exact source atom. The literal lives only in the carrier.
const LITERAL_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/label> \"foo\" }
WHERE { ?s <http://src.example/p> ?o }";
const LITERAL_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> <http://o.example/y> }
WHERE { ?s <http://ext.example/label> \"foo\" }";

#[test]
fn literal_object_in_the_carrier_round_trips_and_discharges() {
    let seed = SeedGraph::from_iri_atoms(
        "literal-carrier".to_owned(),
        vec![(
            "http://s.example/x".to_owned(),
            "http://src.example/p".to_owned(),
            "http://o.example/y".to_owned(),
        )],
    );
    let outcome = discharge_section_law(LITERAL_GET, LITERAL_PUT, std::slice::from_ref(&seed));
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "a literal in the inter-leg carrier must round-trip, not produce a false \
             ObligationViolated\n{outcome:#?}"
    );
    assert!(outcome.countermodel.is_none());
}

// A datatyped literal must survive get → put → comparison too.
const TYPED_LITERAL_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/n> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> }
WHERE { ?s <http://src.example/p> ?o }";
const TYPED_LITERAL_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> <http://o.example/y> }
WHERE { ?s <http://ext.example/n> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> }";

#[test]
fn datatyped_literal_in_the_carrier_round_trips_and_discharges() {
    let seed = SeedGraph::from_iri_atoms(
        "typed-literal-carrier".to_owned(),
        vec![(
            "http://s.example/x".to_owned(),
            "http://src.example/p".to_owned(),
            "http://o.example/y".to_owned(),
        )],
    );
    let outcome = discharge_section_law(
        TYPED_LITERAL_GET,
        TYPED_LITERAL_PUT,
        std::slice::from_ref(&seed),
    );
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "a datatyped literal in the carrier must round-trip with its datatype intact\n{outcome:#?}"
    );
}

// get mints a fresh BLANK NODE that joins two forward-image triples; put joins on it and
// reconstructs the source. The blank lives only in the carrier.
const BLANK_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/r> _:b . _:b <http://ext.example/v> ?o }
WHERE { ?s <http://src.example/p> ?o }";
const BLANK_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> ?o }
WHERE { ?s <http://ext.example/r> ?b . ?b <http://ext.example/v> ?o }";

#[test]
fn blank_node_in_the_carrier_round_trips_and_discharges() {
    let seed = SeedGraph::from_iri_atoms(
        "blank-carrier".to_owned(),
        vec![(
            "http://s.example/x".to_owned(),
            "http://src.example/p".to_owned(),
            "http://o.example/y".to_owned(),
        )],
    );
    let outcome = discharge_section_law(BLANK_GET, BLANK_PUT, std::slice::from_ref(&seed));
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "a fresh blank node in the inter-leg carrier must round-trip, not produce a false \
             ObligationViolated\n{outcome:#?}"
    );
    assert!(outcome.countermodel.is_none());

    let view = SeedGraph {
        label: "independent-blank-view".to_owned(),
        quads: vec![
            purrdf::RdfQuad::new(
                purrdf::RdfTerm::iri("http://s.example/x"),
                "http://ext.example/r",
                purrdf::RdfTerm::blank_node("view-join"),
            ),
            purrdf::RdfQuad::new(
                purrdf::RdfTerm::blank_node("view-join"),
                "http://ext.example/v",
                purrdf::RdfTerm::iri("http://o.example/y"),
            ),
        ],
    };
    let put_get = discharge_put_get_law(BLANK_GET, BLANK_PUT, &[view]);
    assert_eq!(
        put_get.verdict,
        DischargeVerdict::ObligationDischarged,
        "fresh blank-node labels must compare by RDF graph isomorphism in get∘put\n{put_get:#?}"
    );
}

// The quoted-triple comparison key must be injective: two DISTINCT RDF-star quoted triples
// must NOT render to the same `Atom` string (the pre-fix `<<triple>>` placeholder collapsed
// them, so a fabricated/dropped quoted-triple atom could hide in the set comparison).
#[test]
fn distinct_quoted_triples_do_not_collapse_to_equal_atoms() {
    use purrdf::{RdfTerm, RdfTriple};
    let qt1 = RdfTerm::triple(RdfTriple::new(
        RdfTerm::iri("http://s.example/a"),
        "http://p.example/rel",
        RdfTerm::iri("http://o.example/b"),
    ));
    let qt2 = RdfTerm::triple(RdfTriple::new(
        RdfTerm::iri("http://s.example/a"),
        "http://p.example/rel",
        RdfTerm::iri("http://o.example/c"),
    ));
    assert_ne!(
        term_str(&qt1),
        term_str(&qt2),
        "distinct quoted triples must render to distinct atom keys, not a collapsing placeholder"
    );
}

// A get leg with two independent branches; a put leg that recovers both AND fabricates a
// type-guard atom whenever branch-2 data is present. A single happy-path seed touching only
// branch-1 MISSES the fabrication; the branch-covering corpus CATCHES it. (AC2 integrity.)
const FAB_GET: &str = "\
PREFIX src: <http://src.example/>
PREFIX ext: <http://ext.example/>
CONSTRUCT {
  ?a ext:p1 ?b .
  ?c ext:p2 ?d .
} WHERE {
  { ?a src:rel1 ?b . }
  UNION
  { ?c src:rel2 ?d . }
}";

const FAB_PUT: &str = "\
PREFIX src: <http://src.example/>
PREFIX ext: <http://ext.example/>
CONSTRUCT {
  ?a src:rel1 ?b .
  ?c src:rel2 ?d .
  ?c a src:GuardType .
} WHERE {
  { ?a ext:p1 ?b . }
  UNION
  { ?c ext:p2 ?d . }
}";

fn happy_path_branch1_seed() -> SeedGraph {
    SeedGraph::from_iri_atoms(
        "happy".to_owned(),
        vec![(
            "http://seed.example/a".to_owned(),
            "http://src.example/rel1".to_owned(),
            "http://seed.example/b".to_owned(),
        )],
    )
}

#[test]
fn single_happy_path_seed_misses_the_fabricated_guard_atom() {
    // Branch-1 only: the fabricating put branch (keyed on ext:p2) never fires, so the seed
    // round-trips cleanly — a lone happy-path seed would wrongly report the law discharged.
    let seed = happy_path_branch1_seed();
    let outcome = discharge_section_law(FAB_GET, FAB_PUT, std::slice::from_ref(&seed));
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "a single branch-1 seed MUST miss the branch-2 fabrication (that is the blind spot)\n{outcome:#?}"
    );
}

#[test]
fn branch_covering_corpus_catches_the_fabricated_guard_atom() {
    let seeds = derive_seeds(FAB_GET).unwrap();
    // The corpus must exercise branch-2 (and the combined seed): at least three seeds.
    assert!(
        seeds.len() >= 3,
        "expected one seed per branch plus combined, got {}: {seeds:#?}",
        seeds.len()
    );
    let outcome = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationViolated,
        "the branch-covering corpus MUST catch the branch-2 fabrication\n{outcome:#?}"
    );
    let cm = outcome.countermodel.expect("a countermodel is present");
    // The spurious atom is the fabricated `?c a src:GuardType`.
    assert!(
        cm.spurious
            .iter()
            .any(|(_, p, o, _)| p == RDF_TYPE && o == "http://src.example/GuardType"),
        "the countermodel must name the fabricated GuardType atom\n{cm:#?}"
    );
    assert!(
        cm.missing.is_empty(),
        "nothing was dropped, only fabricated\n{cm:#?}"
    );
}

#[test]
fn discharge_is_deterministic_in_verdict_and_countermodel_bytes() {
    let seeds = derive_seeds(FAB_GET).unwrap();
    let a = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
    let b = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
    assert_eq!(
        a, b,
        "same inputs must yield an identical outcome (verdict + countermodel)"
    );
    // Countermodel bytes are stable across independent seed-derivation runs too.
    let seeds2 = derive_seeds(FAB_GET).unwrap();
    assert_eq!(seeds, seeds2, "seed derivation must be deterministic");
    let c = discharge_section_law(FAB_GET, FAB_PUT, &seeds2);
    assert_eq!(a, c);
}

#[test]
fn derive_seeds_is_branch_covering_with_fresh_distinct_iris() {
    let seeds = derive_seeds(FAB_GET).unwrap();
    // branch-0, branch-1, combined.
    let labels: Vec<&str> = seeds.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["branch-0", "branch-1", "combined"],
        "{seeds:#?}"
    );
    // Each per-branch seed carries exactly its one positive pattern.
    assert_eq!(seeds[0].quads.len(), 1);
    assert_eq!(seeds[1].quads.len(), 1);
    // The combined seed unions both branches (two distinct atoms, distinct IRIs).
    assert_eq!(seeds[2].quads.len(), 2, "{:#?}", seeds[2]);
    let all_iris: BTreeSet<String> = seeds
        .iter()
        .flat_map(|s| {
            s.quads
                .iter()
                .flat_map(|quad| [term_str(&quad.subject), term_str(&quad.object)])
        })
        .collect();
    // v0..v3 across the two branches — all fresh and distinct, deterministic.
    assert!(all_iris.contains("http://seed.example/v0"));
    assert!(all_iris.contains("http://seed.example/v3"));
}

#[test]
fn non_executable_leg_is_violated_never_a_silent_pass() {
    let seed = happy_path_branch1_seed();
    let broken = "this is not valid SPARQL {{{";
    let outcome = discharge_section_law(broken, FAB_PUT, std::slice::from_ref(&seed));
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationViolated,
        "a malformed get leg must hard-fail to Violated, never pass\n{outcome:#?}"
    );
    assert!(outcome.countermodel.is_some());
}

#[test]
fn empty_corpus_is_unknown_not_discharged() {
    let outcome = discharge_section_law(FAB_GET, FAB_PUT, &[]);
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationUnknown,
        "an unchecked law is Unknown — never proved absent"
    );
}

#[test]
fn discharge_laws_gates_claims_by_rung() {
    // BridgeView (floor) claims no injective law.
    let none = discharge_laws(FAB_GET, FAB_PUT, MorphismClass::BridgeView).unwrap();
    assert!(
        none.is_empty(),
        "a non-injective rung claims no section/put-get law\n{none:#?}"
    );

    // SectionRetraction claims BOTH SectionLaw and PutGet; the fabrication makes them Violated.
    let claims = discharge_laws(FAB_GET, FAB_PUT, MorphismClass::SectionRetraction).unwrap();
    let laws: BTreeSet<CorrespondenceLaw> = claims.iter().map(|c| c.law).collect();
    assert!(laws.contains(&CorrespondenceLaw::SectionLaw), "{claims:#?}");
    assert!(laws.contains(&CorrespondenceLaw::PutGet), "{claims:#?}");
    let section = claims
        .iter()
        .find(|c| c.law == CorrespondenceLaw::SectionLaw)
        .expect("section claim present");
    assert_eq!(
        section.verdict,
        DischargeVerdict::ObligationViolated,
        "{section:#?}"
    );
}

#[test]
fn discharge_laws_on_real_sioc_produces_claims() {
    // The shipped SIOC get leg has lossy branches (mapSiocTopic), so the auto-derived
    // branch corpus does NOT globally discharge the section law — but the service must run
    // end-to-end on the real queries and return a claim per permitted law.
    let get_rq = read_query("sioc.rq");
    let put_rq = read_query("sioc.put.rq");
    let claims = discharge_laws(&get_rq, &put_rq, MorphismClass::SectionRetraction).unwrap();
    assert_eq!(claims.len(), 2, "SectionLaw + PutGet\n{claims:#?}");
    for c in &claims {
        assert_ne!(
            c.verdict,
            DischargeVerdict::ObligationUnknown,
            "a non-empty SIOC corpus must yield a decided verdict\n{c:#?}"
        );
    }
}
