// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native ontology-surface authoring gates — the whole-corpus structural
//! invariants that recreate the retired pytest cluster (`test_shapes.py`,
//! `test_vocabulary_surface.py`, `test_norms.py`, `test_slices.py`).
//!
//! Every gate is a **detector** returning `Vec<Finding>` (mirroring
//! [`crate::slice_ownership::ownership_findings`]) so the *sink* is a per-family
//! policy knob, not an architecture fork: the live-loader-hole families are folded
//! into the [`crate::validate_all`] `ValidationRun` (so `make validate` HARD-FAILS
//! on the real corpus), and each detector is additionally exercised by a
//! synthetic-negative unit test (proving it fires) and a whole-corpus integration
//! test (asserting the committed corpus is clean, with non-vacuity guards).
//!
//! The gates *read* the corpus; they never author a `sh:NodeShape` — no second
//! source of truth is introduced.
//!
//! Each detector splits into a corpus-loading `pub fn` and a pure inner function
//! over already-parsed [`Dataset`]s, so the synthetic negatives can drive the
//! detection logic without a full repository layout.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_errors::{Diag, Finding, Location, Result, Severity};
use purrdf::slice::rdf_query::{Dataset, Object, Subject};

use crate::codes;

/// The tool / SARIF-rule namespace for every authoring-integrity finding.
const TOOL: &str = "authoring-integrity";

const SH_NODE_SHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
const SLICE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Slice";
const SLICE_TIER: &str = "https://blackcatinformatics.ca/gmeow/sliceTier";

/// The core `rights` module, parsed in isolation for the graft-isolation gate.
const CORE_RIGHTS_MODULE: &str = "slices/core/rights/module.ttl";

/// The norms-extension IRIs the core `rights` module must never reference (in any
/// triple position) — the graft is asserted on the extension side only, matching
/// the retired `test_graft_axioms_live_extension_side_only`. Matched by **exact
/// term identity**, never substring (`normIssuer` must not match `normIssuerRole`).
const NORMS_EXTENSION_TERMS: &[&str] = &[
    "https://blackcatinformatics.ca/gmeow/Norm",
    "https://blackcatinformatics.ca/gmeow/deonticModality",
    "https://blackcatinformatics.ca/gmeow/normIssuer",
    "https://blackcatinformatics.ca/gmeow/normBearer",
];

// ── shared helpers ───────────────────────────────────────────────────────────

/// Build one authoring-integrity finding, hanging the offending IRI off a logical
/// location for SARIF grouping (as [`crate::slice_ownership`] does).
fn finding(severity: Severity, code: &str, message: String, logical: Option<String>) -> Finding {
    let mut f = Finding::new(severity, code, message).with_tool(TOOL);
    if let Some(iri) = logical {
        f.add_location(Location::new(None, None, None, Some(iri)));
    }
    f
}

fn io_err(path: &Path, e: &std::io::Error) -> Diag {
    Diag::of_kind(crate::error::Io {
        detail: format!("{}: {e}", path.display()),
    })
}

fn parse_err(path: &Path, e: &str) -> Diag {
    Diag::of_kind(crate::error::Parse {
        detail: format!("{}: {e}", path.display()),
    })
}

/// Parse a Turtle file into a native [`Dataset`], hard-failing on read/parse error
/// (no-optionality: a committed corpus file that cannot be read or parsed is a
/// HARD FAIL, never a silently skipped gate input).
fn parse_ttl(path: &Path) -> Result<Dataset> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, &e))?;
    Dataset::parse_turtle(&bytes, &path.display().to_string())
        .map_err(|e| parse_err(path, &e.to_string()))
}

/// Render a path relative to the repo root for a stable, location-independent
/// message (falls back to the full path when it is not under the root).
fn rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Every `manifest.ttl` under `slices_dir` — the discovery surface the retired
/// `discover_slices` walked (all manifests, so a duplicate IRI or a tierless
/// manifest anywhere is caught). Deterministically sorted.
fn all_manifests(slices_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![slices_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "manifest.ttl") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

// ── R1: shape-IRI ownership collision ────────────────────────────────────────

/// Every `sh:NodeShape` IRI must be declared in exactly one shape file: merged
/// into one graph, two files declaring the same IRI fuse into a shape whose
/// meaning depends on parse order (the retired
/// `test_no_nodeshape_iri_collision_across_shape_files`).
///
/// Uses [`purrdf::shapes::shape_union::shape_files`] — byte-identical to the merged
/// corpus the live validator sees (base `shapes/*.ttl` minus the DSL lints, the
/// fail-closed `generated/shapes/*.ttl`, and every `slices/*/*/shapes.ttl`).
pub fn shape_iri_collision_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let files = purrdf::shapes::shape_union::shape_files(repo_root)
        .map_err(|e| Diag::of_kind(crate::error::Io { detail: e }))?;
    let mut loaded: Vec<(PathBuf, Dataset)> = Vec::with_capacity(files.len());
    for f in files {
        let ds = parse_ttl(&f)?;
        loaded.push((f, ds));
    }
    detect_shape_collisions(&loaded, repo_root)
}

