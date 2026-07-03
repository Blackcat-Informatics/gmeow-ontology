// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! OKF (Open Knowledge Format) import — the lift lane of the agent surface.
//!
//! The Rust port of `gmeow_tools.okf_import`, the mirror of the OKF *export* leaf
//! ([`crate::stages::okf`], which projects GMEOW → OKF). Here an OKF Markdown bundle
//! (the form an LLM or human authors) is lifted back into GMEOW. The fold from
//! Markdown to RDF is the external `gts from-okf` primitive — we **never
//! re-implement that codec** here (the seam doctrine: `gts` owns the OKF↔graph
//! conversion; gmeow owns the ontology lift). This module shells the binary
//! (HARD FAIL if absent — no degraded fallback), then lifts the recognized `okf:`
//! predicates into the standard `rdfs:` / `skos:` / `rdf:` surface.
//!
//! OKF is a LOSSY surface, so the lift is honest about its bounds: the recognized
//! subset (`okf:title` → `rdfs:label`, `okf:description` → `skos:definition`,
//! `okf:type` → `rdf:type`, `okf:scope_notes` / `okf:examples` → the SKOS
//! documentation predicates) is lifted; everything else is **retained verbatim** as
//! `okf:` annotations — self-identifying provenance, never silently dropped.
//!
//! The MAXIMAL(G) back-half reuses the native transform kernel
//! ([`crate::transform::transform_nt`]) — the same back half as the Turtle /
//! YAML-LD transpile paths — so an OKF source is re-expressed across every
//! vocabulary GMEOW can reach.

use std::path::{Path, PathBuf};
use std::process::Command;

use purrdf::{RdfLiteral, RdfQuad, RdfTerm, SerializeGraph};

use crate::error::PipelineError;
use crate::projections::{gts_base_graph, MaximalInputs};
use crate::transform::{transform_nt, TransformReportNative};

/// The `okf:` profile namespace the external `gts` primitive folds to.
pub const OKF_NS: &str = "https://blackcatinformatics.ca/projects/gts/okf#";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const SKOS_SCOPE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#scopeNote";
const SKOS_EXAMPLE: &str = "http://www.w3.org/2004/02/skos/core#example";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
const NT_MEDIA_TYPE: &str = "application/n-triples";

fn stage_err(message: impl Into<String>) -> PipelineError {
    PipelineError::Stage {
        stage: "okf-import".to_string(),
        message: message.into(),
    }
}

/// The `okf:type` string literal → the `rdf:type` IRI it lifts to.
fn type_to_rdf(value: &str) -> Option<&'static str> {
    match value {
        "Class" => Some(OWL_CLASS),
        "Property" => Some(RDF_PROPERTY),
        "Individual" => Some(OWL_NAMED_INDIVIDUAL),
        _ => None,
    }
}

/// A single-valued `okf:<key>` → standard predicate (literal carried straight).
fn scalar_lift(key: &str) -> Option<&'static str> {
    match key.strip_prefix(OKF_NS)? {
        "title" => Some(RDFS_LABEL),
        "description" => Some(SKOS_DEFINITION),
        _ => None,
    }
}

/// A multi-valued `okf:<key>` (an `okf:json` string list) → a SKOS predicate.
fn json_list_lift(key: &str) -> Option<&'static str> {
    match key.strip_prefix(OKF_NS)? {
        "scope_notes" => Some(SKOS_SCOPE_NOTE),
        "examples" => Some(SKOS_EXAMPLE),
        _ => None,
    }
}

/// Account of an OKF → GMEOW lift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OkfLiftReport {
    /// Distinct OKF document subjects seen.
    pub subjects: usize,
    /// Triples lifted to the `rdfs:`/`skos:`/`rdf:` surface.
    pub lifted: usize,
    /// `okf:` triples kept verbatim as lossy annotations.
    pub retained: usize,
}

/// The result of transpiling an OKF bundle directory to MAXIMAL GMEOW.
///
/// The Rust port of `okf_import.OkfTranspileReport`. Where the Python wrote files,
/// this returns the bytes; the calling binary owns the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OkfTranspileReport {
    /// The OKF → GMEOW lift account.
    pub lift: OkfLiftReport,
    /// The pure-GMEOW intermediate draft, as N-Triples.
    pub draft_nt: String,
    /// The MAXIMAL(G) transform report.
    pub transform: TransformReportNative,
}

