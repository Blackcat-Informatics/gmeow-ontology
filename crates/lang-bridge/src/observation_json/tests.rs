// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored domain transport controls; no dictionary or repository corpus.
use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn native_gmn_refusals_and_coverage_preserve_every_typed_payload() {
    let errors = [
        (
            Gmn1Error::Uncovered(UncoveredTerm("raw\\n中".into())),
            r#"{"Uncovered":"raw\\n中"}"#,
        ),
        (
            Gmn1Error::NamedGraphOutOfDomain {
                graph: "urn:graph".into(),
            },
            r#"{"NamedGraphOutOfDomain":{"graph":"urn:graph"}}"#,
        ),
        (
            Gmn1Error::NonCanonicalOrder {
                detail: "order".into(),
            },
            r#"{"NonCanonicalOrder":{"detail":"order"}}"#,
        ),
        (
            Gmn1Error::MalformedNumber {
                token: "1e400".into(),
            },
            r#"{"MalformedNumber":{"token":"1e400"}}"#,
        ),
        (
            Gmn1Error::UndeclaredDialectVersion {
                detail: "future".into(),
            },
            r#"{"UndeclaredDialectVersion":{"detail":"future"}}"#,
        ),
        (
            Gmn1Error::NonDecodableGrammar {
                detail: "grammar".into(),
            },
            r#"{"NonDecodableGrammar":{"detail":"grammar"}}"#,
        ),
        (
            Gmn1Error::NonNfcLiteral {
                lexical: "é".into(),
            },
            r#"{"NonNfcLiteral":{"lexical":"é"}}"#,
        ),
        (
            Gmn1Error::PerClaimMismatch {
                subject: "urn:claim".into(),
            },
            r#"{"PerClaimMismatch":{"subject":"urn:claim"}}"#,
        ),
    ];
    for (value, wire) in errors {
        assert_eq!(to_vec(&value), wire.as_bytes());
        assert_eq!(from_slice::<Gmn1Error>(wire.as_bytes()).unwrap(), value);
    }
    for name in [
        "IriGlyph",
        "IriDictAlias",
        "IriPrefixMangled",
        "IriPrefixMangledSlashEscaped",
        "IriBareNamespaceRoot",
        "IriExternalByReference",
        "BlankNode",
        "LiteralIdentifier",
        "LiteralInteger",
        "LiteralDecimal",
        "LiteralByReference",
        "TripleTerm",
    ] {
        let wire = format!(r#""{name}""#);
        let value: Gmn1ConstructCategory = from_slice(wire.as_bytes()).unwrap();
        assert_eq!(to_vec(&value), wire.as_bytes());
    }
    let covered = r#"{"Covered":{"subject":"IriExternalByReference","predicate":"IriGlyph","object":"TripleTerm"}}"#;
    let value: QuadCoverage = from_slice(covered.as_bytes()).unwrap();
    assert_eq!(to_vec(&value), covered.as_bytes());
    let uncovered = QuadCoverage::Uncovered(UncoveredTerm("outside".into()));
    assert_eq!(to_vec(&uncovered), br#"{"Uncovered":"outside"}"#);
    for class in ["SilentIngestDrop", "NonUtf8Surface"] {
        let wire = format!(r#"{{"failure_class":"{class}","construct":"raw\\n中"}}"#);
        assert_eq!(
            to_vec(&from_slice::<IngestDiagnostic>(wire.as_bytes()).unwrap()),
            wire.as_bytes()
        );
    }
    let wire = br#"{"checked":["z","a"],"skipped":["shared"]}"#;
    assert_eq!(to_vec(&from_slice::<StandaloneReport>(wire).unwrap()), wire);
}

#[test]
fn native_grammar_transport_preserves_all_branches_groups_and_raw_repeat_bounds() {
    let reference = || Box::new(RuleExpr::Ref("x".into()));
    let rule = GrammarRule {
        name: "authored".into(),
        body: RuleExpr::Seq(vec![
            RuleExpr::Ref("x".into()),
            RuleExpr::Terminal("中\\n".into()),
            RuleExpr::CharClass("^a-z".into()),
            RuleExpr::Hex("00aF".into()),
            RuleExpr::Range("09".into(), "0A".into()),
            RuleExpr::Seq(vec![]),
            RuleExpr::Alt(vec![
                RuleExpr::Terminal("a".into()),
                RuleExpr::Ref("b".into()),
            ]),
            RuleExpr::Diff(reference(), Box::new(RuleExpr::Ref("y".into()))),
            RuleExpr::Star(reference()),
            RuleExpr::Plus(reference()),
            RuleExpr::Opt(reference()),
            RuleExpr::Repeat(Some(8), Some(3), reference()),
            RuleExpr::Group(reference()),
            RuleExpr::Repeat(None, Some(u32::MAX), reference()),
        ]),
    };
    let wire = r#"{"name":"authored","body":{"Seq":[{"Ref":"x"},{"Terminal":"中\\n"},{"CharClass":"^a-z"},{"Hex":"00aF"},{"Range":["09","0A"]},{"Seq":[]},{"Alt":[{"Terminal":"a"},{"Ref":"b"}]},{"Diff":[{"Ref":"x"},{"Ref":"y"}]},{"Star":{"Ref":"x"}},{"Plus":{"Ref":"x"}},{"Opt":{"Ref":"x"}},{"Repeat":[8,3,{"Ref":"x"}]},{"Group":{"Ref":"x"}},{"Repeat":[null,4294967295,{"Ref":"x"}]}]}}"#;
    assert_eq!(to_vec(&rule), wire.as_bytes());
    assert_eq!(from_slice::<GrammarRule>(wire.as_bytes()).unwrap(), rule);
    for token in ["-0", "-1", "1.0", "1e0", "4294967296", r#""1""#] {
        let bad = format!(r#"{{"Repeat":[{token},null,{{"Ref":"x"}}]}}"#);
        assert_eq!(
            from_slice::<RuleExpr>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/Repeat/0"
        );
    }
}

#[test]
fn native_gmn_and_grammar_admission_refuses_ambiguous_or_incomplete_records() {
    for wire in [
        r#"{"NamedGraphOutOfDomain":{}}"#,
        r#"{"NamedGraphOutOfDomain":{"graph":null}}"#,
        r#"{"NamedGraphOutOfDomain":{"graph":"a","graph":"b"}}"#,
        r#"{"Uncovered":"a","MalformedNumber":{"token":"b"}}"#,
        r#"{"Unknown":"a"}"#,
    ] {
        assert!(from_slice::<Gmn1Error>(wire.as_bytes()).is_err(), "{wire}");
    }
    for wire in [
        r#"{"Range":["a"]}"#,
        r#"{"Range":["a","b","c"]}"#,
        r#"{"Diff":[{"Ref":"x"}]}"#,
        r#"{"Repeat":[1,2]}"#,
        r#"{"Ref":"a","Ref":"b"}"#,
        r#"{"Group":null}"#,
        r#"{"Unknown":[]}"#,
    ] {
        assert!(from_slice::<RuleExpr>(wire.as_bytes()).is_err(), "{wire}");
    }
    for wire in [
        r#"["x",{"Ref":"x"}]"#,
        r#"{"name":"x"}"#,
        r#"{"name":"x","name":"y","body":{"Ref":"x"}}"#,
    ] {
        assert!(
            from_slice::<GrammarRule>(wire.as_bytes()).is_err(),
            "{wire}"
        );
    }
    let extended = br#"{"name":"x","body":{"Group":{"Ref":"x"}},"extension":false}"#;
    assert_eq!(
        to_vec(&from_slice::<GrammarRule>(extended).unwrap()),
        br#"{"name":"x","body":{"Group":{"Ref":"x"}}}"#
    );
}
