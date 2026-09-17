// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::frontend::PreparedLogicSource;

fn compile(text: &str) -> Arc<CompiledTheory> {
    let dataset = purrdf::parse_dataset(text.as_bytes(), "application/trig", None).unwrap();
    let source = Arc::new(
        PreparedLogicSource::new(&dataset)
            .unwrap()
            .into_compiled(None)
            .unwrap(),
    );
    assert!(
        source.diagnostics().is_empty(),
        "{:?}",
        source.diagnostics()
    );
    source
}

// Tiny authored input exercises the GMEOW source-selection boundary. It never
// loads or produces a repository corpus, and is not an upstream numeric suite.
fn source(axis: &str, operator: &str, extra: &str, claim: &str) -> String {
    let output = format!("composed{}{}", axis[..1].to_uppercase(), &axis[1..]);
    format!(
        r#"
@prefix ex: <urn:axes:> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
ex:first a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ;
 logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism ;
 logic:sourceEndpoint ex:A ; logic:targetEndpoint ex:B ; logic:{axis} "0.80"^^xsd:decimal .
ex:second a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ;
 logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism ;
 logic:sourceEndpoint ex:B ; logic:targetEndpoint ex:C ; logic:{axis} "0.5"^^xsd:decimal .
ex:result a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ;
 logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism ;
 logic:sourceEndpoint ex:A ; logic:targetEndpoint ex:C {claim} .
ex:chain a logic:CorrespondenceComposition ; logic:compositionFirst ex:first ;
 logic:compositionSecond ex:second ; logic:compositionResult ex:result ; logic:compositionAxisRule ex:rule .
ex:rule a logic:Formula ; logic:antecedent ex:body ; logic:consequent ex:head .
ex:body a logic:Formula ; logic:and ex:selected, ex:left, ex:right, ex:resultBinding, ex:x, ex:y, ex:operation {extra} .
ex:selected a logic:Formula ; logic:relation logic:compositionAxisRule ; logic:argument [logic:termIndex 0; logic:termVariable "c"], [logic:termIndex 1; logic:termIri ex:rule] .
ex:left a logic:Formula ; logic:relation logic:compositionFirst ; logic:argument [logic:termIndex 0; logic:termVariable "c"], [logic:termIndex 1; logic:termVariable "first"] .
ex:right a logic:Formula ; logic:relation logic:compositionSecond ; logic:argument [logic:termIndex 0; logic:termVariable "c"], [logic:termIndex 1; logic:termVariable "second"] .
ex:resultBinding a logic:Formula ; logic:relation logic:compositionResult ; logic:argument [logic:termIndex 0; logic:termVariable "c"], [logic:termIndex 1; logic:termVariable "composite"] .
ex:x a logic:Formula ; logic:relation logic:{axis} ; logic:argument [logic:termIndex 0; logic:termVariable "first"], [logic:termIndex 1; logic:termVariable "x"] .
ex:y a logic:Formula ; logic:relation logic:{axis} ; logic:argument [logic:termIndex 0; logic:termVariable "second"], [logic:termIndex 1; logic:termVariable "y"] .
ex:operation a logic:Formula ; logic:relation logic:rdfNumeric{operator} ; logic:argument [logic:termIndex 0; logic:termVariable "x"], [logic:termIndex 1; logic:termVariable "y"], [logic:termIndex 2; logic:termVariable "z"] .
ex:head a logic:Formula ; logic:relation logic:{output} ; logic:argument [logic:termIndex 0; logic:termVariable "c"], [logic:termIndex 1; logic:termVariable "z"] .
"#
    )
}

const INDEPENDENCE: &str = r#"
ex:independence a logic:Formula ; logic:relation logic:confidenceIndependenceEvidence ;
 logic:argument [logic:termIndex 0; logic:termVariable "c"], [logic:termIndex 1; logic:termVariable "evidence"] .
"#;

fn guard(predicate: &str, subject: &str, object: &str) -> String {
    format!(
        r#"
ex:guard a logic:Formula ; logic:relation {predicate} ;
 logic:argument [logic:termIndex 0; {subject}], [logic:termIndex 1; {object}] .
"#
    )
}

