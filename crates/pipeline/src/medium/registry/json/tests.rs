// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored typed state and the existing tiny declaration fixture only. No corpus,
//! training, emission, source catalog, selected artifact or producer is loaded.

use super::*;
use crate::medium::source_observation;
use purrdf_lex::json::record::{from_slice, to_vec};

const DICTIONARY: &str = r#"{"iri":"urn:d","id":"id","version":"中\n\"\\","strategy":"RawContent","target_length":7,"corpus":"urn:c"}"#;
const CORPUS: &str = r#"{"iri":"urn:c","selectors":[{"StageProduct":"urn:stage"},{"PathPrefix":"x/"},{"Graph":"urn:g"},{"BlobRep":"z"}]}"#;
const MEDIUM: &str = r#"{"iri":"urn:m","codec":"codec","zstd_level":12,"source_kind":"HeaderDict","dictionaries":["urn:d"],"reader_capabilities":["a","z"]}"#;
const REGISTRY: &str = r#"{"dictionaries":{"urn:d":{"iri":"urn:d","id":"id","version":"中\n\"\\","strategy":"RawContent","target_length":7,"corpus":"urn:c"}},"dictionary_by_id":{"id":"urn:d"},"corpora":{"urn:c":{"iri":"urn:c","selectors":[{"StageProduct":"urn:stage"},{"PathPrefix":"x/"},{"Graph":"urn:g"},{"BlobRep":"z"}]}},"split":{"iri":"urn:split","stride":8,"offset":0},"schemas":{"urn:s":{"iri":"urn:s","rep":"rep"}},"schema_by_rep":{"rep":"urn:s"},"media":{"urn:m":{"iri":"urn:m","codec":"codec","zstd_level":12,"source_kind":"HeaderDict","dictionaries":["urn:d"],"reader_capabilities":["a","z"]}},"assignment":{"urn:s":{"schema":"urn:s","medium":"urn:m","dictionary":{"Named":"urn:d"}}}}"#;

/// Independent records deliberately retain selector/vector order and sorted sets.
fn authored_registry() -> MediumRegistry {
    MediumRegistry {
        dictionaries: [(
            "urn:d".to_owned(),
            DictionaryDef {
                iri: "urn:d".to_owned(),
                id: "id".to_owned(),
                version: "中\n\"\\".to_owned(),
                strategy: DictionaryStrategy::RawContent,
                target_length: 7,
                corpus: "urn:c".to_owned(),
            },
        )]
        .into(),
        dictionary_by_id: [("id".to_owned(), "urn:d".to_owned())].into(),
        corpora: [(
            "urn:c".to_owned(),
            CorpusDef {
                iri: "urn:c".to_owned(),
                selectors: vec![
                    CorpusSelector::StageProduct("urn:stage".to_owned()),
                    CorpusSelector::PathPrefix("x/".to_owned()),
                    CorpusSelector::Graph("urn:g".to_owned()),
                    CorpusSelector::BlobRep("z".to_owned()),
                ],
            },
        )]
        .into(),
        split: Some(TrainingSplitDef {
            iri: "urn:split".to_owned(),
            stride: 8,
            offset: 0,
        }),
        schemas: [(
            "urn:s".to_owned(),
            SchemaDef {
                iri: "urn:s".to_owned(),
                rep: "rep".to_owned(),
            },
        )]
        .into(),
        schema_by_rep: [("rep".to_owned(), "urn:s".to_owned())].into(),
        media: [(
            "urn:m".to_owned(),
            MediumDef {
                iri: "urn:m".to_owned(),
                codec: "codec".to_owned(),
                zstd_level: 12,
                source_kind: MediumSourceKind::HeaderDict,
                dictionaries: ["urn:d".to_owned()].into(),
                reader_capabilities: ["z".to_owned(), "a".to_owned()].into(),
            },
        )]
        .into(),
        assignment: [(
            "urn:s".to_owned(),
            RepAssignment {
                schema: "urn:s".to_owned(),
                medium: "urn:m".to_owned(),
                dictionary: DictSelection::Named("urn:d".to_owned()),
            },
        )]
        .into(),
    }
}

