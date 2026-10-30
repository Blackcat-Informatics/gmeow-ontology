// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn selector_admission_requires_the_supplied_digest_and_current_schema() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selector.json");
    let bytes = br#"{"schema_version":2,"fixture":"selected"}"#;
    std::fs::write(&path, bytes).unwrap();
    let digest = format!("{:x}", Sha256::digest(bytes));
    let admitted: Value = read_manifest(&path, &digest).unwrap();
    assert_eq!(admitted["fixture"], "selected");
    assert!(read_manifest::<Value>(&path, &"0".repeat(64)).is_err());
    assert!(read_manifest::<Value>(&path, "").is_err());
    let stale = br#"{"schema_version":1,"fixture":"selected"}"#;
    std::fs::write(&path, stale).unwrap();
    let stale_digest = format!("{:x}", Sha256::digest(stale));
    assert!(read_manifest::<Value>(&path, &stale_digest).is_err());
    let malformed = b"not JSON";
    std::fs::write(&path, malformed).unwrap();
    let error = read_manifest::<Value>(&path, &"0".repeat(64)).unwrap_err();
    assert!(error.to_string().contains("identity mismatch"));
    let duplicate = br#"{"schema_version":2,"schema_version":2}"#;
    std::fs::write(&path, duplicate).unwrap();
    let duplicate_digest = format!("{:x}", Sha256::digest(duplicate));
    assert!(read_manifest::<Value>(&path, &duplicate_digest).is_err());
    std::fs::remove_file(path).unwrap();
    assert!(read_manifest::<Value>(&directory.path().join("selector.json"), &digest).is_err());
}