fn source_with_composed_dependency(subject: &str) -> String {
    let mut text = source("weight", "Add", ", ex:guard, ex:peerSelection", "");
    text.push_str(&guard(
        "logic:composedConfidence",
        subject,
        "logic:termVariable \"observed\"",
    ));
    text.push_str(
        r#"
ex:peerSelection a logic:Formula ; logic:relation logic:compositionAxisRule ;
 logic:argument [logic:termIndex 0; logic:termVariable "c"], [logic:termIndex 1; logic:termIri ex:zproducer_rule] .
ex:first logic:confidence 0.2 .
ex:second logic:confidence 0.5 .
ex:chain logic:compositionAxisRule ex:zproducer_rule .
"#,
    );
    // Reuse only the tiny synthetic rule AST, keeping its correspondence members
    // shared. The producing root sorts after the consumer's root deliberately.
    let producer = source("confidence", "Add", "", "");
    let (_, body) = producer.split_once("ex:rule a logic:Formula").unwrap();
    let mut producer = format!("ex:rule a logic:Formula{body}");
    for node in [
        "rule",
        "body",
        "head",
        "selected",
        "left",
        "right",
        "resultBinding",
        "x",
        "y",
        "operation",
    ] {
        producer = producer.replace(&format!("ex:{node}"), &format!("ex:zproducer_{node}"));
    }
    text.push_str(&producer);
    text
}

#[test]
fn source_rules_compute_axes_without_overwriting_claims_or_losing_derivations() {
    let text = source("weight", "Add", "", "; logic:weight \"1.30\"^^xsd:decimal");
    let theory = compile(&text);
    let execution = execute(Arc::clone(&theory)).unwrap();
    assert!(Arc::ptr_eq(execution.source(), &theory));
    let AxisResult::Computed { value, claimed } = &execution.results()["urn:axes:chain"]["weight"]
    else {
        panic!("computed weight");
    };
    assert!(*claimed);
    assert_eq!(value.literal().lexical_form, "1.3");
    let original = theory
        .program()
        .correspondences
        .iter()
        .find(|c| c.iri == "urn:axes:result")
        .unwrap();
    assert_eq!(
        original.weight.as_ref().unwrap().literal().lexical_form,
        "1.30"
    );
    assert_eq!(execution.runs().len(), 1);
    let native = &execution.runs()[0].materialization;
    assert!(native.chase_admission.is_some());
    assert!(
        native
            .quads
            .iter()
            .any(|q| q.predicate == iri("composedWeight") && !q.source_quad_ids.is_empty())
    );
    let report: serde_json::Value = serde_json::from_slice(&execution.report().unwrap()).unwrap();
    assert_eq!(report["runs"][0]["rules"][0], "urn:axes:rule");
    assert!(
        report["runs"][0]["derivations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|quad| quad["predicate"] == iri("composedWeight")
                && !quad["source_quad_ids"].as_array().unwrap().is_empty())
    );
    assert_eq!(
        execute(theory).unwrap().report().unwrap(),
        execution.report().unwrap()
    );
}

#[test]
fn declared_independence_is_owned_by_the_exact_composition() {
    let text = source("confidence", "Multiply", ", ex:independence", "") + INDEPENDENCE;
    let missing = execute(compile(&text)).unwrap();
    assert!(matches!(
        missing.results()["urn:axes:chain"]["confidence"],
        AxisResult::NotEvaluated
    ));
    let unrelated = execute(compile(
        &(text.clone() + "ex:other logic:confidenceIndependenceEvidence ex:review ."),
    ))
    .unwrap();
    assert!(matches!(
        unrelated.results()["urn:axes:chain"]["confidence"],
        AxisResult::NotEvaluated
    ));
    let selected = execute(compile(
        &(text.clone() + "ex:chain logic:confidenceIndependenceEvidence ex:review ."),
    ))
    .unwrap();
    assert!(matches!(
        selected.results()["urn:axes:chain"]["confidence"],
        AxisResult::Computed { .. }
    ));
    let claimed = text + "ex:result logic:confidence 0.4 .";
    assert!(
        execute(compile(&claimed))
            .unwrap_err()
            .message()
            .contains("required premises")
    );
}

#[test]
fn axis_claims_missing_rules_and_cross_context_inputs_fail_closed() {
    let text = source("weight", "Add", "", "; logic:weight 0.1");
    assert!(
        execute(compile(&text))
            .unwrap_err()
            .message()
            .contains("differs")
    );
    let text = source("weight", "Add", "", "");
    let missing = text.replace(
        "logic:compositionAxisRule ex:rule",
        "logic:compositionAxisRule ex:missing",
    );
    assert!(
        execute(compile(&missing))
            .unwrap_err()
            .message()
            .contains("unlowered rule <urn:axes:missing>")
    );
    let contexts = text
        + "ex:first <https://blackcatinformatics.ca/gmeow/accordingTo> ex:alice . ex:second <https://blackcatinformatics.ca/gmeow/accordingTo> ex:bob .";
    assert!(
        execute(compile(&contexts))
            .unwrap_err()
            .message()
            .contains("cross-context")
    );
}

