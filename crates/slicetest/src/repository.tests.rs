// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::sync::{Arc, Barrier};

use tempfile::tempdir;

use super::{snapshot_worker_executable_with, worker_snapshot_temporary_path};

/// Preserve selected spec identity across the child cwd change and reject paths outside the root.
#[test]
fn worker_paths_survive_a_relative_checkout_and_changed_working_directory() {
    use std::ffi::OsStr;
    use std::path::Path;

    let directory = tempfile::tempdir_in(".").expect("relative scratch directory");
    let root = Path::new(".")
        .join(
            directory
                .path()
                .file_name()
                .expect("scratch directory name"),
        )
        .join("checkout");
    assert!(root.is_relative());
    std::fs::create_dir(&root).expect("synthetic checkout directory");
    let path = root.join("selected.spec");
    std::fs::write(&path, b"path witness only").expect("synthetic path witness");
    let tasks = [super::SpecTask {
        kind: super::SliceSpecKind::Structural,
        path: path.clone(),
    }];
    let command = super::worker_command(
        &std::env::current_exe().expect("test executable path"),
        &root,
        "selected producer",
        super::SliceSpecKind::Structural,
        &tasks,
        None,
    )
    .expect("prepare command without executing a corpus worker");
    let canonical = root.canonicalize().expect("absolute checkout");
    assert_eq!(command.get_current_dir(), Some(canonical.as_path()));
    assert_eq!(
        command
            .get_envs()
            .find(|(name, _)| *name == OsStr::new("GMEOW_ROOT"))
            .and_then(|(_, value)| value),
        Some(canonical.as_os_str()),
    );
    let arguments = command.get_args().collect::<Vec<_>>();
    let selection = arguments
        .windows(2)
        .find(|pair| pair[0] == OsStr::new("--spec"))
        .expect("exact spec selection")[1];
    assert_eq!(selection, OsStr::new("selected.spec"));
    assert_eq!(
        canonical
            .join(selection)
            .canonicalize()
            .expect("child path"),
        path.canonicalize().expect("parent path"),
        "changing cwd must preserve the selected file's identity",
    );

    let outside = [super::SpecTask {
        kind: super::SliceSpecKind::Structural,
        path: directory.path().join("outside.spec"),
    }];
    assert!(
        super::worker_command(
            Path::new(command.get_program()),
            &root,
            "selected producer",
            super::SliceSpecKind::Structural,
            &outside,
            None,
        )
        .is_err(),
        "a worker selection cannot escape the checkout",
    );
}

/// Publish worker evidence only when both the parent and copied executable match the receipt.
#[test]
fn worker_receipt_follows_only_the_exact_executable_bytes() {
    use gmeow_action_cache::executable::{ExecutableReceipt, ExecutableRecipe, sha256_file};

    let directory = tempdir().expect("scratch");
    let source = directory.path().join("producer");
    let worker = directory.path().join("worker");
    std::fs::write(&source, b"exact executable").expect("source");
    std::fs::write(&worker, b"exact executable").expect("worker");
    assert!(super::pin_worker_receipt(&source, &worker).is_err());
    use gmeow_build_inputs::{
        CfgContext, InputInventory, ProductionSelection, SCHEMA, UnitSelection,
    };
    std::fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='1.0.0'\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("Cargo.lock"),
        "version=4\npackage=[]\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("lib.rs"), "pub fn fixture() {}\n").unwrap();
    let source_inventory = InputInventory::collect(
        directory.path(),
        &ProductionSelection {
            schema: SCHEMA,
            units: vec![UnitSelection {
                package: "path+file://<workspace>#fixture@1".into(),
                manifest: Some("Cargo.toml".into()),
                source: Some("lib.rs".into()),
                target: "fixture".into(),
                kinds: vec!["lib".into()],
                cfg: CfgContext::from_rustc("unix", &[], true).unwrap(),
                dependencies: vec![],
                dependency_names: vec![],
                controller: false,
            }],
            roots: vec![0],
            policy_files: vec![],
        },
    )
    .unwrap();
    let receipt = ExecutableReceipt {
        schema: 2,
        resolution: gmeow_build_inputs::CargoResolutionInputs::capture(
            directory.path(),
            ["Cargo.toml".into()].into_iter().collect(),
        )
        .unwrap()
        .bind(&source_inventory.selection)
        .unwrap(),
        recipe: ExecutableRecipe {
            schema: 2,
            profile: "pipeline".into(),
            source_digest: source_inventory.digest().unwrap(),
            source_inventory,
            rustc: "compiler".into(),
            cargo: "cargo".into(),
            compiler_environment: Default::default(),
            units: Vec::new(),
            roots: Vec::new(),
        },
        executable_sha256: sha256_file(&source).expect("digest"),
    };
    receipt
        .write(&source.with_extension("receipt.json"))
        .expect("source receipt");
    super::pin_worker_receipt(&source, &worker).expect("carry exact binding");
    let published =
        ExecutableReceipt::read(&worker.with_extension("receipt.json")).expect("worker receipt");
    assert_eq!(published, receipt);
    published
        .verify(&worker, &receipt.recipe.digest().expect("recipe"))
        .expect("worker authenticates independently");
    std::fs::write(&worker, b"substituted worker").expect("replace");
    assert!(super::pin_worker_receipt(&source, &worker).is_err());
    std::fs::write(&source, b"substituted parent").expect("replace parent");
    assert!(super::pin_worker_receipt(&source, &worker).is_err());
}

