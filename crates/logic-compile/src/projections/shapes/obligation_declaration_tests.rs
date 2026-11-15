// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A schema-level reification obligation is metadata, never an instance of its reifier class.

use std::collections::BTreeSet;

use super::*;
use crate::frontend::derive_validation_shapes;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

const PREFIXES: &str = "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
     @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
     @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix g: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix ex: <https://example.org/> .\n";

/// The reifier class `g:Claim` requires `g:rank`; its superclass `g:Record` requires `g:source`.
const SCHEMA: [&str; 3] = [
    "g:Claim a logic:Class ; logic:subClassOf g:Record , [ a logic:Restriction ; \
     logic:onProperty g:rank ; logic:allValuesFrom xsd:integer ] .\n\
     [] a logic:ClosureEntry ; logic:onClass g:Claim ; \
     logic:closureKey \"https://blackcatinformatics.ca/gmeow/rank\" ; \
     logic:closureValue logic:ClosedWorldClosure .\n",
    "g:Record a logic:Class ; logic:subClassOf [ a logic:Restriction ; \
     logic:onProperty g:source ; logic:allValuesFrom xsd:string ] .\n\
     [] a logic:ClosureEntry ; logic:onClass g:Record ; \
     logic:closureKey \"https://blackcatinformatics.ca/gmeow/source\" ; \
     logic:closureValue logic:ClosedWorldClosure .\n",
    "g:ClaimObligation a rdf:Statement , g:Claim ; rdf:subject g:Answer ; \
     rdf:predicate g:record ; rdf:object logic:Thing .\n",
];

/// The class hierarchy a data graph carries for SHACL class-instance resolution.
const HIERARCHY: &str = "g:Claim rdfs:subClassOf g:Record .\n";

const MIN_COUNT: &str = "MinCountConstraintComponent";

fn dataset(body: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(format!("{PREFIXES}{body}").as_bytes(), "text/turtle", None).unwrap()
}

fn derived(schema: &str) -> Vec<ValidationShapeIr> {
    derive_validation_shapes(&dataset(schema)).unwrap()
}

fn lowered(shapes: &[ValidationShapeIr]) -> String {
    let body: Vec<String> = shapes.iter().map(project_validation_shape_shacl).collect();
    format!("{SHACL_PREFIXES}{}", body.join("\n"))
}

/// The `(focus node, result path, constraint component local name)` findings of the lowered shapes.
fn findings(shapes: &[ValidationShapeIr], data: &str) -> BTreeSet<(String, String, String)> {
    let plan = parse_shapes(&lowered(shapes), None).expect("the projection is valid SHACL");
    validate_dataset(&dataset(data), &plan)
        .unwrap()
        .results
        .iter()
        .map(|finding| {
            let component = finding.source_constraint_component.as_str();
            (
                finding.focus_node.to_string(),
                finding
                    .result_path
                    .as_ref()
                    .map(|path| path.to_rdf_term().to_string())
                    .unwrap_or_default(),
                component.rsplit('#').next().unwrap_or(component).to_owned(),
            )
        })
        .collect()
}

fn finding(focus: &str, path: &str) -> (String, String, String) {
    (
        format!("<{focus}>"),
        format!("<https://blackcatinformatics.ca/gmeow/{path}>"),
        MIN_COUNT.to_owned(),
    )
}

/// Protects: a closure-required path on a reifier class still reds every actual reifier that
/// lacks it. Excluding obligation declarations must not release an occurrence, including one
/// that also carries classic reification terms or names a subject outside the authoring ground.
#[test]
fn instance_reifier_missing_a_closure_required_path_is_still_a_violation() {
    let shapes = derived(&SCHEMA.concat());
    let data = format!(
        "{HIERARCHY}\
         ex:ranked a g:Claim ; g:rank 1 ; g:source \"s\" ; \
             rdf:reifies <<( ex:answer g:record ex:first )>> .\n\
         ex:unranked a g:Claim ; g:source \"s\" ; \
             rdf:reifies <<( ex:answer g:record ex:second )>> .\n\
         ex:scaffolded a g:Claim , rdf:Statement ; g:source \"s\" ; rdf:subject g:Answer ; \
             rdf:predicate g:record ; rdf:reifies <<( ex:answer g:record ex:third )>> .\n\
         ex:foreign a g:Claim , rdf:Statement ; g:source \"s\" ; rdf:subject ex:answer ; \
             rdf:predicate g:record .\n"
    );
    assert_eq!(
        findings(&shapes, &data),
        BTreeSet::from([
            finding("https://example.org/foreign", "rank"),
            finding("https://example.org/scaffolded", "rank"),
            finding("https://example.org/unranked", "rank"),
        ])
    );
}

