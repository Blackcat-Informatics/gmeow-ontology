// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::{CompiledTheory, PreparedLogicSource, Severity, parse_logic_dataset};
use crate::projections::rdf::project_canonical_rdf12_dataset;

const SOURCE: &str = include_str!("../../../tests/fixtures/presentation-merge.ttl");
const EX: &str = "urn:presentation-example:";

fn compile(source: &str) -> CompiledTheory {
    let dataset = purrdf::parse_dataset(source.as_bytes(), "application/trig", None).unwrap();
    PreparedLogicSource::new(&dataset)
        .unwrap()
        .into_compiled(None)
        .unwrap()
}

fn data(program: &LogicProgram) -> &PresentationDefinitions {
    let PresentationProgramIr::Lowered(data) = &program.presentations else {
        panic!("expected lowered declarations: {:?}", program.presentations);
    };
    data
}

fn roundtrips(program: &LogicProgram) {
    let projected = project_canonical_rdf12_dataset(program).unwrap();
    let (rdf, diagnostics) = parse_logic_dataset(&projected.dataset, None).unwrap();
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    assert_eq!(
        rdf.presentations, program.presentations,
        "canonical RDF declarations"
    );
    assert_eq!(
        rdf.canonical_key(),
        program.canonical_key(),
        "canonical RDF program"
    );
    let outputs = [
        crate::clif::parse_clif_str(&crate::clif::project_clif(program).unwrap().content, None),
        crate::cgif::parse_cgif_str(&crate::cgif::project_cgif(program).unwrap().content, None),
        crate::xcl::parse_xcl_str(&crate::xcl::project_xcl(program).unwrap().content, None),
    ];
    for (dialect, result) in ["CLIF", "CGIF", "XCL"].into_iter().zip(outputs) {
        let (parsed, diagnostics) = result.unwrap();
        assert!(
            !diagnostics.iter().any(|d| d.severity == Severity::Error),
            "{dialect}: {diagnostics:?}"
        );
        assert_eq!(
            parsed.presentations, program.presentations,
            "{dialect} declarations"
        );
        assert_eq!(
            parsed.canonical_key(),
            program.canonical_key(),
            "{dialect} program"
        );
    }
    let bytes = serde_json::to_vec(program).unwrap();
    let cached: LogicProgram = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        cached.presentations, program.presentations,
        "cache declarations"
    );
    assert_eq!(
        cached.canonical_key(),
        program.canonical_key(),
        "cache identity"
    );
}

#[test]
fn typed_declarations_own_syntax_and_reuse_original_formula_body() {
    let compiled = compile(SOURCE);
    assert!(
        compiled.diagnostics().is_empty(),
        "{:?}",
        compiled.diagnostics()
    );
    let program = compiled.program();
    assert!(
        program.axioms.is_empty(),
        "presentation syntax leaked into global axioms: {:?}",
        program.axioms
    );
    assert!(program.formulas.is_empty());
    let declarations = data(program);
    assert_eq!(declarations.formulas.len(), 1);
    assert_eq!(declarations.sentences.len(), 3);
    assert_eq!(declarations.contracts.len(), 1);
    assert!(Arc::ptr_eq(
        declarations.formulas.values().next().unwrap(),
        compiled.presentation_formulas().values().next().unwrap()
    ));
    roundtrips(program);
}

#[test]
fn standalone_declarations_and_empty_contexts_survive_without_executing_a_merge() {
    let compiled = compile(&format!(
        "{SOURCE}\nex:unused a logic:PresentationContext ; logic:presentationWorld ex:emptyWorld ; logic:presentationModality \"none\" ."
    ));
    let mut program = compiled.program().clone();
    let PresentationProgramIr::Lowered(declarations) = &mut program.presentations else {
        panic!()
    };
    Arc::make_mut(declarations).merges.clear();
    roundtrips(&program);
    assert_eq!(data(&program).contexts.len(), 2);
}

