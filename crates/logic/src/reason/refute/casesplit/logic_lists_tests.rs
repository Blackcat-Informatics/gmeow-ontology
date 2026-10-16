// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compiled `logic:` lists reach the object-level `graph/logic` world complete.
//!
//! The source is compiled by `gmeow-logic-compile`, projected to the canonical RDF 1.2
//! carrier, re-rooted into `graph/logic` exactly as the pipeline's carrier stage does,
//! and admitted by the class-expression source admission. Admission is not weakened:
//! an incomplete list stays a refusal, so these tests fail if the compiler drops cells.

use super::*;
use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::ir::LogicProgram;
use gmeow_logic_compile::projections::rdf::project_canonical_rdf12_dataset;
use purrdf::RdfDatasetBuilder;
use std::sync::Arc;

const GRAPH_LOGIC: &str = "https://blackcatinformatics.ca/gmeow/graph/logic";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const EX: &str = "https://example.org/lists/";

/// Root every projected quad, reifier and annotation into `graph/logic`, mirroring
/// `crates/pipeline/src/stages/carrier.rs` `rooted_in_graph`.
fn rooted_in_graph_logic(sources: &[Arc<RdfDataset>]) -> Arc<RdfDataset> {
    let graph = RdfTerm::Iri(GRAPH_LOGIC.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    let destination = builder.intern_iri(GRAPH_LOGIC).unwrap();
    builder.declare_named_graph(destination).unwrap();
    for src in sources {
        for mut quad in src.owned_quads() {
            quad.graph_name = Some(graph.clone());
            builder.push_owned_quad(&quad).unwrap();
        }
        for mut reifier in src.owned_reifiers() {
            reifier.graph = Some(graph.clone());
            builder.push_owned_reifier(&reifier).unwrap();
        }
        for mut annotation in src.owned_annotations() {
            annotation.graph = Some(graph.clone());
            builder.push_owned_annotation(&annotation).unwrap();
        }
    }
    builder.freeze().expect("rooted graph/logic carrier")
}

fn canonical(program: &LogicProgram) -> Arc<RdfDataset> {
    project_canonical_rdf12_dataset(program)
        .expect("canonical projection")
        .dataset
}

/// Every source-admission issue of the `graph/logic` world, flattened.
fn admission_issues(analysis: &PreparedClassAnalysis) -> Vec<RefutationSourceIssue> {
    fn flatten(boundary: &FragmentBoundary, out: &mut Vec<RefutationSourceIssue>) {
        match boundary {
            FragmentBoundary::SourceAdmission { issue, .. } => out.push(issue.clone()),
            FragmentBoundary::Combined(boundaries) => {
                for boundary in boundaries {
                    flatten(boundary, out);
                }
            }
            other => panic!("source admission reported a non-source boundary: {other:?}"),
        }
    }
    let mut out = Vec::new();
    if let Some(world) = analysis.admission().selected_worlds.get(GRAPH_LOGIC)
        && let Some(boundary) = &world.refusal
    {
        flatten(boundary, &mut out);
    }
    out
}

/// The single object of `subject predicate ?o` in `graph/logic`.
fn only_object(dataset: &RdfDataset, subject: &str, predicate: &str) -> String {
    let objects: Vec<String> = dataset
        .owned_quads()
        .filter(|quad| quad.subject == RdfTerm::iri(subject) && quad.predicate == predicate)
        .map(|quad| match quad.object {
            RdfTerm::Iri(iri) => iri,
            other => panic!("<{subject}> <{predicate}> object is not an IRI: {other:?}"),
        })
        .collect();
    assert_eq!(objects.len(), 1, "<{subject}> <{predicate}>: {objects:?}");
    objects[0].clone()
}

/// Walk an RDF list in the carrier, requiring one first/rest per cell.
fn carrier_list(dataset: &RdfDataset, head: &str) -> Vec<String> {
    let head = dataset
        .term_id_by_iri(head)
        .expect("the carrier contains its list head");
    dataset
        .rdf_list_strict(head, purrdf::GraphMatch::Any)
        .expect("one first/rest per cell and a finite nil-terminated list")
        .into_iter()
        .map(|member| match dataset.resolve(member) {
            purrdf::TermRef::Iri(iri) => iri.to_owned(),
            other => panic!("a compiled class-list member is an IRI: {other:?}"),
        })
        .collect()
}

fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

#[test]
fn compiled_named_and_anonymous_lists_are_admitted_with_their_exact_members() {
    let ttl = "\
@prefix ex:    <https://example.org/lists/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
ex:Named logic:oneOf ( ex:b ex:a ) .
ex:Anon logic:equivalentClass [ a logic:Class ; logic:oneOf ( ex:c ex:d ) ] .
ex:Either logic:equivalentClass [ a logic:Class ; logic:unionOf ( ex:Left ex:Right ) ] .
[ a logic:AllDisjointClasses ; logic:members ( ex:Left ex:Right ex:Middle ) ] .
";
    let (program, diagnostics) = parse_logic_str(ttl, None).expect("compile");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let carrier = rooted_in_graph_logic(&[canonical(&program)]);
    let analysis = PreparedClassAnalysis::new(carrier.as_ref()).expect("admission");
    let issues = admission_issues(&analysis);
    assert!(
        issues.is_empty(),
        "compiled lists must be admitted without refusal: {issues:?}"
    );

    let world = &analysis.scan.worlds[GRAPH_LOGIC];
    let admitted = |owner: &str, heads: &BTreeMap<String, String>| -> Vec<String> {
        let head = heads
            .get(owner)
            .unwrap_or_else(|| panic!("<{owner}> owns no selected list"));
        world.lists[head]
            .iter()
            .map(|member| resource_key(member).expect("resource member"))
            .collect()
    };
    // (a) the named enumeration: a set, so sorted.
    assert_eq!(
        admitted(&ex("Named"), &world.one_of),
        vec![ex("a"), ex("b")]
    );
    // (b) the anonymous enumeration under equivalentClass.
    let anon = only_object(
        &carrier,
        &ex("Anon"),
        "https://blackcatinformatics.ca/logic/equivalentClass",
    );
    assert!(anon.starts_with(&format!("{LOGIC}enumeration/")), "{anon}");
    assert_eq!(admitted(&anon, &world.one_of), vec![ex("c"), ex("d")]);
    // (c) the anonymous union under equivalentClass.
    let union = only_object(
        &carrier,
        &ex("Either"),
        "https://blackcatinformatics.ca/logic/equivalentClass",
    );
    assert!(
        union.starts_with(&format!("{LOGIC}class-expression/")),
        "{union}"
    );
    assert_eq!(
        admitted(&union, &world.union_of),
        vec![ex("Left"), ex("Right")]
    );
    // (d) the anonymous AllDisjointClasses axiom: not a selected case-split operand,
    // but its member list is complete in the same carrier.
    let axiom = carrier
        .owned_quads()
        .find(|quad| quad.predicate == format!("{LOGIC}members"))
        .and_then(|quad| resource_key(&quad.subject))
        .expect("members edge");
    assert!(axiom.starts_with(&format!("{LOGIC}axiom/")), "{axiom}");
    let head = only_object(&carrier, &axiom, &format!("{LOGIC}members"));
    assert_eq!(
        carrier_list(&carrier, &head),
        vec![ex("Left"), ex("Middle"), ex("Right")]
    );
}

#[test]
fn a_truncated_compiled_list_is_still_refused() {
    // Admission stays strict: the same carrier with one cell removed is refused.
    let ttl = "\
@prefix ex:    <https://example.org/lists/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
ex:Named logic:oneOf ( ex:a ex:b ) .
";
    let (program, _) = parse_logic_str(ttl, None).expect("compile");
    let carrier = rooted_in_graph_logic(&[canonical(&program)]);
    let mut builder = RdfDatasetBuilder::new();
    let destination = builder.intern_iri(GRAPH_LOGIC).unwrap();
    builder.declare_named_graph(destination).unwrap();
    let mut dropped = false;
    for quad in carrier.owned_quads() {
        if !dropped && quad.predicate == RDF_REST {
            dropped = true;
            continue;
        }
        builder.push_owned_quad(&quad).unwrap();
    }
    assert!(dropped);
    let truncated = builder.freeze().unwrap();
    let analysis = PreparedClassAnalysis::new(truncated.as_ref()).expect("admission");
    assert!(
        admission_issues(&analysis)
            .iter()
            .any(|issue| matches!(issue, RefutationSourceIssue::IncompleteList { .. })),
        "a truncated list must remain an IncompleteList refusal"
    );
}

#[test]
fn corpus_every_slice_module_list_has_complete_canonical_projection() {
    #[derive(Debug, serde::Deserialize)]
    struct Observation {
        schema_version: u32,
        source_catalog_id: String,
        source_modules: std::collections::BTreeMap<String, String>,
        destination_graph: String,
        admission_contract: String,
        selected_world_count: usize,
        list_diagnostics: Vec<gmeow_logic_compile::frontend::Diagnostic>,
        blank_list_fields: Vec<String>,
        invalid_constructor_objects: Vec<String>,
        list_sizes: std::collections::BTreeMap<String, usize>,
        list_errors: Vec<String>,
        admission_defects: Vec<RefutationSourceIssue>,
    }
    // Hydrate the producer's exact compact observation. No source reading,
    // compilation, projection or admission executes in this test process.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = gmeow_action_cache::selection::source_artifacts::load(
        &root,
        "stage-compile-logic",
        "pipeline/logic-list-corpus-observation.json",
    )
    .expect("authenticated producer list observation");
    let observation: Result<Observation, gmeow_errors::RecordedDiag> =
        serde_json::from_slice(&bytes).expect("typed list observation");
    let observation = observation.expect("producer completed list admission");
    assert_eq!(observation.schema_version, 1);
    assert!(!observation.source_catalog_id.is_empty());
    assert!(observation.source_modules.len() > 50);
    assert!(
        observation
            .source_modules
            .values()
            .all(|digest| !digest.is_empty())
    );
    assert_eq!(observation.destination_graph, GRAPH_LOGIC);
    assert!(!observation.admission_contract.is_empty());
    assert!(observation.selected_world_count > 0);
    assert!(observation.list_diagnostics.is_empty(), "{observation:#?}");
    assert!(observation.blank_list_fields.is_empty(), "{observation:#?}");
    assert!(
        observation.invalid_constructor_objects.is_empty(),
        "{observation:#?}"
    );
    assert!(observation.list_errors.is_empty(), "{observation:#?}");
    assert!(observation.admission_defects.is_empty(), "{observation:#?}");
    assert!(!observation.list_sizes.is_empty());
    assert!(
        observation.list_sizes.values().all(|size| *size > 0),
        "{observation:#?}"
    );
}
