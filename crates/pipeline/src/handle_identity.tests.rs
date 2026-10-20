// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::typed_digest;

#[test]
fn persisted_payload_identity_vectors() {
    let small = serde_json::json!({
        "kind": "typed-product",
        "value": u64::MAX,
        "label": "RDF λ",
        "selected": true,
        "absent": null,
    });
    let large = serde_json::json!({
        "rows": [small.clone()],
        "text": "RDF λ\0𝄞".repeat(4097),
    });
    let actual = [typed_digest(&small), typed_digest(&large)];
    assert_eq!(
        actual,
        [
            "5b6ec069af9fc0e442f343053b73d9261904371272ff2c34bb246585d78ab625",
            "b66ca0e37f9260dd069b14af34f2c6fef204940c940c0d70c5e4b3cbd8fdcb23",
        ]
    );
}
