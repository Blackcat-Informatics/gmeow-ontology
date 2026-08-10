// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! OKF (Open Knowledge Format) import — the lift lane of the agent surface.
//!
//! The Rust port of `gmeow_tools.okf_import`, the mirror of the OKF *export* leaf
//! ([`crate::stages::okf`], which projects GMEOW → OKF). Here an OKF Markdown bundle
//! (the form an LLM or human authors) is lifted back into GMEOW. The fold from
//! Markdown to RDF is purrdf's native, in-process OKF codec
//! ([`purrdf::lift_okf_bundle`]) — there is no external binary in this path any
//! more (the former `gts from-okf` subprocess seam is retired: purrdf now ships
//! the codec directly). This module builds the [`purrdf::OkfBundle`] from the
//! on-disk directory, lifts it through purrdf's reader, then lifts the recognized
//! `okf:` predicates into the standard `rdfs:` / `skos:` / `rdf:` surface.
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

use std::path::Path;

use purrdf::{DatasetSink, OkfBundle, OkfConfig, RdfLiteral, RdfQuad, RdfTerm, SerializeGraph};

use crate::projections::{MaximalInputs, TagMap};
use crate::transform::{TransformReportNative, transform_nt};

/// The `okf:` profile namespace purrdf's native codec folds to (and the export
/// leaf, [`crate::stages::okf`], mints its `resource:` frontmatter under).
pub const OKF_NS: &str = "https://blackcatinformatics.ca/projects/gts/okf#";

/// The base IRI a bundle document without an explicit `resource:` frontmatter
/// field is minted under (percent-encoded bundle-relative path appended). Every
/// document gmeow's own OKF export produces always carries `resource:` (the
/// term's real IRI), so this base only fires for a freshly hand-authored concept
/// that has no pre-existing GMEOW IRI yet.
const OKF_DOCUMENT_BASE: &str = "https://blackcatinformatics.ca/gmeow/okf-bundle/";

/// The RDF-1.2 reifier predicate — filtered out of the flattened lift output the
/// same way [`crate::projections::gts_base_graph`] filters it from a `.gts`
/// read: OKF import only wants the asserted `okf:` base triples (Markdown-link
/// edges included), never the RDF-star reifier / quoted-triple sidecar rows the
/// reader emits for link text/occurrence provenance.
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// The closed frontmatter-key profile purrdf's [`OkfConfig`] validates against —
/// exactly the field set [`crate::stages::okf::render_okf`] ever emits, so
/// gmeow's own bundles always round-trip and a hand-authored bundle is validated
/// against the same closed vocabulary (an unrecognized key is now a HARD FAIL,
/// never a silently-accepted ad-hoc predicate).
const OKF_RECOGNIZED_KEYS: &[&str] = &[
    "type",
    "title",
    "description",
    "resource",
    "tags",
    "version",
    "curie",
    "parents",
    "prop_kind",
    "domain",
    "range",
    "functional",
    "sub_property_of",
    "types",
    "alignments",
    "scope_notes",
    "examples",
    "use_when",
    "avoid_when",
    "how_to_use",
    "use_for_consumer",
    "avoid_for_consumer",
];

/// Build the mandatory purrdf OKF profile shared by every import.
fn okf_import_config() -> Result<OkfConfig, gmeow_errors::Diag> {
    OkfConfig::new(
        OKF_NS,
        OKF_DOCUMENT_BASE,
        OKF_RECOGNIZED_KEYS.iter().copied(),
    )
    .map_err(|e| stage_err(format!("invalid OKF import profile: {e}")))
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const SKOS_SCOPE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#scopeNote";
const SKOS_EXAMPLE: &str = "http://www.w3.org/2004/02/skos/core#example";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
const NT_MEDIA_TYPE: &str = "application/n-triples";

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "okf-import".to_string(),
        message: message.into(),
    })
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

