// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::stages::source_load::ParsedAuthoredSources;

const TINY: &str = r#"
<urn:C> <https://blackcatinformatics.ca/logic/unionOf> <urn:cell> .
<urn:cell> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:A> .
<urn:cell> <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .
"#;

#[test]
fn complete_list_observation_uses_the_supplied_projection() {
    let catalog = SourceCatalog::from_sources(ParsedAuthoredSources::synthetic(TINY)).unwrap();
    let theory = catalog.compiled_logic().unwrap();
    let canonical = purrdf::parse_dataset(TINY.as_bytes(), "application/n-quads", None).unwrap();
    let mut artifacts = BTreeMap::new();
    record(&catalog, &theory, &canonical, &mut artifacts).unwrap();
    let observation =
        gmeow_logic::reason::refute::decode_list_corpus_observation(&artifacts[CHANNEL])
            .unwrap()
            .unwrap();
    assert_eq!(observation.source_catalog_id, catalog.identity());
    assert_eq!(observation.list_sizes["urn:cell"], 1);
    assert!(observation.list_errors.is_empty());
    assert!(observation.admission_defects.is_empty());
    assert!(observation.selected_world_count > 0);
}

#[test]
fn missing_cell_field_remains_a_recorded_defect() {
    let catalog = SourceCatalog::from_sources(ParsedAuthoredSources::synthetic(TINY)).unwrap();
    let theory = catalog.compiled_logic().unwrap();
    let truncated = TINY
        .lines()
        .filter(|line| !line.contains("#rest>"))
        .collect::<Vec<_>>()
        .join("\n");
    let canonical =
        purrdf::parse_dataset(truncated.as_bytes(), "application/n-quads", None).unwrap();
    let mut artifacts = BTreeMap::new();
    record(&catalog, &theory, &canonical, &mut artifacts).unwrap();
    let observation =
        gmeow_logic::reason::refute::decode_list_corpus_observation(&artifacts[CHANNEL])
            .unwrap()
            .unwrap();
    assert!(!observation.list_errors.is_empty());
    assert!(
        observation
            .admission_defects
            .iter()
            .any(|issue| matches!(issue, RefutationSourceIssue::IncompleteList { .. }))
    );
    assert!(observation.list_sizes.is_empty());
}
