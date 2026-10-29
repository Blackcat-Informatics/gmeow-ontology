// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::tptp::CanonAdmissionPublication;
use purrdf_lex::json::{
    Limits, Value,
    record::{DecodeError, ToJson, from_slice, to_vec},
};

#[test]
/// All original documents and ordinary domain statements stay in the producer
/// observation; byte hydration cannot restore the FOF writer's live authority.
fn canon_admission_roundtrip_preserves_complete_source_without_authorizing_proof() {
    let theory = compile_documents(&[
        ("domain.ttl", "<urn:s> <urn:p> <urn:o> .", None),
        (
            "classes.ttl",
            r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            <urn:union> a logic:Class; logic:unionOf (<urn:a> <urn:b>) .
            <urn:a> a logic:Class . <urn:b> a logic:Class .
        "#,
            None,
        ),
    ]);
    let publication = CanonAdmissionPublication::from_compiled_source(&theory);
    assert_eq!(
        publication.admission().selection,
        SourceSelection::all_classical_roots(&theory)
    );
    assert_eq!(
        project_tptp_fof(&theory, publication.admission()).status,
        FofProjectionStatus::Complete
    );
    let bytes = publication.to_bytes().unwrap();
    assert!(!bytes.ends_with(b"\n"));
    assert_eq!(
        bytes,
        include_bytes!("fixtures/canon-publication-v1.json")
            .strip_suffix(b"\n")
            .expect("original publication reference has one storage LF")
    );
    let loaded = CanonAdmissionPublication::from_bytes(&bytes).unwrap();
    assert_eq!(loaded.to_bytes().unwrap(), bytes);
    assert!(!loaded.admission().matches(&theory));
    let projection = project_tptp_fof(&theory, loaded.admission());
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(
        loaded
            .admission()
            .units
            .iter()
            .any(|unit| unit.documents.contains("domain.ttl"))
    );
    assert!(
        loaded
            .admission()
            .units
            .iter()
            .any(|unit| unit.documents.contains("classes.ttl"))
    );
    let second_theory = compile_documents(&[
        ("domain.ttl", "<urn:s> <urn:p> <urn:o> .", None),
        (
            "classes.ttl",
            r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            <urn:union> a logic:Class; logic:unionOf (<urn:a> <urn:b>) .
            <urn:a> a logic:Class . <urn:b> a logic:Class .
        "#,
            None,
        ),
    ]);
    assert_eq!(
        CanonAdmissionPublication::from_compiled_source(&second_theory)
            .to_bytes()
            .unwrap(),
        bytes
    );

    let mut observation = include_str!("fixtures/canon-observation-v1.txt").lines();
    let input = observation
        .next()
        .and_then(|line| line.strip_prefix("OBSERVATION_INPUT="))
        .expect("complete original observational input");
    let output = observation
        .next()
        .and_then(|line| line.strip_prefix("OBSERVATION_BYTES="))
        .expect("complete original observational output");
    assert!(
        observation.next().is_none(),
        "complete original observation reference consumed"
    );
    let observed = CanonAdmissionPublication::from_bytes(input.as_bytes())
        .expect("raw observational AST metadata is retained without authoring admission");
    assert!(!observed.admission().matches(&theory));
    assert_eq!(
        observed.to_bytes().expect("complete native observation"),
        output.as_bytes()
    );
    assert_eq!(
        CanonAdmissionPublication::from_bytes(output.as_bytes())
            .expect("canonical observation hydrates again")
            .to_bytes()
            .expect("repeated native observational encoding"),
        output.as_bytes()
    );

    let mut selections = include_str!("fixtures/source-selection-identities-v1.txt").lines();
    while let Some(line) = selections.next() {
        let preimage = line
            .strip_prefix("SELECTION_PREIMAGE=")
            .expect("complete original selection preimage");
        let selection: SourceSelection = from_slice(preimage.as_bytes())
            .expect("native selection reader covers every original variant");
        assert_eq!(to_vec(&selection), preimage.as_bytes());
        let digest = selections
            .next()
            .and_then(|line| line.strip_prefix("SELECTION_DIGEST="))
            .expect("original selection digest");
        assert_eq!(selection.digest(), digest);
    }
}

