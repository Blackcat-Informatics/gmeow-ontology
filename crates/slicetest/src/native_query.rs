// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The oxigraph-free native SPARQL substrate for the slice-test harness.
//!
//! Every store the harness ran over was an oxigraph in-memory `Store` queried via
//! `SparqlEvaluator`; this module replaces both with the native stack:
//!
//! * the data graph is a frozen [`Arc<RdfDataset>`](RdfDataset), built by parsing
//!   each Turtle source through the canonical native codec
//!   (`purrdf::parse_dataset`) and `RdfDataset::union`-ing them into one — the
//!   same merge `gmeow_validate::store::build_store` did, but in the IR, never an
//!   oxigraph `Store`;
//! * queries run through [`NativeSparqlEngine`] (`purrdf::sparql`), the single
//!   required impl of the `purrdf` `SparqlEngine` seam;
//! * result terms are dataset-independent [`TermValue`]s, rendered to a canonical
//!   N-Triples lexical form so a competency question's expected rows compare on the
//!   SAME string both sides.
//!
//! Slice sources are Turtle (a single default graph), so the union keeps every quad
//! in the default graph — no flattening is required (unlike the GTS-bundle conformance
//! gate, whose `gmeow.gts` carries named graphs).

use std::path::PathBuf;
use std::sync::Arc;

use gmeow_errors::{Diag, Result};
use purrdf::ir::RdfDatasetBuilder;
use purrdf::parse_dataset;
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfDataset, RdfTerm, SparqlEngine, SparqlRequest, SparqlResult, TermValue};

use crate::error::{DatasetRead, SparqlEval, UnexpectedResultForm};

/// One SELECT result: the projected variable names and the rows of optional terms,
/// the dataset-independent egress shape the native engine materializes.
pub struct Solutions {
    pub variables: Vec<String>,
    pub rows: Vec<Vec<Option<TermValue>>>,
}

/// Memoized parse results, keyed by path. A slice test file runs many cells that each
/// rebuild the same scoped module/example datasets; parsing a multi-thousand-triple
/// `module.ttl` once per cell dominates the test wall-clock. The fixtures are immutable
/// for the life of a test run and a frozen [`RdfDataset`] is read-only + `Arc`-shared,
/// so caching by path is sound and turns O(cells x parse) back into O(files x parse).
static PARSE_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Arc<RdfDataset>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Parse one Turtle file into a frozen dataset (the native codec, lenient on the
/// private-use `@x-gmeow-*` language tags, exactly as `gmeow_validate`'s `parse_file`).
/// Results are memoized by path (see [`PARSE_CACHE`]).
///
/// # Errors
/// Hard-fails if the file cannot be read or parsed.
pub fn dataset_from_file(path: &std::path::Path) -> Result<Arc<RdfDataset>> {
    let key = path.to_path_buf();
    if let Some(ds) = PARSE_CACHE.lock().expect("parse cache mutex").get(&key) {
        return Ok(Arc::clone(ds));
    }
    let bytes = std::fs::read(path).map_err(|e| {
        Diag::of_kind(DatasetRead {
            detail: format!("read {}: {e}", path.display()),
        })
    })?;
    let ds = parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        Diag::of_kind(DatasetRead {
            detail: format!("parse {}: {e}", path.display()),
        })
    })?;
    PARSE_CACHE
        .lock()
        .expect("parse cache mutex")
        .insert(key, Arc::clone(&ds));
    Ok(ds)
}

/// Build one merged dataset from a set of Turtle sources by parsing each and unioning
/// them — the IR-native twin of `gmeow_validate::store::build_store`.
///
/// # Errors
/// Hard-fails if any file fails to read or parse.
pub fn dataset_from_files(paths: &[PathBuf]) -> Result<Arc<RdfDataset>> {
    let parsed: Vec<Arc<RdfDataset>> = paths
        .iter()
        .map(|p| dataset_from_file(p))
        .collect::<Result<_>>()?;
    Ok(union(&parsed))
}

/// Union a set of frozen datasets into one (blank scopes are standardized apart by
/// `RdfDataset::union`). An empty input yields an empty dataset.
#[must_use]
pub fn union(datasets: &[Arc<RdfDataset>]) -> Arc<RdfDataset> {
    let refs: Vec<&RdfDataset> = datasets.iter().map(AsRef::as_ref).collect();
    Arc::new(RdfDataset::union(&refs))
}

