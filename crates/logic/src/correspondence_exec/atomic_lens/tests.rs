// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::correspondence_exec::CarrierAtoms;
use gmeow_logic_compile::ir::DischargeVerdict;

pub(super) const SOURCE: &str = "urn:gmeow:example:magnitude";
pub(super) const VIEW: &str = "urn:gmeow:example:rmMagnitude";
pub(super) const SUBJECT: &str = "urn:gmeow:example:observation";
pub(super) const GRAPH: &str = "urn:gmeow:example:standpoint";
pub(super) const EMPTY_GRAPH: &str = "urn:gmeow:example:empty-context";
pub(super) const REIFIER: &str = "urn:gmeow:example:claim";

pub(super) fn source() -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri(SUBJECT);
    let property = builder.intern_iri(SOURCE);
    let old = builder.intern_iri("urn:gmeow:example:old");
    let graph = builder.intern_iri(GRAPH);
    let empty = builder.intern_iri(EMPTY_GRAPH);
    builder.declare_named_graph(empty);
    builder.push_quad(subject, property, old, Some(graph));
    let quality = builder.intern_iri("urn:gmeow:example:persistent-quality");
    let inheres = builder.intern_iri("urn:gmeow:example:inheresIn");
    builder.push_quad(quality, inheres, subject, Some(graph));
    let statement = builder.intern_triple(subject, property, old);
    let reifier = builder.intern_iri(REIFIER);
    builder.push_reifier_in_graph(reifier, statement, Some(graph));
    let standpoint = builder.intern_iri("https://blackcatinformatics.ca/logic/standpoint");
    builder.push_annotation_in_graph(reifier, standpoint, graph, Some(graph));
    let provenance = builder.intern_iri("http://www.w3.org/ns/prov#wasDerivedFrom");
    let origin = builder.intern_iri("urn:gmeow:example:original-record");
    builder.push_annotation_in_graph(reifier, provenance, origin, Some(graph));
    let loss = builder.intern_iri("urn:gmeow:example:loss-evidence");
    builder.push_quad(subject, loss, quality, None);
    builder.freeze().unwrap()
}

pub(super) fn edit(subject: &str, predicate: &str, value: Option<&str>) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let graph = builder.intern_iri(GRAPH);
    let empty = builder.intern_iri(EMPTY_GRAPH);
    builder.declare_named_graph(graph);
    builder.declare_named_graph(empty);
    if let Some(value) = value {
        let subject = builder.intern_iri(subject);
        let predicate = builder.intern_iri(predicate);
        let value = builder.intern_iri(value);
        builder.push_quad(subject, predicate, value, Some(graph));
    }
    builder.freeze().unwrap()
}

fn lens() -> AtomicPropertyLens {
    AtomicPropertyLens::new(SOURCE, VIEW, false, ViewLimits::default()).unwrap()
}

#[test]
fn nonempty_prior_retains_standpoint_provenance_and_loss_evidence() {
    let original = source();
    let state = lens().acquire(Arc::clone(&original)).unwrap();
    let updated = state
        .put_shared_scopes(edit(SUBJECT, VIEW, Some("urn:gmeow:example:new")))
        .unwrap();
    let actual = updated.carrier.materialize().unwrap();
    let source_atoms = CarrierAtoms::read(&original);
    let result_atoms = CarrierAtoms::read(&actual);
    assert_eq!(source_atoms.graphs, result_atoms.graphs);
    let removed: Vec<_> = source_atoms
        .assertions
        .difference(&result_atoms.assertions)
        .collect();
    let added: Vec<_> = result_atoms
        .assertions
        .difference(&source_atoms.assertions)
        .collect();
    assert_eq!(
        removed.len(),
        1,
        "only the selected assertion may leave the source"
    );
    assert_eq!(added.len(), 1, "only the edited focus may enter it");
    assert_eq!(removed[0].1, SOURCE);
    assert_eq!(added[0].1, SOURCE);
    assert_eq!(added[0].2, "urn:gmeow:example:new");
    assert_eq!(removed[0].3.as_deref(), Some(GRAPH));
    assert_eq!(added[0].3.as_deref(), Some(GRAPH));
    assert_eq!(actual.reifier_quads().count(), 1);
    assert_eq!(actual.annotation_quads().count(), 2);
}

