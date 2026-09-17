// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Positive: every committed composition link in the real corpus is compatible.
#[test]
fn validate_compositions_corpus_ok() {
    let fixture =
        crate::fixture::stage_fixture(&repo_root(), 1, "stage-validate-result-shape-composition")
            .expect("load authenticated composition-validation receipt without rebuilding corpus");
    assert_eq!(
        fixture.outcome.product.digest,
        "result-shape-composition-ok"
    );
    assert!(
        !fixture.outcome.built,
        "test consumers may never run stages"
    );
}

/// Negative: a consumer whose cqInputShape requires a column the producer's
/// cqResultShape does NOT provide must produce an Err.
#[test]
fn incompatible_composition_hard_fails() {
    // Producer declares only column ?x (IRI); consumer requires ?x AND ?y (IRI).
    // is_satisfiable_by must reject the link.
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex:    <https://example.org/> .\n\
            \n\
            ex:producer gmeow:cqResultShape ex:producerShape .\n\
            ex:consumer gmeow:cqConsumes    ex:producer ;\n\
                        gmeow:cqInputShape  ex:consumerShape .\n\
            \n\
            ex:producerShape a logic:ResultShape ;\n\
                logic:shapeCardinality logic:RowsContains ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"x\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] .\n\
            \n\
            ex:consumerShape a logic:ResultShape ;\n\
                logic:shapeCardinality logic:RowsContains ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"x\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"y\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] .\n";
    let store = native_query::dataset_from_turtle(ttl.as_bytes(), "test").unwrap();
    let result = validate_store(&store);
    assert!(
        result.is_err(),
        "expected Err for missing column ?y, got Ok"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains('y') || msg.contains("mismatch"),
        "error message should mention the missing column or mismatch: {msg}"
    );
}

/// Positive in-memory: a perfectly compatible link passes validate_store.
#[test]
fn compatible_composition_passes() {
    // Producer declares ?x (IRI, Required); consumer requires exactly ?x (IRI).
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex:    <https://example.org/> .\n\
            \n\
            ex:producer gmeow:cqResultShape ex:producerShape .\n\
            ex:consumer gmeow:cqConsumes    ex:producer ;\n\
                        gmeow:cqInputShape  ex:consumerShape .\n\
            \n\
            ex:producerShape a logic:ResultShape ;\n\
                logic:shapeCardinality logic:RowsContains ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"x\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] .\n\
            \n\
            ex:consumerShape a logic:ResultShape ;\n\
                logic:shapeCardinality logic:RowsContains ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"x\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] .\n";
    let store = native_query::dataset_from_turtle(ttl.as_bytes(), "test").unwrap();
    validate_store(&store).expect("compatible composition must pass");
}
