// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dogfooding parity gate (#861 P6 integration): the build DAG authored as
//! data in `slices/core/pipeline/module.ttl` must load, validate, bind against
//! the registry, AND be IDENTICAL to the authoritative Rust [`full_spec`] the run
//! executes. This proves the two never diverge — the slice file IS the build
//! graph, and the Rust spec is its faithful executable twin.

use std::path::{Path, PathBuf};

use gmeow_pipeline::{bind, default_registry, full_spec, PipelineSpec};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn slice_dag_binds_and_matches_full_spec() {
    let root = repo_root();
    let ttl = std::fs::read_to_string(root.join("slices/core/pipeline/module.ttl"))
        .expect("read pipeline slice");
    let spec = PipelineSpec::from_turtle(&[&ttl]).expect("parse the dogfooded build DAG");

    // The slice DAG validates (acyclic, complete, one Sink, engine-lock derived)
    // and binds against the default registry (every stageImpl resolves; kind +
    // consumes agree with the Rust impl).
    let graph = spec.validate().expect("slice DAG validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("slice DAG binds to registry");
    assert_eq!(bound.len(), spec.stages.len(), "every slice stage bound");

    // The slice DAG is IDENTICAL to the authoritative Rust full_spec (same stages,
    // impl keys, and consumes sets) — the single authoritative graph the run uses.
    let full = full_spec();
    let mut slice: Vec<(String, String, Vec<String>)> = spec
        .stages
        .iter()
        .map(|s| (s.id.clone(), s.impl_key.clone(), s.consumes.clone()))
        .collect();
    let mut rust: Vec<(String, String, Vec<String>)> = full
        .stages
        .iter()
        .map(|s| (s.id.clone(), s.impl_key.clone(), s.consumes.clone()))
        .collect();
    slice.sort();
    rust.sort();
    assert_eq!(
        slice, rust,
        "the dogfooded slice DAG and the Rust full_spec must be identical"
    );
}
