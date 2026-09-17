// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn validates_qids_and_pids() {
    for valid in ["Q1", "Q42", "P31"] {
        assert!(is_valid_id(valid), "{valid}");
    }
    for invalid in ["", "42", "Q", "Q0", "Q01", "Q12abc", "P0"] {
        assert!(!is_valid_id(invalid), "{invalid}");
    }
}

#[test]
fn syntax_iri_flags_namespace_misuse() {
    assert_eq!(
        check_syntax_iri("https://www.wikidata.org/entity/Q42", false)[0].kind,
        NamespaceMisuse::HttpsUrlShouldBeCurie
    );
    assert!(
        check_syntax_iri("https://www.wikidata.org/entity/P31", false)[0]
            .message
            .contains("wd:P31")
    );
    assert_eq!(
        check_syntax_iri("http://www.wikidata.org/entity/P31", false)[0].kind,
        NamespaceMisuse::WdPropShouldBeWdt
    );
    assert!(check_syntax_iri("http://www.wikidata.org/entity/P31", true).is_empty());
    assert_eq!(
        check_syntax_iri("http://www.wikidata.org/prop/direct/Q42", false)[0].kind,
        NamespaceMisuse::WdtItemShouldBeWd
    );
    assert_eq!(
        check_syntax_iri("http://www.wikidata.org/entity/Q0", false)[0].kind,
        NamespaceMisuse::BadSyntax
    );
    // HTTPS direct-property namespace: previously unrecognized (dropped); now flagged
    // with the wdt: CURIE suggestion, mirroring the HTTPS-entity branch.
    assert_eq!(
        check_syntax_iri("https://www.wikidata.org/prop/direct/P31", false)[0].kind,
        NamespaceMisuse::HttpsUrlShouldBeCurie
    );
    assert!(
        check_syntax_iri("https://www.wikidata.org/prop/direct/P31", false)[0]
            .message
            .contains("wdt:P31")
    );
    assert_eq!(
        check_syntax_iri("https://www.wikidata.org/prop/direct/P0", false)[0].kind,
        NamespaceMisuse::BadSyntax
    );
}

#[test]
fn validates_lexeme_and_sense_ids() {
    // Well-formed lexeme ids and sense ids pass the entity-syntax gate…
    for valid in ["L7", "L1119", "L14462", "L7-S1", "L1570700-S2"] {
        assert!(
            is_valid_entity_id(valid),
            "{valid} should be a valid entity id"
        );
    }
    // …while malformed lexeme/sense ids are rejected exactly like a malformed QID.
    for invalid in [
        "L", "L0", "L01", "L7x", "L7-S", "L7-S0", "L7-Sx", "L-S1", "S1",
    ] {
        assert!(!is_valid_entity_id(invalid), "{invalid} must be rejected");
    }
    // The kinds are distinct: a lexeme id is not a QID/PID, and a QID/PID never carries a
    // sense suffix (a sense hangs off a lexeme, never an item or property).
    assert!(!is_valid_lexeme_id("Q7"));
    assert!(!is_valid_sense_id("Q42-S3"));
    assert!(!is_valid_sense_id("P31-S1"));
    assert!(is_valid_sense_id("L42-S3"));
}

#[test]
fn syntax_iri_accepts_wd_lexeme_and_sense_ids_and_flags_malformed_ones() {
    // A well-formed lexeme id and sense id under the wd: namespace raise no misuse.
    assert!(check_syntax_iri("http://www.wikidata.org/entity/L7", true).is_empty());
    assert!(check_syntax_iri("http://www.wikidata.org/entity/L7-S1", true).is_empty());
    // A malformed lexeme/sense id is flagged BadSyntax, like a malformed QID.
    assert_eq!(
        check_syntax_iri("http://www.wikidata.org/entity/L0", true)[0].kind,
        NamespaceMisuse::BadSyntax
    );
    assert_eq!(
        check_syntax_iri("http://www.wikidata.org/entity/L7-S0", true)[0].kind,
        NamespaceMisuse::BadSyntax
    );
    // A QID with a sense suffix is not a real sense id — flagged BadSyntax.
    assert_eq!(
        check_syntax_iri("http://www.wikidata.org/entity/Q42-S3", true)[0].kind,
        NamespaceMisuse::BadSyntax
    );
    // The HTTPS entity namespace suggests the wd: CURIE for a lexeme id, mirroring QIDs.
    assert_eq!(
        check_syntax_iri("https://www.wikidata.org/entity/L7", true)[0].kind,
        NamespaceMisuse::HttpsUrlShouldBeCurie
    );
}

