// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `stage-conformance` Transform stage: the external-corpus divergence fold.
//!
//! The committed external conformance corpus
//! (`conformance/logic/cases/external/<corpus>/<case>/`) freezes, per case, both
//! the native verdict (`expected/verdicts.json`, the per-world status the native DL
//! consistency path decided at vendor time) and the published external verdict
//! (`source/manifest.ttl`, the W3C `otest:`/`mf:` declared outcome). This stage
//! grades the frozen native verdict against the frozen published verdict for every
//! case that carries a published outcome and projects each divergence into a
//! `gmeow:Finding` N-Quads graph in [`CONFORMANCE_GRAPH`].
//!
//! The grading reuses the same divergence machinery the `ingest-external`
//! `--grade-suite` lane drives ([`gmeow_conformance::divergence::emit_divergence_nq`]
//! → [`gmeow_logic::reason::compare_external_corpus`]): agreements are dropped, a
//! native `incomplete` becomes a `DlGap` row, and a decided native verdict that
//! differs from the published expected becomes a `CorpusOnly` row. The emitter sorts
//! and content-addresses every finding, so the product is byte-deterministic and
//! GTS-fold-stable.
//!
//! Grading off the FROZEN committed verdicts (rather than re-running the reasoner in
//! this stage) is deterministic by construction and never couples the snapshot fold
//! to engine availability: the native token in `expected/verdicts.json` IS the
//! verdict `gmeow_logic::reason::dl_consistency` produced for that case (it is the
//! golden the conformance harness asserts). [`crate::stages::snapshot`] folds this
//! stage's product into the `graph/conformance` named graph of `gmeow.gts`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_conformance::divergence::emit_divergence_nq;
use gmeow_conformance::external::parse_test_manifest;
use gmeow_logic::reason::ExternalComparison;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// The in-memory logical path of the external-corpus divergence N-Quads product
/// [`crate::stages::snapshot`] folds into the `graph/conformance` named graph. The
/// `pipeline/` prefix marks it as in-memory dataflow that is never written to disk
/// (the same convention the diagnostics / composed dataflow products follow).
pub const CONFORMANCE_NQ_PATH: &str = "pipeline/conformance-divergence.nq";

/// The committed external-corpus root: one `<corpus>/<case>/` subtree per vendored
/// suite (`w3c-owl2-el`, `w3c-mini`, `szs-mini`, …).
const EXTERNAL_ROOT: &str = "conformance/logic/cases/external";

/// One graded external case: its corpus, case id, world IRI, and the two verdict
/// tokens (native = frozen `expected/verdicts.json` status; published = the
/// `source/manifest.ttl` `otest:`/`mf:` declared outcome).
struct GradedCase {
    corpus: String,
    comparison: ExternalComparison,
}