/// The pure collision logic over already-parsed shape files: a `sh:NodeShape` IRI
/// (named subjects only — `subjects_of_type` excludes blank nodes) appearing in
/// more than one file is an ownership collision.
fn detect_shape_collisions(files: &[(PathBuf, Dataset)], root: &Path) -> Result<Vec<Finding>> {
    let mut iri_to_files: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (path, ds) in files {
        let shapes = ds
            .subjects_of_type(SH_NODE_SHAPE)
            .map_err(|e| parse_err(path, &e.to_string()))?;
        for iri in shapes {
            iri_to_files.entry(iri).or_default().push(path.clone());
        }
    }
    let mut findings = Vec::new();
    for (iri, mut owners) in iri_to_files {
        if owners.len() > 1 {
            owners.sort();
            let list = owners
                .iter()
                .map(|p| rel(p, root))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_SHAPE_IRI_COLLISION,
                format!(
                    "sh:NodeShape {iri} is declared in {n} shape files ({list}) — a merged \
                     shape graph must define each shape IRI exactly once",
                    n = owners.len(),
                ),
                Some(iri),
            ));
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    Ok(findings)
}

// ── R4: graft isolation ──────────────────────────────────────────────────────

/// The core `rights` module must reference zero norms-extension IRIs (the retired
/// `test_graft_axioms_live_extension_side_only`): the graft lives on the extension
/// side only, with zero core churn.
pub fn graft_isolation_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let path = repo_root.join(CORE_RIGHTS_MODULE);
    let ds = parse_ttl(&path)?;
    Ok(detect_graft_leaks(&ds, &rel(&path, repo_root)))
}

/// The pure graft-leak logic: any norms-extension IRI appearing in subject,
/// predicate, or object position by **exact term identity**.
fn detect_graft_leaks(ds: &Dataset, source_label: &str) -> Vec<Finding> {
    // Ordered set of leaked terms → the positions they were found in.
    let mut leaked: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    let mut note = |term: &str, position: &'static str| {
        if let Some(canon) = NORMS_EXTENSION_TERMS.iter().find(|t| **t == term) {
            let positions = leaked.entry(*canon).or_default();
            if !positions.contains(&position) {
                positions.push(position);
            }
        }
    };
    ds.for_each_quad(|s, p, o, _g| {
        if let Subject::Named(iri) = &s {
            note(iri, "subject");
        }
        note(p, "predicate");
        if let Object::Named(iri) = &o {
            note(iri, "object");
        }
    });
    leaked
        .into_iter()
        .map(|(term, positions)| {
            finding(
                Severity::Error,
                codes::AUTHORING_GRAFT_LEAK,
                format!(
                    "core module {source_label} references norms-extension IRI {term} (in {pos}) \
                     — the norms graft must live on the extension side only",
                    pos = positions.join("/"),
                ),
                Some(term.to_string()),
            )
        })
        .collect()
}

// ── R6: slice discipline (duplicate IRI + mandatory tier) ────────────────────

/// Slice discipline: a slice IRI is manifest-only identity and must be unique, and
/// every `gmeow:Slice` manifest must carry a `gmeow:sliceTier` (the retired
/// `test_duplicate_iri_rejected` / `test_missing_tier_rejected`). Closes the
/// purrdf loader hole — `SliceCatalog::discover` keeps duplicate IRIs and
/// `ManifestView.tier` is silently `None`.
pub fn slice_discipline_findings(slices_dir: &Path) -> Result<Vec<Finding>> {
    let manifests = all_manifests(slices_dir);
    let mut loaded: Vec<(PathBuf, Dataset)> = Vec::with_capacity(manifests.len());
    for m in manifests {
        let ds = parse_ttl(&m)?;
        loaded.push((m, ds));
    }
    detect_slice_discipline(&loaded, slices_dir)
}

