// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native MAXIMAL(G) transform kernel: GMEOW A-Box -> base + E(G) + P(G).
//!
//! Python supplies serialized repo-or-bundle inputs and remains the CLI/file
//! surface. This module owns the graph dataflow: deterministic skolemization,
//! suppression-aware strong-equivalence saturation, projection CONSTRUCT
//! execution, provenance merge, and GTS byte emission.
//!
//! Oxigraph-free: the transient triple stores are flat
//! [`purrdf::RdfDataset`]s built from owned [`RdfQuad`] streams and pattern-queried
//! through the native [`DatasetView`]; the projection CONSTRUCT runs through
//! [`NativeSparqlEngine`]. The deterministic Skolem IRI minting, the content-addressed
//! reifier hash, and every committed N-Triples / GTS byte are preserved exactly: the
//! canonical labels come from the SAME RDFC-1.0 canonicalizer and the term tokens come
//! from the native term renderer, which is byte-identical to oxigraph's N-Triples term
//! form for the IRIs / blanks / typed-decimal literals this kernel emits.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    DatasetView, GraphMatch, NativeRdfFormat, RdfDataset, RdfLiteral, RdfQuad, RdfTerm,
    SparqlEngine, SparqlRequest, SparqlResult, TermId, TermRef, TermValue,
};
use sha2::{Digest, Sha256};

use gmeow_errors::ResultExt;

use crate::projections::{TagMap, retag_quads};

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const SKOLEM_BASE: &str = "https://blackcatinformatics.ca/gmeow/.well-known/genid/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

const GM_DISPLAYABLE: &str = "https://blackcatinformatics.ca/gmeow/displayable";
const GM_COARSEN_TO: &str = "https://blackcatinformatics.ca/gmeow/coarsenTo";
const GM_COARSEN_GUARDED: &str = "https://blackcatinformatics.ca/gmeow/coarsenGuarded";
const GM_APPELLATION: &str = "https://blackcatinformatics.ca/gmeow/Appellation";
const GM_STATEMENT_METADATA: &str = "https://blackcatinformatics.ca/gmeow/StatementMetadata";
const GM_Q_SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/qSubject";
const GM_Q_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/qPredicate";
const GM_Q_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/qObject";
const GM_Q_OBJECT_LITERAL: &str = "https://blackcatinformatics.ca/gmeow/qObjectLiteral";
const GM_MAPPED_FROM: &str = "https://blackcatinformatics.ca/gmeow/mappedFrom";
const GM_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const SCHEMA_SAME_AS: &str = "https://schema.org/sameAs";
const SAME_AS_MIRROR_RULE: &str = "https://blackcatinformatics.ca/gmeow/rules/sameAsMirror";

const STRONG_CLASS_PREDICATES: &[&str] = &["owl:equivalentClass", "skos:exactMatch"];
const STRONG_PROPERTY_PREDICATES: &[&str] = &["owl:equivalentProperty", "skos:exactMatch"];

