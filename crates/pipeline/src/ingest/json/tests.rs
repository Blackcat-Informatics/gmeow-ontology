// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored span records and tiny blob products only; no source/corpus producer.

use super::*;
use crate::node::StageProduct;
use purrdf_lex::json::record::from_slice;

const EMPTY: &[u8] = br#"{"paths":[],"entries":[]}"#;
const COMPLETE: &str = r#"{"paths":["z/中😀\n\"\\.ttl","a/é.ttl"],"entries":[{"subject":"urn:a","path_id":0,"line":18446744073709551615,"column":4294967295,"byte_offset":18446744073709551615},{"subject":"urn:b","path_id":1,"line":2,"column":3,"byte_offset":4},{"subject":"urn:c","path_id":0,"line":5,"column":6,"byte_offset":7}]}"#;
const ENTRY: &str = r#"{"subject":"urn:s","path_id":0,"line":1,"column":2,"byte_offset":3}"#;

fn authored_index() -> SpanIndex {
    let mut index = SpanIndex::new();
    let z: Arc<str> = Arc::from("z/中😀\n\"\\.ttl");
    index.insert("urn:c", SourceSpan::new(z.clone(), 5, 6, 7));
    index.insert("urn:b", SourceSpan::new(Arc::from("a/é.ttl"), 2, 3, 4));
    index.insert("urn:a", SourceSpan::new(z, u64::MAX, u32::MAX, u64::MAX));
    index
}

fn blob_product(bytes: Option<Vec<u8>>) -> StageProduct {
    let dataset = Arc::new(purrdf::RdfDataset::union(&[]).unwrap());
    match bytes {
        Some(bytes) => StageProduct::from_bundle(
            "stage-source-load",
            Arc::new(crate::bundle::bundle_from_artifacts_over_with_rep_blob(
                dataset,
                BTreeMap::new(),
                purrdf::provenance::DatasetProvenance::new(),
                crate::stages::carrier::REP_SPAN_TABLE,
                "application/json",
                bytes,
            )),
        )
        .expect("the stage carrier digests"),
        None => StageProduct::from_artifacts_over("stage-source-load", dataset, BTreeMap::new())
            .expect("the stage carrier digests"),
    }
}

#[test]
fn json_span_literal_bytes_pin_subject_path_and_field_order_without_lf() {
    let index = authored_index();
    assert_eq!(index.to_json_bytes().unwrap(), COMPLETE.as_bytes());
    assert_eq!(SpanIndex::new().to_json_bytes().unwrap(), EMPTY);
    assert_eq!(from_slice::<SpanIndex>(EMPTY).unwrap(), SpanIndex::new());
    let decoded: SpanIndex = from_slice(COMPLETE.as_bytes()).unwrap();
    assert_eq!(decoded, index);
    assert_eq!(decoded.to_json_bytes().unwrap(), COMPLETE.as_bytes());
    assert!(Arc::ptr_eq(
        &decoded.lookup("urn:a").unwrap().path,
        &decoded.lookup("urn:c").unwrap().path
    ));
    assert!(!Arc::ptr_eq(
        &decoded.lookup("urn:a").unwrap().path,
        &decoded.lookup("urn:b").unwrap().path
    ));
}

#[test]
fn json_span_record_required_null_duplicate_and_unknown_policy() {
    for input in [
        "null",
        "[]",
        "{}",
        r#"{"paths":[]}"#,
        r#"{"entries":[]}"#,
        r#"{"paths":null,"entries":[]}"#,
        r#"{"paths":[],"entries":null}"#,
        r#"{"paths":[false],"entries":[]}"#,
        r#"{"paths":[],"entries":[false]}"#,
        r#"{"paths":[],"paths":[],"entries":[]}"#,
        r#"{"paths":[],"entries":[],"entries":[]}"#,
        r#"{"paths":[],"entries":[]} null"#,
        "{",
    ] {
        assert!(
            from_slice::<SpanIndex>(input.as_bytes()).is_err(),
            "{input}"
        );
    }
    let unknown = br#"{"paths":[],"entries":[],"unknown":1e400,"unknown":null}"#;
    assert_eq!(
        from_slice::<SpanIndex>(unknown)
            .unwrap()
            .to_json_bytes()
            .unwrap(),
        EMPTY
    );
    for field in ["subject", "path_id", "line", "column", "byte_offset"] {
        let value: purrdf_lex::json::Value = from_slice(ENTRY.as_bytes()).unwrap();
        let mut missing = value.as_object().unwrap().clone();
        let _ = missing.remove(field);
        assert!(
            SpanEntryWire::from_json(&Value::Object(missing)).is_err(),
            "{field}"
        );
        let duplicate = ENTRY.replacen('{', &format!("{{\"{field}\":null,"), 1);
        assert!(
            from_slice::<SpanEntryWire>(duplicate.as_bytes()).is_err(),
            "{field}"
        );
    }
    for input in [
        "null",
        "[]",
        r#"["urn:s",0,1,2,3]"#,
        r#"{"subject":null,"path_id":0,"line":1,"column":2,"byte_offset":3}"#,
    ] {
        assert!(
            from_slice::<SpanEntryWire>(input.as_bytes()).is_err(),
            "{input}"
        );
    }
    let unknown = ENTRY.replacen('{', "{\"unknown\":false,", 1);
    assert_eq!(
        purrdf_lex::json::record::to_vec(&from_slice::<SpanEntryWire>(unknown.as_bytes()).unwrap()),
        ENTRY.as_bytes()
    );
}

