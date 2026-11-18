// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{FilePlan, Sink};
use crate::OutputFormat;
use crate::commands::prove::ProveRequest;

fn request(inputs: &[std::path::PathBuf]) -> ProveRequest<'_> {
    ProveRequest {
        inputs,
        dir: None,
        modules: &[],
        max_steps: None,
        step_cap: None,
        format: OutputFormat::Json,
        gate: false,
        evidence_out: None,
        proof_out: None,
        check_proof: None,
        out: None,
    }
}

/// PROTECTS: all prover sinks and stale-output removal stay in the admitted
/// directory when a parent symlink switches onto semantic or replay inputs.
#[cfg(unix)]
#[test]
fn switched_parent_cannot_redirect_publication_or_removal_onto_loaded_inputs() {
    use std::os::unix::fs::symlink;

    for sink in [Sink::Fof, Sink::Proof, Sink::Evidence] {
        for basename in ["module.ttl", "proof.cbor"] {
            let root = tempfile::tempdir().expect("isolated parent-swap fixture");
            let inputs = root.path().join("inputs");
            let outputs = root.path().join("outputs");
            std::fs::create_dir(&inputs).expect("input directory");
            std::fs::create_dir(&outputs).expect("output directory");
            let source = inputs.join("module.ttl");
            let proof = inputs.join("proof.cbor");
            let source_bytes = b"<urn:s> <urn:p> <urn:o> .";
            let proof_bytes = b"exact proof input";
            std::fs::write(&source, source_bytes).expect("semantic source");
            std::fs::write(&proof, proof_bytes).expect("replay source");
            let link = root.path().join("selected-parent");
            symlink(&outputs, &link).expect("initial output parent");
            let path = link.join(basename);
            let selected = [source.clone()];
            let mut requested = request(&selected);
            requested.check_proof = Some(&proof);
            match sink {
                Sink::Fof => requested.out = Some(&path),
                Sink::Proof => requested.proof_out = Some(&path),
                Sink::Evidence => requested.evidence_out = Some(&path),
            }
            let plan = match FilePlan::open(&requested) {
                Ok(plan) => plan,
                Err(_) => panic!("distinct selected files admit"),
            };
            std::fs::remove_file(&link).expect("switch fixture symlink");
            symlink(&inputs, &link).expect("redirect onto protected input directory");
            assert_eq!(plan.sources[0].read().expect("held source"), source_bytes);
            let replay = plan.replay_input().expect("held proof input");
            assert_eq!(replay.read().expect("held proof bytes"), proof_bytes);
            plan.write(sink, b"new selected output")
                .expect("held-parent publication");
            assert_eq!(
                std::fs::read(outputs.join(basename)).expect("actual output"),
                b"new selected output"
            );
            assert_eq!(
                std::fs::read(&source).expect("source retained"),
                source_bytes
            );
            assert_eq!(std::fs::read(&proof).expect("proof retained"), proof_bytes);
            plan.remove(sink).expect("held-parent stale cleanup");
            assert!(!outputs.join(basename).exists());
            assert_eq!(
                std::fs::read(&source).expect("source retained"),
                source_bytes
            );
            assert_eq!(std::fs::read(&proof).expect("proof retained"), proof_bytes);
        }
    }
}

/// PROTECTS: source loading reads the admitted file description after its path
/// is replaced, and truncation refuses a target that became a protected hardlink.
#[test]
fn opened_source_is_loaded_and_actual_output_identity_is_checked_before_truncation() {
    let root = tempfile::tempdir().expect("isolated input identity fixture");
    let source = root.path().join("module.ttl");
    let held = root.path().join("held-source.ttl");
    let output = root.path().join("report.json");
    let original = b"<urn:original> <https://blackcatinformatics.ca/logic/instanceOf> <urn:A> .";
    std::fs::write(&source, original).expect("source");
    let inputs = [source.clone()];
    let mut requested = request(&inputs);
    requested.evidence_out = Some(&output);
    let plan = match FilePlan::open(&requested) {
        Ok(plan) => plan,
        Err(_) => panic!("distinct output admits"),
    };
    std::fs::rename(&source, &held).expect("retain original file under distinct name");
    std::fs::write(&source, "replacement path contents").expect("replacement source");
    std::fs::hard_link(&held, &output).expect("output now aliases actual loaded input");
    let loaded = match super::super::load_theory(&plan.sources) {
        Ok(loaded) => loaded,
        Err(_) => panic!("the actual opened source is valid Turtle"),
    };
    assert!(
        loaded
            .theory
            .program()
            .axioms
            .iter()
            .any(|axiom| axiom.subject == "urn:original")
    );
    assert_eq!(
        loaded.identities[0].content_digest,
        purrdf_hash::hex::encode(purrdf_hash::blake3::hash(original).as_bytes())
    );
    assert!(plan.write(Sink::Evidence, b"must not truncate").is_err());
    assert_eq!(std::fs::read(&held).expect("original intact"), original);
    assert_eq!(std::fs::read(&output).expect("hardlink intact"), original);
    assert_eq!(
        std::fs::read(&source).expect("replacement intact"),
        b"replacement path contents"
    );
}
