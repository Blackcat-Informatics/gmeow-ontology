// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native MAXIMAL(G) transform kernel: GMEOW A-Box -> base + E(G) + P(G).
//!
//! Python supplies serialized repo-or-bundle inputs and remains the CLI/file
//! surface. This module owns the graph dataflow: deterministic skolemization,
//! suppression-aware strong-equivalence saturation, projection CONSTRUCT
//! execution, provenance merge, and GTS byte emission.
//!
//! Oxigraph-free (EPIC #906): the transient triple stores are flat
//! [`gmeow_rdf::RdfDataset`]s built from owned [`RdfQuad`] streams and pattern-queried
//! through the native [`DatasetView`]; the projection CONSTRUCT runs through
//! [`NativeSparqlEngine`]. The deterministic Skolem IRI minting, the content-addressed
//! reifier hash, and every committed N-Triples / GTS byte are preserved exactly: the
//! canonical labels come from the SAME RDFC-1.0 canonicalizer and the term tokens come
//! from the native term renderer, which is byte-identical to oxigraph's N-Triples term
//! form for the IRIs / blanks / typed-decimal literals this kernel emits.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use gmeow_rdf::{
    DatasetView, GraphMatch, NativeRdfFormat, RdfDataset, RdfLiteral, RdfQuad, RdfTerm,
    SparqlEngine, SparqlRequest, SparqlResult, TermId, TermRef, TermValue,
};
use gmeow_sparql_eval::NativeSparqlEngine;
use sha2::{Digest, Sha256};

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const SKOLEM_BASE: &str = "https://blackcatinformatics.ca/gmeow/.well-known/genid/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
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
/// The dataset is built with the FLAT codec ([`gmeow_rdf::flat_dataset_from_quads`]):
/// `rdf:reifies` / quoted-triple rows stay plain quads (no RDF 1.2 fold), exactly as
/// oxigraph's `Store` held them. The final GTS path re-folds via `parse_dataset`.
struct Graph {
    quads: Vec<RdfQuad>,
    ds: Arc<RdfDataset>,
}

impl Graph {
    fn from_quads(quads: Vec<RdfQuad>) -> Result<Self, String> {
        let ds = gmeow_rdf::flat_dataset_from_quads(&quads)
            .map_err(|e| format!("flat dataset build failed: {e}"))?;
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
            RdfTerm::triple(gmeow_rdf::RdfTriple::new(st, predicate, ot)) // codespell:ignore ot
        }
    }
}

/// Return the deterministic skolemized default graph as N-Triples.
pub fn skolemize_nt(raw_nt: &str) -> Result<String, String> {
    let graph = skolemized_graph(raw_nt)?;
    dump_nt(&graph)
}

/// Compute only E(G), returning row-shaped data for the Python compatibility API.
pub fn saturate_nt(
    abox_nt: &str,
    ontology_nt: &str,
    cells: &[CellInput],
    denied: &[(String, String, String)],
) -> Result<Vec<DerivedRowNative>, String> {
    let abox = parse_graph(abox_nt.as_bytes())?;
    let onto = parse_graph(ontology_nt.as_bytes())?;
    let cells = convert_cells(cells);
    let denied = denied.iter().cloned().collect();
    let vocab = suppression_vocab(&onto)?;
    let derived = saturate_graph(&abox, &onto, &cells, &denied, &vocab)?;
    Ok(derived_to_rows(&derived))
}

