// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn every_dependency_names_a_real_task() {
    let names = CHECK_DAG
        .iter()
        .map(|task| task.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names.len(),
        CHECK_DAG.len(),
        "CHECK_DAG repeats a task name"
    );
    for task in CHECK_DAG {
        for dependency in task.dependencies {
            assert!(
                names.contains(dependency),
                "{} depends on unknown task {dependency}",
                task.name
            );
        }
    }
}

#[test]
fn the_plan_is_acyclic_and_covers_every_task() {
    let waves = plan_waves();
    let scheduled = waves.iter().flatten().copied().collect::<BTreeSet<_>>();
    assert_eq!(scheduled.len(), CHECK_DAG.len());
}

/// `sync` is the gate's longest single stage, so it must NOT be a blanket
/// prerequisite. Every task that reads only authored sources starts in wave 0,
/// concurrently with `sync` itself.
#[test]
fn sync_is_not_a_blanket_prerequisite() {
    let wave_zero = plan_waves()
        .first()
        .expect("the plan has at least one wave")
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for name in ["sync", "check-lint", "crate-check", "i18n-lint"] {
        assert!(
            wave_zero.contains(name),
            "{name} reads no generated/ artifact and must start immediately"
        );
    }
}

/// The monolithic `rust-gate` node is split. Non-corpus lanes remain siblings
/// under `rust-build`; the corpus-consuming runner waits on the explicit producer.
#[test]
fn the_rust_lanes_are_independent_siblings() {
    for name in ["clippy", "doctests"] {
        assert_eq!(
            task(name).dependencies,
            AFTER_RUST_BUILD,
            "{name} must depend on rust-build and nothing else"
        );
    }
    assert_eq!(task("test-fixtures").dependencies, AFTER_SYNC);
    assert_eq!(task("nextest").dependencies, AFTER_TEST_FIXTURES);
    assert!(
        !CHECK_DAG
            .iter()
            .any(|task| matches!(task.name, "carrier-purity" | "coherence-gate-teeth")),
        "carrier/coherence proofs must run inside the one nextest inventory"
    );
    assert!(
        !CHECK_DAG.iter().any(|task| task.name == "rust-gate"),
        "the monolithic rust-gate node must not be scheduled alongside its parts"
    );
}

/// The breadth-dominated lanes belong to `make heavy`, not the per-commit gate.
#[test]
fn the_heavy_lanes_are_not_scheduled_by_check() {
    for name in ["acceptance", "wasm-parity", "console-smoke", "bench-soak"] {
        assert!(
            !CHECK_DAG.iter().any(|task| task.name == name),
            "{name} moved to `make heavy` and must not reappear in CHECK_DAG"
        );
    }
}

#[test]
fn the_final_task_waits_for_every_other_task() {
    let expected = CHECK_DAG
        .iter()
        .map(|task| task.name)
        .filter(|name| *name != "compliance-report" && *name != "sync")
        .collect::<BTreeSet<_>>();
    let declared = FINAL_DEPS.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        declared, expected,
        "compliance-report must wait for every other gate task (sync is transitive)"
    );
}

#[test]
fn the_host_lock_lives_on_durable_shared_storage() {
    let path = host_lock_path();
    assert!(
        path.starts_with("/var/tmp"),
        "the host gate lock must live on durable, host-shared storage: {}",
        path.display()
    );
    assert!(
        !path.starts_with("/tmp/"),
        "the host gate lock must not live on tmpfs"
    );
}

#[test]
fn owner_records_round_trip_their_pid() {
    assert_eq!(record_pid("pid=1234 purpose=check root=/x\n"), Some(1234));
    assert_eq!(record_pid("purpose=check root=/x"), None);
    assert_eq!(record_pid(""), None);
    assert!(pid_alive(std::process::id()));
    // pid 0 is never a userspace process on Linux.
    assert!(!pid_alive(0));
}

/// Every task's `target` is a real Makefile RULE. The scheduler spawns
/// `make <target>`; an undeclared target fails the child, which at least reports —
/// but a target that exists only as a `.PHONY` entry with no rule would "succeed"
/// with `make: Nothing to be done`, so the RULE line is what is asserted here.
#[test]
fn every_task_target_has_a_makefile_rule() {
    let makefile = std::fs::read_to_string(workspace_root().join("Makefile"))
        .expect("the workspace Makefile is readable");
    let rules: BTreeSet<&str> = makefile
        .lines()
        .filter(|line| !line.starts_with(['\t', ' ', '#']))
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, rest)| {
            !name.is_empty()
                && !rest.starts_with('=')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
        .map(|(name, _)| name)
        .collect();
    let missing = CHECK_DAG
        .iter()
        .map(|task| task.target)
        .filter(|target| !rules.contains(target))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "these CHECK_DAG targets have no Makefile rule: {missing:?}"
    );
}
