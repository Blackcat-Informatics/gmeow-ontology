// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored JSON and plain inventory checks only; no source or fixture loading.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const SOURCE_Z: &str = "dsl/mappings/projections/zé\n\"\\中😀.ttl";
const SOURCE_A: &str = "dsl/mappings/projections/a.ttl";
const PROFILE: &str = r#"{"iri":"urn:profile:z","source":"dsl/mappings/projections/zé\n\"\\中😀.ttl","cell_minimum":3}"#;
const COMPLETE: &str = r#"{"declared":[{"iri":"urn:profile:z","source":"dsl/mappings/projections/zé\n\"\\中😀.ttl","cell_minimum":3},{"iri":"urn:profile:a","source":"dsl/mappings/projections/a.ttl","cell_minimum":0}],"measured":{"dsl/mappings/projections/a.ttl":0,"dsl/mappings/projections/zé\n\"\\中😀.ttl":5}}"#;

fn authored_inventory() -> ProjectionInventory {
    ProjectionInventory {
        declared: vec![
            ProjectionProfile {
                iri: "urn:profile:z".to_owned(),
                source: SOURCE_Z.to_owned(),
                cell_minimum: 3,
            },
            ProjectionProfile {
                iri: "urn:profile:a".to_owned(),
                source: SOURCE_A.to_owned(),
                cell_minimum: 0,
            },
        ],
        measured: BTreeMap::from([(SOURCE_Z.to_owned(), 5), (SOURCE_A.to_owned(), 0)]),
    }
}

#[test]
fn json_projection_inventory_literal_bytes_preserve_records_vector_map_and_escaping() {
    let inventory = authored_inventory();
    assert_eq!(to_vec(&inventory.declared[0]), PROFILE.as_bytes());
    assert_eq!(to_vec(&inventory), COMPLETE.as_bytes());
    let decoded: ProjectionInventory = from_slice(COMPLETE.as_bytes()).unwrap();
    assert_eq!(decoded.declared, inventory.declared);
    assert_eq!(decoded.measured, inventory.measured);
    assert_eq!(to_vec(&decoded), COMPLETE.as_bytes());
    assert_eq!(decoded.declared[0].source, SOURCE_Z);
}

#[test]
fn json_projection_inventory_empty_collections_and_zero_fields_remain_present() {
    let inventory = ProjectionInventory {
        declared: Vec::new(),
        measured: BTreeMap::new(),
    };
    let expected = br#"{"declared":[],"measured":{}}"#;
    assert_eq!(to_vec(&inventory), expected);
    assert_eq!(
        to_vec(&from_slice::<ProjectionInventory>(expected).unwrap()),
        expected
    );
    let profile = ProjectionProfile {
        iri: String::new(),
        source: String::new(),
        cell_minimum: 0,
    };
    assert_eq!(
        to_vec(&profile),
        br#"{"iri":"","source":"","cell_minimum":0}"#
    );
}

#[test]
fn json_projection_inventory_profile_required_null_duplicate_and_unknown_policy() {
    for input in [
        "null",
        "[]",
        "{}",
        r#"["i","s",0]"#,
        r#"{"source":"s","cell_minimum":0}"#,
        r#"{"iri":"i","cell_minimum":0}"#,
        r#"{"iri":"i","source":"s"}"#,
        r#"{"iri":null,"source":"s","cell_minimum":0}"#,
        r#"{"iri":"i","source":null,"cell_minimum":0}"#,
        r#"{"iri":"i","source":"s","cell_minimum":null}"#,
        r#"{"iri":"i","iri":"j","source":"s","cell_minimum":0}"#,
        r#"{"iri":"i","source":"s","source":"t","cell_minimum":0}"#,
        r#"{"iri":"i","source":"s","cell_minimum":0,"cell_minimum":1}"#,
        r#"{"iri":false,"source":"s","cell_minimum":0}"#,
        r#"{"iri":"i","source":[],"cell_minimum":0}"#,
    ] {
        assert!(
            from_slice::<ProjectionProfile>(input.as_bytes()).is_err(),
            "{input}"
        );
    }
    let unknown = br#"{"iri":"i","source":"s","cell_minimum":0,"unknown":1e400,"unknown":null}"#;
    assert_eq!(
        to_vec(&from_slice::<ProjectionProfile>(unknown).unwrap()),
        br#"{"iri":"i","source":"s","cell_minimum":0}"#
    );
}

