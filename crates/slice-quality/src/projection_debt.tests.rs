// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent tiny authoring directories; no repository corpus or producer.

use super::*;
use crate::model::ProjectionCeilingCommitment;

const SLICE: &str = "https://external.test/slice";
const PREFIXES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .
    @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
    @prefix ex: <https://external.test/> .";

fn source() -> tempfile::TempDir {
    // gmeow-test-input: synthetic-only
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("manifest.ttl"),
        format!("{PREFIXES} <{SLICE}> a gmeow:Slice ."),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("module.ttl"),
        format!("{PREFIXES} ex:A a sh:NodeShape ."),
    )
    .unwrap();
    directory
}

fn governance(count: u64) -> GovernanceFloors {
    GovernanceFloors {
        vocabularies: vec![crate::counting::shacl_vocab()],
        ceilings: vec![ProjectionCeilingCommitment {
            slice: SLICE.into(),
            vocab_prefix: "sh".into(),
            count,
        }],
        ..GovernanceFloors::default()
    }
}

#[test]
fn owner_set_installed_debt_reader_uses_every_declared_boundary() {
    use crate::model::GroundingOwners;
    let directory = source();
    let mut policy = governance(0);
    policy.ceilings.clear();
    let descriptor = &mut policy.vocabularies[0];
    descriptor.prefix = "gufo".into();
    descriptor.namespaces = vec!["https://w3id.org/gufo#".into()];
    descriptor.count_kind = CountKind::TypedAxiom;
    descriptor.owners = GroundingOwners::new([
        GroundingOwners::MATH.to_owned(),
        GroundingOwners::LANG.to_owned(),
    ])
    .unwrap();
    std::fs::write(
        directory.path().join("module.ttl"),
        format!(
            r#"{PREFIXES}
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
        ex:Thing skos:relatedMatch <https://w3id.org/gufo#Kind> {{|
            a logic:GroundingCorrespondence ; gmeow:sssomFile "tiny.sssom.tsv" ;
            gmeow:justification ex:curation ; logic:sourceEndpoint ex:Thing ;
            logic:targetEndpoint <https://w3id.org/gufo#Kind> ;
            logic:morphismClass logic:BridgeView ;
            logic:morphismKind logic:CommitmentShiftingBridge ;
            logic:preservationKind logic:ValidationOnly
        |}} ."#
        ),
    )
    .unwrap();
    for (surface, expected) in [
        (GroundingOwners::LANG, 0),
        (GroundingOwners::MATH, 0),
        (GroundingOwners::LOGIC, 1),
    ] {
        std::fs::write(
            directory.path().join("manifest.ttl"),
            format!("{PREFIXES} <{surface}> a gmeow:Slice ."),
        )
        .unwrap();
        let rows = measure_slice_debt(directory.path(), &policy).unwrap();
        assert_eq!(rows[0].measured, expected, "{surface}");
        assert_eq!(rows[0].ceiling, 0);
    }
}

#[test]
fn projection_debt_measures_recursive_authoring_only_without_a_checkout() {
    let directory = source();
    let root = directory.path();
    std::fs::create_dir_all(root.join("mappings/nested")).unwrap();
    std::fs::write(
        root.join("mappings/nested/mapping.ttl"),
        format!("{PREFIXES} ex:B a sh:NodeShape ."),
    )
    .unwrap();
    for non_authoring in ["examples", "tests"] {
        std::fs::create_dir(root.join(non_authoring)).unwrap();
        std::fs::write(
            root.join(non_authoring).join("not-authoring.ttl"),
            "invalid Turtle",
        )
        .unwrap();
    }
    let rows = measure_slice_debt(root, &governance(2)).unwrap();
    assert_eq!(
        rows,
        vec![ProjectionDebtRow {
            slice: SLICE.into(),
            vocabulary: "sh".into(),
            measured: 2,
            ceiling: 2,
        }]
    );
    let first = render_projection_debt(&rows, "tsv").unwrap();
    assert_eq!(
        first,
        render_projection_debt(&measure_slice_debt(root, &governance(2)).unwrap(), "tsv").unwrap()
    );
    assert!(first.contains("\tsh\t2\t2\t0\n"));
}