/// The pure slice-discipline logic over already-parsed manifests.
fn detect_slice_discipline(manifests: &[(PathBuf, Dataset)], root: &Path) -> Result<Vec<Finding>> {
    let mut iri_to_manifests: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut findings = Vec::new();
    for (path, ds) in manifests {
        let slices = ds
            .subjects_of_type(SLICE_CLASS)
            .map_err(|e| parse_err(path, &e.to_string()))?;
        for iri in slices {
            let tiers = ds
                .object_iris(&iri, SLICE_TIER)
                .map_err(|e| parse_err(path, &e.to_string()))?;
            if tiers.is_empty() {
                findings.push(finding(
                    Severity::Error,
                    codes::SLICE_DISCIPLINE_MISSING_TIER,
                    format!(
                        "gmeow:Slice {iri} in {file} declares no gmeow:sliceTier — tier is \
                         mandatory (tierCore / tierExtension / tierProfile)",
                        file = rel(path, root),
                    ),
                    Some(iri.clone()),
                ));
            }
            iri_to_manifests.entry(iri).or_default().push(path.clone());
        }
    }
    for (iri, mut manifests) in iri_to_manifests {
        if manifests.len() > 1 {
            manifests.sort();
            let list = manifests
                .iter()
                .map(|p| rel(p, root))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(finding(
                Severity::Error,
                codes::SLICE_DISCIPLINE_DUPLICATE_IRI,
                format!(
                    "slice IRI {iri} is declared by {n} manifests ({list}) — slice identity is \
                     manifest-only and must be unique",
                    n = manifests.len(),
                ),
                Some(iri),
            ));
        }
    }
    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds(ttl: &str) -> Dataset {
        let prefixes = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix dcterms: <http://purl.org/dc/terms/> .\n\
             @prefix ex: <https://example.org/> .\n";
        Dataset::parse_turtle(format!("{prefixes}{ttl}").as_bytes(), "test").unwrap()
    }

    #[test]
    fn shape_iri_collision_fires_when_one_iri_owns_two_files() {
        let a = ds("ex:PersonShape a sh:NodeShape .");
        let b = ds("ex:PersonShape a sh:NodeShape .\nex:OtherShape a sh:NodeShape .");
        let files = vec![
            (PathBuf::from("shapes/a.ttl"), a),
            (PathBuf::from("shapes/b.ttl"), b),
        ];
        let findings = detect_shape_collisions(&files, Path::new("")).unwrap();
        assert_eq!(findings.len(), 1, "exactly the colliding IRI is reported");
        assert_eq!(findings[0].code, codes::AUTHORING_SHAPE_IRI_COLLISION);
        assert!(findings[0].message.contains("PersonShape"));
        // The non-colliding OtherShape is not flagged.
        assert!(!findings[0].message.contains("OtherShape"));
    }

    #[test]
    fn shape_iri_collision_clean_when_every_iri_is_unique() {
        let a = ds("ex:AShape a sh:NodeShape .");
        let b = ds("ex:BShape a sh:NodeShape .");
        let files = vec![(PathBuf::from("a.ttl"), a), (PathBuf::from("b.ttl"), b)];
        assert!(
            detect_shape_collisions(&files, Path::new(""))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn graft_leak_fires_on_a_norms_iri_in_any_position() {
        // gmeow:Norm as object, gmeow:normIssuer as predicate.
        let d = ds("ex:x a gmeow:Norm ; gmeow:normIssuer ex:issuer .");
        let findings = detect_graft_leaks(&d, "slices/core/rights/module.ttl");
        let codes_seen: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
        assert!(
            findings.len() >= 2,
            "both Norm and normIssuer are flagged: {findings:?}"
        );
        assert!(codes_seen.iter().all(|c| *c == codes::AUTHORING_GRAFT_LEAK));
        assert!(findings.iter().any(|f| f.message.contains("/Norm")));
        assert!(findings.iter().any(|f| f.message.contains("/normIssuer")));
    }

    #[test]
    fn graft_leak_exact_identity_not_substring() {
        // A distinct term whose IRI has a norms term as a prefix must NOT match.
        let d = ds("ex:x <https://blackcatinformatics.ca/gmeow/normIssuerRole> ex:r .");
        assert!(
            detect_graft_leaks(&d, "m.ttl").is_empty(),
            "normIssuerRole must not match normIssuer by substring"
        );
    }

    #[test]
    fn slice_discipline_flags_missing_tier() {
        let d = ds("ex:s a gmeow:Slice ; rdfs:label \"S\"@x-gmeow-english .");
        let findings = detect_slice_discipline(
            &[(PathBuf::from("slices/g/s/manifest.ttl"), d)],
            Path::new(""),
        )
        .unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, codes::SLICE_DISCIPLINE_MISSING_TIER);
    }

    #[test]
    fn slice_discipline_flags_duplicate_iri() {
        let a = ds("ex:dup a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
        let b = ds("ex:dup a gmeow:Slice ; gmeow:sliceTier gmeow:tierExtension .");
        let findings = detect_slice_discipline(
            &[
                (PathBuf::from("slices/core/one/manifest.ttl"), a),
                (PathBuf::from("slices/extensions/two/manifest.ttl"), b),
            ],
            Path::new(""),
        )
        .unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, codes::SLICE_DISCIPLINE_DUPLICATE_IRI);
        assert!(findings[0].message.contains("dup"));
    }

    #[test]
    fn slice_discipline_clean_on_well_formed_unique_tiered_manifests() {
        let a = ds("ex:one a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
        let b = ds("ex:two a gmeow:Slice ; gmeow:sliceTier gmeow:tierExtension .");
        assert!(
            detect_slice_discipline(
                &[
                    (PathBuf::from("slices/core/one/manifest.ttl"), a),
                    (PathBuf::from("slices/extensions/two/manifest.ttl"), b),
                ],
                Path::new(""),
            )
            .unwrap()
            .is_empty()
        );
    }
}
