// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trust-policy and profile-policy checks stay above cryptographic validity.

use std::path::Path;

use ciborium::value::Value;
use ed25519_dalek::SigningKey;
use gmeow_gts::cose::verify_signatures;
use gmeow_gts::model::{Graph, OpaqueNode, Term, TermKind};
use gmeow_gts::policy::{evaluate_profile_policy, signature_trust, Severity, TrustPolicy};
use gmeow_gts::reader::read;
use gmeow_gts::stream::{COMPACTION, SEALED_SOURCE};
use gmeow_gts::writer::Writer;

const EX: &str = "https://example.org/";
const KID: &str = "did:example:issuer";

fn iri(value: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn lit(value: &str) -> Term {
    Term {
        kind: TermKind::Literal,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn claim_terms() -> Vec<Term> {
    vec![
        iri(&(EX.to_string() + "claim")),
        iri(&(EX.to_string() + "says")),
        lit("the moon is made of cheese"),
    ]
}

fn signed_graph(profile: &str) -> Graph {
    let key = SigningKey::from_bytes(&[3u8; 32]);
    let verifier = key.verifying_key();
    let mut writer = Writer::new(profile);
    writer.sign_with(key, KID);
    writer.add_terms(&claim_terms());
    writer.add_quads(&[(0, 1, 2, None)]);
    let mut graph = read(&writer.to_bytes(), false, None);
    verify_signatures(&mut graph.signatures, |kid| {
        (kid == KID).then_some(verifier)
    });
    graph
}

fn finding_codes(graph: &Graph, policy: Option<&TrustPolicy>) -> Vec<String> {
    evaluate_profile_policy(graph, policy, None)
        .into_iter()
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn valid_signature_does_not_imply_trusted_signer_or_true_claim() {
    let graph = signed_graph("evidence");

    assert!(graph.signatures.iter().all(|sig| sig.status == "valid"));
    assert_eq!(graph.quads, vec![(0, 1, 2, None)]);
    assert!(signature_trust(&graph, None)
        .iter()
        .all(|item| !item.trusted));

    let findings = evaluate_profile_policy(&graph, None, None);
    assert!(findings
        .iter()
        .any(|f| f.code == "ProfileSignerTrustNotEvaluated"));

    let trusted = TrustPolicy::new([KID], true);
    assert!(signature_trust(&graph, Some(&trusted))
        .iter()
        .all(|item| item.trusted));
    assert!(!evaluate_profile_policy(&graph, Some(&trusted), None)
        .iter()
        .any(|f| f.severity == Severity::Error));
}

#[test]
fn trusted_signer_policy_rejects_valid_but_unauthorized_signer() {
    let graph = signed_graph("evidence");
    let policy = TrustPolicy::new(["did:example:someone-else"], true);

    assert!(finding_codes(&graph, Some(&policy))
        .iter()
        .any(|code| code == "ProfileSignerUntrusted"));
}

#[test]
fn invalid_and_unverified_signature_statuses_are_policy_errors() {
    let key = SigningKey::from_bytes(&[4u8; 32]);
    let wrong = SigningKey::from_bytes(&[5u8; 32]).verifying_key();
    let mut writer = Writer::new("evidence");
    writer.sign_with(key, KID);
    writer.add_terms(&claim_terms());
    writer.add_quads(&[(0, 1, 2, None)]);

    let mut invalid = read(&writer.to_bytes(), false, None);
    verify_signatures(&mut invalid.signatures, |_| Some(wrong));
    assert!(finding_codes(&invalid, None)
        .iter()
        .any(|code| code == "ProfileSignatureInvalid"));

    let unverified = read(&writer.to_bytes(), false, None);
    assert!(finding_codes(&unverified, None)
        .iter()
        .any(|code| code == "ProfileSignatureUnverified"));
}

#[test]
fn evidence_profile_requires_signatures_and_head_commitment() {
    let mut writer = Writer::new("evidence");
    writer.add_terms(&claim_terms());
    writer.add_quads(&[(0, 1, 2, None)]);

    let graph = read(&writer.to_bytes(), false, None);
    let codes = finding_codes(&graph, None);
    assert!(codes.iter().any(|code| code == "ProfileSignatureRequired"));
    assert!(codes
        .iter()
        .any(|code| code == "EvidenceHeadCommitmentRequired"));
}

#[test]
fn evidence_sealed_source_satisfies_unsigned_source_commitment_policy() {
    let mut writer = Writer::new("evidence");
    writer.add_terms(&[
        iri(&(EX.to_string() + "rewrite")),
        iri(SEALED_SOURCE),
        lit("source"),
    ]);
    writer.add_quads(&[(0, 1, 2, None)]);

    let graph = read(&writer.to_bytes(), false, None);
    let codes = finding_codes(&graph, None);
    assert!(!codes.iter().any(|code| code == "ProfileSignatureRequired"));
    assert!(!codes
        .iter()
        .any(|code| code == "EvidenceHeadCommitmentRequired"));
}

#[test]
fn opaque_profile_requires_pseudonymous_recipient_kids() {
    let mut graph = Graph {
        segment_profiles: vec!["opaque".to_string()],
        ..Graph::default()
    };
    graph.opaque.push(OpaqueNode {
        id: Vec::new(),
        frame_type: "meta".to_string(),
        reason: "missing-key".to_string(),
        sigstat: "unverified".to_string(),
        pub_meta: None,
        recipients: Some(vec![Value::Map(vec![("kid".into(), "did:court".into())])]),
    });

    let findings = evaluate_profile_policy(&graph, None, None);
    assert!(findings
        .iter()
        .any(|f| f.code == "OpaqueRecipientKidPublic"));

    graph.opaque[0].recipients = Some(vec![Value::Map(vec![(
        "kid".into(),
        ("anon:".to_string() + &"a".repeat(32)).into(),
    )])]);
    let findings = evaluate_profile_policy(&graph, None, None);
    assert!(!findings
        .iter()
        .any(|f| f.code == "OpaqueRecipientKidPublic"));

    let custom_policy = TrustPolicy {
        pseudonymous_kid_pattern: "^did:court$".to_string(),
        ..TrustPolicy::default()
    };
    graph.opaque[0].recipients = Some(vec![Value::Map(vec![("kid".into(), "did:court".into())])]);
    let findings = evaluate_profile_policy(&graph, Some(&custom_policy), None);
    assert!(!findings
        .iter()
        .any(|f| f.code == "OpaqueRecipientKidPublic"));

    graph.opaque[0].recipients = Some(vec![Value::Map(vec![("kid".into(), "did:other".into())])]);
    let findings = evaluate_profile_policy(&graph, Some(&custom_policy), None);
    assert!(findings
        .iter()
        .any(|f| f.code == "OpaqueRecipientKidPublic"));
}

#[test]
fn profile_and_stream_vocabulary_findings_match_policy_codes() {
    let mut files = Writer::new("generic");
    files.add_terms(&[
        iri("https://w3id.org/gts/files#FileEntry"),
        iri(&(EX.to_string() + "relatedTo")),
        lit("x.txt"),
    ]);
    files.add_quads(&[(0, 1, 2, None)]);
    assert!(finding_codes(&read(&files.to_bytes(), false, None), None)
        .iter()
        .any(|code| code == "ProfileVocabularyUndeclared"));

    let mut stream = Writer::new("generic");
    stream.add_terms(&[
        iri(&(EX.to_string() + "rewrite")),
        iri(COMPACTION),
        lit("agent"),
    ]);
    stream.add_quads(&[(0, 1, 2, None)]);
    assert!(finding_codes(&read(&stream.to_bytes(), false, None), None)
        .iter()
        .any(|code| code == "StreamVocabularyWithoutLayout"));

    let mut claimed_stream = Writer::with_layout("generic", Some("streamable"));
    claimed_stream.add_terms(&[
        iri(&(EX.to_string() + "rewrite")),
        iri(COMPACTION),
        lit("agent"),
    ]);
    claimed_stream.add_quads(&[(0, 1, 2, None)]);
    assert!(
        !finding_codes(&read(&claimed_stream.to_bytes(), false, None), None)
            .iter()
            .any(|code| code == "StreamVocabularyWithoutLayout")
    );
}

#[test]
fn profile_policy_security_vector_descriptor_is_still_present() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors/security/profile-policy.json");
    let vector: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(vector["id"], "profile-policy");
    assert!(vector["expected_findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|code| code == "OpaqueRecipientKidPublic"));
}

#[cfg(feature = "policy-config")]
#[test]
fn trust_policy_loads_from_json() {
    let json = r#"{
      "trusted_signers": ["did:example:issuer"],
      "require_trusted_signer": true,
      "pseudonymous_kid_pattern": "^did:example:recipient$"
    }"#;
    let policy = TrustPolicy::from_json_str(json).unwrap();
    assert!(policy.require_trusted_signer);
    assert!(policy.is_trusted(Some("did:example:issuer")));
    assert!(policy.is_pseudonymous_recipient("did:example:recipient"));
    assert!(policy.to_json_string().unwrap().contains("trusted_signers"));
}

#[cfg(feature = "policy-config-yaml")]
#[test]
fn trust_policy_loads_from_yaml() {
    let yaml = "\
trusted_signers:
  - did:example:issuer
require_trusted_signer: true
";
    let policy = TrustPolicy::from_yaml_str(yaml).unwrap();
    assert!(policy.require_trusted_signer);
    assert!(policy.is_trusted(Some("did:example:issuer")));
    assert!(policy.is_pseudonymous_recipient(&format!("anon:{}", "a".repeat(32))));
    assert!(policy.to_yaml_string().unwrap().contains("trusted_signers"));
}
