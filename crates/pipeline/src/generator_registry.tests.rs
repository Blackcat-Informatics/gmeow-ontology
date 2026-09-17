// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
/// The registry retains every required producer, including canonical abstract projection.
fn registry_has_expected_generators() {
    let names: Vec<_> = generator_names();
    assert!(names.contains(&"canonical-abstract"));
    assert!(names.contains(&"mappings"));
    assert!(names.contains(&"statements"));
    assert!(names.contains(&"gts"));
    assert!(names.contains(&"docs"));
}

#[test]
/// Canonical abstract projection declares exactly one source and all three consumers.
fn canonical_abstract_registry_names_its_one_source_and_three_targets() {
    let generator = generator_by_name("canonical-abstract").expect("generator registered");
    assert_eq!(generator.sources, ["metadata/gmeow-abstract.txt"]);
    assert_eq!(
        generator.outputs,
        [
            "ontology/gmeow.ttl",
            "metadata/gmeow-self.ttl",
            "CITATION.cff",
        ]
    );
}

#[test]
fn mappings_outputs_match_issue_example() {
    let generator = generator_by_name("mappings").unwrap();
    assert!(generator.outputs.contains(&"generated/mappings/"));
    assert!(generator.outputs.contains(&"generated/projections/"));
    assert!(
        generator
            .outputs
            .contains(&"generated/queries/projections/")
    );
}

#[test]
fn retained_product_paths_exclude_the_ignored_generated_projection() {
    // `generated/` is a git-ignored local projection materialized by `make check`,
    // never staged by `make commit`; only the two retained products remain tracked.
    assert_eq!(
        retained_product_paths(),
        vec!["catalog-v001.xml", "packages/python/gmeow_models/"]
    );
}

#[test]
fn generator_order_puts_dependencies_first() {
    let (order, cycle) = generator_order();
    assert!(
        cycle.is_none(),
        "generator dependency graph has a cycle: {cycle:?}"
    );
    let pos = |name| order.iter().position(|n| *n == name).unwrap();
    assert!(pos("logic-compile") < pos("mappings"));
    assert!(pos("mappings") < pos("metadata"));
}
