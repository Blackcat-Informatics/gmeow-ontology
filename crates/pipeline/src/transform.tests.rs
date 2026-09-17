// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn native_transform_input_reuses_flat_data_and_keeps_only_selected_claims() {
    let plain = purrdf::parse_dataset(
        b"<urn:person> <urn:name> \"Ada\" .",
        "application/n-triples",
        None,
    )
    .unwrap();
    let reused = Graph::from_default_dataset(&plain).unwrap();
    assert!(Arc::ptr_eq(&reused.ds, &plain));
    assert_eq!(reused.len(), 1);

    let source = purrdf::parse_dataset(
        br#"@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
                @prefix g: <https://blackcatinformatics.ca/gmeow/> .
                <urn:claim> rdf:reifies <<( <urn:person> <urn:name> "Ada"@ar--rtl )>> ;
                    g:accordingTo <urn:observer> .
                <urn:outside> {
                    <urn:claim> rdf:reifies <<( <urn:person> <urn:name> "Other"@ar--rtl )>> ;
                        g:accordingTo <urn:other-observer> .
                }"#,
        "application/trig",
        None,
    )
    .unwrap();
    let selected = Graph::from_default_dataset(&source).unwrap();
    assert_eq!(selected.len(), 2);
    assert!(selected.ds.named_graphs().next().is_none());
    let rows = purrdf::flat_rdf_quads_from_dataset(&selected.ds);
    let claim = rows
        .iter()
        .find(|quad| quad.predicate == RDF_REIFIES)
        .unwrap();
    let RdfTerm::Triple(triple) = &claim.object else {
        panic!("the selected claim must retain its quoted proposition");
    };
    let RdfTerm::Literal(literal) = &triple.object else {
        panic!("the claim's name must remain a directional literal");
    };
    assert_eq!(literal.lexical_form, "Ada");
    assert_eq!(literal.direction, Some(purrdf::RdfTextDirection::Rtl));
    assert!(
        rows.iter()
            .any(|quad| quad.object == RdfTerm::iri("urn:observer"))
    );
    assert!(!rows.iter().any(|quad| quad.predicate == "urn:name"));
}

#[test]
fn prepared_transform_preserves_projected_provenance_across_independent_inputs() {
    let tags = TagMap::from([("x-gmeow-english".to_owned(), "en".to_owned())]);
    let projections = [(
        "name".to_owned(),
        format!("CONSTRUCT {{ ?s <{GM}writtenRep> ?name }} WHERE {{ ?s <{GM}fullName> ?name }}"),
    )];
    let program = TransformProgram::prepare("", &[], &[], &projections, &tags)
        .expect("prepare GMEOW projection");
    for name in ["Ada", "Grace"] {
        let source =
            format!("<https://example.org/{name}> <{GM}fullName> \"{name}\"@x-gmeow-english .");
        let dataset = purrdf::parse_dataset(source.as_bytes(), "application/n-triples", None)
            .expect("instance");
        let result = program
            .execute_default_graph(&dataset)
            .expect("typed transform");
        assert_eq!(result.asserted, 1);
        assert_eq!(result.projected, 1);
        let output = purrdf::flat_rdf_quads_from_dataset(&result.dataset);
        assert_eq!(output.len(), 2);
        assert!(
            output
                .iter()
                .all(|quad| quad.subject == RdfTerm::iri(format!("https://example.org/{name}")))
        );
        let name_id = result
            .dataset
            .term_id_by_value(&TermValue::lang_literal(name, "en"))
            .expect("the published name has the public language tag");
        assert!(result.dataset.quads().all(|quad| quad.o == name_id));
        let public_name = result.dataset.to_owned_term(name_id);
        let bundle =
            purrdf::gts::flattened_dataset_from_bytes(&result.gts_bytes).expect("complete GTS");
        let quads = purrdf::flat_rdf_quads_from_dataset(&bundle);
        assert!(quads.iter().any(|quad| quad.predicate == GM_MAPPED_FROM
            && quad.object == RdfTerm::iri(format!("{GM}projections/name"))));
        assert!(quads.iter().any(|quad| quad.predicate == RDF_REIFIES && matches!(
                &quad.object, RdfTerm::Triple(triple) if triple.predicate == format!("{GM}writtenRep") && triple.object == public_name
            )), "provenance must refer to the published assertion");
        let text = result.into_text().expect("explicit text output");
        assert!(!text.base_plus_derived_nt.contains("x-gmeow"));
        assert_eq!(
            text,
            transform_nt(&source, "", &[], &[], &projections, &tags).expect("consumer adapter")
        );
    }
}