#[test]
fn json_projection_inventory_envelope_required_null_duplicate_and_malformed_policy() {
    for input in [
        "",
        "{",
        "null",
        "[]",
        "{}",
        r#"{"declared":[]}"#,
        r#"{"measured":{}}"#,
        r#"{"declared":null,"measured":{}}"#,
        r#"{"declared":[],"measured":null}"#,
        r#"{"declared":{},"measured":{}}"#,
        r#"{"declared":[],"measured":[]}"#,
        r#"{"declared":[false],"measured":{}}"#,
        r#"{"declared":[],"declared":[],"measured":{}}"#,
        r#"{"declared":[],"measured":{},"measured":{}}"#,
        r#"{"declared":[],"measured":{"unknown":1e400}}"#,
        r#"{"declared":[],"measured":{}} false"#,
    ] {
        assert!(
            from_slice::<ProjectionInventory>(input.as_bytes()).is_err(),
            "{input}"
        );
    }
    let unknown = br#"{"declared":[],"measured":{},"unknown":1e400,"unknown":null}"#;
    assert_eq!(
        to_vec(&from_slice::<ProjectionInventory>(unknown).unwrap()),
        br#"{"declared":[],"measured":{}}"#
    );
}

#[test]
fn json_projection_inventory_strict_counts_preserve_host_maximum_and_nested_pointers() {
    let maximum = format!(
        r#"{{"declared":[{{"iri":"i","source":"s","cell_minimum":{}}}],"measured":{{"s":{}}}}}"#,
        usize::MAX,
        usize::MAX
    );
    let decoded: ProjectionInventory = from_slice(maximum.as_bytes()).unwrap();
    assert_eq!(decoded.declared[0].cell_minimum, usize::MAX);
    assert_eq!(decoded.measured["s"], usize::MAX);
    assert_eq!(to_vec(&decoded), maximum.as_bytes());
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "18446744073709551616",
        "null",
        "false",
        "\"1\"",
    ] {
        let input = format!(r#"{{"iri":"i","source":"s","cell_minimum":{token}}}"#);
        let error = from_slice::<ProjectionProfile>(input.as_bytes()).unwrap_err();
        assert_eq!(error.pointer(), "/cell_minimum");
        let input = format!(r#"{{"declared":[],"measured":{{"profile/~x":{token}}}}}"#);
        let error = from_slice::<ProjectionInventory>(input.as_bytes()).unwrap_err();
        assert_eq!(error.pointer(), "/measured/profile~1~0x");
    }
    if usize::BITS < 64 {
        assert!(
            from_slice::<ProjectionProfile>(
                br#"{"iri":"i","source":"s","cell_minimum":4294967296}"#
            )
            .is_err()
        );
        assert!(
            from_slice::<ProjectionInventory>(br#"{"declared":[],"measured":{"s":4294967296}}"#)
                .is_err()
        );
    }
    let nested = COMPLETE.replace("\"cell_minimum\":3", "\"cell_minimum\":-0");
    let error = from_slice::<ProjectionInventory>(nested.as_bytes()).unwrap_err();
    assert_eq!(error.pointer(), "/declared/0/cell_minimum");
}

#[test]
fn json_projection_inventory_map_duplicates_are_last_wins_but_every_value_is_admitted() {
    let input = br#"{"declared":[],"measured":{"z":9,"a":3,"z":2}}"#;
    let decoded: ProjectionInventory = from_slice(input).unwrap();
    assert_eq!(decoded.measured["z"], 2);
    assert_eq!(
        to_vec(&decoded),
        br#"{"declared":[],"measured":{"a":3,"z":2}}"#
    );
    for input in [
        br#"{"declared":[],"measured":{"z":-0,"z":2}}"#.as_slice(),
        br#"{"declared":[],"measured":{"z":2,"z":-0}}"#,
        br#"{"declared":[],"measured":{"z":null,"z":2}}"#,
        br#"{"declared":[],"measured":{"z":18446744073709551616,"z":2}}"#,
    ] {
        assert!(from_slice::<ProjectionInventory>(input).is_err());
    }
}

#[test]
fn json_projection_inventory_plain_model_check_keeps_exact_paths_and_cell_floors() {
    let inventory = authored_inventory();
    let decoded: ProjectionInventory = from_slice(COMPLETE.as_bytes()).unwrap();
    assert_eq!(decoded.check().unwrap(), inventory.measured);
    let mut raised = authored_inventory();
    raised.measured.insert(SOURCE_Z.to_owned(), 6);
    assert_eq!(raised.check().unwrap()[SOURCE_Z], 6);
    let mut missing = authored_inventory();
    let _ = missing.measured.remove(SOURCE_Z);
    let error = missing.check().unwrap_err();
    assert!(error.to_string().contains("no longer on disk"));
    assert!(error.to_string().contains(SOURCE_Z));
    let mut extra = authored_inventory();
    extra
        .measured
        .insert("dsl/mappings/projections/unregistered.ttl".to_owned(), 1);
    let error = extra.check().unwrap_err();
    assert!(error.to_string().contains("not declared"));
    assert!(error.to_string().contains("unregistered.ttl"));
    let mut below = authored_inventory();
    below.measured.insert(SOURCE_Z.to_owned(), 2);
    let error = below.check().unwrap_err();
    assert!(error.to_string().contains("cell-count ratchet"));
    assert!(error.to_string().contains("declares 2"));
}