/// `rdf:type` — the predicate whose OBJECT names a term's kind. A quad on this
/// predicate whose object is a canonical `logic:` type marker gets an OWL-view
/// twin (`logic:Class`→`owl:Class`, …) via [`gmeow_ns::owl_view_of_type_marker`].
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// `dataset` with the complete OWL/RDFS **projection** of its canonical `logic:`
/// vocabulary materialized — the surface a SHACL shape set and the fixtures'
/// `owl:`/`rdfs:` ASK/SELECT queries are written against.
///
/// # Why a SHACL data graph is not the authored graph
///
/// GMEOW's authored surface is the canonical `logic:` vocabulary; the OWL/RDFS
/// surface is one of its lossy projections (Principle 17). `logic:subClassOf`'s
/// own definition says it "lowers to `rdfs:subClassOf` on the OWL/RDFS projection
/// surface", and the shipped projection does exactly that (`generated/owl/*.ttl`
/// carries `<C> rdfs:subClassOf <D>` for an edge authored `logic:subClassOf`).
///
/// SHACL is a projection surface too, and the generated shape set is derived
/// against the OWL/RDFS spelling — a `logic:Constraint` whose formula names
/// `rdfs:subClassOf` projects a `sh:sparql` that matches `rdfs:subClassOf`. So a
/// conformance cell that validates the RAW authored module against those shapes
/// is comparing two different surfaces: every re-authored subsumption edge is
/// invisible to the shape, and the constraint fails (or, worse for a negative
/// constraint, passes vacuously). Lowering the data into the shape's own surface
/// first is what restores the correspondence — the shapes and the ontology are
/// both right; only the un-projected data graph was wrong.
///
/// # What is projected
///
/// The single source of the projection map is the `logic:` slice's
/// `graph/correspondence-laws` corpus, mirrored in `gmeow_ns`:
///
/// * `rdf:type` OBJECTS — every canonical typing / property-characteristic /
///   axiom marker via [`gmeow_ns::owl_view_of_type_marker`]
///   (`logic:Class`→`owl:Class`, `logic:AllDisjointClasses`→`owl:AllDisjointClasses`, …);
/// * PREDICATES — every canonical axiom / class-expression / restriction /
///   cardinality / identity / header edge via [`gmeow_ns::owl_view_of_predicate`]
///   (`logic:subClassOf`→`rdfs:subClassOf`, `logic:members`→`owl:members`,
///   `logic:someValuesFrom`→`owl:someValuesFrom`, …).
///
/// The canonical `logic:` quads are KEPT (the projection ADDS the OWL/RDFS view,
/// it never rewrites the authored quad away), and blank-node identity is preserved
/// so an `owl:Restriction`-encoded axiom survives intact.
#[must_use]
pub fn with_owl_rdfs_projection(dataset: &Arc<RdfDataset>) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let mut projected = 0usize;
    for quad in dataset.owned_quads() {
        let pred_view = gmeow_ns::owl_view_of_predicate(&quad.predicate);
        let obj_view = object_marker_view(&quad.predicate, &quad.object);
        // Emit EVERY surface combination of {canonical, projected} for the predicate
        // and the object, so a fixture written in any spelling matches. The fixtures
        // are not uniform: a restriction cell reads `logic:onProperty …; logic:onClass
        // owl:Thing` (canonical predicate, projected object), while a property cell
        // reads `rdfs:range owl:Thing` (both projected). The canonical quad is always
        // kept last; the projection only ever ADDS the view spellings.
        match (pred_view, obj_view) {
            (None, None) => {}
            (Some(p), None) => {
                let mut lowered = quad.clone();
                lowered.predicate = p.to_owned();
                builder.push_owned_quad(&lowered);
                projected += 1;
            }
            (None, Some(o)) => {
                let mut lowered = quad.clone();
                lowered.object = RdfTerm::iri(o);
                builder.push_owned_quad(&lowered);
                projected += 1;
            }
            (Some(p), Some(o)) => {
                // both-projected, predicate-only, object-only
                let mut both = quad.clone();
                both.predicate = p.to_owned();
                both.object = RdfTerm::iri(o);
                builder.push_owned_quad(&both);
                let mut pred_only = quad.clone();
                pred_only.predicate = p.to_owned();
                builder.push_owned_quad(&pred_only);
                let mut obj_only = quad.clone();
                obj_only.object = RdfTerm::iri(o);
                builder.push_owned_quad(&obj_only);
                projected += 3;
            }
        }
        builder.push_owned_quad(&quad);
    }
    if projected == 0 {
        return Arc::clone(dataset);
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }
    builder
        .freeze()
        .expect("projecting a valid dataset's canonical edges re-freezes successfully")
}