#[test]
fn dc_expected_sets_are_counted_like_python_report() {
    assert_eq!(EXPECTED_DC.len(), 15);
    assert_eq!(EXPECTED_DCTERMS.len(), 46);
    assert_eq!(EXPECTED_DCMITYPE.len(), 12);
}

#[test]
fn dc_expected_namespace_ignores_out_of_scope_terms() {
    let expected_dcterms = expected_set(DCTERMS_NS, EXPECTED_DCTERMS);
    let expected_dc = expected_set(DC_NS, EXPECTED_DC);
    let expected_dcmitype = expected_set(DCMITYPE_NS, EXPECTED_DCMITYPE);

    assert_eq!(
        expected_dc_namespace(
            "http://purl.org/dc/terms/title",
            &expected_dcterms,
            &expected_dc,
            &expected_dcmitype,
        ),
        Some("dcterms")
    );
    assert_eq!(
        expected_dc_namespace(
            "http://purl.org/dc/terms/notARealDctermsTerm",
            &expected_dcterms,
            &expected_dc,
            &expected_dcmitype,
        ),
        None
    );
    assert_eq!(
        expected_dc_namespace(
            "http://purl.org/dc/dcmitype/StillImage",
            &expected_dcterms,
            &expected_dc,
            &expected_dcmitype,
        ),
        Some("dcmitype")
    );
}

#[test]
fn wikidata_entities_rejects_api_errors_and_malformed_payloads() {
    let error = serde_json::json!({
        "error": {
            "code": "bad-request",
            "info": "bad ids"
        }
    });
    let error_diag = wikidata_entities(&error).unwrap_err();
    assert!(error_diag.is::<crate::error::Mapping>());
    assert!(
        error_diag
            .message()
            .contains("Wikidata API error bad-request")
    );

    let missing_success = serde_json::json!({
        "entities": {}
    });
    assert!(
        wikidata_entities(&missing_success)
            .unwrap_err()
            .message()
            .contains("success=1")
    );

    let missing_entities = serde_json::json!({
        "success": 1
    });
    assert!(
        wikidata_entities(&missing_entities)
            .unwrap_err()
            .message()
            .contains("entities object")
    );

    let ok = serde_json::json!({
        "success": 1,
        "entities": {
            "Q42": {}
        }
    });
    assert!(wikidata_entities(&ok).unwrap().contains_key("Q42"));
}

#[test]
fn check_existence_rejects_invalid_chunk_sizes() {
    let root = tempfile::tempdir().unwrap();
    let identifiers = vec!["Q42".to_owned()];
    let timeout = Duration::from_secs(1);
    let delay = Duration::ZERO;

    let zero = check_existence(&identifiers, root.path(), timeout, 0, delay).unwrap_err();
    assert!(zero.is::<crate::error::Mapping>());
    assert!(zero.message().contains("between 1 and 50"));
    let too_large = check_existence(&identifiers, root.path(), timeout, 51, delay).unwrap_err();
    assert!(too_large.message().contains("between 1 and 50"));
}

#[test]
fn check_existence_uses_cache_for_every_wikidata_entity_kind() {
    let root = tempfile::tempdir().unwrap();
    let valid = [
        "Q42",
        "P31",
        "L7",
        "L1119",
        "L14462",
        "L7-S1",
        "L1119-S1",
        "L14462-S2",
    ]
    .map(str::to_owned)
    .to_vec();
    let malformed = ["Q0", "P0", "L0", "L01", "L7-S0", "L7-Sx", "Q42-S1"]
        .map(str::to_owned)
        .to_vec();
    let entities = valid
        .iter()
        .map(|identifier| (identifier.clone(), serde_json::json!({})))
        .collect::<serde_json::Map<_, _>>();
    let cached = serde_json::json!({
        "success": 1,
        "entities": entities,
    });
    save_cached(root.path(), &cache_key(&valid), &cached).unwrap();

    let identifiers = valid.iter().chain(&malformed).cloned().collect::<Vec<_>>();
    let statuses = check_existence(
        &identifiers,
        root.path(),
        Duration::from_nanos(1),
        WIKIDATA_MAX_IDS_PER_REQUEST,
        Duration::ZERO,
    )
    .expect("the complete cache entry prevents every network request");

    for identifier in valid {
        assert_eq!(statuses[&identifier], ExistenceStatus::Ok, "{identifier}");
    }
    for identifier in malformed {
        assert_eq!(
            statuses[&identifier],
            ExistenceStatus::BadSyntax,
            "{identifier}"
        );
    }
}

#[test]
fn slice_module_discovery_fails_loudly_when_empty() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("slices")).unwrap();
    let err = slice_module_files(root.path()).unwrap_err();
    assert!(err.is::<crate::error::Io>());
    assert!(err.message().contains("no slice module files found"));
}
