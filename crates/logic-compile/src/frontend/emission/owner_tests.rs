// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::tests::{node, prepared};
use super::*;

fn owner<'a>(
    compiled: &'a SourceCompilation<'_>,
    name: &str,
    family: OwnerFamily,
) -> &'a OwnerLowering {
    compiled
        .owner_lowerings()
        .iter()
        .find(|lowering| {
            lowering.source == node(compiled.source(), name) && lowering.family == family
        })
        .unwrap()
}

fn index(owner: &OwnerLowering) -> usize {
    let OwnerDisposition::Emitted { index } = owner.disposition else {
        panic!("owner did not emit: {owner:?}")
    };
    index
}

#[test]
fn rule_contract_path_and_sugar_bindings_follow_their_actual_sorted_values() {
    let source = prepared(
        r#"
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        ex:firstRule a logic:Rule ; logic:head [ rdf:subject ex:Zulu ; rdf:predicate ex:p ; rdf:object ex:v ] .
        ex:lastRule a logic:Rule ; logic:head [ rdf:subject ex:Alfa ; rdf:predicate ex:p ; rdf:object ex:v ] .
        ex:contract a logic:ReasoningContract ; logic:modelSemantics logic:StableModelSemantics ; logic:negationOperator logic:DefaultNegation .
        logic:StableModelSemantics a logic:ModelSemantics .
        logic:DefaultNegation a logic:NegationOperator .
        ex:range a logic:ValueRangeConstraint ; logic:onClass ex:Probability ; logic:valuePath ex:magnitude ; logic:minInclusiveBound 0 ; logic:maxInclusiveBound 1 ; logic:formalizes ex:Probability .
        ex:zeta a logic:PathShape ; logic:pathStepPredicate ex:p .
        ex:alpha a logic:PathShape ; logic:pathStepPredicate ex:p .
    "#,
    );
    let compiled = source.compile_with_sources(None).unwrap();
    assert!(
        compiled.diagnostics().is_empty(),
        "{:?}",
        compiled.diagnostics()
    );
    let first = index(owner(&compiled, "firstRule", OwnerFamily::Rule));
    let last = index(owner(&compiled, "lastRule", OwnerFamily::Rule));
    assert!(first > last);
    assert_eq!(
        compiled.program().rules[first].head.subject,
        "https://example.org/Zulu"
    );
    assert_eq!(
        compiled.program().rules[last].head.subject,
        "https://example.org/Alfa"
    );
    assert_eq!(
        index(owner(&compiled, "contract", OwnerFamily::Contract)),
        0
    );
    assert_eq!(
        compiled.program().constraints[index(owner(&compiled, "range", OwnerFamily::Constraint))]
            .iri,
        "https://example.org/range"
    );
    for name in ["alpha", "zeta"] {
        assert_eq!(
            compiled.program().path_shapes[index(owner(&compiled, name, OwnerFamily::PathShape))]
                .iri,
            format!("https://example.org/{name}")
        );
    }
}

#[test]
fn constraint_and_reasoning_program_keep_their_shared_formula_ownership() {
    let source = prepared(
        r#"
        ex:law a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:implication .
        ex:implication a logic:Formula ; logic:antecedent ex:guard ; logic:consequent ex:claim .
        ex:guard a logic:Formula ; logic:relation logic:instanceOf ; logic:argument ex:x, ex:class .
        ex:claim a logic:Formula ; logic:relation ex:p ; logic:argument ex:x, ex:value .
        ex:x logic:termIndex 0 ; logic:termVariable "x" ; logic:variableSort ex:Thing .
        ex:class logic:termIndex 1 ; logic:termIri ex:Thing .
        ex:value logic:termIndex 1 ; logic:termIri ex:valueConstant .
        ex:constraint a logic:Constraint ; logic:integrity ex:law .
        ex:program a logic:ReasoningProgram ; logic:evaluationMode logic:BackwardEvaluation ; logic:clause ex:law ; logic:programQuery ex:claim ; logic:verdictProbe ex:ground .
        ex:ground a logic:Formula ; logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:individual ], ex:value .
    "#,
    );
    let compiled = source.compile_with_sources(None).unwrap();
    assert!(
        compiled.diagnostics().is_empty(),
        "{:?}",
        compiled.diagnostics()
    );
    assert_eq!(
        compiled.program().constraints
            [index(owner(&compiled, "constraint", OwnerFamily::Constraint))]
        .iri,
        "https://example.org/constraint"
    );
    assert_eq!(
        compiled.program().reasoning_programs
            [index(owner(&compiled, "program", OwnerFamily::ReasoningProgram))]
        .iri,
        "https://example.org/program"
    );
    assert!(compiled.program().formulas.is_empty());
    assert!(
        compiled
            .formula_lowerings()
            .iter()
            .all(|lowering| lowering.disposition == FormulaDisposition::ReadForOwner)
    );
}