const PREFIXES: &[(&str, &str)] = &[
    ("gmeow", GM),
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("owl", "http://www.w3.org/2002/07/owl#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("skos", "http://www.w3.org/2004/02/skos/core#"),
    ("schema", "https://schema.org/"),
    ("foaf", "http://xmlns.com/foaf/0.1/"),
    ("doap", "http://usefulinc.com/ns/doap#"),
    ("vcard", "http://www.w3.org/2006/vcard/ns#"),
    ("vcardx", "http://www.w3.org/2006/vcard/ns#"),
    ("org", "http://www.w3.org/ns/org#"),
    ("time", "http://www.w3.org/2006/time#"),
    ("sioc", "http://rdfs.org/sioc/ns#"),
    ("bibo", "http://purl.org/ontology/bibo/"),
    ("bf", "http://id.loc.gov/ontologies/bibframe/"),
    ("bibframe", "http://id.loc.gov/ontologies/bibframe/"),
    ("gedcom", "http://www.w3.org/2000/10/swap/pim/gedcom#"),
    ("rel", "http://purl.org/vocab/relationship/"),
    ("cc", "http://creativecommons.org/ns#"),
    ("odrl", "http://www.w3.org/ns/odrl/2/"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("spdx", "http://spdx.org/rdf/terms#"),
    ("prov", "http://www.w3.org/ns/prov#"),
    ("geo", "http://www.opengis.net/ont/geosparql#"),
    ("geosparql", "http://www.opengis.net/ont/geosparql#"),
    ("sosa", "http://www.w3.org/ns/sosa/"),
    ("ical", "http://www.w3.org/2002/12/cal/ical#"),
    ("oa", "http://www.w3.org/ns/oa#"),
    ("iiif", "http://iiif.io/api/presentation/3#"),
    ("exif", "http://www.w3.org/2003/12/exif/ns#"),
    ("wgs84", "http://www.w3.org/2003/01/geo/wgs84_pos#"),
    ("mads", "http://www.loc.gov/mads/rdf/v1#"),
    ("codemeta", "https://codemeta.github.io/terms/"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransformReportNative {
    pub base_nt: String,
    pub base_plus_derived_nt: String,
    pub gts_bytes: Vec<u8>,
    pub asserted: usize,
    pub saturated: usize,
    pub projected: usize,
    pub suppressed_dropped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellInput {
    pub iri: String,
    pub subject: String,
    pub predicate_curie: String,
    pub object: String,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedRowNative {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub reifier: String,
    pub annotations: Vec<(String, String)>,
}

/// A cell's IRIs are kept as plain strings (already validated as absolute IRIs by the
/// native term model when they enter the dataset).
#[derive(Debug, Clone)]
struct Cell {
    iri: String,
    subject: String,
    predicate_curie: String,
    object: String,
    confidence: String,
}

type EdgeMap = BTreeMap<String, Vec<Cell>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TripleKey(String, String, String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct AnnotationKey(String, String);

/// A derived triple. The subject is an IRI (a skolemized blank becomes a skolem IRI,
/// and every materialized/projected subject is an IRI), the predicate is an IRI, and
/// the object is any term.
#[derive(Debug, Clone)]
struct DerivedTriple {
    subject: RdfTerm,
    predicate: String,
    object: RdfTerm,
    annotations: BTreeSet<AnnotationKey>,
}

#[derive(Debug, Clone, Default)]
struct SuppressionVocab {
    bearer_props: Vec<String>,
    appellation_domain_props: BTreeSet<String>,
    appellation_classes: BTreeSet<String>,
    coarsen_guarded: BTreeSet<String>,
}

// ── Native flat triple store ─────────────────────────────────────────────────────

/// A transient, flat (un-folded) RDF store: a `Vec<RdfQuad>` plus a frozen
/// [`RdfDataset`] index over it for pattern queries. The oxigraph-free twin of the
/// transform's transient `oxigraph::store::Store`. Built once, then queried read-only.
///
/// The dataset is built with the FLAT codec ([`purrdf::flat_dataset_from_quads`]):
/// `rdf:reifies` / quoted-triple rows stay plain quads (no RDF 1.2 fold), exactly as
/// oxigraph's `Store` held them. The final GTS path re-folds via `parse_dataset`.
struct Graph {
    quads: Vec<RdfQuad>,
    ds: Arc<RdfDataset>,
}

impl Graph {
    fn from_quads(quads: Vec<RdfQuad>) -> gmeow_errors::Result<Self> {
        let ds = purrdf::flat_dataset_from_quads(&quads).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Transform {
                message: format!("flat dataset build failed: {e}"),
            })
        })?;
        Ok(Self { quads, ds })
    }

    fn id(&self, value: &TermValue) -> Option<TermId> {
        self.ds.term_id_by_value(value)
    }

    fn iri_id(&self, iri: &str) -> Option<TermId> {
        self.id(&TermValue::iri(iri))
    }

    fn len(&self) -> usize {
        self.quads.len()
    }

    /// Scan `(s?, p?, o?)` in the DEFAULT graph, yielding the resolved owned quads. The
    /// transform only ever queried the default graph.
    fn scan(&self, s: Option<TermId>, p: Option<TermId>, o: Option<TermId>) -> Vec<OwnedTriple> {
        let mut out = Vec::new();
        for q in self.ds.quads_for_pattern(s, p, o, GraphMatch::Default) {
            out.push(OwnedTriple {
                subject: resolve_term(&self.ds, q.s),
                predicate: resolve_term(&self.ds, q.p),
                object: resolve_term(&self.ds, q.o),
            });
        }
        out
    }

    /// All default-graph quads (subject/predicate/object resolved), in dataset order.
    fn default_triples(&self) -> Vec<OwnedTriple> {
        self.scan(None, None, None)
    }
}

/// A resolved default-graph triple, terms by value.
#[derive(Debug, Clone)]
struct OwnedTriple {
    subject: RdfTerm,
    predicate: RdfTerm,
    object: RdfTerm,
}

/// Resolve a dataset-local term id into an owned [`RdfTerm`].
fn resolve_term(ds: &RdfDataset, id: TermId) -> RdfTerm {
    match ds.resolve(id) {
        TermRef::Iri(iri) => RdfTerm::iri(iri.to_owned()),
        TermRef::Blank { label, scope } => {
            // A flat store's blank labels are scope-qualified; the transform inputs are
            // already canonicalized/skolemized so blanks never survive past
            // skolemization, but resolve faithfully if one appears.
            let _ = scope;
            RdfTerm::blank_node(label.to_owned())
        }
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => {
            let lit = if let Some(lang) = language {
                let mut l = RdfLiteral::language_tagged(lexical.to_owned(), lang.to_owned());
                l.direction = direction;
                l
            } else {
                let dt = ds.resolve(datatype);
                match dt {
                    TermRef::Iri(dt_iri) => {
                        RdfLiteral::typed(lexical.to_owned(), dt_iri.to_owned())
                    }
                    _ => RdfLiteral::simple(lexical.to_owned()),
                }
            };
            RdfTerm::literal(lit)
        }
        TermRef::Triple { s, p, o } => {
            let st = resolve_term(ds, s);
            let pt = resolve_term(ds, p);
            let ot = resolve_term(ds, o); // codespell:ignore ot
            let predicate = match pt {
                RdfTerm::Iri(iri) => iri,
                other => other.to_string(),
            };
            RdfTerm::triple(purrdf::RdfTriple::new(st, predicate, ot)) // codespell:ignore ot
        }
    }
}

/// Return the deterministic skolemized default graph as N-Triples.
pub fn skolemize_nt(raw_nt: &str) -> gmeow_errors::Result<String> {
    let graph = skolemized_graph(raw_nt)?;
    dump_nt(&graph)
}

/// Compute only E(G), returning row-shaped data for the Python compatibility API.
pub fn saturate_nt(
    abox_nt: &str,
    ontology_nt: &str,
    cells: &[CellInput],
    denied: &[(String, String, String)],
) -> gmeow_errors::Result<Vec<DerivedRowNative>> {
    let abox = parse_graph(abox_nt.as_bytes())?;
    let onto = parse_graph(ontology_nt.as_bytes())?;
    let cells = convert_cells(cells)?;
    let denied = denied.iter().cloned().collect();
    let vocab = suppression_vocab(&onto)?;
    let derived = saturate_graph(&abox, &onto, &cells, &denied, &vocab)?;
    Ok(derived_to_rows(&derived))
}

/// Compute MAXIMAL(G) over serialized inputs.
///
/// `tag_map` is the projection-boundary internal→public BCP-47 language-tag
/// remap (empty = no-op). It is applied to the base+derived quad stream — and
/// therefore to both `base_plus_derived_nt` and `gts_bytes` — before this
/// function returns: `saturate_graph`/`projection_derived` run their CONSTRUCTs
/// over `abox ∪ onto`, and a profile CONSTRUCT (e.g. the `ontolex` profile's
/// static exonym/endonym catalog) can copy an internally-tagged ontology
/// literal straight through to a derived triple. Without this retag, that
/// internal `x-gmeow-*` tag would leak into every consumer-facing MAXIMAL(G)
/// output regardless of source content — the exact class of regression the
/// self-sufficiency parity harness (`gmeow-cli/tests/self_sufficiency.rs`)
/// pins. Reuses [`crate::projections::retag_quads`], the SAME retag already
/// applied at the `project`/`export` projection boundaries (Principle 4: one
/// canonical source), rather than re-deriving the rule here.
pub fn transform_nt(
    raw_nt: &str,
    ontology_nt: &str,
    cells: &[CellInput],
    denied: &[(String, String, String)],
    projection_queries: &[(String, String)],
    tag_map: &TagMap,
) -> gmeow_errors::Result<TransformReportNative> {
    let mut abox = skolemized_graph(raw_nt)?;
    let onto = parse_graph(ontology_nt.as_bytes())?;
    let cells = convert_cells(cells)?;
    let denied = denied.iter().cloned().collect();
    let vocab = suppression_vocab(&onto)?;
    let suppressed = suppressed_nodes(&abox, &vocab)?;
    if !suppressed.is_empty() {
        abox = published_graph(&abox, &suppressed)?;
    }

    let saturated = saturate_graph(&abox, &onto, &cells, &denied, &vocab)?;
    let saturated_count = saturated.len();
    let projected = projection_derived(&abox, &onto, projection_queries, &suppressed)?;
    let projected_count = projected.len();
    let derived = merge_derived(saturated, projected);

    let base_nt = dump_nt(&abox)?;
    let base_plus_derived = base_plus_derived_graph(&abox, &derived, tag_map)?;
    let base_plus_derived_nt = dump_nt(&base_plus_derived)?;
    let gts_bytes = gts_from_maximal(&base_plus_derived, &derived)?;

    Ok(TransformReportNative {
        asserted: abox.len(),
        saturated: saturated_count,
        projected: projected_count,
        suppressed_dropped: suppressed.len(),
        base_nt,
        base_plus_derived_nt,
        gts_bytes,
    })
}

fn convert_cells(inputs: &[CellInput]) -> gmeow_errors::Result<Vec<Cell>> {
    inputs
        .iter()
        .map(|cell| {
            // A cell either records no confidence (legal — no annotation) or a
            // well-formed probability. A malformed value is a HARD FAIL: an
            // authored `gmeow:confidence` must be an `xsd:decimal` in [0.0, 1.0]
            // — never emitted verbatim into the derived triple's provenance.
            if !cell.confidence.is_empty()
                && crate::up_projection_corpus::decimal_confidence(&cell.confidence).is_none()
            {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Transform {
                    message: format!(
                        "cell {} carries a malformed gmeow:confidence {:?}: expected a decimal in [0.0, 1.0]",
                        cell.iri, cell.confidence
                    ),
                }));
            }
            Ok(Cell {
                iri: cell.iri.clone(),
                subject: cell.subject.clone(),
                predicate_curie: cell.predicate_curie.clone(),
                object: cell.object.clone(),
                confidence: cell.confidence.clone(),
            })
        })
        .collect()
}

fn skolemized_graph(raw_nt: &str) -> gmeow_errors::Result<Graph> {
    // Native full RDFC-1.0 (SHA-256) canonical N-Quads — the canonical `_:c14nN`
    // labels (hence the persisted skolem IRIs) are stable. We canonicalize the parsed
    // input then map each canonical blank to its deterministic skolem IRI.
    let parsed = purrdf::parse_dataset(
        raw_nt.as_bytes(),
        NativeRdfFormat::NTriples.media_type(),
        None,
    )
    .ctx("skolem input parse failed")?;
    let canon_nq = purrdf::canonical_flat_nquads(parsed.as_ref()).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Transform {
            message: format!("canonicalization failed: {e}"),
        })
    })?;
    let canon = purrdf::parse_dataset(
        canon_nq.as_bytes(),
        NativeRdfFormat::NQuads.media_type(),
        None,
    )
    .ctx("canonical re-parse failed")?;

    let flat = purrdf::flat_rdf_quads_from_dataset(canon.as_ref());
    let mut out: Vec<RdfQuad> = Vec::with_capacity(flat.len());
    for quad in &flat {
        let subject = skolem_term(&quad.subject)?;
        let object = skolem_term(&quad.object)?;
        let predicate = quad.predicate.clone();
        let graph_name = match &quad.graph_name {
            None => None,
            Some(RdfTerm::Iri(iri)) => Some(RdfTerm::iri(iri.clone())),
            Some(RdfTerm::BlankNode(label)) => Some(RdfTerm::iri(format!("{SKOLEM_BASE}{label}"))),
            Some(other) => Some(other.clone()),
        };
        let mut q = RdfQuad::new(subject, predicate, object);
        if let Some(g) = graph_name {
            q = q.in_graph(g);
        }
        out.push(q);
    }
    Graph::from_quads(out)
}