#[test]
fn reifier_hash_matches_python_contract() {
    let s = RdfTerm::iri("https://example.org/s");
    let p = "https://example.org/p";
    let o = RdfTerm::iri("https://example.org/o");
    assert_eq!(
        reifier_for(&s, p, &o),
        "https://blackcatinformatics.ca/gmeow/derivations/bc5c0b0074e06845"
    );
}

/// G9 canonical-subsumption sweep: `subclass_closure` reads `onto` — the
/// AUTHORED `ontology/gmeow.ttl` ⊕ slice `module.ttl` merge (see
/// `ontology_source_files` in `crates/pipeline/src/scoreboards.rs`), never a
/// lowered `rdfs:`-only projection. It must traverse the canonical
/// `logic:subClassOf` edge, not only its `rdfs:` projection (gmeow_ns::SUB_CLASS_OF
/// doctrine; crates/ns/src/lib.rs:106-166), or a re-authored Appellation
/// subclass silently drops out of the suppression vocabulary.
#[test]
fn subclass_closure_traverses_canonical_logic_subclass_of() {
    const GM_APPELLATION_LOCAL: &str = "https://blackcatinformatics.ca/gmeow/Appellation";
    const GM_PERSON_NAME: &str = "https://blackcatinformatics.ca/gmeow/PersonName";
    let nt = format!(
        "<{GM_PERSON_NAME}> <https://blackcatinformatics.ca/logic/subClassOf> <{GM_APPELLATION_LOCAL}> .\n"
    );
    let graph = parse_graph(nt.as_bytes()).expect("fixture must parse");
    let closure = subclass_closure(&graph, GM_APPELLATION_LOCAL);
    assert!(
        closure.contains(&format!("<{GM_PERSON_NAME}>")),
        "subclass_closure must traverse the canonical logic:subClassOf edge: {closure:?}"
    );
}

#[test]
fn curie_prefers_longest_namespace() {
    assert_eq!(
        curie("http://id.loc.gov/ontologies/bibframe/Work"),
        "bf:Work"
    );
}

// ── Equivalence saturation E(G): strong-only, lint-gated, suppression-safe ──
//
// These reproduce the saturation-engine scenarios over hermetic, minimal
// N-Triples inputs — no repo ontology, DSL, or fixture files. `saturate_nt`
// is the engine under test; the fixtures below exercise every branch of
// `build_strong_edges` / `saturate_graph` / `emit_derived` / `cell_annotations`.

