// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Report joins retain every target's source attribution without mixing in
/// another target or unrelated transcode witnesses from the same stores.
#[test]
fn report_view_preserves_target_boundaries_and_shared_source_evidence() {
    let mut compiler = LossLedger::new();
    compiler.record_projection_drops_attributed(
        "owl-dl",
        PreservationKind::SoundUnder,
        &["standpoint boundary".into()],
        &[("shared loss".into(), Some("urn:source:b".into()))],
    );
    compiler.record_transcode_loss("named-graph-dropped", "trig", "turtle", "other lane", 1);
    let mut mappings = compiler.clone();
    mappings.record_projection_drops_attributed(
        "owl-dl",
        PreservationKind::SoundUnder,
        &["standpoint boundary".into()],
        &[("shared loss".into(), Some("urn:source:a".into()))],
    );
    mappings.record_projection_drops_attributed(
        "sssom",
        PreservationKind::SoundUnder,
        &[],
        &[(
            "mapping-only loss".into(),
            Some("urn:source:mapping".into()),
        )],
    );
    let before = (compiler.to_nodes(), mappings.to_nodes());
    let view = ProjectionLossView::new(&[&compiler, &mappings]);
    assert_eq!(
        view.projection_drops_for("owl-dl"),
        ["standpoint boundary", "actual: shared loss"]
    );
    assert_eq!(
        view.term_source_drops("owl-dl"),
        [
            ("shared loss".into(), "urn:source:a".into()),
            ("shared loss".into(), "urn:source:b".into()),
        ]
    );
    assert_eq!(
        view.projection_drops_for("sssom"),
        ["actual: mapping-only loss"]
    );
    assert_eq!(
        view.term_source_drops("sssom"),
        [("mapping-only loss".into(), "urn:source:mapping".into())]
    );
    assert!(view.projection_drops_for("canonical-rdf12").is_empty());
    assert!(view.term_source_drops("canonical-rdf12").is_empty());
    assert_eq!((compiler.to_nodes(), mappings.to_nodes()), before);
}

#[test]
fn transcode_rows_round_trip_and_sort() {
    let mut store = LossLedger::new();
    // Deliberately out of (from,to,code) order and across two pairs (R1).
    store.record_transcode_loss("named-graph-dropped", "trig", "turtle", "graphs go", 1);
    store.record_transcode_loss("rdf12-star-unrepresentable", "trig", "turtle", "star go", 3);
    store.record_transcode_loss("owl-dl-projection", "turtle", "owl-dl", "dl drop", 2);

    let rows = store.transcode_rows();
    // Two distinct pairs never collapsed into one witness (R1).
    assert_eq!(rows.len(), 3);
    // Sorted by (from, to, code): trig<turtle pair first (named<rdf12), then turtle→owl-dl.
    assert_eq!(rows[0].from, "trig");
    assert_eq!(rows[0].code, "named-graph-dropped");
    assert_eq!(rows[0].count, 1);
    assert_eq!(rows[1].code, "rdf12-star-unrepresentable");
    assert_eq!(rows[1].count, 3);
    assert_eq!(rows[2].from, "turtle");
    assert_eq!(rows[2].to, "owl-dl");
    assert_eq!(rows[2].count, 2);
}

#[test]
fn transcode_rows_aggregate_multiple_observations_of_one_node() {
    // Two records sharing the SAME (code, from, to) hash-cons-merge into one
    // DiagNode carrying two observations. The read-back must aggregate ALL of
    // them — reading only the first would silently drop the second's count.
    let mut store = LossLedger::new();
    store.record_transcode_loss("named-graph-dropped", "trig", "turtle", "graphs go", 2);
    store.record_transcode_loss("named-graph-dropped", "trig", "turtle", "graphs go", 3);

    let rows = store.transcode_rows();
    // Still ONE row per (from, to, code) — the observations merged, not the rows.
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].from, "trig");
    assert_eq!(rows[0].to, "turtle");
    assert_eq!(rows[0].code, "named-graph-dropped");
    // Count is the SUM of every observation (2 + 3), not just the first (2).
    assert_eq!(rows[0].count, 5);
    // The static per-code note is preserved, never dropped.
    assert_eq!(rows[0].note, "graphs go");
}