/// Map a term to its skolemized form: a blank node becomes the deterministic skolem
/// IRI `{SKOLEM_BASE}{label}`; every other term passes through. Recurses into quoted
/// triple components.
fn skolem_term(term: &RdfTerm) -> gmeow_errors::Result<RdfTerm> {
    Ok(match term {
        RdfTerm::Iri(iri) => RdfTerm::iri(iri.clone()),
        RdfTerm::BlankNode(label) => RdfTerm::iri(format!("{SKOLEM_BASE}{label}")),
        RdfTerm::Literal(lit) => RdfTerm::literal(lit.clone()),
        RdfTerm::Triple(triple) => RdfTerm::triple(purrdf::RdfTriple::new(
            skolem_term(&triple.subject)?,
            triple.predicate.clone(),
            skolem_term(&triple.object)?,
        )),
    })
}

fn build_strong_edges(
    cells: &[Cell],
    onto: &Graph,
    denied: &BTreeSet<(String, String, String)>,
) -> gmeow_errors::Result<(EdgeMap, EdgeMap)> {
    let mut class_edges: EdgeMap = BTreeMap::new();
    let mut property_edges: EdgeMap = BTreeMap::new();
    for cell in cells {
        let subject = cell.subject.as_str();
        if !subject.starts_with(GM) {
            continue;
        }
        let denial = (
            curie(subject),
            cell.predicate_curie.clone(),
            curie(cell.object.as_str()),
        );
        if denied.contains(&denial) {
            continue;
        }
        let is_class = has_type(onto, subject, OWL_CLASS);
        let mut is_property = false;
        for kind in [
            OWL_OBJECT_PROPERTY,
            OWL_DATATYPE_PROPERTY,
            OWL_ANNOTATION_PROPERTY,
        ] {
            if has_type(onto, subject, kind) {
                is_property = true;
                break;
            }
        }
        if is_class && STRONG_CLASS_PREDICATES.contains(&cell.predicate_curie.as_str()) {
            class_edges
                .entry(subject.to_owned())
                .or_default()
                .push(cell.clone());
        } else if is_property && STRONG_PROPERTY_PREDICATES.contains(&cell.predicate_curie.as_str())
        {
            property_edges
                .entry(subject.to_owned())
                .or_default()
                .push(cell.clone());
        }
    }
    Ok((class_edges, property_edges))
}

fn saturate_graph(
    abox: &Graph,
    onto: &Graph,
    cells: &[Cell],
    denied: &BTreeSet<(String, String, String)>,
    vocab: &SuppressionVocab,
) -> gmeow_errors::Result<BTreeMap<TripleKey, DerivedTriple>> {
    let (class_edges, property_edges) = build_strong_edges(cells, onto, denied)?;
    let suppressed = suppressed_nodes(abox, vocab)?;
    let mut derived: BTreeMap<TripleKey, DerivedTriple> = BTreeMap::new();

    if let Some(rdf_type) = abox.iri_id(RDF_TYPE) {
        for q in abox.scan(None, Some(rdf_type), None) {
            if suppressed.contains(&term_token(&q.subject)) {
                continue;
            }
            let RdfTerm::Iri(cls) = &q.object else {
                continue;
            };
            if let Some(edge_cells) = class_edges.get(cls.as_str()) {
                for cell in edge_cells {
                    emit_derived(
                        &mut derived,
                        abox,
                        q.subject.clone(),
                        RDF_TYPE.to_owned(),
                        RdfTerm::iri(cell.object.clone()),
                        cell_annotations(cell),
                    )?;
                }
            }
        }
    }

    for (prop, edge_cells) in &property_edges {
        let Some(pred) = abox.iri_id(prop) else {
            continue;
        };
        for q in abox.scan(None, Some(pred), None) {
            if suppressed.contains(&term_token(&q.subject))
                || suppressed.contains(&term_token(&q.object))
            {
                continue;
            }
            if vocab.coarsen_guarded.contains(prop) && has_any(abox, &q.subject, GM_COARSEN_TO) {
                continue;
            }
            for cell in edge_cells {
                emit_derived(
                    &mut derived,
                    abox,
                    q.subject.clone(),
                    cell.object.clone(),
                    q.object.clone(),
                    cell_annotations(cell),
                )?;
            }
        }
    }

    if let Some(same_as) = abox.iri_id(OWL_SAME_AS) {
        for q in abox.scan(None, Some(same_as), None) {
            if suppressed.contains(&term_token(&q.subject))
                || suppressed.contains(&term_token(&q.object))
            {
                continue;
            }
            emit_derived(
                &mut derived,
                abox,
                q.subject,
                SCHEMA_SAME_AS.to_owned(),
                q.object,
                vec![AnnotationKey(
                    GM_MAPPED_FROM.to_owned(),
                    format!("<{SAME_AS_MIRROR_RULE}>"),
                )],
            )?;
        }
    }

    Ok(derived)
}

fn emit_derived(
    derived: &mut BTreeMap<TripleKey, DerivedTriple>,
    abox: &Graph,
    subject: RdfTerm,
    predicate: String,
    object: RdfTerm,
    annotations: Vec<AnnotationKey>,
) -> gmeow_errors::Result<()> {
    if contains_triple(abox, &subject, &predicate, &object) {
        return Ok(());
    }
    let key = triple_key(&subject, &predicate, &object);
    let row = derived.entry(key).or_insert_with(|| DerivedTriple {
        subject,
        predicate,
        object,
        annotations: BTreeSet::new(),
    });
    row.annotations.extend(annotations);
    Ok(())
}

fn cell_annotations(cell: &Cell) -> Vec<AnnotationKey> {
    let mut rows = vec![AnnotationKey(
        GM_MAPPED_FROM.to_owned(),
        format!("<{}>", cell.iri),
    )];
    if !cell.confidence.is_empty() {
        let lit = RdfTerm::literal(RdfLiteral::typed(cell.confidence.clone(), XSD_DECIMAL));
        rows.push(AnnotationKey(GM_CONFIDENCE.to_owned(), term_token(&lit)));
    }
    rows
}

fn projection_derived(
    abox: &Graph,
    onto: &Graph,
    projection_queries: &[(String, String)],
    suppressed: &BTreeSet<String>,
) -> gmeow_errors::Result<BTreeMap<TripleKey, DerivedTriple>> {
    let projection_input = projection_input_graph(abox, onto)?;
    let onto_subjects = subjects(onto);
    // Strip the `<`/`>` brackets off every onto subject ONCE, up front: the
    // per-quad `derives_from_onto_subject` check below is called once per
    // projection-result quad (potentially thousands, across every projection
    // query), and re-deriving these unbracketed IRIs from `onto_subjects` on
    // every call would be O(quads × onto_subjects) redundant string work.
    let onto_subject_iris: Vec<&str> = onto_subjects
        .iter()
        .filter_map(|s| s.strip_prefix('<').and_then(|s| s.strip_suffix('>')))
        .collect();
    let mut derived: BTreeMap<TripleKey, DerivedTriple> = BTreeMap::new();
    let engine = NativeSparqlEngine::new();
    for (name, query) in projection_queries {
        let alignment = format!("{GM}projections/{name}");
        let result = engine
            .query(
                &projection_input.ds,
                SparqlRequest {
                    query,
                    base_iri: None,
                    substitutions: &[],
                },
            )
            .with_ctx(|| format!("projection query evaluation failed for {name}"))?;
        let SparqlResult::Graph(triples) = result else {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Transform {
                message: format!("projection query {name} did not return a graph"),
            }));
        };
        for quad in triples.owned_quads() {
            let subject = quad.subject;
            let object = quad.object;
            // CONSTRUCT predicates are always IRIs; `RdfQuad::predicate` carries the IRI.
            let predicate = quad.predicate;
            let subject_token = term_token(&subject);
            if onto_subjects.contains(&subject_token)
                || derives_from_onto_subject(&subject_token, &onto_subject_iris)
            {
                continue;
            }
            if suppressed.contains(&term_token(&subject))
                || suppressed.contains(&term_token(&object))
            {
                continue;
            }
            emit_derived(
                &mut derived,
                abox,
                subject,
                predicate,
                object,
                vec![AnnotationKey(
                    GM_MAPPED_FROM.to_owned(),
                    format!("<{alignment}>"),
                )],
            )?;
        }
    }
    Ok(derived)
}

