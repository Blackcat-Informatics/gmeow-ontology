// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact producer-owned class/source observations and native proof determinism.
//! Consumers never parse source RDF or execute a native reasoning operation.

use super::super::native_cases::{
    Admissions, CLASS_ADMISSION_CHANNEL, CLASS_DIAGNOSTIC_CHANNEL, CLASS_DIAGNOSTIC_WITNESSES,
    ClassDiagnostics, DETERMINISM_CHANNEL, DETERMINISM_INPUTS, Determinism, FULL_CORPORA,
};
use gmeow_conformance::paths::repo_root;
use gmeow_logic::reason::refute::ClassDiagnosticOutcome;
use std::collections::BTreeSet;

fn observed<T: serde::de::DeserializeOwned>(channel: &str) -> T {
    let bytes = gmeow_action_cache::selection::source_artifacts::load(
        &repo_root(),
        "stage-conformance",
        channel,
    )
    .expect("authenticated native operation observations");
    serde_json::from_slice(&bytes).expect("complete typed native observations")
}

fn assert_class_diagnostic_soundness(
    observations: &ClassDiagnostics,
    expected_inventory: &BTreeSet<String>,
    minimum_supported: usize,
) {
    let mut supported = 0usize;
    let mut contradictions = Vec::new();
    for key in expected_inventory {
        let result = observations
            .get(key)
            .unwrap_or_else(|| panic!("missing selected class diagnostic for {key}"))
            .as_ref()
            .unwrap_or_else(|error| panic!("class diagnostic failed for {key}: {error}"));
        result
            .validate()
            .unwrap_or_else(|error| panic!("{key}: {error}"));
        let ClassDiagnosticOutcome::Executed { execution } = result else {
            continue;
        };
        if !execution.has_conflict() {
            continue;
        }
        supported += 1;
        let profile_path = repo_root()
            .join(key)
            .parent()
            .expect("class diagnostic input parent")
            .join("profile.json");
        let profile: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&profile_path).expect("published source profile"),
        )
        .expect("valid published profile");
        match profile["w3c_published_verdict"].as_str() {
            Some("inconsistent") => {}
            Some(published) => contradictions.push(format!(
                "{key}: supported native class conflict contradicts W3C {published:?}"
            )),
            None => contradictions.push(format!(
                "{key}: supported class conflict has no published W3C verdict"
            )),
        }
    }
    assert_eq!(
        observations.keys().cloned().collect::<BTreeSet<_>>(),
        *expected_inventory
    );
    assert!(contradictions.is_empty(), "{}", contradictions.join("\n"));
    assert!(
        supported >= minimum_supported,
        "expected at least {minimum_supported} supported class proofs, saw {supported}"
    );
}

/// The ordinary producer retains the complete positive proof frontier. Negative
/// breadth controls are independently checked by the heavy-lane test below.
fn corpus_soundness_sweep_no_supported_class_conflict_contradicts_w3c() {
    let observations: ClassDiagnostics = observed(CLASS_DIAGNOSTIC_CHANNEL);
    let admissions: Admissions = observed(CLASS_ADMISSION_CHANNEL);
    let mut admission_inventory = BTreeSet::new();
    for corpus in FULL_CORPORA {
        for (slug, case) in super::common::case_slugs(&repo_root().join(corpus)) {
            let key = format!("{corpus}/{slug}/input.nq");
            let profile: serde_json::Value = serde_json::from_slice(
                &std::fs::read(case.join("profile.json")).expect("published source profile"),
            )
            .expect("valid published profile");
            if profile["verdict_mode"] == "class-source-admission" {
                admission_inventory.insert(key.clone());
                let observed = admissions
                    .get(&key)
                    .unwrap_or_else(|| panic!("missing selected admission for {key}"))
                    .as_ref()
                    .unwrap_or_else(|error| panic!("source admission failed for {key}: {error}"));
                observed.admission.validate().unwrap();
                assert!(observed.admission.outside_selection(), "{key}");
                assert_eq!(
                    observed.input_blake3,
                    *blake3::hash(&std::fs::read(case.join("input.nq")).unwrap()).as_bytes(),
                    "{key}"
                );
                assert_eq!(
                    profile["w3c_published_verdict"], "inconsistent",
                    "original W3C attribution: {key}"
                );
                continue;
            }
        }
    }
    let class_inventory = CLASS_DIAGNOSTIC_WITNESSES
        .iter()
        .map(|path| (*path).to_owned())
        .collect::<BTreeSet<_>>();
    assert_class_diagnostic_soundness(&observations, &class_inventory, 5);
    assert_eq!(
        admissions.keys().cloned().collect::<BTreeSet<_>>(),
        admission_inventory
    );
    assert!(class_inventory.is_disjoint(&admission_inventory));
    assert_eq!(
        admission_inventory,
        BTreeSet::from(["webont-i5-5-003", "webont-i5-5-004"].map(|slug| format!(
            "conformance/logic/cases/external/w3c-owl2-full-native/{slug}/input.nq"
        )))
    );
}

#[test]
#[ignore = "heavy lane only: exhaustive 152-case class-diagnostic corpus"]
fn exhaustive_class_diagnostic_soundness_sweep() {
    let root = repo_root();
    let observations = super::super::heavy::load(&root)
        .expect("authenticated exhaustive native conformance observations");
    let expected = super::super::native_cases::class_diagnostic_inputs(&root)
        .expect("complete class-diagnostic inventory")
        .into_iter()
        .map(|path| {
            path.strip_prefix(&root)
                .expect("class diagnostic inside checkout")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(expected.len(), 152);
    assert_class_diagnostic_soundness(&observations.class_diagnostics, &expected, 5);
}

/// Independently repeated full-native executions retain all worlds, completions,
/// source admission and complete proof DAGs for both original positive fixtures.
fn kernel_output_is_byte_stable_on_datatype_and_counting_inputs() {
    let observations: Determinism = observed(DETERMINISM_CHANNEL);
    assert_eq!(
        observations
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        DETERMINISM_INPUTS.iter().copied().collect::<BTreeSet<_>>()
    );
    for (path, result) in observations {
        let [first, second] = result.unwrap_or_else(|error| panic!("{path}: {error}"));
        first.validate_structure().unwrap();
        second.validate_structure().unwrap();
        assert_eq!(format!("{first:?}"), format!("{second:?}"), "{path}");
        assert_eq!(first, second, "{path}");
        assert!(
            first.has_conflict(),
            "{path}: determinism must exercise actual supported native output"
        );
    }
}

pub(super) fn authenticated_refutation_contracts() {
    corpus_soundness_sweep_no_supported_class_conflict_contradicts_w3c();
    kernel_output_is_byte_stable_on_datatype_and_counting_inputs();
}
