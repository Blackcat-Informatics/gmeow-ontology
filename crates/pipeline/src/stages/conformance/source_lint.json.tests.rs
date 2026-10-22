// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
#[test]
fn json_source_lint_configuration_input_pins_actual_literal_bytes_and_digest() {
    let expected = r#"{"namespace":"https://blackcatinformatics.ca/gmeow/","ontology_iri":"https://blackcatinformatics.ca/gmeow","selector_tokens":["default","main","preferred","primary"],"core_slice_iris":[],"annotation_predicates":["http://purl.org/dc/terms/description","http://purl.org/dc/terms/title","http://www.w3.org/2000/01/rdf-schema#comment","http://www.w3.org/2000/01/rdf-schema#label","http://www.w3.org/2004/02/skos/core#definition"]}"#;
    let bytes = purrdf_lex::json::record::to_vec(&Configuration::from(&configuration()));
    assert_eq!(bytes, expected.as_bytes());
    match configuration_input().unwrap() {
        ActionInput::Raw {
            logical_path,
            file_kind,
            executable,
            digest,
        } => {
            assert_eq!(logical_path, "structural-source-fixtures/configuration");
            assert_eq!(file_kind, FileKind::Aggregate);
            assert!(!executable);
            assert_eq!(digest, bytes_digest(expected.as_bytes()));
        }
        other => panic!("unexpected input {other:?}"),
    }
}
