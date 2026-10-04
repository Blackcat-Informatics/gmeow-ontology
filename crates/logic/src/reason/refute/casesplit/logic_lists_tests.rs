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
use purrdf::{RdfDatasetBuilder, RdfQuad};
use std::sync::Arc;

const GRAPH_LOGIC: &str = "https://blackcatinformatics.ca/gmeow/graph/logic";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const EX: &str = "https://example.org/lists/";

/// Root every projected quad, reifier and annotation into `graph/logic`, mirroring
/// `crates/pipeline/src/stages/carrier.rs` `rooted_in_graph`.
fn rooted_in_graph_logic(sources: &[Arc<RdfDataset>]) -> Arc<RdfDataset> {
    let graph = RdfTerm::Iri(GRAPH_LOGIC.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    let destination = builder.intern_iri(GRAPH_LOGIC);
    builder.declare_named_graph(destination);
    for src in sources {
        for mut quad in src.owned_quads() {
            quad.graph_name = Some(graph.clone());
            builder.push_owned_quad(&quad);
        }
        for mut reifier in src.owned_reifiers() {
            reifier.graph = Some(graph.clone());
            builder.push_owned_reifier(&reifier);
        }
        for mut annotation in src.owned_annotations() {
            annotation.graph = Some(graph.clone());
            builder.push_owned_annotation(&annotation);
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

fn is_list_defect(issue: &RefutationSourceIssue) -> bool {
    matches!(
        issue,
        RefutationSourceIssue::IncompleteList { .. }
            | RefutationSourceIssue::CyclicList { .. }
            | RefutationSourceIssue::MalformedNil
            | RefutationSourceIssue::ConflictingListField { .. }
            | RefutationSourceIssue::ExpressionMultiplicity { .. }
    )
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
    let mut members = Vec::new();
    let mut cursor = head.to_owned();
    while cursor != RDF_NIL {
        members.push(only_object(dataset, &cursor, RDF_FIRST));
        cursor = only_object(dataset, &cursor, RDF_REST);
        assert!(members.len() < 1_000, "list at <{head}> does not terminate");
    }
    members
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
    let destination = builder.intern_iri(GRAPH_LOGIC);
    builder.declare_named_graph(destination);
    let mut dropped = false;
    for quad in carrier.owned_quads() {
        if !dropped && quad.predicate == RDF_REST {
            dropped = true;
            continue;
        }
        builder.push_owned_quad(&quad);
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

fn module_sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read slices directory") {
        let path = entry.expect("slices entry").path();
        if path.is_dir() {
            module_sources(&path, out);
        } else if path.file_name().is_some_and(|name| name == "module.ttl") {
            out.push(path);
        }
    }
}

#[test]
fn every_slice_module_list_reaches_graph_logic_complete() {
    let slices = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices");
    let mut modules = Vec::new();
    module_sources(&slices, &mut modules);
    modules.sort();
    assert!(
        modules.len() > 50,
        "the slice corpus is present: {modules:?}"
    );

    let mut projections = Vec::new();
    let mut list_diagnostics = Vec::new();
    for module in &modules {
        let (program, diagnostics) = gmeow_logic_compile::frontend::parse_logic_path(module, None)
            .unwrap_or_else(|error| panic!("compile {}: {}", module.display(), error.0));
        list_diagnostics.extend(
            diagnostics
                .into_iter()
                .filter(|diagnostic| {
                    diagnostic.code.contains("LIST")
                        || diagnostic.code.contains("ENUMERATION")
                        || diagnostic.code == "UNSUPPORTED_NESTED_CLASS_EXPRESSION"
                })
                .map(|diagnostic| (module.display().to_string(), diagnostic)),
        );
        projections.push(canonical(&program));
    }
    assert!(
        list_diagnostics.is_empty(),
        "the slice corpus authors no malformed list: {list_diagnostics:#?}"
    );
    let carrier = rooted_in_graph_logic(&projections);
    let blank: Vec<RdfQuad> = carrier
        .owned_quads()
        .filter(|quad| {
            matches!(quad.subject, RdfTerm::BlankNode(_))
                || matches!(quad.object, RdfTerm::BlankNode(_))
        })
        .filter(|quad| {
            gmeow_logic_compile::lists::constructor(&quad.predicate).is_some()
                || matches!(quad.predicate.as_str(), RDF_FIRST | RDF_REST)
        })
        .collect();
    assert!(
        blank.is_empty(),
        "no list constructor or cell may reach graph/logic through a blank node: {blank:?}"
    );
    let analysis = PreparedClassAnalysis::new(carrier.as_ref()).expect("admission");
    let defects: Vec<_> = admission_issues(&analysis)
        .into_iter()
        .filter(is_list_defect)
        .collect();
    assert!(
        defects.is_empty(),
        "graph/logic must carry every compiled list complete: {defects:#?}"
    );
    // Every list-valued constructor edge resolves to a complete list.
    for quad in carrier.owned_quads() {
        if gmeow_logic_compile::lists::constructor(&quad.predicate).is_none() {
            continue;
        }
        let RdfTerm::Iri(head) = &quad.object else {
            panic!("list constructor object is not a cell: {quad:?}");
        };
        assert!(
            !carrier_list(&carrier, head).is_empty(),
            "empty list at {quad:?}"
        );
    }
}
