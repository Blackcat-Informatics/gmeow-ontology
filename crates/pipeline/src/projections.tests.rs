// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::RdfLiteral;

#[test]
fn projection_retags_nested_claim_literals_without_losing_direction_or_scope() {
    let mut literal = RdfLiteral::language_tagged("مرحبا", "x-gmeow-arabic");
    literal.direction = Some(purrdf::RdfTextDirection::Rtl);
    let inner = RdfTerm::triple(purrdf::RdfTriple::new(
        RdfTerm::blank_node("speaker"),
        format!("{GM}fullName"),
        RdfTerm::literal(literal),
    ));
    let outer = RdfTerm::triple(purrdf::RdfTriple::new(
        RdfTerm::iri("https://example.org/claim"),
        format!("{GM}content"),
        inner,
    ));
    let mut quads = [
        RdfQuad::new(RdfTerm::blank_node("evidence"), RDF_REIFIES, outer)
            .in_graph(RdfTerm::iri("https://example.org/standpoint")),
    ];
    retag_quads(
        &mut quads,
        &TagMap::from([("x-gmeow-arabic".to_owned(), "ar".to_owned())]),
    );
    assert_eq!(
        quads[0].graph_name,
        Some(RdfTerm::iri("https://example.org/standpoint"))
    );
    let RdfTerm::Triple(outer) = &quads[0].object else {
        panic!("quoted claim");
    };
    let RdfTerm::Triple(inner) = &outer.object else {
        panic!("nested claim");
    };
    assert_eq!(inner.subject, RdfTerm::blank_node("speaker"));
    let RdfTerm::Literal(literal) = &inner.object else {
        panic!("name");
    };
    assert_eq!(literal.language.as_deref(), Some("ar"));
    assert_eq!(literal.direction, Some(purrdf::RdfTextDirection::Rtl));
}

const SCHEMA_PERSON: &str = "https://schema.org/Person";
const GM_PERSON: &str = "https://blackcatinformatics.ca/gmeow/Person";
const GM_FULL_NAME: &str = "https://blackcatinformatics.ca/gmeow/fullName";
const SCHEMA_NAME: &str = "https://schema.org/name";
const EX_ME: &str = "https://example.org/me";

fn nt(s: &str, p: &str, o: &str) -> String {
    format!("<{s}> <{p}> <{o}> .\n")
}

fn lit(s: &str, p: &str, literal: &str) -> String {
    format!("<{s}> <{p}> {literal} .\n")
}

#[test]
fn profiles_registry_matches_python_names() {
    let profiles = profiles();
    // A representative spread across the phases + the identity/base profiles.
    for name in [
        "schema-org",
        "foaf",
        "vcard",
        "oai_dc",
        "dcat",
        "codemeta",
        "prov",
        "mailmap",
    ] {
        assert!(profiles.contains_key(name), "missing profile {name}");
    }
    assert_eq!(profiles.len(), 27, "profile count drifted");
    assert_eq!(
        profiles["schema-org"].prefixes,
        vec!["schema".to_owned(), "rdfs".to_owned()]
    );
}

/// Pin the registry to the suppression leak sweep's coverage set. This
/// crate registers the projection profiles; `gmeow-validate` declares which
/// profiles the P10 leak sweep covers. They MUST be identical — otherwise a
/// newly registered projection profile silently escapes the leak sweep (or a
/// swept name has no CONSTRUCT). Full set-equality, so add / remove / swap all
/// trip the gate, restoring the dynamic coverage the Python parametrization had.
#[test]
fn registry_equals_the_suppression_leak_sweep_set() {
    let registered: std::collections::BTreeSet<String> = profiles().into_keys().collect();
    let swept: std::collections::BTreeSet<String> =
        gmeow_validate::projection_profiles::PROJECTION_PROFILES
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
    let unswept: Vec<&String> = registered.difference(&swept).collect();
    let unregistered: Vec<&String> = swept.difference(&registered).collect();
    assert!(
        unswept.is_empty() && unregistered.is_empty(),
        "projection registry drifted from the suppression leak sweep — \
             registered-but-unswept: {unswept:?}; swept-but-unregistered: {unregistered:?}"
    );
}

