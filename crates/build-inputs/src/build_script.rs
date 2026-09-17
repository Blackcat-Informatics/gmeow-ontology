// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{
    InputInventory, NON_PRODUCER, ProductionSelection, Result, SELECTION_DIGEST_ENV, SELECTION_ENV,
    digest, fail,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Emit one action owner's implementation fingerprint. Only the controller can
/// supply an exact production selection; ordinary test/debug Cargo builds emit
/// an explicitly separate identity and never resolve or construct corpus inputs.
pub fn emit_action_identity(fingerprint_env: &str, pipeline_metadata: bool) -> Result<()> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(fail)?);
    let workspace = manifest.join("../..").canonicalize().map_err(fail)?;
    let manifest = manifest.join("Cargo.toml");
    let manifest = crate::relative(&workspace, &manifest)?;
    let producer = std::env::var("GMEOW_PRODUCER_BUILD_CONTRACT").unwrap_or_default();
    let compilation = std::env::var("GMEOW_PRODUCER_COMPILATION_CONTRACT").unwrap_or_default();
    for variable in [
        SELECTION_ENV,
        SELECTION_DIGEST_ENV,
        "GMEOW_PRODUCER_BUILD_CONTRACT",
        "GMEOW_PRODUCER_COMPILATION_CONTRACT",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let selection = std::env::var(SELECTION_ENV).ok();
    let mut context: BTreeMap<String, String> = std::env::vars()
        .filter(|(name, _)| {
            name.starts_with("CARGO_CFG_")
                || name.starts_with("CARGO_FEATURE_")
                || [
                    "HOST",
                    "TARGET",
                    "PROFILE",
                    "OPT_LEVEL",
                    "DEBUG",
                    "CARGO_ENCODED_RUSTFLAGS",
                    "RUSTFLAGS",
                ]
                .contains(&name.as_str())
        })
        .collect();
    let rustc = std::env::var("RUSTC").map_err(fail)?;
    let output = std::process::Command::new(rustc)
        .arg("-Vv")
        .output()
        .map_err(fail)?;
    if !output.status.success() {
        return Err(fail("rustc compiler identity lookup failed"));
    }
    let rustc = String::from_utf8(output.stdout).map_err(fail)?;
    context.insert("rustc".into(), rustc.clone());
    let source = match selection {
        Some(selection) => {
            if !hex_digest(&producer)
                || !hex_digest(&compilation)
                || context.get("OPT_LEVEL").map(String::as_str) != Some("3")
            {
                return Err(fail(
                    "production source selection has no optimized executable/compilation admission",
                ));
            }
            let expected = std::env::var(SELECTION_DIGEST_ENV).map_err(fail)?;
            if !hex_digest(&expected) {
                return Err(fail("source selection document has no exact commitment"));
            }
            let path = PathBuf::from(selection);
            let metadata = std::fs::symlink_metadata(&path).map_err(fail)?;
            if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
                return Err(fail("invalid or oversized source selection document"));
            }
            let bytes = std::fs::read(&path).map_err(fail)?;
            if digest(&bytes) != expected {
                return Err(fail("source selection document changed after admission"));
            }
            let selection: ProductionSelection = serde_json::from_slice(&bytes).map_err(fail)?;
            let selection = selection.scoped_to_manifest(&manifest)?;
            let inventory = InputInventory::collect(&workspace, &selection)?;
            inventory.emit_cargo_rerun_directives(&workspace);
            context.insert("compilation".into(), compilation.clone());
            inventory.digest()?
        }
        None if producer.is_empty() && compilation.is_empty() => {
            context.insert("operation".into(), NON_PRODUCER.into());
            digest(manifest.as_bytes())
        }
        None => {
            return Err(fail(
                "admitted producer build is missing its exact source selection",
            ));
        }
    };
    context.insert("source".into(), source);
    let fingerprint = digest(&serde_json::to_vec(&context).map_err(fail)?);
    println!("cargo:rustc-env={fingerprint_env}={fingerprint}");
    if pipeline_metadata {
        println!("cargo:rustc-env=GMEOW_PRODUCER_BUILD_CONTRACT={producer}");
        println!("cargo:rustc-env=GMEOW_PRODUCER_COMPILATION_CONTRACT={compilation}");
        println!(
            "cargo:rustc-env=GMEOW_TOOLCHAIN_FINGERPRINT={}",
            digest(rustc.as_bytes())
        );
        println!(
            "cargo:rustc-env=GMEOW_BUILD_TARGET={}",
            context
                .get("TARGET")
                .ok_or_else(|| fail("missing Cargo target"))?
        );
        println!(
            "cargo:rustc-env=GMEOW_BUILD_PROFILE={}",
            if producer.is_empty() {
                NON_PRODUCER
            } else {
                "pipeline"
            }
        );
        let features: Vec<_> = context
            .iter()
            .filter_map(|(name, value)| {
                name.strip_prefix("CARGO_FEATURE_")
                    .filter(|_| value.as_str() == "1")
            })
            .collect();
        println!(
            "cargo:rustc-env=GMEOW_BUILD_FEATURES={}",
            features.join(",")
        );
    }
    Ok(())
}
fn hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
