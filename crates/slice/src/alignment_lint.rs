// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native alignment-direction lint scaffolding (#936 Task 1).
//!
//! This module ports the input-loading and constant groundwork from
//! `src/gmeow_tools/alignment_lint.py` into `gmeow-slice`. The actual semantic
//! checks (inverse-direction, domain-range, property-character,
//! equivalence-collapse, DC refinement) are Tasks 2–4 and are intentionally left
//! as stubs here.
//!
//! The diagnostic carrier is the existing [`ProjectionDiagnostic`] from
//! [`crate::projection_lint`]; no new diagnostic struct is introduced.

#![allow(dead_code)] // scaffolding used by Tasks 2–4; remove as checks land

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;

use crate::error::SliceError;
use crate::fno_emit::collect_ontology_store;
use crate::projection_lint::ProjectionDiagnostic;

// ── Predicate / class constants (ported from alignment_lint.py) ───────────────

/// Predicate CURIEs whose alignment asserts (near-)equivalence for properties.
/// PUBLIC: the saturator may materialize cross-vocabulary triples only for these.
pub(crate) const STRONG_PROPERTY_PREDICATES: &[&str] =
    &["owl:equivalentProperty", "skos:exactMatch"];

/// Class-level strong equivalence (the collapse gate's edge set).
pub(crate) const STRONG_CLASS_PREDICATES: &[&str] = &["owl:equivalentClass", "skos:exactMatch"];

/// Intentionally directional/hierarchical predicates — exempt from direction checks.
pub(crate) const HIERARCHICAL_PREDICATES: &[&str] =
    &["skos:broadMatch", "skos:narrowMatch", "rdfs:subPropertyOf"];

/// Strength rank used to pick the canonical term in a self-contradicting pair.
pub(crate) const PREDICATE_RANK: &[(&str, i32)] = &[
    ("owl:equivalentProperty", 3),
    ("skos:exactMatch", 3),
    ("skos:closeMatch", 1),
];

/// OWL property-character types read from `rdf:type` assertions.
pub(crate) const CHARACTER_TYPES: &[&str] = &[
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
];

/// OWL property-typing terms. A target that uses none of these does not speak the
/// OWL characteristic vocabulary, so a character comparison would be noise.
pub(crate) const OWL_PROPERTY_TYPES: &[&str] = &[
    "http://www.w3.org/2002/07/owl#ObjectProperty",
    "http://www.w3.org/2002/07/owl#DatatypeProperty",
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
];

/// dcterms refinements → broader dcterms element (per DCMI specification).
pub(crate) const DCTERMS_REFINEMENTS: &[(&str, &str)] = &[
    // description refinements
    ("dcterms:abstract", "dcterms:description"),
    ("dcterms:tableOfContents", "dcterms:description"),
    // date refinements
    ("dcterms:created", "dcterms:date"),
    ("dcterms:modified", "dcterms:date"),
    ("dcterms:issued", "dcterms:date"),
    ("dcterms:valid", "dcterms:date"),
    ("dcterms:available", "dcterms:date"),
    ("dcterms:dateAccepted", "dcterms:date"),
    ("dcterms:dateCopyrighted", "dcterms:date"),
    ("dcterms:dateSubmitted", "dcterms:date"),
    // relation refinements
    ("dcterms:references", "dcterms:relation"),
    ("dcterms:isReferencedBy", "dcterms:relation"),
    ("dcterms:requires", "dcterms:relation"),
    ("dcterms:isRequiredBy", "dcterms:relation"),
    ("dcterms:replaces", "dcterms:relation"),
    ("dcterms:isReplacedBy", "dcterms:relation"),
    ("dcterms:hasPart", "dcterms:relation"),
    ("dcterms:isPartOf", "dcterms:relation"),
    ("dcterms:hasVersion", "dcterms:relation"),
    ("dcterms:isVersionOf", "dcterms:relation"),
    ("dcterms:conformsTo", "dcterms:relation"),
    // rights refinements
    ("dcterms:license", "dcterms:rights"),
    ("dcterms:rightsHolder", "dcterms:rights"),
    ("dcterms:accessRights", "dcterms:rights"),
    // coverage refinements
    ("dcterms:spatial", "dcterms:coverage"),
    ("dcterms:temporal", "dcterms:coverage"),
    // format refinements
    ("dcterms:extent", "dcterms:format"),
    ("dcterms:medium", "dcterms:format"),
    // identifier refinements
    ("dcterms:bibliographicCitation", "dcterms:identifier"),
];