#[test]
fn project_graph_runs_a_construct_to_schema_org() {
    // A minimal gmeow A-box projected by a hand-written CONSTRUCT emits the
    // expected pure schema.org triples — proving the native SPARQL driver runs.
    let source = {
        let mut s = String::new();
        s.push_str(&nt(EX_ME, RDF_TYPE, GM_PERSON));
        s.push_str(&lit(EX_ME, GM_FULL_NAME, "\"Ada Lovelace\""));
        s
    };
    let query = format!(
        "CONSTRUCT {{ ?s a <{SCHEMA_PERSON}> . ?s <{SCHEMA_NAME}> ?n . }} \
             WHERE {{ ?s a <{GM_PERSON}> . ?s <{GM_FULL_NAME}> ?n . }}"
    );
    let out = project_graph(&source, &query, &TagMap::new()).unwrap();
    assert!(
        out.contains(&format!("<{EX_ME}> <{RDF_TYPE}> <{SCHEMA_PERSON}> .")),
        "missing schema:Person type: {out}"
    );
    assert!(
        out.contains(&format!("<{EX_ME}> <{SCHEMA_NAME}> \"Ada Lovelace\" .")),
        "missing schema:name: {out}"
    );
    // Directional & lossy: no gmeow: predicate leaks into the projection.
    assert!(
        !out.contains(GM_FULL_NAME),
        "internal gmeow predicate leaked: {out}"
    );
}

#[test]
fn project_graph_retags_public_language() {
    // The projection-boundary retag rewrites an internal x-gmeow-* literal tag to
    // its public BCP-47 form; a bare pass-through (empty map) leaves it internal.
    let source = lit(EX_ME, GM_FULL_NAME, "\"Ada\"@x-gmeow-english");
    let query =
        format!("CONSTRUCT {{ ?s <{SCHEMA_NAME}> ?n . }} WHERE {{ ?s <{GM_FULL_NAME}> ?n . }}");

    let untagged = project_graph(&source, &query, &TagMap::new()).unwrap();
    assert!(untagged.contains("@x-gmeow-english"), "{untagged}");

    let mut map = TagMap::new();
    map.insert("x-gmeow-english".to_owned(), "en".to_owned());
    let retagged = project_graph(&source, &query, &map).unwrap();
    assert!(retagged.contains("\"Ada\"@en"), "not retagged: {retagged}");
    assert!(!retagged.contains("x-gmeow-english"), "leak: {retagged}");
}

#[test]
fn view_namespaces_resolves_selectors() {
    assert!(view_namespaces("all").unwrap().is_empty());
    assert!(view_namespaces("maximal").unwrap().is_empty());
    assert_eq!(
        view_namespaces("gmeow").unwrap(),
        BTreeSet::from([GM.to_owned()])
    );
    assert_eq!(
        view_namespaces("schema-org").unwrap(),
        BTreeSet::from([
            "https://schema.org/".to_owned(),
            "http://www.w3.org/2000/01/rdf-schema#".to_owned(),
        ])
    );
    assert!(view_namespaces("not-a-profile").is_err());
}

#[test]
fn keep_in_view_filters_by_predicate_and_type() {
    let namespaces = BTreeSet::from(["https://schema.org/".to_owned()]);
    // predicate in namespace → keep
    let name_edge = RdfQuad::new(
        RdfTerm::iri(EX_ME),
        SCHEMA_NAME.to_owned(),
        RdfTerm::literal(RdfLiteral::simple("Ada".to_owned())),
    );
    assert!(keep_in_view(&name_edge, &namespaces));
    // rdf:type to a class in namespace → keep
    let type_edge = RdfQuad::new(
        RdfTerm::iri(EX_ME),
        RDF_TYPE.to_owned(),
        RdfTerm::iri(SCHEMA_PERSON),
    );
    assert!(keep_in_view(&type_edge, &namespaces));
    // gmeow predicate → drop
    let gmeow_edge = RdfQuad::new(
        RdfTerm::iri(EX_ME),
        GM_FULL_NAME.to_owned(),
        RdfTerm::literal(RdfLiteral::simple("Ada".to_owned())),
    );
    assert!(!keep_in_view(&gmeow_edge, &namespaces));
    // rdf:type to a gmeow class → drop
    let gmeow_type = RdfQuad::new(
        RdfTerm::iri(EX_ME),
        RDF_TYPE.to_owned(),
        RdfTerm::iri(GM_PERSON),
    );
    assert!(!keep_in_view(&gmeow_type, &namespaces));
}

