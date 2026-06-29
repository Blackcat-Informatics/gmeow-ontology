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

    // The slice DAG validates (acyclic, complete, one Sink) and binds against the
    // default registry (every stageImpl resolves; kind + consumes + resources
    // agree with the Rust impl).
    let graph = spec.validate().expect("slice DAG validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("slice DAG binds to registry");
    assert_eq!(bound.len(), spec.stages.len(), "every slice stage bound");

    // The slice DAG is IDENTICAL to the authoritative Rust full_spec — the single
    // authoritative graph the run uses. We compare every field the loader fills
    // authoritatively from `module.ttl`: id, impl_key, consumes, kind, AND
    // resources. (`kind` is `gmeow:stageKind`; `resources` is
    // `gmeow:requiresResource` — both are loaded from the slice, so a drift in
    // either must surface here, not just in id/impl_key/consumes.)
    //
    // `formats` (`gmeow:producesFormat`) is loaded from the slice too, but is
    // DELIBERATELY excluded: `full_spec()` leaves `formats` empty (run.rs builds
    // stages with `formats: Vec::new()`), so the Rust spec is not authoritative
    // for it. The slice is the sole formats source; comparing it would compare a
    // populated slice value against an intentionally-empty Rust one. Format
    // coverage is asserted independently against the slice in the loader tests.
    type StageTuple = (
        String,
        String,
        Vec<String>,
        &'static str,
        Vec<String>,
        Vec<(String, Vec<String>)>,
    );
    let project = |s: &gmeow_pipeline::StageSpec| -> StageTuple {
        (
            s.id.clone(),
            s.impl_key.clone(),
            s.consumes.clone(),
            s.kind.tag(),
            s.resources.clone(),
            s.dataflow_entities.clone(),
        )
    };
    let full = full_spec();
    let mut slice: Vec<StageTuple> = spec.stages.iter().map(project).collect();
    let mut rust: Vec<StageTuple> = full.stages.iter().map(project).collect();
    slice.sort();
    rust.sort();
    assert_eq!(
        slice, rust,
        "the dogfooded slice DAG and the Rust full_spec must be identical \
         (id, impl_key, consumes, kind, resources, dataflow_entities)"
    );
}
