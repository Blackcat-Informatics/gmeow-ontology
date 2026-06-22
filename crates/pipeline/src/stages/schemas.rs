// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `schemas` export leaf (#861 P4): LinkML/JSON-Schema/Pydantic/TS/GraphQL/OpenAPI.
//!
//! # Why this leaf shells out to Python (the lane-only external exception)
//!
//! Five of the six committed `generated/schemas/` artifacts cannot be produced
//! in Rust: `gmeow.schema.json` (LinkML `JsonSchemaGenerator`), `gmeow.py`
//! (`PydanticGenerator`), `gmeow.ts` (`TypescriptGenerator`), and
//! `gmeow.graphql` (`GraphqlGenerator`) are emitted by the **external LinkML
//! toolkit** (`linkml.generators.*`), and `gmeow.openapi.json` is derived from
//! the json-schema text. There is NO Rust LinkML generator suite — this is an
//! irreducible external dependency, sanctioned as an "ext deps lane-only"
//! exception under the project's north-star goals (peer to the pre-acknowledged
//! SPARQL / EDOAL exceptions). The `emit_linkml` OWL→LinkML fold and the
//! OpenAPI derivation are native Python in this repo, but they are entangled
//! with the LinkML generators in one render pipeline.
//!
//! So this leaf does NOT port the generators. It invokes the repo's existing
//! Python (`gmeow_tools.schema_compile.SchemaGenerator.render`) through
//! `uv run` as a lane-only external tool over a private staging tree, then
//! reads the six produced files back and folds their bytes — verbatim,
//! including the committed banner/normalization the Python already applies —
//! into the stage product. Byte-identity with the committed artifacts is
//! guaranteed because it is the SAME toolkit over the SAME fold.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// The committed logical paths of the six schema artifacts, in the fixed order
/// the render pipeline produces them. Keys are repo-relative (`generated/…`),
/// matching `gmeow_tools.config.SCHEMAS_DIR`.
pub const LINKML_PATH: &str = "generated/schemas/gmeow.linkml.yaml";
/// JSON Schema (LinkML `JsonSchemaGenerator`).
pub const JSON_SCHEMA_PATH: &str = "generated/schemas/gmeow.schema.json";
/// Pydantic models (LinkML `PydanticGenerator`).
pub const PYDANTIC_PATH: &str = "generated/schemas/gmeow.py";
/// TypeScript interfaces (LinkML `TypescriptGenerator`).
pub const TYPESCRIPT_PATH: &str = "generated/schemas/gmeow.ts";
/// GraphQL type stubs (LinkML `GraphqlGenerator`).
pub const GRAPHQL_PATH: &str = "generated/schemas/gmeow.graphql";
/// OpenAPI 3.1 derived from the JSON Schema text.
pub const OPENAPI_PATH: &str = "generated/schemas/gmeow.openapi.json";

/// All six committed logical paths (the render output order).
pub const SCHEMA_PATHS: [&str; 6] = [
    LINKML_PATH,
    JSON_SCHEMA_PATH,
    PYDANTIC_PATH,
    TYPESCRIPT_PATH,
    GRAPHQL_PATH,
    OPENAPI_PATH,
];

/// The inline Python driver: render the six schema artifacts into the staging
/// directory handed as `argv[1]`, reusing the repo's registered `schemas`
/// generator (the exact `emit_linkml → _write_yaml → gen_* → gen_openapi →
/// _write_artifacts(_normalize_text)` sequence). The generator writes under
/// `<staging>/generated/schemas/` (its `SCHEMAS_DIR` relative to
/// `PROJECT_ROOT`).
///
/// This replicates the prologue of `gmeow_tools.generator.run`: importing the
/// module registers the `schemas` generator instance (`@register`), then we
/// stamp `_source_hash` from `source_hash(gen.inputs)` — the framework value
/// the committed banner was minted with — before calling `render`. We do NOT
/// call `run` itself: that would publish into / compare against the real tree.
const RENDER_SCRIPT: &str = "\
import sys
from pathlib import Path
import gmeow_tools.schema_compile  # noqa: F401  (registers the `schemas` generator)
from gmeow_tools.generator import registry, source_hash
gen = registry()['schemas']
object.__setattr__(gen, '_source_hash', source_hash(gen.inputs))
gen.render(Path(sys.argv[1]))
";

