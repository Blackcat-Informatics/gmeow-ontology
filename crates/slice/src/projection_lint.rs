// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native cross-layer consistency lint for the projection stack (#854).
//!
//! The alignment stack represents the same mappings four ways — SSSOM (1:1 term
//! links), EDOAL (complex cells), FnO (the transform functions), and SPARQL
//! CONSTRUCT (the executors) — plus the ontology. Each can drift independently. This
//! is the SUBSUME/ENHANCE move that pulls the three Python `projection_lint`
//! invariants (`gmeow_tools.projection_lint`, pure rdflib) into the native slice
//! framework so their problems can surface as canonical diagnostics
//! (`mapping-compile.fno-type` / `.fno-ref` / `.spec-drift`) folded into the dev-gate
//! report (the SARIF/JSON/HTML + `gmeow.gts` projections, #809/#654).
//!
//! The three checks (mirroring the retired Python, message wording preserved):
//!
//! * [`fno_type_mismatches`] — an `fno:Parameter`/`fno:Output` whose `fno:predicate`
//!   is a GMEOW property with a declared `rdfs:range` must declare an `fno:type` equal
//!   to that range.
//! * [`fno_reference_integrity`] — every FnO function an EDOAL cell invokes via
//!   `edoal:transformation` must be a defined `fno:Function`.
//! * [`projection_spec_drift`] — for each profile, every target-vocabulary term a
//!   CONSTRUCT executor emits must be declared in the spec (an EDOAL cell or an SSSOM
//!   alignment), and no EDOAL cell may be dead (declare a term the executor never
//!   emits).
//!
//! ## Inputs — the committed `generated/` tree
//!
//! The lint reads the **committed** rendered artifacts under `root`
//! (`generated/projections/*.{fno.ttl,edoal.ttl}`, `generated/queries/*.rq`), exactly
//! the default-arg behaviour of the Python trio (`PROJECTIONS_DIR` /
//! `PROJECTION_QUERY_DIR`). FnO/EDOAL/SPARQL emission is itself native, but only the
//! FnO + SSSOM emitters have PyO3 bindings, so re-emitting the `.rq` queries in-memory
//! is not yet possible; reading the committed tree is the correct semantic anyway —
//! the finding is over the *shipped* surface (`gmeow.gts`). The ontology
//! `rdfs:range`s come from the shared [`fno_emit::collect_ontology_store`] (one source
//! of truth with the emitter), and the SSSOM `aligned` set from the DSL-derived
//! [`mapping_emit::alignment_terms`] (byte-parity with the committed `*.sssom.tsv`).
//!
//! ## Why this lives in `gmeow-slice`
//!
//! `gmeow-slice` is the one crate that owns the FnO/SSSOM/EDOAL emitters and the
//! ontology-merge machinery the lint reuses; `gmeow-rdf-core` is oxigraph-free (#885)
//! and cannot host an oxigraph-parsing, ontology-reading consistency check.

use std::collections::BTreeSet;
use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphNameRef, NamedNode, NamedOrBlankNode, Term};
use oxigraph::store::Store;
use regex::Regex;

use crate::error::SliceError;
use crate::fno_emit::{collect_ontology_store, predicate_ranges};
use crate::mapping_emit::{alignment_terms, PREFIX_REGISTRY};

// ── Namespace constants ───────────────────────────────────────────────────────

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";

const FNO_PARAMETER: &str = "https://w3id.org/function/ontology#Parameter";
const FNO_OUTPUT: &str = "https://w3id.org/function/ontology#Output";
const FNO_FUNCTION: &str = "https://w3id.org/function/ontology#Function";
const FNO_PREDICATE: &str = "https://w3id.org/function/ontology#predicate";
const FNO_TYPE: &str = "https://w3id.org/function/ontology#type";

const ALIGN_CELL: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#Cell";
const ALIGN_ENTITY2: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#entity2";
const EDOAL_TRANSFORMATION: &str = "http://ns.inria.org/edoal/1.0/#transformation";
const EDOAL_URI: &str = "http://ns.inria.org/edoal/1.0/#uri";

