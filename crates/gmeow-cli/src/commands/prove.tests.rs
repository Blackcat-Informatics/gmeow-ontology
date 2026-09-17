// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

use super::{ProverFlavor, problem_aliases, prover_passes, run_bounded_process};

fn script(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("scratch directory");
    let path = directory.path().join("stub-prover");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write stub");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("make stub executable");
    (directory, path)
}

#[test]
fn bounded_process_drains_output_after_retention_limit() {
    let (_directory, binary) =
        script("i=0\nwhile [ \"$i\" -lt 5000 ]; do\n  printf '0123456789'\n  i=$((i + 1))\ndone");
    let receipt = run_bounded_process(&binary, &[], None, Duration::from_secs(5), 128)
        .expect("bounded child");
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(receipt.stdout.total_bytes, 50_000);
    assert_eq!(receipt.stdout.retained.len(), 128);
    assert!(receipt.stdout.truncated);
}

#[test]
fn parent_deadline_reaps_the_selected_process_group() {
    let (_directory, binary) = script("sleep 30 &\nwait");
    let started = std::time::Instant::now();
    let receipt = run_bounded_process(&binary, &[], None, Duration::from_millis(50), 128)
        .expect("timed child receipt");
    assert!(receipt.timed_out);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "deadline must not wait for a descendant-held pipe"
    );
}

#[test]
fn problem_aliases_bind_digest_and_actual_file_name() {
    let aliases = problem_aliases(
        std::path::Path::new("/tmp/gmeow_deadbeef_random.p"),
        "012345",
    );
    assert!(aliases.contains("gmeow_012345"));
    assert!(aliases.contains("gmeow_deadbeef_random.p"));
    assert!(aliases.contains("gmeow_deadbeef_random"));
}

#[test]
fn prover_passes_request_supported_proof_and_model_artifacts() {
    let e = prover_passes(ProverFlavor::EProver, 17);
    assert_eq!(e.len(), 1);
    assert!(e[0].1.iter().any(|argument| argument == "--proof-object"));
    assert!(e[0].1.iter().any(|argument| argument == "--cpu-limit=17"));
    assert!(
        !e[0]
            .1
            .iter()
            .any(|argument| argument == "-s" || argument == "--silent")
    );

    let vampire = prover_passes(ProverFlavor::Vampire, 19);
    assert_eq!(vampire.len(), 2);
    assert_eq!(vampire[0].0, "refutation");
    assert_eq!(vampire[1].0, "finite-model");
    for (_, arguments) in &vampire {
        assert!(arguments.windows(2).any(|pair| pair == ["--proof", "tptp"]));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--output_mode", "szs"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--input_syntax", "tptp"])
        );
    }
    assert!(
        vampire[0]
            .1
            .windows(2)
            .any(|pair| pair == ["--schedule", "casc"])
    );
    assert!(
        vampire[1]
            .1
            .windows(2)
            .any(|pair| pair == ["--schedule", "casc_sat"])
    );
}
