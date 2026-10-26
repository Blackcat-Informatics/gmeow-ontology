// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored observations; no source catalog, tokenizer or corpus production.
use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const WIRE: &str = r#"{"aliases":{"a":"first","z":"last"},"nec_reverse":null,"acceptance":[3,1],"resolved_acceptance":[3,2],"current_header":{"Ok":null},"future_header":{"Err":{"UndeclaredDialectVersion":{"detail":"future"}}},"per_claim":{"Ok":null},"standalone":{"Ok":{"checked":["z","a"],"skipped":["shared"]}},"glyphs":{"urn:glyph":{"codepoints":["U+002B","U+002D"],"scopes":["logic","math"]}},"records":{"sigils":{"Ok":{"text":"@S{中}","canonical_equal":true}},"scoped":{"Ok":{"written":{"text":"@L{raw}","canonical_equal":false},"fallback":{"Ok":true},"wrong_scope":{"Err":{"Uncovered":"+"}},"wrong_fallback_scope":{"Ok":null}}},"process":{"Ok":{"written":{"text":"@P{raw}","canonical_equal":true},"primary":{"Covered":{"subject":"IriExternalByReference","predicate":"IriGlyph","object":"TripleTerm"}},"glyph_count":2}}},"costs":{"rows":[{"codepoints":["U+002B"],"values":["01","2"],"measured":{"Ok":7}}],"spellings":["+","add"],"denotation_form_present":true,"aliases":{"urn:add":["add","plus"]},"emitted":{"term":{"Ok":"exact\n中"}}},"stamps":{"resolved":"3","dictionary_major":"3","acceptance_major":"4","values":{"urn:record":["3","3"]},"repeated_quad_equal":true},"notation":{"Ok":{"production":"raw production","canonical":[{"name":"glyph","body":{"Group":{"Repeat":[8,3,{"Terminal":"+"}]}}}],"views":{"Abnf":{"Err":{"failure_class":"SilentIngestDrop","construct":"authored refusal"}},"Ebnf":{"Ok":[{"name":"glyph","body":{"Ref":"raw"}}]}}}}}"#;

