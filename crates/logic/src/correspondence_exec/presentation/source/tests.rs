// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::frontend::{FormulaDisposition, PreparedLogicSource, SourceNode};

const SOURCE: &str =
    include_str!("../../../../../logic-compile/tests/fixtures/presentation-merge.ttl");
const EX: &str = "urn:presentation-example:";

fn compile(source: &str) -> Arc<CompiledTheory> {
    let dataset = purrdf::parse_dataset(source.as_bytes(), "application/trig", None).unwrap();
    Arc::new(
        PreparedLogicSource::new(&dataset)
            .unwrap()
            .into_compiled(None)
            .unwrap(),
    )
}

#[test]
fn authored_merge_reuses_original_formulas_and_complete_native_evidence() {
    let source = format!(
        "{SOURCE}\nex:empty {{}}\nex:claim <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> <<(ex:a ex:p ex:b)>> ; <http://www.w3.org/ns/prov#wasDerivedFrom> ex:document ."
    );
    let theory = compile(&source);
    assert!(theory.diagnostics().iter().all(|diagnostic| diagnostic.severity != gmeow_logic_compile::frontend::Severity::Error), "{:?}", theory.diagnostics());
    let root = SourceNode {
        term: theory
            .source()
            .dataset()
            .term_id_by_iri(&format!("{EX}body"))
            .unwrap(),
        graph: None,
    };
    assert!(
        theory
            .formula_lowerings()
            .iter()
            .any(|entry| entry.source == root
                && entry.disposition == FormulaDisposition::ReadForOwner)
    );
    assert!(
        theory.program().formulas.is_empty(),
        "presentation sentences must not become global assertions"
    );
    let original = theory.presentation_formulas().get(&root).unwrap();
    let batch = execute(Arc::clone(&theory), PresentationLimits::default()).unwrap();
    let merge = &batch.merges()[&format!("{EX}merge")];
    assert!(Arc::ptr_eq(batch.source(), &theory));
    assert_eq!(merge.output().symbols().len(), 4);
    assert_eq!(merge.output().sentences().len(), 3);
    for sentence in merge.output().sentences() {
        assert!(Arc::ptr_eq(sentence.body.formula(), original));
        let EvidenceDataset::Source(owner) = sentence.evidence.provenance() else {
            panic!("source evidence must retain its compiler publication");
        };
        assert!(Arc::ptr_eq(owner, &theory));
        assert!(
            sentence
                .evidence
                .provenance()
                .same_publication(sentence.evidence.complement())
        );
        assert!(std::ptr::eq(
            sentence.evidence.provenance().dataset(),
            theory.source().dataset()
        ));
    }
    assert!(
        merge
            .output()
            .sentences()
            .iter()
            .any(|sentence| sentence.sign == SentenceSign::Negative
                && sentence.kind == SentenceKind::Caveat)
    );
    assert!(merge.output().sentences().iter().any(|sentence| {
        sentence
            .evidence
            .loss()
            .unsupported_constructs
            .contains(&format!("{EX}unprojectedConstraint"))
    }));
    let identity = merge
        .factor(merge.left_injection(), merge.right_injection())
        .unwrap();
    assert!(identity.agrees_with(&PresentationMap::identity(Arc::clone(merge.output()))));
    let report: serde_json::Value = serde_json::from_slice(&batch.report().unwrap()).unwrap();
    assert_eq!(report["bodies"].as_object().unwrap().len(), 1);
    assert_eq!(report["evidence"].as_object().unwrap().len(), 3);
    assert_eq!(
        report["merges"][format!("{EX}merge")]["sentences"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn source_merge_refuses_missing_untyped_and_graph_excluded_selections() {
    let cases = [
        SOURCE.replace(
            "logic:mergeRight ex:rightMap",
            "logic:mergeRight ex:missing",
        ),
        SOURCE.replace(
            "logic:mergeRight ex:rightMap",
            "logic:mergeRight \"not a map\"",
        ),
        SOURCE.replace("ex:merge a logic:PresentationMerge ;", "ex:merge"),
        SOURCE.replace("ex:apex a logic:FinitePresentation ;", "ex:apex"),
        SOURCE.replace(
            "logic:generatorBinding ex:pImage, ex:xImage",
            "logic:generatorBinding ex:pImage",
        ),
        SOURCE.replace(
            "logic:sentenceBinding ex:pBinding, ex:xBinding",
            "logic:sentenceBinding ex:pBinding",
        ),
        SOURCE.replace(
            "logic:sentenceContext ex:context",
            "logic:sentenceContext ex:missing",
        ),
        SOURCE.replace("ex:body a logic:Formula ;", "ex:body"),
        SOURCE.replace("logic:termIri ex:x", "logic:termVariable \"free\""),
        SOURCE.replace(
            "logic:presentationModality \"none\"",
            "logic:presentationModality \"unknown-mode\"",
        ),
        SOURCE.replace(
            "logic:individualSymbolRole true",
            "logic:individualSymbolRole false",
        ),
        SOURCE.replace(
            "logic:preservationKind logic:ExactPreservation",
            "logic:preservationKind logic:ValidationOnly",
        ),
        format!("{SOURCE}\nex:program logic:hasPresentationMerge ex:missing ."),
        format!(
            "{SOURCE}\nex:graph {{ ex:hidden a logic:PresentationMerge ; logic:mergeLeft ex:leftMap ; logic:mergeRight ex:rightMap . }}"
        ),
    ];
    for (index, source) in cases.iter().enumerate() {
        assert!(
            execute(compile(source), PresentationLimits::default()).is_err(),
            "hostile source {index} was admitted"
        );
    }
}

#[test]
fn source_merge_never_discards_additional_execution_scope() {
    for record in [
        "merge", "leftMap", "left", "context", "pSymbol", "positive", "body", "pBinding", "pImage",
    ] {
        for property in [
            "logic:standpoint",
            "logic:accordingTo",
            "logic:time",
            "logic:inModule",
            "logic:imports",
            "logic:world",
            "logic:path",
            "logic:modality",
            "logic:confidence",
            "<https://blackcatinformatics.ca/gmeow/accordingTo>",
        ] {
            let source = format!("{SOURCE}\nex:{record} {property} ex:extraScope .");
            let failure = execute(compile(&source), PresentationLimits::default()).unwrap_err();
            assert!(
                failure
                    .message()
                    .contains("explicit contextual or sorted presentation lowering"),
                "scope on {record} through {property} was not explicitly refused: {failure}"
            );
        }
    }
    // A graph-local annotation outside the selected grammar is retained, not
    // silently unioned into the default graph's execution context.
    let source = format!("{SOURCE}\nex:other {{ ex:body logic:standpoint ex:observer . }}");
    execute(compile(&source), PresentationLimits::default()).unwrap();
}

#[test]
fn source_merge_checks_nested_syntax_before_using_a_lowered_formula() {
    let source = SOURCE.replace(
        "logic:termIri ex:x",
        "logic:termIri ex:x ; logic:variableSort ex:Person",
    );
    let failure = execute(compile(&source), PresentationLimits::default()).unwrap_err();
    assert!(failure.message().contains("variableSort"), "{failure}");

    let body = "ex:body a logic:Formula ; logic:relation ex:p ;\n    logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termIri ex:x ] .";
    let sorted =
        "ex:body a logic:Formula ; logic:forall ex:atom ; logic:quantifiedVariable ex:var .
        ex:var logic:termIndex 0 ; logic:termVariable \"x\" ; logic:variableSort ex:Person .
        ex:atom a logic:Formula ; logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] .";
    let source = SOURCE.replace(body, sorted).replace(
        "logic:sentenceBinding ex:pBinding, ex:xBinding",
        "logic:sentenceBinding ex:pBinding",
    );
    assert_ne!(source, SOURCE);
    let failure = execute(compile(&source), PresentationLimits::default()).unwrap_err();
    assert!(failure.message().contains("variableSort"), "{failure}");
    // The corresponding unsorted closed formula is admitted. This refusal is
    // about lost sort semantics, not a malformed binding or open formula.
    execute(
        compile(&source.replace(" ; logic:variableSort ex:Person", "")),
        PresentationLimits::default(),
    )
    .unwrap();

    let modal = format!(
        "ex:body a logic:Formula ; logic:necessarily ex:atom ; logic:overAccessibility logic:epistemicallyPossible .\n{}",
        body.replace("ex:body", "ex:atom")
    );
    let source = SOURCE.replace(body, &modal);
    let failure = execute(compile(&source), PresentationLimits::default()).unwrap_err();
    assert!(
        failure
            .message()
            .contains("presentation-world-bound lowering"),
        "{failure}"
    );
}

#[test]
fn evidence_scope_is_preserved_as_native_metadata() {
    let source = format!(
        "{SOURCE}\nex:positiveEvidence logic:standpoint ex:observer ; logic:time ex:interval ; logic:confidence 0.6 ."
    );
    let theory = compile(&source);
    let batch = execute(Arc::clone(&theory), PresentationLimits::default()).unwrap();
    let evidence = &batch.evidence[&format!("{EX}positiveEvidence")];
    assert!(std::ptr::eq(
        evidence.provenance().dataset(),
        theory.source().dataset()
    ));
    assert!(
        evidence
            .provenance()
            .same_publication(evidence.complement())
    );
}

#[test]
fn scoped_syntax_statements_cannot_hide_behind_native_reification() {
    let reifies = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies>";
    for annotation in [
        format!("ex:claim {reifies} <<(ex:body logic:relation ex:p)>> ; logic:standpoint ex:observer ."),
        format!("ex:claim {reifies} <<(ex:body logic:relation ex:p)>> ; logic:provenance ex:document .
            ex:metadata {reifies} <<(ex:claim logic:provenance ex:document)>> ; logic:standpoint ex:observer ."),
    ] {
        let source = format!("{SOURCE}\n{annotation}");
        let failure = execute(compile(&source), PresentationLimits::default()).unwrap_err();
        assert!(failure.message().contains("standpoint"), "{failure}");
    }
    for annotation in [
        format!(
            "ex:claim {reifies} <<(ex:body logic:relation ex:unasserted)>> ; logic:standpoint ex:observer ."
        ),
        format!(
            "ex:other {{ ex:claim {reifies} <<(ex:body logic:relation ex:p)>> ; logic:standpoint ex:observer . }}"
        ),
        format!(
            "ex:claim {reifies} <<(ex:body logic:relation ex:p)>> ; logic:provenance ex:document ."
        ),
    ] {
        let source = format!("{SOURCE}\n{annotation}");
        let theory = compile(&source);
        let batch = execute(Arc::clone(&theory), PresentationLimits::default()).unwrap();
        assert!(Arc::ptr_eq(batch.source(), &theory));
    }
}

#[test]
fn source_merge_requires_exact_shared_apex_and_bounded_work() {
    let mut source = SOURCE.to_owned();
    source.push_str("\nex:other a logic:FinitePresentation ; logic:presentationContract ex:contract ; logic:presentationContext ex:context ; logic:presentationSymbol ex:pSymbol, ex:xSymbol ; logic:presentationSentence ex:base .");
    source = source.replace(
        "ex:rightMap a logic:PresentationMap ; logic:mapSource ex:apex",
        "ex:rightMap a logic:PresentationMap ; logic:mapSource ex:other",
    );
    let error = execute(compile(&source), PresentationLimits::default()).unwrap_err();
    assert!(error.to_string().contains("exact common apex"), "{error}");
    let theory = compile(SOURCE);
    assert!(
        execute(
            Arc::clone(&theory),
            PresentationLimits {
                max_binding_slots: 10,
                ..PresentationLimits::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        execute(theory, PresentationLimits::default())
            .unwrap()
            .merges()
            .len(),
        1
    );
}

#[test]
fn source_merge_report_is_stable_across_independent_compiler_publications() {
    let first = execute(compile(SOURCE), PresentationLimits::default()).unwrap();
    let second = execute(compile(SOURCE), PresentationLimits::default()).unwrap();
    assert!(!Arc::ptr_eq(first.source(), second.source()));
    assert_eq!(first.report().unwrap(), second.report().unwrap());
    let mut blocks = SOURCE.split("\n\n");
    let prefixes = blocks.next().unwrap();
    let mut declarations = blocks.collect::<Vec<_>>();
    declarations.reverse();
    let reordered = format!("{prefixes}\n\n{}", declarations.join("\n\n"));
    assert_eq!(
        first.report().unwrap(),
        execute(compile(&reordered), PresentationLimits::default())
            .unwrap()
            .report()
            .unwrap()
    );
    let a = &first.merges()[&format!("{EX}merge")];
    let b = &second.merges()[&format!("{EX}merge")];
    assert!(a.factor(b.left_injection(), b.right_injection()).is_err());
}