#[test]
fn json_source_medium_complete_literal_pins_all_record_fields_and_declaration_order() {
    let registry = authored_registry();
    assert_eq!(
        to_vec(&registry.dictionaries["urn:d"]),
        DICTIONARY.as_bytes()
    );
    assert_eq!(to_vec(&registry.corpora["urn:c"]), CORPUS.as_bytes());
    assert_eq!(to_vec(&registry.media["urn:m"]), MEDIUM.as_bytes());
    assert_eq!(to_vec(&registry), REGISTRY.as_bytes());
    assert_eq!(
        from_slice::<MediumRegistry>(REGISTRY.as_bytes()).unwrap(),
        registry
    );
    let observation = SourceMediumRegistry {
        source_path: source_observation::SOURCE.to_owned(),
        source_digest: "digest".to_owned(),
        registry,
        producer_media: [
            ("z".to_owned(), "urn:m".to_owned()),
            ("a".to_owned(), "urn:m".to_owned()),
        ]
        .into(),
    };
    let expected = format!(
        r#"{{"source_path":"slices/core/gts/module.ttl","source_digest":"digest","registry":{REGISTRY},"producer_media":{{"a":"urn:m","z":"urn:m"}}}}"#
    );
    assert_eq!(to_vec(&observation), expected.as_bytes());
    let restored = source_observation::decode(expected.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), expected.as_bytes());
    assert_eq!(restored.declared_medium("a").unwrap(), "urn:m");
    assert!(restored.declared_medium("missing").is_err());
}

