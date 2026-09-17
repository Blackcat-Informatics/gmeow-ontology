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
    assert!(first.dataset.get().is_some());

    let warm = NativeInput::new(root.path(), &path).unwrap();
    for operation in ["synthetic-first", "synthetic-second"] {
        let value: String = warm
            .observe(&store, operation, |_| panic!("warm action executed"))
            .unwrap();
        assert_eq!(value, "selected observation");
    }
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
    assert!(input.dataset.get().is_none());
}