/// The FnO catalog files (projection transforms + the language conversion catalog),
/// mirroring `projection_lint._FNO_FILES`.
const FNO_FUNCTIONS_FILE: &str = "functions.fno.ttl";
const FNO_TRANSFORMS_FILE: &str = "transforms.fno.ttl";

/// Each projection profile and the target-vocabulary prefixes it emits — a verbatim
/// port of the Python `projection_lint._PROFILE_TARGETS`. The `spec-drift` check is
/// scoped to exactly these profiles + prefixes; the `_STRUCTURAL_OUTPUTS` allowlist
/// below is tuned against the terms these emit.
const PROFILE_TARGETS: &[(&str, &[&str])] = &[
    ("schema-org", &["schema"]),
    // vcardx: the RFC-9554 extension namespace (PRONOUNS) — checked alongside vcard:
    // so the new vcardx:* output stays under CONSTRUCT↔EDOAL↔SSSOM drift.
    ("vcard", &["vcard", "vcardx"]),
    ("foaf", &["foaf", "wgs84"]),
    ("geosparql", &["geo"]),
    ("ical", &["ical"]),
    ("owl-time", &["time"]),
    ("bot", &["bot"]),
    // Rights module (#21): the structural ODRL policy + CC REL licence projections.
    ("odrl", &["odrl"]),
    ("cc", &["cc"]),
    ("dcterms", &["dcterms"]),
    ("oai_dc", &["dc"]),
    ("spdx", &["spdx"]),
    ("sosa", &["sosa", "geo"]),
    // Image super-ontology (#22)
    ("iiif", &["iiif", "oa"]),
    ("exif", &["exif"]),
    // Transpiler coverage profiles (#34 phases 2-3)
    ("org", &["org"]),
    ("bibo", &["bibo"]),
    // bibframe's minted identifier nodes carry their value via rdf:value.
    ("bibframe", &["bibframe", "rdf"]),
    ("gedcom", &["gedcom"]),
    ("sioc", &["sioc"]),
];

/// Target terms a compose/decompose transform legitimately MINTS — intermediate
/// nodes, linking properties, composed literals and datatypes. Outputs of declared
/// FnO transforms (no standalone EDOAL/SSSOM cell). A verbatim port of the Python
/// `projection_lint._STRUCTURAL_OUTPUTS`.
const STRUCTURAL_OUTPUTS: &[&str] = &[
    "http://www.w3.org/2006/time#inXSDDateTime",
    "https://schema.org/reviewRating",
    "https://schema.org/DataDownload",
    "https://schema.org/mainEntityOfPage",
    "https://schema.org/Quotation",
    "https://schema.org/DataFeedItem",
    "https://schema.org/dataFeedElement",
    "https://schema.org/CommunicateAction",
    "https://schema.org/potentialAction",
    "https://schema.org/target",
    "http://id.loc.gov/ontologies/bibframe/Doi",
    "http://id.loc.gov/ontologies/bibframe/Identifier",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#value",
    "http://rdfs.org/sioc/ns#container_of",
    "http://www.w3.org/2000/10/swap/pim/gedcom#spouseIn",
    "http://www.w3.org/2006/vcard/ns#hasName",
    "http://www.w3.org/2006/vcard/ns#Name",
    "http://www.w3.org/2006/vcard/ns#hasAddress",
    "http://www.w3.org/2006/vcard/ns#label",
    "http://www.w3.org/2006/vcard/ns#hasGeo",
    "http://www.w3.org/2006/vcard/ns#Geo",
    "http://www.w3.org/2006/vcard/ns#latitude",
    "http://www.w3.org/2006/vcard/ns#longitude",
    "http://www.opengis.net/ont/geosparql#wktLiteral",
    "http://www.opengis.net/ont/geosparql#geoJSONLiteral",
];

// ── Diagnostic carrier ───────────────────────────────────────────────────────