#[test]
fn gts_subset_filters_a_maximal_gts_by_view() {
    // Build a maximal-style .gts (gmeow base + a schema.org projection + a
    // provenance reifier), then prove each view keeps exactly its slice.
    let mut maximal = String::new();
    maximal.push_str(&nt(EX_ME, RDF_TYPE, GM_PERSON));
    maximal.push_str(&nt(EX_ME, RDF_TYPE, SCHEMA_PERSON));
    maximal.push_str(&lit(EX_ME, SCHEMA_NAME, "\"Ada\""));
    // an RDF-1.2 reifier row over the derived schema:Person triple
    maximal.push_str(&format!(
        "<{GM}derivations/abcd> <{RDF_REIFIES}> \
             <<( <{EX_ME}> <{RDF_TYPE}> <{SCHEMA_PERSON}> )>> .\n"
    ));

    let gts = build_gts(&maximal);

    // gmeow view: only the pure gmeow base; the schema.org projection + reifier drop.
    let gmeow_view = project_gts_subset(&gts, "gmeow", &TagMap::new()).unwrap();
    assert!(gmeow_view.contains(&format!("<{EX_ME}> <{RDF_TYPE}> <{GM_PERSON}> .")));
    assert!(
        !gmeow_view.contains(SCHEMA_PERSON),
        "schema leaked: {gmeow_view}"
    );
    assert!(
        !gmeow_view.contains(RDF_REIFIES),
        "reifier leaked: {gmeow_view}"
    );

    // schema-org view: the schema triples, not the gmeow-only base type.
    let schema_view = project_gts_subset(&gts, "schema-org", &TagMap::new()).unwrap();
    assert!(schema_view.contains(&format!("<{EX_ME}> <{RDF_TYPE}> <{SCHEMA_PERSON}> .")));
    assert!(schema_view.contains(&format!("<{EX_ME}> <{SCHEMA_NAME}> \"Ada\" .")));
    assert!(
        !schema_view.contains(&format!("<{EX_ME}> <{RDF_TYPE}> <{GM_PERSON}> .")),
        "gmeow-only base type leaked: {schema_view}"
    );

    // all view: everything in the base, but never the reifier rows.
    let all_view = project_gts_subset(&gts, "all", &TagMap::new()).unwrap();
    assert!(all_view.contains(GM_PERSON));
    assert!(all_view.contains(SCHEMA_PERSON));
    assert!(
        !all_view.contains(RDF_REIFIES),
        "reifier leaked into all: {all_view}"
    );
}

const FOAF_KNOWS: &str = "http://xmlns.com/foaf/0.1/knows";
const GM_KNOWS: &str = "https://blackcatinformatics.ca/gmeow/knows";

/// A hermetic SSSOM lift map: `foaf:knows` cleanly renames to `gmeow:knows`.
fn knows_sssom() -> String {
    concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  foaf: http://xmlns.com/foaf/0.1/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\n",
        "gmeow:knows\tskos:exactMatch\tfoaf:knows\n",
    )
    .to_owned()
}

#[test]
fn up_project_lifts_consumer_vocab_to_gmeow() {
    // A foaf:knows source triple lifts up to gmeow:knows through the lawful put
    // executor — the up-projection recall smoke.
    let source_nt = format!("<{EX_ME}> <{FOAF_KNOWS}> <https://example.org/you> .\n");
    let inputs = UpProjectionInputs {
        sssom_texts: vec![knows_sssom()],
        projection_ttls: Vec::new(),
        ontology_nt: String::new(),
        discharged_section_cells: BTreeSet::new(),
    };
    let up = up_project(&source_nt, &inputs, &TagMap::new()).unwrap();
    assert!(up.lifted >= 1, "nothing lifted: {up:?}");
    assert!(
        up.graph_nt.contains(GM_KNOWS),
        "renamed predicate absent: {}",
        up.graph_nt
    );
    assert!(
        !up.graph_nt.contains(FOAF_KNOWS),
        "consumer predicate leaked into GMEOW draft: {}",
        up.graph_nt
    );
}