fn projection_input_graph(abox: &Graph, onto: &Graph) -> gmeow_errors::Result<Graph> {
    let mut quads: Vec<RdfQuad> = Vec::new();
    quads.extend(default_quads(onto));
    quads.extend(default_quads(abox));
    for triple in materialized_claims(abox) {
        quads.push(triple);
    }
    Graph::from_quads(quads)
}

fn materialized_claims(abox: &Graph) -> Vec<RdfQuad> {
    let mut out = Vec::new();
    for cell in subjects_for_type(abox, GM_STATEMENT_METADATA) {
        let Some(s_term) = value(abox, &cell, GM_Q_SUBJECT) else {
            continue;
        };
        let Some(p_term) = value(abox, &cell, GM_Q_PREDICATE) else {
            continue;
        };
        let mut o_term = value(abox, &cell, GM_Q_OBJECT);
        if o_term.is_none() {
            o_term = value(abox, &cell, GM_Q_OBJECT_LITERAL);
        }
        let Some(o_term) = o_term else {
            continue;
        };
        if !is_subject_term(&s_term) {
            continue;
        }
        let RdfTerm::Iri(predicate) = p_term else {
            continue;
        };
        out.push(RdfQuad::new(s_term, predicate, o_term));
    }
    out
}

fn merge_derived(
    saturated: BTreeMap<TripleKey, DerivedTriple>,
    projected: BTreeMap<TripleKey, DerivedTriple>,
) -> BTreeMap<TripleKey, DerivedTriple> {
    let mut merged = saturated;
    for (key, row) in projected {
        let target = merged.entry(key).or_insert_with(|| DerivedTriple {
            subject: row.subject,
            predicate: row.predicate,
            object: row.object,
            annotations: BTreeSet::new(),
        });
        target.annotations.extend(row.annotations);
    }
    merged
}

fn base_plus_derived_graph(
    base: &Graph,
    derived: &BTreeMap<TripleKey, DerivedTriple>,
    tag_map: &TagMap,
) -> gmeow_errors::Result<Graph> {
    let mut quads: Vec<RdfQuad> = default_quads(base);
    for row in derived.values() {
        quads.push(RdfQuad::new(
            row.subject.clone(),
            row.predicate.clone(),
            row.object.clone(),
        ));
    }
    retag_quads(&mut quads, tag_map);
    Graph::from_quads(quads)
}

fn gts_from_maximal(
    base_plus_derived: &Graph,
    derived: &BTreeMap<TripleKey, DerivedTriple>,
) -> gmeow_errors::Result<Vec<u8>> {
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    // Native carrier ingestion: serialize the default graph to N-Triples and
    // parse it into a frozen dataset, then fold it in. The native parse folds any RDF
    // 1.2 statement layer into the dataset's reifier/annotation side-tables, so a
    // single `add_dataset` reproduces the old `add_quads` + `add_rdf12` split.
    let base_nt = dump_nt(base_plus_derived)?;
    let base_dataset = purrdf::parse_dataset(base_nt.as_bytes(), "application/n-triples", None)?;
    builder
        .add_dataset(&base_dataset)
        .map_err(|message| gmeow_errors::Diag::of_kind(crate::error::Transform { message }))?;
    let statement_nt = statement_layer_nt(derived);
    if !statement_nt.trim().is_empty() {
        let statement_dataset =
            purrdf::parse_dataset(statement_nt.as_bytes(), "application/n-triples", None)?;
        builder
            .add_dataset(&statement_dataset)
            .map_err(|message| gmeow_errors::Diag::of_kind(crate::error::Transform { message }))?;
    }
    gmeow_gts_profile::emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
}

fn statement_layer_nt(derived: &BTreeMap<TripleKey, DerivedTriple>) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for row in derived.values() {
        let reifier = reifier_for(&row.subject, &row.predicate, &row.object);
        let _ = writeln!(
            &mut out,
            "<{reifier}> <{RDF_REIFIES}> <<( {} <{}> {} )>> .",
            term_token(&row.subject),
            row.predicate,
            term_token(&row.object)
        );
        for ann in &row.annotations {
            let _ = writeln!(&mut out, "<{reifier}> <{}> {} .", ann.0, ann.1);
        }
    }
    out
}

fn derived_to_rows(derived: &BTreeMap<TripleKey, DerivedTriple>) -> Vec<DerivedRowNative> {
    derived
        .values()
        .map(|row| DerivedRowNative {
            subject: term_token(&row.subject),
            predicate: row.predicate.clone(),
            object: term_token(&row.object),
            reifier: reifier_for(&row.subject, &row.predicate, &row.object),
            annotations: row
                .annotations
                .iter()
                .map(|ann| (ann.0.clone(), ann.1.clone()))
                .collect(),
        })
        .collect()
}

fn published_graph(abox: &Graph, suppressed: &BTreeSet<String>) -> gmeow_errors::Result<Graph> {
    let mut quads: Vec<RdfQuad> = Vec::new();
    for q in abox.default_triples() {
        if !suppressed.contains(&term_token(&q.subject))
            && !suppressed.contains(&term_token(&q.object))
        {
            quads.push(RdfQuad::new(
                q.subject,
                predicate_iri(&q.predicate),
                q.object,
            ));
        }
    }
    Graph::from_quads(quads)
}

fn suppression_vocab(onto: &Graph) -> gmeow_errors::Result<SuppressionVocab> {
    let classes = subclass_closure(onto, GM_APPELLATION);
    let mut bearer: BTreeSet<String> = BTreeSet::new();
    for (prop, rng) in subject_objects(onto, RDFS_RANGE) {
        if classes.contains(&term_token(&rng))
            && let Some(prop) = subject_iri(&prop)
        {
            bearer.insert(prop);
        }
    }
    let mut domain_props: BTreeSet<String> = BTreeSet::new();
    for (prop, dom) in subject_objects(onto, RDFS_DOMAIN) {
        if classes.contains(&term_token(&dom))
            && let Some(prop) = subject_iri(&prop)
        {
            domain_props.insert(prop);
        }
    }
    let mut coarsen_guarded = BTreeSet::new();
    for (sub, obj) in subject_objects(onto, GM_COARSEN_GUARDED) {
        if matches!(&obj, RdfTerm::Literal(lit) if lit.lexical_form == "true")
            && let Some(prop) = subject_iri(&sub)
        {
            coarsen_guarded.insert(prop);
        }
    }
    Ok(SuppressionVocab {
        bearer_props: bearer.into_iter().collect(),
        appellation_domain_props: domain_props,
        appellation_classes: classes,
        coarsen_guarded,
    })
}

fn suppressed_nodes(
    abox: &Graph,
    vocab: &SuppressionVocab,
) -> gmeow_errors::Result<BTreeSet<String>> {
    let mut suppressed = BTreeSet::new();
    for (subject, object) in subject_objects(abox, GM_DISPLAYABLE) {
        if matches!(&object, RdfTerm::Literal(lit) if lit.lexical_form == "false" || lit.lexical_form == "0")
        {
            suppressed.insert(term_token(&subject));
        }
    }
    if suppressed.is_empty() {
        return Ok(suppressed);
    }

    let mut appellations = BTreeSet::new();
    for cls in &vocab.appellation_classes {
        for subject in subjects_for_type_token(abox, cls) {
            appellations.insert(subject);
        }
    }
    for prop in &vocab.appellation_domain_props {
        for (subject, _object) in subject_objects(abox, prop) {
            appellations.insert(term_token(&subject));
        }
    }

    let mut extra = BTreeSet::new();
    for prop in &vocab.bearer_props {
        for (bearer, appellation) in subject_objects(abox, prop) {
            if suppressed.contains(&term_token(&bearer))
                && appellations.contains(&term_token(&appellation))
            {
                extra.insert(term_token(&appellation));
            }
        }
    }
    suppressed.extend(extra);
    Ok(suppressed)
}

