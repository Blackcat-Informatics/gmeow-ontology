// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Checkout-bound packet preparation composed with the existing governed submission.

use std::path::{Path, PathBuf};

use gmeow_errors::ResultExt;
use gmeow_mcp::{McpServer, optional_str_checked, required_str};
use serde_json::{Value, json};

fn failure(message: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::McpDev { message })
}

#[cfg(test)]
#[path = "candidate_packet.tests.rs"]
mod tests;

pub(super) fn descriptor() -> Value {
    let mut descriptor = gmeow_mcp::candidate_submission_descriptor();
    descriptor["name"] = json!("submit_packet_candidate");
    descriptor["description"] = json!(
        "Assemble a packet from slice_dir in this checkout and submit its native \
         definitional closure as the candidate KB through submit_candidate. axis and \
         batch select the packet. Packet and slice provenance are bound from assembly; \
         KB and provenance overrides are rejected. The existing verdict/policy gate \
         controls the append-only store; dry_run writes nothing."
    );
    let properties = descriptor["inputSchema"]["properties"]
        .as_object_mut()
        .expect("native tool properties");
    for supplied in ["kb", "kb_format", "for_slice", "for_packet"] {
        properties.remove(supplied);
    }
    properties.insert("slice_dir".into(), json!({"type":"string"}));
    properties.insert("axis".into(), json!({"type":"string"}));
    properties.insert(
        "batch".into(),
        json!({"type":"integer", "minimum":0, "maximum":u32::MAX}),
    );
    descriptor["inputSchema"]["required"] = json!(["formula", "standpoint", "slice_dir"]);
    descriptor["inputSchema"]["additionalProperties"] = json!(false);
    descriptor["annotations"] =
        json!({"readOnlyHint":false, "destructiveHint":false, "idempotentHint":false});
    descriptor
}

fn selected_directory(root: &Path, requested: &str) -> gmeow_errors::Result<PathBuf> {
    let root = root
        .canonicalize()
        .ctx("resolve packet candidate checkout")?;
    let selected = root
        .join(requested)
        .canonicalize()
        .ctx("resolve packet candidate slice")?;
    if !selected.starts_with(&root) {
        return Err(failure(
            "packet candidate slice is outside the bound checkout".into(),
        ));
    }
    Ok(selected)
}

fn selectors(args: &Value) -> gmeow_errors::Result<(Option<&str>, Option<u32>)> {
    let properties = descriptor()["inputSchema"]["properties"]
        .as_object()
        .expect("native tool properties")
        .clone();
    let object = args
        .as_object()
        .ok_or_else(|| failure("packet candidate arguments must be an object".into()))?;
    for name in object.keys() {
        if !properties.contains_key(name) {
            return Err(failure(format!(
                "packet candidate argument {name:?} is not accepted"
            )));
        }
    }
    required_str(args, "formula")?;
    required_str(args, "standpoint")?;
    required_str(args, "slice_dir")?;
    let axis = optional_str_checked(args, "axis")?;
    let batch = args
        .get("batch")
        .map(|value| {
            value
                .as_u64()
                .and_then(|batch| u32::try_from(batch).ok())
                .ok_or_else(|| {
                    failure("packet candidate batch must be an unsigned 32-bit integer".into())
                })
        })
        .transpose()?;
    Ok((axis, batch))
}

pub(super) fn submit(
    server: &McpServer,
    args: &Value,
    root: &Path,
) -> gmeow_errors::Result<String> {
    let (axis, batch) = selectors(args)?;
    let directory = selected_directory(root, required_str(args, "slice_dir")?)?;
    let shapes = server.view().validation_shapes()?;
    let tiers = gmeow_slice_brief::exemplar_tiers(&directory, shapes)?;
    let prepared = gmeow_slice_brief::assemble_candidate_packet(&gmeow_slice_brief::BriefInputs {
        slice_dir: &directory,
        axis,
        batch,
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    })?;
    let packet_context = json!({
        "packet": &prepared.packet.packet_iri,
        "slice": &prepared.packet.source_slice,
        "digest": &prepared.packet.digest,
        "axis": &prepared.packet.axis,
        "batch": prepared.packet.batch,
        "kb_format": "application/n-quads",
    });
    let mut submission = args.as_object().expect("validated arguments").clone();
    for selector in ["slice_dir", "axis", "batch"] {
        submission.remove(selector);
    }
    submission.insert("kb".into(), json!(prepared.knowledge_base_nquads()?));
    submission.insert("kb_format".into(), json!("application/n-quads"));
    submission.insert("for_slice".into(), json!(prepared.packet.source_slice));
    submission.insert("for_packet".into(), json!(prepared.packet.packet_iri));
    let output = server.submit_candidate(&Value::Object(submission))?;
    let mut output: Value =
        serde_json::from_str(&output).ctx("read native candidate submission result")?;
    let object = output
        .as_object_mut()
        .ok_or_else(|| failure("native candidate submission must return an object".into()))?;
    if object
        .insert("packet_context".into(), packet_context)
        .is_some()
    {
        return Err(failure(
            "native submission already owns the packet_context field".into(),
        ));
    }
    Ok(output.to_string())
}