#[test]
/// Unsupported selected owners remain visible rather than disappearing behind
/// a successful producer publication, and changed selection/profile is rejected.
fn canon_admission_keeps_refusals_and_rejects_incompatible_observations() {
    let theory = compile(
        r#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        <urn:domain> <urn:p> <urn:o> .
        <urn:restricted> a logic:Class; logic:onProperty <urn:p>;
            logic:allValuesFrom <http://www.w3.org/2001/XMLSchema#integer> .
    "#,
    );
    let bytes = CanonAdmissionPublication::from_compiled_source(&theory)
        .to_bytes()
        .unwrap();
    let loaded = CanonAdmissionPublication::from_bytes(&bytes).unwrap();
    assert!(loaded.admission().blockers().count() > 0);
    let original = purrdf_lex::json::read_slice(&bytes, Limits::DEFAULT)
        .expect("native original admission document");
    for (path, value) in [
        (vec!["schema_version"], 2_u32.to_json()),
        (vec!["admission", "profile", "id"], Value::from("unknown")),
        (
            vec!["admission", "selection", "owners"],
            Value::from("None"),
        ),
        (
            vec!["admission", "selection_digest"],
            Value::from("changed"),
        ),
    ] {
        let mut changed = original.clone();
        let mut target = &mut changed;
        for key in path {
            target = &mut target[key];
        }
        *target = value;
        assert!(
            CanonAdmissionPublication::from_bytes(
                purrdf_lex::json::write_compact(&changed).as_bytes()
            )
            .is_err()
        );
    }
    let invalid = CanonAdmissionPublication::from_bytes(b"not JSON")
        .expect_err("malformed input is refused with its native cause");
    assert!(invalid.is::<DecodeError>());
    assert_eq!(
        gmeow_errors::code::code_str(invalid.code()),
        "logic-compile.frontend"
    );
    assert!(invalid.to_string().contains("decode canon proof admission"));

    let mut probes = include_str!("fixtures/canon-admission-probes-v1.txt").lines();
    let mut count = 0;
    let mut named_record_refusals = 0;
    while let Some(label) = probes.next() {
        let label = label
            .strip_prefix("PROBE_LABEL=")
            .expect("original probe label");
        let input = probes
            .next()
            .and_then(|line| line.strip_prefix("PROBE_INPUT="))
            .expect("complete original probe input");
        let recorded = probes.next().expect("complete original probe outcome");
        let named_record_refusal = matches!(
            label,
            "legacy-positional-publication"
                | "legacy-positional-admission"
                | "legacy-positional-complexity"
                | "legacy-positional-class-definition"
        );
        let target_index_refusal = label == "index-maximum" && usize::BITS < 64;
        let outcome = CanonAdmissionPublication::from_bytes(input.as_bytes());
        if named_record_refusal || target_index_refusal || recorded == "PROBE_REFUSED" {
            if named_record_refusal || target_index_refusal {
                assert!(
                    recorded.starts_with("PROBE_OUTPUT="),
                    "originally accepted observation"
                );
            }
            // Named records deliberately replace legacy positional struct forms.
            // A target-sized index never acquires a wider domain from a fixture.
            assert!(outcome.is_err(), "native refusal for {label}");
            named_record_refusals += usize::from(named_record_refusal);
        } else {
            let expected = recorded
                .strip_prefix("PROBE_OUTPUT=")
                .expect("complete original accepted probe output");
            let publication = outcome.expect("originally accepted named observation hydrates");
            assert!(!publication.admission().matches(&theory));
            assert_eq!(
                publication
                    .to_bytes()
                    .expect("complete native probe output"),
                expected.as_bytes(),
                "{label}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 30);
    assert_eq!(named_record_refusals, 4);
}
