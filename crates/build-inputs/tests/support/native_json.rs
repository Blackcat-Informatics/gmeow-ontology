// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

pub fn original(name: &str) -> (&'static str, Vec<u8>) {
    let references = include_str!("../fixtures/native-json-v1.txt");
    assert_eq!(references.lines().count(), 11);
    let row = references
        .lines()
        .find(|row| row.split('|').next() == Some(name))
        .unwrap();
    let mut fields = row.splitn(3, '|');
    assert_eq!(fields.next().unwrap(), name);
    let identity = fields.next().unwrap();
    let bytes = purrdf_hash::hex::decode(fields.next().unwrap()).unwrap();
    assert_eq!(gmeow_build_inputs::digest(&bytes), identity);
    (identity, bytes)
}

pub fn assert_reference(name: &str, bytes: &[u8], identity: &str) {
    let expected = original(name);
    assert_eq!(identity, expected.0, "{name}");
    assert_eq!(bytes, expected.1, "{name}");
}