#[test]
fn projection_debt_uses_trusted_defaults_and_reports_deficits_without_rebaselining() {
    let directory = source();
    let mut policy = governance(4);
    let rows = measure_slice_debt(directory.path(), &policy).unwrap();
    assert_eq!(rows[0].headroom(), 3);
    policy.ceilings.clear();
    let rows = measure_slice_debt(directory.path(), &policy).unwrap();
    assert_eq!(rows[0].ceiling, 0);
    assert_eq!(rows[0].headroom(), -1);
    std::fs::write(directory.path().join("mappings-local.ttl"), "not selected").unwrap();
    std::fs::write(
        directory.path().join("module.ttl"),
        format!(
            "{PREFIXES} ex:A a sh:NodeShape . ex:local a gmeow:ProjectionCeilingCommitment ;
         gmeow:ceilingSlice <{SLICE}> ; gmeow:ceilingVocabulary ex:sh ; gmeow:ceilingCount 999 ."
        ),
    )
    .unwrap();
    assert_eq!(
        measure_slice_debt(directory.path(), &policy).unwrap()[0].ceiling,
        0
    );
}

#[test]
fn projection_debt_rejects_missing_malformed_and_ambiguous_selected_sources() {
    let directory = source();
    let root = directory.path();
    std::fs::remove_file(root.join("module.ttl")).unwrap();
    assert!(measure_slice_debt(root, &governance(0)).is_err());
    std::fs::write(root.join("module.ttl"), "invalid Turtle").unwrap();
    assert!(measure_slice_debt(root, &governance(0)).is_err());
    std::fs::write(root.join("module.ttl"), "").unwrap();
    std::fs::write(
        root.join("manifest.ttl"),
        format!("{PREFIXES} <{SLICE}> a gmeow:Slice . ex:other a gmeow:Slice ."),
    )
    .unwrap();
    assert!(measure_slice_debt(root, &governance(0)).is_err());
    std::fs::remove_file(root.join("manifest.ttl")).unwrap();
    assert!(measure_slice_debt(root, &governance(0)).is_err());
}

#[test]
fn projection_debt_rejects_a_mapping_tree_that_cannot_be_scanned() {
    let directory = source();
    std::fs::write(directory.path().join("mappings"), "not a directory").unwrap();
    assert!(measure_slice_debt(directory.path(), &governance(0)).is_err());
}

#[test]
fn projection_debt_rejects_ambiguous_or_missing_governance() {
    let directory = source();
    assert!(measure_slice_debt(directory.path(), &GovernanceFloors::default()).is_err());
    let mut policy = governance(1);
    policy.ceilings.push(policy.ceilings[0].clone());
    assert!(measure_slice_debt(directory.path(), &policy).is_err());
    let mut policy = governance(1);
    policy.vocabularies.push(policy.vocabularies[0].clone());
    assert!(measure_slice_debt(directory.path(), &policy).is_err());
    let mut policy = governance(1);
    policy.ceilings[0].vocab_prefix = "unknown".into();
    assert!(measure_slice_debt(directory.path(), &policy).is_err());
}

#[test]
fn projection_debt_does_not_present_documentary_surfaces_as_measured() {
    let directory = source();
    let mut policy = governance(1);
    let mut documentary = policy.vocabularies[0].clone();
    documentary.prefix = "prolog".into();
    documentary.count_kind = CountKind::NonRdfSurface;
    policy.vocabularies.push(documentary);
    assert_eq!(
        measure_slice_debt(directory.path(), &policy).unwrap().len(),
        1
    );
    policy.vocabularies[0].count_kind = CountKind::NonRdfSurface;
    assert!(measure_slice_debt(directory.path(), &policy).is_err());
}

