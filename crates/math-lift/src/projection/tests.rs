// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored inputs exercise real codecs; no repository corpus is read or produced.

use super::*;
use std::sync::Arc;

const PREFIX: &str = "@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix qudt: <http://qudt.org/schema/qudt/> .
@prefix ex: <urn:projection:test:> .";
const LEAF: &str = "ex:n a math:NumberLiteral ; math:literalValue ex:largeNumber . ex:largeNumber a math:Number, math:Integer, math:RationalValue ; math:inNumberSystem math:integers ; math:isExact true ; math:numerator \"123456789012345678901234567890\"^^xsd:integer ; math:denominator 1 .
ex:r a math:ExpressionRendering ; lang:renderedContent ex:n ; lang:renderingKind lang:renderingNotation ; lang:renderingConvention <http://www.w3.org/1998/Math/MathML> ; lang:renderingPreservation logic:ExactPreservation .";

fn data(source: &str) -> Arc<RdfDataset> {
    // gmeow-test-input: synthetic-only
    purrdf::parse_dataset(
        format!("{PREFIX}\n{source}").as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap()
}

fn empty() -> Arc<RdfDataset> {
    data("")
}

#[test]
fn math_projection_emits_real_content_xml_and_mandatory_loss_records() {
    let signed = data(
        "ex:n a math:NumberLiteral ; math:literalValue ex:seven . ex:seven a math:Number, math:Integer, math:RationalValue ; math:inNumberSystem math:integers ; math:isExact true ; math:numerator \"+007\"^^xsd:integer ; math:denominator 1 .",
    );
    let emitted = project(&signed, "urn:projection:test:n", Target::OpenMath, &empty()).unwrap();
    assert!(String::from_utf8_lossy(&emitted.bytes).contains("<OMI>7</OMI>"));
    let source = data(LEAF);
    for (target, subject, root) in [
        (Target::MathMl, "urn:projection:test:r", "math"),
        (Target::OpenMath, "urn:projection:test:n", "OMOBJ"),
    ] {
        let emitted = project(&source, subject, target, &empty()).unwrap();
        let text = std::str::from_utf8(&emitted.bytes).unwrap();
        let parsed = purrdf_lex::xml::Document::parse(text).unwrap();
        assert_eq!(parsed.root_element().tag_name().name(), root);
        assert!(text.contains("123456789012345678901234567890"));
        let ledger = data(&emitted.ledger);
        let mut index = Source::new(&ledger);
        index
            .typed(&RdfTerm::iri(&emitted.record), &math("ProjectionRecord"))
            .unwrap();
        assert_eq!(
            index
                .iri(&RdfTerm::iri(&emitted.record), &math("projectionSource"))
                .unwrap(),
            subject
        );
        assert!(
            !index
                .values(
                    &RdfTerm::iri(&emitted.record),
                    &format!("{LOGIC}unsupportedConstruct")
                )
                .is_empty()
        );
        assert_eq!(
            emitted.bytes,
            project(&source, subject, target, &empty()).unwrap().bytes
        );
    }
}

#[test]
fn math_projection_keeps_argument_order_and_resolves_real_dictionary_symbols() {
    let variable = data(
        "ex:v a math:VariableExpression ; math:variableOccurrence ex:occ . ex:occ a math:VariableOccurrence ; math:declaredVariable ex:declaration . ex:declaration a math:FreeVariableDeclaration .",
    );
    let emitted_variable = project(
        &variable,
        "urn:projection:test:v",
        Target::OpenMath,
        &empty(),
    )
    .unwrap();
    let variable_xml = std::str::from_utf8(&emitted_variable.bytes).unwrap();
    let document = purrdf_lex::xml::Document::parse(variable_xml).unwrap();
    let token = document
        .root_element()
        .children()
        .find(|node| node.is_element())
        .unwrap();
    assert!(purrdf_lex::terminals::is_ncname(
        token.attribute("name").unwrap()
    ));
    assert!(
        emitted_variable
            .ledger
            .contains("urn:projection:test:declaration")
    );
    assert!(
        lift(
            variable_xml.as_bytes(),
            Target::OpenMath,
            "",
            &empty(),
            "urn:lift:/"
        )
        .is_ok()
    );
    let source = data(&format!(
        "{LEAF}\nex:m a math:NumberLiteral ; math:literalValue ex:two . ex:two a math:Number, math:Integer, math:RationalValue ; math:inNumberSystem math:integers ; math:isExact true ; math:numerator 2 ; math:denominator 1 .
ex:add a math:ArithmeticOperation ; skos:exactMatch <http://www.openmath.org/cd/arith1#plus> .
ex:app a math:ApplicationExpression ; math:operator ex:add ; math:argumentSlot ex:b, ex:a .
ex:a a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:n .
ex:b a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:m ."
    ));
    let emitted = project(
        &source,
        "urn:projection:test:app",
        Target::OpenMath,
        &empty(),
    )
    .unwrap();
    let xml = String::from_utf8(emitted.bytes).unwrap();
    assert!(xml.contains("cd=\"arith1\" name=\"plus\""));
    assert!(
        xml.find("123456789012345678901234567890").unwrap() < xml.find("<OMI>2</OMI>").unwrap()
    );
    let lifted = lift(xml.as_bytes(), Target::OpenMath, "", &source, "urn:lift:/").unwrap();
    let index = Source::new(&data(&lifted.turtle));
    assert!(index.rows.values().any(|predicates| {
        predicates
            .get(&math("operator"))
            .is_some_and(|values| values.contains(&RdfTerm::iri("urn:projection:test:add")))
    }));
}

#[test]
fn math_projection_rejects_missing_ambiguous_cyclic_and_unsupported_expressions() {
    for source in [
        "ex:n a math:NumberLiteral .",
        "ex:n a math:NumberLiteral ; math:literalValue 1, 2 .",
        "ex:n a math:BindingExpression .",
        "ex:n a math:NumberLiteral, math:ApplicationExpression ; math:literalValue 1 .",
        "ex:n a math:ApplicationExpression ; math:operator ex:unknown ; math:argumentSlot ex:a . ex:a a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:n .",
        "ex:n a math:NumberLiteral ; math:literalValue \"not a number\"^^xsd:integer .",
        "ex:n a math:NumberLiteral ; math:literalValue \"NaN\"^^xsd:double .",
    ] {
        assert!(
            project(
                &data(source),
                "urn:projection:test:n",
                Target::OpenMath,
                &empty()
            )
            .is_err(),
            "{source}"
        );
    }
    assert!(Target::parse("invented").is_err());
}

#[test]
fn math_projection_requires_the_actual_rendering_contract() {
    for mutation in [
        LEAF.replace("lang:renderingNotation", "lang:renderingSpeech"),
        LEAF.replace("logic:ExactPreservation", "logic:ValidationOnly"),
        LEAF.replace("http://www.w3.org/1998/Math/MathML", "urn:other:notation"),
    ] {
        assert!(
            project(
                &data(&mutation),
                "urn:projection:test:r",
                Target::MathMl,
                &empty()
            )
            .is_err()
        );
    }
}

fn unit_reference() -> Arc<RdfDataset> {
    data(
        "ex:metre a qudt:Unit ; math:hasDimension ex:length ; qudt:hasDimensionVector <http://qudt.org/vocab/dimensionvector/A0E0L1I0M0H0T0D0> . ex:q gmeow:hasReferenceFrame ex:explicitFrame .",
    )
}

#[test]
fn math_projection_qudt_checks_classification_dimensions_and_typed_values() {
    let source = data(
        "ex:q a math:Quantity ; math:quantityValue \"2.500\"^^xsd:decimal ; gmeow:unit ex:metre ; math:hasDimension ex:length .",
    );
    let reference = unit_reference();
    let emitted = project(&source, "urn:projection:test:q", Target::Qudt, &reference).unwrap();
    let target = data(std::str::from_utf8(&emitted.bytes).unwrap());
    let mut index = Source::new(&target);
    assert_eq!(
        index
            .literal(
                &RdfTerm::iri("urn:projection:test:q"),
                &format!("{QUDT}numericValue")
            )
            .unwrap()
            .lexical_form,
        "2.500"
    );
    assert!(project(&source, "urn:projection:test:q", Target::Qudt, &empty()).is_err());
    let bad = data(
        "ex:q a math:Quantity ; math:quantityValue 2 ; gmeow:unit ex:metre ; math:hasDimension ex:mass .",
    );
    assert!(project(&bad, "urn:projection:test:q", Target::Qudt, &reference).is_err());
    let lifted = lift(
        &emitted.bytes,
        Target::Qudt,
        "urn:projection:test:q",
        &reference,
        "urn:lift:/",
    )
    .unwrap();
    assert!(lifted.turtle.contains("2.500"));
    let contradictory = String::from_utf8(emitted.bytes)
        .unwrap()
        .replace("A0E0L1I0M0H0T0D0", "A0E0L0I0M1H0T0D0");
    assert!(
        lift(
            contradictory.as_bytes(),
            Target::Qudt,
            "urn:projection:test:q",
            &reference,
            "urn:lift:/"
        )
        .is_err()
    );
}

#[test]
fn math_projection_cube_preserves_exact_rational_cells_without_decimal_rounding() {
    let source_text = "ex:matrix a math:DatasetMatrix ; math:hasEntry ex:entry .
ex:entry a math:MatrixEntry ; math:atRow 0 ; math:atColumn 3 ; math:entryValue ex:value .
ex:value a math:RationalValue ; math:numerator 1 ; math:denominator 3 .";
    let source = data(source_text);
    let zero = data(&source_text.replace(
        "math:denominator 3",
        "math:denominator \" -0 \"^^xsd:integer",
    ));
    assert!(
        project(
            &zero,
            "urn:projection:test:matrix",
            Target::DataCube,
            &empty()
        )
        .is_err()
    );
    let emitted = project(
        &source,
        "urn:projection:test:matrix",
        Target::DataCube,
        &empty(),
    )
    .unwrap();
    let bytes = String::from_utf8(emitted.bytes).unwrap();
    assert!(bytes.contains("DataStructureDefinition"));
    assert!(!bytes.contains("0.333"));
    let lifted = lift(
        bytes.as_bytes(),
        Target::DataCube,
        "urn:projection:test:matrix",
        &empty(),
        "urn:lift:/",
    )
    .unwrap();
    let mut index = Source::new(&data(&lifted.turtle));
    let rationals: Vec<_> = index.rows.keys().cloned().collect();
    assert!(
        rationals
            .into_iter()
            .any(|subject| index.values(&subject, &math("denominator"))
                == vec![RdfTerm::literal(RdfLiteral::typed(
                    "3",
                    format!("{}integer", "http://www.w3.org/2001/XMLSchema#")
                ))])
    );
    assert!(
        project(
            &data("ex:matrix a math:DatasetMatrix ."),
            "urn:projection:test:matrix",
            Target::DataCube,
            &empty()
        )
        .is_err()
    );
}

#[test]
fn math_projection_stato_requires_an_existing_model_expression_anchor() {
    let source = data("ex:model a math:StatisticalModel ; math:modelFormula ex:formula .
ex:formula a math:ModelFormula . math:StatisticalModel skos:closeMatch <http://purl.obolibrary.org/obo/STATO_0000107> .");
    let emitted = project(
        &source,
        "urn:projection:test:model",
        Target::Stato,
        &empty(),
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&emitted.bytes).contains("STATO_0000107"));
    let mut ledger = Source::new(&data(&emitted.ledger));
    let losses = ledger.values(
        &RdfTerm::iri(&emitted.record),
        &format!("{LOGIC}unsupportedConstruct"),
    );
    assert!(losses.contains(&RdfTerm::iri(
        "http://www.w3.org/2004/02/skos/core#closeMatch"
    )));
    assert!(losses.contains(&RdfTerm::iri(math("StatisticalModel"))));
    assert!(
        lift(
            &emitted.bytes,
            Target::Stato,
            "urn:projection:test:model",
            &empty(),
            "urn:lift:/"
        )
        .is_err()
    );
    assert!(
        lift(
            &emitted.bytes,
            Target::Stato,
            "urn:projection:test:model",
            &source,
            "urn:lift:/"
        )
        .is_ok()
    );
}

#[test]
fn math_projection_external_content_refuses_unresolved_and_foreign_constructs() {
    let invalid_utf8 = lift(&[0xff], Target::MathMl, "", &empty(), "urn:lift:/")
        .err()
        .unwrap();
    assert!(invalid_utf8.downcast_ref::<std::str::Utf8Error>().is_some());
    assert_eq!(
        invalid_utf8.inner().failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/math/UnliftableIngest")
    );
    for (target, text) in [
        (
            Target::MathMl,
            "<math xmlns='http://www.w3.org/1998/Math/MathML'><bind/></math>",
        ),
        (
            Target::MathMl,
            "<math xmlns='urn:wrong'><cn type='integer'>1</cn></math>",
        ),
        (
            Target::MathMl,
            "<math xmlns='http://www.w3.org/1998/Math/MathML'><cn type='real'>no</cn></math>",
        ),
        (
            Target::OpenMath,
            "<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMS cd='unknown' name='f'/></OMOBJ>",
        ),
        (
            Target::OpenMath,
            "<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMBIND/></OMOBJ>",
        ),
    ] {
        assert!(
            lift(text.as_bytes(), target, "", &empty(), "urn:lift:/").is_err(),
            "{text}"
        );
    }
}

#[test]
fn math_projection_presentation_lift_records_notation_without_a_fabricated_ast() {
    let lifted = lift(b"<math xmlns='http://www.w3.org/1998/Math/MathML'><mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow></math>", Target::MathMl, "", &empty(), "urn:lift:/").unwrap();
    assert!(lifted.turtle.contains("ExpressionRendering"));
    assert!(lifted.turtle.contains("presentation carries notation only"));
    assert!(!lifted.turtle.contains("ApplicationExpression"));
    assert!(!lifted.turtle.contains("NumberLiteral"));
}

#[test]
fn math_projection_symbol_lifts_keep_native_ownership_and_variable_identity() {
    let reference = data(
        "ex:add a math:ArithmeticOperation ; skos:exactMatch <http://www.openmath.org/cd/arith1#plus> .",
    );
    let input = b"<math xmlns='http://www.w3.org/1998/Math/MathML'><apply><csymbol definitionURL='urn:projection:test:add'>+</csymbol><ci>x</ci><ci> x </ci></apply></math>";
    let lifted = lift(input, Target::MathMl, "", &reference, "urn:lift:/").unwrap();
    let mut index = Source::new(&data(&lifted.turtle));
    let subjects: Vec<_> = index.rows.keys().cloned().collect();
    let declarations = subjects
        .iter()
        .filter(|subject| {
            index
                .values(subject, RDF_TYPE)
                .contains(&RdfTerm::iri(math("FreeVariableDeclaration")))
        })
        .count();
    assert_eq!(
        declarations, 1,
        "token whitespace must not split one free variable"
    );
    let external =
        b"<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMS cd='arith1' name='plus'/></OMOBJ>";
    let reversed = data("<http://www.openmath.org/cd/arith1#plus> skos:exactMatch ex:add .");
    assert!(lift(external, Target::OpenMath, "", &reversed, "urn:lift:/").is_err());
    let close = data(
        "ex:add a math:ArithmeticOperation ; skos:closeMatch <http://www.openmath.org/cd/arith1#plus> .",
    );
    assert!(
        lift(external, Target::OpenMath, "", &close, "urn:lift:/").is_err(),
        "approximate output alignment must not authorize exact symbol reconstruction"
    );
    assert!(
        lift(input, Target::MathMl, "", &close, "urn:lift:/").is_ok(),
        "an explicitly typed native identity needs no reverse external inference"
    );
}

#[test]
fn math_projection_requests_are_explicit_atomic_and_source_bound() {
    let source = data(&format!(
        "{LEAF}\nex:request a math:ProjectionRequest ; math:projectionSource ex:r ; math:projectionTargetName \"mathml\" ."
    ));
    let files = project_requests(&source, &empty(), &empty()).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains_key("generated/projections/math/loss-ledger.ttl"));
    assert!(files.keys().any(|path| path.ends_with(".mml")));
    let malformed = data(&format!(
        "{LEAF}\nex:request a math:ProjectionRequest ; math:projectionSource ex:r ; math:projectionTargetName \"unknown\" ."
    ));
    assert!(project_requests(&malformed, &empty(), &empty()).is_err());
    assert!(
        project_requests(&empty(), &empty(), &empty())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn math_projection_competencies_inspect_actual_correspondences_and_sensitive_losses() {
    use purrdf::{SparqlEngine, SparqlRequest, SparqlResult};
    let source = data(&format!(
        "{LEAF}\nex:n logic:confidence 0.8 ; math:distributionParameterization ex:parameters . ex:parameters math:parameterValue 4 ."
    ));
    let emission = project(&source, "urn:projection:test:r", Target::MathMl, &empty()).unwrap();
    let ledger = data(&emission.ledger);
    let engine = purrdf::sparql::NativeSparqlEngine::new();
    for (name, query) in COMPETENCY_QUERIES {
        let result = engine
            .query(
                &ledger,
                SparqlRequest {
                    query,
                    base_iri: None,
                    substitutions: &[],
                },
            )
            .unwrap();
        let SparqlResult::Solutions {
            variables, rows, ..
        } = result
        else {
            panic!("competency must return solutions: {name}");
        };
        assert!(!rows.is_empty(), "actual emission must answer {name}");
        if *name == "projection-preservation.rq" {
            assert_eq!(rows.len(), 1);
            assert!(variables.iter().any(|name| name == "correspondence"));
        }
        if *name == "projection-sensitive-losses.rq" {
            assert_eq!(rows.len(), 2);
        }
    }
    assert!(!emission.ledger.contains("projectsConfidenceAsProbability"));
    assert!(!emission.ledger.contains("ExactPreservation"));
}

#[test]
fn math_projection_distinguishes_cycles_from_shared_expression_subtrees() {
    let prefix = "ex:add a math:ArithmeticOperation ; skos:exactMatch <http://www.openmath.org/cd/arith1#plus> . ex:app a math:ApplicationExpression ; math:operator ex:add ; math:argumentSlot ex:a, ex:b . ex:a a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:n . ex:b a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:n .";
    let shared = data(&format!("{LEAF}\n{prefix}"));
    let canonical_index = data(&format!(
        "{LEAF}\n{}",
        prefix.replace("math:slotIndex 0", "math:slotIndex \" -0 \"^^xsd:integer")
    ));
    assert!(
        project(
            &canonical_index,
            "urn:projection:test:app",
            Target::OpenMath,
            &empty()
        )
        .is_ok()
    );
    let emission = project(
        &shared,
        "urn:projection:test:app",
        Target::OpenMath,
        &empty(),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&emission.bytes)
            .matches("<OMI>")
            .count(),
        2
    );
    let cyclic = data(&prefix.replace("math:slotExpression ex:n", "math:slotExpression ex:app"));
    let error = project(
        &cyclic,
        "urn:projection:test:app",
        Target::OpenMath,
        &empty(),
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("cyclic"));
}

#[test]
fn math_projection_qudt_lift_never_invents_a_reference_frame() {
    let source = data(
        "ex:q a math:Quantity ; math:quantityValue 2 ; gmeow:unit ex:metre ; math:hasDimension ex:length .",
    );
    let reference = data(
        "ex:metre a qudt:Unit ; math:hasDimension ex:length ; qudt:hasDimensionVector <http://qudt.org/vocab/dimensionvector/A0E0L1I0M0H0T0D0> .",
    );
    let emitted = project(&source, "urn:projection:test:q", Target::Qudt, &reference).unwrap();
    assert!(
        lift(
            &emitted.bytes,
            Target::Qudt,
            "urn:projection:test:q",
            &reference,
            "urn:lift:/"
        )
        .is_err()
    );
}

#[test]
fn math_projection_stato_preserves_the_selected_statistical_profile() {
    for (class, anchor, body, target) in [
        (
            "Statistic",
            "definingExpression",
            "ex:anchor a math:NumberLiteral ; math:literalValue ex:one . ex:one a math:Number, math:Integer, math:RationalValue ; math:inNumberSystem math:integers ; math:isExact true ; math:numerator 1 ; math:denominator 1 .",
            "STATO_0000039",
        ),
        (
            "HypothesisTest",
            "testStatistic",
            "ex:anchor a math:Statistic ; math:definingExpression ex:body . ex:body a math:NumberLiteral ; math:literalValue ex:one . ex:one a math:Number, math:Integer, math:RationalValue ; math:inNumberSystem math:integers ; math:isExact true ; math:numerator 1 ; math:denominator 1 .",
            "STATO_0000038",
        ),
    ] {
        let source = data(&format!(
            "ex:root a math:{class} ; math:{anchor} ex:anchor . {body} math:{class} skos:exactMatch <http://purl.obolibrary.org/obo/{target}> ."
        ));
        let emitted =
            project(&source, "urn:projection:test:root", Target::Stato, &empty()).unwrap();
        let lifted = lift(
            &emitted.bytes,
            Target::Stato,
            "urn:projection:test:root",
            &source,
            "urn:lift:/",
        )
        .unwrap();
        let mut index = Source::new(&data(&lifted.turtle));
        let subjects: Vec<_> = index.rows.keys().cloned().collect();
        assert!(subjects.iter().any(|subject| {
            index
                .values(subject, RDF_TYPE)
                .contains(&RdfTerm::iri(math(class)))
        }));
        assert!(!lifted.turtle.contains("StatisticalModel"));
    }
}

#[test]
fn math_projection_rational_tokens_refuse_hidden_markup_and_qualifiers() {
    assert!(lift(b"<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMV name='urn:illegal:name'/></OMOBJ>", Target::OpenMath, "", &empty(), "urn:lift:/").is_err());
    for (target, text) in [
        (
            Target::OpenMath,
            "<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMI>+1</OMI></OMOBJ>",
        ),
        (
            Target::MathMl,
            "<math xmlns='http://www.w3.org/1998/Math/MathML'><cn type='rational'>1<sep><ci>x</ci></sep>3</cn></math>",
        ),
        (
            Target::OpenMath,
            "<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMA><OMS cd='nums1' name='rational'/><OMI>1<OMI>9</OMI></OMI><OMI>3</OMI></OMA></OMOBJ>",
        ),
        (
            Target::OpenMath,
            "<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMA><OMS cd='nums1' name='rational'/><OMI type='other'>1</OMI><OMI>3</OMI></OMA></OMOBJ>",
        ),
    ] {
        assert!(lift(text.as_bytes(), target, "", &empty(), "urn:lift:/").is_err());
    }
    let lifted = lift(b"<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMI>1<!-- split token -->2</OMI></OMOBJ>", Target::OpenMath, "", &empty(), "urn:lift:/").unwrap();
    let index = Source::new(&data(&lifted.turtle));
    assert!(
        index.rows.values().any(
            |row| row.get(&math("numerator")).is_some_and(|values| values
                == &vec![RdfTerm::literal(RdfLiteral::typed(
                    "12",
                    "http://www.w3.org/2001/XMLSchema#integer"
                ))])
        )
    );
}

#[test]
fn math_projection_uses_situated_number_objects_without_guessing_from_labels() {
    let source = data(
        "ex:n a math:NumberLiteral ; math:literalValue ex:pi . ex:pi a math:RealNumber ; math:inNumberSystem math:realNumbers ; math:isExact true ; <http://www.w3.org/2000/01/rdf-schema#label> \"3.14\" . ex:r a math:ExpressionRendering ; lang:renderedContent ex:n ; lang:renderingKind lang:renderingNotation ; lang:renderingConvention <http://www.w3.org/1998/Math/MathML> ; lang:renderingPreservation logic:ExactPreservation .",
    );
    let emitted = project(&source, "urn:projection:test:r", Target::MathMl, &empty()).unwrap();
    let xml = std::str::from_utf8(&emitted.bytes).unwrap();
    assert!(xml.contains("definitionURL=\"urn:projection:test:pi\""));
    assert!(!xml.contains("3.14"));
    assert!(project(&source, "urn:projection:test:n", Target::OpenMath, &empty()).is_err());
    let lifted = lift(&emitted.bytes, Target::MathMl, "", &source, "urn:lift:/").unwrap();
    let index = Source::new(&data(&lifted.turtle));
    assert!(index.rows.values().any(|row| {
        row.get(&math("literalValue"))
            .is_some_and(|values| values == &vec![RdfTerm::iri("urn:projection:test:pi")])
    }));
    for mutation in [
        LEAF.replace("math:isExact true", "math:isExact false"),
        LEAF.replace("math:inNumberSystem math:integers ;", ""),
        LEAF.replace("math:denominator 1", "math:denominator 0"),
        LEAF.replace("math:RationalValue", "math:ApproximateValue"),
    ] {
        assert!(
            project(
                &data(&mutation),
                "urn:projection:test:n",
                Target::OpenMath,
                &empty()
            )
            .is_err()
        );
    }
    let lifted = lift(
        b"<math xmlns='http://www.w3.org/1998/Math/MathML'><cn type='real'>-12.50</cn></math>",
        Target::MathMl,
        "",
        &empty(),
        "urn:lift:/",
    )
    .unwrap();
    let mut index = Source::new(&data(&lifted.turtle));
    let number = index
        .rows
        .iter()
        .find_map(|(subject, row)| {
            row.get(RDF_TYPE)
                .is_some_and(|types| types.contains(&RdfTerm::iri(math("Number"))))
                .then_some(subject.clone())
        })
        .unwrap();
    assert_eq!(
        index.iri(&number, &math("inNumberSystem")).unwrap(),
        math("realNumbers")
    );
    assert_eq!(
        index
            .literal(&number, &math("isExact"))
            .unwrap()
            .lexical_form,
        "true"
    );
    let numerator = index
        .literal(&number, &math("numerator"))
        .unwrap()
        .lexical_form
        .parse::<i64>()
        .unwrap();
    let denominator = index
        .literal(&number, &math("denominator"))
        .unwrap()
        .lexical_form
        .parse::<i64>()
        .unwrap();
    assert_eq!(numerator * 2, denominator * -25);
    assert!(
        index
            .rows
            .values()
            .filter_map(|row| row.get(&math("literalValue")))
            .flatten()
            .all(|value| !matches!(value, RdfTerm::Literal(_)))
    );
}

#[test]
fn math_projection_lift_requires_native_symbol_ownership_and_normalizes_variables() {
    let xml =
        b"<OMOBJ xmlns='http://www.openmath.org/OpenMath'><OMS cd='arith1' name='plus'/></OMOBJ>";
    let reversed = data("<http://www.openmath.org/cd/arith1#plus> skos:exactMatch ex:add .");
    assert!(lift(xml, Target::OpenMath, "", &reversed, "urn:lift:/").is_err());
    let close = data(
        "ex:add a math:ArithmeticOperation ; skos:closeMatch <http://www.openmath.org/cd/arith1#plus> .",
    );
    assert!(lift(xml, Target::OpenMath, "", &close, "urn:lift:/").is_err());
    let exact = data(
        "ex:add a math:ArithmeticOperation ; skos:exactMatch <http://www.openmath.org/cd/arith1#plus> .",
    );
    assert!(lift(xml, Target::OpenMath, "", &exact, "urn:lift:/").is_ok());
    let lifted = lift(b"<math xmlns='http://www.w3.org/1998/Math/MathML'><apply><csymbol definitionURL='urn:projection:test:add'/><ci>x</ci><ci> x </ci></apply></math>", Target::MathMl, "", &exact, "urn:lift:/").unwrap();
    let index = Source::new(&data(&lifted.turtle));
    assert_eq!(
        index
            .rows
            .values()
            .filter(|row| {
                row.get(RDF_TYPE).is_some_and(|types| {
                    types.contains(&RdfTerm::iri(math("FreeVariableDeclaration")))
                })
            })
            .count(),
        1
    );
}