#[test]
fn four_laws_use_independent_edit_and_initial_state_domains() {
    let state = lens().acquire(source()).unwrap();
    let first = edit(
        SUBJECT,
        VIEW,
        Some("urn:gmeow:example:first-independent-value"),
    );
    let second = edit(
        "urn:gmeow:example:other-observation",
        VIEW,
        Some("urn:gmeow:example:second-independent-value"),
    );
    for outcome in [
        state.check_get_put("nonempty-acquisition").unwrap(),
        state
            .check_put_get("first-edit", Arc::clone(&first))
            .unwrap(),
        state
            .check_put_get("second-edit", Arc::clone(&second))
            .unwrap(),
        state
            .check_put_put("successive-edits", first, second)
            .unwrap(),
        state
            .check_section(
                "empty-initial-with-witness",
                AtomicInitialState::EmptyWithComplement,
            )
            .unwrap(),
    ] {
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "{outcome:?}"
        );
    }
}

#[test]
fn deleting_focus_keeps_complement_without_retaining_edit_history() {
    let original = source();
    let state = lens().acquire(Arc::clone(&original)).unwrap();
    let first = state
        .put_shared_scopes(edit(SUBJECT, VIEW, Some("urn:gmeow:example:transient")))
        .unwrap();
    let first_view = Arc::downgrade(first.get().dataset());
    let deleted = first.put_shared_scopes(edit(SUBJECT, VIEW, None)).unwrap();
    drop(first);
    assert!(
        first_view.upgrade().is_none(),
        "later puts must release earlier edits"
    );
    assert!(Arc::ptr_eq(
        &state.augmented.complement,
        &deleted.augmented.complement
    ));
    assert!(Arc::ptr_eq(
        deleted.augmented.complement.residual.base(),
        &original
    ));
    assert_eq!(deleted.carrier.stats().work.materializations, 0);
    assert_eq!(
        deleted
            .augmented
            .complement
            .residual
            .stats()
            .work
            .copied_rows,
        0
    );
    assert_eq!(
        deleted
            .augmented
            .complement
            .residual
            .stats()
            .work
            .copied_text_bytes,
        0
    );
    assert_eq!(deleted.carrier.named_graphs().count(), 2);
    assert_eq!(deleted.get().dataset().quads().count(), 0);
    assert_eq!(deleted.carrier.reifier_quads().count(), 1);
    assert_eq!(deleted.carrier.annotation_quads().count(), 2);
    assert_eq!(
        state
            .check_put_get("deletion", edit(SUBJECT, VIEW, None))
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationDischarged
    );
}

#[test]
fn recovery_retains_complement_after_original_state_is_dropped() {
    let original = source();
    let state = lens().acquire(Arc::clone(&original)).unwrap();
    let augmented = state.get();
    drop(state);
    let recovered = augmented
        .restore(AtomicInitialState::EmptyWithComplement)
        .unwrap();
    let actual = recovered.carrier.materialize().unwrap();
    assert!(CarrierAtoms::read(&original) == CarrierAtoms::read(&actual));
}

#[test]
fn edits_cannot_silently_change_scope_or_statement_metadata() {
    let state = lens().acquire(source()).unwrap();
    assert!(
        state
            .put_shared_scopes(edit(SUBJECT, SOURCE, Some("urn:gmeow:example:new")))
            .is_err()
    );
    assert!(
        state
            .put_shared_scopes(RdfDatasetBuilder::new().freeze().unwrap())
            .is_err()
    );
    assert!(
        state
            .put_shared_scopes(edit(REIFIER, VIEW, Some("urn:gmeow:example:new")))
            .is_err()
    );
    assert!(state.put_shared_scopes(source()).is_err());
    assert_eq!(
        state
            .check_get_put("unchanged-after-refused-edits")
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationDischarged
    );
}