#[test]
fn native_dictionary_observation_preserves_complete_wire_and_unmodified_grammar() {
    let observed: Observations = from_slice(WIRE.as_bytes()).unwrap();
    assert_eq!(to_vec(&observed), WIRE.as_bytes());
    assert_eq!(observed.acceptance, (3, 1));
    assert_eq!(observed.resolved_acceptance, (3, 2));
    assert_eq!(observed.standalone.as_ref().unwrap().checked, ["z", "a"]);
    assert_eq!(observed.costs.rows[0].values, ["01", "2"]);
    assert_eq!(observed.costs.rows[0].measured.as_ref().unwrap(), &7);
    assert_eq!(
        observed.notation.as_ref().unwrap().canonical[0].body,
        gmeow_lang_bridge::RuleExpr::Group(Box::new(gmeow_lang_bridge::RuleExpr::Repeat(
            Some(8),
            Some(3),
            Box::new(gmeow_lang_bridge::RuleExpr::Terminal("+".into()))
        )))
    );
    let absent = WIRE.replace(r#""nec_reverse":null,"#, "");
    assert_eq!(
        to_vec(&from_slice::<Observations>(absent.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let extended = WIRE.replacen('{', r#"{"extension":true,"#, 1);
    assert_eq!(
        to_vec(&from_slice::<Observations>(extended.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let repeated = WIRE.replace(
        r#""aliases":{"a":"first","z":"last"}"#,
        r#""aliases":{"a":"old","a":"first","z":"last"}"#,
    );
    assert_eq!(
        to_vec(&from_slice::<Observations>(repeated.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
}

#[test]
fn native_dictionary_observation_refuses_bad_coordinates_records_and_outcomes() {
    for row in ["[]", "[3]", "[3,1,2]", "null"] {
        let bad = WIRE.replace(r#""acceptance":[3,1]"#, &format!(r#""acceptance":{row}"#));
        assert_eq!(
            from_slice::<Observations>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/acceptance"
        );
    }
    for token in ["-0", "-1", "1.0", "1e0", "4294967296", r#""1""#] {
        let bad = WIRE.replace(
            r#""acceptance":[3,1]"#,
            &format!(r#""acceptance":[3,{token}]"#),
        );
        assert_eq!(
            from_slice::<Observations>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/acceptance/1"
        );
    }
    for token in ["-0", "-1", "1.0", "1e0", "18446744073709551616", "null"] {
        let bad = WIRE.replace(
            r#""measured":{"Ok":7}"#,
            &format!(r#""measured":{{"Ok":{token}}}"#),
        );
        assert_eq!(
            from_slice::<Observations>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/costs/rows/0/measured/Ok"
        );
    }
    for bad in [
        WIRE.replace(
            r#""current_header":{"Ok":null}"#,
            r#""current_header":{"Ok":false}"#,
        ),
        WIRE.replace(
            r#""current_header":{"Ok":null}"#,
            r#""current_header":{"Ok":null,"Err":{"Uncovered":"x"}}"#,
        ),
        WIRE.replace(r#""aliases":{"a":"first","z":"last"},"#, ""),
        WIRE.replacen('{', r#"{"aliases":{},"#, 1),
        WIRE.replace(r#""checked":["z","a"]"#, r#""checked":null"#),
        WIRE.replace(r#""glyph_count":2"#, r#""glyph_count":2.0"#),
        WIRE.replace(
            r#""emitted":{"term":{"Ok":"exact\n中"}}"#,
            r#""emitted":{"term":{"Ok":null},"term":{"Ok":"valid"}}"#,
        ),
        WIRE.replace(r#""resolved":"3","#, ""),
        "[]".to_owned(),
    ] {
        assert!(
            from_slice::<Observations>(bad.as_bytes()).is_err(),
            "accepted {bad}"
        );
    }
    let object = purrdf_lex::json::read(WIRE).unwrap();
    let positional = Value::Array(
        object
            .as_object()
            .unwrap()
            .iter()
            .map(|(_, value)| value.clone())
            .collect(),
    );
    assert!(
        Observations::from_json(&positional).is_err(),
        "a complete positional inventory is not a named record"
    );
    assert!(
        from_slice::<Written>(br#"["text",true]"#).is_err(),
        "a complete positional nested record is refused"
    );
}

#[test]
fn native_dictionary_observation_retains_measured_cost_causal_refusals() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let cause = ledger.attach(
        Diag::from(std::io::Error::other("authored cost premise")).with_focus("urn:cost:premise"),
        StageId::new("cost-input"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("authored cost refusal"))
            .with_focus("urn:cost:refusal")
            .with_antecedents([cause]),
        StageId::new("cost"),
    );
    let mut observed: Observations = from_slice(WIRE.as_bytes()).unwrap();
    observed.costs.rows[0].measured = Err(refusal.clone());
    observed.current_header = Err(Gmn1Error::MalformedNumber {
        token: "raw".into(),
    });
    observed.per_claim = Err(Gmn1Error::PerClaimMismatch {
        subject: "urn:claim".into(),
    });
    observed.standalone = Err(Gmn1Error::NonDecodableGrammar {
        detail: "standalone".into(),
    });
    observed.notation = Err(IngestDiagnostic {
        failure_class: gmeow_lang_bridge::LangFailure::NonUtf8Surface,
        construct: "raw notation".into(),
    });
    let encoded = to_vec(&observed);
    let decoded: Observations = from_slice(&encoded).unwrap();
    assert_eq!(to_vec(&decoded), encoded);
    assert_eq!(
        decoded.costs.rows[0].measured.as_ref().unwrap_err(),
        &refusal
    );
    assert_eq!(decoded.current_header, observed.current_header);
    assert_eq!(decoded.per_claim, observed.per_claim);
    assert_eq!(decoded.standalone, observed.standalone);
    assert_eq!(
        decoded.notation.as_ref().unwrap_err(),
        observed.notation.as_ref().unwrap_err()
    );
    let mut replay = DiagLedger::new();
    replay.replay(refusal.causes);
    replay.replay([*refusal.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}
