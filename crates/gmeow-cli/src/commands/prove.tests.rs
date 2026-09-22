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

#[test]
fn stream_read_failure_retains_partial_bytes_and_digest() {
    struct BrokenReader(bool);
    impl std::io::Read for BrokenReader {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            if self.0 {
                return Err(std::io::Error::other("synthetic read fault"));
            }
            self.0 = true;
            output[..4].copy_from_slice(&[0xff, 1, 2, 3]);
            Ok(4)
        }
    }
    let capture = std::sync::Mutex::new(super::empty_stream());
    assert!(super::drain_stream(BrokenReader(false), 2, &capture).is_err());
    let bytes = capture.into_inner().expect("capture lock");
    assert_eq!(bytes.retained, [0xff, 1]);
    assert_eq!(bytes.total_bytes, 4);
    assert!(bytes.truncated);
    assert_eq!(
        bytes.digest,
        blake3::hash(&[0xff, 1, 2, 3]).to_hex().to_string()
    );
    let projected = bytes.into_report();
    assert_eq!(projected.retained, [0xff, 1]);
    assert!(!projected.utf8_valid);
}

#[test]
fn failed_version_keeps_both_streams_and_process_identity() {
    let (_directory, binary) =
        script("printf 'partial identity'; printf 'version error' >&2; exit 7");
    let prover = super::ResolvedProver {
        flavor: ProverFlavor::EProver,
        digest: super::digest_file(&binary).expect("executable identity"),
        path: binary,
    };
    let error = super::prover_version(&prover).expect_err("version process failed");
    let report = super::ReportDiagnostic::from_diag(&error);
    assert_eq!(report.code, "PROVER_VERSION_CHILD_FAILED");
    let structured = report.diagnostic.expect("structured diagnostic");
    let receipt = &structured.fields["evidence"];
    assert_eq!(receipt["exit_code"], 7);
    assert_eq!(receipt["selection"]["arguments"][0], "--version");
    assert_eq!(receipt["selection"]["executable_digest"], prover.digest);
    assert_eq!(receipt["stdout"]["total_bytes"], 16);
    assert_eq!(receipt["stderr"]["total_bytes"], 13);
    assert_eq!(
        receipt["stdout"]["digest"],
        blake3::hash(b"partial identity").to_hex().to_string()
    );
    assert_eq!(
        receipt["stderr"]["digest"],
        blake3::hash(b"version error").to_hex().to_string()
    );
}

#[test]
fn spawn_failure_preserves_problem_and_original_io_cause() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let binary = directory.path().join("not-executable");
    std::fs::write(&binary, b"not an executable").expect("write input");
    let problem = directory.path().join("problem.p");
    std::fs::write(&problem, b"fof(a,axiom,p).\n").expect("write problem");
    let args = vec!["--auto".to_owned()];
    let error = run_bounded_process(&binary, &args, Some(&problem), Duration::from_secs(1), 32)
        .expect_err("selected file has no execute permission");
    let projected = super::ReportDiagnostic::from_diag(&error);
    assert_eq!(projected.code, "PROVER_SPAWN_FAILED");
    let evidence = projected.diagnostic.expect("evidence");
    assert!(evidence.causes.len() >= 2);
    let receipt = &evidence.fields["evidence"];
    assert_eq!(
        receipt["selection"]["arguments"][1],
        problem.display().to_string()
    );
    assert_eq!(
        receipt["selection"]["problem_digest"],
        blake3::hash(b"fof(a,axiom,p).\n").to_hex().to_string()
    );
    assert_eq!(receipt["stdout"]["total_bytes"], 0);
    assert_eq!(receipt["stderr"]["total_bytes"], 0);
    assert!(receipt["exit_code"].is_null());
}

#[test]
fn malformed_later_source_preserves_prior_and_failing_input_identity() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let first = directory.path().join("a.ttl");
    let second = directory.path().join("b.ttl");
    std::fs::write(&first, "<urn:s> <urn:p> <urn:o> .").expect("source");
    std::fs::write(&second, "this is not turtle").expect("bad source");
    let error = match super::load_theory(&[first, second]) {
        Ok(_) => panic!("malformed source must fail"),
        Err(error) => error,
    };
    let report = super::ProveReport::blank(super::DecisionClass::Malformed, error);
    assert_eq!(report.common.inputs.len(), 2);
    assert_eq!(report.common.diagnostics[0].code, "SOURCE_PARSE_FAILED");
    let diagnostic = report.common.diagnostics[0]
        .diagnostic
        .as_ref()
        .expect("evidence");
    assert!(!diagnostic.causes.is_empty());
    assert!(diagnostic.observed.is_some());
}

#[test]
fn exited_parent_does_not_leave_descendant_pipe_unbounded() {
    let (_directory, binary) = script("sleep 30 &\nprintf 'done'; exit 0");
    let started = std::time::Instant::now();
    let receipt = run_bounded_process(&binary, &[], None, Duration::from_millis(100), 32)
        .expect("parent receipt");
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(receipt.stdout.retained, b"done");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn escaped_descendant_cannot_extend_the_parent_deadline() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let pid_file = directory.path().join("escaped.pid");
    let binary = directory.path().join("synthetic-prover");
    std::fs::write(
        &binary,
        format!(
            "#!/bin/sh\nsetsid sh -c 'echo $$ > {}; sleep 30' &\nprintf 'done'\nexit 0\n",
            pid_file.display()
        ),
    )
    .expect("write synthetic prover");
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
        .expect("make synthetic prover executable");
    let started = std::time::Instant::now();
    let receipt = run_bounded_process(&binary, &[], None, Duration::from_millis(100), 32)
        .expect("parent receipt");
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(receipt.stdout.retained, b"done");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "an escaped descendant holding the pipe must not extend the deadline"
    );
    for _ in 0..20 {
        if let Ok(pid) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid.trim().parse::<i32>() {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn process_identity_failure_retains_the_original_diagnostic() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let error = run_bounded_process(directory.path(), &[], None, Duration::from_secs(1), 32)
        .expect_err("a directory cannot be hashed as an executable");
    let projected = super::ReportDiagnostic::from_diag(&error);
    assert_eq!(projected.code, "PROVER_EXECUTABLE_IDENTITY_FAILED");
    let evidence = projected.diagnostic.expect("structured evidence");
    assert_eq!(
        evidence.fields["evidence"]["cause_diagnostic"]["code"],
        "diag.foreign-error"
    );
    assert!(
        evidence
            .causes
            .iter()
            .any(|cause| cause.contains("hash selected prover"))
    );
}