#[test]
fn transpile_graph_chains_up_then_maximal() {
    // The full transpile: a foaf: source is lifted to pure GMEOW, then run through
    // MAXIMAL(G). A strong equivalentProperty cell mirrors gmeow:knows back out to
    // foaf:knows in the projected layer — the round-trip closes.
    let source_nt = format!("<{EX_ME}> <{FOAF_KNOWS}> <https://example.org/you> .\n");
    let up_inputs = UpProjectionInputs {
        sssom_texts: vec![knows_sssom()],
        projection_ttls: Vec::new(),
        ontology_nt: String::new(),
        discharged_section_cells: BTreeSet::new(),
    };
    let maximal_inputs = MaximalInputs {
        ontology_nt: format!(
            "<{GM_KNOWS}> <{RDF_TYPE}> <http://www.w3.org/2002/07/owl#ObjectProperty> .\n"
        ),
        cells: vec![CellInput {
            iri: "https://blackcatinformatics.ca/gmeow/te/knows-foaf".to_owned(),
            subject: GM_KNOWS.to_owned(),
            predicate_curie: "owl:equivalentProperty".to_owned(),
            object: FOAF_KNOWS.to_owned(),
            confidence: Some(
                gmeow_logic_compile::ir::UnitInterval::new(purrdf::RdfLiteral::typed(
                    "0.9",
                    "http://www.w3.org/2001/XMLSchema#decimal",
                ))
                .unwrap(),
            ),
        }],
        denied: Vec::new(),
        projection_queries: Vec::new(),
    };

    let report = transpile_graph(
        &source_nt,
        "smoke",
        &up_inputs,
        &maximal_inputs,
        &TagMap::new(),
    )
    .unwrap();

    assert!(report.lifted >= 1, "nothing lifted: {report:?}");
    assert!(
        report.draft_nt.contains(GM_KNOWS),
        "draft missing gmeow:knows"
    );
    // E(G) mirrored the gmeow:knows edge back out to foaf:knows.
    assert!(report.transform.saturated >= 1, "no saturation: {report:?}");
    assert!(
        report.transform.base_plus_derived_nt.contains(FOAF_KNOWS),
        "equivalentProperty mirror missing: {}",
        report.transform.base_plus_derived_nt
    );
    assert!(!report.transform.gts_bytes.is_empty(), "empty gts");

    // An empty stem is a hard fail, not a silent default.
    assert!(transpile_graph("", " ", &up_inputs, &maximal_inputs, &TagMap::new()).is_err());
}

