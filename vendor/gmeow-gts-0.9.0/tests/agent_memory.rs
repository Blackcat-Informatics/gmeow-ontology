// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end coverage for the dependency-light Rust agent-memory example.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use gmeow_gts::examples::agent_memory::{
    Memory, MemoryError, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions,
};
use gmeow_gts::nquads::to_nquads;
use gmeow_gts::reader::read;

fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "gmeow-gts-{name}-{}-{nanos}.gts",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn gts_verify(path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gts"))
        .arg("verify")
        .arg(path)
        .output()
        .expect("gts binary runs")
}

#[test]
fn rust_agent_memory_flow_is_append_only_and_verifyable() {
    let path = temp_path("agent-memory-flow");
    let mem = Memory::new(&path);

    let old = mem
        .store(
            "Synthetic rover records battery telemetry in UTC",
            StoreOptions {
                source: Some("synthetic bench run 001"),
                confidence: Some(0.8),
                according_to: Some("example-agent"),
            },
        )
        .unwrap();
    let first_hits = mem
        .recall(RecallOptions {
            query: "battery telemetry",
            min_confidence: Some(0.5),
            ..RecallOptions::default()
        })
        .unwrap();
    assert_eq!(first_hits[0].text, old.text);
    assert_eq!(
        first_hits[0].source.as_deref(),
        Some("synthetic bench run 001")
    );

    let new = mem
        .store(
            "Synthetic rover records battery and thermal telemetry in UTC",
            StoreOptions {
                source: Some("synthetic bench run 002"),
                confidence: Some(0.9),
                according_to: Some("example-agent"),
            },
        )
        .unwrap();
    let tool = mem
        .record_tool_call(
            "urn:gmeow:tool:synthetic-search",
            ToolCallOptions {
                arguments: Some("{\"query\":\"battery telemetry\"}"),
                result: Some("matched one synthetic claim"),
                invocation: Some("urn:gmeow:invocation:test"),
                generated: &[new.id.as_str()],
            },
        )
        .unwrap();
    assert_eq!(tool.generated, vec![new.id.clone()]);

    mem.revise(
        &old.id,
        RevisionOptions {
            reason: Some("synthetic correction"),
            superseded_by: Some(&new.id),
        },
    )
    .unwrap();

    let current = mem
        .recall(RecallOptions {
            query: "battery telemetry",
            ..RecallOptions::default()
        })
        .unwrap();
    assert_eq!(current[0].id, new.id);
    assert!(current.iter().all(|claim| claim.id != old.id));
    assert!(current.iter().any(|claim| claim.id == new.id));

    let history = mem
        .recall(RecallOptions {
            query: "battery telemetry",
            include_suppressed: true,
            ..RecallOptions::default()
        })
        .unwrap();
    assert!(history
        .iter()
        .any(|claim| claim.id == old.id && claim.suppressed));

    let calls = mem.tool_calls().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool, "urn:gmeow:tool:synthetic-search");
    assert_eq!(
        calls[0].invocation.as_deref(),
        Some("urn:gmeow:invocation:test")
    );

    assert_eq!(mem.verify().unwrap(), Vec::<String>::new());
    let bytes = std::fs::read(&path).unwrap();
    let graph = read(&bytes, true, None);
    assert!(graph.diagnostics.is_empty());
    assert_eq!(graph.segment_heads.len(), 4);
    assert!(to_nquads(&graph).contains("wasDerivedFrom"));

    let out = gts_verify(&path);
    assert!(
        out.status.success(),
        "gts verify accepts the produced package"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn rust_agent_memory_validates_inputs_and_empty_packages() {
    let path = temp_path("agent-memory-validation");
    let mem = Memory::new(&path);

    assert_eq!(mem.recall(RecallOptions::default()).unwrap(), Vec::new());
    assert!(matches!(
        mem.store("   ", StoreOptions::default()),
        Err(MemoryError::EmptyClaim)
    ));
    assert!(matches!(
        mem.store(
            "Synthetic claim",
            StoreOptions {
                confidence: Some(f64::NAN),
                ..StoreOptions::default()
            }
        ),
        Err(MemoryError::InvalidConfidence)
    ));
    assert!(matches!(
        mem.record_tool_call("   ", ToolCallOptions::default()),
        Err(MemoryError::EmptyTool)
    ));
}