/// One projection-lint problem. The `check`/`instance` convention mirrors the native
/// SSSOM validator's diagnostic dict (`gmeow_rdf.validate_sssom`) so the PyO3 binding
/// packs both into the same `{severity, code, message, check, instance}` shape the
/// Python finding leg already consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionDiagnostic {
    /// Always `"ERROR"` — every projection-lint problem is a hard inconsistency
    /// (mirrors the Python trio, whose every returned string is a gate failure).
    pub severity: String,
    /// The drift family: `fno-type`, `fno-ref`, or `spec-drift`. The Python finding
    /// leg maps this to the canonical code `mapping-compile.<check>`.
    pub check: String,
    /// A stable per-check code (same value as `check`); carried for dict parity with
    /// the SSSOM validator's `code` slot.
    pub code: String,
    /// The human-readable problem, verbatim from the retired Python lint.
    pub message: String,
    /// The most-specific RDF node the problem concerns (the FnO param/output IRI, the
    /// undefined function IRI, or the drifting target term), or `None`.
    pub instance: Option<String>,
}

impl ProjectionDiagnostic {
    fn error(check: &str, message: String, instance: Option<String>) -> Self {
        Self {
            severity: "ERROR".to_owned(),
            check: check.to_owned(),
            code: check.to_owned(),
            message,
            instance,
        }
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Run the projection-lint invariants plus the alignment-direction lint against the
/// committed `generated/` tree under `root`, returning every problem as a
/// [`ProjectionDiagnostic`].
///
/// An empty result means the projection stack and SSSOM alignments are internally
/// consistent. Projection checks run first (`fno-type` → `fno-ref` → `spec-drift`),
/// then alignment checks (`inverse-direction`, `domain-range`, `property-character`,
/// `equivalence-collapse`, `dc-refinement`, `dc-hand-authored`). The combined list is
/// sorted deterministically by severity → check → instance.
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source (a committed
/// artifact, the ontology, an SSSOM source) or a `_PROFILE_TARGETS` prefix absent
/// from the curated [`PREFIX_REGISTRY`] — no degraded fallback (CONSTITUTION /
/// no-compromises).
pub fn lint_projection(
    root: &Path,
    allow_network: bool,
) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let projections = root.join("generated").join("projections");
    let queries = root.join("generated").join("queries");

    let onto = collect_ontology_store(root)?;
    let fno = fno_catalog_store(root, &projections)?;

    let mut out: Vec<ProjectionDiagnostic> = Vec::new();
    out.extend(fno_type_mismatches(&onto, &fno)?);
    out.extend(fno_reference_integrity(&fno, &projections)?);
    out.extend(projection_spec_drift(root, &projections, &queries)?);
    out.extend(crate::alignment_lint::lint_alignment_directions(
        root,
        allow_network,
    )?);

    out.sort_by(|a, b| {
        let order = |s: &str| match s {
            "ERROR" => 0,
            "WARNING" => 1,
            "INFO" => 2,
            _ => 3,
        };
        order(&a.severity)
            .cmp(&order(&b.severity))
            .then_with(|| a.check.cmp(&b.check))
            .then_with(|| a.instance.cmp(&b.instance))
    });
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
                    // Last path segment — split on `/` OR `#`, matching the sibling
                    // `fno_emit::local` helper. A future-proofing superset of the retired
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

// ── Check 3: CONSTRUCT ↔ EDOAL ↔ SSSOM drift ───────────────────────────────────

/// CONSTRUCT↔EDOAL↔SSSOM inconsistencies, per profile (mirrors
/// `projection_lint.projection_spec_drift`).
fn projection_spec_drift(
    root: &Path,
    projections: &Path,
    queries: &Path,
) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    // An SSSOM row may place the external term in subject OR object position, so the
    // `aligned` set carries both endpoints of every equivalence.
    let aligned = alignment_terms(root)?;

    let mut problems: Vec<ProjectionDiagnostic> = Vec::new();
    for (profile, prefixes) in PROFILE_TARGETS {
        let namespaces = prefix_namespaces(prefixes)?;

        let query_text = std::fs::read_to_string(queries.join(format!("{profile}.rq")))
            .map_err(SliceError::Io)?;
        let emitted = target_terms_in_query(&query_text, prefixes)?;

        let edoal = edoal_targets(
            &projections.join(format!("{profile}.edoal.ttl")),
            &namespaces,
        )?;

        // declared = EDOAL cells ∪ SSSOM-aligned-in-namespace ∪ structural mints.
        let mut declared: BTreeSet<String> = edoal.clone();
        for term in &aligned {
            if starts_with_any(term, &namespaces) {
                declared.insert(term.clone());
            }
        }
        for term in STRUCTURAL_OUTPUTS {
            declared.insert((*term).to_owned());
        }

        // emitted − declared (sorted): an executor output with no spec cell.
        for term in emitted.difference(&declared) {
            problems.push(ProjectionDiagnostic::error(
                "spec-drift",
                format!(
                    "{profile}: {term} emitted by the executor but declared in \
                     neither EDOAL nor SSSOM"
                ),
                Some(term.clone()),
            ));
        }
        // edoal − emitted (sorted): a dead EDOAL cell.
        for term in edoal.difference(&emitted) {
            problems.push(ProjectionDiagnostic::error(
                "spec-drift",
                format!(
                    "{profile}: {term} declared in EDOAL but never emitted by the \
                     {profile}.rq executor (dead cell)"
                ),
                Some(term.clone()),
            ));
        }
    }
    Ok(problems)
}

/// Target-vocabulary IRIs mentioned in a CONSTRUCT query (comments stripped), mirroring
/// `projection_lint._target_terms_in_query`'s `\b(prefix):([A-Za-z][\w-]*)` scan.
fn target_terms_in_query(text: &str, prefixes: &[&str]) -> Result<BTreeSet<String>, SliceError> {
    let pattern = format!(r"\b({}):([A-Za-z][\w-]*)", prefixes.join("|"));
    let re = Regex::new(&pattern)
        .map_err(|e| SliceError::Parse(format!("CURIE scan regex build failed: {e}")))?;
    let map = prefix_map();
    let mut out: BTreeSet<String> = BTreeSet::new();
    for line in text.lines() {
        let body = line.split('#').next().unwrap_or(line);
        for caps in re.captures_iter(body) {
            let prefix = &caps[1];
            let local = &caps[2];
            let Some(ns) = map.iter().find(|(p, _)| *p == prefix).map(|(_, ns)| *ns) else {
                // A profile-declared prefix not in the registry is a hard error
                // caught by prefix_namespaces above; a match here for an unmapped
                // prefix is impossible because the alternation only contains
                // profile prefixes, which are validated. Defensive skip.
                continue;
            };
            out.insert(format!("{ns}{local}"));
        }
    }
    Ok(out)
}

/// Target-vocabulary IRIs an EDOAL file declares (its cells' `entity2`/`edoal:uri`),
/// mirroring `projection_lint._edoal_targets`.
fn edoal_targets(path: &Path, namespaces: &[String]) -> Result<BTreeSet<String>, SliceError> {
    let store = parse_ttl(path)?;
    let mut out: BTreeSet<String> = BTreeSet::new();
    for cell in subject_terms_of_type(&store, ALIGN_CELL)? {
        let Some(entity2) = first_object_of_term(&store, &cell, ALIGN_ENTITY2)? else {
            continue;
        };
        if let Some(uri) = first_object_iri_of_term(&store, &entity2, EDOAL_URI)? {
            if starts_with_any(&uri, namespaces) {
                out.insert(uri);
            }
        }
    }
    Ok(out)
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

// ── Prefix resolution ──────────────────────────────────────────────────────────

/// The curated prefix → namespace map (the static [`PREFIX_REGISTRY`], the
/// `config.PREFIXES` authority the Python lint reads).
fn prefix_map() -> &'static [(&'static str, &'static str)] {
    PREFIX_REGISTRY
}

/// The namespace IRIs for a profile's prefixes — hard-fails if any prefix is absent
/// from the curated registry (the `PREFIXES[p]` KeyError, made explicit).
fn prefix_namespaces(prefixes: &[&str]) -> Result<Vec<String>, SliceError> {
    let map = prefix_map();
    prefixes
        .iter()
        .map(|p| {
            map.iter()
                .find(|(name, _)| name == p)
                .map(|(_, ns)| (*ns).to_owned())
                .ok_or_else(|| {
                    SliceError::Parse(format!(
                        "projection-lint: profile prefix `{p}` is not in the curated PREFIX_REGISTRY"
                    ))
                })
        })
        .collect()
}

fn starts_with_any(term: &str, namespaces: &[String]) -> bool {
    namespaces.iter().any(|ns| term.starts_with(ns))
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
// The same trivial parse/query boilerplate each sibling emitter repeats
// (fno_emit / mapping_emit / sparql_emit); kept local so the lint reads its own
// committed-tree stores without widening the emitters' query surface.

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
    Ok(())
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
/// `align:Cell` nodes are minted as stable blank nodes (`edoal_emit::_stable_bnode`),
/// so the cell scan must carry blank-node subjects — unlike the FnO
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

fn first_object_of_term(
    store: &Store,
    subject: &Term,
    pred: &str,
) -> Result<Option<Term>, SliceError> {
    Ok(objects_of_term(store, subject, pred)?.into_iter().next())
}

fn first_object_iri_of_term(
    store: &Store,
    subject: &Term,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    match first_object_of_term(store, subject, pred)? {
        Some(Term::NamedNode(nn)) => Ok(Some(nn.as_str().to_owned())),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `_PROFILE_TARGETS` prefix MUST resolve in the curated registry — the
    /// load-bearing parity point for byte-faithful `spec-drift` (the Python
    /// `PREFIXES[p]` lookup). A drifted/renamed prefix is a hard error.
    #[test]
    fn every_profile_prefix_resolves_in_registry() {
        for (profile, prefixes) in PROFILE_TARGETS {
            for p in *prefixes {
                assert!(
                    PREFIX_REGISTRY.iter().any(|(name, _)| name == p),
                    "profile {profile}: prefix `{p}` missing from PREFIX_REGISTRY"
                );
            }
        }
        // And the resolver agrees.
        for (_, prefixes) in PROFILE_TARGETS {
            assert!(prefix_namespaces(prefixes).is_ok());
        }
    }

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

    /// The CURIE scan honors word boundaries and comment stripping, and resolves to
    /// full namespace IRIs.
    #[test]
    fn curie_scan_matches_python_semantics() {
        let text = "CONSTRUCT { ?s schema:name ?n }  # schema:ignored in comment\n\
                    WHERE { ?s xschema:nope ?x ; vcardx:pronouns ?p }\n";
        let terms = target_terms_in_query(text, &["schema", "vcardx"]).unwrap();
        assert!(terms.contains("https://schema.org/name"));
        // vcardx resolves via the registry (RFC-9554 extension namespace).
        assert!(terms.iter().any(|t| t.ends_with("pronouns")));
        // comment-stripped + word-boundary: schema:ignored / xschema:nope excluded.
        assert!(!terms.contains("https://schema.org/ignored"));
        assert!(!terms.iter().any(|t| t.ends_with("nope")));
    }

    /// A `spec-drift`: an EDOAL cell declaring a term the executor never emits is a
    /// dead cell; an emitted term with no spec cell is undeclared.
    #[test]
    fn spec_drift_dead_cell_and_undeclared() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let proj = root.join("generated").join("projections");
        let queries = root.join("generated").join("queries");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&queries).unwrap();

        // schema-org profile: EDOAL declares schema:deadTerm; the .rq emits
        // schema:liveTerm. With no SSSOM alignment, each side drifts.
        write_ttl(
            &proj.join("schema-org.edoal.ttl"),
            "@prefix align: <http://knowledgeweb.semanticweb.org/heterogeneity/alignment#> .\n\
             @prefix edoal: <http://ns.inria.org/edoal/1.0/#> .\n\
             [] a align:Cell ; align:entity2 [ edoal:uri <https://schema.org/deadTerm> ] .\n",
        );
        std::fs::write(
            queries.join("schema-org.rq"),
            "CONSTRUCT { ?s schema:liveTerm ?o } WHERE { ?s ?p ?o }\n",
        )
        .unwrap();
        // Every other profile needs an (empty) .rq + .edoal.ttl so the loop reads them.
        for (profile, _) in PROFILE_TARGETS {
            if *profile == "schema-org" {
                continue;
            }
            std::fs::write(
                queries.join(format!("{profile}.rq")),
                "CONSTRUCT {} WHERE {}\n",
            )
            .unwrap();
            write_ttl(
                &proj.join(format!("{profile}.edoal.ttl")),
                "@prefix align: <http://knowledgeweb.semanticweb.org/heterogeneity/alignment#> .\n",
            );
        }
        // Minimal SSSOM source set so alignment_terms() succeeds (empty alignment).
        std::fs::create_dir_all(root.join("dsl").join("mappings")).unwrap();

        let probs = projection_spec_drift(root, &proj, &queries).unwrap();
        let msgs: Vec<&str> = probs.iter().map(|p| p.message.as_str()).collect();
        assert!(
            msgs.iter()
                .any(|m| m.contains("deadTerm") && m.contains("dead cell")),
            "expected dead-cell drift, got {msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("liveTerm") && m.contains("neither EDOAL nor SSSOM")),
            "expected undeclared-emission drift, got {msgs:?}"
        );
        assert!(probs.iter().all(|p| p.check == "spec-drift"));
    }

    // ── helpers ────────────────────────────────────────────────────────────────

    fn store_from_turtle(ttl: &str) -> Store {
        let store = new_store().unwrap();
        for quad in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(ttl.as_bytes())
        {
            store.insert(&quad.unwrap()).unwrap();
        }
        store
    }

    fn write_ttl(path: &Path, ttl: &str) {
        // The lenient Turtle parser reads hand-written `[]` blank-node syntax + CURIEs
        // directly, so the fixture text is written verbatim (no serializer round-trip).
        std::fs::write(path, ttl).unwrap();
    }

    /// After #936, lint_projection folds alignment-direction diagnostics too.
    /// A hand-authored dc: alignment in the SSSOM mapping set surfaces as a
    /// `dc-hand-authored` WARNING even when the projection stack itself is clean.
    #[test]
    fn lint_projection_includes_alignment_findings() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let proj = root.join("generated").join("projections");
        let queries = root.join("generated").join("queries");
        let mappings = root.join("generated").join("mappings");
        let ontology = root.join("ontology");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&queries).unwrap();
        std::fs::create_dir_all(&mappings).unwrap();
        std::fs::create_dir_all(&ontology).unwrap();

        // Minimal clean FnO catalog (transforms file must exist to avoid dsl fallback).
        write_ttl(
            &proj.join("functions.fno.ttl"),
            "@prefix fno: <https://w3id.org/function/ontology#> .\n",
        );
        write_ttl(
            &proj.join("transforms.fno.ttl"),
            "@prefix fno: <https://w3id.org/function/ontology#> .\n",
        );

        // Empty executors + EDOAL cells for every profile so spec-drift is clean.
        for (profile, _) in PROFILE_TARGETS {
            std::fs::write(
                queries.join(format!("{profile}.rq")),
                "CONSTRUCT {} WHERE {}\n",
            )
            .unwrap();
            write_ttl(
                &proj.join(format!("{profile}.edoal.ttl")),
                "@prefix align: <http://knowledgeweb.semanticweb.org/heterogeneity/alignment#> .\n",
            );
        }

        // SSSOM mapping with a hand-authored dc: alignment.
        std::fs::write(
            mappings.join("dc.sssom.tsv"),
            "subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\n\
             gmeow:creator\tskos:closeMatch\tdc:creator\tsemapv:ManualMappingCuration\t0.9\n",
        )
        .unwrap();

        // Minimal ontology so the mapping subject is recognized as a property.
        write_ttl(
            &ontology.join("gmeow.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:creator a owl:ObjectProperty .\n",
        );

        let problems = lint_projection(root, false).unwrap();
        let dc: Vec<_> = problems
            .iter()
            .filter(|d| d.check == "dc-hand-authored")
            .collect();
        assert!(
            !dc.is_empty(),
            "expected a dc-hand-authored alignment finding, got {problems:?}"
        );
        assert_eq!(dc[0].severity, "WARNING");
        assert_eq!(
            dc[0].instance.as_deref(),
            Some("http://purl.org/dc/elements/1.1/creator")
        );
    }
}
