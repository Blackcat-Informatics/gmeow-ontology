// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn json_gmn_verdict_producer_adapter_sorts_paths_and_preserves_record_order() {
    let mut verdicts = BTreeMap::new();
    verdicts.insert(
        "z/猫.gmn".to_owned(),
        GmnValidationVerdict {
            conformant: true,
            detail: None,
            failure_class: None,
        },
    );
    verdicts.insert(
        "a/é.gmn".to_owned(),
        GmnValidationVerdict {
            conformant: false,
            detail: Some("bad\n\"glyph\"".to_owned()),
            failure_class: Some("urn:failure".to_owned()),
        },
    );
    let bytes = verdict_map_bytes(&verdicts);
    let expected = "{\"a/é.gmn\":{\"conformant\":false,\"detail\":\"bad\\n\\\"glyph\\\"\",\"failureClass\":\"urn:failure\"},\"z/猫.gmn\":{\"conformant\":true}}";
    assert_eq!(bytes, expected.as_bytes());
    assert!(!bytes.ends_with(b"\n"));
}
