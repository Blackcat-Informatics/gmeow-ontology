// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const ASSESSMENT: &str = "https://blackcatinformatics.ca/gmeow/claim/advice.synthetic";
const ASSESSMENT_CLASS: &str = "https://blackcatinformatics.ca/gmeow/ComplianceAssessment";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn add_assessment(builder: &mut RdfDatasetBuilder, subject: &str, class: &str, graph: &str) {
    builder.push_owned_quad(
        &purrdf::RdfQuad::new(RdfTerm::iri(subject), RDF_TYPE, RdfTerm::iri(class))
            .in_graph(RdfTerm::iri(graph)),
    );
}

#[test]
fn advisory_census_requires_the_claim_graph_family_and_assessment_type() {
    let mut builder = RdfDatasetBuilder::new();
    add_assessment(&mut builder, ASSESSMENT, ASSESSMENT_CLASS, GRAPH);
    add_assessment(
        &mut builder,
        "https://blackcatinformatics.ca/gmeow/claim/advice.other-graph",
        ASSESSMENT_CLASS,
        "https://blackcatinformatics.ca/gmeow/graph/other",
    );
    add_assessment(
        &mut builder,
        "https://blackcatinformatics.ca/gmeow/claim/advice.wrong-class",
        "https://blackcatinformatics.ca/gmeow/Norm",
        GRAPH,
    );
    add_assessment(
        &mut builder,
        "https://blackcatinformatics.ca/gmeow/claim/binding.synthetic",
        ASSESSMENT_CLASS,
        GRAPH,
    );
    let dataset = builder.freeze().expect("synthetic norm-claim controls");
    let observed = observe_shipped(&dataset).expect("norm-claim observation");
    assert_eq!(observed.graph, GRAPH);
    assert_eq!(observed.asserted_quads, 3);
    assert_eq!(
        observed.advisory_assessments,
        BTreeSet::from([ASSESSMENT.to_owned()])
    );
}

#[test]
fn shipped_claim_commitment_includes_statement_metadata() {
    fn claims(evidence: &str) -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        add_assessment(&mut builder, ASSESSMENT, ASSESSMENT_CLASS, GRAPH);
        let reifier = RdfTerm::iri("https://blackcatinformatics.ca/gmeow/claim/evidence");
        builder.push_owned_reifier(
            &purrdf::RdfReifier::new(
                reifier.clone(),
                purrdf::RdfTriple::new(
                    RdfTerm::iri(ASSESSMENT),
                    RDF_TYPE,
                    RdfTerm::iri(ASSESSMENT_CLASS),
                ),
            )
            .in_graph(Some(RdfTerm::iri(GRAPH))),
        );
        builder.push_owned_annotation(
            &purrdf::RdfAnnotation::new(
                reifier,
                "http://www.w3.org/ns/prov#wasDerivedFrom",
                RdfTerm::iri(evidence),
            )
            .in_graph(Some(RdfTerm::iri(GRAPH))),
        );
        builder.freeze().expect("synthetic claim provenance")
    }
    let first = observe_shipped(&claims("urn:evidence:first")).expect("first observation");
    let second = observe_shipped(&claims("urn:evidence:second")).expect("second observation");
    assert_eq!(first.asserted_quads, second.asserted_quads);
    assert_eq!(first.advisory_assessments, second.advisory_assessments);
    assert_ne!(
        first.graph_digest, second.graph_digest,
        "matching advisory censuses cannot conceal substituted statement provenance"
    );
}

#[test]
fn shipped_norm_claims_abox_carries_the_advisory_assessment_and_reasons_cleanly() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes =
        gmeow_action_cache::selection::source_artifacts::load(&root, "stage-validate", CHANNEL)
            .expect("exact producer-selected norm-claims closure observation");
    let observed: Observation = serde_json::from_slice(&bytes).expect("typed closure evidence");
    let shipped = gmeow_bundle_import::load_authenticated_corpus_artifact(&root, ARTIFACT)
        .expect("exact selected terminal norm-claims observation");
    let shipped: ShippedObservation =
        serde_json::from_slice(&shipped).expect("typed terminal census");
    assert_eq!(shipped.graph, GRAPH);
    assert_eq!(
        observed.claims, shipped,
        "closure input must be the actual shipped graph, including statement metadata"
    );
    assert!(
        shipped.asserted_quads > 0,
        "the shipped norm-claims ABox must be nonempty"
    );
    assert!(
        !shipped.advisory_assessments.is_empty(),
        "the shipped graph must contain an advice-family ComplianceAssessment"
    );
    let closure = observed
        .closure
        .expect("the standalone norms TBox and actual shipped ABox must close without error");
    assert_eq!(closure.source_path, NORMS_SOURCE);
    assert_eq!(closure.source_digest.len(), 64);
    assert!(
        closure
            .source_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    assert!(
        closure.source_quads > 0,
        "the standalone norms TBox must be nonempty"
    );
    assert!(
        closure.closure_triples > 0,
        "the nonempty TBox and ABox must yield a nonempty closure"
    );
}