/// Grade every committed external corpus case that carries a published verdict and
/// emit the divergences as one `graph/conformance` N-Quads document.
///
/// Cases are discovered generically under [`EXTERNAL_ROOT`]: each `<corpus>/<case>/`
/// directory carrying both an `expected/verdicts.json` and a `source/manifest.ttl`
/// is graded. A case dir that carries a `source/manifest.ttl` this stage cannot
/// parse — or an `expected/verdicts.json` it cannot read — is a HARD failure (no
/// silent skip): the corpus is a committed, drift-gated surface.
///
/// Returns the per-corpus divergence N-Quads, concatenated in corpus order (each
/// corpus's emitter output is itself sorted + content-addressed). An all-agree
/// corpus contributes nothing; an empty result means the whole committed corpus
/// agrees with every published expectation.
pub fn build_conformance_divergence(root: &Path) -> Result<Vec<u8>, PipelineError> {
    let external = root.join(EXTERNAL_ROOT);
    let mut by_corpus: BTreeMap<String, Vec<ExternalComparison>> = BTreeMap::new();

    for graded in grade_external_cases(&external)? {
        by_corpus
            .entry(graded.corpus)
            .or_default()
            .push(graded.comparison);
    }

    let mut out = String::new();
    for (corpus, mut comparisons) in by_corpus {
        // Deterministic per-corpus order (the case id is unique within a corpus).
        comparisons.sort_by(|a, b| a.case.cmp(&b.case).then(a.world.cmp(&b.world)));
        let nq = emit_divergence_nq(&corpus, &comparisons);
        if !nq.is_empty() {
            out.push_str(&nq);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    Ok(out.into_bytes())
}

/// Discover and grade every committed external case under `external`, sorted by
/// `(corpus, case)`. A case dir lacking a `source/manifest.ttl` carries no published
/// external verdict to grade against and is skipped (it is not a corpus divergence
/// source); a manifest or verdicts file that is present but unparsable HARD-fails.
fn grade_external_cases(external: &Path) -> Result<Vec<GradedCase>, PipelineError> {
    let mut graded: Vec<GradedCase> = Vec::new();
    if !external.is_dir() {
        return Err(stage_err(&format!(
            "external corpus root {} is missing",
            external.display()
        )));
    }
    for corpus_dir in sorted_dirs(external)? {
        let corpus = dir_name(&corpus_dir)?;
        for case_dir in sorted_dirs(&corpus_dir)? {
            let case = dir_name(&case_dir)?;
            let manifest_path = case_dir.join("source").join("manifest.ttl");
            if !manifest_path.is_file() {
                // No published external verdict to grade against (e.g. a corpus
                // README-only or fixture dir) — nothing to compare, not a defect.
                continue;
            }
            let published = published_verdict(&manifest_path)?;
            let (world, native) = native_verdict(&case_dir, &case)?;
            graded.push(GradedCase {
                corpus: corpus.clone(),
                comparison: ExternalComparison {
                    case,
                    world,
                    native,
                    published,
                },
            });
        }
    }
    Ok(graded)
}

/// The published external verdict: parse the case's `source/manifest.ttl` (the
/// committed W3C `otest:`/`mf:` declaration) and return its lowercase verdict token.
///
/// The manifest carries exactly one recognized test entry per committed case; a
/// parse failure, a zero-entry manifest, or a multi-entry manifest all HARD-fail
/// (the committed corpus is a single-test-per-case surface).
fn published_verdict(manifest_path: &Path) -> Result<String, PipelineError> {
    let text = std::fs::read_to_string(manifest_path)
        .map_err(|e| stage_err(&format!("read {}: {e}", manifest_path.display())))?;
    let abs = std::path::absolute(manifest_path)
        .map_err(|e| stage_err(&format!("resolve {}: {e}", manifest_path.display())))?;
    let base = format!("file://{}", abs.display());
    let entries = parse_test_manifest(&text, Some(&base))
        .map_err(|e| stage_err(&format!("parse manifest {}: {e}", manifest_path.display())))?;
    let [entry] = entries.as_slice() else {
        return Err(stage_err(&format!(
            "manifest {} carries {} recognized test entries; a committed external case is a single-test surface",
            manifest_path.display(),
            entries.len()
        )));
    };
    Ok(entry.outcome().verdict_status().as_str().to_string())
}

/// The native verdict: read the case's frozen `expected/verdicts.json` (the
/// per-world status the native DL consistency path decided at vendor time) and
/// return its single `(world, status)` pair.
///
/// `expected/verdicts.json` is a `{ "<world-iri>": { "status": "<token>", … } }`
/// object. A committed external case scopes its premises to exactly one world, so a
/// missing file, a parse failure, a zero-world object, a multi-world object, or a
/// missing/non-string `status` all HARD-fail (no silent skip).
fn native_verdict(case_dir: &Path, case: &str) -> Result<(String, String), PipelineError> {
    let path = case_dir.join("expected").join("verdicts.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| stage_err(&format!("read {}: {e}", path.display())))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| stage_err(&format!("parse {}: {e}", path.display())))?;
    let obj = value.as_object().ok_or_else(|| {
        stage_err(&format!(
            "{} is not a JSON object of world→verdict",
            path.display()
        ))
    })?;
    let [(world, world_verdict)] = obj.iter().collect::<Vec<_>>()[..] else {
        return Err(stage_err(&format!(
            "{} carries {} worlds; case {case} must scope to exactly one world",
            path.display(),
            obj.len()
        )));
    };
    let status = world_verdict
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            stage_err(&format!(
                "{} world {world} has no string \"status\"",
                path.display()
            ))
        })?;
    Ok((world.clone(), status.to_string()))
}

/// The immediate subdirectories of `dir`, sorted by path.
fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)
        .map_err(|e| stage_err(&format!("read_dir {}: {e}", dir.display())))?
    {
        let path = entry
            .map_err(|e| stage_err(&format!("read_dir entry under {}: {e}", dir.display())))?
            .path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// The final path component as an owned `String` (HARD-fails on a non-UTF-8 name).
fn dir_name(dir: &Path) -> Result<String, PipelineError> {
    dir.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| stage_err(&format!("directory {} has no UTF-8 name", dir.display())))
}

fn stage_err(message: &str) -> PipelineError {
    PipelineError::Stage {
        stage: "stage-conformance".to_string(),
        message: message.to_string(),
    }
}

// ── Stage impl ───────────────────────────────────────────────────────────────────

/// The `stage-conformance` Transform stage: grades the committed external corpus
/// (native frozen verdict vs published external verdict) and emits the divergences
/// as the in-memory `graph/conformance` N-Quads product
/// [`crate::stages::snapshot`] folds into `gmeow.gts`. It consumes no upstream
/// product — it reads the committed corpus directly.
pub struct ConformanceStage;

impl Stage for ConformanceStage {
    fn id(&self) -> &str {
        "stage-conformance"
    }
    fn kind(&self) -> StageKind {
        StageKind::Transform
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "conformance.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
        // The committed external corpus is a raw source read: every case's
        // `source/manifest.ttl` and `expected/verdicts.json` busts this stage's
        // cache (a corpus edit re-grades + re-folds the bundle).
        let external = root.join(EXTERNAL_ROOT);
        let mut files: Vec<PathBuf> = Vec::new();
        if external.is_dir() {
            for corpus_dir in sorted_dirs(&external)? {
                for case_dir in sorted_dirs(&corpus_dir)? {
                    let manifest = case_dir.join("source").join("manifest.ttl");
                    if manifest.is_file() {
                        files.push(manifest);
                        files.push(case_dir.join("expected").join("verdicts.json"));
                    }
                }
            }
        }
        files.sort();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let nq = build_conformance_divergence(input.root)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(CONFORMANCE_NQ_PATH.to_string(), nq);
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn grades_committed_corpus_deterministically() {
        let root = repo_root();
        let a = build_conformance_divergence(&root).expect("grade run a");
        let b = build_conformance_divergence(&root).expect("grade run b");
        assert_eq!(a, b, "divergence fold must be deterministic");
        // Every emitted quad lands in the conformance graph (never elsewhere).
        let text = String::from_utf8(a).expect("utf-8");
        for line in text.lines() {
            assert!(
                line.ends_with(&format!(
                    "<{}> .",
                    gmeow_conformance::divergence::CONFORMANCE_GRAPH
                )),
                "line not in the conformance graph: {line}"
            );
        }
    }
}