#[test]
fn rejected_owners_bind_their_own_or_shared_original_diagnostics() {
    let source = prepared(
        r#"
        ex:rule a logic:Rule . ex:path a logic:PathShape .
        ex:program a logic:ReasoningProgram . ex:corr a logic:Correspondence .
        ex:constraint a logic:Constraint ; logic:integrity ex:broken .
        ex:broken a logic:Formula .
        ex:sugar a logic:ValueRangeConstraint ; logic:onClass ex:Thing ; logic:valuePath ex:p .
    "#,
    );
    let compiled = source.compile_with_sources(None).unwrap();
    for (name, family) in [
        ("rule", OwnerFamily::Rule),
        ("path", OwnerFamily::PathShape),
        ("program", OwnerFamily::ReasoningProgram),
        ("corr", OwnerFamily::Correspondence),
        ("constraint", OwnerFamily::Constraint),
        ("sugar", OwnerFamily::Constraint),
    ] {
        let rejected = owner(&compiled, name, family);
        assert_eq!(rejected.disposition, OwnerDisposition::Rejected);
        assert!(!rejected.diagnostics.is_empty(), "{rejected:?}");
        assert!(
            rejected
                .diagnostics
                .iter()
                .all(|&index| index < compiled.diagnostics().len())
        );
    }
    let broken = compiled
        .formula_lowerings()
        .iter()
        .find(|lowering| lowering.source == node(&source, "broken"))
        .unwrap();
    let FormulaDisposition::Malformed { diagnostic } = broken.disposition else {
        panic!("broken formula must fail")
    };
    assert!(
        owner(&compiled, "constraint", OwnerFamily::Constraint)
            .diagnostics
            .contains(&diagnostic)
    );
}

#[test]
fn emitted_unsupported_contract_keeps_its_admission_error() {
    let source = prepared(
        "ex:contract a logic:ReasoningContract ; logic:uncertaintyMeasure logic:ProbabilisticMeasure .",
    );
    let compiled = source.compile_with_sources(None).unwrap();
    let contract = owner(&compiled, "contract", OwnerFamily::Contract);
    assert_eq!(index(contract), 0);
    assert!(
        contract
            .diagnostics
            .iter()
            .any(
                |&index| compiled.diagnostics()[index].code == "UNSUPPORTED_CONTRACT"
                    && compiled.diagnostics()[index].severity == crate::frontend::Severity::Error
            )
    );
}

#[test]
fn native_correspondence_owners_and_shared_leg_use_only_selected_graph_fields() {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let ty = builder.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let class = builder.intern_iri("https://blackcatinformatics.ca/logic/Correspondence");
    let named = builder.intern_iri("https://example.org/named");
    for name in ["first", "second"] {
        let subject = builder.intern_iri(&format!("https://example.org/{name}"));
        builder.push_annotation(subject, ty, class);
        for (field, value) in [
            ("correspondenceRelation", "Subsumes"),
            ("morphismClass", "LossyLens"),
            ("morphismKind", "InstitutionMorphism"),
        ] {
            let p = builder.intern_iri(&format!("https://blackcatinformatics.ca/logic/{field}"));
            let o = builder.intern_iri(&format!("https://blackcatinformatics.ca/logic/{value}"));
            builder.push_annotation(subject, p, o);
            builder.push_quad(subject, p, o, None);
            let invalid = builder.intern_iri("https://example.org/unselected");
            builder.push_quad(subject, p, invalid, Some(named));
        }
        let get = builder.intern_iri("https://blackcatinformatics.ca/logic/getLeg");
        let leg = builder.intern_iri("https://example.org/sharedLeg");
        builder.push_annotation(subject, get, leg);
    }
    let leg = builder.intern_iri("https://example.org/sharedLeg");
    let path = builder.intern_iri("https://blackcatinformatics.ca/gmeow/path");
    let step = builder.intern_iri("https://example.org/p");
    builder.push_annotation(leg, path, step);
    let source = PreparedLogicSource::new(&builder.freeze().unwrap()).unwrap();
    let compiled = source.compile_with_sources(None).unwrap();
    assert!(
        compiled.diagnostics().is_empty(),
        "{:?}",
        compiled.diagnostics()
    );
    assert_eq!(compiled.program().correspondences.len(), 2);
    assert_eq!(compiled.program().transaction_programs.len(), 1);
    assert_eq!(
        index(owner(
            &compiled,
            "sharedLeg",
            OwnerFamily::TransactionProgram
        )),
        0
    );
    for name in ["first", "second"] {
        assert_eq!(
            compiled.program().correspondences
                [index(owner(&compiled, name, OwnerFamily::Correspondence))]
            .iri,
            format!("https://example.org/{name}")
        );
    }
}

#[test]
fn unlowered_leg_anonymous_correspondence_and_named_owners_are_accounted_for() {
    let source = prepared(
        r#"
        ex:corr a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ; logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism ; logic:getLeg ex:missing .
        _:anonymous a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ; logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism .
        ex:named { ex:rule a logic:Rule . ex:contract a logic:ReasoningContract, logic:ReasoningPreset . ex:corr a logic:Correspondence . }
    "#,
    );
    let compiled = source.compile_with_sources(None).unwrap();
    let missing = owner(&compiled, "missing", OwnerFamily::TransactionProgram);
    assert_eq!(missing.disposition, OwnerDisposition::Rejected);
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|&index| compiled.diagnostics()[index].code == "UNLOWERED_TRANSACTION_PROGRAM")
    );
    assert!(
        compiled
            .owner_lowerings()
            .iter()
            .any(|lowering| lowering.family == OwnerFamily::Correspondence
                && lowering.disposition == OwnerDisposition::Rejected
                && matches!(
                    source.dataset().resolve(lowering.source.term),
                    purrdf::TermRef::Blank { .. }
                )
                && lowering
                    .diagnostics
                    .iter()
                    .any(|&index| compiled.diagnostics()[index].code
                        == "UNLOWERED_CORRESPONDENCE_IDENTITY"))
    );
    let excluded: Vec<_> = compiled
        .owner_lowerings()
        .iter()
        .filter(|lowering| lowering.disposition == OwnerDisposition::OutsideDefaultGraph)
        .collect();
    assert_eq!(
        excluded.len(),
        3,
        "one contract extraction for both type declarations"
    );
    assert!(
        excluded
            .iter()
            .all(|lowering| lowering.source.graph.is_some())
    );
}
