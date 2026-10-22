// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const BUILD: &str = r#"{"fingerprint":"中\n\"\\","toolchain":"declared-toolchain","target":"declared-target","profile":"declared-profile","features":["z","a","z"]}"#;
#[test]
fn json_build_identity_literal_preserves_ordered_features_and_genuine_serde_bytes() {
    let identity = BuildIdentity {
        fingerprint: "中\n\"\\".into(),
        toolchain: "declared-toolchain".into(),
        target: "declared-target".into(),
        profile: "declared-profile".into(),
        features: vec!["z".into(), "a".into(), "z".into()],
    };
    assert_eq!(to_vec(&identity), BUILD.as_bytes());
    assert_eq!(serde_json::to_vec(&identity).unwrap(), BUILD.as_bytes());
    assert_eq!(
        from_slice::<BuildIdentity>(BUILD.as_bytes()).unwrap(),
        identity
    );
    let unknown = BUILD.replacen('{', "{\"extra\":false,", 1);
    assert_eq!(
        to_vec(&from_slice::<BuildIdentity>(unknown.as_bytes()).unwrap()),
        BUILD.as_bytes()
    );
}
#[test]
fn json_build_identity_requires_five_fields_and_refuses_duplicates_and_wrong_types() {
    for field in ["fingerprint", "toolchain", "target", "profile", "features"] {
        let bad = BUILD.replacen(
            &format!("\"{field}\":"),
            &format!("\"{field}\":null,\"{field}\":"),
            1,
        );
        assert_eq!(
            from_slice::<BuildIdentity>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            format!("/{field}")
        );
    }
    for bad in [
        "{}".to_owned(),
        "[]".into(),
        BUILD.replace("\"toolchain\":\"declared-toolchain\",", ""),
        BUILD.replace("\"target\":\"declared-target\"", "\"target\":null"),
        BUILD.replace("\"features\":[\"z\",\"a\",\"z\"]", "\"features\":null"),
        format!("{BUILD} []"),
    ] {
        assert!(
            from_slice::<BuildIdentity>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad = BUILD.replace("[\"z\",\"a\",\"z\"]", "[\"z\",42]");
    assert_eq!(
        from_slice::<BuildIdentity>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/features/1"
    );
}
