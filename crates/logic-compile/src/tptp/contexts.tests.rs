// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const FORMULAS: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/> .
ex:left a logic:Formula ; logic:relation ex:p ;
    logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ],
                   [ logic:termIndex 1 ; logic:termIri ex:b ] .
ex:right a logic:Formula ; logic:relation ex:q ;
    logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ],
                   [ logic:termIndex 1 ; logic:termIri ex:b ] .
"#;

/// Neither atomic routing nor compound formula emission may discard source scope.
#[test]
fn selected_formula_scope_annotations_block_context_erasure() {
    for annotation in [
        "logic:standpoint ex:observer",
        "logic:accordingTo ex:observer",
        "<https://blackcatinformatics.ca/gmeow/accordingTo> ex:observer",
        "logic:time \"2026\"",
        "logic:confidence 0.7",
        "logic:modality logic:Epistemic",
        "logic:provenance ex:reporter",
        "logic:inModule ex:module",
        "logic:world ex:world",
        "logic:path ex:path",
        "logic:queryContext ex:query",
    ] {
        for source in [
            format!("{FORMULAS}\nex:left {annotation} ."),
            format!(
                "{FORMULAS}\nex:root a logic:Formula ; logic:and ex:left, ex:right ; {annotation} ."
            ),
            format!("{FORMULAS}\nex:left logic:inContext ex:context ; {annotation} ."),
        ] {
            let (_, projection) = project(&source);
            assert_eq!(
                projection.status,
                FofProjectionStatus::Blocked,
                "{annotation}"
            );
            assert!(projection.problem.is_none(), "{annotation}");
            assert!(
                projection
                    .blockers
                    .iter()
                    .any(|blocker| blocker.code == "CONTEXT_ERASURE_FORBIDDEN"),
                "the source scope must have an explicit disposition: {annotation}: {:?}",
                projection.blockers
            );
        }
    }
}

/// Read-for-owner children cannot hide scope from their selected formula ancestor.
#[test]
fn nested_formula_scope_blocks_the_selected_ancestor() {
    let (_, projection) = project(&format!(
        "{FORMULAS}\nex:root a logic:Formula ; logic:and ex:left, ex:right .\nex:left logic:standpoint ex:observer ."
    ));
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(
        projection
            .blockers
            .iter()
            .any(|blocker| blocker.code == "CONTEXT_ERASURE_FORBIDDEN")
    );
}

/// The existing explicit-context translation retains its actual context argument.
#[test]
fn translated_in_context_formulas_remain_admitted() {
    for annotation in [
        "ex:left logic:inContext ex:context .",
        "ex:root logic:inContext ex:context .",
    ] {
        let (_, projection) = project(&format!(
            "{FORMULAS}\nex:root a logic:Formula ; logic:and ex:left, ex:right .\n{annotation}"
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Complete,
            "{:?}",
            projection.blockers
        );
        assert!(
            projection
                .problem
                .unwrap()
                .contains("I|https://example.org/context")
        );
    }
}

/// Typed modal translations retain both their source context and accessibility family.
#[test]
fn translated_modal_formulas_keep_their_typed_accessibility() {
    for relation in [
        "https://blackcatinformatics.ca/logic/epistemicallyPossible",
        "https://blackcatinformatics.ca/logic/doxasticallyAccessible",
        "https://blackcatinformatics.ca/logic/deonticallyIdeal",
        "https://blackcatinformatics.ca/logic/temporallySucceeds",
        "https://blackcatinformatics.ca/logic/counterfactuallyCloser",
        "https://blackcatinformatics.ca/gmeow/sharpens",
    ] {
        for operator in ["necessarily", "possibly"] {
            let (_, projection) = project(&format!(
                "{FORMULAS}\nex:root a logic:Formula ; logic:inContext ex:context ; logic:{operator} ex:left ; logic:overAccessibility <{relation}> ."
            ));
            assert_eq!(
                projection.status,
                FofProjectionStatus::Complete,
                "{:?}",
                projection.blockers
            );
            let modal = projection
                .sentences
                .iter()
                .find(|sentence| {
                    !sentence.source_units.is_empty()
                        && sentence.body.contains(&format!("I|{relation}"))
                })
                .expect("the source-backed modal sentence retains its typed accessibility");
            assert!(modal.body.contains("I|https://example.org/context"));
            assert!(modal.body.contains("I|https://example.org/p"));
            assert!(
                modal
                    .body
                    .contains(if operator == "necessarily" { "=>" } else { "&" })
            );
        }
    }
}

/// An unselected document remains inventoried without contaminating a selected theory.
#[test]
fn unselected_formula_scope_does_not_block_an_independent_selection() {
    let contextual = format!("{FORMULAS}\nex:left logic:standpoint ex:observer .");
    let theory = compile_documents(&[
        ("scoped.ttl", &contextual, None),
        ("selected.ttl", "<urn:s> <urn:p> <urn:o> .", None),
    ]);
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["selected.ttl".into()]));
    let admission = SourceAdmission::classical_fof(&theory, selection);
    assert_eq!(
        project_tptp_fof(&theory, &admission).status,
        FofProjectionStatus::Complete
    );
}

/// Metadata in another graph cannot scope the selected default-graph formula.
#[test]
fn named_graph_scope_does_not_contaminate_default_graph_formulas() {
    let dataset = parse_dataset(
        format!("{FORMULAS}\nex:metadata {{ ex:left logic:standpoint ex:observer . }}").as_bytes(),
        "application/trig",
        None,
    )
    .unwrap();
    let theory = PreparedLogicSource::new(&dataset)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
}
