// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal, SOUND native SLME — Syntactic Locality Module Extraction (issue #695).
//!
//! This is the Java/Docker-free replacement for the ROBOT `extract` shell-out. It
//! computes a *module* of a source ontology around a seed signature Σ (the caller's
//! `terms`), using syntactic ⊥-/⊤-locality. The contract is **soundness, not
//! minimality**: every construct that touches Σ is kept, and any construct we do not
//! classify by exact locality is kept *conservatively* (subject/closure-∈Σ test) with
//! a `slme.conservative-keep` warning. The extracted module may therefore be a
//! superset of ROBOT's — over-extraction is acceptable, under-extraction is NOT.
//!
//! # Algorithm
//!
//! Σ starts as the seed IRIs. Source triples are grouped into "axioms" by their named
//! subject; each `(s, p, o)` with a [`oxigraph::model::NamedNode`] subject is classified by predicate
//! and notion (Bot or Top). A *kept* triple is non-local; keeping it adds the named
//! entities named in the rule's "add" list to Σ and re-iterates to a fixpoint.
//!
//! - `BOT` — fixpoint over the ⊥ notion.
//! - `TOP` — fixpoint over the ⊤ notion.
//! - `STAR` — nested ⊥⊤*: repeat { a BOT pass, then a TOP pass } until the kept
//!   triple-set stops changing (smallest module).
//!
//! After the fixpoint the module = all kept triples + the blank-node closure of their
//! objects, serialized deterministically to Turtle (canonical (S, P, O) sort).

use std::collections::{BTreeMap, BTreeSet};

use oxigraph::model::{GraphName, NamedOrBlankNode, Quad, Term, Triple};

use gmeow_diagnostics::{Finding, Severity};
use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::{dataset_from_oxigraph_quads, parse_dataset, serialize_dataset, SerializeGraph};

// ── Vocabulary IRIs ─────────────────────────────────────────────────────────────

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";

const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

const OWL_EQUIVCLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_INVERSE: &str = "http://www.w3.org/2002/07/owl#inverseOf";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";

/// The owl:* meta-types that mark a `X rdf:type T` triple as a *declaration* (kept
/// iff `X ∈ Σ`) rather than instance typing.
const DECLARATION_TYPES: &[&str] = &[
    OWL_CLASS,
    OWL_OBJECT_PROPERTY,
    OWL_DATATYPE_PROPERTY,
    OWL_ANNOTATION_PROPERTY,
    RDF_PROPERTY,
];

// ── Method ──────────────────────────────────────────────────────────────────────

/// The locality notion applied to an axiom.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Notion {
    Bot,
    Top,
}

/// The extraction method (normalized).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    Bot,
    Top,
    Star,
}

impl Method {
    /// Normalize a free-form method string (case-insensitive). Unknown → `Star`,
    /// with the unknown name returned so the caller can emit a warning.
    fn parse(method: &str) -> (Self, Option<String>) {
        match method.trim().to_ascii_uppercase().as_str() {
            "BOT" => (Self::Bot, None),
            "TOP" => (Self::Top, None),
            "STAR" => (Self::Star, None),
            other => (Self::Star, Some(other.to_owned())),
        }
    }

    /// The canonical normalized name actually used.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bot => "BOT",
            Self::Top => "TOP",
            Self::Star => "STAR",
        }
    }
}

// ── Output ────────────────────────────────────────────────────────────────────

/// The result of a module extraction.
pub struct ModuleResult {
    /// The extracted module serialized to deterministic Turtle.
    pub module_ttl: String,
    /// Number of top-level (named-subject) triples kept.
    pub selected_axiom_count: usize,
    /// The normalized method actually used.
    pub method: Method,
    /// Conservative-keep / unknown-method warnings.
    pub findings: Vec<Finding>,
}

// ── Public entry ────────────────────────────────────────────────────────────────