/// Produce the six schema artifacts by driving the repo's LinkML render
/// pipeline (lane-only external Python) over a private staging tree rooted at
/// `root`, then read the produced files back into a logical-path → bytes map.
///
/// The staging directory is created under the system temp dir and removed on
/// the way out (success or failure). The subprocess runs with cwd = `root` and
/// `uv run --project <root>` so it resolves the repo's pinned Python + LinkML.
pub(crate) fn render_schemas(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let staging = staging_dir(root)?;
    let result = render_into(root, &staging);
    // Best-effort cleanup; a leaked temp dir must never mask the real result.
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// A unique, freshly-created staging directory under the system temp dir.
fn staging_dir(root: &Path) -> Result<std::path::PathBuf, PipelineError> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let unique = format!("gmeow-schemas-{}-{stamp}", std::process::id());
    // Tie the name to the root so concurrent worktrees never collide.
    let root_tag = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("{unique}-{root_tag}"));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Run the render into `staging` and read the six files back.
fn render_into(root: &Path, staging: &Path) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let output = Command::new("uv")
        .arg("run")
        .arg("--project")
        .arg(root)
        .arg("python")
        .arg("-c")
        .arg(RENDER_SCRIPT)
        .arg(staging)
        .current_dir(root)
        .output()
        .map_err(|e| PipelineError::Stage {
            stage: "stage-export-schemas".into(),
            message: format!("failed to spawn `uv run` for the LinkML render: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PipelineError::Stage {
            stage: "stage-export-schemas".into(),
            message: format!(
                "LinkML schema render failed (status {}): {}",
                output.status,
                stderr.trim()
            ),
        });
    }

    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for logical in SCHEMA_PATHS {
        let produced = staging.join(logical);
        let bytes = std::fs::read(&produced).map_err(|e| PipelineError::Stage {
            stage: "stage-export-schemas".into(),
            message: format!(
                "render did not produce {logical} at {}: {e}",
                produced.display()
            ),
        })?;
        artifacts.insert(logical.to_string(), bytes);
    }
    Ok(artifacts)
}

/// The `stage-export-schemas` export-leaf stage.
pub struct SchemasStage;

impl Stage for SchemasStage {
    fn id(&self) -> &str {
        "stage-export-schemas"
    }
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "schemas.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let artifacts = render_schemas(input.root)?;
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Probe whether the lane-only LinkML toolkit is available in the repo's
    /// `uv` environment. Mirrors the okf / network-gated capability-probe skip
    /// pattern: a missing toolkit is a clean SKIP, never a test failure.
    fn linkml_available(root: &Path) -> bool {
        match Command::new("uv")
            .arg("run")
            .arg("--project")
            .arg(root)
            .arg("python")
            .arg("-c")
            .arg("import linkml")
            .current_dir(root)
            .output()
        {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    #[test]
    fn schemas_byte_identical_to_committed() {
        let root = repo_root();
        if !linkml_available(&root) {
            eprintln!(
                "SKIP schemas_byte_identical_to_committed: the lane-only LinkML toolkit \
                 (`uv run python -c \"import linkml\"`) is unavailable in this environment"
            );
            return;
        }

        let artifacts = render_schemas(&root).expect("render schemas via the LinkML lane");
        assert_eq!(artifacts.len(), 6, "expected six schema artifacts");

        for logical in SCHEMA_PATHS {
            let produced = artifacts
                .get(logical)
                .unwrap_or_else(|| panic!("missing produced artifact {logical}"));
            let committed = std::fs::read(root.join(logical))
                .unwrap_or_else(|e| panic!("read committed {logical}: {e}"));
            assert_eq!(
                produced.as_slice(),
                committed.as_slice(),
                "{logical} is not byte-identical to the committed generated/ artifact"
            );
        }
    }
}