#[test]
fn acquisition_requires_statement_classification_before_selecting_its_focus() {
    let original = source();
    let unfolded = purrdf::native_quads::flat_dataset_from_quads(
        &purrdf::native_quads::flat_rdf_quads_from_dataset(&original),
    )
    .unwrap();
    let error = lens().acquire(unfolded).unwrap_err();
    assert!(error.to_string().contains("native statement bindings"));
    assert!(lens().acquire(original).is_ok());
}

#[test]
fn edited_quoted_value_keeps_its_scoped_quality_identity_in_the_complement() {
    fn carrier(predicate: &str, lexical: &str, with_complement: bool) -> Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        let quality = builder.intern_blank("quality", purrdf::BlankScope(7));
        let observation = builder.intern_iri(SUBJECT);
        let property = builder.intern_iri(predicate);
        let empty = builder.intern_blank("empty-context", purrdf::BlankScope(9));
        builder.declare_named_graph(empty);
        let quoted_property = builder.intern_iri("urn:gmeow:example:reportedLabel");
        let mut label = purrdf::RdfLiteral::language_tagged(lexical, "ar");
        label.direction = Some(purrdf::RdfTextDirection::Rtl);
        let label = builder.intern_literal(label);
        let quoted = builder.intern_triple(quality, quoted_property, label);
        builder.push_quad(quality, property, quoted, None);
        if with_complement {
            let owns = builder.intern_iri("urn:gmeow:example:hasQuality");
            builder.push_quad(observation, owns, quality, None);
        }
        builder.freeze().unwrap()
    }
    let original = carrier(SOURCE, "original", true);
    let edited = carrier(VIEW, "independent edit", false);
    let state = lens().acquire(original).unwrap();
    let updated = state.put_shared_scopes(Arc::clone(&edited)).unwrap();
    let actual = updated.carrier.materialize().unwrap();
    let property = actual.term_id_by_iri(SOURCE).unwrap();
    let quad = actual
        .quads_for_pattern(None, Some(property), None, GraphMatch::Default)
        .next()
        .unwrap();
    let expected = edited.quads().next().unwrap();
    assert_eq!(actual.term_value(quad.s), edited.term_value(expected.s));
    assert_eq!(actual.term_value(quad.o), edited.term_value(expected.o));
    let owns = actual
        .term_id_by_iri("urn:gmeow:example:hasQuality")
        .unwrap();
    assert!(
        actual
            .quads_for_pattern(None, Some(owns), Some(quad.s), GraphMatch::Default)
            .next()
            .is_some()
    );
    assert_eq!(
        state
            .check_put_get("scoped-quoted-edit", edited)
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationDischarged
    );
    assert_eq!(
        updated
            .check_section(
                "scoped-quoted-recovery",
                AtomicInitialState::EmptyWithComplement
            )
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationDischarged
    );
}

#[test]
fn inverse_focus_updates_source_direction_and_preserves_residual() {
    let inverse = AtomicPropertyLens::new(SOURCE, VIEW, true, ViewLimits::default()).unwrap();
    let state = inverse.acquire(source()).unwrap();
    let view = edit("urn:gmeow:example:new-object", VIEW, Some(SUBJECT));
    let updated = state.put_shared_scopes(Arc::clone(&view)).unwrap();
    let actual = updated.carrier.materialize().unwrap();
    let selected = actual.term_id_by_iri(SOURCE).unwrap();
    let rows: Vec<_> = actual
        .quads_for_pattern(None, Some(selected), None, GraphMatch::Any)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        actual.term_value(rows[0].s),
        TermValue::Iri(SUBJECT.to_owned())
    );
    assert_eq!(
        actual.term_value(rows[0].o),
        TermValue::Iri("urn:gmeow:example:new-object".to_owned())
    );
    assert_eq!(
        state
            .check_put_get("inverse-independent-edit", view)
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationDischarged
    );
    assert_eq!(
        state.check_get_put("inverse-acquisition").unwrap().verdict,
        DischargeVerdict::ObligationDischarged
    );
}