/// Read every `.md` file under `okf_dir` into a purrdf [`OkfBundle`], keyed by
/// its POSIX-normalized path relative to `okf_dir`. Deterministic (bundle paths
/// are collected then handed to the bundle in whatever order `read_dir` returns
/// — `OkfBundle` is a `BTreeMap` internally, so bundle iteration is lexical
/// regardless of insertion order).
///
/// # Errors
///
/// Returns a diagnostic on any filesystem error, non-UTF-8 document, or an
/// invalid/unsafe bundle path (delegated to [`OkfBundle::insert`]).
fn read_okf_bundle_dir(okf_dir: &Path) -> Result<OkfBundle, gmeow_errors::Diag> {
    let mut bundle = OkfBundle::new();
    let mut stack = vec![okf_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                message: format!("read OKF bundle dir {}: {e}", dir.display()),
            })
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Io {
                    message: format!("read OKF bundle dir {}: {e}", dir.display()),
                })
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Io {
                    message: format!("stat {}: {e}", path.display()),
                })
            })?;
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let relative = path.strip_prefix(okf_dir).map_err(|e| {
                stage_err(format!(
                    "OKF bundle document {} is not under {}: {e}",
                    path.display(),
                    okf_dir.display()
                ))
            })?;
            let relative_posix = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let text = std::fs::read_to_string(&path).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Io {
                    message: format!("read {}: {e}", path.display()),
                })
            })?;
            bundle
                .insert(relative_posix, text)
                .map_err(|e| stage_err(format!("{}: {e}", path.display())))?;
        }
    }
    Ok(bundle)
}