/// `graph` here is `onto`/`ontology_nt` — `ontology/gmeow.ttl` ⊕ every slice
/// `module.ttl`, parsed directly from the committed AUTHORED sources (see
/// `ontology_source_files` in `crates/pipeline/src/scoreboards.rs`), never a
/// lowered `rdfs:`-only projection. So this closure must scan both the canonical
/// `logic:subClassOf` edge and its `rdfs:` projection (gmeow_ns::SUB_CLASS_OF
/// doctrine; crates/ns/src/lib.rs:106-166) or a re-authored Appellation subclass
/// silently drops out of the suppression vocabulary.
fn subclass_closure(graph: &Graph, root: &str) -> BTreeSet<String> {
    let mut closure = BTreeSet::new();
    closure.insert(format!("<{root}>"));
    let mut edges: Vec<(RdfTerm, RdfTerm)> = Vec::new();
    for predicate in gmeow_ns::SUB_CLASS_OF {
        edges.extend(subject_objects(graph, predicate));
    }
    loop {
        let mut grew = false;
        for (sub, sup) in &edges {
            if closure.contains(&term_token(sup)) && !closure.contains(&term_token(sub)) {
                closure.insert(term_token(sub));
                grew = true;
            }
        }
        if !grew {
            return closure;
        }
    }
}

fn subjects_for_type(graph: &Graph, class: &str) -> Vec<RdfTerm> {
    let (Some(rdf_type), Some(cls)) = (graph.iri_id(RDF_TYPE), graph.iri_id(class)) else {
        return Vec::new();
    };
    graph
        .scan(None, Some(rdf_type), Some(cls))
        .into_iter()
        .map(|q| q.subject)
        .collect()
}

fn subjects_for_type_token(graph: &Graph, class_token: &str) -> Vec<String> {
    let Ok(class) = iri_from_token(class_token) else {
        return Vec::new();
    };
    subjects_for_type(graph, &class)
        .iter()
        .map(term_token)
        .collect()
}

fn subject_objects(graph: &Graph, predicate: &str) -> Vec<(RdfTerm, RdfTerm)> {
    let Some(pred) = graph.iri_id(predicate) else {
        return Vec::new();
    };
    graph
        .scan(None, Some(pred), None)
        .into_iter()
        .map(|q| (q.subject, q.object))
        .collect()
}

fn value(graph: &Graph, subject: &RdfTerm, predicate: &str) -> Option<RdfTerm> {
    let subj_id = graph.id(&term_value(subject))?;
    let pred = graph.iri_id(predicate)?;
    graph
        .scan(Some(subj_id), Some(pred), None)
        .into_iter()
        .next()
        .map(|q| q.object)
}

fn subjects(graph: &Graph) -> BTreeSet<String> {
    graph
        .default_triples()
        .iter()
        .map(|q| term_token(&q.subject))
        .collect()
}

/// Whether `subject_token` (an already-bracketed IRI token, e.g.
/// `<https://…/appAbkhazianExonym-form>`) is a FRESHLY MINTED individual derived
/// from an onto-only subject — i.e. its IRI is a proper string extension of some
/// `onto_subjects` entry.
///
/// Closes a coverage gap in the exact-match guard `onto_subjects.contains(..)`:
/// that guard only catches a projection CONSTRUCT that re-emits an onto
/// subject's IRI verbatim, not one that mints a fresh sub-resource IRI off it
/// (the common `BIND(IRI(CONCAT(STR(?onto_individual), "-form")))` idiom several
/// projection profiles use, e.g. `ontolex`'s per-Appellation lexical Form). Both
/// guards exist for the SAME reason: `MAXIMAL(G) = G + E(G) + P(G)` projects
/// facts *about the instance data*, so a CONSTRUCT result whose driving
/// individual is purely ontology/reference-catalog content (never in `abox`)
/// is NOT a derived fact about the transpiled instance — without this check, a
/// profile whose WHERE clause matches static ontology-authored reference data
/// (e.g. the imported `imports/languages-reference.ttl` catalog) leaks that
/// ENTIRE static catalog into every MAXIMAL(G) transform, regardless of the
/// actual instance content.
fn derives_from_onto_subject(subject_token: &str, onto_subject_iris: &[&str]) -> bool {
    // Tokens are bracketed IRIs (`<https://…>`); compare the INNER IRI text, not
    // the bracketed token — `<...Exonym-form>` does not `starts_with`
    // `<...Exonym>` (the closing `>` breaks the naive bracketed prefix match),
    // but the inner IRI `...Exonym-form` does start with `...Exonym`. Callers
    // pre-strip the brackets off `onto_subject_iris` ONCE (this runs once per
    // projection-result quad, so re-deriving the onto side's unbracketed IRIs
    // on every call would be redundant O(quads × onto_subjects) string work).
    let Some(subject_iri) = subject_token
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
    else {
        return false;
    };
    onto_subject_iris
        .iter()
        .any(|onto_iri| onto_iri.len() < subject_iri.len() && subject_iri.starts_with(onto_iri))
}

fn has_type(graph: &Graph, subject: &str, class: &str) -> bool {
    let (Some(s), Some(p), Some(o)) = (
        graph.iri_id(subject),
        graph.iri_id(RDF_TYPE),
        graph.iri_id(class),
    ) else {
        return false;
    };
    !graph.scan(Some(s), Some(p), Some(o)).is_empty()
}

fn has_any(graph: &Graph, subject: &RdfTerm, predicate: &str) -> bool {
    let (Some(s), Some(p)) = (graph.id(&term_value(subject)), graph.iri_id(predicate)) else {
        return false;
    };
    !graph.scan(Some(s), Some(p), None).is_empty()
}

fn contains_triple(graph: &Graph, subject: &RdfTerm, predicate: &str, object: &RdfTerm) -> bool {
    let (Some(s), Some(p), Some(o)) = (
        graph.id(&term_value(subject)),
        graph.iri_id(predicate),
        graph.id(&term_value(object)),
    ) else {
        return false;
    };
    !graph.scan(Some(s), Some(p), Some(o)).is_empty()
}

/// The default-graph quads of `graph`, as owned `RdfQuad`s (no graph name).
fn default_quads(graph: &Graph) -> Vec<RdfQuad> {
    graph
        .default_triples()
        .into_iter()
        .map(|q| RdfQuad::new(q.subject, predicate_iri(&q.predicate), q.object))
        .collect()
}

fn dump_nt(graph: &Graph) -> gmeow_errors::Result<String> {
    // Serialize the FLAT default graph to N-Triples, matching oxigraph's
    // `dump_graph_to_writer(DefaultGraph, NTriples)`: every default-graph quad as a
    // single `s p o .` line, in canonical dataset order.
    let flat = purrdf::flat_dataset_from_quads(&default_quads(graph)).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Transform {
            message: format!("N-Triples flatten failed: {e}"),
        })
    })?;
    let bytes = purrdf::serialize_dataset(
        flat.as_ref(),
        NativeRdfFormat::NTriples.media_type(),
        purrdf::SerializeGraph::DefaultGraph,
    )
    .ctx("N-Triples serialization failed")?;
    String::from_utf8(bytes).ctx("N-Triples output is not UTF-8")
}

fn subject_iri(subject: &RdfTerm) -> Option<String> {
    match subject {
        RdfTerm::Iri(n) => Some(n.clone()),
        _ => None,
    }
}

/// Whether a term can stand in subject position (IRI or blank node).
fn is_subject_term(term: &RdfTerm) -> bool {
    matches!(term, RdfTerm::Iri(_) | RdfTerm::BlankNode(_))
}

/// The IRI of a predicate term (predicates are always IRIs).
fn predicate_iri(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => iri.clone(),
        other => other.to_string(),
    }
}

/// The dataset-independent value of a term, for `term_id_by_value` lookups.
fn term_value(term: &RdfTerm) -> TermValue {
    match term {
        RdfTerm::Iri(iri) => TermValue::iri(iri.clone()),
        RdfTerm::BlankNode(label) => TermValue::blank(label.clone()),
        RdfTerm::Literal(lit) => literal_value(lit),
        RdfTerm::Triple(triple) => TermValue::Triple {
            s: Box::new(term_value(&triple.subject)),
            p: Box::new(TermValue::iri(triple.predicate.clone())),
            o: Box::new(term_value(&triple.object)),
        },
    }
}

fn literal_value(lit: &RdfLiteral) -> TermValue {
    if let Some(lang) = &lit.language {
        TermValue::lang_literal(lit.lexical_form.clone(), lang)
    } else if let Some(dt) = &lit.datatype {
        TermValue::typed_literal(lit.lexical_form.clone(), dt.clone())
    } else {
        TermValue::simple_literal(lit.lexical_form.clone())
    }
}

