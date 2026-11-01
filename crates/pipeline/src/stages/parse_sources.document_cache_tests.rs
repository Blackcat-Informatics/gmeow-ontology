// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Catalog identities retain complete ordered receipt metadata while document
/// compilation preserves exact selection and bounds its retained cache.
#[test]
fn standalone_compilation_shares_exact_selection_and_bounds_retention() {
    let catalog = SourceCatalog::from_sources(ParsedAuthoredSources::synthetic(
        "<urn:test:A> <https://blackcatinformatics.ca/logic/subClassOf> <urn:test:B> .",
    ))
    .unwrap();
    use gmeow_logic_compile::frontend::{SourceBase, SourceBaseOrigin, SourceDocument};

    // Frozen packets pin the complete wire and actual catalog identities.
    let check_reference = |case: &str, receipts: &[SourceDocument]| {
        let row = include_str!("parse_sources/fixtures/source-catalog-cbor-v1.txt")
            .lines()
            .find(|row| row.split('|').next() == Some(case))
            .expect("the original catalog reference contains this case");
        let fields: Vec<_> = row.split('|').collect();
        let [_, wire, digest] = fields.as_slice() else {
            panic!("original catalog reference must contain three fields: {case}");
        };
        let mut bytes = Vec::new();
        crate::handle_identity::write_source_catalog_cbor(receipts, &mut bytes).unwrap();
        let identity = crate::handle_identity::source_catalog_digest(receipts);
        assert_eq!(
            purrdf_hash::hex::encode(&bytes),
            *wire,
            "complete source-catalog CBOR: {case}",
        );
        assert_eq!(identity, *digest, "actual catalog identity: {case}");
        identity
    };
    assert_eq!(
        check_reference("actual-synthetic", &catalog.receipts),
        catalog.identity(),
        "the admitted catalog uses the complete native identity operation",
    );
    check_reference("empty", &[]);
    let document = |path: &str, digest: &str, role: &str, base| SourceDocument {
        path: path.to_owned(),
        content_digest: digest.to_owned(),
        role: role.to_owned(),
        base,
    };
    let rich = vec![
        document("source/λ\0𝄞.ttl", "digest:none", "ontology", None),
        document(
            "source/caller.ttl",
            "digest:caller",
            "statements",
            Some(SourceBase {
                iri: "urn:source:caller".into(),
                origin: SourceBaseOrigin::Caller,
            }),
        ),
        document(
            "source/enclosing.ttl",
            "digest:enclosing",
            "mappings",
            Some(SourceBase {
                iri: "urn:source:enclosing".into(),
                origin: SourceBaseOrigin::Enclosing,
            }),
        ),
        document(
            "source/directive.ttl",
            "digest:directive",
            "ontology",
            Some(SourceBase {
                iri: "urn:source:directive".into(),
                origin: SourceBaseOrigin::Directive {
                    line: u64::MAX,
                    column: u32::MAX,
                },
            }),
        ),
    ];
    let rich_identity = check_reference("rich", &rich);
    let mut reordered = rich.clone();
    reordered.reverse();
    let mut duplicated = rich.clone();
    duplicated.push(rich[0].clone());
    let mut changed_path = rich.clone();
    changed_path[0].path.push_str(".changed");
    let mut changed_digest = rich.clone();
    changed_digest[0].content_digest.push_str(":changed");
    let mut changed_role = rich.clone();
    changed_role[0].role = "examples".into();
    let mut changed_base = rich.clone();
    changed_base[1]
        .base
        .as_mut()
        .unwrap()
        .iri
        .push_str(":changed");
    let mut changed_coordinates = rich.clone();
    changed_coordinates[3].base.as_mut().unwrap().origin = SourceBaseOrigin::Directive {
        line: u64::MAX - 1,
        column: u32::MAX - 1,
    };
    for (case, receipts) in [
        ("reordered", reordered),
        ("duplicated", duplicated),
        ("changed-path", changed_path),
        ("changed-digest", changed_digest),
        ("changed-role", changed_role),
        ("changed-base", changed_base),
        ("changed-coordinates", changed_coordinates),
    ] {
        assert_ne!(
            check_reference(case, &receipts),
            rich_identity,
            "source identity binds each receipt field, order and multiplicity: {case}",
        );
    }
    let first = catalog.compiled_document("synthetic.nq", None).unwrap();
    let again = catalog.compiled_document("synthetic.nq", None).unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    let scoped = catalog
        .compiled_document("synthetic.nq", Some("urn:source:other".into()))
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &scoped));
    assert_eq!(
        scoped.program().source_iri.as_deref(),
        Some("urn:source:other")
    );
    assert!(catalog.compiled_document("absent.nq", None).is_err());
    for index in 0..20 {
        catalog
            .compiled_document("synthetic.nq", Some(format!("urn:source:{index}")))
            .unwrap();
    }
    assert_eq!(catalog.documents.lock().unwrap().len(), 16);
    let overflow = catalog
        .compiled_document("synthetic.nq", Some("urn:source:19".into()))
        .unwrap();
    let recomputed = catalog
        .compiled_document("synthetic.nq", Some("urn:source:19".into()))
        .unwrap();
    assert!(!Arc::ptr_eq(&overflow, &recomputed));
    assert_eq!(overflow.program(), recomputed.program());
}