const GM_PERSON: &str = "https://blackcatinformatics.ca/gmeow/Person";
const GM_CORPUS: &str = "https://blackcatinformatics.ca/gmeow/Corpus";
const SCHEMA_PERSON: &str = "https://schema.org/Person";
const SCHEMA_DATASET: &str = "https://schema.org/Dataset";
const FOAF_PERSON: &str = "http://xmlns.com/foaf/0.1/Person";
const WD_Q42: &str = "http://www.wikidata.org/entity/Q42";
const EX_ME: &str = "https://example.org/sat/me";
const EX_CORPUS: &str = "https://example.org/sat/corpus";
const EX_SUPPRESSED: &str = "https://example.org/sat/suppressed";
const EX_CONTROL: &str = "https://example.org/sat/control";
const PERSON_SCHEMA_CELL: &str = "https://blackcatinformatics.ca/gmeow/te/person-schema";
const PERSON_FOAF_CELL: &str = "https://blackcatinformatics.ca/gmeow/te/person-foaf";
const GM_KNOWS: &str = "https://blackcatinformatics.ca/gmeow/knows";
const FOAF_KNOWS: &str = "http://xmlns.com/foaf/0.1/knows";
const KNOWS_FOAF_CELL: &str = "https://blackcatinformatics.ca/gmeow/te/knows-foaf";
const EX_A: &str = "https://example.org/sat/a";
const EX_B: &str = "https://example.org/sat/b";
const EX_C: &str = "https://example.org/sat/c";
const EX_D: &str = "https://example.org/sat/d";

fn cell(
    iri: &str,
    subject: &str,
    predicate_curie: &str,
    object: &str,
    confidence: &str,
) -> CellInput {
    CellInput {
        iri: iri.to_owned(),
        subject: subject.to_owned(),
        predicate_curie: predicate_curie.to_owned(),
        object: object.to_owned(),
        confidence: (!confidence.is_empty()).then(|| {
            gmeow_logic_compile::ir::UnitInterval::new(RdfLiteral::typed(confidence, XSD_DECIMAL))
                .unwrap()
        }),
    }
}

/// One N-Triples statement with an IRI object.
fn nt(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> <{object}> .\n")
}

/// The minimal ontology every class-edge scenario needs: `gmeow:Person a owl:Class`.
fn person_onto() -> String {
    nt(GM_PERSON, RDF_TYPE, OWL_CLASS)
}

/// A single `gmeow:Person` instance.
fn person_abox() -> String {
    nt(EX_ME, RDF_TYPE, GM_PERSON)
}

/// Two strong class edges for `gmeow:Person`: one via `owl:equivalentClass`
/// (confidence 0.9), one via `skos:exactMatch` (confidence 0.8).
fn person_cells() -> Vec<CellInput> {
    vec![
        cell(
            PERSON_SCHEMA_CELL,
            GM_PERSON,
            "owl:equivalentClass",
            SCHEMA_PERSON,
            "0.9",
        ),
        cell(
            PERSON_FOAF_CELL,
            GM_PERSON,
            "skos:exactMatch",
            FOAF_PERSON,
            "0.8",
        ),
    ]
}

/// The minimal ontology a property-edge scenario needs: `gmeow:knows a owl:ObjectProperty`.
fn knows_onto() -> String {
    nt(GM_KNOWS, RDF_TYPE, OWL_OBJECT_PROPERTY)
}

/// One strong property edge: `gmeow:knows owl:equivalentProperty foaf:knows`.
fn knows_cells() -> Vec<CellInput> {
    vec![cell(
        KNOWS_FOAF_CELL,
        GM_KNOWS,
        "owl:equivalentProperty",
        FOAF_KNOWS,
        "0.9",
    )]
}

fn iri_token(iri: &str) -> String {
    format!("<{iri}>")
}

fn type_objects(rows: &[DerivedRowNative]) -> BTreeSet<String> {
    rows.iter()
        .filter(|r| r.predicate == RDF_TYPE)
        .map(|r| r.object.clone())
        .collect()
}

#[test]
fn saturate_materializes_all_strong_class_edges() {
    // gmeow:Person saturates to every strong external equivalent at once.
    let rows = saturate_nt(&person_abox(), &person_onto(), &person_cells(), &[]).unwrap();
    assert_eq!(
        type_objects(&rows),
        BTreeSet::from([iri_token(SCHEMA_PERSON), iri_token(FOAF_PERSON)]),
    );
}