/// Grandfathered hand-authored `dc:` alignments (existing before issue #60).
pub(crate) const GRANDFATHERED_DC: &[&str] = &["dc:rights"];

// ── Namespace constants ───────────────────────────────────────────────────────

const GMEOW_PREFIX: &str = "gmeow:";

// ── Native model ───────────────────────────────────────────────────────────────

/// One SSSOM mapping row — the subset the alignment-direction lint consumes.
/// Mirrors the Python `Mapping` dataclass (subject_id, predicate_id, object_id,
/// confidence, mapping_justification).
#[derive(Debug, Clone)]
pub(crate) struct Mapping {
    pub subject_id: String,
    pub predicate_id: String,
    pub object_id: String,
    pub confidence: String,
    pub mapping_justification: String,
}

// ── Public entry point ─────────────────────────────────────────────────────────

/// Lint SSSOM property mappings for inverse / mismatched target terms.
///
/// Task 1 scaffold: loads all inputs, emits informational findings for any target
/// vocabulary whose axioms are unavailable, and returns an empty semantic finding
/// list. The actual checks arrive in Tasks 2–4.
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source (the ontology,
/// SSSOM mapping tables) — no degraded fallback for required inputs.
pub(crate) fn lint_alignment_directions(
    root: &Path,
    allow_network: bool,
) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let _onto = collect_ontology_store(root)?;
    let mappings = load_sssom_mappings(root)?;

    // Collect referenced target prefixes from the object side of mappings.
    let referenced: BTreeSet<String> = mappings
        .iter()
        .filter(|m| m.subject_id.starts_with(GMEOW_PREFIX))
        .filter_map(|m| prefix_of(&m.object_id))
        .collect();

    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();

    if allow_network {
        // TODO(#936): implement network fetch for --network.
        findings.push(ProjectionDiagnostic {
            severity: "INFO".to_owned(),
            check: "domain-range".to_owned(),
            code: "domain-range".to_owned(),
            message: "network fetch for target axioms is not yet implemented (#936)".to_owned(),
            instance: None,
        });
    }

    let target_graphs = load_target_axiom_stores(root, &referenced)?;

    // Emit an INFO finding for every referenced prefix with no axioms available.
    for prefix in &referenced {
        if !target_graphs.contains_key(prefix) {
            findings.push(info_unavailable(prefix));
        }
    }

    // Tasks 2–4 will run the actual checks here and append to `findings`.
    let _ = target_graphs; // silence unused warning until checks land

    Ok(findings)
}

// ── Target-axiom loading ───────────────────────────────────────────────────────

/// Load the axiom graph for each referenced target prefix.
///
/// Returns a map from prefix to its merged store (snapshot + fixture). Prefixes
/// with no available axioms are omitted; callers emit INFO findings for those.
fn load_target_axiom_stores(
    root: &Path,
    prefixes: &BTreeSet<String>,
) -> Result<BTreeMap<String, Store>, SliceError> {
    let mut out: BTreeMap<String, Store> = BTreeMap::new();
    for prefix in prefixes {
        let mut store = new_store()?;
        let mut has_axioms = false;

        if let Some(snapshot) = load_target_snapshot(root, prefix)? {
            merge_store(&mut store, &snapshot)?;
            has_axioms = true;
        }
        if let Some(fixture) = load_fixture(root, prefix)? {
            merge_store(&mut store, &fixture)?;
            has_axioms = true;
        }

        if has_axioms {
            out.insert(prefix.clone(), store);
        }
    }
    Ok(out)
}

/// Load a vendored target axiom snapshot from `imports/targets/<prefix>.ttl`, if
/// it exists.
fn load_target_snapshot(root: &Path, prefix: &str) -> Result<Option<Store>, SliceError> {
    let path = root
        .join("imports")
        .join("targets")
        .join(format!("{prefix}.ttl"));
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(parse_ttl(&path)?))
}