/// The N-Triples token of a term — `<iri>` / `_:label` / a typed/lang literal —
/// byte-identical to oxigraph's `Term::to_string()` for the terms this kernel handles
/// (the native renderer is the single source of truth, [`purrdf::RdfTerm`]'s
/// `Display`). This feeds the content-addressed reifier hash and the statement-layer
/// N-Triples, so its byte stability is reasoning-critical.
fn term_token(term: &RdfTerm) -> String {
    term.to_string()
}

fn triple_key(subject: &RdfTerm, predicate: &str, object: &RdfTerm) -> TripleKey {
    TripleKey(
        term_token(subject),
        format!("<{predicate}>"),
        term_token(object),
    )
}

fn reifier_for(subject: &RdfTerm, predicate: &str, object: &RdfTerm) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        format!(
            "{}|<{}>|{}",
            term_token(subject),
            predicate,
            term_token(object)
        )
        .as_bytes(),
    );
    let digest = hasher.finalize();
    format!(
        "{GM}derivations/{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7]
    )
}

fn curie(iri: &str) -> String {
    static SORTED_PREFIXES: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    let prefixes = SORTED_PREFIXES.get_or_init(|| {
        let mut prefixes = PREFIXES.to_vec();
        prefixes.sort_by_key(|(_, ns)| std::cmp::Reverse(ns.len()));
        prefixes
    });
    for (prefix, ns) in prefixes {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_owned()
}

fn iri_from_token(token: &str) -> gmeow_errors::Result<String> {
    token
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .map(str::to_owned)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Transform {
                message: format!("expected IRI token, got {token:?}"),
            })
        })
}