#[test]
fn transpile_graph_fans_out_without_x_gmeow_leak() {
    // The on-gate, fixture-scale twin of the off-gate CLI process test
    // (`gmeow-cli/tests/self_sufficiency.rs::transpile_blinded_lifts_and_fans_out_without_x_gmeow_leak_heavy_offgate`),
    // driven through the REAL production chain (`up_project` → `transform_nt`, via
    // `transpile_graph`) — no CLI process, no bundle load. Proves the two halves
    // together, through the FULL chain (not `transform_nt` alone):
    //
    //  * fan-out: an equivalentProperty cell makes MAXIMAL(G) mirror `gmeow:knows`
    //    back out to `foaf:knows` — more triples come out than were asserted.
    //  * zero-leak: a GMEOW-native literal carrying an internal `x-gmeow-*` tag
    //    (passed through `up_project`'s gmeow-namespace passthrough leg unchanged,
    //    per `put_executor::fact_queries`) survives to the MAXIMAL(G) boundary
    //    retagged to its public BCP-47 form — never as the internal tag.
    let source_nt = format!(
        "{}{}",
        nt(EX_ME, FOAF_KNOWS, "https://example.org/you"),
        lit(EX_ME, GM_FULL_NAME, "\"Ada Lovelace\"@x-gmeow-english"),
    );
    let up_inputs = UpProjectionInputs {
        sssom_texts: vec![knows_sssom()],
        projection_ttls: Vec::new(),
        ontology_nt: String::new(),
        discharged_section_cells: BTreeSet::new(),
    };
    let maximal_inputs = MaximalInputs {
        ontology_nt: format!(
            "<{GM_KNOWS}> <{RDF_TYPE}> <http://www.w3.org/2002/07/owl#ObjectProperty> .\n"
        ),
        cells: vec![CellInput {
            iri: "https://blackcatinformatics.ca/gmeow/te/knows-foaf".to_owned(),
            subject: GM_KNOWS.to_owned(),
            predicate_curie: "owl:equivalentProperty".to_owned(),
            object: FOAF_KNOWS.to_owned(),
            confidence: Some(
                gmeow_logic_compile::ir::UnitInterval::new(purrdf::RdfLiteral::typed(
                    "0.9",
                    "http://www.w3.org/2001/XMLSchema#decimal",
                ))
                .unwrap(),
            ),
        }],
        denied: Vec::new(),
        projection_queries: Vec::new(),
    };

    // Non-vacuity / negative control: with NO tag_map, the internal tag survives
    // the full chain untouched — proving the fixture genuinely would have leaked
    // before the retag boundary existed (not a vacuously-passing assertion below).
    let unmapped = transpile_graph(
        &source_nt,
        "leak-control",
        &up_inputs,
        &maximal_inputs,
        &TagMap::new(),
    )
    .unwrap();
    assert!(
        unmapped
            .transform
            .base_plus_derived_nt
            .contains("x-gmeow-english"),
        "fixture is vacuous: with an empty tag_map the internal tag should still \
             be present: {}",
        unmapped.transform.base_plus_derived_nt
    );

    let mut tag_map = TagMap::new();
    tag_map.insert("x-gmeow-english".to_owned(), "en".to_owned());

    let report = transpile_graph(
        &source_nt,
        "fanout-no-leak",
        &up_inputs,
        &maximal_inputs,
        &tag_map,
    )
    .unwrap();

    // Fan-out: MAXIMAL(G) genuinely produced MORE than was asserted — the
    // equivalentProperty cell mirrors gmeow:knows back out to foaf:knows.
    assert!(
        report.transform.saturated >= 1,
        "no saturation fan-out fired: {report:?}"
    );
    assert!(
        report.transform.base_plus_derived_nt.contains(&format!(
            "<{EX_ME}> <{FOAF_KNOWS}> <https://example.org/you> ."
        )),
        "equivalentProperty mirror missing: {}",
        report.transform.base_plus_derived_nt
    );

    // Zero-leak: no x-gmeow-* internal tag survives the full chain, on ANY literal
    // (parsed, not just a substring scan) — using the same `is_internal_tag`
    // predicate the P10 suppression leak sweep uses.
    let out_quads = flat_quads_from_nt(&report.transform.base_plus_derived_nt).unwrap();
    assert!(
        out_quads.iter().all(|q| match &q.object {
            RdfTerm::Literal(literal) => literal
                .language
                .as_deref()
                .is_none_or(|lang| !gmeow_validate::language_tags::is_internal_tag(lang)),
            _ => true,
        }),
        "an internal x-gmeow-* tag leaked into MAXIMAL(G): {}",
        report.transform.base_plus_derived_nt
    );
    assert!(
        !report.transform.base_plus_derived_nt.contains("x-gmeow"),
        "x-gmeow substring leaked: {}",
        report.transform.base_plus_derived_nt
    );

    // The properly-tagged public form DOES appear, routed through tag_map.
    assert!(
        report
            .transform
            .base_plus_derived_nt
            .contains("\"Ada Lovelace\"@en"),
        "public BCP-47 retag missing: {}",
        report.transform.base_plus_derived_nt
    );
}