/// Protects: a schema-level obligation declaration is not a focus node of the derived shape of
/// its reifier class or of any superclass, so authoring the obligation needs no dummy values.
/// The same source reds without the exclusion, which is the defect this guards against.
#[test]
fn obligation_declaration_is_not_a_focus_node_of_its_class_shapes() {
    let source = format!("{HIERARCHY}{}", SCHEMA.concat());
    let mut shapes = derived(&source);
    assert_eq!(findings(&shapes, &source), BTreeSet::new());

    for shape in &mut shapes {
        shape.excludes_obligation_declarations = false;
    }
    let declaration = "https://blackcatinformatics.ca/gmeow/ClaimObligation";
    assert_eq!(
        findings(&shapes, &source),
        BTreeSet::from([finding(declaration, "rank"), finding(declaration, "source")]),
        "one missing required path per class the declaration is an instance of"
    );
}

/// Protects: the lowered shape of an obligation's reifier class is a pure function of the
/// authored source, byte-identical whatever order the source states it in.
#[test]
fn obligation_reifier_class_shape_lowers_deterministically() {
    let claim_shape = |schema: String| {
        let shapes = derived(&schema);
        let shape = shapes
            .iter()
            .find(|shape| shape.iri == "https://blackcatinformatics.ca/gmeow/Claim-shape")
            .expect("the reifier class shape is derived");
        project_validation_shape_shacl(shape)
    };
    let forward = claim_shape(SCHEMA.concat());
    let reversed = claim_shape(SCHEMA.iter().rev().copied().collect());
    assert_eq!(forward, reversed);
    assert_eq!(forward, EXPECTED_CLAIM_SHAPE);
}

const EXPECTED_CLAIM_SHAPE: &str = "<https://blackcatinformatics.ca/gmeow/Claim-shape> a sh:NodeShape ;\n    \
     sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT DISTINCT ?this WHERE { \
     ?this <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>/\
     <http://www.w3.org/2000/01/rdf-schema#subClassOf>* \
     <https://blackcatinformatics.ca/gmeow/Claim> . \
     FILTER NOT EXISTS { \
     ?this <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
     <http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement> ; \
     <http://www.w3.org/1999/02/22-rdf-syntax-ns#subject> ?subject ; \
     <http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate> ?predicate . \
     FILTER ( isIRI(?subject) && isIRI(?predicate) && \
     ( STRSTARTS(STR(?subject), 'https://blackcatinformatics.ca/gmeow/') || \
     STRSTARTS(STR(?subject), 'https://blackcatinformatics.ca/math/') || \
     STRSTARTS(STR(?subject), 'https://blackcatinformatics.ca/lang/') || \
     STRSTARTS(STR(?subject), 'https://blackcatinformatics.ca/logic/') ) && \
     ( STRSTARTS(STR(?predicate), 'https://blackcatinformatics.ca/gmeow/') || \
     STRSTARTS(STR(?predicate), 'https://blackcatinformatics.ca/math/') || \
     STRSTARTS(STR(?predicate), 'https://blackcatinformatics.ca/lang/') || \
     STRSTARTS(STR(?predicate), 'https://blackcatinformatics.ca/logic/') ) ) \
     FILTER NOT EXISTS { ?this <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ?statement } \
     } }\"\"\" ] ;\n    \
     sh:property [ sh:path <https://blackcatinformatics.ca/gmeow/rank> ; sh:minCount 1 ; \
     sh:datatype <http://www.w3.org/2001/XMLSchema#integer> ] .\n";