#[test]
fn exact_carriers_preserve_scoped_historical_names_and_full_sentence_indices() {
    let source = SOURCE.replace("logic:presentationModality \"none\"", "logic:presentationModality \"none\" ; logic:presentationStandpoint ex:stance ; logic:presentationTime ex:instant ; logic:presentationPath ex:history ; logic:presentationModule ex:module");
    let compiled = compile(&source);
    let mut program = compiled.program().clone();
    let PresentationProgramIr::Lowered(declarations) = &mut program.presentations else {
        panic!()
    };
    let declarations = Arc::make_mut(declarations);
    let names = &mut declarations
        .symbols
        .get_mut(&format!("{EX}leftPrivate"))
        .unwrap()
        .names;
    for scope in [17, 29] {
        names.insert(PresentationTerm(TermValue::Blank {
            label: "same-label".into(),
            scope: purrdf::BlankScope(scope),
        }));
    }
    declarations
        .evidence
        .get_mut(&format!("{EX}baseEvidence"))
        .unwrap()
        .origin = PresentationTerm(TermValue::Blank {
        label: "original-source".into(),
        scope: purrdf::BlankScope(43),
    });
    roundtrips(&program);
}

#[test]
fn malformed_selected_declarations_remain_explicit_refusals_at_every_exact_boundary() {
    let compiled =
        compile(&SOURCE.replace("logic:mergeRight ex:rightMap", "logic:mergeRight ex:absent"));
    let program = compiled.program();
    let PresentationProgramIr::Refused { roots, detail, .. } = &program.presentations else {
        panic!()
    };
    assert!(!roots.is_empty());
    assert!(!detail.is_empty());
    assert!(project_canonical_rdf12_dataset(program).is_err());
    assert!(crate::clif::project_clif(program).is_err());
    assert!(crate::cgif::project_cgif(program).is_err());
    assert!(crate::xcl::project_xcl(program).is_err());
    let decoded: LogicProgram =
        serde_json::from_slice(&serde_json::to_vec(program).unwrap()).unwrap();
    assert_eq!(decoded.presentations, program.presentations);
}

#[test]
fn unsupported_target_views_disclose_presentations_and_merge_operations() {
    let compiled = compile(SOURCE);
    for target in ["OWL-DL", "OWL-EL", "gUFO", "Datalog", "N3"] {
        let notes = crate::projections::contract_drop_notes(compiled.program(), target, &|_| false);
        for name in ["apex", "left", "right", "merge"] {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains(&format!("<{EX}{name}>")) && note.contains(target)),
                "{notes:?}"
            );
        }
    }
}

#[test]
fn declaration_identity_covers_bindings_contexts_evidence_and_execution_selection() {
    let compiled = compile(SOURCE);
    let original = compiled.program();
    let variants = [
        SOURCE.replace(
            "logic:presentationWorld ex:world",
            "logic:presentationWorld ex:anotherWorld",
        ),
        SOURCE.replace(
            "logic:sentenceSign logic:NegativeSupport",
            "logic:sentenceSign logic:PositiveSupport",
        ),
        SOURCE.replace(
            "logic:sentenceKind logic:PresentationCaveat",
            "logic:sentenceKind logic:PresentationAxiom",
        ),
        SOURCE.replace(
            "logic:evidenceOrigin ex:rightSource",
            "logic:evidenceOrigin ex:otherSource",
        ),
        SOURCE.replace("logic:CompleteOverApproximation", "logic:ExactPreservation"),
        SOURCE.replace(
            "urn:presentation-example:unprojectedConstraint",
            "urn:presentation-example:otherConstraint",
        ),
        SOURCE.replace(
            "logic:mergeRight ex:rightMap",
            "logic:mergeRight ex:leftMap",
        ),
        SOURCE.replace(
            "logic:bindingTarget ex:pSymbol",
            "logic:bindingTarget ex:xSymbol",
        ),
        SOURCE.replace(
            "logic:bindingSymbol ex:pSymbol",
            "logic:bindingSymbol ex:xSymbol",
        ),
    ];
    for source in variants {
        let changed = compile(&source);
        assert_ne!(changed.program().canonical_key(), original.canonical_key());
        assert!(
            matches!(
                changed.program().presentations,
                PresentationProgramIr::Lowered(_)
            ),
            "{:?}",
            changed.diagnostics()
        );
    }
}
