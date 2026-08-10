// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Coverage graph-analysis over the vendored entity-slice fixtures.
//!
//! Native throughout: it loads the merged fixture graphs into one
//! [`purrdf::RdfDataset`], collects every distinct class
//! (`rdf:type` object) and predicate IRI, then classifies each as *covered* or a
//! *gap*. The SSSOM-derived `aligned` set (every external IRI GMEOW links to) is
//! computed by the native mapping evaluator and passed in.
//!
//! The classification rule is deterministic:
//!
//! * **ignored** — the IRI sits in the RDF / RDFS / OWL / XSD plumbing namespaces;
//!   it is neither covered nor a gap.
//! * **covered** — the IRI is in the GMEOW namespace, in a *recommended* namespace
//!   GMEOW reuses wholesale (SKOS), or in the `aligned` SSSOM set.
//! * **gap** — used in the slice but not covered.
//!
//! Classes additionally require the IRI to start with `http`; predicates do not
//! because a predicate is already necessarily an IRI.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gmeow_errors::{Finding, Location, Report, Severity};
use purrdf::{DatasetView, GraphMatch, TermRef, TermValue};

use crate::model::rdf;
use crate::store;

/// The four sorted coverage sets, ready to hand back across the FFI boundary.
///
/// Each field is a [`BTreeSet`] so iteration order is the sorted order the Python
/// `CoverageReport` consumers (and the parity golden) expect.
#[derive(Debug, Default, Clone)]
pub struct CoverageSets {
    /// Used classes GMEOW covers.
    pub covered_classes: BTreeSet<String>,
    /// Used classes GMEOW does not yet align to.
    pub gap_classes: BTreeSet<String>,
    /// Used predicates GMEOW covers.
    pub covered_predicates: BTreeSet<String>,
    /// Used predicates GMEOW does not yet align to.
    pub gap_predicates: BTreeSet<String>,
}

/// RDF-plumbing namespaces that are neither covered nor counted as gaps.
///
/// These are the literal prefix strings from `config.PREFIXES` (rdf / rdfs / owl
/// / xsd) — the Python `_IGNORED` tuple.
const IGNORED: [&str; 4] = [
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "http://www.w3.org/2000/01/rdf-schema#",
    "http://www.w3.org/2002/07/owl#",
    "http://www.w3.org/2001/XMLSchema#",
];

/// Namespaces GMEOW reuses wholesale (recommended value vocabularies) — the
/// Python `_RECOMMENDED` tuple (SKOS).
const RECOMMENDED: [&str; 1] = ["http://www.w3.org/2004/02/skos/core#"];

/// GMEOW's own grounding-layer namespaces (`lang:`, `logic:`, `math:`). Like the
/// primary `gmeow:` namespace, a term here is authored in-repo and is covered by
/// authorship, not an external identifier requiring an SSSOM alignment — the
/// coverage gate ensures every USED term is either GMEOW-native or aligned, and a
/// grounding-layer term is GMEOW-native exactly as a `gmeow:` term is. (Forcing an
/// external alignment onto a grounding predicate to satisfy the gate would be
/// fabricated linkage, not real coverage.)
const GMEOW_GROUNDING: [&str; 3] = [
    "https://blackcatinformatics.ca/lang/",
    "https://blackcatinformatics.ca/logic/",
    "https://blackcatinformatics.ca/math/",
];

/// Mirror of `coverage._is_ignored`.
fn is_ignored(iri: &str) -> bool {
    IGNORED.iter().any(|ns| iri.starts_with(ns))
}

/// Mirror of `coverage._is_covered`.
fn is_covered(iri: &str, aligned: &BTreeSet<String>, namespace: &str) -> bool {
    iri.starts_with(namespace)
        || GMEOW_GROUNDING.iter().any(|ns| iri.starts_with(ns))
        || RECOMMENDED.iter().any(|ns| iri.starts_with(ns))
        || aligned.contains(iri)
}