#[test]
fn transpile_graph_rejects_empty_lift() {
    // A source with no lawful lift rule produces an empty draft — surfaced, never
    // a silent empty publication.
    let source_nt = "<https://example.org/x> <https://unknown.example/p> \"v\" .\n";
    let err = transpile_graph(
        source_nt,
        "empty",
        &UpProjectionInputs::default(),
        &MaximalInputs::default(),
        &TagMap::new(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("nothing lifted"), "{err}");
}

// ── SIOC reasoned-superclass recovery (Deliverable B) ────────────────
//
// The real SIOC ↔ email-thread correspondence: a subject bearing `sioc:has_container`
// / `sioc:reply_of` lifts to `gmeow:partOfThread` / `gmeow:inReplyTo` (both
// `rdfs:domain gmeow:Message`), so prp-dom entails it IS a `gmeow:Message`. The IRIs
// and axioms below are the real ones from `slices/extensions/email/module.ttl`
// (partOfThread/inReplyTo domain=Message, subPropertyOf=partOf, range=Thread;
// EmailMessage⊑Message; Message⊑InformationObject) and
// `slices/core/documents/module.ttl` (FeedPosting⊑Work).
const SIOC_HAS_CONTAINER: &str = "http://rdfs.org/sioc/ns#has_container";
const SIOC_REPLY_OF: &str = "http://rdfs.org/sioc/ns#reply_of";
const GM_PART_OF_THREAD: &str = "https://blackcatinformatics.ca/gmeow/partOfThread";
const GM_IN_REPLY_TO: &str = "https://blackcatinformatics.ca/gmeow/inReplyTo";
const GM_PART_OF: &str = "https://blackcatinformatics.ca/gmeow/partOf";
const GM_MESSAGE: &str = "https://blackcatinformatics.ca/gmeow/Message";
const GM_EMAIL_MESSAGE: &str = "https://blackcatinformatics.ca/gmeow/EmailMessage";
const GM_FEED_POSTING: &str = "https://blackcatinformatics.ca/gmeow/FeedPosting";
const GM_INFORMATION_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/InformationObject";
const GM_WORK: &str = "https://blackcatinformatics.ca/gmeow/Work";
const GM_THREAD: &str = "https://blackcatinformatics.ca/gmeow/Thread";

const SIOC_X: &str = "https://example.org/msg/1";
const SIOC_THREAD: &str = "https://example.org/thread/1";
const SIOC_PARENT: &str = "https://example.org/msg/0";

/// The real property-domain TBox fragment the reasoned harvest reasons over. Uses
/// the actual email/documents module axioms and IRIs. Crucially it INCLUDES the
/// `EmailMessage ⊑ Message` and `FeedPosting ⊑ Work` axioms, so AC5's negative
/// assertions are non-vacuous: EmailMessage/FeedPosting are absent only because
/// prp-dom + subClassOf propagate UPWARD, never because the axiom is missing.
fn email_thread_tbox() -> String {
    let mut t = String::new();
    t.push_str(&nt(GM_PART_OF_THREAD, RDFS_DOMAIN, GM_MESSAGE));
    t.push_str(&nt(GM_PART_OF_THREAD, RDFS_RANGE, GM_THREAD));
    t.push_str(&nt(GM_PART_OF_THREAD, RDFS_SUBPROPERTYOF, GM_PART_OF));
    t.push_str(&nt(GM_IN_REPLY_TO, RDFS_DOMAIN, GM_MESSAGE));
    t.push_str(&nt(GM_IN_REPLY_TO, RDFS_RANGE, GM_MESSAGE));
    t.push_str(&nt(GM_MESSAGE, RDFS_SUBCLASSOF, GM_INFORMATION_OBJECT));
    t.push_str(&nt(GM_EMAIL_MESSAGE, RDFS_SUBCLASSOF, GM_MESSAGE));
    t.push_str(&nt(GM_FEED_POSTING, RDFS_SUBCLASSOF, GM_WORK));
    t
}

/// The authenticated shipped bundle — the exact snapshot the real `gmeow-dev
/// up-project` folds. A missing or mismatched producer identity fails closed.
fn committed_gts() -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    gmeow_bundle_import::load_authenticated_source_bytes(&root)
        .expect("load authenticated producer bundle without rebuilding it")
}

/// The lifted SIOC image plus the bundle ontology it reasoned against — cached across AC4/AC5.
struct SiocRun {
    up: UpProjection,
    ontology_nt: String,
}