/// Locate the `gts` CLI (built with OKF support). HARD FAIL if absent.
///
/// The Rust port of `okf_import.find_gts_binary`. Resolution order:
/// `$GMEOW_GTS_BIN` → `gts` on `PATH` → the sibling `gmeow-gts` Rust target dirs
/// (relative to `sibling_base`, when the caller supplies a repo root). No degraded
/// fallback — OKF import requires the external Rust codec, so a missing binary is a
/// hard error with a clear remedy (mirrors the Python `OkfBinaryNotFoundError`).
pub fn find_gts_binary(sibling_base: Option<&Path>) -> Result<PathBuf, PipelineError> {
    if let Some(env) = std::env::var_os("GMEOW_GTS_BIN") {
        let candidate = PathBuf::from(&env);
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(stage_err(format!(
            "GMEOW_GTS_BIN={} is not a file",
            candidate.display()
        )));
    }
    if let Some(on_path) = which_on_path("gts") {
        return Ok(on_path);
    }
    if let Some(base) = sibling_base {
        for rel in ["target/release/gts", "target/debug/gts"] {
            let candidate = base.join("gmeow-gts").join("rust").join(rel);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(stage_err(
        "gts binary with OKF support not found. Build it with \
         `cargo build --release --features okf --bin gts` in the gmeow-gts repo \
         and point GMEOW_GTS_BIN at the resulting binary (or put it on PATH).",
    ))
}

/// The `$PATH` search for an executable file named `name` (the native `shutil.which`).
fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    // Append the platform executable suffix (empty on Unix, `.exe` on Windows)
    // so the lookup resolves `gts.exe` where the OS requires it.
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(&filename);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Fold an OKF bundle directory to a flat GMEOW quad stream via `gts from-okf`.
///
/// The Rust port of `okf_import.okf_dir_to_graph`. Shells the external primitive
/// (the only OKF→graph codec), writing a temporary GTS snapshot into the system
/// temp dir (the consumer runtime path must work from a read-only install), then
/// reads its asserted base triples back through the native GTS loader (reusing
/// [`gts_base_graph`], which drops the RDF-1.2 reifier / quoted-triple rows exactly
/// as the Python compatibility reader did — the asserted `okf:` metadata comes
/// through intact).
pub fn okf_dir_to_graph(
    okf_dir: &Path,
    gts_bin: Option<&Path>,
    sibling_base: Option<&Path>,
) -> Result<Vec<RdfQuad>, PipelineError> {
    let binary = match gts_bin {
        Some(path) => path.to_path_buf(),
        None => find_gts_binary(sibling_base)?,
    };
    let tmp = tempfile::Builder::new()
        .prefix(".gmeow-tmp-okfin-")
        .tempdir()
        .map_err(PipelineError::Io)?;
    let out = tmp.path().join("from-okf.gts");
    let output = Command::new(&binary)
        .arg("from-okf")
        .arg(okf_dir)
        .arg("-o")
        .arg(&out)
        .output()
        .map_err(|e| stage_err(format!("failed to spawn {}: {e}", binary.display())))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stage_err(format!(
            "gts from-okf failed ({}): {}",
            output.status,
            stderr.trim()
        )));
    }
    let bytes = std::fs::read(&out).map_err(PipelineError::Io)?;
    gts_base_graph(&bytes).map_err(stage_err)
}

/// Lift recognized `okf:` predicates to GMEOW; retain the rest as annotations.
///
/// The Rust port of `okf_import.lift_okf_graph`. The recognized subset becomes
/// `rdfs:label` / `skos:definition` / `rdf:type` / `skos:scopeNote` /
/// `skos:example`; every other `okf:` triple is kept verbatim (lossy honesty), and
/// non-`okf:` triples pass through unchanged.
pub fn lift_okf_graph(source: &[RdfQuad]) -> (Vec<RdfQuad>, OkfLiftReport) {
    let mut out: Vec<RdfQuad> = Vec::with_capacity(source.len());
    let mut subjects: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut lifted = 0usize;
    let mut retained = 0usize;
    let okf_type = format!("{OKF_NS}type");
    let okf_resource = format!("{OKF_NS}resource");

    for quad in source {
        let predicate = quad.predicate.as_str();
        if !predicate.starts_with(OKF_NS) {
            out.push(quad.clone());
            continue;
        }
        subjects.insert(subject_key(&quad.subject));

        if predicate == okf_type {
            if let RdfTerm::Literal(lit) = &quad.object {
                if let Some(rdf_type) = type_to_rdf(&lit.lexical_form) {
                    out.push(RdfQuad::new(
                        quad.subject.clone(),
                        RDF_TYPE,
                        RdfTerm::Iri(rdf_type.to_string()),
                    ));
                    lifted += 1;
                    continue;
                }
            }
        } else if let Some(target) = scalar_lift(predicate) {
            out.push(RdfQuad::new(
                quad.subject.clone(),
                target,
                quad.object.clone(),
            ));
            lifted += 1;
            continue;
        } else if let Some(target) = json_list_lift(predicate) {
            if let RdfTerm::Literal(lit) = &quad.object {
                for item in json_list(&lit.lexical_form) {
                    out.push(RdfQuad::new(
                        quad.subject.clone(),
                        target,
                        RdfTerm::Literal(RdfLiteral::simple(item)),
                    ));
                    lifted += 1;
                }
                continue;
            }
        } else if predicate == okf_resource {
            // The subject already IS the resource IRI (gts from-okf mints it from
            // resource:); the explicit okf:resource triple is redundant identity —
            // drop it rather than retain a self-reference.
            continue;
        }
        // Unmapped okf:* — retained verbatim as a provenance-bearing annotation.
        out.push(quad.clone());
        retained += 1;
    }

    (
        out,
        OkfLiftReport {
            subjects: subjects.len(),
            lifted,
            retained,
        },
    )
}

