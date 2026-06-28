// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native cross-layer consistency lint for the projection stack.
//!
//! The alignment stack represents the same mappings four ways — SSSOM (1:1 term
//! links), EDOAL (complex cells), FnO (the transform functions), and SPARQL
//! CONSTRUCT (the executors) — plus the ontology. The EDOAL and SPARQL dialects now
//! lower from one shared get-leg model, so the historical CONSTRUCT↔EDOAL↔SSSOM
//! drift is gone by construction; the two remaining cross-layer invariants surface
//! as canonical diagnostics (`mapping-compile.fno-type` / `.fno-ref`) folded into the
//! dev-gate report (the SARIF/JSON/HTML + `gmeow.gts` projections).
//!
//! The two checks (mirroring the retired Python, message wording preserved):
//!
//! * [`fno_type_mismatches`] — an `fno:Parameter`/`fno:Output` whose `fno:predicate`
//!   is a GMEOW property with a declared `rdfs:range` must declare an `fno:type` equal
//!   to that range.
//! * [`fno_reference_integrity`] — every FnO function an EDOAL cell invokes via
//!   `edoal:transformation` must be a defined `fno:Function`.
//!
//! ## Inputs — the committed `generated/` tree
//!
//! The lint reads the **committed** rendered artifacts under `root`
//! (`generated/projections/*.{fno.ttl,edoal.ttl}`). The finding is over the *shipped*
//! surface (`gmeow.gts`). The ontology `rdfs:range`s come from the shared
//! [`crate::mapping_support::collect_ontology_store`] (one source of truth with the
//! correspondence lowerings).
//!
//! ## Why this lives in `gmeow-slice`
//!
//! `gmeow-slice` is the one crate that owns the ontology-merge machinery the lint
//! reuses; `gmeow-rdf-core` is oxigraph-free and cannot host an oxigraph-parsing,
//! ontology-reading consistency check.

use std::collections::BTreeSet;
use std::path::Path;

use oxigraph::model::{GraphNameRef, NamedNode, NamedOrBlankNode, Term};
use oxigraph::store::Store;

use crate::error::SliceError;
use crate::mapping_support::{collect_ontology_store, predicate_ranges};

// ── Namespace constants ───────────────────────────────────────────────────────

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";

const FNO_PARAMETER: &str = "https://w3id.org/function/ontology#Parameter";
const FNO_OUTPUT: &str = "https://w3id.org/function/ontology#Output";
const FNO_FUNCTION: &str = "https://w3id.org/function/ontology#Function";
const FNO_PREDICATE: &str = "https://w3id.org/function/ontology#predicate";
const FNO_TYPE: &str = "https://w3id.org/function/ontology#type";

const ALIGN_CELL: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#Cell";
const EDOAL_TRANSFORMATION: &str = "http://ns.inria.org/edoal/1.0/#transformation";

/// The FnO catalog files (projection transforms + the language conversion catalog),
/// mirroring `projection_lint._FNO_FILES`.
const FNO_FUNCTIONS_FILE: &str = "functions.fno.ttl";
const FNO_TRANSFORMS_FILE: &str = "transforms.fno.ttl";

// ── Diagnostic carrier ───────────────────────────────────────────────────────

/// One projection-lint problem. The `check`/`instance` convention mirrors the native
/// SSSOM validator's diagnostic dict (`gmeow_rdf.validate_sssom`) so the PyO3 binding
/// packs both into the same `{severity, code, message, check, instance}` shape the
/// Python finding leg already consumes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectionDiagnostic {
    /// Severity token: `"ERROR"`, `"WARNING"`, or `"INFO"`.
    pub severity: String,
    /// The drift family: `fno-type` or `fno-ref`. The finding leg maps this to the
    /// canonical code `mapping-compile.<check>`.
    pub check: String,
    /// A stable per-check code (same value as `check`); carried for dict parity with
    /// the SSSOM validator's `code` slot.
    pub code: String,
    /// The human-readable problem, verbatim from the retired Python lint.
    pub message: String,
    /// The most-specific RDF node the problem concerns (the FnO param/output IRI, the
    /// undefined function IRI, or the drifting target term), or `None`.
    pub instance: Option<String>,
    /// For alignment-direction findings, the SSSOM row CURIEs that the finding is
    /// about. These are `None` for projection-stack findings (`fno-type`, `fno-ref`).
    pub subject_id: Option<String>,
    pub predicate_id: Option<String>,
    pub object_id: Option<String>,
}