#[test]
fn saturate_ignores_close_match_hints() {
    // gmeow:Corpus has ONLY a closeMatch cell — a hint must not become a fact.
    let onto = nt(GM_CORPUS, RDF_TYPE, OWL_CLASS);
    let corpus_cell = "https://blackcatinformatics.ca/gmeow/te/corpus-dataset";
    let cells = vec![cell(
        corpus_cell,
        GM_CORPUS,
        "skos:closeMatch",
        SCHEMA_DATASET,
        "0.5",
    )];
    let abox = nt(EX_CORPUS, RDF_TYPE, GM_CORPUS);
    let rows = saturate_nt(&abox, &onto, &cells, &[]).unwrap();
    assert!(
        rows.is_empty(),
        "closeMatch must never materialize: {rows:?}"
    );

    // Positive control (non-vacuous): the SAME fixture with a STRONG
    // predicate DOES materialize — proving the empty result above is
    // closeMatch filtering, not a broken/inert fixture.
    let strong = vec![cell(
        corpus_cell,
        GM_CORPUS,
        "owl:equivalentClass",
        SCHEMA_DATASET,
        "0.5",
    )];
    let control = saturate_nt(&abox, &onto, &strong, &[]).unwrap();
    assert_eq!(
        type_objects(&control),
        BTreeSet::from([iri_token(SCHEMA_DATASET)]),
        "strong predicate over the same fixture must materialize"
    );
}

#[test]
fn saturate_refuses_denied_cell_keeps_siblings() {
    // A lint-ERROR row (the denial key is the CURIE triple) emits nothing;
    // the sibling strong edge is untouched.
    let denied = vec![(
        "gmeow:Person".to_owned(),
        "owl:equivalentClass".to_owned(),
        "schema:Person".to_owned(),
    )];
    let rows = saturate_nt(&person_abox(), &person_onto(), &person_cells(), &denied).unwrap();
    let types = type_objects(&rows);
    assert!(
        !types.contains(&iri_token(SCHEMA_PERSON)),
        "denied edge leaked"
    );
    assert!(types.contains(&iri_token(FOAF_PERSON)), "sibling edge lost");
}

#[test]
fn saturate_drops_suppressed_nodes_keeps_control() {
    // A displayable-false node never saturates; its control twin does (non-vacuous).
    let cells = vec![cell(
        PERSON_SCHEMA_CELL,
        GM_PERSON,
        "owl:equivalentClass",
        SCHEMA_PERSON,
        "0.9",
    )];
    let mut abox = String::new();
    abox.push_str(&nt(EX_SUPPRESSED, RDF_TYPE, GM_PERSON));
    abox.push_str(&format!(
            "<{EX_SUPPRESSED}> <{GM_DISPLAYABLE}> \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean> .\n"
        ));
    abox.push_str(&nt(EX_CONTROL, RDF_TYPE, GM_PERSON));
    let rows = saturate_nt(&abox, &person_onto(), &cells, &[]).unwrap();
    let subjects: BTreeSet<String> = rows.iter().map(|r| r.subject.clone()).collect();
    assert!(
        !subjects.contains(&iri_token(EX_SUPPRESSED)),
        "suppressed node saturated"
    );
    assert!(
        subjects.contains(&iri_token(EX_CONTROL)),
        "control twin missing"
    );
}

#[test]
fn saturate_mirrors_same_as_to_schema() {
    // owl:sameAs external links mirror to schema:sameAs, rule-attributed.
    let abox = nt(EX_ME, OWL_SAME_AS, WD_Q42);
    let rows = saturate_nt(&abox, &person_onto(), &[], &[]).unwrap();
    let mirrors: Vec<&DerivedRowNative> = rows
        .iter()
        .filter(|r| r.predicate == SCHEMA_SAME_AS)
        .collect();
    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].subject, iri_token(EX_ME));
    assert_eq!(mirrors[0].object, iri_token(WD_Q42));
    assert!(
        mirrors[0]
            .annotations
            .contains(&(GM_MAPPED_FROM.to_owned(), iri_token(SAME_AS_MIRROR_RULE)))
    );
}