#[test]
fn json_source_medium_unit_enums_preserve_pascal_strings_and_single_null_objects() {
    for (name, variant) in [
        ("Trained", DictionaryStrategy::Trained),
        ("RawContent", DictionaryStrategy::RawContent),
        ("TermTable", DictionaryStrategy::TermTable),
    ] {
        let string = format!("\"{name}\"");
        let object = format!("{{\"{name}\":null}}");
        assert_eq!(to_vec(&variant), string.as_bytes());
        assert_eq!(
            from_slice::<DictionaryStrategy>(string.as_bytes()).unwrap(),
            variant
        );
        assert_eq!(
            from_slice::<DictionaryStrategy>(object.as_bytes()).unwrap(),
            variant
        );
    }
    for (name, variant) in [
        ("PerRep", MediumSourceKind::PerRep),
        ("HeaderDict", MediumSourceKind::HeaderDict),
        ("WholeArtifact", MediumSourceKind::WholeArtifact),
    ] {
        let string = format!("\"{name}\"");
        let object = format!("{{\"{name}\":null}}");
        assert_eq!(to_vec(&variant), string.as_bytes());
        assert_eq!(
            from_slice::<MediumSourceKind>(string.as_bytes()).unwrap(),
            variant
        );
        assert_eq!(
            from_slice::<MediumSourceKind>(object.as_bytes()).unwrap(),
            variant
        );
    }
    for bad in [
        "null",
        "[]",
        "{}",
        "\"trained\"",
        "{\"Trained\":false}",
        "{\"Trained\":null,\"RawContent\":null}",
        "{\"Trained\":null,\"Trained\":null}",
    ] {
        assert!(
            from_slice::<DictionaryStrategy>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    for bad in [
        "\"per-rep\"",
        "{\"PerRep\":[]}",
        "{\"PerRep\":null,\"PerRep\":null}",
    ] {
        assert!(
            from_slice::<MediumSourceKind>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn json_source_medium_selection_and_corpus_variants_require_exact_single_string_payloads() {
    assert_eq!(to_vec(&DictSelection::Baseline), br#""Baseline""#);
    assert_eq!(
        from_slice::<DictSelection>(br#"{"Baseline":null}"#).unwrap(),
        DictSelection::Baseline
    );
    let named = r#"{"Named":"中\n\"\\"}"#;
    assert_eq!(
        to_vec(&from_slice::<DictSelection>(named.as_bytes()).unwrap()),
        named.as_bytes()
    );
    for name in ["BlobRep", "Graph", "PathPrefix", "StageProduct"] {
        let literal = format!(r#"{{"{name}":"中\n\"\\"}}"#);
        let decoded: CorpusSelector = from_slice(literal.as_bytes()).unwrap();
        assert_eq!(to_vec(&decoded), literal.as_bytes());
        for payload in ["null", "[]", "7", "false"] {
            let bad = format!("{{\"{name}\":{payload}}}");
            assert_eq!(
                from_slice::<CorpusSelector>(bad.as_bytes())
                    .unwrap_err()
                    .pointer(),
                format!("/{name}")
            );
        }
    }
    for bad in [
        "\"Graph\"",
        "{\"unknown\":\"x\"}",
        "{\"Graph\":\"a\",\"BlobRep\":\"b\"}",
        "{\"Graph\":\"a\",\"Graph\":\"b\"}",
        "[]",
        "{}",
    ] {
        assert!(
            from_slice::<CorpusSelector>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    for bad in [
        "\"Named\"",
        "{\"Named\":null}",
        "{\"Named\":[\"x\"]}",
        "{\"Named\":\"x\",\"Baseline\":null}",
        "{\"Named\":\"x\",\"Named\":\"x\"}",
        "{\"Baseline\":false}",
    ] {
        assert!(
            from_slice::<DictSelection>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        from_slice::<CorpusSelector>(br#"{"Graph":""}"#).unwrap(),
        CorpusSelector::Graph(String::new())
    );
}

#[test]
fn json_source_medium_unsigned_tokens_and_host_bounds_keep_nested_field_pointers() {
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "18446744073709551616",
        "null",
        "\"1\"",
    ] {
        let bad = DICTIONARY.replace("\"target_length\":7", &format!("\"target_length\":{token}"));
        assert_eq!(
            from_slice::<DictionaryDef>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/target_length"
        );
        let bad = REGISTRY.replace("\"target_length\":7", &format!("\"target_length\":{token}"));
        assert_eq!(
            from_slice::<MediumRegistry>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/dictionaries/urn:d/target_length"
        );
        for field in ["stride", "offset"] {
            let bad = r#"{"iri":"split","stride":8,"offset":0}"#;
            let original = if field == "stride" { 8 } else { 0 };
            let bad = bad.replace(
                &format!("\"{field}\":{original}"),
                &format!("\"{field}\":{token}"),
            );
            assert_eq!(
                from_slice::<TrainingSplitDef>(bad.as_bytes())
                    .unwrap_err()
                    .pointer(),
                format!("/{field}")
            );
        }
    }
    let max = DICTIONARY.replace(
        "\"target_length\":7",
        &format!("\"target_length\":{}", usize::MAX),
    );
    assert_eq!(
        from_slice::<DictionaryDef>(max.as_bytes())
            .unwrap()
            .target_length,
        usize::MAX
    );
    let max: TrainingSplitDef =
        from_slice(br#"{"iri":"s","stride":18446744073709551615,"offset":18446744073709551615}"#)
            .unwrap();
    assert_eq!(max.stride, u64::MAX);
    assert_eq!(max.offset, u64::MAX);
    if usize::BITS < 64 {
        let bad = DICTIONARY.replace("\"target_length\":7", "\"target_length\":4294967296");
        assert!(from_slice::<DictionaryDef>(bad.as_bytes()).is_err());
    }
}

#[test]
fn json_source_medium_signed_levels_keep_exact_i32_contract_without_grid_restrictions() {
    for token in [
        "-0",
        "1.0",
        "1e0",
        "2147483648",
        "-2147483649",
        "null",
        "\"12\"",
    ] {
        let bad = MEDIUM.replace("\"zstd_level\":12", &format!("\"zstd_level\":{token}"));
        assert_eq!(
            from_slice::<MediumDef>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/zstd_level"
        );
    }
    for (token, expected) in [
        ("-2147483648", i32::MIN),
        ("2147483647", i32::MAX),
        ("0", 0),
        ("-1", -1),
    ] {
        let input = MEDIUM.replace("\"zstd_level\":12", &format!("\"zstd_level\":{token}"));
        assert_eq!(
            from_slice::<MediumDef>(input.as_bytes())
                .unwrap()
                .zstd_level,
            expected
        );
    }
}

#[test]
fn json_source_medium_record_defaults_nulls_duplicates_unknowns_and_map_last_wins_are_preserved() {
    let empty = br#"{"dictionaries":{},"dictionary_by_id":{},"corpora":{},"split":null,"schemas":{},"schema_by_rep":{},"media":{},"assignment":{}}"#;
    assert_eq!(to_vec(&MediumRegistry::default()), empty);
    let missing_split = String::from_utf8(empty.to_vec())
        .unwrap()
        .replace("\"split\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<MediumRegistry>(missing_split.as_bytes()).unwrap()),
        empty
    );
    let unknown = REGISTRY.replacen('{', "{\"unknown\":1e400,", 1);
    assert_eq!(
        to_vec(&from_slice::<MediumRegistry>(unknown.as_bytes()).unwrap()),
        REGISTRY.as_bytes()
    );
    for bad in [
        "{}".to_owned(),
        "[]".to_owned(),
        "null".to_owned(),
        REGISTRY.replace("\"dictionary_by_id\":{\"id\":\"urn:d\"},", ""),
        REGISTRY.replace(
            "\"dictionary_by_id\":{\"id\":\"urn:d\"}",
            "\"dictionary_by_id\":null",
        ),
        REGISTRY.replace(
            "\"split\":{\"iri\":\"urn:split\",\"stride\":8,\"offset\":0}",
            "\"split\":null,\"split\":null",
        ),
        REGISTRY.replace(
            "\"target_length\":7",
            "\"target_length\":7,\"target_length\":7",
        ),
    ] {
        assert!(
            from_slice::<MediumRegistry>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let maps = REGISTRY.replace(
        "\"dictionary_by_id\":{\"id\":\"urn:d\"}",
        "\"dictionary_by_id\":{\"id\":\"first\",\"id\":\"last\"}",
    );
    assert_eq!(
        from_slice::<MediumRegistry>(maps.as_bytes())
            .unwrap()
            .dictionary_by_id["id"],
        "last"
    );
    let bad = REGISTRY.replace(
        "\"dictionary_by_id\":{\"id\":\"urn:d\"}",
        "\"dictionary_by_id\":{\"id\":null,\"id\":\"last\"}",
    );
    assert!(from_slice::<MediumRegistry>(bad.as_bytes()).is_err());
    let duplicate_set = MEDIUM.replace(
        "\"reader_capabilities\":[\"a\",\"z\"]",
        "\"reader_capabilities\":[\"z\",\"a\",\"z\"]",
    );
    assert_eq!(
        to_vec(&from_slice::<MediumDef>(duplicate_set.as_bytes()).unwrap()),
        MEDIUM.as_bytes()
    );
    assert!(from_slice::<MediumRegistry>(format!("{REGISTRY} false").as_bytes()).is_err());
    assert!(from_slice::<MediumRegistry>(b"{").is_err());
}

#[test]
fn json_source_medium_real_decoder_keeps_wrong_source_refusal_without_revalidating_rdf() {
    let valid = format!(
        r#"{{"source_path":"slices/core/gts/module.ttl","source_digest":"","registry":{REGISTRY},"producer_media":{{}}}}"#
    );
    assert!(source_observation::decode(valid.as_bytes()).is_ok());
    let bad = valid.replace("slices/core/gts/module.ttl", "other.ttl");
    assert!(
        source_observation::decode(bad.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("different original document")
    );
    let typed_only = valid.replace("\"stride\":8", "\"stride\":0");
    assert_eq!(
        source_observation::decode(typed_only.as_bytes())
            .unwrap()
            .registry
            .split
            .unwrap()
            .stride,
        0,
        "JSON restores typed produced state; semantic RDF admission is not rerun here"
    );
    for bad in [valid.replace("\"source_path\":\"slices/core/gts/module.ttl\",", ""),
        valid.replace("\"source_digest\":\"\"", "\"source_digest\":null"),
        valid.replace("\"source_path\":\"slices/core/gts/module.ttl\"", "\"source_path\":\"slices/core/gts/module.ttl\",\"source_path\":\"slices/core/gts/module.ttl\"")] {
        assert!(source_observation::decode(bad.as_bytes()).is_err());
    }
}

#[test]
fn json_source_medium_authored_fixture_registry_lookup_and_assignment_survive_transport() {
    let registry =
        MediumRegistry::from_dataset(&crate::medium::registry::fixture::dataset("")).unwrap();
    let restored: MediumRegistry = from_slice(&to_vec(&registry)).unwrap();
    assert_eq!(restored, registry);
    assert_eq!(
        restored
            .dictionary_by_id("gmeow-core-v1")
            .unwrap()
            .target_length,
        4096
    );
    assert_eq!(
        restored.schema_for("cells-archive").unwrap().rep,
        "cells-archive"
    );
    assert_eq!(
        restored.assignment_for("cells-archive").unwrap().dictionary,
        DictSelection::Named(format!("{}dictCore", crate::medium::GMEOW))
    );
    assert_eq!(
        restored
            .assignment_for(crate::medium::SNAPSHOT_WIRE_REP)
            .unwrap()
            .dictionary,
        DictSelection::Baseline
    );
    assert!(restored.assignment_for("missing").is_err());
}