/// The OWL-view spelling of a quad's OBJECT when it names a canonical `logic:` marker,
/// or `None`.
///
/// In `rdf:type` position EVERY typing / property-characteristic / axiom marker lowers
/// ([`gmeow_ns::owl_view_of_type_marker`]). In a CLASS-POSITION predicate
/// ([`gmeow_ns::is_class_position_predicate`] — `rdfs:range`/`logic:range`,
/// `owl:onClass`/`logic:onClass`, …) only the universal class-identity markers
/// `logic:Thing` / `logic:Nothing` are lowered; they are the sole canonical markers that
/// appear as a class-valued object.
///
/// Crucially, a marker object under a NON-class predicate is left untouched: a
/// `logic:GroundingCorrespondence`'s `logic:sourceEndpoint logic:Thing` names the term
/// `logic:Thing` as data, NOT a class filler, so lowering it would mint a second
/// `sourceEndpoint` value and trip that shape's `maxCount 1`.
fn object_marker_view(predicate: &str, object: &RdfTerm) -> Option<&'static str> {
    let RdfTerm::Iri(object_iri) = object else {
        return None;
    };
    if predicate == RDF_TYPE {
        gmeow_ns::owl_view_of_type_marker(object_iri)
    } else if gmeow_ns::is_class_position_predicate(predicate) {
        match object_iri.as_str() {
            gmeow_ns::LOGIC_THING => Some(gmeow_ns::OWL_THING),
            gmeow_ns::LOGIC_NOTHING => Some(gmeow_ns::OWL_NOTHING),
            _ => None,
        }
    } else {
        None
    }
}

/// Merge frozen datasets into one **preserving blank-node identity** across inputs.
///
/// Unlike [`union`] (which standardizes every input's blanks APART under a fresh
/// merge scope), this folds each dataset's already-scope-qualified owned terms into a
/// single builder at the default scope. A blank that appears in several inputs with
/// the same scope-qualified label therefore stays ONE node, and identical quads dedup
/// — the property the RDFS fixpoint depends on (re-deriving an existing quad must NOT
/// mint a fresh blank each round, which `union`'s re-scoping does, defeating
/// termination). The inputs here are all derived FROM one base dataset (its blanks
/// share a scope), so their qualified labels coincide and identity is preserved.
#[must_use]
pub fn merge_preserving_blanks(datasets: &[Arc<RdfDataset>]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for ds in datasets {
        for quad in ds.owned_quads() {
            builder.push_owned_quad(&quad);
        }
        for reifier in ds.owned_reifiers() {
            builder.push_owned_reifier(&reifier);
        }
        for annotation in ds.owned_annotations() {
            builder.push_owned_annotation(&annotation);
        }
    }
    builder
        .freeze()
        .expect("merge of valid datasets re-freezes successfully")
}

/// Parse inline Turtle into a frozen dataset (the native codec).
///
/// # Errors
/// Hard-fails if the Turtle fails to parse.
pub fn dataset_from_turtle(ttl: &str) -> Result<Arc<RdfDataset>> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).map_err(|e| {
        Diag::of_kind(DatasetRead {
            detail: format!("parse turtle: {e}"),
        })
    })
}

/// Run a SPARQL query over `dataset`, returning the native result.
///
/// # Errors
/// Hard-fails on a parse or evaluation error (carrying the diagnostic message).
pub fn query(dataset: &Arc<RdfDataset>, sparql: &str) -> Result<SparqlResult> {
    let engine = NativeSparqlEngine::new();
    engine
        .query(
            dataset,
            SparqlRequest {
                query: sparql,
                base_iri: None,
                substitutions: &[],
            },
        )
        .map_err(|e| {
            Diag::of_kind(SparqlEval {
                detail: e.to_string(),
            })
        })
}

/// Run a SELECT query, hard-failing if it is not a SELECT.
///
/// # Errors
/// Hard-fails on a parse/eval error or if the form is not SELECT.
pub fn select(dataset: &Arc<RdfDataset>, sparql: &str) -> Result<Solutions> {
    match query(dataset, sparql)? {
        SparqlResult::Solutions {
            variables, rows, ..
        } => Ok(Solutions { variables, rows }),
        SparqlResult::Boolean(_) | SparqlResult::Graph(_) => {
            Err(Diag::of_kind(UnexpectedResultForm {
                detail: "query must be a SELECT".to_owned(),
            }))
        }
    }
}

/// Render a [`TermValue`] to a canonical N-Triples lexical form. Used on BOTH the
/// actual binding and the expected cell value so a competency row compares on the
/// same string regardless of binding/iteration order.
#[must_use]
pub fn render_term(term: &TermValue) -> String {
    match term {
        TermValue::Iri(iri) => format!("<{iri}>"),
        TermValue::Blank { label, .. } => format!("_:{label}"),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            let escaped = escape_literal(lexical_form);
            match language {
                Some(lang) => format!("\"{escaped}\"@{lang}"),
                None => format!("\"{escaped}\"^^<{datatype}>"),
            }
        }
        TermValue::Triple { s, p, o } => {
            format!(
                "<< {} {} {} >>",
                render_term(s),
                render_term(p),
                render_term(o)
            )
        }
    }
}

/// Escape a literal lexical form for the N-Triples quoted-string rendering used by
/// [`render_term`].
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}