impl ProjectionDiagnostic {
    fn error(check: &str, message: String, instance: Option<String>) -> Self {
        Self {
            severity: "ERROR".to_owned(),
            check: check.to_owned(),
            code: check.to_owned(),
            message,
            instance,
            subject_id: None,
            predicate_id: None,
            object_id: None,
        }
    }

    /// Severity-first ordering used for stable, deterministic lint output:
    /// ERROR < WARNING < INFO < everything else, then check, then instance.
    pub fn cmp_severity_check_instance(&self, other: &Self) -> std::cmp::Ordering {
        let order = |s: &str| match s {
            "ERROR" => 0,
            "WARNING" => 1,
            "INFO" => 2,
            _ => 3,
        };
        order(&self.severity)
            .cmp(&order(&other.severity))
            .then_with(|| self.check.cmp(&other.check))
            .then_with(|| self.instance.cmp(&other.instance))
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Run the projection-lint invariants plus the alignment-direction lint against the
/// committed `generated/` tree under `root`, returning every problem as a
/// [`ProjectionDiagnostic`].
///
/// An empty result means the projection stack and SSSOM alignments are internally
/// consistent. Projection checks run first (`fno-type` → `fno-ref`), then alignment
/// checks (`inverse-direction`, `domain-range`, `property-character`,
/// `equivalence-collapse`, `dc-refinement`, `dc-hand-authored`). The combined list is
/// sorted deterministically by severity → check → instance.
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source (a committed
/// artifact, the ontology, an SSSOM source) — no degraded fallback (CONSTITUTION /
/// no-compromises).
pub fn lint_projection(
    root: &Path,
    allow_network: bool,
) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let projections = root.join("generated").join("projections");

    let onto = collect_ontology_store(root)?;
    let fno = fno_catalog_store(root, &projections)?;

    let mut out: Vec<ProjectionDiagnostic> = Vec::new();
    out.extend(fno_type_mismatches(&onto, &fno)?);
    out.extend(fno_reference_integrity(&fno, &projections)?);
    out.extend(crate::alignment_lint::lint_alignment_directions(
        root,
        allow_network,
    )?);

    out.sort_by(ProjectionDiagnostic::cmp_severity_check_instance);
    Ok(out)
}

// ── Check 1: fno:type ↔ rdfs:range ─────────────────────────────────────────────

/// FnO param/output `fno:type`s that disagree with their predicate's `rdfs:range`
/// (mirrors `projection_lint.fno_type_mismatches`).
fn fno_type_mismatches(onto: &Store, fno: &Store) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let mut params: BTreeSet<String> = BTreeSet::new();
    params.extend(subjects_of_type(fno, FNO_PARAMETER)?);
    params.extend(subjects_of_type(fno, FNO_OUTPUT)?);

    let mut problems: Vec<ProjectionDiagnostic> = Vec::new();
    for param in &params {
        let subject = NamedNode::new(param)
            .map_err(|e| SliceError::Parse(format!("invalid FnO node IRI {param}: {e}")))?;
        let Some(predicate) = first_object_iri(fno, &subject, FNO_PREDICATE)? else {
            continue; // not a URIRef predicate — skip (mirrors the isinstance guard)
        };
        let Some(ftype) = first_object_iri(fno, &subject, FNO_TYPE)? else {
            continue; // no fno:type declared — skip
        };
        // The ontology range set; an external/projected predicate has none → skip.
        let mut ranges: Vec<String> = predicate_ranges(onto, &predicate)?;
        if ranges.is_empty() {
            continue;
        }
        ranges.sort();
        ranges.dedup();
        if !ranges.contains(&ftype) {
            problems.push(ProjectionDiagnostic::error(
                "fno-type",
                format!(
                    "{param}: predicate {predicate} has range {} but fno:type is {ftype}",
                    py_list_repr(&ranges)
                ),
                Some(param.clone()),
            ));
        }
    }
    Ok(problems)
}

// ── Check 2: EDOAL → FnO reference integrity ───────────────────────────────────

/// EDOAL `edoal:transformation` references to undefined FnO functions (mirrors
/// `projection_lint.fno_reference_integrity`).
fn fno_reference_integrity(
    fno: &Store,
    projections: &Path,
) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let defined: BTreeSet<String> = subjects_of_type(fno, FNO_FUNCTION)?.into_iter().collect();
    let mut problems: Vec<ProjectionDiagnostic> = Vec::new();