/// Drive the REAL production inverse-ingest entry `up_project` on the SIOC image with inputs
/// assembled from the committed bundle — the exact `(SSSOM, projection cells, ontology,
/// discharged verdicts)` the shipped `gmeow-dev up-project` consumes. There is NO synthetic
/// exactMatch SSSOM: the SIOC thread predicates ship as closeMatch + EDOAL `=` cells, and the
/// executed discharged `logic:SectionLaw` (Deliverable A) is the sole authorization for the
/// lawful FACT lift. This test therefore FAILS if the A→B promotion regresses (the SIOC facts
/// vanish) and PASSES because the promotion lifts them. Cached (the bundle fold + gate machinery
/// runs once for both acceptance criteria).
fn sioc_run() -> &'static SiocRun {
    static CACHE: std::sync::OnceLock<SiocRun> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let gts = committed_gts();
        let sssom_texts: Vec<String> = crate::bundle_blobs::Bundle::from_snapshot(&gts)
            .expect("fold bundle")
            .archive(crate::bundle_blobs::REP_MAPPINGS)
            .expect("mappings archive")
            .into_values()
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .collect();
        let projection_ttls: Vec<String> = crate::bundle_blobs::Bundle::from_snapshot(&gts)
            .expect("fold bundle")
            .archive(crate::bundle_blobs::REP_CELLS)
            .expect("cells archive")
            .into_iter()
            .filter(|(k, _)| k.ends_with(".ttl"))
            .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
            .collect();
        let base = gts_base_graph(&gts).expect("base graph");
        let ontology_nt = quads_to_nt(&base).expect("ontology nt");
        let discharged_section_cells =
            discharged_section_cells_from_bundle(&gts).expect("discharged cells");
        let source_nt = format!(
            "{}{}",
            nt(SIOC_X, SIOC_HAS_CONTAINER, SIOC_THREAD),
            nt(SIOC_X, SIOC_REPLY_OF, SIOC_PARENT),
        );
        let inputs = UpProjectionInputs {
            sssom_texts,
            projection_ttls,
            ontology_nt: ontology_nt.clone(),
            discharged_section_cells,
        };
        let up = up_project(&source_nt, &inputs, &TagMap::new())
            .expect("up_project over the real bundle inputs");
        SiocRun { up, ontology_nt }
    })
}

/// The `<s> a <class> .` N-Triples line for the SIOC subject.
fn x_type_line(class: &str) -> String {
    format!("<{SIOC_X}> <{RDF_TYPE}> <{class}> .")
}

#[test]
fn up_project_recovers_message_superclass_via_prp_dom() {
    // AC4 (positive): the real inverse-ingest surface over the SHIPPED bundle. `sioc:has_container`
    // / `sioc:reply_of` lift to `gmeow:partOfThread` / `gmeow:inReplyTo` — as lawful FACTS,
    // authorized by their executed discharged `logic:SectionLaw` (Deliverable A), NOT by any
    // synthetic exactMatch SSSOM. Both lifted predicates have `rdfs:domain gmeow:Message`, so
    // the reasoned harvest recovers the entailed `<X> a gmeow:Message`.
    let up = &sioc_run().up;
    assert!(
        up.graph_nt.contains(GM_PART_OF_THREAD),
        "sioc:has_container did not lift to gmeow:partOfThread: {}",
        up.graph_nt
    );
    assert!(
        up.graph_nt.contains(GM_IN_REPLY_TO),
        "sioc:reply_of did not lift to gmeow:inReplyTo: {}",
        up.graph_nt
    );
    assert!(
        up.lifted >= 2,
        "the two SIOC thread predicates must lift as FACTS (not lossy claims): {up:?}"
    );
    assert!(
        up.graph_nt.contains(&x_type_line(GM_MESSAGE)),
        "entailed gmeow:Message superclass NOT recovered: {}",
        up.graph_nt
    );
}