#[test]
fn unrelated_formulas_cannot_supply_axis_outputs_or_escape_the_selected_envelope() {
    let text = source("weight", "Add", "", "");
    let unbound = text.replace("logic:termIri ex:rule", "logic:termIri ex:other");
    assert!(
        execute(compile(&unbound))
            .unwrap_err()
            .message()
            .contains("envelope")
    );
    let absent = text.replace("; logic:compositionAxisRule ex:rule", "");
    let result = execute(compile(&absent)).unwrap();
    assert!(matches!(
        result.results()["urn:axes:chain"]["weight"],
        AxisResult::NotSelected
    ));
    assert!(result.runs().is_empty());
}

#[test]
fn compatible_compositions_share_a_run_and_standpoints_keep_distinct_inputs() {
    let first = source("weight", "Add", "", "");
    let second = first
        .split("ex:rule a logic:Formula")
        .next()
        .unwrap()
        .replace("ex:first", "ex:first2")
        .replace("ex:second", "ex:second2")
        .replace("ex:result", "ex:result2")
        .replace("ex:chain", "ex:chain2")
        .replace("0.80", "0.20");
    let mut text = first + &second;
    for (members, world) in [
        (["first", "second", "result"], "alice"),
        (["first2", "second2", "result2"], "bob"),
    ] {
        for name in members {
            text.push_str(&format!(
                "ex:{name} <https://blackcatinformatics.ca/gmeow/accordingTo> ex:{world} .\n"
            ));
        }
    }
    text.push_str("ex:chain3 a logic:CorrespondenceComposition ; logic:compositionFirst ex:first ; logic:compositionSecond ex:second ; logic:compositionResult ex:result ; logic:compositionAxisRule ex:rule .");
    let execution = execute(compile(&text)).unwrap();
    assert_eq!(execution.results().len(), 3);
    assert_eq!(
        execution.runs().len(),
        2,
        "compatible compositions share one world run"
    );
    for (name, expected) in [("chain", "1.3"), ("chain2", "0.7"), ("chain3", "1.3")] {
        let AxisResult::Computed { value, .. } =
            &execution.results()[&format!("urn:axes:{name}")]["weight"]
        else {
            panic!("computed value");
        };
        assert_eq!(value.literal().lexical_form, expected);
    }
    for run in execution.runs() {
        assert!(
            run.materialization
                .quads
                .iter()
                .all(|quad| quad.graph == run.standpoint)
        );
    }
}

#[test]
fn unavailable_source_guards_are_rejected_even_when_the_source_contains_them() {
    for fact in ["", "ex:evidence ex:approved true ."] {
        let text = source("weight", "Add", ", ex:guard", "")
            + &guard(
                "ex:approved",
                "logic:termIri ex:evidence",
                "logic:termLiteral true",
            )
            + fact;
        let error = execute(compile(&text)).unwrap_err();
        assert!(
            error
                .message()
                .contains("unsupported source guard <urn:axes:approved>"),
            "{error}"
        );
    }
}

#[test]
fn supported_predicates_cannot_read_foreign_members_or_compositions() {
    for (predicate, subject, object, fact) in [
        (
            "logic:weight",
            "logic:termIri ex:outside",
            "logic:termVariable \"outsideWeight\"",
            "ex:outside logic:weight 0.9 .",
        ),
        (
            "logic:evidenceSource",
            "logic:termVariable \"outside\"",
            "logic:termIri ex:evidence",
            "ex:outside logic:evidenceSource ex:evidence .",
        ),
        (
            "logic:compositionAxisRule",
            "logic:termIri ex:otherChain",
            "logic:termIri ex:rule",
            "ex:otherChain logic:compositionAxisRule ex:rule .",
        ),
        (
            "logic:confidenceIndependenceEvidence",
            "logic:termVariable \"first\"",
            "logic:termIri ex:evidence",
            "ex:first logic:confidenceIndependenceEvidence ex:evidence .",
        ),
    ] {
        let text =
            source("weight", "Add", ", ex:guard", "") + &guard(predicate, subject, object) + fact;
        let error = execute(compile(&text)).unwrap_err();
        assert!(
            error
                .message()
                .contains("outside its composition-local input"),
            "{predicate}: {error}"
        );
    }
}