#[test]
fn saturate_mirrors_strong_property_edge() {
    // A strong equivalentProperty cell mirrors <a> gmeow:knows <b> to
    // <a> foaf:knows <b>, carrying the object through, cell-attributed.
    let abox = nt(EX_A, GM_KNOWS, EX_B);
    let rows = saturate_nt(&abox, &knows_onto(), &knows_cells(), &[]).unwrap();
    assert_eq!(rows.len(), 1, "exactly the one mirrored edge: {rows:?}");
    let mirror = &rows[0];
    assert_eq!(mirror.predicate, FOAF_KNOWS);
    assert_eq!(mirror.subject, iri_token(EX_A));
    assert_eq!(mirror.object, iri_token(EX_B));
    assert!(
        mirror
            .annotations
            .contains(&(GM_MAPPED_FROM.to_owned(), iri_token(KNOWS_FOAF_CELL)))
    );
}

#[test]
fn saturate_drops_property_edge_with_suppressed_object() {
    // The property branch skips an edge whose OBJECT is suppressed (the
    // class-edge test only covers subject suppression); a control edge to a
    // visible object still mirrors — non-vacuous.
    let mut abox = String::new();
    abox.push_str(&nt(EX_A, GM_KNOWS, EX_SUPPRESSED));
    abox.push_str(&format!(
            "<{EX_SUPPRESSED}> <{GM_DISPLAYABLE}> \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean> .\n"
        ));
    abox.push_str(&nt(EX_C, GM_KNOWS, EX_B));
    let rows = saturate_nt(&abox, &knows_onto(), &knows_cells(), &[]).unwrap();
    let edges: BTreeSet<(String, String)> = rows
        .iter()
        .filter(|r| r.predicate == FOAF_KNOWS)
        .map(|r| (r.subject.clone(), r.object.clone()))
        .collect();
    assert!(
        !edges.contains(&(iri_token(EX_A), iri_token(EX_SUPPRESSED))),
        "suppressed-object edge leaked"
    );
    assert!(
        edges.contains(&(iri_token(EX_C), iri_token(EX_B))),
        "control edge lost"
    );
}

#[test]
fn saturate_coarsen_guard_skips_edge_when_coarsen_to_present() {
    // A coarsen-guarded property whose subject carries gmeow:coarsenTo is
    // skipped; an unguarded subject still mirrors (positive control).
    let mut onto = knows_onto();
    onto.push_str(&format!(
            "<{GM_KNOWS}> <{GM_COARSEN_GUARDED}> \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean> .\n"
        ));
    let mut abox = String::new();
    abox.push_str(&nt(EX_A, GM_KNOWS, EX_B));
    abox.push_str(&nt(EX_A, GM_COARSEN_TO, EX_D)); // guard trips for EX_A
    abox.push_str(&nt(EX_C, GM_KNOWS, EX_B)); // no coarsenTo → control mirrors
    let rows = saturate_nt(&abox, &onto, &knows_cells(), &[]).unwrap();
    let subjects: BTreeSet<String> = rows
        .iter()
        .filter(|r| r.predicate == FOAF_KNOWS)
        .map(|r| r.subject.clone())
        .collect();
    assert!(
        !subjects.contains(&iri_token(EX_A)),
        "coarsen-guarded edge leaked"
    );
    assert!(
        subjects.contains(&iri_token(EX_C)),
        "unguarded control edge lost"
    );
}

#[test]
fn saturate_annotates_cell_iri_and_confidence() {
    // Every derived triple is mappedFrom-attributed to its authored cell and
    // carries the cell's confidence as a typed decimal literal.
    let rows = saturate_nt(&person_abox(), &person_onto(), &person_cells(), &[]).unwrap();
    let schema_row = rows
        .iter()
        .find(|r| r.object == iri_token(SCHEMA_PERSON))
        .expect("schema:Person row");
    assert!(
        schema_row
            .annotations
            .contains(&(GM_MAPPED_FROM.to_owned(), iri_token(PERSON_SCHEMA_CELL)))
    );
    assert!(schema_row.annotations.contains(&(
        GM_CONFIDENCE.to_owned(),
        format!("\"0.9\"^^<{XSD_DECIMAL}>")
    )));
}