#[test]
fn actual_drop_carries_structural_limitation_as_antecedent() {
    // U2 producer-side antecedent DAG: a concrete per-run drop is CAUSED BY the
    // target's declared structural limitation. `actual_drop_causes` reads that edge off
    // the actual witness's own antecedents and pairs each drop note with the stable
    // finding IRI of its cause — the provenance a consumer attaches as a related location.
    let mut store = LossLedger::new();
    store.record_projection_drops(
        "owl-dl",
        PreservationKind::SoundUnder,
        &["OWL-DL cannot carry full first-order formulas".to_owned()],
        &["logic:Formula #3 dropped as unsupported residue".to_owned()],
    );
    let causes = store.actual_drop_causes("owl-dl");
    assert_eq!(causes.len(), 1, "one (drop, cause) pair: {causes:?}");
    assert_eq!(
        causes[0].0,
        "logic:Formula #3 dropped as unsupported residue"
    );
    assert!(
        causes[0].1.starts_with("https://"),
        "the cause is the structural witness's stable finding IRI: {}",
        causes[0].1
    );

    // No fabrication: a target with NO declared structural limitation has no antecedent
    // edge, so no cause is asserted (epistemic-shape preservation).
    let mut bare = LossLedger::new();
    bare.record_projection_drops(
        "canonical-rdf12",
        PreservationKind::SoundUnder,
        &[],
        &["a per-run drop with no structural cause".to_owned()],
    );
    assert!(
        bare.actual_drop_causes("canonical-rdf12").is_empty(),
        "with no structural limitation there is no genuine cause — no fabricated antecedent"
    );

    // And the antecedent edge is genuinely ON the witness (not re-derived): the actual
    // node's `to_finding` projection surfaces the cause as a related location too.
    let finding = store
        .ledger
        .findings("logic-compile")
        .into_iter()
        .find(|f| f.code.contains("actual"))
        .expect("an actual-drop finding");
    assert!(
        !finding.related_locations.is_empty(),
        "to_finding must project the wired antecedent as a related location: {finding:?}"
    );
}

#[test]
fn attributed_actual_drops_carry_source_term_and_read_back_sorted() {
    // Term-attributed drops ride the actual observation's typed `observed` slot; the note
    // bytes are unchanged (still readable via `projection_drops_for`), and the structured
    // source term reads back via `term_source_drops`, sorted by (source, note). A drop with
    // no source term stays whole-program (never surfaces in `term_source_drops`).
    let mut store = LossLedger::new();
    store.record_projection_drops_attributed(
        "sssom",
        PreservationKind::SoundUnder,
        &[],
        &[
            (
                "gmeow:Agent close-match loses caveats".to_owned(),
                Some("https://blackcatinformatics.ca/gmeow/Agent".to_owned()),
            ),
            (
                "gmeow:Activity exact-match loses standpoint".to_owned(),
                Some("https://blackcatinformatics.ca/gmeow/Activity".to_owned()),
            ),
            ("a genuinely program-wide drop".to_owned(), None),
        ],
    );

    // All three notes survive as `gmeow:lossyDrop` (byte-identical to the unattributed
    // path — the attribution is additive).
    let drops = store.projection_drops_for("sssom");
    assert_eq!(
        drops,
        vec![
            "actual: a genuinely program-wide drop".to_owned(),
            "actual: gmeow:Activity exact-match loses standpoint".to_owned(),
            "actual: gmeow:Agent close-match loses caveats".to_owned(),
        ]
    );

    // Only the two attributed drops read back, sorted by (source term, note); the
    // program-wide drop is absent.
    let attributed = store.term_source_drops("sssom");
    assert_eq!(
        attributed,
        vec![
            (
                "gmeow:Activity exact-match loses standpoint".to_owned(),
                "https://blackcatinformatics.ca/gmeow/Activity".to_owned()
            ),
            (
                "gmeow:Agent close-match loses caveats".to_owned(),
                "https://blackcatinformatics.ca/gmeow/Agent".to_owned()
            ),
        ]
    );

    // A different target is isolated by focus (R1): no cross-target attribution bleed.
    assert!(store.term_source_drops("owl-dl").is_empty());

    // The attribution survives the transport round-trip (the compile-logic → mappings JSON
    // channel carries nodes, not the live store), so the report re-serialized in mappings
    // sees the same source terms.
    let round_tripped = LossLedger::from_nodes(store.to_nodes());
    assert_eq!(round_tripped.term_source_drops("sssom"), attributed);
}

#[test]
fn projection_drops_match_structural_then_prefixed_actual() {
    let mut store = LossLedger::new();
    let structural = vec!["z structural".to_owned(), "a structural".to_owned()];
    let actual = vec!["y actual".to_owned(), "b actual".to_owned()];
    store.record_projection_drops("owl-dl", PreservationKind::SoundUnder, &structural, &actual);

    let drops = store.projection_drops_for("owl-dl");
    assert_eq!(
        drops,
        vec![
            "a structural".to_owned(),
            "z structural".to_owned(),
            "actual: b actual".to_owned(),
            "actual: y actual".to_owned(),
        ]
    );
    // A different target is isolated by focus (R1): no cross-target bleed.
    assert!(store.projection_drops_for("gufo").is_empty());
}
