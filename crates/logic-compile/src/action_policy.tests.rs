// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use sha2::{Digest as _, Sha256};

fn tiny_source(extra: Option<RdfQuad>) -> Arc<RdfDataset> {
    let subject = RdfTerm::iri(format!("{POLICY_NAMESPACE}synthetic"));
    let mut builder = RdfDatasetBuilder::new();
    for (predicate, object) in [
        (
            "https://blackcatinformatics.ca/logic/precondition",
            RdfTerm::iri("urn:synthetic:ready"),
        ),
        (
            TOOL_NAME,
            RdfTerm::Literal(RdfLiteral {
                lexical_form: "synthetic".to_owned(),
                datatype: None,
                language: Some("en".to_owned()),
                direction: Some(RdfTextDirection::Rtl),
            }),
        ),
        (
            ANNOTATIONS[0],
            RdfTerm::Literal(RdfLiteral::simple("a synthetic label")),
        ),
        (
            ANNOTATIONS[1],
            RdfTerm::Literal(RdfLiteral::simple("a synthetic comment")),
        ),
    ] {
        builder.push_owned_quad(&RdfQuad::new(subject.clone(), predicate, object));
    }
    if let Some(quad) = extra {
        builder.push_owned_quad(&quad);
    }
    builder
        .freeze()
        .expect("tiny explicit native policy source")
}

#[test]
fn native_policy_transport_preserves_context_literal_metadata_and_complement() {
    let source = tiny_source(None);
    let policy = PreparedActionPolicy::from_dataset(&source, SOURCE_SHA256).unwrap();
    let bytes = serde_json::to_vec(&policy).unwrap();
    let restored: PreparedActionPolicy = serde_json::from_slice(&bytes).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.nquads(), policy.nquads());
    assert_eq!(restored.source_statements(), &policy_statements(&source));
    assert_eq!(restored.omitted_annotations().len(), 2);
    assert_eq!(restored.omitted_annotations(), policy.omitted_annotations());
    let dataset = restored.dataset().unwrap();
    assert!(std::ptr::eq(dataset, restored.dataset().unwrap()));
    assert!(
        dataset
            .owned_quads()
            .all(|quad| quad.graph_name == Some(RdfTerm::iri(WORLD)))
    );
    let literal = dataset
        .owned_quads()
        .find(|quad| quad.predicate == TOOL_NAME)
        .unwrap();
    let RdfTerm::Literal(value) = literal.object else {
        panic!("tool name remains a native literal")
    };
    assert_eq!(value.language.as_deref(), Some("en"));
    assert_eq!(value.direction, Some(RdfTextDirection::Rtl));
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
}

#[test]
fn native_policy_refuses_unaccounted_source_growth_and_inconsistent_hydration() {
    let source = tiny_source(Some(RdfQuad::new(
        RdfTerm::iri(format!("{POLICY_NAMESPACE}synthetic")),
        "urn:synthetic:unaccounted",
        RdfTerm::Literal(RdfLiteral::simple("must not disappear")),
    )));
    assert!(PreparedActionPolicy::from_dataset(&source, SOURCE_SHA256).is_err());
    assert!(project_nquads(&source).is_err());
    let source = tiny_source(None);
    assert!(PreparedActionPolicy::from_dataset(&source, &"0".repeat(64)).is_err());
    let mut policy = PreparedActionPolicy::from_dataset(&source, SOURCE_SHA256).unwrap();
    policy.nquads.push_str("\nforged projection");
    assert!(policy.validate().is_err());
}

#[test]
fn action_policy_identity_pin_matches_original_bytes_without_compilation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = std::fs::read(root.join(SOURCE_PATH))
        .expect("original policy bytes for pure identity check");
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), SOURCE_SHA256);
}
