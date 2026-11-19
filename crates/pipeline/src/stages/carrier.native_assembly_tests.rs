// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Count the `owl:AllDisjointClasses` typed subjects (blank nodes) and the
/// `owl:members` list-head triples in a canonical N-Quads blob.
fn disjoint_shape(canon: &str) -> (usize, usize) {
    let all_disjoint = canon
        .lines()
        .filter(|l| {
            l.contains("<http://www.w3.org/2002/07/owl#AllDisjointClasses>")
                && l.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>")
        })
        .count();
    let members = canon
        .lines()
        .filter(|l| l.contains("<http://www.w3.org/2002/07/owl#members>"))
        .count();
    (all_disjoint, members)
}

/// Two distinct `owl:AllDisjointClasses` axioms authored in SEPARATE files MUST
/// survive the native `RdfDataset::union` standardize-apart as TWO distinct blank
/// lists — never collapsing into one. This is exactly why the removed
/// `ingest_turtle_scoped` string-prefixed per-file blanks; the union's per-input
/// `BlankScope` is its native replacement. Each file independently mints `_:b0`
/// (the codecs restart blank counters per parse), so without standardize-apart the
/// two axioms would merge into a single subject and one of the lists would vanish.
#[test]
fn two_all_disjoint_lists_survive_union_distinctly() {
    // Two files, each with ONE owl:AllDisjointClasses over a DIFFERENT class set,
    // both anonymous (blank-node subject + blank-node list cells).
    let file_a = br#"@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .
[] a owl:AllDisjointClasses ; owl:members ( ex:A ex:B ex:C ) .
"#;
    let file_b = br#"@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .
[] a owl:AllDisjointClasses ; owl:members ( ex:D ex:E ) .
"#;

    let union = union_turtle_datasets(&[file_a.to_vec(), file_b.to_vec()])
        .expect("union two disjoint files");
    let nq = dataset_to_nquads(&union).expect("union → n-quads");
    let canon = canonicalize_nq(&nq, "base").expect("canonicalize union");

    let (subjects, members) = disjoint_shape(&canon);
    assert_eq!(
        subjects, 2,
        "the union must keep TWO distinct AllDisjointClasses subjects (one per file); \
             a collapse would leave only 1.\nCanonical:\n{canon}"
    );
    assert_eq!(
        members, 2,
        "each AllDisjointClasses must keep its own owl:members list head"
    );

    // The two list contents (3-element and 2-element) must both be present — a
    // collapse would lose one set entirely. Count rdf:first cells: 3 + 2 = 5.
    let first_cells = canon
        .lines()
        .filter(|l| l.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#first>"))
        .count();
    assert_eq!(
        first_cells, 5,
        "both lists (3 + 2 members) must survive distinctly; got {first_cells} rdf:first cells"
    );

    // Contrast: parsing BOTH files into ONE dataset WITHOUT standardize-apart
    // would let the two `_:b0` subjects collide. We can't easily force that here,
    // but the union path above is the production assembly — its 2-subject result
    // is the proof the native union preserves per-file distinctness.
}

/// The projection-ledger named graph built natively (`turtle_to_nquads`)
/// canonicalizes to a STABLE, idempotent RDFC-1.0 N-Quads form that carries every
/// authored triple (typed literals + the blank-node structural-drop list). This
/// retired the prior oxigraph-`Store` cross-check: the conversion is now fully native
/// (`turtle_to_nquads` → `canonical_flat_nquads`), so the meaningful invariant is
/// canonical idempotence + content fidelity, not equality to a removed oxigraph path.
#[test]
fn projection_ledger_canonicalizes_stably() {
    // A representative projection-report fragment: typed loss-ledger entries with
    // a blank-node structural-drop list (exercises blank canonicalization).
    let report_ttl = br#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
<https://blackcatinformatics.ca/gmeow/projection/okf>
    a gmeow:ProjectionLedgerEntry ;
    rdfs:label "OKF projection" ;
    gmeow:preservationKind gmeow:Lossy ;
    gmeow:droppedCount "3"^^xsd:integer ;
    gmeow:structuralDrop [ gmeow:dropKind gmeow:StatementLayer ] .
"#;

    // Native path: the C3 helper, then RDFC-1.0 canonicalize.
    let native = turtle_to_nquads(report_ttl).expect("native turtle → n-quads");
    let native_canon = canonicalize_nq(&native, "projledger").expect("canon native");

    // Idempotence: re-canonicalizing the canonical form is a fixpoint.
    let recanon = canonicalize_nq(native_canon.as_bytes(), "projledger").expect("recanon");
    assert_eq!(
        native_canon, recanon,
        "RDFC-1.0 canonicalization of the projection-ledger N-Quads must be idempotent"
    );

    // Content fidelity: every authored triple survives (typed literal + blank-node
    // structural-drop list), and a canonical blank label (`_:c14n…`) is minted.
    assert!(native_canon.contains("<https://blackcatinformatics.ca/gmeow/projection/okf>"));
    assert!(native_canon.contains("\"3\"^^<http://www.w3.org/2001/XMLSchema#integer>"));
    assert!(native_canon.contains("<https://blackcatinformatics.ca/gmeow/StatementLayer>"));
    assert!(
        native_canon.contains("_:c14n"),
        "the blank structural-drop node must carry a canonical RDFC-1.0 label"
    );
}

/// Reusing the checked report under both graph names preserves the coherence
/// preimage and refuses an independently loaded report that disagrees.
#[test]
fn projection_report_reuse_preserves_carrier_and_rejects_a_mismatch() {
    const REPORT: &[u8] = br#"<urn:report> <urn:kind> <urn:projection> ;
        <urn:count> 7 ; <urn:label> "Projection"@en ."#;
    const DIFFERENT: &[u8] = br#"<urn:report> <urn:kind> <urn:projection> ;
        <urn:count> "7" ; <urn:label> "Projection"@en ."#;

    let report_iri = crate::stages::superset::rdf_fanout_graph_iri(
        crate::stages::compile_logic::PROJECTION_REPORT_PATH,
    )
    .expect("the report has a registered fanout graph");
    let ledger = parse_into_graph(
        &turtle_to_nquads(REPORT).expect("map the report to normalized statements"),
        "application/n-quads",
        GRAPH_PROJECTION_LEDGER,
    )
    .expect("construct the native ledger as mappings does");
    let other = parse_into_graph(DIFFERENT, "text/turtle", "urn:other")
        .expect("include the alternative term in a different graph");
    let dataset =
        purrdf::RdfDataset::union(&[&ledger, &other]).expect("combine the two supplied graphs");
    let bundle = crate::bundle::bundle_from_artifacts_over(
        std::sync::Arc::new(dataset),
        BTreeMap::new(),
        DatasetProvenance::new(),
    );
    let upstream = BTreeMap::from([(
        "stage-mappings".to_owned(),
        StageProduct::from_bundle("stage-mappings", std::sync::Arc::new(bundle))
            .expect("admit the supplied mappings product"),
    )]);
    let ledger = upstream["stage-mappings"].dataset();
    let graph = ledger
        .term_id_by_iri(GRAPH_PROJECTION_LEDGER)
        .expect("the ledger graph is declared");
    let rows = graph_quad_count(ledger, GRAPH_PROJECTION_LEDGER);

    let mut separate = CarrierParts::new(&upstream);
    separate
        .push_graph("stage-mappings", GRAPH_PROJECTION_LEDGER)
        .expect("attach the native projection ledger");
    separate
        .push_parsed(REPORT, "text/turtle", &report_iri)
        .expect("attach the separately parsed report");

    let mut shared = CarrierParts::new(&upstream);
    let selection = shared
        .select_graph_source("stage-mappings", GRAPH_PROJECTION_LEDGER)
        .expect("select the native ledger once");
    shared.place_graph_source(selection.clone(), GRAPH_PROJECTION_LEDGER);
    check_projection_report(REPORT, &report_iri, ledger, graph, rows)
        .expect("admit the report's exact terms");
    shared.place_graph_source(selection, &report_iri);

    let canonical = |parts: CarrierParts<'_>| {
        purrdf::try_canonicalize_flat_view(
            &parts.view().expect("compose the report carrier"),
            purrdf::CanonHash::Sha256,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .expect("canonicalize the report carrier")
        .nquads()
        .to_owned()
    };
    assert_eq!(canonical(shared), canonical(separate));
    for bytes in [
        DIFFERENT,
        b"<urn:report> <urn:count> 7 .",
        b"<incomplete Turtle",
    ] {
        assert!(check_projection_report(bytes, &report_iri, ledger, graph, rows).is_err());
    }
}

/// Snapshot reuse keeps the old documentation placement, certificate inputs
/// and materialized carrier content across independently scoped neighbours.
#[test]
fn rooted_documentation_reuse_preserves_snapshot_identity() {
    let documentation = parse_into_graph(
        br#"_:member <urn:docs:count> 7 ; <urn:docs:label> "Documentation"@en .
            <urn:docs:claim> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies>
                <<( _:member <urn:docs:count> 7 )>> ;
                <urn:docs:evidence> <urn:docs:source> ."#,
        "text/turtle",
        GRAPH_DOCUMENTATION,
    )
    .expect("parse and root documentation RDF");
    let empty = parse_into_graph(b"", "text/turtle", GRAPH_DOCUMENTATION)
        .expect("root an empty documentation graph");
    let neighbour = parse_into_graph(
        b"_:member <urn:docs:label> <urn:docs:source> .",
        "text/turtle",
        "urn:neighbour",
    )
    .expect("parse an independently scoped neighbouring graph");
    let canonical = |view: &purrdf::CompositeDatasetView| {
        purrdf::try_canonicalize_flat_view(
            view,
            purrdf::CanonHash::Sha256,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .expect("canonicalize the documentation carrier")
    };
    let hashes = |view: &purrdf::CompositeDatasetView| {
        gmeow_logic::certificate::per_graph_axiom_hashes(view, purrdf::gts::writer::digest_string)
    };
    let content = |view: &purrdf::CompositeDatasetView| {
        let carrier = view
            .materialize()
            .expect("materialize the composed carrier");
        let mut snapshot = purrdf::gts_compose::SnapshotBuilder::new();
        let _ingestion = snapshot
            .add_view(carrier.as_ref())
            .expect("fold the materialized carrier");
        assert!(holds_named_graph(carrier.as_ref(), GRAPH_DOCUMENTATION));
        snapshot.snapshot_content_id()
    };

    for dataset in [documentation, empty] {
        let upstream = documentation_input(dataset);
        let mut selected = CarrierParts::new(&upstream);
        let mut reused = CarrierParts::new(&upstream);
        for parts in [&mut selected, &mut reused] {
            parts.push_dataset(neighbour.clone());
        }
        selected
            .push_graph("stage-docs-render", GRAPH_DOCUMENTATION)
            .expect("select the documentation graph");
        reused
            .push_rooted_graph("stage-docs-render", GRAPH_DOCUMENTATION)
            .expect("reuse the native documentation graph");
        for parts in [&mut selected, &mut reused] {
            parts.push_dataset(neighbour.clone());
        }
        let selected = selected.view().expect("compose with the graph selection");
        let reused = reused.view().expect("compose with the root dataset");

        assert_eq!(canonical(&selected).nquads(), canonical(&reused).nquads());
        assert_eq!(hashes(&selected), hashes(&reused));
        assert_eq!(content(&selected), content(&reused));
    }
}

/// Reuse refuses carrier rows from the default graph and unexpected declarations.
#[test]
fn rooted_documentation_reuse_refuses_foreign_surfaces() {
    let empty = RdfDatasetBuilder::new()
        .freeze()
        .expect("freeze an undeclared dataset");
    let unexpected_declaration = purrdf::parse_dataset(
        format!("<{GRAPH_DOCUMENTATION}> {{}} <urn:unexpected> {{}}").as_bytes(),
        "application/trig",
        None,
    )
    .expect("parse the candidate's extra graph declaration");
    for dataset in [empty, unexpected_declaration] {
        let upstream = documentation_input(dataset);
        let error = CarrierParts::new(&upstream)
            .push_rooted_graph("stage-docs-render", GRAPH_DOCUMENTATION)
            .expect_err("refuse an absent or extra graph declaration");
        assert!(error.to_string().contains("must declare only"));
    }

    #[derive(Debug)]
    enum Layer {
        Ordinary,
        Reifier,
        Annotation,
    }
    for layer in [Layer::Ordinary, Layer::Reifier, Layer::Annotation] {
        let mut builder = RdfDatasetBuilder::new();
        let graph = builder
            .intern_iri(GRAPH_DOCUMENTATION)
            .expect("intern the documentation graph");
        builder
            .declare_named_graph(graph)
            .expect("declare the graph");
        let s = builder.intern_iri("urn:s").expect("intern the subject");
        let p = builder.intern_iri("urn:p").expect("intern the predicate");
        let o = builder.intern_iri("urn:o").expect("intern the object");
        let reifier = builder
            .intern_iri("urn:reifier")
            .expect("intern the reifier");
        let statement = builder.intern_triple(s, p, o).expect("intern the triple");
        match layer {
            Layer::Ordinary => builder
                .push_quad(s, p, o, None)
                .expect("add a default quad"),
            Layer::Reifier => builder
                .push_reifier_in_graph(reifier, statement, None)
                .expect("add a default reifier"),
            Layer::Annotation => {
                builder
                    .push_reifier_in_graph(reifier, statement, Some(graph))
                    .expect("declare the named reifier");
                builder
                    .push_annotation_in_graph(reifier, p, o, None)
                    .expect("add a default annotation");
            }
        }
        let dataset = builder
            .freeze()
            .expect("freeze RDF with default graph rows");
        let upstream = documentation_input(dataset);
        let error = CarrierParts::new(&upstream)
            .push_rooted_graph("stage-docs-render", GRAPH_DOCUMENTATION)
            .expect_err("refuse default graph rows");
        assert!(
            error.to_string().contains("contains RDF rows outside"),
            "{layer:?}: {error}"
        );
    }
}

fn documentation_input(
    dataset: std::sync::Arc<purrdf::RdfDataset>,
) -> BTreeMap<String, StageProduct> {
    BTreeMap::from([(
        "stage-docs-render".to_owned(),
        StageProduct::from_artifacts_over("stage-docs-render", dataset, BTreeMap::new())
            .expect("admit the supplied documentation product"),
    )])
}
