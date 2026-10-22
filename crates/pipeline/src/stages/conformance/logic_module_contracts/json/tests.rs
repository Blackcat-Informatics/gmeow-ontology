// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const BUILD: &str =
    r#"{"fingerprint":"f","toolchain":"t","target":"x","profile":"p","features":["z","a","z"]}"#;
const ATOM: &str = r#"{"Atom":{"relation":{"Iri":"urn:p"},"args":[{"Var":"x"}]}}"#;
const EMPTY: &str = r#"{"producer":{"fingerprint":"f","toolchain":"t","target":"x","profile":"p","features":["z","a","z"]},"source_path":"module.ttl","source_digest":"digest","source_iri":"urn:source","diagnostics":[],"declared_completeness_roots":[],"reconstructed":{},"top_level_formulas":[],"domain_axiom_predicates":[],"grounding_laws":{}}"#;
#[test]
fn json_logic_module_complete_literal_preserves_ast_vectors_sets_and_endpoints() {
    let atom = Formula::atom(
        gmeow_logic_compile::ir::Term::iri("urn:p").unwrap(),
        vec![gmeow_logic_compile::ir::Term::var("x").unwrap()],
    )
    .unwrap();
    let observed = Observations {
        producer: crate::cache::BuildIdentity {
            fingerprint: "f".into(),
            toolchain: "t".into(),
            target: "x".into(),
            profile: "p".into(),
            features: vec!["z".into(), "a".into(), "z".into()],
        },
        source_path: "module.ttl".into(),
        source_digest: "digest".into(),
        source_iri: "urn:source".into(),
        diagnostics: vec![gmeow_logic_compile::frontend::Diagnostic {
            severity: gmeow_logic_compile::frontend::Severity::Info,
            code: "AUTHORED".into(),
            message: "中".into(),
            subject: None,
        }],
        declared_completeness_roots: ["z".into(), "a".into()].into(),
        reconstructed: [("a".into(), Ok(atom.clone()))].into(),
        top_level_formulas: vec![Formula::Not(Box::new(atom.clone())), atom.clone(), atom],
        domain_axiom_predicates: ["z".into(), "a".into()].into(),
        grounding_laws: [(
            "law".into(),
            Endpoints {
                sources: vec!["z".into(), "a".into(), "z".into()],
                targets: vec!["b".into(), "a".into()],
            },
        )]
        .into(),
    };
    let expected = format!(
        "{{\"producer\":{BUILD},\"source_path\":\"module.ttl\",\"source_digest\":\"digest\",\"source_iri\":\"urn:source\",\"diagnostics\":[{{\"severity\":\"Info\",\"code\":\"AUTHORED\",\"message\":\"中\",\"subject\":null}}],\"declared_completeness_roots\":[\"a\",\"z\"],\"reconstructed\":{{\"a\":{{\"Ok\":{ATOM}}}}},\"top_level_formulas\":[{{\"Not\":{ATOM}}},{ATOM},{ATOM}],\"domain_axiom_predicates\":[\"a\",\"z\"],\"grounding_laws\":{{\"law\":{{\"sources\":[\"z\",\"a\",\"z\"],\"targets\":[\"b\",\"a\"]}}}}}}"
    );
    assert_eq!(to_vec(&observed), expected.as_bytes());
    let restored: Observations = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(restored.top_level_formulas, observed.top_level_formulas);
    assert_eq!(to_vec(&restored), expected.as_bytes());
    let unknown = expected.replacen('{', "{\"unknown\":false,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observations>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_logic_module_required_endpoints_and_typed_results_fail_at_exact_members() {
    assert_eq!(
        to_vec(&Endpoints::default()),
        br#"{"sources":[],"targets":[]}"#
    );
    for bad in [
        "{}",
        "[]",
        r#"{"sources":[]}"#,
        r#"{"sources":null,"targets":[]}"#,
        r#"{"sources":[],"targets":[],"targets":[]}"#,
    ] {
        assert!(from_slice::<Endpoints>(bad.as_bytes()).is_err(), "{bad}");
    }
    for bad in [
        "{}".into(),
        "[]".into(),
        EMPTY.replace("\"source_path\":\"module.ttl\",", ""),
        EMPTY.replace("\"source_iri\":\"urn:source\"", "\"source_iri\":null"),
        EMPTY.replace(
            "\"diagnostics\":[]",
            "\"diagnostics\":[],\"diagnostics\":[]",
        ),
        format!("{EMPTY} {{}}"),
    ] {
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    let bad=EMPTY.replace("\"reconstructed\":{}",r#""reconstructed":{"a":{"Ok":{"Atom":{"relation":{"Iri":"urn:p"},"args":[{"Var":null}]}}}}"#);
    assert_eq!(
        from_slice::<Observations>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/reconstructed/a/Ok/Atom/args/0/Var"
    );
    // Map values are admitted before replacement; invalid earlier values cannot disappear.
    let invalid_earlier = EMPTY.replace(
        "\"reconstructed\":{}",
        r#""reconstructed":{"a":{"Ok":null},"a":{"Ok":{"And":[]}}}"#,
    );
    assert_eq!(
        from_slice::<Observations>(invalid_earlier.as_bytes())
            .unwrap_err()
            .pointer(),
        "/reconstructed/a/Ok"
    );
    let valid_last = EMPTY.replace(
        "\"reconstructed\":{}",
        r#""reconstructed":{"a":{"Ok":{"And":[]}},"a":{"Ok":{"Or":[]}}}"#,
    );
    let restored: Observations = from_slice(valid_last.as_bytes()).unwrap();
    assert_eq!(restored.reconstructed.len(), 1);
    assert_eq!(
        restored.reconstructed["a"].as_ref().unwrap(),
        &Formula::Or(vec![])
    );
    let duplicate_tag = EMPTY.replace(
        "\"reconstructed\":{}",
        r#""reconstructed":{"a":{"Ok":{"And":[]},"Ok":{"Or":[]}}}"#,
    );
    assert_eq!(
        from_slice::<Observations>(duplicate_tag.as_bytes())
            .unwrap_err()
            .pointer(),
        "/reconstructed/a"
    );
    let bad = EMPTY.replace(
        "\"grounding_laws\":{}",
        r#""grounding_laws":{"a":{"sources":[],"targets":null}}"#,
    );
    assert_eq!(
        from_slice::<Observations>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/grounding_laws/a/targets"
    );
}
#[test]
fn json_logic_module_reconstruction_refusal_preserves_causal_graph_and_replay() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("authored premise"))
            .with_focus("urn:logic-module:premise"),
        StageId::new("input"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("reconstruction refusal"))
            .with_focus("urn:logic-module:reconstruct")
            .with_antecedents([premise]),
        StageId::new("reconstruct"),
    );
    assert_ne!(refusal.root.fingerprint, refusal.causes[0].fingerprint);
    let mut observed: Observations = from_slice(EMPTY.as_bytes()).unwrap();
    observed
        .reconstructed
        .insert("bad".into(), Err(refusal.clone()));
    let restored: Observations = from_slice(&to_vec(&observed)).unwrap();
    assert_eq!(
        restored.reconstructed["bad"].as_ref().unwrap_err(),
        &refusal
    );
    let mut replay = DiagLedger::new();
    replay.replay(refusal.causes);
    replay.replay([*refusal.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}