#[test]
fn saturate_allows_absent_confidence() {
    // A cell may record no confidence — it still materializes, the
    // gmeow:confidence annotation is simply omitted (not a default).
    let cells = vec![cell(
        PERSON_SCHEMA_CELL,
        GM_PERSON,
        "owl:equivalentClass",
        SCHEMA_PERSON,
        "",
    )];
    let rows = saturate_nt(&person_abox(), &person_onto(), &cells, &[]).unwrap();
    let schema_row = rows
        .iter()
        .find(|r| r.object == iri_token(SCHEMA_PERSON))
        .expect("schema:Person row");
    assert!(
        schema_row
            .annotations
            .iter()
            .all(|(k, _)| k != GM_CONFIDENCE),
        "absent confidence must not be annotated: {:?}",
        schema_row.annotations
    );
    assert!(
        schema_row
            .annotations
            .contains(&(GM_MAPPED_FROM.to_owned(), iri_token(PERSON_SCHEMA_CELL)))
    );
}

/// GMEOW derivation provenance must carry the exact source coordinate, not a
/// decimal reconstructed from a textual or binary64 intermediate.
#[test]
fn saturate_preserves_confidence_datatype_and_decimal_precision() {
    for (lexical, datatype) in [
        ("0.123456789012345678", XSD_DECIMAL),
        ("+0.5E0", "http://www.w3.org/2001/XMLSchema#double"),
    ] {
        let mut cells = person_cells();
        let confidence =
            gmeow_logic_compile::ir::UnitInterval::new(RdfLiteral::typed(lexical, datatype))
                .unwrap();
        cells[0].confidence = Some(confidence);
        let result = saturate_nt(&person_abox(), &person_onto(), &cells, &[]).unwrap();
        assert!(
            result.iter().any(|row| row.annotations.contains(&(
                GM_CONFIDENCE.to_owned(),
                format!("\"{lexical}\"^^<{datatype}>")
            ))),
            "{lexical} {datatype}"
        );
    }
}

#[test]
fn saturate_skips_already_asserted_triple() {
    // G is canonical — a triple already in the A-Box gets no derived row / reifier.
    let cells = vec![cell(
        PERSON_SCHEMA_CELL,
        GM_PERSON,
        "owl:equivalentClass",
        SCHEMA_PERSON,
        "0.9",
    )];
    let mut abox = person_abox();
    abox.push_str(&nt(EX_ME, RDF_TYPE, SCHEMA_PERSON));
    let rows = saturate_nt(&abox, &person_onto(), &cells, &[]).unwrap();
    assert!(
        rows.is_empty(),
        "already-asserted triple was re-derived: {rows:?}"
    );
}

#[test]
fn saturate_is_deterministic() {
    // Two runs over a RICH A-Box — multiple subjects, class + property +
    // sameAs edges, and a literal-bearing triple — derive byte-identical
    // rows, including the content-addressed reifiers. Ordering/reifier
    // nondeterminism only surfaces with many mixed rows, not the 2-row
    // single-subject case.
    let mut onto = person_onto();
    onto.push_str(&knows_onto());
    let mut cells = person_cells();
    cells.extend(knows_cells());
    let mut abox = String::new();
    abox.push_str(&nt(EX_ME, RDF_TYPE, GM_PERSON));
    abox.push_str(&nt(EX_CONTROL, RDF_TYPE, GM_PERSON));
    abox.push_str(&nt(EX_A, GM_KNOWS, EX_B));
    abox.push_str(&nt(EX_C, GM_KNOWS, EX_D));
    abox.push_str(&nt(EX_ME, OWL_SAME_AS, WD_Q42));
    abox.push_str(&format!("<{EX_ME}> <{GM}fullName> \"Ada\" .\n"));

    let run_a = saturate_nt(&abox, &onto, &cells, &[]).unwrap();
    let run_b = saturate_nt(&abox, &onto, &cells, &[]).unwrap();
    assert_eq!(run_a, run_b);
    assert_eq!(
        run_a.len(),
        7,
        "2 Person subjects × 2 class edges + 2 property mirrors + 1 sameAs mirror: {run_a:?}"
    );
}