#[test]
fn worker_snapshot_copies_exact_bytes_when_hard_links_cross_devices() {
    let source_dir = tempdir().expect("source tempdir");
    let cache_dir = tempdir().expect("cache tempdir");
    let source = source_dir.path().join("gmeow-dev");
    let snapshot = cache_dir.path().join(".gmeow-dev.tmp");
    let bytes = b"exact worker executable bytes\0\xff";
    std::fs::write(&source, bytes).expect("write worker fixture");

    snapshot_worker_executable_with(&source, &snapshot, |_, _| {
        Err(std::io::Error::from(ErrorKind::CrossesDevices))
    })
    .expect("cross-device worker snapshot falls back to an exact copy");

    assert_eq!(
        std::fs::read(&snapshot).expect("read worker snapshot"),
        bytes
    );
    assert_eq!(
        std::fs::metadata(&snapshot)
            .expect("snapshot metadata")
            .permissions(),
        std::fs::metadata(&source)
            .expect("source metadata")
            .permissions()
    );
}

#[test]
fn worker_snapshot_fails_closed_for_other_link_errors() {
    let directory = tempdir().expect("tempdir");
    let source = directory.path().join("gmeow-dev");
    let snapshot = directory.path().join(".gmeow-dev.tmp");
    std::fs::write(&source, b"worker").expect("write worker fixture");

    let error = snapshot_worker_executable_with(&source, &snapshot, |_, _| {
        Err(std::io::Error::from(ErrorKind::PermissionDenied))
    })
    .expect_err("non-EXDEV link failure must not copy");

    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
    assert!(!snapshot.exists(), "failed snapshot must leave no bytes");
}

#[test]
fn worker_snapshot_does_not_remove_an_existing_cross_device_destination() {
    let directory = tempdir().expect("tempdir");
    let source = directory.path().join("gmeow-dev");
    let snapshot = directory.path().join(".gmeow-dev.tmp");
    std::fs::write(&source, b"current worker").expect("write worker fixture");
    std::fs::write(&snapshot, b"another invocation").expect("write owned snapshot");

    let error = snapshot_worker_executable_with(&source, &snapshot, |_, _| {
        Err(std::io::Error::from(ErrorKind::CrossesDevices))
    })
    .expect_err("an existing destination must fail closed");

    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    assert_eq!(
        std::fs::read(&snapshot).expect("read existing snapshot"),
        b"another invocation",
        "a losing invocation must not remove or replace another call's snapshot"
    );
}

#[test]
fn concurrent_cross_device_snapshots_use_unique_owned_paths() {
    const WORKERS: usize = 16;

    let directory = tempdir().expect("tempdir");
    let source = directory.path().join("gmeow-dev");
    let bytes = b"concurrent worker bytes\0\xff";
    std::fs::write(&source, bytes).expect("write worker fixture");
    let barrier = Arc::new(Barrier::new(WORKERS));

    let handles = (0..WORKERS)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let directory = directory.path().to_path_buf();
            let source = source.clone();
            std::thread::Builder::new()
                .name("same-worker-name".to_owned())
                .spawn(move || {
                    barrier.wait();
                    let snapshot = worker_snapshot_temporary_path(&directory);
                    snapshot_worker_executable_with(&source, &snapshot, |_, _| {
                        Err(std::io::Error::from(ErrorKind::CrossesDevices))
                    })
                    .expect("concurrent cross-device snapshot");
                    snapshot
                })
                .expect("spawn snapshot worker")
        })
        .collect::<Vec<_>>();
    let snapshots = handles
        .into_iter()
        .map(|handle| handle.join().expect("snapshot worker joins"))
        .collect::<Vec<_>>();

    assert_eq!(
        snapshots.iter().collect::<BTreeSet<_>>().len(),
        WORKERS,
        "every concurrent invocation must own a unique temporary path"
    );
    for snapshot in snapshots {
        assert_eq!(
            std::fs::read(snapshot).expect("read concurrent snapshot"),
            bytes
        );
    }
}
