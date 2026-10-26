// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored literal coordinates only; neither sources nor reasoning are invoked.
use crate::ir::{FiniteNumericLiteral, UnitInterval};
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn json_numeric_literal_exact_identity_range_and_retained_serde_match() {
    for lexical in ["0.95", "0.9", "0", "1", "-0.0"] {
        let input =
            format!(r#"["{lexical}","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#);
        let native: UnitInterval = from_slice(input.as_bytes()).unwrap();
        let retained: UnitInterval = serde_json::from_str(&input).unwrap();
        assert_eq!(native, retained);
        assert_eq!(native.literal().lexical_form, lexical);
        assert_eq!(to_vec(&native), input.as_bytes());
        assert_eq!(serde_json::to_vec(&native).unwrap(), input.as_bytes());
    }
    let weight = br#"["-2.5","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#;
    assert!(from_slice::<UnitInterval>(weight).is_err());
    assert_eq!(
        to_vec(&from_slice::<FiniteNumericLiteral>(weight).unwrap()),
        weight
    );
    let first: UnitInterval =
        from_slice(br#"["0.90","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#).unwrap();
    let second: UnitInterval =
        from_slice(br#"["0.9","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#).unwrap();
    assert_ne!(
        first, second,
        "numeric equality does not erase RDF literal identity"
    );
}
#[test]
fn json_numeric_literal_refuses_invalid_four_cell_and_numeric_values() {
    for bad in [
        r#""0.95""#,
        r#"["0.95","http://www.w3.org/2001/XMLSchema#decimal",null]"#,
        r#"["0.95","http://www.w3.org/2001/XMLSchema#decimal",null,null,null]"#,
        r#"[0.95,"http://www.w3.org/2001/XMLSchema#decimal",null,null]"#,
        r#"["0.95","http://www.w3.org/2001/XMLSchema#string",null,null]"#,
        r#"["NaN","http://www.w3.org/2001/XMLSchema#double",null,null]"#,
        r#"["INF","http://www.w3.org/2001/XMLSchema#double",null,null]"#,
        r#"["1.01","http://www.w3.org/2001/XMLSchema#decimal",null,null]"#,
        r#"["0.5","http://www.w3.org/2001/XMLSchema#decimal","en",null]"#,
    ] {
        assert!(from_slice::<UnitInterval>(bad.as_bytes()).is_err(), "{bad}");
        assert!(
            serde_json::from_str::<UnitInterval>(bad).is_err(),
            "retained owner: {bad}"
        );
    }
}
