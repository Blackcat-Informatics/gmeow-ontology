// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn sample_projection() -> Vec<(usize, String, String, String, Option<String>)> {
    vec![
        (
            0,
            "ontology/gmeow.ttl".to_string(),
            "root-ontology".to_string(),
            "ontology/gmeow.ttl".to_string(),
            None,
        ),
        (
            1,
            "slices/core/epistemics/module.ttl".to_string(),
            "source".to_string(),
            "slices/core/epistemics/module.ttl".to_string(),
            None,
        ),
        (
            2,
            "imports/prov.ttl".to_string(),
            "import".to_string(),
            "imports/prov.ttl".to_string(),
            None,
        ),
    ]
}

#[test]
fn projection_carries_procedure_and_execution_nodes() {
    let nt = project_provenance_graph(&sample_projection());
    assert!(nt.contains(&format!("<{PROCEDURE_IRI}> <{RDF_TYPE}> <{LOGIC}Plan> .")));
    assert!(nt.contains(&format!(
        "<{EXECUTION_IRI}> <{RDF_TYPE}> <{LOGIC}Enactment> ."
    )));
    assert!(nt.contains(&format!(
        "<{EXECUTION_IRI}> <{LOGIC}enactsPrescriptionVersion> <{PROCEDURE_IRI}> ."
    )));
}

#[test]
fn every_lane_is_a_step_with_a_loadbearing_bit() {
    let nt = project_provenance_graph(&sample_projection());
    for lane in LANES {
        let iri = lane_iri(lane.slug);
        assert!(
            nt.contains(&format!("<{iri}> <{RDF_TYPE}> <{LOGIC}ActionSchema> .")),
            "lane {} must be an ActionSchema",
            lane.slug
        );
        assert!(
            nt.contains(&format!("<{PROCEDURE_IRI}> <{LOGIC}planBody> <{iri}> .")),
            "lane {} must link to the plan",
            lane.slug
        );
        let expect = format!(
            "<{iri}> <{LOGIC}loadBearing> \"{}\"^^<{XSD_BOOLEAN}> .",
            lane.load_bearing
        );
        assert!(
            nt.contains(&expect),
            "lane {} must carry its loadBearing bit ({})",
            lane.slug,
            lane.load_bearing
        );
    }
}

#[test]
fn projection_carries_no_runtime_ids() {
    // S0.5: the public projection must NEVER leak a runtime id.
    let nt = project_provenance_graph(&sample_projection());
    assert!(!nt.contains("unit#"), "no runtime UnitId in the graph");
    assert!(
        !nt.contains("artifact#"),
        "no runtime ArtifactId in the graph"
    );
    assert!(
        !nt.contains("origin-set#"),
        "no runtime OriginSetId in the graph"
    );
}

#[test]
fn projection_is_byte_deterministic() {
    let a = project_provenance_graph(&sample_projection());
    // Re-project from a row-shuffled input — the emitter sorts, so the bytes match.
    let mut shuffled = sample_projection();
    shuffled.reverse();
    let b = project_provenance_graph(&shuffled);
    assert_eq!(
        a, b,
        "the projection must be byte-stable across input order"
    );
}

#[test]
fn each_unit_carries_name_and_kind() {
    let nt = project_provenance_graph(&sample_projection());
    let root = unit_iri("ontology/gmeow.ttl");
    assert!(nt.contains(&format!("<{root}> <{RDF_TYPE}> <{GMEOW}CompilationUnit> .")));
    assert!(nt.contains(&format!("<{root}> <{GMEOW}originKind> \"root-ontology\" .")));
}

/// Shift-left: drive the SAME native structural lint `make validate`/`make
/// check` run (`gmeow_validate::lint::structural_lint_dataset`) over this
/// generator's real output fragment, so a missing/incorrect A-Box annotation
/// on a minted `CompilationUnit`/`ActionSchema`/`Plan`/`Enactment`
/// individual reds HERE — a fast `cargo nextest -p gmeow-pipeline` — rather
/// than only surfacing at the next expensive whole-bundle SHACL validation
/// (`make validate` / the pipeline stage-validate).
#[test]
fn minted_individuals_satisfy_the_assertional_abox_contract() {
    use gmeow_validate::lint::{LintConfig, structural_lint_dataset};

    let nt = project_provenance_graph(&sample_projection());
    // The real bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the
    // kernel slice; add it here (same pattern as
    // `release.rs`'s `minted_attestations_satisfy_the_assertional_contract`)
    // so the graphBoxRole-typing check has its declaration to resolve against.
    let doc = format!("{nt}<{GMEOW}boxABox> <{RDF_TYPE}> <{GMEOW}GraphBoxRole> .\n");
    let ds = purrdf::parse_dataset(doc.as_bytes(), "application/n-triples", None)
        .expect("parse the provenance N-Triples fragment");

    let cfg = LintConfig {
        namespace: GMEOW.to_string(),
        ontology_iri: GMEOW.trim_end_matches('/').to_string(),
        selector_tokens: Default::default(),
        core_slice_iris: Default::default(),
        annotation_predicates: Default::default(),
    };
    let report = structural_lint_dataset(&ds, &cfg);
    let errors = report.errors();
    let provenance_errors: Vec<&String> = errors
        .iter()
        .filter(|e| e.contains(&format!("{GMEOW}provenance/")))
        .collect();
    assert!(
        provenance_errors.is_empty(),
        "every minted provenance individual must satisfy the A-Box annotation \
             contract (rdfs:label / skos:definition / rdfs:isDefinedBy / \
             gmeow:graphBoxRole): {provenance_errors:?}"
    );
}