/// Compute MAXIMAL(G) over serialized inputs.
pub fn transform_nt(
    raw_nt: &str,
    ontology_nt: &str,
    cells: &[CellInput],
    denied: &[(String, String, String)],
    projection_queries: &[(String, String)],
) -> Result<TransformReportNative, String> {
    let mut abox = skolemized_graph(raw_nt)?;
    let onto = parse_graph(ontology_nt.as_bytes())?;
    let cells = convert_cells(cells);
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
    let base_plus_derived = base_plus_derived_graph(&abox, &derived)?;
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

fn convert_cells(inputs: &[CellInput]) -> Vec<Cell> {
    inputs
        .iter()
        .map(|cell| Cell {
            iri: cell.iri.clone(),
            subject: cell.subject.clone(),
            predicate_curie: cell.predicate_curie.clone(),
            object: cell.object.clone(),
            confidence: cell.confidence.clone(),
        })
        .collect()
}

fn skolemized_graph(raw_nt: &str) -> Result<Graph, String> {
    // Native full RDFC-1.0 (SHA-256) canonical N-Quads — the canonical `_:c14nN`
    // labels (hence the persisted skolem IRIs) are stable. We canonicalize the parsed
    // input then map each canonical blank to its deterministic skolem IRI.
    let parsed = gmeow_rdf::parse_dataset(
        raw_nt.as_bytes(),
        NativeRdfFormat::NTriples.media_type(),
        None,
    )
    .map_err(|e| format!("skolem input parse failed: {e}"))?;
    let canon_nq = gmeow_rdf::canonical_flat_nquads(parsed.as_ref())
        .map_err(|e| format!("canonicalization failed: {e}"))?;
    let canon = gmeow_rdf::parse_dataset(
        canon_nq.as_bytes(),
        NativeRdfFormat::NQuads.media_type(),
        None,
    )
    .map_err(|e| format!("canonical re-parse failed: {e}"))?;

    let flat = gmeow_rdf::flat_rdf_quads_from_dataset(canon.as_ref());
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
fn skolem_term(term: &RdfTerm) -> Result<RdfTerm, String> {
    Ok(match term {
        RdfTerm::Iri(iri) => RdfTerm::iri(iri.clone()),
        RdfTerm::BlankNode(label) => RdfTerm::iri(format!("{SKOLEM_BASE}{label}")),
        RdfTerm::Literal(lit) => RdfTerm::literal(lit.clone()),
        RdfTerm::Triple(triple) => RdfTerm::triple(gmeow_rdf::RdfTriple::new(
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
) -> Result<(EdgeMap, EdgeMap), String> {
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
) -> Result<BTreeMap<TripleKey, DerivedTriple>, String> {
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
) -> Result<(), String> {
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
) -> Result<BTreeMap<TripleKey, DerivedTriple>, String> {
    let projection_input = projection_input_graph(abox, onto)?;
    let onto_subjects = subjects(onto);
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
            .map_err(|e| format!("projection query evaluation failed for {name}: {e}"))?;
        let SparqlResult::Graph(triples) = result else {
            return Err(format!("projection query {name} did not return a graph"));
        };
        for quad in triples.owned_quads() {
            let subject = quad.subject;
            let object = quad.object;
            // CONSTRUCT predicates are always IRIs; `RdfQuad::predicate` carries the IRI.
            let predicate = quad.predicate;
            if onto_subjects.contains(&term_token(&subject)) {
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

fn projection_input_graph(abox: &Graph, onto: &Graph) -> Result<Graph, String> {
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
) -> Result<Graph, String> {
    let mut quads: Vec<RdfQuad> = default_quads(base);
    for row in derived.values() {
        quads.push(RdfQuad::new(
            row.subject.clone(),
            row.predicate.clone(),
            row.object.clone(),
        ));
    }
    Graph::from_quads(quads)
}

fn gts_from_maximal(
    base_plus_derived: &Graph,
    derived: &BTreeMap<TripleKey, DerivedTriple>,
) -> Result<Vec<u8>, String> {
    let mut builder = gmeow_rdf::gts_compose::SnapshotBuilder::new();
    // Native carrier ingestion (#909): serialize the default graph to N-Triples and
    // parse it into a frozen dataset, then fold it in. The native parse folds any RDF
    // 1.2 statement layer into the dataset's reifier/annotation side-tables, so a
    // single `add_dataset` reproduces the old `add_quads` + `add_rdf12` split.
    let base_nt = dump_nt(base_plus_derived)?;
    let base_dataset = gmeow_rdf::parse_dataset(base_nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| e.to_string())?;
    builder.add_dataset(&base_dataset)?;
    let statement_nt = statement_layer_nt(derived);
    if !statement_nt.trim().is_empty() {
        let statement_dataset =
            gmeow_rdf::parse_dataset(statement_nt.as_bytes(), "application/n-triples", None)
                .map_err(|e| e.to_string())?;
        builder.add_dataset(&statement_dataset)?;
    }
    gmeow_rdf::gts_compose::emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
    )
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

fn published_graph(abox: &Graph, suppressed: &BTreeSet<String>) -> Result<Graph, String> {
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

fn suppression_vocab(onto: &Graph) -> Result<SuppressionVocab, String> {
    let classes = subclass_closure(onto, GM_APPELLATION);
    let mut bearer: BTreeSet<String> = BTreeSet::new();
    for (prop, rng) in subject_objects(onto, RDFS_RANGE) {
        if classes.contains(&term_token(&rng)) {
            if let Some(prop) = subject_iri(&prop) {
                bearer.insert(prop);
            }
        }
    }
    let mut domain_props: BTreeSet<String> = BTreeSet::new();
    for (prop, dom) in subject_objects(onto, RDFS_DOMAIN) {
        if classes.contains(&term_token(&dom)) {
            if let Some(prop) = subject_iri(&prop) {
                domain_props.insert(prop);
            }
        }
    }
    let mut coarsen_guarded = BTreeSet::new();
    for (sub, obj) in subject_objects(onto, GM_COARSEN_GUARDED) {
        if matches!(&obj, RdfTerm::Literal(lit) if lit.lexical_form == "true") {
            if let Some(prop) = subject_iri(&sub) {
                coarsen_guarded.insert(prop);
            }
        }
    }
    Ok(SuppressionVocab {
        bearer_props: bearer.into_iter().collect(),
        appellation_domain_props: domain_props,
        appellation_classes: classes,
        coarsen_guarded,
    })
}

fn suppressed_nodes(abox: &Graph, vocab: &SuppressionVocab) -> Result<BTreeSet<String>, String> {
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

fn subclass_closure(graph: &Graph, root: &str) -> BTreeSet<String> {
    let mut closure = BTreeSet::new();
    closure.insert(format!("<{root}>"));
    let edges = subject_objects(graph, RDFS_SUB_CLASS_OF);
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

fn dump_nt(graph: &Graph) -> Result<String, String> {
    // Serialize the FLAT default graph to N-Triples, matching oxigraph's
    // `dump_graph_to_writer(DefaultGraph, NTriples)`: every default-graph quad as a
    // single `s p o .` line, in canonical dataset order.
    let flat = gmeow_rdf::flat_dataset_from_quads(&default_quads(graph))
        .map_err(|e| format!("N-Triples flatten failed: {e}"))?;
    let bytes = gmeow_rdf::serialize_dataset(
        flat.as_ref(),
        NativeRdfFormat::NTriples.media_type(),
        gmeow_rdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("N-Triples serialization failed: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("N-Triples output is not UTF-8: {e}"))
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
/// (the native renderer is the single source of truth, [`gmeow_rdf::RdfTerm`]'s
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

fn iri_from_token(token: &str) -> Result<String, String> {
    token
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .map(str::to_owned)
        .ok_or_else(|| format!("expected IRI token, got {token:?}"))
}

fn parse_graph(data: &[u8]) -> Result<Graph, String> {
    let parsed = gmeow_rdf::parse_dataset(data, NativeRdfFormat::NTriples.media_type(), None)
        .map_err(|e| format!("RDF parse failed: {e}"))?;
    // Un-fold to the flat quad stream so `rdf:reifies` / quoted-triple rows stay plain
    // quads (the old oxigraph `Store` held them flat), then re-freeze flat.
    let flat = gmeow_rdf::flat_rdf_quads_from_dataset(parsed.as_ref());
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

    #[test]
    fn curie_prefers_longest_namespace() {
        assert_eq!(
            curie("http://id.loc.gov/ontologies/bibframe/Work"),
            "bf:Work"
        );
    }
}
