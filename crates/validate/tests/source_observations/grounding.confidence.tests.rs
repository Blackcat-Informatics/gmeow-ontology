// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Literal reader controls only; no authenticated artifact or corpus is loaded.
use super::Cell;

const CELL: &str = r#"{"iri":"urn:cell","subject":"urn:subject","predicate":"urn:predicate","obj":"urn:object","source_endpoint":"urn:source","target_endpoint":"urn:target","sssom_file":"grounding.ttl","morphism_class":"urn:class","morphism_kind":"urn:kind","preservation":"urn:preservation","confidence":["0.95","http://www.w3.org/2001/XMLSchema#decimal",null,null],"grounding":true}"#;

#[test]
fn json_grounding_cell_confidence_reads_actual_decimal_tuple_and_preserves_owner_bytes() {
    for lexical in ["0.95", "0.9"] {
        let input = CELL.replace("\"0.95\"", &format!("\"{lexical}\""));
        let cell: Cell = serde_json::from_str(&input).unwrap();
        assert_eq!(cell.iri, "urn:cell");
        assert_eq!(cell.subject, "urn:subject");
        assert_eq!(cell.predicate, "urn:predicate");
        assert_eq!(cell.obj, "urn:object");
        assert_eq!(cell.source_endpoint.as_deref(), Some("urn:source"));
        assert_eq!(cell.target_endpoint.as_deref(), Some("urn:target"));
        assert_eq!(cell.sssom_file, "grounding.ttl");
        assert_eq!(cell.morphism_class.as_deref(), Some("urn:class"));
        assert_eq!(cell.morphism_kind.as_deref(), Some("urn:kind"));
        assert_eq!(cell.preservation.as_deref(), Some("urn:preservation"));
        assert!(cell.grounding);
        let confidence = cell.confidence.as_ref().unwrap();
        assert_eq!(confidence.literal().lexical_form, lexical);
        assert_eq!(
            confidence.literal().datatype_iri(),
            "http://www.w3.org/2001/XMLSchema#decimal"
        );
        assert_eq!(confidence.literal().language, None);
        assert_eq!(confidence.literal().direction, None);
        let expected =
            format!("[\"{lexical}\",\"http://www.w3.org/2001/XMLSchema#decimal\",null,null]");
        assert_eq!(serde_json::to_string(confidence).unwrap(), expected);
    }
}

#[test]
fn json_grounding_cell_confidence_refuses_stale_string_and_invalid_coordinate_but_allows_absence() {
    let tuple = r#"["0.95","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#;
    for invalid in [
        r#""0.95""#,
        r#"["0.95","http://www.w3.org/2001/XMLSchema#string",null,null]"#,
        r#"["1.01","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#,
        r#"["-0.01","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#,
        r#"["not-a-number","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#,
    ] {
        let bad = CELL.replace(tuple, invalid);
        assert!(serde_json::from_str::<Cell>(&bad).is_err(), "{bad}");
    }
    let null = CELL.replace(tuple, "null");
    assert!(
        serde_json::from_str::<Cell>(&null)
            .unwrap()
            .confidence
            .is_none()
    );
    let absent = CELL.replace(&format!("\"confidence\":{tuple},"), "");
    assert!(
        serde_json::from_str::<Cell>(&absent)
            .unwrap()
            .confidence
            .is_none()
    );
}
