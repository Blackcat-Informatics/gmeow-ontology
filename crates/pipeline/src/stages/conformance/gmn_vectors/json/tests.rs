// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const POSITIVE: &str = r#"{"text":"t","source":"s","reconstructed":{"Ok":"s"},"content_digest":"d","per_claim":{"Ok":null},"idempotence":{"Ok":null}}"#;
#[test]
fn json_conformance_vectors_literal_complete_map_set_and_option_order() {
    let positive: PositiveObservation = from_slice(POSITIVE.as_bytes()).unwrap();
    let observed = Observation {
        codebook_digest: "d".into(),
        declared_digests: ["z".into(), "a".into()].into(),
        manifest_stems: ["p".into()].into(),
        positives: [("p".into(), Ok(positive))].into(),
        expected_negatives: [("z".into(), "Z".into()), ("a".into(), "A".into())].into(),
        negatives: [("z".into(), Ok(Some("中".into()))), ("a".into(), Ok(None))].into(),
    };
    let expected = format!(
        "{{\"codebook_digest\":\"d\",\"declared_digests\":[\"a\",\"z\"],\"manifest_stems\":[\"p\"],\"positives\":{{\"p\":{{\"Ok\":{POSITIVE}}}}},\"expected_negatives\":{{\"a\":\"A\",\"z\":\"Z\"}},\"negatives\":{{\"a\":{{\"Ok\":null}},\"z\":{{\"Ok\":\"中\"}}}}}}"
    );
    assert_eq!(to_vec(&observed), expected.as_bytes());
    assert_eq!(
        to_vec(&from_slice::<Observation>(expected.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    let unknown = expected.replacen('{', "{\"extra\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observation>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    for bad in [
        "{}".into(),
        "[]".into(),
        expected.replace("\"manifest_stems\":[\"p\"],", ""),
        expected.replace("\"positives\":{", "\"positives\":null,\"positives\":{"),
        expected.replace("\"negatives\":{", "\"negatives\":null,\"negatives\":{"),
    ] {
        assert!(from_slice::<Observation>(bad.as_bytes()).is_err(), "{bad}");
    }
    let bad = expected.replace(
        "\"a\":{\"Ok\":null}",
        "\"a\":{\"Ok\":42},\"a\":{\"Ok\":null}",
    );
    assert_eq!(
        from_slice::<Observation>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/negatives/a/Ok"
    );
    let bad = expected.replace(
        &format!("\"p\":{{\"Ok\":{POSITIVE}}}"),
        &format!("\"p\":{{\"Ok\":null}},\"p\":{{\"Ok\":{POSITIVE}}}"),
    );
    assert_eq!(
        from_slice::<Observation>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/positives/p/Ok"
    );
}
#[test]
fn json_conformance_vectors_outer_errors_keep_the_complete_recorded_refusal() {
    let failure = gmeow_errors::DiagLedger::new().record(
        gmeow_errors::Diag::from(std::io::Error::other("vector operational failure"))
            .with_focus("urn:outer-vector"),
        gmeow_errors::StageId::new("vector"),
    );
    let observed = Observation {
        codebook_digest: "d".into(),
        declared_digests: Default::default(),
        manifest_stems: Default::default(),
        positives: [("bad".into(), Err(failure.clone()))].into(),
        expected_negatives: Default::default(),
        negatives: [("bad".into(), Err(failure.clone()))].into(),
    };
    let restored: Observation = from_slice(&to_vec(&observed)).unwrap();
    assert_eq!(restored.positives["bad"].as_ref().unwrap_err(), &failure);
    assert_eq!(restored.negatives["bad"].as_ref().unwrap_err(), &failure);
}