// ── projection P(G): onto-only-catalog exclusion + tag_map retag boundary ──
//
// Regression coverage for the `gmeow-cli/tests/self_sufficiency.rs` "zero
// x-gmeow leak" finding: a projection CONSTRUCT whose WHERE clause matches
// `?app gmeow:fullName ?name` (the `ontolex` profile's real shape) and mints
// a fresh `<subject>-form` IRI via `BIND(IRI(CONCAT(...)))` must NOT leak an
// onto-only individual's data into MAXIMAL(G) — only abox-derived facts are
// "about the instance" — and any internally-tagged literal that DOES survive
// into the output must be retagged to its public BCP-47 form.

const CATALOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/catalogEntry";
const FULL_NAME_QUERY: &str = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\nCONSTRUCT { ?form gmeow:writtenRep ?name }\nWHERE { ?app gmeow:fullName ?name . BIND(IRI(CONCAT(STR(?app), \"-form\")) AS ?form) }";

#[test]
fn projection_derived_excludes_onto_only_catalog_forms_but_keeps_abox_derived_ones() {
    // onto: an ontology-authored "reference catalog" individual (mirrors
    // `imports/languages-reference.ttl`'s exonym Appellations) — NOT part of
    // the transpiled instance.
    let ontology_nt = format!("<{CATALOG_ENTRY}> <{GM}fullName> \"Catalog Entry\" .\n");
    // abox: our actual instance data, carrying the SAME predicate.
    let raw_nt = format!("<{EX_ME}> <{GM}fullName> \"Ada Lovelace\" .\n");

    let report = transform_nt(
        &raw_nt,
        &ontology_nt,
        &[],
        &[],
        &[("catalog-forms".to_owned(), FULL_NAME_QUERY.to_owned())],
        &TagMap::new(),
    )
    .unwrap();

    assert!(
        !report.base_plus_derived_nt.contains("catalogEntry-form"),
        "onto-only catalog individual's synthesized form leaked into MAXIMAL(G): {}",
        report.base_plus_derived_nt
    );
    assert!(
        report.base_plus_derived_nt.contains("sat/me-form"),
        "abox-derived synthesized form is missing (over-exclusion): {}",
        report.base_plus_derived_nt
    );
}

#[test]
fn transform_nt_retags_internal_language_tags_at_the_maximal_output_boundary() {
    // The instance's own fullName literal carries an internal x-gmeow-*
    // authoring tag (the normal in-ontology convention); `tag_map` maps it
    // to its public BCP-47 form, exactly as `project`/`export` already do at
    // their projection boundaries (`crate::projections::retag_quads`).
    let raw_nt = format!("<{EX_ME}> <{GM}fullName> \"Ada Lovelace\"@x-gmeow-english .\n");
    let mut tag_map = TagMap::new();
    tag_map.insert("x-gmeow-english".to_owned(), "en".to_owned());

    let report = transform_nt(&raw_nt, "", &[], &[], &[], &tag_map).unwrap();

    assert!(
        report.base_plus_derived_nt.contains("\"Ada Lovelace\"@en"),
        "internal tag was not retagged to its public BCP-47 form: {}",
        report.base_plus_derived_nt
    );
    assert!(
        !report.base_plus_derived_nt.contains("x-gmeow-english"),
        "internal tag leaked into the MAXIMAL(G) output: {}",
        report.base_plus_derived_nt
    );
}