#[test]
fn every_seeded_input_family_remains_available_to_local_guards() {
    for (predicate, subject, object, fact) in [
        (
            "logic:confidence",
            "first",
            "logic:termLiteral 0.25",
            "ex:first logic:confidence 0.25 .",
        ),
        (
            "logic:evidenceStrength",
            "first",
            "logic:termLiteral 0.25",
            "ex:first logic:evidenceStrength 0.25 .",
        ),
        (
            "logic:weight",
            "first",
            "logic:termVariable \"ownWeight\"",
            "",
        ),
        (
            "logic:probability",
            "first",
            "logic:termLiteral 0.25",
            "ex:first logic:probability 0.25 .",
        ),
        (
            "logic:evidenceScale",
            "first",
            "logic:termIri ex:scale",
            "ex:first logic:evidenceScale ex:scale .",
        ),
        (
            "logic:crossChainProbabilityModel",
            "first",
            "logic:termIri ex:model",
            "ex:first logic:crossChainProbabilityModel ex:model .",
        ),
        (
            "logic:evidenceSource",
            "first",
            "logic:termIri ex:evidence",
            "ex:first logic:evidenceSource ex:evidence .",
        ),
        (
            "logic:compositionFirst",
            "c",
            "logic:termVariable \"firstAlias\"",
            "",
        ),
        (
            "logic:compositionSecond",
            "c",
            "logic:termVariable \"secondAlias\"",
            "",
        ),
        (
            "logic:compositionResult",
            "c",
            "logic:termVariable \"resultAlias\"",
            "",
        ),
        (
            "logic:compositionAxisRule",
            "c",
            "logic:termVariable \"policy\"",
            "",
        ),
        (
            "logic:confidenceIndependenceEvidence",
            "c",
            "logic:termIri ex:evidence",
            "ex:chain logic:confidenceIndependenceEvidence ex:evidence .",
        ),
        (
            "logic:probabilityIndependenceEvidence",
            "c",
            "logic:termIri ex:evidence",
            "ex:chain logic:probabilityIndependenceEvidence ex:evidence .",
        ),
    ] {
        let text = source("weight", "Add", ", ex:guard", "")
            + &guard(
                predicate,
                &format!("logic:termVariable \"{subject}\""),
                object,
            )
            + fact;
        let execution = execute(compile(&text)).unwrap();
        let AxisResult::Computed { value, .. } = &execution.results()["urn:axes:chain"]["weight"]
        else {
            panic!("seeded {predicate} guard must be evaluated");
        };
        assert_eq!(value.literal().lexical_form, "1.3", "{predicate}");
    }
}

#[test]
fn explicit_member_bindings_admit_aliases_and_constants_without_foreign_reads() {
    for member in ["logic:termVariable \"alias\"", "logic:termIri ex:first"] {
        let text = source("weight", "Add", ", ex:guard, ex:aliasBinding", "")
            + &guard("logic:weight", member, "logic:termVariable \"aliasWeight\"")
            + &format!(
                r#"
ex:aliasBinding a logic:Formula ; logic:relation logic:compositionFirst ;
 logic:argument [logic:termIndex 0; logic:termVariable "c"], [logic:termIndex 1; {member}] .
"#
            );
        let execution = execute(compile(&text)).unwrap();
        assert!(matches!(
            execution.results()["urn:axes:chain"]["weight"],
            AxisResult::Computed { .. }
        ));
    }
}

#[test]
fn composed_value_guards_use_selected_native_derivations_not_authored_observations() {
    let text = source_with_composed_dependency("logic:termVariable \"c\"")
        + "ex:chain logic:composedConfidence 0.1 .";
    let execution = execute(compile(&text)).unwrap();
    for (axis, expected) in [("confidence", "0.7"), ("weight", "1.3")] {
        let AxisResult::Computed { value, .. } = &execution.results()["urn:axes:chain"][axis]
        else {
            panic!("native {axis} must be computed");
        };
        assert_eq!(value.literal().lexical_form, expected);
    }
    assert_eq!(execution.runs().len(), 1);
    let native = &execution.runs()[0].materialization;
    assert!(native.quads.iter().any(|quad| {
        quad.predicate == iri("composedWeight") && !quad.source_quad_ids.is_empty()
    }));

    let missing_source = text.replace("ex:first logic:confidence 0.2 .", "");
    let execution = execute(compile(&missing_source)).unwrap();
    for axis in ["confidence", "weight"] {
        assert!(matches!(
            execution.results()["urn:axes:chain"][axis],
            AxisResult::NotEvaluated
        ));
    }
    let unselected = text.replace("ex:chain logic:compositionAxisRule ex:zproducer_rule .", "");
    let error = execute(compile(&unselected)).unwrap_err();
    assert!(
        error
            .message()
            .contains("without a selected producing rule")
    );
}

#[test]
fn composed_value_guards_cannot_cross_composition_ownership() {
    for subject in [
        "logic:termVariable \"first\"",
        "logic:termIri ex:otherChain",
    ] {
        let text = source_with_composed_dependency(subject)
            + "ex:otherChain logic:composedConfidence 0.7 .";
        let error = execute(compile(&text)).unwrap_err();
        assert!(
            error
                .message()
                .contains("outside its composition-local input")
        );
    }
}
