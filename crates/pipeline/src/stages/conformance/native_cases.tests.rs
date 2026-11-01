// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::Cell;

use gmeow_action_cache::{STORE_FORMAT_VERSION, StoreLimits};

use super::*;

#[test]
fn sibling_action_misses_share_native_source_and_warm_hits_need_no_parse() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.nq");
    std::fs::write(&path, "<urn:s> <urn:p> <urn:first> .\n").unwrap();
    let store = ActionStore::open(
        ActionStore::default_root(root.path()),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let first = NativeInput::new(root.path(), &path).unwrap();
    let identity = Cell::new(std::ptr::null::<RdfDataset>());
    let observe = |dataset: &RdfDataset| -> Result<String, gmeow_errors::Diag> {
        if identity.get().is_null() {
            identity.set(std::ptr::from_ref(dataset));
        } else {
            assert_eq!(identity.get(), std::ptr::from_ref(dataset));
        }
        Ok("selected observation".to_owned())
    };
    first.observe(&store, "synthetic-first", observe).unwrap();
    first.observe(&store, "synthetic-second", observe).unwrap();
    let native = first
        .observe_native(&store, "class-source-admission-v1", |dataset| {
            observe(dataset)?;
            let gmeow_conformance::native_observation::NativeCaseObservation::ClassSourceAdmission(
                value,
            ) = gmeow_conformance::native_observation::observe(
                dataset,
                *purrdf_hash::blake3::hash(&first.bytes).as_bytes(),
                gmeow_conformance::native_observation::NativeCaseOperation::ClassSourceAdmission,
            )?
            else {
                panic!("selected source operation")
            };
            Ok(value)
        })
        .unwrap();
    assert!(first.dataset.get().is_some());

    let warm = NativeInput::new(root.path(), &path).unwrap();
    for operation in ["synthetic-first", "synthetic-second"] {
        let value: String = warm
            .observe(&store, operation, |_| panic!("warm action executed"))
            .unwrap();
        assert_eq!(value, "selected observation");
    }
    let restored: gmeow_conformance::native_observation::SourceAdmissionObservation = warm
        .observe_native(&store, "class-source-admission-v1", |_| {
            panic!("warm native action executed")
        })
        .unwrap();
    assert_eq!(restored, native);
    assert!(warm.dataset.get().is_none());

    std::fs::write(&path, "<urn:s> <urn:p> <urn:second> .\n").unwrap();
    let changed = NativeInput::new(root.path(), &path).unwrap();
    let calls = Cell::new(0);
    let value = changed
        .observe(&store, "synthetic-first", |_| {
            calls.set(calls.get() + 1);
            Ok("changed source".to_owned())
        })
        .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(value, "changed source");
    assert!(changed.dataset.get().is_some());
}

#[test]
fn native_parse_failure_never_initializes_an_input_or_publishes_an_observation() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.nq");
    std::fs::write(&path, "broken source").unwrap();
    let store = ActionStore::open(
        ActionStore::default_root(root.path()),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let input = NativeInput::new(root.path(), &path).unwrap();
    for operation in ["synthetic-first", "synthetic-second"] {
        assert!(
            input
                .observe::<String>(&store, operation, |_| panic!("invalid source admitted"))
                .is_err()
        );
    }
    assert!(
        input
            .observe_native::<gmeow_conformance::native_observation::SourceAdmissionObservation>(
                &store,
                "class-source-admission-v1",
                |_| panic!("invalid native source admitted")
            )
            .is_err()
    );
    assert!(input.dataset.get().is_none());
}

#[test]
fn native_class_admission_channel_retains_causal_outcomes_and_validates_shadowed_values() {
    use gmeow_conformance::native_observation::{NativeCaseObservation, NativeCaseOperation};
    let bytes = b"<urn:s> <urn:p> <urn:o> .";
    let dataset = parse_dataset(bytes, NativeRdfFormat::NQuads.media_type(), None).unwrap();
    let NativeCaseObservation::ClassSourceAdmission(observation) =
        gmeow_conformance::native_observation::observe(
            &dataset,
            *purrdf_hash::blake3::hash(bytes).as_bytes(),
            NativeCaseOperation::ClassSourceAdmission,
        )
        .unwrap()
    else {
        panic!("selected source operation")
    };
    let mut ledger = gmeow_errors::DiagLedger::new();
    let premise = ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("authored source premise"))
            .with_focus("urn:class-admission:input"),
        gmeow_errors::StageId::new("class-admission-input"),
    );
    let refusal = ledger.record(
        gmeow_errors::Diag::from(std::io::Error::other("authored admission refusal"))
            .with_focus("urn:class-admission:output")
            .with_antecedents([premise]),
        gmeow_errors::StageId::new("class-admission-output"),
    );
    let mut earlier = observation.clone();
    earlier.admission.contract = "earlier raw observation".into();
    let observations = Admissions::from([
        ("authored".into(), Ok(observation)),
        ("refused".into(), Err(refusal.clone())),
    ]);
    let wire =
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(&observations));
    assert_eq!(
        admissions_from_bytes(wire.as_bytes()).unwrap(),
        observations
    );
    assert_eq!(refusal.causes.len(), 1);
    assert_eq!(
        refusal.root.antecedents.as_ref(),
        &[refusal.causes[0].fingerprint]
    );
    let earlier =
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_result_to_json(&Ok(earlier)));
    let duplicate = wire.replacen('{', &format!("{{\"authored\":{earlier},"), 1);
    assert_eq!(
        admissions_from_bytes(duplicate.as_bytes()).unwrap(),
        observations
    );
    for invalid in [
        wire.replacen('{', "{\"authored\":{\"Ok\":null},", 1),
        r#"{"authored":{"Ok":null,"Err":null}}"#.to_owned(),
        r#"{"authored":{"Err":{}}}"#.to_owned(),
    ] {
        assert!(
            admissions_from_bytes(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