#[test]
fn projection_debt_parses_every_selected_shape_and_mapping_file() {
    let directory = source();
    let root = directory.path();
    std::fs::write(root.join("shapes.ttl"), "invalid Turtle").unwrap();
    assert!(measure_slice_debt(root, &governance(0)).is_err());
    std::fs::remove_file(root.join("shapes.ttl")).unwrap();
    std::fs::create_dir_all(root.join("mappings/nested")).unwrap();
    std::fs::write(root.join("mappings/nested/broken.ttl"), "invalid Turtle").unwrap();
    assert!(measure_slice_debt(root, &governance(0)).is_err());
}

#[cfg(unix)]
#[test]
fn projection_debt_rejects_mapping_cycles_and_dangling_optional_sources() {
    let directory = source();
    let root = directory.path();
    std::fs::create_dir(root.join("mappings")).unwrap();
    std::os::unix::fs::symlink(root.join("mappings"), root.join("mappings/loop")).unwrap();
    assert!(measure_slice_debt(root, &governance(0)).is_err());
    std::fs::remove_file(root.join("mappings/loop")).unwrap();
    std::os::unix::fs::symlink(root.join("missing.ttl"), root.join("shapes.ttl")).unwrap();
    assert!(measure_slice_debt(root, &governance(0)).is_err());
}

#[test]
fn projection_debt_preserves_exact_u64_headroom_and_wide_totals() {
    let mut row = ProjectionDebtRow {
        slice: SLICE.into(),
        vocabulary: "sh".into(),
        measured: u64::MAX,
        ceiling: u64::MAX - 1,
    };
    assert_eq!(row.headroom(), -1);
    let report = render_projection_debt(&[row.clone(), row.clone()], "tsv").unwrap();
    assert!(report.contains("total measured=36893488147419103230 total headroom=-2"));
    row.ceiling = 0;
    assert_eq!(row.headroom(), -i128::from(u64::MAX));
    row.measured = 0;
    row.ceiling = u64::MAX;
    assert_eq!(row.headroom(), i128::from(u64::MAX));
    assert!(
        render_projection_debt(&[row.clone()], "human")
            .unwrap()
            .contains("18446744073709551615")
    );
    assert!(render_projection_debt(&[row], "unknown").is_err());
}

#[test]
fn projection_debt_preserves_the_declared_subsumption_budget_unit() {
    let directory = source();
    std::fs::write(
        directory.path().join("module.ttl"),
        r#"
        @prefix ex: <https://external.test/> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        ex:A rdfs:subClassOf ex:B ; rdfs:label "A" .
        ex:p rdfs:subPropertyOf ex:q ; rdfs:domain ex:A ; rdfs:range ex:B .
    "#,
    )
    .unwrap();
    let mut policy = governance(2);
    let vocabulary = &mut policy.vocabularies[0];
    vocabulary.prefix = "rdfs".into();
    vocabulary.namespaces = vec!["http://www.w3.org/2000/01/rdf-schema#".into()];
    vocabulary.count_kind = CountKind::StructuralAxiom;
    vocabulary.counted_predicates = vec![
        "http://www.w3.org/2000/01/rdf-schema#subClassOf".into(),
        "http://www.w3.org/2000/01/rdf-schema#subPropertyOf".into(),
    ];
    policy.ceilings[0].vocab_prefix = "rdfs".into();
    let rows = measure_slice_debt(directory.path(), &policy).unwrap();
    assert_eq!(rows[0].measured, 2);
    assert_eq!(rows[0].headroom(), 0);
}

#[test]
fn projection_debt_shared_inventory_accepts_manifest_only_profiles() {
    let directory = source();
    std::fs::remove_file(directory.path().join("module.ttl")).unwrap();
    assert!(
        crate::ratchet_surface_paths(directory.path())
            .unwrap()
            .is_empty()
    );
    assert!(measure_slice_debt(directory.path(), &governance(0)).is_err());
    std::fs::create_dir(directory.path().join("module.ttl")).unwrap();
    assert!(crate::ratchet_surface_paths(directory.path()).is_err());
}
