// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `mappings` stage (#861 P3): compile the alignment artifacts.
//!
//! Two of the four mapping families are ALREADY pure Rust and are wired directly
//! here — no port:
//!   * **SSSOM** → `gmeow_slice::emit_sssom_sets(root)` (byte-identical to the
//!     historical Python emitter, its own parity gate) → `generated/mappings/*.sssom.tsv`.
//!   * **FnO** → `gmeow_slice::emit_fno(root)` → `generated/projections/functions.fno.ttl`.
//!
//! The remaining two families — **EDOAL** (`emit_edoal`) and the **SPARQL
//! CONSTRUCT** projections (`emit_sparql`, the closed-algebra renderer in
//! `mapping_dsl.py`) — are now ALSO pure Rust (#861 P3/P4):
//!   * **EDOAL** → `gmeow_slice::emit_edoal_sets(root)` (byte-identical to the
//!     historical Python emitter — built as N-Triples then serialized through the
//!     project's canonical Turtle serializer) → `generated/projections/*.edoal.ttl`.
//!   * **SPARQL CONSTRUCT** → `gmeow_slice::emit_sparql_sets(root)` (the
//!     closed-algebra text renderer) → `generated/queries/*.rq`.
//!
//! NOT produced here: the six hand-authored standpoint `.rq` (`standpoint-*.rq`,
//! pure-text emitters with no DSL input) and `generated/mappings/dsl-stats.json`
//! (the DSL surface-count summary). Those are emitted by the Python driver's other
//! branches; this stage covers the four DSL-driven mapping families.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_slice::emit_sssom_sets;
use gmeow_slice::fno_emit::emit_fno;
use gmeow_slice::{emit_edoal_sets, emit_sparql_sets};

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// Directory (logical-path prefix) of the SSSOM TSV sets.
pub const SSSOM_DIR: &str = "generated/mappings";
/// Committed logical path of the FnO transform catalog.
pub const FNO_PATH: &str = "generated/projections/functions.fno.ttl";
/// Directory (logical-path prefix) of the EDOAL alignment Turtle files.
pub const EDOAL_DIR: &str = "generated/projections";
/// Directory (logical-path prefix) of the SPARQL CONSTRUCT projection queries.
pub const QUERIES_DIR: &str = "generated/queries";

/// Compile the four DSL-driven mapping families (SSSOM + FnO + EDOAL + SPARQL)
/// from `root`, returning `{logical_path → bytes}`.
pub fn compile_mappings(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // SSSOM — byte-identical to the Python emitter.
    let sssom = emit_sssom_sets(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("SSSOM emission failed: {e}"),
    })?;
    for (filename, tsv) in sssom {
        artifacts.insert(format!("{SSSOM_DIR}/{filename}"), tsv.into_bytes());
    }

    // FnO — the transform catalog as N-Triples (compared by isomorphism).
    let fno = emit_fno(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("FnO emission failed: {e}"),
    })?;
    artifacts.insert(FNO_PATH.to_string(), fno.into_bytes());

    // EDOAL — per-profile alignment Turtle (byte-identical to the Python emitter).
    let edoal = emit_edoal_sets(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("EDOAL emission failed: {e}"),
    })?;
    for (filename, ttl) in edoal {
        artifacts.insert(format!("{EDOAL_DIR}/{filename}"), ttl.into_bytes());
    }

    // SPARQL CONSTRUCT — per-profile executable projection queries.
    let sparql = emit_sparql_sets(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("SPARQL emission failed: {e}"),
    })?;
    for (filename, rq) in sparql {
        artifacts.insert(format!("{QUERIES_DIR}/{filename}"), rq.into_bytes());
    }

    Ok(artifacts)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `mappings` pipeline stage (SSSOM + FnO + EDOAL + SPARQL).
pub struct MappingsStage;