/// Transpile an OKF bundle directory to MAXIMAL GMEOW.
///
/// The Rust port of `okf_import.transpile_okf`, chaining the lift and the MAXIMAL
/// back-half end to end: `gts from-okf` folds the Markdown bundle, the recognized
/// `okf:` predicates are lifted to GMEOW (unmapped ones retained), the pure-GMEOW
/// draft is produced, then `MAXIMAL(G) = G + E(G) + P(G)` is run over it via
/// [`transform_nt`]. `maximal` carries the repo/bundle-derived inputs the back-half
/// needs (ontology, cells, denied set, projection queries), passed in so this driver
/// stays consumer-safe.
///
/// # Errors
///
/// - The external `gts` binary is missing (HARD FAIL).
/// - Nothing lifts to GMEOW (an empty draft has nothing to project — surfaced, not a
///   silent empty publication).
pub fn transpile_okf(
    okf_dir: &Path,
    maximal: &MaximalInputs,
    gts_bin: Option<&Path>,
    sibling_base: Option<&Path>,
) -> Result<OkfTranspileReport, PipelineError> {
    let graph = okf_dir_to_graph(okf_dir, gts_bin, sibling_base)?;
    let (lifted, report) = lift_okf_graph(&graph);
    if report.lifted == 0 {
        return Err(stage_err(format!(
            "transpile: nothing lifted to GMEOW from OKF bundle {}",
            okf_dir.display()
        )));
    }
    let draft_nt = quads_to_nt(&lifted)?;
    let transform = transform_nt(
        &draft_nt,
        &maximal.ontology_nt,
        &maximal.cells,
        &maximal.denied,
        &maximal.projection_queries,
    )
    .map_err(stage_err)?;
    Ok(OkfTranspileReport {
        lift: report,
        draft_nt,
        transform,
    })
}