/// Extract a syntactic-locality module from `ontology_ttl` around the seed `terms`.
///
/// # Errors
///
/// Returns an error string if the Turtle source cannot be parsed or the in-memory
/// store cannot be created/iterated.
pub fn extract_module(
    ontology_ttl: &str,
    terms: &[String],
    method: &str,
) -> Result<ModuleResult, String> {
    let (method, unknown_method) = Method::parse(method);
    let mut findings: Vec<Finding> = Vec::new();
    if let Some(name) = unknown_method {
        findings.push(
            Finding::new(
                Severity::Warning,
                "slme.unknown-method",
                format!("unknown extraction method {name:?}; defaulting to STAR"),
            )
            .with_tool("slme"),
        );
    }

    // Parse the source through the native codec into the frozen IR, then fold it into
    // an in-memory oxigraph Store for the locality walk (text-free IR → Store hop, so
    // the parse path can never drift from the rest of the stack's codec).
    let dataset = parse_dataset(ontology_ttl.as_bytes(), "text/turtle", None)
        .map_err(|e| format!("Failed to parse Turtle source: {e}"))?;
    let store = store_from_dataset(dataset.as_ref(), GraphPolicy::PreserveNamedGraphs)
        .map_err(|e| format!("Failed to materialize Turtle source into store: {e}"))?;

    // Snapshot all source triples (default-graph; the SLME notion is graph-flat).
    let mut all_triples: Vec<Triple> = Vec::new();
    for q in store.iter() {
        let q = q.map_err(|e| format!("store iteration error: {e}"))?;
        all_triples.push(Triple::new(q.subject, q.predicate, q.object));
    }

    // Index blank-node subjects → their triples, for the blank-node closure walk.
    let mut bnode_index: BTreeMap<String, Vec<Triple>> = BTreeMap::new();
    for t in &all_triples {
        if let NamedOrBlankNode::BlankNode(b) = &t.subject {
            bnode_index
                .entry(b.as_str().to_owned())
                .or_default()
                .push(t.clone());
        }
    }

    // The seed signature Σ.
    let mut sigma: BTreeSet<String> = terms.iter().cloned().collect();

    // The set of kept top-level (named-subject) triples, keyed by canonical string.
    let mut kept: BTreeMap<String, Triple> = BTreeMap::new();

    // Conservative-keep warnings are emitted once per (predicate, subject); track to
    // avoid duplicates across fixpoint iterations.
    let mut warned: BTreeSet<(String, String)> = BTreeSet::new();

    match method {
        Method::Bot => {
            grow_fixpoint(
                Notion::Bot,
                &all_triples,
                &bnode_index,
                &mut sigma,
                &mut kept,
                &mut warned,
                &mut findings,
            );
        }
        Method::Top => {
            grow_fixpoint(
                Notion::Top,
                &all_triples,
                &bnode_index,
                &mut sigma,
                &mut kept,
                &mut warned,
                &mut findings,
            );
        }
        Method::Star => {
            // Nested ⊥⊤*: alternate BOT and TOP passes until the kept set is stable.
            loop {
                let before = kept.len();
                let before_sigma = sigma.len();
                grow_fixpoint(
                    Notion::Bot,
                    &all_triples,
                    &bnode_index,
                    &mut sigma,
                    &mut kept,
                    &mut warned,
                    &mut findings,
                );
                grow_fixpoint(
                    Notion::Top,
                    &all_triples,
                    &bnode_index,
                    &mut sigma,
                    &mut kept,
                    &mut warned,
                    &mut findings,
                );
                if kept.len() == before && sigma.len() == before_sigma {
                    break;
                }
            }
        }
    }

    let selected_axiom_count = kept.len();

    // Collect the module = kept triples + blank-node closure of their objects.
    let mut module: BTreeSet<String> = BTreeSet::new();
    let mut module_quads: Vec<Quad> = Vec::new();
    for t in kept.values() {
        push_quad(&mut module, &mut module_quads, t);
        collect_bnode_closure(&t.object, &bnode_index, &mut module, &mut module_quads);
    }

    let module_ttl = serialize_turtle(module_quads)?;

    Ok(ModuleResult {
        module_ttl,
        selected_axiom_count,
        method,
        findings,
    })
}