impl Stage for MappingsStage {
    fn id(&self) -> &str {
        "stage-mappings"
    }
    fn kind(&self) -> StageKind {
        StageKind::Transform
    }
    fn consumes(&self) -> &[String] {
        // Reads dsl/mappings + slice mapping cells from the root (like statements
        // reads dsl/statements). The slice DAG edge is reconciled at P6 wiring.
        &[]
    }
    fn impl_version(&self) -> &str {
        "mappings.v2-sssom-fno-edoal-sparql"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let artifacts = compile_mappings(input.root)?;
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::{RdfFormat, RdfParser};
    use oxigraph::store::Store;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn triple_set(bytes: &[u8], format: RdfFormat) -> std::collections::BTreeSet<String> {
        let store = Store::new().unwrap();
        for quad in RdfParser::from_format(format).lenient().for_reader(bytes) {
            store.insert(&quad.unwrap()).unwrap();
        }
        store
            .iter()
            .map(|q| {
                let q = q.unwrap();
                format!("{} {} {} .", q.subject, q.predicate, q.object)
            })
            .collect()
    }

    #[test]
    fn sssom_emits_and_overlaps_byte_identically_with_committed() {
        // The stage wires `gmeow_slice::emit_sssom_sets` — the SAME Rust the
        // Python build calls — so for every set it emits that has a committed
        // counterpart, the bytes MUST match exactly (the emitter's own parity
        // contract). The total set count vs committed is subject to the
        // committed-vs-local env/staleness drift and is the CI `check-generated`
        // gate, not asserted here.
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");
        let mut overlap = 0usize;
        for (path, bytes) in &artifacts {
            if !path.ends_with(".sssom.tsv") {
                continue;
            }
            if let Ok(committed) = std::fs::read(root.join(path)) {
                assert_eq!(bytes, &committed, "SSSOM {path} drifted from committed");
                overlap += 1;
            }
        }
        assert!(
            overlap >= 60,
            "expected 60+ SSSOM sets byte-matching committed, got {overlap}"
        );
    }

    #[test]
    fn edoal_and_sparql_emit_byte_identically_with_committed() {
        // The stage wires `gmeow_slice::emit_edoal_sets` / `emit_sparql_sets` — the
        // same Rust the byte-parity unit tests in the slice crate exercise. Every
        // EDOAL `.edoal.ttl` and SPARQL `.rq` the stage emits MUST equal its
        // committed counterpart byte-for-byte (the emitters' parity contract).
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");
        let mut edoal = 0usize;
        let mut sparql = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for (path, bytes) in &artifacts {
            let is_edoal = path.starts_with(EDOAL_DIR) && path.ends_with(".edoal.ttl");
            let is_sparql = path.starts_with(QUERIES_DIR) && path.ends_with(".rq");
            if !is_edoal && !is_sparql {
                continue;
            }
            let committed = std::fs::read(root.join(path))
                .unwrap_or_else(|_| panic!("committed missing: {path}"));
            if bytes != &committed {
                let got = String::from_utf8_lossy(bytes);
                let want = String::from_utf8_lossy(&committed);
                let mut detail = String::from("len/content differ");
                for (i, (a, b)) in got.lines().zip(want.lines()).enumerate() {
                    if a != b {
                        detail = format!("line {}: got {a:?} want {b:?}", i + 1);
                        break;
                    }
                }
                failures.push(format!("{path}: {detail}"));
            } else if is_edoal {
                edoal += 1;
            } else {
                sparql += 1;
            }
        }
        assert!(
            failures.is_empty(),
            "EDOAL/SPARQL byte-parity drift:\n{}",
            failures.join("\n")
        );
        assert_eq!(
            edoal, 45,
            "expected 45 EDOAL files byte-matching, got {edoal}"
        );
        assert_eq!(
            sparql, 45,
            "expected 45 SPARQL files byte-matching, got {sparql}"
        );
    }

    #[test]
    fn fno_is_well_formed_ntriples() {
        // Wiring check: `emit_fno` produces a non-empty FnO catalog that parses.
        // (Committed-byte/iso parity is the CI `check-generated` gate, env-matched.)
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");
        let fno = artifacts.get(FNO_PATH).expect("fno artifact");
        let triples = triple_set(fno, RdfFormat::NTriples);
        assert!(
            triples.len() > 20,
            "FnO catalog unexpectedly small: {} triples",
            triples.len()
        );
    }
}