#[test]
fn json_span_unsigned_fields_refuse_coercion_and_preserve_width_boundaries() {
    let entry: SpanEntryWire = from_slice(br#"{"subject":"s","path_id":4294967295,"line":18446744073709551615,"column":4294967295,"byte_offset":18446744073709551615}"#).unwrap();
    assert_eq!(entry.path_id, u32::MAX);
    assert_eq!(entry.line, u64::MAX);
    assert_eq!(entry.column, u32::MAX);
    assert_eq!(entry.byte_offset, u64::MAX);
    for field in ["path_id", "line", "column", "byte_offset"] {
        for token in [
            "null",
            "false",
            "\"1\"",
            "-0",
            "-1",
            "1.0",
            "1e0",
            "18446744073709551616",
        ] {
            let input = ENTRY.to_owned();
            let old = match field {
                "path_id" => "\"path_id\":0",
                "line" => "\"line\":1",
                "column" => "\"column\":2",
                _ => "\"byte_offset\":3",
            };
            let input = input.replace(old, &format!("\"{field}\":{token}"));
            let error =
                from_slice::<SpanEntryWire>(input.as_bytes()).expect_err("invalid coordinate");
            assert_eq!(error.pointer(), format!("/{field}"));
        }
    }
    for field in ["path_id", "column"] {
        let input = ENTRY.replace(
            if field == "path_id" {
                "\"path_id\":0"
            } else {
                "\"column\":2"
            },
            &format!("\"{field}\":4294967296"),
        );
        assert!(from_slice::<SpanEntryWire>(input.as_bytes()).is_err());
    }
}

#[test]
fn json_span_path_bounds_have_nested_pointer_and_no_empty_fallback() {
    for paths in ["[]", "[\"only\"]"] {
        let input = format!(
            "{{\"paths\":{paths},\"entries\":[{{\"subject\":\"s\",\"path_id\":1,\"line\":0,\"column\":0,\"byte_offset\":0}}]}}"
        );
        let error = from_slice::<SpanIndex>(input.as_bytes()).unwrap_err();
        assert_eq!(error.pointer(), "/entries/0/path_id");
        assert!(error.to_string().contains("path_id 1 out of range"));
    }
}

#[test]
fn json_span_duplicate_subject_is_last_wins_and_duplicate_path_slots_stay_distinct() {
    let input = br#"{"paths":["same","same"],"entries":[{"subject":"s","path_id":0,"line":1,"column":0,"byte_offset":0},{"subject":"s","path_id":1,"line":2,"column":0,"byte_offset":0},{"subject":"t","path_id":1,"line":3,"column":0,"byte_offset":0},{"subject":"u","path_id":0,"line":4,"column":0,"byte_offset":0}]}"#;
    let index: SpanIndex = from_slice(input).unwrap();
    assert_eq!(index.len(), 3);
    assert_eq!(index.lookup("s").unwrap().line, 2);
    assert!(Arc::ptr_eq(
        &index.lookup("s").unwrap().path,
        &index.lookup("t").unwrap().path
    ));
    assert!(!Arc::ptr_eq(
        &index.lookup("s").unwrap().path,
        &index.lookup("u").unwrap().path
    ));
    let mut inserted = SpanIndex::new();
    inserted.insert("s", SourceSpan::new(Arc::from("first"), 1, 0, 0));
    inserted.insert("s", SourceSpan::new(Arc::from("second"), 2, 0, 0));
    assert_eq!(inserted.lookup("s").unwrap().line, 1);
}

#[test]
fn json_span_writer_path_reference_conversion_is_checked_on_host_boundary() {
    assert_eq!(encode_path_id(0).unwrap(), 0);
    if let Ok(maximum) = usize::try_from(u64::from(u32::MAX)) {
        assert_eq!(encode_path_id(maximum).unwrap(), u32::MAX);
    }
    if let Ok(overflow) = usize::try_from(4_294_967_296_u64) {
        let error = encode_path_id(overflow).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("source-span path ID 4294967296: exceeds u32")
        );
        let kind = error
            .downcast_ref::<crate::error::SpanPathIdOverflow>()
            .unwrap();
        assert_eq!(kind.index, overflow);
        assert!(
            std::error::Error::source(kind)
                .unwrap()
                .is::<std::num::TryFromIntError>()
        );
    }
}

#[test]
fn json_span_actual_product_reader_recovers_complete_shared_paths() {
    let expected = authored_index();
    let product = blob_product(Some(expected.to_json_bytes().unwrap()));
    let restored = product.span_index().unwrap();
    assert_eq!(restored, expected);
    assert_eq!(restored.to_json_bytes().unwrap(), COMPLETE.as_bytes());
    assert!(Arc::ptr_eq(
        &restored.lookup("urn:a").unwrap().path,
        &restored.lookup("urn:c").unwrap().path
    ));
    let product = blob_product(Some(EMPTY.to_vec()));
    assert!(product.span_index().unwrap().is_empty());
}

#[test]
fn json_span_actual_product_reader_refuses_missing_corrupt_and_bad_bounds() {
    let error = blob_product(None).span_index().unwrap_err();
    assert!(error.is::<crate::error::SpanTableConsumedAfterDrop>());
    for bytes in [b"{".as_slice(), b"null", br#"{"paths":[],"entries":null}"#,
        br#"{"paths":[],"entries":[{"subject":"s","path_id":0,"line":0,"column":0,"byte_offset":0}]}"#] {
        let error = blob_product(Some(bytes.to_vec())).span_index().unwrap_err();
        assert!(error.to_string().contains("source-span table blob JSON"));
    }
}