/// Classify the classes and predicates used across the fixture graphs.
///
/// Mirrors `coverage.analyze` over `coverage.load_fixtures`: builds one merged
/// store from `fixture_paths`, collects the distinct `rdf:type` objects (used
/// classes) and distinct predicates (used predicates), and routes each IRI into
/// the covered / gap / ignored buckets.
///
/// `aligned` is the SSSOM-derived external-IRI set (`coverage.covered_iris()`),
/// computed in Python; `namespace` is `config.NAMESPACE`.
///
/// # Errors
///
/// Fails if any fixture fails to read or parse.
pub fn coverage_analyze(
    fixture_paths: &[PathBuf],
    aligned: &BTreeSet<String>,
    namespace: &str,
) -> gmeow_errors::Result<CoverageSets> {
    let ds = store::dataset_from_paths(fixture_paths)?;
    let mut sets = CoverageSets::default();

    // Used classes: the distinct named-node objects of rdf:type.
    let mut classes: BTreeSet<String> = BTreeSet::new();
    if let Some(type_id) = ds.term_id_by_value(&TermValue::iri(rdf::TYPE)) {
        for q in ds.quads_for_pattern(None, Some(type_id), None, GraphMatch::Any) {
            if let TermRef::Iri(n) = ds.resolve(q.o) {
                classes.insert(n.to_owned());
            }
        }
    }
    for iri in classes {
        if !iri.starts_with("http") || is_ignored(&iri) {
            continue;
        }
        if is_covered(&iri, aligned, namespace) {
            sets.covered_classes.insert(iri);
        } else {
            sets.gap_classes.insert(iri);
        }
    }

    // Used predicates: every distinct predicate IRI in the merged graph.
    let mut predicates: BTreeSet<String> = BTreeSet::new();
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let TermRef::Iri(p) = ds.resolve(q.p) {
            predicates.insert(p.to_owned());
        }
    }
    for iri in predicates {
        if is_ignored(&iri) {
            continue;
        }
        if is_covered(&iri, aligned, namespace) {
            sets.covered_predicates.insert(iri);
        } else {
            sets.gap_predicates.insert(iri);
        }
    }

    Ok(sets)
}

/// Coverage outcome over the fixture graphs (mirrors `coverage.CoverageReport`).
///
/// The four sorted sets are the classified used-classes / used-predicates; the
/// two coverage fractions are `covered / (covered + gap)`, or `1.0` when the
/// total is zero (matching the Python `class_coverage` / `predicate_coverage`
/// properties).
#[derive(Debug, Default, Clone)]
pub struct CoverageReport {
    /// Used classes GMEOW covers.
    pub covered_classes: BTreeSet<String>,
    /// Used classes GMEOW does not yet align to.
    pub gap_classes: BTreeSet<String>,
    /// Used predicates GMEOW covers.
    pub covered_predicates: BTreeSet<String>,
    /// Used predicates GMEOW does not yet align to.
    pub gap_predicates: BTreeSet<String>,
}

impl CoverageReport {
    /// Fraction of used classes that are covered (`0..=1`; `1.0` if none used).
    #[must_use]
    pub fn class_coverage(&self) -> f64 {
        let total = self.covered_classes.len() + self.gap_classes.len();
        if total == 0 {
            1.0
        } else {
            self.covered_classes.len() as f64 / total as f64
        }
    }

    /// Fraction of used predicates that are covered (`0..=1`; `1.0` if none).
    #[must_use]
    pub fn predicate_coverage(&self) -> f64 {
        let total = self.covered_predicates.len() + self.gap_predicates.len();
        if total == 0 {
            1.0
        } else {
            self.covered_predicates.len() as f64 / total as f64
        }
    }
}

/// Discover every vendored coverage fixture under `fixtures_dir`, sorted.
///
/// Mirrors `coverage.fixture_paths` (`sorted(fixtures_dir.rglob("*.ttl"))`): it
/// recurses into subdirectories (e.g. `external/`) so the real-world site
/// snapshots are part of the measurement.
fn fixture_paths(fixtures_dir: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut stack = vec![fixtures_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                detail: format!("failed to read fixtures dir {}: {e}", dir.display()),
            })
        })? {
            let entry = entry.map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Io {
                    detail: format!("failed to read fixtures dir entry: {e}"),
                })
            })?;
            // Use the dirent file type (no stat, no symlink follow) so a circular symlink
            // can't drive the walk into an infinite loop.
            let is_dir = entry
                .file_type()
                .map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Io {
                        detail: format!("failed to read fixtures dir entry type: {e}"),
                    })
                })?
                .is_dir();
            let path = entry.path();
            if is_dir {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "ttl")
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