/// Load a hand-authored target fixture from `tests/fixtures/target_axioms/<prefix>.ttl`,
/// if it exists.
fn load_fixture(root: &Path, prefix: &str) -> Result<Option<Store>, SliceError> {
    let path = root
        .join("tests")
        .join("fixtures")
        .join("target_axioms")
        .join(format!("{prefix}.ttl"));
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(parse_ttl(&path)?))
}

// ── SSSOM mapping loading ──────────────────────────────────────────────────────

/// Load all SSSOM mapping rows from `generated/mappings/*.sssom.tsv`.
///
/// Mirrors Python `load_mappings(MAPPINGS_DIR)`, reading the committed generated
/// SSSOM tables. Comment/header lines starting with `#` are skipped; the first
/// non-comment line is the TSV header.
fn load_sssom_mappings(root: &Path) -> Result<Vec<Mapping>, SliceError> {
    let mappings_dir = root.join("generated").join("mappings");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    if mappings_dir.is_dir() {
        for entry in std::fs::read_dir(&mappings_dir).map_err(SliceError::Io)? {
            let entry = entry.map_err(SliceError::Io)?;
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".sssom.tsv"))
            {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut mappings: Vec<Mapping> = Vec::new();
    for path in &files {
        mappings.extend(parse_sssom_tsv(path)?);
    }
    Ok(mappings)
}

/// Parse one SSSOM TSV file into [`Mapping`] rows.
fn parse_sssom_tsv(path: &Path) -> Result<Vec<Mapping>, SliceError> {
    let text = std::fs::read_to_string(path).map_err(SliceError::Io)?;
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    if lines.is_empty() {
        return Ok(Vec::new());
    }

    let header = lines.remove(0);
    let columns: Vec<&str> = header.split('\t').collect();
    let idx = |name: &str| columns.iter().position(|c| *c == name);

    let subject_idx = idx("subject_id").ok_or_else(|| {
        SliceError::Parse(format!("{} missing subject_id column", path.display()))
    })?;
    let predicate_idx = idx("predicate_id").ok_or_else(|| {
        SliceError::Parse(format!("{} missing predicate_id column", path.display()))
    })?;
    let object_idx = idx("object_id")
        .ok_or_else(|| SliceError::Parse(format!("{} missing object_id column", path.display())))?;
    let justification_idx = idx("mapping_justification");
    let confidence_idx = idx("confidence");

    let mut rows: Vec<Mapping> = Vec::new();
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        let get = |i: usize| cells.get(i).unwrap_or(&"").to_string();
        rows.push(Mapping {
            subject_id: get(subject_idx),
            predicate_id: get(predicate_idx),
            object_id: get(object_idx),
            confidence: confidence_idx.map(get).unwrap_or_default(),
            mapping_justification: justification_idx.map(get).unwrap_or_default(),
        });
    }
    Ok(rows)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Return the CURIE prefix of `curie` if it has one.
fn prefix_of(curie: &str) -> Option<String> {
    curie.split_once(':').map(|(prefix, _)| prefix.to_owned())
}

/// Build an INFO diagnostic for a target prefix whose axioms are unavailable.
fn info_unavailable(prefix: &str) -> ProjectionDiagnostic {
    ProjectionDiagnostic {
        severity: "INFO".to_owned(),
        check: "domain-range".to_owned(),
        code: "domain-range".to_owned(),
        message: format!(
            "skipped — no axioms available for target {prefix:?} \
             (vendor a snapshot or run with --network)"
        ),
        instance: None,
    }
}

fn new_store() -> Result<Store, SliceError> {
    Store::new().map_err(|e| SliceError::Parse(format!("store creation failed: {e}")))
}

/// Parse a Turtle file into a fresh oxigraph store (lenient, so GMEOW's
/// `@x-gmeow-*` language tags parse).
fn parse_ttl(path: &Path) -> Result<Store, SliceError> {
    let store = new_store()?;
    let bytes = std::fs::read(path).map_err(SliceError::Io)?;
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(bytes.as_slice())
    {
        let quad = quad
            .map_err(|e| SliceError::Parse(format!("syntax error in {}: {e}", path.display())))?;
        store
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(store)
}

/// Merge every quad from `source` into `target`.
fn merge_store(target: &mut Store, source: &Store) -> Result<(), SliceError> {
    for quad in source.iter() {
        let quad = quad.map_err(|e| SliceError::Parse(format!("store iteration failed: {e}")))?;
        target
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_property_predicates_contain_expected_curies() {
        assert!(STRONG_PROPERTY_PREDICATES.contains(&"owl:equivalentProperty"));
        assert!(STRONG_PROPERTY_PREDICATES.contains(&"skos:exactMatch"));
    }

    #[test]
    fn strong_class_predicates_contain_expected_curies() {
        assert!(STRONG_CLASS_PREDICATES.contains(&"owl:equivalentClass"));
        assert!(STRONG_CLASS_PREDICATES.contains(&"skos:exactMatch"));
    }

    #[test]
    fn hierarchical_predicates_and_ranks_are_present() {
        assert!(HIERARCHICAL_PREDICATES.contains(&"skos:broadMatch"));
        assert!(HIERARCHICAL_PREDICATES.contains(&"skos:narrowMatch"));
        assert!(HIERARCHICAL_PREDICATES.contains(&"rdfs:subPropertyOf"));
        assert!(PREDICATE_RANK
            .iter()
            .any(|(p, _)| *p == "owl:equivalentProperty"));
        assert!(PREDICATE_RANK.iter().any(|(p, _)| *p == "skos:exactMatch"));
        assert!(PREDICATE_RANK.iter().any(|(p, _)| *p == "skos:closeMatch"));
    }

    #[test]
    fn character_and_owl_property_types_are_present() {
        assert!(CHARACTER_TYPES.contains(&"http://www.w3.org/2002/07/owl#FunctionalProperty"));
        assert!(CHARACTER_TYPES.contains(&"http://www.w3.org/2002/07/owl#TransitiveProperty"));
        assert!(OWL_PROPERTY_TYPES.contains(&"http://www.w3.org/2002/07/owl#ObjectProperty"));
        assert!(OWL_PROPERTY_TYPES.contains(&"http://www.w3.org/2002/07/owl#AsymmetricProperty"));
    }

    #[test]
    fn dcterms_refinements_and_grandfathered_dc_are_present() {
        assert!(DCTERMS_REFINEMENTS
            .iter()
            .any(|(r, b)| *r == "dcterms:abstract" && *b == "dcterms:description"));
        assert!(GRANDFATHERED_DC.contains(&"dc:rights"));
    }

    /// Target snapshot loading succeeds for at least one vendored target and
    /// produces a non-empty store.
    #[test]
    fn target_snapshot_loads_with_content() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let store = load_target_snapshot(root, "org")
            .expect("loading org snapshot should not fail")
            .expect("org snapshot should exist");
        let len = store.len().expect("store length should be readable");
        assert!(len > 0, "org snapshot should contain triples");
    }

    /// SSSOM mapping loading returns rows from the committed generated tables.
    #[test]
    fn sssom_mapping_loading_returns_rows() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let mappings = load_sssom_mappings(root).expect("loading mappings should not fail");
        assert!(
            !mappings.is_empty(),
            "expected at least one SSSOM mapping row"
        );
        assert!(mappings.iter().any(|m| m.subject_id.starts_with("gmeow:")));
    }

    /// Missing target snapshots produce INFO findings, not errors or panics.
    #[test]
    fn missing_target_snapshot_produces_info_finding() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let findings = lint_alignment_directions(root, false)
            .expect("lint_alignment_directions should not error");
        let info: Vec<_> = findings.iter().filter(|f| f.severity == "INFO").collect();
        // Some referenced targets (e.g. schema when offline? fhir? others?) lack
        // vendored snapshots/fixtures, so at least one INFO is expected.
        assert!(
            !info.is_empty(),
            "expected at least one INFO finding for unavailable targets"
        );
    }
}