/// Parse an `okf:json` list literal into its string items (best-effort): a JSON
/// string array yields its items; anything else falls back to the raw lexical form.
fn json_list(lexical: &str) -> Vec<String> {
    match serde_json::from_str::<serde_json::Value>(lexical) {
        Ok(serde_json::Value::Array(items)) => items
            .into_iter()
            .map(|item| match item {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
            .collect(),
        Ok(serde_json::Value::String(s)) => vec![s],
        _ => vec![lexical.to_string()],
    }
}

/// A stable identity key for a subject term (for the distinct-subject count).
fn subject_key(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => format!("<{iri}>"),
        RdfTerm::BlankNode(b) => format!("_:{b}"),
        other => format!("{other:?}"),
    }
}

/// Serialize a flat default-graph quad stream to canonical N-Triples.
fn quads_to_nt(quads: &[RdfQuad]) -> Result<String, PipelineError> {
    let flat = purrdf::flat_dataset_from_quads(quads)
        .map_err(|e| stage_err(format!("N-Triples flatten failed: {e}")))?;
    let bytes =
        purrdf::serialize_dataset(flat.as_ref(), NT_MEDIA_TYPE, SerializeGraph::DefaultGraph)
            .map_err(|e| stage_err(format!("N-Triples serialization failed: {e}")))?;
    String::from_utf8(bytes).map_err(|e| stage_err(format!("N-Triples output is not UTF-8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resolver's two HARD-FAIL paths, exercised sequentially in ONE test so the
    /// process-global env vars they mutate never race a sibling test:
    ///
    /// 1. a `GMEOW_GTS_BIN` pointing at a non-file is refused (not a silent PATH
    ///    fallthrough) — the explicit override must be honored or refused;
    /// 2. with no `GMEOW_GTS_BIN`, no `gts` on a stripped `PATH`, and no sibling base,
    ///    the resolver errors with the build-it remedy (no degraded fallback).
    #[test]
    fn find_gts_binary_hard_fails_when_absent() {
        let saved_path = std::env::var_os("PATH");
        let saved_bin = std::env::var_os("GMEOW_GTS_BIN");

        // Case 1: a non-file override.
        // SAFETY: single-threaded test body; the env is restored at the end.
        unsafe { std::env::set_var("GMEOW_GTS_BIN", "/nonexistent/gts-binary-xyz") };
        let err = find_gts_binary(None).expect_err("non-file override must fail");
        assert!(
            err.to_string().contains("is not a file"),
            "non-file override refused, got: {err}"
        );

        // Case 2: nothing resolvable at all.
        unsafe {
            std::env::remove_var("GMEOW_GTS_BIN");
            std::env::set_var("PATH", "");
        }
        let err = find_gts_binary(None).expect_err("must hard-fail when absent");
        let msg = err.to_string();
        assert!(
            msg.contains("gts binary with OKF support not found"),
            "clear not-found message, got: {msg}"
        );
        assert!(
            msg.contains("--features okf"),
            "message carries the build remedy, got: {msg}"
        );

        // Restore the process environment.
        unsafe {
            match saved_path {
                Some(v) => std::env::set_var("PATH", v),
                None => std::env::remove_var("PATH"),
            }
            match saved_bin {
                Some(v) => std::env::set_var("GMEOW_GTS_BIN", v),
                None => std::env::remove_var("GMEOW_GTS_BIN"),
            }
        }
    }

    #[test]
    fn lift_maps_the_recognized_okf_subset() {
        let subject = RdfTerm::Iri("https://example.org/Dog".to_string());
        let source = vec![
            RdfQuad::new(
                subject.clone(),
                format!("{OKF_NS}type"),
                RdfTerm::Literal(RdfLiteral::simple("Class")),
            ),
            RdfQuad::new(
                subject.clone(),
                format!("{OKF_NS}title"),
                RdfTerm::Literal(RdfLiteral::simple("Dog")),
            ),
            RdfQuad::new(
                subject.clone(),
                format!("{OKF_NS}examples"),
                RdfTerm::Literal(RdfLiteral::simple("[\"Rex\", \"Fido\"]")),
            ),
            // An unmapped okf:* triple is retained verbatim.
            RdfQuad::new(
                subject.clone(),
                format!("{OKF_NS}path"),
                RdfTerm::Literal(RdfLiteral::simple("classes/Dog.md")),
            ),
            // The redundant okf:resource self-reference is dropped.
            RdfQuad::new(
                subject.clone(),
                format!("{OKF_NS}resource"),
                RdfTerm::Iri("https://example.org/Dog".to_string()),
            ),
        ];
        let (out, report) = lift_okf_graph(&source);

        // type→owl:Class, title→rdfs:label, two example items → skos:example.
        assert_eq!(report.lifted, 4, "type + title + 2 examples lifted");
        assert_eq!(report.retained, 1, "okf:path retained");
        assert_eq!(report.subjects, 1);

        let has = |p: &str, matcher: &dyn Fn(&RdfTerm) -> bool| {
            out.iter().any(|q| q.predicate == p && matcher(&q.object))
        };
        assert!(has(
            RDF_TYPE,
            &|o| matches!(o, RdfTerm::Iri(i) if i == OWL_CLASS)
        ));
        assert!(has(
            RDFS_LABEL,
            &|o| matches!(o, RdfTerm::Literal(l) if l.lexical_form == "Dog")
        ));
        let examples: Vec<&str> = out
            .iter()
            .filter(|q| q.predicate == SKOS_EXAMPLE)
            .filter_map(|q| match &q.object {
                RdfTerm::Literal(l) => Some(l.lexical_form.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(examples, vec!["Rex", "Fido"]);
        // The okf:resource identity triple never survives.
        assert!(!out
            .iter()
            .any(|q| q.predicate == format!("{OKF_NS}resource")));
        // The okf:path annotation is retained verbatim.
        assert!(out.iter().any(|q| q.predicate == format!("{OKF_NS}path")));
    }

    #[test]
    fn non_okf_triples_pass_through_unchanged() {
        let subject = RdfTerm::Iri("https://example.org/Dog".to_string());
        let source = vec![RdfQuad::new(
            subject,
            RDFS_LABEL,
            RdfTerm::Literal(RdfLiteral::simple("Dog")),
        )];
        let (out, report) = lift_okf_graph(&source);
        assert_eq!(report.lifted, 0);
        assert_eq!(report.retained, 0);
        assert_eq!(out.len(), 1, "the non-okf triple passes through");
    }
}