fn parse_graph(data: &[u8]) -> gmeow_errors::Result<Graph> {
    let parsed = purrdf::parse_dataset(data, NativeRdfFormat::NTriples.media_type(), None)
        .ctx("RDF parse failed")?;
    // Un-fold to the flat quad stream so `rdf:reifies` / quoted-triple rows stay plain
    // quads (the old oxigraph `Store` held them flat), then re-freeze flat.
    let flat = purrdf::flat_rdf_quads_from_dataset(parsed.as_ref());
    Graph::from_quads(flat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reifier_hash_matches_python_contract() {
        let s = RdfTerm::iri("https://example.org/s");
        let p = "https://example.org/p";
        let o = RdfTerm::iri("https://example.org/o");
        assert_eq!(
            reifier_for(&s, p, &o),
            "https://blackcatinformatics.ca/gmeow/derivations/bc5c0b0074e06845"
        );
    }

    /// G9 canonical-subsumption sweep: `subclass_closure` reads `onto` — the
    /// AUTHORED `ontology/gmeow.ttl` ⊕ slice `module.ttl` merge (see
    /// `ontology_source_files` in `crates/pipeline/src/scoreboards.rs`), never a
    /// lowered `rdfs:`-only projection. It must traverse the canonical
    /// `logic:subClassOf` edge, not only its `rdfs:` projection (gmeow_ns::SUB_CLASS_OF
    /// doctrine; crates/ns/src/lib.rs:106-166), or a re-authored Appellation
    /// subclass silently drops out of the suppression vocabulary.
    #[test]
    fn subclass_closure_traverses_canonical_logic_subclass_of() {
        const GM_APPELLATION_LOCAL: &str = "https://blackcatinformatics.ca/gmeow/Appellation";
        const GM_PERSON_NAME: &str = "https://blackcatinformatics.ca/gmeow/PersonName";
        let nt = format!(
            "<{GM_PERSON_NAME}> <https://blackcatinformatics.ca/logic/subClassOf> <{GM_APPELLATION_LOCAL}> .\n"
        );
        let graph = parse_graph(nt.as_bytes()).expect("fixture must parse");
        let closure = subclass_closure(&graph, GM_APPELLATION_LOCAL);
        assert!(
            closure.contains(&format!("<{GM_PERSON_NAME}>")),
            "subclass_closure must traverse the canonical logic:subClassOf edge: {closure:?}"
        );
    }

    #[test]
    fn curie_prefers_longest_namespace() {
        assert_eq!(
            curie("http://id.loc.gov/ontologies/bibframe/Work"),
            "bf:Work"
        );
    }

    // ── Equivalence saturation E(G): strong-only, lint-gated, suppression-safe ──
    //
    // These reproduce the saturation-engine scenarios over hermetic, minimal
    // N-Triples inputs — no repo ontology, DSL, or fixture files. `saturate_nt`
    // is the engine under test; the fixtures below exercise every branch of
    // `build_strong_edges` / `saturate_graph` / `emit_derived` / `cell_annotations`.

    const GM_PERSON: &str = "https://blackcatinformatics.ca/gmeow/Person";
    const GM_CORPUS: &str = "https://blackcatinformatics.ca/gmeow/Corpus";
    const SCHEMA_PERSON: &str = "https://schema.org/Person";
    const SCHEMA_DATASET: &str = "https://schema.org/Dataset";
    const FOAF_PERSON: &str = "http://xmlns.com/foaf/0.1/Person";
    const WD_Q42: &str = "http://www.wikidata.org/entity/Q42";
    const EX_ME: &str = "https://example.org/sat/me";
    const EX_CORPUS: &str = "https://example.org/sat/corpus";
    const EX_SUPPRESSED: &str = "https://example.org/sat/suppressed";
    const EX_CONTROL: &str = "https://example.org/sat/control";
    const PERSON_SCHEMA_CELL: &str = "https://blackcatinformatics.ca/gmeow/te/person-schema";
    const PERSON_FOAF_CELL: &str = "https://blackcatinformatics.ca/gmeow/te/person-foaf";
    const GM_KNOWS: &str = "https://blackcatinformatics.ca/gmeow/knows";
    const FOAF_KNOWS: &str = "http://xmlns.com/foaf/0.1/knows";
    const KNOWS_FOAF_CELL: &str = "https://blackcatinformatics.ca/gmeow/te/knows-foaf";
    const EX_A: &str = "https://example.org/sat/a";
    const EX_B: &str = "https://example.org/sat/b";
    const EX_C: &str = "https://example.org/sat/c";
    const EX_D: &str = "https://example.org/sat/d";

    fn cell(
        iri: &str,
        subject: &str,
        predicate_curie: &str,
        object: &str,
        confidence: &str,
    ) -> CellInput {
        CellInput {
            iri: iri.to_owned(),
            subject: subject.to_owned(),
            predicate_curie: predicate_curie.to_owned(),
            object: object.to_owned(),
            confidence: confidence.to_owned(),
        }
    }

    /// One N-Triples statement with an IRI object.
    fn nt(subject: &str, predicate: &str, object: &str) -> String {
        format!("<{subject}> <{predicate}> <{object}> .\n")
    }

    /// The minimal ontology every class-edge scenario needs: `gmeow:Person a owl:Class`.
    fn person_onto() -> String {
        nt(GM_PERSON, RDF_TYPE, OWL_CLASS)
    }

    /// A single `gmeow:Person` instance.
    fn person_abox() -> String {
        nt(EX_ME, RDF_TYPE, GM_PERSON)
    }

    /// Two strong class edges for `gmeow:Person`: one via `owl:equivalentClass`
    /// (confidence 0.9), one via `skos:exactMatch` (confidence 0.8).
    fn person_cells() -> Vec<CellInput> {
        vec![
            cell(
                PERSON_SCHEMA_CELL,
                GM_PERSON,
                "owl:equivalentClass",
                SCHEMA_PERSON,
                "0.9",
            ),
            cell(
                PERSON_FOAF_CELL,
                GM_PERSON,
                "skos:exactMatch",
                FOAF_PERSON,
                "0.8",
            ),
        ]
    }

    /// The minimal ontology a property-edge scenario needs: `gmeow:knows a owl:ObjectProperty`.
    fn knows_onto() -> String {
        nt(GM_KNOWS, RDF_TYPE, OWL_OBJECT_PROPERTY)
    }

    /// One strong property edge: `gmeow:knows owl:equivalentProperty foaf:knows`.
    fn knows_cells() -> Vec<CellInput> {
        vec![cell(
            KNOWS_FOAF_CELL,
            GM_KNOWS,
            "owl:equivalentProperty",
            FOAF_KNOWS,
            "0.9",
        )]
    }

    fn iri_token(iri: &str) -> String {
        format!("<{iri}>")
    }

    fn type_objects(rows: &[DerivedRowNative]) -> BTreeSet<String> {
        rows.iter()
            .filter(|r| r.predicate == RDF_TYPE)
            .map(|r| r.object.clone())
            .collect()
    }

    #[test]
    fn saturate_materializes_all_strong_class_edges() {
        // gmeow:Person saturates to every strong external equivalent at once.
        let rows = saturate_nt(&person_abox(), &person_onto(), &person_cells(), &[]).unwrap();
        assert_eq!(
            type_objects(&rows),
            BTreeSet::from([iri_token(SCHEMA_PERSON), iri_token(FOAF_PERSON)]),
        );
    }

    #[test]
    fn saturate_ignores_close_match_hints() {
        // gmeow:Corpus has ONLY a closeMatch cell — a hint must not become a fact.
        let onto = nt(GM_CORPUS, RDF_TYPE, OWL_CLASS);
        let corpus_cell = "https://blackcatinformatics.ca/gmeow/te/corpus-dataset";
        let cells = vec![cell(
            corpus_cell,
            GM_CORPUS,
            "skos:closeMatch",
            SCHEMA_DATASET,
            "0.5",
        )];
        let abox = nt(EX_CORPUS, RDF_TYPE, GM_CORPUS);
        let rows = saturate_nt(&abox, &onto, &cells, &[]).unwrap();
        assert!(
            rows.is_empty(),
            "closeMatch must never materialize: {rows:?}"
        );

        // Positive control (non-vacuous): the SAME fixture with a STRONG
        // predicate DOES materialize — proving the empty result above is
        // closeMatch filtering, not a broken/inert fixture.
        let strong = vec![cell(
            corpus_cell,
            GM_CORPUS,
            "owl:equivalentClass",
            SCHEMA_DATASET,
            "0.5",
        )];
        let control = saturate_nt(&abox, &onto, &strong, &[]).unwrap();
        assert_eq!(
            type_objects(&control),
            BTreeSet::from([iri_token(SCHEMA_DATASET)]),
            "strong predicate over the same fixture must materialize"
        );
    }

    #[test]
    fn saturate_refuses_denied_cell_keeps_siblings() {
        // A lint-ERROR row (the denial key is the CURIE triple) emits nothing;
        // the sibling strong edge is untouched.
        let denied = vec![(
            "gmeow:Person".to_owned(),
            "owl:equivalentClass".to_owned(),
            "schema:Person".to_owned(),
        )];
        let rows = saturate_nt(&person_abox(), &person_onto(), &person_cells(), &denied).unwrap();
        let types = type_objects(&rows);
        assert!(
            !types.contains(&iri_token(SCHEMA_PERSON)),
            "denied edge leaked"
        );
        assert!(types.contains(&iri_token(FOAF_PERSON)), "sibling edge lost");
    }

    #[test]
    fn saturate_drops_suppressed_nodes_keeps_control() {
        // A displayable-false node never saturates; its control twin does (non-vacuous).
        let cells = vec![cell(
            PERSON_SCHEMA_CELL,
            GM_PERSON,
            "owl:equivalentClass",
            SCHEMA_PERSON,
            "0.9",
        )];
        let mut abox = String::new();
        abox.push_str(&nt(EX_SUPPRESSED, RDF_TYPE, GM_PERSON));
        abox.push_str(&format!(
            "<{EX_SUPPRESSED}> <{GM_DISPLAYABLE}> \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean> .\n"
        ));
        abox.push_str(&nt(EX_CONTROL, RDF_TYPE, GM_PERSON));
        let rows = saturate_nt(&abox, &person_onto(), &cells, &[]).unwrap();
        let subjects: BTreeSet<String> = rows.iter().map(|r| r.subject.clone()).collect();
        assert!(
            !subjects.contains(&iri_token(EX_SUPPRESSED)),
            "suppressed node saturated"
        );
        assert!(
            subjects.contains(&iri_token(EX_CONTROL)),
            "control twin missing"
        );
    }

    #[test]
    fn saturate_mirrors_same_as_to_schema() {
        // owl:sameAs external links mirror to schema:sameAs, rule-attributed.
        let abox = nt(EX_ME, OWL_SAME_AS, WD_Q42);
        let rows = saturate_nt(&abox, &person_onto(), &[], &[]).unwrap();
        let mirrors: Vec<&DerivedRowNative> = rows
            .iter()
            .filter(|r| r.predicate == SCHEMA_SAME_AS)
            .collect();
        assert_eq!(mirrors.len(), 1);
        assert_eq!(mirrors[0].subject, iri_token(EX_ME));
        assert_eq!(mirrors[0].object, iri_token(WD_Q42));
        assert!(
            mirrors[0]
                .annotations
                .contains(&(GM_MAPPED_FROM.to_owned(), iri_token(SAME_AS_MIRROR_RULE)))
        );
    }

    #[test]
    fn saturate_mirrors_strong_property_edge() {
        // A strong equivalentProperty cell mirrors <a> gmeow:knows <b> to
        // <a> foaf:knows <b>, carrying the object through, cell-attributed.
        let abox = nt(EX_A, GM_KNOWS, EX_B);
        let rows = saturate_nt(&abox, &knows_onto(), &knows_cells(), &[]).unwrap();
        assert_eq!(rows.len(), 1, "exactly the one mirrored edge: {rows:?}");
        let mirror = &rows[0];
        assert_eq!(mirror.predicate, FOAF_KNOWS);
        assert_eq!(mirror.subject, iri_token(EX_A));
        assert_eq!(mirror.object, iri_token(EX_B));
        assert!(
            mirror
                .annotations
                .contains(&(GM_MAPPED_FROM.to_owned(), iri_token(KNOWS_FOAF_CELL)))
        );
    }

    #[test]
    fn saturate_drops_property_edge_with_suppressed_object() {
        // The property branch skips an edge whose OBJECT is suppressed (the
        // class-edge test only covers subject suppression); a control edge to a
        // visible object still mirrors — non-vacuous.
        let mut abox = String::new();
        abox.push_str(&nt(EX_A, GM_KNOWS, EX_SUPPRESSED));
        abox.push_str(&format!(
            "<{EX_SUPPRESSED}> <{GM_DISPLAYABLE}> \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean> .\n"
        ));
        abox.push_str(&nt(EX_C, GM_KNOWS, EX_B));
        let rows = saturate_nt(&abox, &knows_onto(), &knows_cells(), &[]).unwrap();
        let edges: BTreeSet<(String, String)> = rows
            .iter()
            .filter(|r| r.predicate == FOAF_KNOWS)
            .map(|r| (r.subject.clone(), r.object.clone()))
            .collect();
        assert!(
            !edges.contains(&(iri_token(EX_A), iri_token(EX_SUPPRESSED))),
            "suppressed-object edge leaked"
        );
        assert!(
            edges.contains(&(iri_token(EX_C), iri_token(EX_B))),
            "control edge lost"
        );
    }

    #[test]
    fn saturate_coarsen_guard_skips_edge_when_coarsen_to_present() {
        // A coarsen-guarded property whose subject carries gmeow:coarsenTo is
        // skipped; an unguarded subject still mirrors (positive control).
        let mut onto = knows_onto();
        onto.push_str(&format!(
            "<{GM_KNOWS}> <{GM_COARSEN_GUARDED}> \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean> .\n"
        ));
        let mut abox = String::new();
        abox.push_str(&nt(EX_A, GM_KNOWS, EX_B));
        abox.push_str(&nt(EX_A, GM_COARSEN_TO, EX_D)); // guard trips for EX_A
        abox.push_str(&nt(EX_C, GM_KNOWS, EX_B)); // no coarsenTo → control mirrors
        let rows = saturate_nt(&abox, &onto, &knows_cells(), &[]).unwrap();
        let subjects: BTreeSet<String> = rows
            .iter()
            .filter(|r| r.predicate == FOAF_KNOWS)
            .map(|r| r.subject.clone())
            .collect();
        assert!(
            !subjects.contains(&iri_token(EX_A)),
            "coarsen-guarded edge leaked"
        );
        assert!(
            subjects.contains(&iri_token(EX_C)),
            "unguarded control edge lost"
        );
    }

    #[test]
    fn saturate_annotates_cell_iri_and_confidence() {
        // Every derived triple is mappedFrom-attributed to its authored cell and
        // carries the cell's confidence as a typed decimal literal.
        let rows = saturate_nt(&person_abox(), &person_onto(), &person_cells(), &[]).unwrap();
        let schema_row = rows
            .iter()
            .find(|r| r.object == iri_token(SCHEMA_PERSON))
            .expect("schema:Person row");
        assert!(
            schema_row
                .annotations
                .contains(&(GM_MAPPED_FROM.to_owned(), iri_token(PERSON_SCHEMA_CELL)))
        );
        assert!(schema_row.annotations.contains(&(
            GM_CONFIDENCE.to_owned(),
            format!("\"0.9\"^^<{XSD_DECIMAL}>")
        )));
    }

    #[test]
    fn saturate_allows_absent_confidence() {
        // A cell may record no confidence — it still materializes, the
        // gmeow:confidence annotation is simply omitted (not a default).
        let cells = vec![cell(
            PERSON_SCHEMA_CELL,
            GM_PERSON,
            "owl:equivalentClass",
            SCHEMA_PERSON,
            "",
        )];
        let rows = saturate_nt(&person_abox(), &person_onto(), &cells, &[]).unwrap();
        let schema_row = rows
            .iter()
            .find(|r| r.object == iri_token(SCHEMA_PERSON))
            .expect("schema:Person row");
        assert!(
            schema_row
                .annotations
                .iter()
                .all(|(k, _)| k != GM_CONFIDENCE),
            "absent confidence must not be annotated: {:?}",
            schema_row.annotations
        );
        assert!(
            schema_row
                .annotations
                .contains(&(GM_MAPPED_FROM.to_owned(), iri_token(PERSON_SCHEMA_CELL)))
        );
    }

    #[test]
    fn saturate_rejects_out_of_range_confidence() {
        // A confidence outside [0.0, 1.0] is malformed — hard fail, never
        // emitted verbatim as a bogus xsd:decimal into the provenance layer.
        let cells = vec![cell(
            PERSON_SCHEMA_CELL,
            GM_PERSON,
            "owl:equivalentClass",
            SCHEMA_PERSON,
            "1.5",
        )];
        let err = saturate_nt(&person_abox(), &person_onto(), &cells, &[]).unwrap_err();
        assert_eq!(err.code(), crate::error::Transform::register());
        assert!(
            err.to_string().contains("malformed gmeow:confidence"),
            "{err}"
        );
    }

    #[test]
    fn saturate_rejects_non_numeric_confidence() {
        // A non-numeric confidence is malformed — hard fail.
        let cells = vec![cell(
            PERSON_SCHEMA_CELL,
            GM_PERSON,
            "owl:equivalentClass",
            SCHEMA_PERSON,
            "abc",
        )];
        let err = saturate_nt(&person_abox(), &person_onto(), &cells, &[]).unwrap_err();
        assert_eq!(err.code(), crate::error::Transform::register());
        assert!(
            err.to_string().contains("malformed gmeow:confidence"),
            "{err}"
        );
    }

    #[test]
    fn saturate_skips_already_asserted_triple() {
        // G is canonical — a triple already in the A-Box gets no derived row / reifier.
        let cells = vec![cell(
            PERSON_SCHEMA_CELL,
            GM_PERSON,
            "owl:equivalentClass",
            SCHEMA_PERSON,
            "0.9",
        )];
        let mut abox = person_abox();
        abox.push_str(&nt(EX_ME, RDF_TYPE, SCHEMA_PERSON));
        let rows = saturate_nt(&abox, &person_onto(), &cells, &[]).unwrap();
        assert!(
            rows.is_empty(),
            "already-asserted triple was re-derived: {rows:?}"
        );
    }

    #[test]
    fn saturate_is_deterministic() {
        // Two runs over a RICH A-Box — multiple subjects, class + property +
        // sameAs edges, and a literal-bearing triple — derive byte-identical
        // rows, including the content-addressed reifiers. Ordering/reifier
        // nondeterminism only surfaces with many mixed rows, not the 2-row
        // single-subject case.
        let mut onto = person_onto();
        onto.push_str(&knows_onto());
        let mut cells = person_cells();
        cells.extend(knows_cells());
        let mut abox = String::new();
        abox.push_str(&nt(EX_ME, RDF_TYPE, GM_PERSON));
        abox.push_str(&nt(EX_CONTROL, RDF_TYPE, GM_PERSON));
        abox.push_str(&nt(EX_A, GM_KNOWS, EX_B));
        abox.push_str(&nt(EX_C, GM_KNOWS, EX_D));
        abox.push_str(&nt(EX_ME, OWL_SAME_AS, WD_Q42));
        abox.push_str(&format!("<{EX_ME}> <{GM}fullName> \"Ada\" .\n"));

        let run_a = saturate_nt(&abox, &onto, &cells, &[]).unwrap();
        let run_b = saturate_nt(&abox, &onto, &cells, &[]).unwrap();
        assert_eq!(run_a, run_b);
        assert_eq!(
            run_a.len(),
            7,
            "2 Person subjects × 2 class edges + 2 property mirrors + 1 sameAs mirror: {run_a:?}"
        );
    }

    // ── projection P(G): onto-only-catalog exclusion + tag_map retag boundary ──
    //
    // Regression coverage for the `gmeow-cli/tests/self_sufficiency.rs` "zero
    // x-gmeow leak" finding: a projection CONSTRUCT whose WHERE clause matches
    // `?app gmeow:fullName ?name` (the `ontolex` profile's real shape) and mints
    // a fresh `<subject>-form` IRI via `BIND(IRI(CONCAT(...)))` must NOT leak an
    // onto-only individual's data into MAXIMAL(G) — only abox-derived facts are
    // "about the instance" — and any internally-tagged literal that DOES survive
    // into the output must be retagged to its public BCP-47 form.

    const CATALOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/catalogEntry";
    const FULL_NAME_QUERY: &str = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\nCONSTRUCT { ?form gmeow:writtenRep ?name }\nWHERE { ?app gmeow:fullName ?name . BIND(IRI(CONCAT(STR(?app), \"-form\")) AS ?form) }";

    #[test]
    fn projection_derived_excludes_onto_only_catalog_forms_but_keeps_abox_derived_ones() {
        // onto: an ontology-authored "reference catalog" individual (mirrors
        // `imports/languages-reference.ttl`'s exonym Appellations) — NOT part of
        // the transpiled instance.
        let ontology_nt = format!("<{CATALOG_ENTRY}> <{GM}fullName> \"Catalog Entry\" .\n");
        // abox: our actual instance data, carrying the SAME predicate.
        let raw_nt = format!("<{EX_ME}> <{GM}fullName> \"Ada Lovelace\" .\n");

        let report = transform_nt(
            &raw_nt,
            &ontology_nt,
            &[],
            &[],
            &[("catalog-forms".to_owned(), FULL_NAME_QUERY.to_owned())],
            &TagMap::new(),
        )
        .unwrap();

        assert!(
            !report.base_plus_derived_nt.contains("catalogEntry-form"),
            "onto-only catalog individual's synthesized form leaked into MAXIMAL(G): {}",
            report.base_plus_derived_nt
        );
        assert!(
            report.base_plus_derived_nt.contains("sat/me-form"),
            "abox-derived synthesized form is missing (over-exclusion): {}",
            report.base_plus_derived_nt
        );
    }

    #[test]
    fn transform_nt_retags_internal_language_tags_at_the_maximal_output_boundary() {
        // The instance's own fullName literal carries an internal x-gmeow-*
        // authoring tag (the normal in-ontology convention); `tag_map` maps it
        // to its public BCP-47 form, exactly as `project`/`export` already do at
        // their projection boundaries (`crate::projections::retag_quads`).
        let raw_nt = format!("<{EX_ME}> <{GM}fullName> \"Ada Lovelace\"@x-gmeow-english .\n");
        let mut tag_map = TagMap::new();
        tag_map.insert("x-gmeow-english".to_owned(), "en".to_owned());

        let report = transform_nt(&raw_nt, "", &[], &[], &[], &tag_map).unwrap();

        assert!(
            report.base_plus_derived_nt.contains("\"Ada Lovelace\"@en"),
            "internal tag was not retagged to its public BCP-47 form: {}",
            report.base_plus_derived_nt
        );
        assert!(
            !report.base_plus_derived_nt.contains("x-gmeow-english"),
            "internal tag leaked into the MAXIMAL(G) output: {}",
            report.base_plus_derived_nt
        );
    }
}