/// Run the coverage analysis over the vendored fixtures (mirrors
/// `coverage.run_coverage`).
///
/// Computes the SSSOM-derived `aligned` set from `mappings_dir`
/// (`mapping_eval::aligned_iris`), discovers every `*.ttl` under `fixtures_dir`
/// (recursive, sorted), classifies them via [`coverage_analyze`], and wraps the
/// result into a [`CoverageReport`].
///
/// # Errors
///
/// Fails if the mappings dir or any fixture cannot be read or parsed.
pub fn run_coverage(
    fixtures_dir: &Path,
    mappings_dir: &Path,
    namespace: &str,
) -> gmeow_errors::Result<CoverageReport> {
    let aligned = crate::mapping_eval::aligned_iris(mappings_dir)?;
    let paths = fixture_paths(fixtures_dir)?;
    let sets = coverage_analyze(&paths, &aligned, namespace)?;
    Ok(CoverageReport {
        covered_classes: sets.covered_classes,
        gap_classes: sets.gap_classes,
        covered_predicates: sets.covered_predicates,
        gap_predicates: sets.gap_predicates,
    })
}

/// Project a coverage report into the canonical diagnostics `Report` (mirrors
/// `coverage.to_diagnostics_report`).
///
/// Coverage gaps are *informational* — an external IRI a fixture uses that GMEOW
/// does not yet align to — so every gap rides as an `info` finding (the report
/// stays `ok`). The gap IRI is the finding's `logical` so SARIF/HTML consumers
/// can group by term.
#[must_use]
pub fn coverage_to_diagnostics(report: &CoverageReport) -> Report {
    const TOOL: &str = "coverage";
    let gap_finding = |iri: &str, code: &str, kind: &str| -> Finding {
        let message = format!("{kind} used by a fixture but not aligned: {iri}");
        let mut f = Finding::new(Severity::Info, code, message).with_tool(TOOL);
        f.add_location(Location::new(None, None, None, Some(iri.to_owned())));
        f
    };

    let mut out = Report::new(TOOL);
    for iri in &report.gap_classes {
        out.add_finding(gap_finding(iri, crate::codes::COVERAGE_GAP_CLASS, "class"));
    }
    for iri in &report.gap_predicates {
        out.add_finding(gap_finding(
            iri,
            crate::codes::COVERAGE_GAP_PREDICATE,
            "predicate",
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    /// Write `contents` to `name` inside a fresh RAII temp directory.
    ///
    /// The returned [`tempfile::TempDir`] owns the directory: it is removed on
    /// drop, including on panic and early return. Bind it to a named `_tmp`
    /// (never a bare `_`, which would drop it immediately) so it outlives the
    /// path. The file *name* is preserved because the coverage walk dispatches
    /// on the `.ttl` extension.
    fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[test]
    fn classifies_covered_gap_and_ignored() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_coverage_basic.ttl",
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:thing a gmeow:Email ;\n\
                 gmeow:addressValue \"x\" ;\n\
                 foaf:homepage ex:hp ;\n\
                 ex:unaligned ex:y ;\n\
                 owl:sameAs ex:z .\n\
             ex:other a foaf:Person, skos:Concept, owl:Class .\n",
        );
        let mut aligned = BTreeSet::new();
        aligned.insert("http://xmlns.com/foaf/0.1/Person".to_owned());
        aligned.insert("http://xmlns.com/foaf/0.1/homepage".to_owned());
        let sets = coverage_analyze(std::slice::from_ref(&path), &aligned, NS).unwrap();

        // Classes: gmeow:Email + foaf:Person covered; skos:Concept covered
        // (recommended); owl:Class ignored; no gap classes here.
        assert!(sets.covered_classes.contains(&format!("{NS}Email")));
        assert!(
            sets.covered_classes
                .contains("http://xmlns.com/foaf/0.1/Person")
        );
        assert!(
            sets.covered_classes
                .contains("http://www.w3.org/2004/02/skos/core#Concept")
        );
        assert!(
            !sets
                .covered_classes
                .iter()
                .any(|c| c.starts_with("http://www.w3.org/2002/07/owl#"))
        );
        assert!(sets.gap_classes.is_empty());

        // Predicates: gmeow:addressValue + foaf:homepage covered; ex:unaligned
        // gap; owl:sameAs + rdf:type ignored.
        assert!(
            sets.covered_predicates
                .contains(&format!("{NS}addressValue"))
        );
        assert!(
            sets.covered_predicates
                .contains("http://xmlns.com/foaf/0.1/homepage")
        );
        assert!(
            sets.gap_predicates
                .contains("https://example.org/unaligned")
        );
        assert!(
            !sets
                .covered_predicates
                .iter()
                .any(|p| p.starts_with("http://www.w3.org/2002/07/owl#"))
        );
        assert!(!sets.gap_predicates.iter().any(|p| p.contains("sameAs")));
    }

    #[test]
    fn to_diagnostics_emits_info_gaps_and_stays_ok() {
        // One covered + one gap each; only the gaps surface as info findings.
        let mut report = CoverageReport::default();
        report
            .covered_classes
            .insert("http://xmlns.com/foaf/0.1/Person".to_owned());
        report
            .gap_classes
            .insert("https://example.org/UnalignedClass".to_owned());
        report
            .covered_predicates
            .insert(format!("{NS}addressValue"));
        report
            .gap_predicates
            .insert("https://example.org/unalignedPredicate".to_owned());

        let diag = coverage_to_diagnostics(&report);
        assert_eq!(diag.tool, "coverage");
        assert!(diag.ok(), "info-only report must stay ok");
        assert_eq!(diag.error_count(), 0);
        assert_eq!(diag.warning_count(), 0);
        assert_eq!(diag.findings.len(), 2);
        assert!(diag.findings.iter().all(|f| f.severity == Severity::Info));
        let codes: BTreeSet<&str> = diag.findings.iter().map(|f| f.code.as_str()).collect();
        let expected: BTreeSet<&str> = ["coverage.gap-class", "coverage.gap-predicate"]
            .into_iter()
            .collect();
        assert_eq!(codes, expected);
        // The gap IRI rides as the finding's logical location.
        let class_finding = diag
            .findings
            .iter()
            .find(|f| f.code == "coverage.gap-class")
            .unwrap();
        assert_eq!(
            class_finding.locations[0].logical.as_deref(),
            Some("https://example.org/UnalignedClass")
        );
    }

    #[test]
    fn run_coverage_over_real_fixtures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let fixtures = root.join("tests").join("fixtures").join("coverage");
        let mappings = root.join("generated").join("mappings");
        let report = run_coverage(&fixtures, &mappings, NS).unwrap();

        // Covered classes: GMEOW-aligned externals + a GMEOW-native class.
        for iri in [
            "http://xmlns.com/foaf/0.1/Person",
            "https://schema.org/Person",
            "https://schema.org/Organization",
            &format!("{NS}EmailMessage"),
        ] {
            assert!(
                report.covered_classes.contains(iri),
                "expected covered class {iri}; got {:?}",
                report.covered_classes
            );
        }

        // Covered predicates: a GMEOW-native predicate + SSSOM-aligned externals
        // (these pin the aligned_iris SSSOM walk end-to-end).
        for iri in [
            format!("{NS}addressValue"),
            "https://schema.org/description".to_owned(),
            "https://schema.org/url".to_owned(),
            "http://xmlns.com/foaf/0.1/homepage".to_owned(),
        ] {
            assert!(
                report.covered_predicates.contains(&iri),
                "expected covered predicate {iri}; got {:?}",
                report.covered_predicates
            );
        }

        // The slice is intentionally partial, so there are real gaps.
        assert!(!report.gap_classes.is_empty());
        let cc = report.class_coverage();
        assert!(cc > 0.0 && cc <= 1.0, "class_coverage out of range: {cc}");

        // Covered and gap sets are disjoint in both dimensions.
        assert!(
            report
                .covered_classes
                .intersection(&report.gap_classes)
                .next()
                .is_none()
        );
        assert!(
            report
                .covered_predicates
                .intersection(&report.gap_predicates)
                .next()
                .is_none()
        );
    }
}
