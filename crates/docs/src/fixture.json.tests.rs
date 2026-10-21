// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored sites and JSON only: no rendering, discovery, action store or corpus.

use super::*;

// Independently supplied SHA-1 of the exact authored domain/label/framing/payload bytes.
const DIGEST: &str = "9eb22eb612249bac6ae424a0189a7db696d0faa3";
const ENVELOPE: &str =
    r#"{"digest":"9eb22eb612249bac6ae424a0189a7db696d0faa3","files":{"index.html":"hi"}}"#;

fn authored() -> Site {
    Site {
        files: BTreeMap::from([("index.html".to_owned(), b"hi".to_vec())]),
    }
}

fn error(raw: &[u8]) -> DecodeError {
    match json::record::from_slice::<CachedSite>(raw) {
        Ok(_) => panic!("invalid cache envelope admitted"),
        Err(error) => error,
    }
}

#[test]
fn json_cached_site_literal_envelope_pins_shared_digest_and_compact_order() {
    let cached = CachedSite::from_site(&authored());
    assert_eq!(cached.digest, DIGEST);
    let encoded = json::write_compact(&cached.to_json());
    assert_eq!(encoded, ENVELOPE);
    assert!(!encoded.ends_with('\n'));
    assert_eq!(
        json::record::to_vec(&cached.files).as_slice(),
        br#"{"index.html":"hi"}"#
    );
    assert_eq!(payload_digest("site", br#"{"index.html":"hi"}"#), DIGEST);
    assert_ne!(payload_digest("book", br#"{"index.html":"hi"}"#), DIGEST);
}

#[test]
fn json_cached_site_roundtrip_preserves_unicode_controls_empty_files_and_sorted_names() {
    let site = Site {
        files: BTreeMap::from([
            (
                "猫/\"x".to_owned(),
                "é☃\n\r\t\u{0000}\u{0008}\u{000c}\\/\"\u{2028}"
                    .as_bytes()
                    .to_vec(),
            ),
            ("".to_owned(), Vec::new()),
            ("a".to_owned(), b"a".to_vec()),
        ]),
    };
    let cached = CachedSite::from_site(&site);
    assert_eq!(
        String::from_utf8(json::record::to_vec(&cached.files)).unwrap(),
        concat!(
            r#"{"":"","a":"a","猫/\"x":"é☃\n\r\t\u0000\b\f\\/\""#,
            "\u{2028}",
            r#""}"#
        )
    );
    let encoded = json::write_compact(&cached.to_json());
    let decoded: CachedSite = json::record::from_slice(encoded.as_bytes()).unwrap();
    assert_eq!(decoded.into_site(Path::new("<authored-unicode>")), site);
    let empty = Site {
        files: BTreeMap::new(),
    };
    let bytes = json::write_compact(&CachedSite::from_site(&empty).to_json());
    let decoded: CachedSite = json::record::from_slice(bytes.as_bytes()).unwrap();
    assert_eq!(decoded.into_site(Path::new("<authored-empty>")), empty);
}

#[test]
fn json_cached_site_literal_string_escaping_matches_canonical_payload_bytes() {
    const TEXT: &str = "\"\\\u{0008}\u{000c}\n\r\t\u{0000}/\u{007f}\u{0085}\u{2028}\u{2029}🦀";
    let files = BTreeMap::from([
        (format!("k{TEXT}"), TEXT.to_owned()),
        ("a".to_owned(), String::new()),
    ]);
    assert_eq!(
        String::from_utf8(json::record::to_vec(&files)).unwrap(),
        concat!(
            r#"{"a":"","k\"\\\b\f\n\r\t\u0000/"#,
            "\u{007f}\u{0085}\u{2028}\u{2029}🦀",
            r#"":"\"\\\b\f\n\r\t\u0000/"#,
            "\u{007f}\u{0085}\u{2028}\u{2029}🦀",
            r#""}"#,
        )
    );
}

#[test]
fn json_cached_site_required_fields_and_all_record_kinds_refuse_without_coercion() {
    for text in [
        "null",
        "true",
        "1",
        "\"site\"",
        "[]",
        "[\"digest\",{}]",
        "{}",
        r#"{"digest":"x"}"#,
        r#"{"files":{}}"#,
        r#"{"digest":null,"files":{}}"#,
        r#"{"digest":1,"files":{}}"#,
        r#"{"digest":false,"files":{}}"#,
        r#"{"digest":[],"files":{}}"#,
        r#"{"digest":{},"files":{}}"#,
        r#"{"digest":"x","files":null}"#,
        r#"{"digest":"x","files":[]}"#,
        r#"{"digest":"x","files":false}"#,
        r#"{"digest":"x","files":1}"#,
        r#"{"digest":"x","files":"map"}"#,
    ] {
        assert!(
            json::record::from_slice::<CachedSite>(text.as_bytes()).is_err(),
            "{text}"
        );
    }
}

#[test]
fn json_cached_site_file_values_are_strict_strings_with_native_member_pointers() {
    for token in ["null", "true", "1", "1e400", "[]", "{}"] {
        let text = format!(r#"{{"digest":"x","files":{{"a/b":{token}}}}}"#);
        assert_eq!(error(text.as_bytes()).pointer(), "/files/a~1b");
    }
}

#[test]
fn json_cached_site_known_duplicates_refuse_and_file_map_duplicates_keep_last_string() {
    for (text, pointer) in [
        (r#"{"digest":"x","digest":"y","files":{}}"#, "/digest"),
        (r#"{"digest":"x","files":{},"files":{}}"#, "/files"),
    ] {
        assert_eq!(error(text.as_bytes()).pointer(), pointer);
    }
    let text =
        format!(r#"{{"digest":"{DIGEST}","files":{{"index.html":"old","index.html":"hi"}}}}"#);
    let decoded: CachedSite = json::record::from_slice(text.as_bytes()).unwrap();
    assert_eq!(json::write_compact(&decoded.to_json()), ENVELOPE);
    assert_eq!(
        decoded.into_site(Path::new("<authored-duplicates>")),
        authored()
    );
}

#[test]
fn json_cached_site_whitespace_escapes_and_ignored_members_do_not_change_payload_identity() {
    let text = format!(
        r#" {{ "files" : {{ "index.html" : "\u0068i" }}, "ignored":{{"number":1e400}}, "digest" : "{DIGEST}" }}
"#
    );
    let decoded: CachedSite = json::record::from_slice(text.as_bytes()).unwrap();
    assert_eq!(json::write_compact(&decoded.to_json()), ENVELOPE);
    assert_eq!(
        decoded.into_site(Path::new("<authored-formatting>")),
        authored()
    );
}

#[test]
#[should_panic(expected = "tampered docs-fixture site cache at <authored-tamper>")]
fn json_cached_site_changed_payload_never_reconstructs_with_stale_digest() {
    let text = ENVELOPE.replace("\"hi\"", "\"changed\"");
    let decoded: CachedSite = json::record::from_slice(text.as_bytes()).unwrap();
    let _ = decoded.into_site(Path::new("<authored-tamper>"));
}

#[test]
#[should_panic(expected = "tampered docs-fixture site cache at <authored-digest>")]
fn json_cached_site_changed_declared_digest_never_reconstructs_valid_payload() {
    let text = ENVELOPE.replace(DIGEST, "wrong");
    let decoded: CachedSite = json::record::from_slice(text.as_bytes()).unwrap();
    let _ = decoded.into_site(Path::new("<authored-digest>"));
}

#[test]
fn json_cached_site_malformed_trailing_and_invalid_unicode_documents_refuse() {
    for raw in [
        &b""[..],
        &b"{"[..],
        &b"\xff"[..],
        &b"\xef\xbb\xbf{}"[..],
        &br#"{"digest":"\uD800","files":{}}"#[..],
    ] {
        assert!(json::record::from_slice::<CachedSite>(raw).is_err());
    }
    for suffix in [" {}", " null", "x", ","] {
        assert!(
            json::record::from_slice::<CachedSite>(format!("{ENVELOPE}{suffix}").as_bytes())
                .is_err()
        );
    }
}

#[test]
#[should_panic(expected = "rendered site file bad.bin is not UTF-8")]
fn json_cached_site_non_utf8_file_is_refused_before_caching() {
    let site = Site {
        files: BTreeMap::from([("bad.bin".to_owned(), vec![0xff])]),
    };
    let _ = CachedSite::from_site(&site);
}