// ── Fixpoint over one notion ──────────────────────────────────────────────────────

/// Grow Σ and the kept set under one locality `notion` to a fixpoint.
#[allow(clippy::too_many_arguments)]
fn grow_fixpoint(
    notion: Notion,
    all_triples: &[Triple],
    bnode_index: &BTreeMap<String, Vec<Triple>>,
    sigma: &mut BTreeSet<String>,
    kept: &mut BTreeMap<String, Triple>,
    warned: &mut BTreeSet<(String, String)>,
    findings: &mut Vec<Finding>,
) {
    loop {
        let mut changed = false;
        for t in all_triples {
            // Only named-subject triples are top-level axioms; blank-node-subject
            // triples are pulled in by the blank-node closure, not classified here.
            let NamedOrBlankNode::NamedNode(subj) = &t.subject else {
                continue;
            };
            let key = triple_key(t);
            if kept.contains_key(&key) {
                continue;
            }
            let decision = classify(
                notion,
                subj.as_str(),
                &t.predicate,
                &t.object,
                sigma,
                bnode_index,
            );
            match decision {
                Decision::Drop => {}
                Decision::Keep { add } => {
                    // A new keep always changes the kept set, so re-iterate; the Σ
                    // additions are folded in (no separate change flag needed).
                    kept.insert(key, t.clone());
                    for iri in add {
                        sigma.insert(iri);
                    }
                    changed = true;
                }
                Decision::ConservativeKeep => {
                    kept.insert(key, t.clone());
                    // Conservative keep also pulls the blank-node closure's named
                    // IRIs into Σ so anything anchored to them re-iterates.
                    let mut closure_names: BTreeSet<String> = BTreeSet::new();
                    collect_named_iris_in_closure(&t.object, bnode_index, &mut closure_names);
                    for iri in closure_names {
                        sigma.insert(iri);
                    }
                    // Also add named IRIs directly named in the triple.
                    if let Term::NamedNode(o) = &t.object {
                        sigma.insert(o.as_str().to_owned());
                    }
                    sigma.insert(subj.as_str().to_owned());
                    let warn_key = (t.predicate.as_str().to_owned(), subj.as_str().to_owned());
                    if warned.insert(warn_key) {
                        findings.push(
                            Finding::new(
                                Severity::Warning,
                                "slme.conservative-keep",
                                format!(
                                    "kept complex construct heuristically (not by exact \
                                     locality): subject <{}> predicate <{}>",
                                    subj.as_str(),
                                    t.predicate.as_str()
                                ),
                            )
                            .with_tool("slme"),
                        );
                    }
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

// ── Per-axiom classification ──────────────────────────────────────────────────────

/// A locality decision for one source triple.
enum Decision {
    /// Local — drop the triple.
    Drop,
    /// Non-local by exact locality — keep, adding the listed IRIs to Σ.
    Keep { add: Vec<String> },
    /// Unhandled / complex construct that touches Σ — keep conservatively.
    ConservativeKeep,
}

/// The closed set of logical predicates handled by exact locality.
const LOGICAL_PREDICATES: &[&str] = &[
    RDFS_SUBCLASS,
    OWL_EQUIVCLASS,
    OWL_DISJOINT,
    RDFS_SUBPROP,
    RDFS_DOMAIN,
    RDFS_RANGE,
    OWL_INVERSE,
    RDF_TYPE,
];

/// Classify a `(subject, predicate, object)` triple under `notion`.
fn classify(
    notion: Notion,
    subject: &str,
    predicate: &oxigraph::model::NamedNode,
    object: &Term,
    sigma: &BTreeSet<String>,
    bnode_index: &BTreeMap<String, Vec<Triple>>,
) -> Decision {
    let pred = predicate.as_str();
    let in_sigma = |iri: &str| sigma.contains(iri);

    // Soundness: if the predicate IS in the seed signature (and is not a
    // built-in logical predicate that already has its own locality rule below),
    // the property is requested, so the whole assertion must be kept —
    // regardless of whether the subject/object are in Σ.
    if in_sigma(pred) && !LOGICAL_PREDICATES.contains(&pred) {
        return match object {
            Term::NamedNode(o) => Decision::Keep {
                add: vec![subject.to_owned(), o.as_str().to_owned()],
            },
            Term::Literal(_) => Decision::Keep {
                add: vec![subject.to_owned()],
            },
            Term::BlankNode(_) | Term::Triple(_) => Decision::ConservativeKeep,
        };
    }

    // A blank-node object always means a complex construct → conservative test.
    if matches!(object, Term::BlankNode(_)) {
        return conservative(subject, object, sigma, bnode_index);
    }

    match pred {
        RDFS_SUBCLASS => {
            // C ⊑ D, both named. Bot: keep iff C∈Σ (add D). Top: keep iff D∈Σ (add C).
            let Term::NamedNode(d) = object else {
                return conservative(subject, object, sigma, bnode_index);
            };
            let c = subject;
            let d = d.as_str();
            match notion {
                Notion::Bot => {
                    if in_sigma(c) {
                        Decision::Keep {
                            add: vec![d.to_owned()],
                        }
                    } else {
                        Decision::Drop
                    }
                }
                Notion::Top => {
                    if in_sigma(d) {
                        Decision::Keep {
                            add: vec![c.to_owned()],
                        }
                    } else {
                        Decision::Drop
                    }
                }
            }
        }
        OWL_EQUIVCLASS => {
            // C ≡ D: keep iff C∈Σ or D∈Σ (add the other). Both notions.
            let Term::NamedNode(d) = object else {
                return conservative(subject, object, sigma, bnode_index);
            };
            let c = subject;
            let d = d.as_str();
            if in_sigma(c) || in_sigma(d) {
                Decision::Keep {
                    add: vec![c.to_owned(), d.to_owned()],
                }
            } else {
                Decision::Drop
            }
        }
        OWL_DISJOINT => {
            // C disjoint D: keep iff C∈Σ AND D∈Σ.
            let Term::NamedNode(d) = object else {
                return conservative(subject, object, sigma, bnode_index);
            };
            if in_sigma(subject) && in_sigma(d.as_str()) {
                Decision::Keep { add: vec![] }
            } else {
                Decision::Drop
            }
        }
        RDFS_SUBPROP => {
            // P ⊑ Q: Bot keep iff P∈Σ (add Q). Top keep iff Q∈Σ (add P).
            let Term::NamedNode(q) = object else {
                return conservative(subject, object, sigma, bnode_index);
            };
            let p = subject;
            let q = q.as_str();
            match notion {
                Notion::Bot => {
                    if in_sigma(p) {
                        Decision::Keep {
                            add: vec![q.to_owned()],
                        }
                    } else {
                        Decision::Drop
                    }
                }
                Notion::Top => {
                    if in_sigma(q) {
                        Decision::Keep {
                            add: vec![p.to_owned()],
                        }
                    } else {
                        Decision::Drop
                    }
                }
            }
        }
        RDFS_DOMAIN | RDFS_RANGE => {
            // P domain/range C: keep iff P∈Σ (add C).
            let Term::NamedNode(c) = object else {
                return conservative(subject, object, sigma, bnode_index);
            };
            if in_sigma(subject) {
                Decision::Keep {
                    add: vec![c.as_str().to_owned()],
                }
            } else {
                Decision::Drop
            }
        }
        OWL_INVERSE => {
            // P inverseOf Q: keep iff P∈Σ or Q∈Σ (add the other).
            let Term::NamedNode(q) = object else {
                return conservative(subject, object, sigma, bnode_index);
            };
            let p = subject;
            let q = q.as_str();
            if in_sigma(p) || in_sigma(q) {
                Decision::Keep {
                    add: vec![p.to_owned(), q.to_owned()],
                }
            } else {
                Decision::Drop
            }
        }
        RDF_TYPE => {
            let Term::NamedNode(ty) = object else {
                return conservative(subject, object, sigma, bnode_index);
            };
            let ty = ty.as_str();
            if DECLARATION_TYPES.contains(&ty) {
                // Declaration X a owl:Class|… : keep iff X∈Σ.
                if in_sigma(subject) {
                    Decision::Keep { add: vec![] }
                } else {
                    Decision::Drop
                }
            } else {
                // Instance typing x a C (named class): keep iff x∈Σ or C∈Σ
                // (add the other).
                if in_sigma(subject) || in_sigma(ty) {
                    Decision::Keep {
                        add: vec![subject.to_owned(), ty.to_owned()],
                    }
                } else {
                    Decision::Drop
                }
            }
        }
        _ => {
            // Annotation triple: a non-logical predicate whose object is a literal.
            // Keep iff s∈Σ. (Covers rdfs:label/comment, skos:*, dc/dcterms:*, and
            // any other non-logical predicate with a literal object.)
            if matches!(object, Term::Literal(_)) && !LOGICAL_PREDICATES.contains(&pred) {
                if in_sigma(subject) {
                    Decision::Keep { add: vec![] }
                } else {
                    Decision::Drop
                }
            } else {
                // Any other unhandled logical predicate / class-expression usage:
                // conservative subject/closure-∈Σ test.
                conservative(subject, object, sigma, bnode_index)
            }
        }
    }
}

/// The conservative-keep test for an unhandled / complex triple: keep iff the subject
/// ∈ Σ, or any named IRI directly in the triple's object ∈ Σ, or any named IRI in the
/// blank-node closure of the object ∈ Σ. This is the sound (superset-permissible)
/// fallback — it never drops a construct that touches Σ.
fn conservative(
    subject: &str,
    object: &Term,
    sigma: &BTreeSet<String>,
    bnode_index: &BTreeMap<String, Vec<Triple>>,
) -> Decision {
    if sigma.contains(subject) {
        return Decision::ConservativeKeep;
    }
    if let Term::NamedNode(o) = object {
        if sigma.contains(o.as_str()) {
            return Decision::ConservativeKeep;
        }
    }
    // Blank-node object: walk its closure and keep if any named IRI reached is in Σ.
    let mut closure_names: BTreeSet<String> = BTreeSet::new();
    collect_named_iris_in_closure(object, bnode_index, &mut closure_names);
    if closure_names.iter().any(|n| sigma.contains(n)) {
        return Decision::ConservativeKeep;
    }
    Decision::Drop
}

// ── Blank-node closure helpers ─────────────────────────────────────────────────

/// Push a triple into the module quad set (dedup by canonical key).
fn push_quad(module: &mut BTreeSet<String>, quads: &mut Vec<Quad>, t: &Triple) {
    let key = triple_key(t);
    if module.insert(key) {
        quads.push(Quad::new(
            t.subject.clone(),
            t.predicate.clone(),
            t.object.clone(),
            GraphName::DefaultGraph,
        ));
    }
}

/// Recursively pull the blank-node closure of `object` into the module.
fn collect_bnode_closure(
    object: &Term,
    bnode_index: &BTreeMap<String, Vec<Triple>>,
    module: &mut BTreeSet<String>,
    quads: &mut Vec<Quad>,
) {
    let mut stack: Vec<String> = Vec::new();
    if let Term::BlankNode(b) = object {
        stack.push(b.as_str().to_owned());
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(bid) = stack.pop() {
        if !seen.insert(bid.clone()) {
            continue;
        }
        if let Some(triples) = bnode_index.get(&bid) {
            for t in triples {
                push_quad(module, quads, t);
                if let Term::BlankNode(b) = &t.object {
                    stack.push(b.as_str().to_owned());
                }
            }
        }
    }
}

/// Collect every named IRI reachable in the blank-node closure of `object` (including
/// the object itself if it is named). Used to grow Σ on a conservative keep.
fn collect_named_iris_in_closure(
    object: &Term,
    bnode_index: &BTreeMap<String, Vec<Triple>>,
    out: &mut BTreeSet<String>,
) {
    if let Term::NamedNode(n) = object {
        out.insert(n.as_str().to_owned());
        return;
    }
    let mut stack: Vec<String> = Vec::new();
    if let Term::BlankNode(b) = object {
        stack.push(b.as_str().to_owned());
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(bid) = stack.pop() {
        if !seen.insert(bid.clone()) {
            continue;
        }
        // bnode_index only holds blank-node-subject triples, so the subject of each
        // is the blank node `bid` itself — predicates and objects can introduce named IRIs.
        if let Some(triples) = bnode_index.get(&bid) {
            for t in triples {
                out.insert(t.predicate.as_str().to_owned());
                match &t.object {
                    Term::NamedNode(n) => {
                        out.insert(n.as_str().to_owned());
                    }
                    Term::BlankNode(b) => stack.push(b.as_str().to_owned()),
                    _ => {}
                }
            }
        }
    }
}

// ── Determinism: canonical keys + Turtle serialization ──────────────────────────

/// Canonical (subject, predicate, object) string key for a triple.
fn triple_key(t: &Triple) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        subject_sort_key(&t.subject),
        t.predicate.as_str(),
        term_sort_key(&t.object)
    )
}

fn subject_sort_key(s: &NamedOrBlankNode) -> String {
    match s {
        NamedOrBlankNode::NamedNode(n) => n.as_str().to_owned(),
        NamedOrBlankNode::BlankNode(b) => format!("_:{}", b.as_str()),
    }
}

fn term_sort_key(t: &Term) -> String {
    match t {
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::BlankNode(b) => format!("_:{}", b.as_str()),
        // The language tag MUST be part of the key: this key backs triple_key,
        // which dedups kept/module triples. GMEOW carries multilingual labels
        // ("x"@en vs "x"@fr), so a value+datatype-only key would collide and
        // silently drop a translation — under-extraction. Lang-tagged literals
        // key on the tag; all others on the datatype.
        Term::Literal(l) => match l.language() {
            Some(lang) => format!("\"{}\"@{}", l.value(), lang),
            None => format!("\"{}\"^^{}", l.value(), l.datatype().as_str()),
        },
        // RDF-star quoted-triple object: key on its full N-Triples form so two
        // distinct quoted triples never collide (defensive — SLME source vocabs
        // are not RDF-star, but never silently dedup distinct terms).
        Term::Triple(inner) => inner.to_string(),
    }
}

/// Serialize the module quads to deterministic Turtle via the native codec.
///
/// The oxigraph `Quad`s are folded into the frozen `RdfDataset` IR
/// (`dataset_from_oxigraph_quads`) and serialized through the native
/// `serialize_dataset` path, which emits canonical, deterministic Turtle — so no
/// manual pre-sort is needed (and the codec never drifts from the rest of the stack).
/// All module quads live in the default graph, so `SerializeGraph::DefaultGraph` is
/// the faithful selector.
fn serialize_turtle(quads: Vec<Quad>) -> Result<String, String> {
    let dataset =
        dataset_from_oxigraph_quads(&quads).map_err(|e| format!("Turtle fold error: {e}"))?;
    let bytes = serialize_dataset(
        dataset.as_ref(),
        "text/turtle",
        SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("Turtle serialize error: {e}"))?;
    let body = String::from_utf8(bytes).map_err(|e| format!("Turtle serialize utf8 error: {e}"))?;
    Ok(format!("{}\n", body.trim_end_matches('\n')))
}

#[cfg(test)]
mod tests;
