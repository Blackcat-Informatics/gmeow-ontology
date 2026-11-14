// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The build pipeline is authored as data in `slices/core/pipeline/module.ttl`, and that
//! file is what the producer executes: the spec is parsed from it, with no second
//! description in Rust to keep in step.

use std::path::{Path, PathBuf};

use gmeow_pipeline::{PipelineSpec, bind, default_registry, full_spec};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The executed pipeline is the authored slice: the spec the producer runs parses from
/// the slice file on disk to the same value, validates as a DAG with one sink, and binds
/// every stage to a registered implementation.
#[test]
fn the_executed_pipeline_is_the_authored_slice() {
    let root = repo_root();
    let ttl = std::fs::read_to_string(root.join("slices/core/pipeline/module.ttl"))
        .expect("read pipeline slice");
    let authored = PipelineSpec::from_turtle(&[&ttl]).expect("parse the authored pipeline");
    let executed = full_spec();
    assert_eq!(
        authored, executed,
        "the producer executes the pipeline slice it was compiled from"
    );

    let graph = executed
        .validate()
        .expect("the authored pipeline validates");
    let bound = bind(&executed, &graph, &default_registry()).expect("the authored pipeline binds");
    assert_eq!(
        bound.len(),
        executed.stages.len(),
        "every authored stage is bound"
    );
}