/// Fold an OKF bundle directory to a flat GMEOW quad stream via purrdf's native,
/// in-process OKF codec.
///
/// The Rust port of `okf_import.okf_dir_to_graph`, now calling
/// [`purrdf::lift_okf_bundle`] directly instead of shelling an external `gts`
/// binary: reads the bundle directory into an [`OkfBundle`], lifts it through
/// purrdf's reader into a [`DatasetSink`], then flattens the resulting
/// [`purrdf::RdfDataset`] and drops the RDF-1.2 reifier / quoted-triple rows
/// (the same filter [`crate::projections::gts_base_graph`] applies to a `.gts`
/// read) — the asserted `okf:` metadata (including the `okf:links` Markdown-link
/// edges) comes through intact; only the reifier-carried link text/occurrence
/// provenance is dropped, matching the prior subprocess contract exactly.
///
/// # Errors
///
/// Returns a diagnostic on any filesystem error, an unsafe/duplicate bundle
/// path, malformed frontmatter, an unrecognized frontmatter key, or a dangling
/// Markdown-link target (all HARD FAILs — no degraded fallback).
pub fn okf_dir_to_graph(okf_dir: &Path) -> Result<Vec<RdfQuad>, gmeow_errors::Diag> {
    let bundle = read_okf_bundle_dir(okf_dir)?;
    let config = okf_import_config()?;
    let mut sink = DatasetSink::new();
    let outcome = purrdf::lift_okf_bundle(&bundle, &config, &mut sink)
        .map_err(|e| stage_err(format!("OKF bundle lift failed: {e}")))?;
    if outcome.cancelled {
        return Err(stage_err(
            "OKF bundle lift was cancelled before the sink finished",
        ));
    }
    let dataset = sink
        .into_dataset()
        .ok_or_else(|| stage_err("OKF bundle lift did not finish the sink"))?;
    let flat = purrdf::flat_rdf_quads_from_dataset(dataset.as_ref());
    let mut base = Vec::with_capacity(flat.len());
    for quad in flat {
        if quad.predicate == RDF_REIFIES
            || matches!(quad.subject, RdfTerm::Triple(_))
            || matches!(quad.object, RdfTerm::Triple(_))
        {
            continue;
        }
        base.push(RdfQuad::new(quad.subject, quad.predicate, quad.object));
    }
    Ok(base)
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
            if let RdfTerm::Literal(lit) = &quad.object
                && let Some(rdf_type) = type_to_rdf(&lit.lexical_form)
            {
                out.push(RdfQuad::new(
                    quad.subject.clone(),
                    RDF_TYPE,
                    RdfTerm::Iri(rdf_type.to_string()),
                ));
                lifted += 1;
                continue;
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
            // The subject already IS the resource IRI (purrdf's OKF reader mints
            // it from `resource:`); the explicit okf:resource triple is redundant
            // identity — drop it rather than retain a self-reference.
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
/// back-half end to end: purrdf's native OKF codec folds the Markdown bundle, the
/// recognized `okf:` predicates are lifted to GMEOW (unmapped ones retained), the
/// pure-GMEOW draft is produced, then `MAXIMAL(G) = G + E(G) + P(G)` is run over
/// it via [`transform_nt`]. `maximal` carries the repo/bundle-derived inputs the
/// back-half needs (ontology, cells, denied set, projection queries), passed in
/// so this driver stays consumer-safe. `tag_map` is the internal→public BCP-47
/// language-tag remap applied at the MAXIMAL(G) output boundary (empty = no-op)
/// — see [`transform_nt`]'s doc comment for why this is load-bearing, not
/// cosmetic.
///
/// # Errors
///
/// - The bundle directory is malformed (unsafe path, invalid YAML frontmatter,
///   unrecognized frontmatter key, dangling Markdown-link target — all HARD FAIL).
/// - Nothing lifts to GMEOW (an empty draft has nothing to project — surfaced, not a
///   silent empty publication).
pub fn transpile_okf(
    okf_dir: &Path,
    maximal: &MaximalInputs,
    tag_map: &TagMap,
) -> Result<OkfTranspileReport, gmeow_errors::Diag> {
    let graph = okf_dir_to_graph(okf_dir)?;
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
        tag_map,
    )
    .map_err(|e| stage_err(e.to_string()))?;
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
fn quads_to_nt(quads: &[RdfQuad]) -> Result<String, gmeow_errors::Diag> {
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

    /// `okf_dir_to_graph` reads a bundle directory (nested subdirectories
    /// included), lifts it through purrdf's native in-process codec (no
    /// subprocess, no temp `.gts` file), and returns the flattened asserted
    /// `okf:` base triples — the `okf:links` Markdown-link edge survives, but the
    /// RDF-star reifier / linkText / linkOccurrence sidecar rows the reader
    /// emits are filtered out (matching the prior `gts_base_graph` contract).
    #[test]
    fn okf_dir_to_graph_lifts_a_bundle_directory_in_process() {
        let tmp = tempfile::Builder::new()
            .prefix(".gmeow-test-okfin-")
            .tempdir()
            .expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("classes")).expect("mkdir");
        std::fs::write(
            tmp.path().join("classes/schema.md"),
            "---\ntype: Class\ntitle: Schema\n---\nColumns: id.\n",
        )
        .expect("write schema.md");
        std::fs::write(
            tmp.path().join("classes/table.md"),
            "---\ntype: Class\ntitle: Table\nresource: https://example.org/data/Table\n---\nSee [schema](schema.md).\n",
        )
        .expect("write table.md");

        let quads = okf_dir_to_graph(tmp.path()).expect("lift bundle directory");
        assert!(!quads.is_empty(), "expected lifted quads");

        let type_predicate = format!("{OKF_NS}type");
        let links_predicate = format!("{OKF_NS}links");
        assert!(
            quads.iter().any(|q| q.predicate == type_predicate),
            "expected an okf:type triple"
        );
        assert!(
            quads.iter().any(|q| q.predicate == links_predicate),
            "expected the schema->table okf:links edge"
        );
        // No RDF-1.2 reifier / quoted-triple row survives the filter.
        assert!(
            quads.iter().all(|q| q.predicate != RDF_REIFIES
                && !matches!(q.subject, RdfTerm::Triple(_))
                && !matches!(q.object, RdfTerm::Triple(_))),
            "reifier/quoted-triple rows must be filtered out"
        );
    }

    /// An unrecognized frontmatter key is a HARD FAIL under the closed
    /// [`OKF_RECOGNIZED_KEYS`] profile — never a silently-accepted ad-hoc
    /// predicate.
    #[test]
    fn okf_dir_to_graph_hard_fails_on_unrecognized_frontmatter_key() {
        let tmp = tempfile::Builder::new()
            .prefix(".gmeow-test-okfin-bad-")
            .tempdir()
            .expect("tempdir");
        std::fs::write(
            tmp.path().join("concept.md"),
            "---\ntype: Class\nunrecognized_key: value\n---\nBody.\n",
        )
        .expect("write concept.md");

        let err = okf_dir_to_graph(tmp.path()).expect_err("unrecognized key must hard-fail");
        assert!(
            err.to_string().contains("unrecognized"),
            "expected an unrecognized-key error, got: {err}"
        );
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
        assert!(
            !out.iter()
                .any(|q| q.predicate == format!("{OKF_NS}resource"))
        );
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