    for path in edoal_files(projections)? {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let store = parse_ttl(&path)?;
        for cell in subject_terms_of_type(&store, ALIGN_CELL)? {
            for trans in objects_of_term(&store, &cell, EDOAL_TRANSFORMATION)? {
                for refr in objects_of_term(&store, &trans, RDFS_SEE_ALSO)? {
                    let Term::NamedNode(nn) = &refr else {
                        continue;
                    };
                    let iri = nn.as_str();
                    // Last path segment — split on `/` OR `#`, matching the FnO IRI
                    // local-name convention. A future-proofing superset of the retired
                    // Python `/`-only split: no behaviour change on any current FnO IRI
                    // (all `…/fn…`), but a `#fn…` function IRI is extracted correctly.
                    let local = iri.rsplit(['/', '#']).next().unwrap_or(iri);
                    if local.starts_with("fn") && !defined.contains(iri) {
                        problems.push(ProjectionDiagnostic::error(
                            "fno-ref",
                            format!("{name}: undefined FnO function {iri}"),
                            Some(iri.to_owned()),
                        ));
                    }
                }
            }
        }
    }
    Ok(problems)
}

// ── Source loading ─────────────────────────────────────────────────────────────

/// The merged FnO catalog store: `functions.fno.ttl` + `transforms.fno.ttl`. The
/// transforms catalog is hand-authored in the DSL tree, so it falls back to
/// `dsl/mappings/transforms.fno.ttl` when absent from `projections` (mirrors
/// `projection_lint._fno_graph`'s fallback + `_run_invariants`' staging copy).
fn fno_catalog_store(root: &Path, projections: &Path) -> Result<Store, SliceError> {
    let store = new_store()?;
    load_ttl_into(&store, &projections.join(FNO_FUNCTIONS_FILE))?;

    let transforms = projections.join(FNO_TRANSFORMS_FILE);
    let transforms = if transforms.is_file() {
        transforms
    } else {
        root.join("dsl").join("mappings").join(FNO_TRANSFORMS_FILE)
    };
    load_ttl_into(&store, &transforms)?;
    Ok(store)
}

