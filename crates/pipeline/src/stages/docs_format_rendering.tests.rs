// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn derived_preservation_matches_the_honest_join() {
    assert_eq!(
        derived_preservation(DocFormat::Site),
        PreservationKind::SoundUnder
    );
    assert_eq!(
        derived_preservation(DocFormat::Mdbook),
        PreservationKind::SoundUnder
    );
    assert_eq!(
        derived_preservation(DocFormat::Pdf),
        PreservationKind::ValidationOnly
    );
    assert_eq!(
        derived_preservation(DocFormat::Snippets),
        PreservationKind::ValidationOnly
    );
    for format in DocFormat::ALL {
        assert_ne!(derived_preservation(format), PreservationKind::Exact);
    }
}

#[test]
fn declared_dag_edges_match_the_composition_legs() {
    use gmeow_docs::formats::PROJECTION_DAG_EDGES;
    use std::collections::BTreeSet;

    let surface_to_format: Vec<(String, DocFormat)> = DocFormat::ALL
        .iter()
        .map(|&format| (surface(format), format))
        .collect();
    let mut derived = BTreeSet::new();
    for target in DocFormat::ALL {
        for key in composition_leg_keys(target) {
            if let Some(leg) = legs().into_iter().find(|leg| &leg.key == key)
                && let Some(&(_, source)) = surface_to_format
                    .iter()
                    .find(|(candidate, _)| *candidate == leg.source)
                && leg.target_fmt == Some(target)
            {
                derived.insert((source, target));
            }
        }
    }
    assert_eq!(
        PROJECTION_DAG_EDGES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        derived
    );
}

#[test]
fn leg_targets_come_from_the_single_leg_table() {
    for leg in legs() {
        assert_eq!(leg_target_fmt(leg.key), leg.target_fmt);
    }
    let known: std::collections::HashSet<&str> = legs().iter().map(|leg| leg.key).collect();
    for format in DocFormat::ALL {
        for key in composition_leg_keys(format) {
            assert!(known.contains(key), "unknown composition leg {key}");
        }
    }
}

#[test]
fn loss_ledger_carries_each_formats_dropped_capabilities() {
    let mut ledger = Vec::new();
    let mut loss = LossLedger::new();
    fold_docs_format_loss(&mut ledger, &mut loss);
    assert_eq!(ledger.len(), DocFormat::ALL.len());
    for format in DocFormat::ALL {
        let target = format!("docs-format:{}", format.slug());
        let drops = loss.projection_drops_for(&target);
        assert!(!drops.is_empty());
        for capability in &format_capabilities(format).dropped {
            assert!(drops.iter().any(|drop| drop.contains(capability.slug())));
        }
    }
}