#[test]
fn up_project_never_fabricates_subkind_or_sibling() {
    // AC5 (negative control, SAME output as AC4): prp-dom + subClassOf are
    // upward-only, so the recovered type is exactly `gmeow:Message` (and its
    // superclasses) — never the `gmeow:EmailMessage` SubKind below it, nor the
    // unrelated `gmeow:FeedPosting` sibling.
    let run = sioc_run();
    let up = &run.up;
    // Sanity: the positive recovery still holds in this same output (guards against
    // the negatives passing only because nothing was reasoned at all).
    assert!(
        up.graph_nt.contains(&x_type_line(GM_MESSAGE)),
        "sanity: gmeow:Message must be recovered here too: {}",
        up.graph_nt
    );
    assert!(
        !up.graph_nt.contains(&x_type_line(GM_EMAIL_MESSAGE)),
        "fabricated a SubKind (gmeow:EmailMessage) — downward invention: {}",
        up.graph_nt
    );
    assert!(
        !up.graph_nt.contains(&x_type_line(GM_FEED_POSTING)),
        "fabricated a sibling (gmeow:FeedPosting): {}",
        up.graph_nt
    );
    // Non-vacuity: EmailMessage ⊑ Message and FeedPosting ⊑ Work ARE in the SHIPPED bundle
    // ontology, so the SubKind/sibling absence above is sound upward-only reasoning, not a
    // missing axiom.
    assert!(
        run.ontology_nt.contains(GM_EMAIL_MESSAGE) && run.ontology_nt.contains(GM_FEED_POSTING),
        "negative control would be vacuous: bundle ontology lacks the SubKind/sibling axioms"
    );
}

#[test]
fn harvest_reasoned_types_needs_world_colocation() {
    // Mis-scope regression — driven at the `harvest_reasoned_types` level (not
    // through `up_project`). WHY this level: the public `up_project` input cannot
    // mis-scope — `harvest_reasoned_types` always lands the lifted assertions and the
    // extracted TBox fragment in the SAME (default) world via
    // `flat_dataset_from_quads`, so co-location is structurally guaranteed for every
    // public caller. To exercise the silent-failure mode we feed the harvest a
    // deliberately mis-scoped dataset: the `gmeow:partOfThread` assertion pinned to a
    // NAMED graph (a distinct reasoning world) while the `rdfs:domain` axiom stays in
    // the default world. The native reasoner is world-indexed with no cross-world
    // union, so prp-dom cannot fire and `gmeow:Message` is NOT derived. If a future
    // refactor breaks world co-location, prp-dom silently no-ops and THIS test fails.
    let tbox = email_thread_tbox();

    // Co-located control: default-graph lifted assertion DOES recover Message — proves
    // the harvest genuinely fires when the worlds coincide (so the negative below is
    // about co-location, not a dead harvest).
    let colocated = vec![RdfQuad::new(
        RdfTerm::iri(SIOC_X),
        GM_PART_OF_THREAD.to_owned(),
        RdfTerm::iri(SIOC_THREAD),
    )];
    let recovered = harvest_reasoned_types(&colocated, &tbox).unwrap();
    assert!(
        recovered
            .iter()
            .any(|q| q.predicate == RDF_TYPE
                && matches!(&q.object, RdfTerm::Iri(c) if c == GM_MESSAGE)),
        "co-located harvest must recover gmeow:Message: {recovered:?}"
    );

    // Mis-scoped: the SAME assertion in a named graph (a different world) than the
    // domain axiom ⇒ prp-dom no-ops, nothing derived.
    let misscoped = vec![
        RdfQuad::new(
            RdfTerm::iri(SIOC_X),
            GM_PART_OF_THREAD.to_owned(),
            RdfTerm::iri(SIOC_THREAD),
        )
        .in_graph(RdfTerm::iri("https://example.org/other-world")),
    ];
    let harvested = harvest_reasoned_types(&misscoped, &tbox).unwrap();
    assert!(
        !harvested
            .iter()
            .any(|q| matches!(&q.object, RdfTerm::Iri(c) if c == GM_MESSAGE)),
        "mis-scoped assertion must NOT derive gmeow:Message — world co-location is \
             load-bearing: {harvested:?}"
    );
}

/// Compose a `.gts` from an N-Triples (base + RDF-1.2 statement layer) document,
/// via the same native `gts_compose` path the transform kernel uses.
fn build_gts(nt: &str) -> Vec<u8> {
    let dataset = purrdf::parse_dataset(nt.as_bytes(), NT_MEDIA_TYPE, None).unwrap();
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    builder.add_dataset(&dataset).unwrap();
    // gmeow-test-input: synthetic-only
    purrdf::gts_compose::emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(None),
    )
    .unwrap()
}