/// Every `*.edoal.ttl` under `projections`, sorted by path (mirrors the Python
/// `sorted(projections_dir.glob("*.edoal.ttl"))`).
fn edoal_files(projections: &Path) -> Result<Vec<std::path::PathBuf>, SliceError> {
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    if projections.is_dir() {
        for entry in std::fs::read_dir(projections).map_err(SliceError::Io)? {
            let path = entry.map_err(SliceError::Io)?.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".edoal.ttl"))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Format a sorted IRI list as Python's `sorted(...)` list repr (`['a', 'b']`), so the
/// `fno-type` message is byte-identical to the retired Python lint.
fn py_list_repr(items: &[String]) -> String {
    let inner = items
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

// ── oxigraph store helpers ─────────────────────────────────────────────────────
//
// The same trivial parse/query boilerplate the shared `mapping_support` helpers use;
// kept local so the lint reads its own committed-tree stores without widening the
// shared support surface.

fn new_store() -> Result<Store, SliceError> {
    Store::new().map_err(|e| SliceError::Parse(format!("store creation failed: {e}")))
}

fn parse_ttl(path: &Path) -> Result<Store, SliceError> {
    let store = new_store()?;
    load_ttl_into(&store, path)?;
    Ok(store)
}

/// Parse one committed Turtle artifact into `store` (lenient, so GMEOW's `@x-gmeow-*`
/// language tags parse — mirrors the emitters' `load_into_store`).
fn load_ttl_into(store: &Store, path: &Path) -> Result<(), SliceError> {
    let bytes = std::fs::read(path).map_err(SliceError::Io)?;
    crate::rdf_text::turtle_bytes_into_store(store, &bytes, &path.display().to_string())
}

/// Every named-node subject of `?s a <type_iri>`.
fn subjects_of_type(store: &Store, type_iri: &str) -> Result<Vec<String>, SliceError> {
    let rdf_type = NamedNode::new(RDF_TYPE)
        .map_err(|e| SliceError::Parse(format!("invalid rdf:type IRI: {e}")))?;
    let class = NamedNode::new(type_iri)
        .map_err(|e| SliceError::Parse(format!("invalid type IRI {type_iri}: {e}")))?;
    let mut subjects = Vec::new();
    for quad in store.quads_for_pattern(
        None,
        Some(rdf_type.as_ref()),
        Some(class.as_ref().into()),
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        if let NamedOrBlankNode::NamedNode(nn) = &quad.subject {
            subjects.push(nn.as_str().to_owned());
        }
    }
    Ok(subjects)
}

/// The first IRI object of `<subject> <pred> ?o`, or `None` when the first object is
/// not a NamedNode (mirrors rdflib's `graph.value(...)` restricted to a URIRef).
fn first_object_iri(
    store: &Store,
    subject: &NamedNode,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    let predicate = NamedNode::new(pred)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {pred}: {e}")))?;
    match store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
    {
        Some(quad) => match quad.map_err(|e| SliceError::Parse(e.to_string()))?.object {
            Term::NamedNode(nn) => Ok(Some(nn.as_str().to_owned())),
            _ => Ok(None),
        },
        None => Ok(None),
    }
}

fn term_subject(term: &Term) -> Option<NamedOrBlankNode> {
    match term {
        Term::NamedNode(nn) => Some(NamedOrBlankNode::NamedNode(nn.clone())),
        Term::BlankNode(bn) => Some(NamedOrBlankNode::BlankNode(bn.clone())),
        _ => None,
    }
}

/// Every subject (named OR blank) of `?s a <type_iri>`, as a [`Term`]. EDOAL
/// `align:Cell` nodes are minted as stable blank nodes by the EDOAL correspondence
/// lowering, so the cell scan must carry blank-node subjects — unlike the FnO
/// `fno:Parameter`/`Function` IRIs that [`subjects_of_type`] handles.
fn subject_terms_of_type(store: &Store, type_iri: &str) -> Result<Vec<Term>, SliceError> {
    let rdf_type = NamedNode::new(RDF_TYPE)
        .map_err(|e| SliceError::Parse(format!("invalid rdf:type IRI: {e}")))?;
    let class = NamedNode::new(type_iri)
        .map_err(|e| SliceError::Parse(format!("invalid type IRI {type_iri}: {e}")))?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(
        None,
        Some(rdf_type.as_ref()),
        Some(class.as_ref().into()),
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        match quad.subject {
            NamedOrBlankNode::NamedNode(nn) => out.push(Term::NamedNode(nn)),
            NamedOrBlankNode::BlankNode(bn) => out.push(Term::BlankNode(bn)),
        }
    }
    Ok(out)
}

/// All object terms of `<subject> <pred> ?o` (subject may be a blank node).
fn objects_of_term(store: &Store, subject: &Term, pred: &str) -> Result<Vec<Term>, SliceError> {
    let Some(subj) = term_subject(subject) else {
        return Ok(Vec::new());
    };
    let predicate = NamedNode::new(pred)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {pred}: {e}")))?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(
        Some(subj.as_ref()),
        Some(predicate.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        out.push(quad.map_err(|e| SliceError::Parse(e.to_string()))?.object);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A param whose `fno:type` disagrees with its predicate's ontology `rdfs:range`
    /// is flagged; an agreeing one is clean.
    #[test]
    fn type_mismatch_is_flagged_match_is_clean() {
        let onto = store_from_turtle(
            "@prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             gm:eventTime rdfs:range xsd:dateTime .\n",
        );
        // Mismatch: declares xsd:string, range is xsd:dateTime.
        let bad = store_from_turtle(
            "@prefix fno: <https://w3id.org/function/ontology#> .\n\
             @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             gm:pTime a fno:Parameter ; fno:predicate gm:eventTime ; fno:type xsd:string .\n",
        );
        let probs = fno_type_mismatches(&onto, &bad).unwrap();
        assert_eq!(probs.len(), 1, "expected one mismatch");
        assert_eq!(probs[0].check, "fno-type");
        assert!(probs[0].message.contains("fno:type is"));
        assert_eq!(
            probs[0].instance.as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/pTime")
        );

        // Match: declares xsd:dateTime, equal to the range → clean.
        let good = store_from_turtle(
            "@prefix fno: <https://w3id.org/function/ontology#> .\n\
             @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             gm:pTime a fno:Parameter ; fno:predicate gm:eventTime ; fno:type xsd:dateTime .\n",
        );
        assert!(fno_type_mismatches(&onto, &good).unwrap().is_empty());

        // A predicate with no ontology range is skipped (external/projected).
        let no_range = store_from_turtle(
            "@prefix fno: <https://w3id.org/function/ontology#> .\n\
             @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             gm:pX a fno:Output ; fno:predicate gm:unranged ; fno:type xsd:string .\n",
        );
        assert!(fno_type_mismatches(&onto, &no_range).unwrap().is_empty());
    }

    /// An EDOAL cell transforming via an undefined `fn*` function is flagged; one that
    /// references a defined function is clean.
    #[test]
    fn undefined_fno_reference_is_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path();
        // FnO catalog defines only fnAlpha.
        write_ttl(
            &proj.join("functions.fno.ttl"),
            "@prefix fno: <https://w3id.org/function/ontology#> .\n\
             @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
             gm:fnAlpha a fno:Function .\n",
        );
        // EDOAL cell references fnBeta (undefined) via transformation→seeAlso.
        write_ttl(
            &proj.join("x.edoal.ttl"),
            "@prefix align: <http://knowledgeweb.semanticweb.org/heterogeneity/alignment#> .\n\
             @prefix edoal: <http://ns.inria.org/edoal/1.0/#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
             [] a align:Cell ; edoal:transformation [ rdfs:seeAlso gm:fnBeta ] .\n",
        );
        let fno = parse_ttl(&proj.join("functions.fno.ttl")).unwrap();
        let probs = fno_reference_integrity(&fno, proj).unwrap();
        assert_eq!(probs.len(), 1);
        assert_eq!(probs[0].check, "fno-ref");
        assert!(probs[0].message.contains("undefined FnO function"));
        assert!(probs[0].message.contains("fnBeta"));

        // Now define fnBeta too → clean.
        write_ttl(
            &proj.join("functions.fno.ttl"),
            "@prefix fno: <https://w3id.org/function/ontology#> .\n\
             @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
             gm:fnAlpha a fno:Function .\n\
             gm:fnBeta a fno:Function .\n",
        );
        let fno = parse_ttl(&proj.join("functions.fno.ttl")).unwrap();
        assert!(fno_reference_integrity(&fno, proj).unwrap().is_empty());
    }

    /// A `#`-separated FnO function IRI has its local name extracted correctly
    /// (split on `/` OR `#`). The retired Python `/`-only split would take
    /// `transform#fnGamma` as the local name — not starting with `fn` — and MISS
    /// this undefined reference; the native superset catches it.
    #[test]
    fn hash_separated_fno_reference_is_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path();
        write_ttl(
            &proj.join("functions.fno.ttl"),
            "@prefix fno: <https://w3id.org/function/ontology#> .\n\
             @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
             gm:fnAlpha a fno:Function .\n",
        );
        // seeAlso → a `#`-namespaced undefined function (local name `fnGamma`).
        write_ttl(
            &proj.join("h.edoal.ttl"),
            "@prefix align: <http://knowledgeweb.semanticweb.org/heterogeneity/alignment#> .\n\
             @prefix edoal: <http://ns.inria.org/edoal/1.0/#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             [] a align:Cell ; edoal:transformation \
                [ rdfs:seeAlso <https://example.org/transform#fnGamma> ] .\n",
        );
        let fno = parse_ttl(&proj.join("functions.fno.ttl")).unwrap();
        let probs = fno_reference_integrity(&fno, proj).unwrap();
        assert_eq!(
            probs.len(),
            1,
            "expected the #-separated undefined ref flagged"
        );
        assert!(probs[0].message.contains("fnGamma"));
    }

    // ── helpers ────────────────────────────────────────────────────────────────

    fn store_from_turtle(ttl: &str) -> Store {
        crate::rdf_text::turtle_bytes_to_store(ttl.as_bytes(), "test fixture").unwrap()
    }

    fn write_ttl(path: &Path, ttl: &str) {
        // The lenient Turtle parser reads hand-written `[]` blank-node syntax + CURIEs
        // directly, so the fixture text is written verbatim (no serializer round-trip).
        std::fs::write(path, ttl).unwrap();
    }
}
