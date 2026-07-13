// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow describe <term>` — the graph→[`Card`] assembly (the native port of the
//! Python `describe.py` prose-card composer).
//!
//! This module reads a GTS bundle (offline, repo-free — the documentation rides
//! the package), resolves a user query to a `gmeow:` term IRI, gathers the
//! Tier-1 documentation properties for that term (label, definition, gUFO
//! stereotype, domain/range, super-terms, owning slice, scope/usage advisories,
//! the flat-first/reify-on-demand pairing, box roles), and composes them into the
//! ONE canonical [`crate::card::Card`], rendered by the ONE canonical
//! [`crate::card::render_card`] (§19 one-path). The prior rich-terminal renderer
//! in Python is the inferior element being retired; the canonical Markdown card
//! is the single source of truth (GREENFIELD — no divergent second renderer).
//!
//! Language selection routes entirely through `gmeow_validate::language_tags`
//! (the Rust authority for the `x-gmeow-*` private-use tag discipline): the tag
//! map, the available-language set, and the requested-language resolution all
//! come from that crate, so the fold and describe paths agree by construction.
//!
//! The crate is PyO3-free; every dependency here is a pure-Rust rlib.

use std::collections::BTreeSet;
use std::sync::Arc;

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};

use gmeow_errors::{Diag, Result};
use gmeow_validate::language_tags::{
    LangSelector, LitDesc, filter_literals, load_tag_map, marked, resolve_lang_input,
    select_literal,
};

use crate::card::{Card, CardDetail, render_card};
use crate::error::GtsRead;

/// The GMEOW namespace prefix for term IRIs.
const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
/// The gUFO ontology namespace.
const GUFO: &str = "http://purl.org/nemo/gufo#";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const SKOS_SCOPE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#scopeNote";
const SKOS_EXAMPLE: &str = "http://www.w3.org/2004/02/skos/core#example";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const RDFS_DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";

const GM_USE_WHEN: &str = "https://blackcatinformatics.ca/gmeow/useWhen";
const GM_AVOID_WHEN: &str = "https://blackcatinformatics.ca/gmeow/avoidWhen";
const GM_HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/howToUse";
const GM_USE_FOR_CONSUMER: &str = "https://blackcatinformatics.ca/gmeow/useForConsumer";
const GM_AVOID_FOR_CONSUMER: &str = "https://blackcatinformatics.ca/gmeow/avoidForConsumer";
const GM_PAIRS_WITH: &str = "https://blackcatinformatics.ca/gmeow/pairsWith";
const GM_GRAPH_BOX_ROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";

/// The gUFO stereotype local names describe surfaces (mirrors the Python
/// `_GUFO_STEREOTYPES`). A term's `rdf:type` in the gUFO namespace is only
/// treated as a stereotype when its local name is one of these.
const GUFO_STEREOTYPES: &[&str] = &[
    "Kind",
    "SubKind",
    "Category",
    "Role",
    "Phase",
    "Mixin",
    "RoleMixin",
    "PhaseMixin",
    "AbstractIndividualType",
    "EventType",
    "SituationType",
];

/// A frozen GTS bundle plus the default-graph query surface `describe` needs.
///
/// Wraps an [`RdfDataset`] loaded from a GTS bundle with every base quad's graph
/// component preserved; every read is scoped to the *default* graph
/// ([`GraphMatch::Default`]) — the GTS default graph carries the authored,
/// import-free ontology (the same scope the Python `load_graph_from_gts` kept).
pub struct DescribeGraph {
    ds: Arc<RdfDataset>,
}

impl DescribeGraph {
    /// Load a GTS bundle's bytes into a describe-ready graph. Folds every segment
    /// and preserves each base quad's graph component (so default-graph reads are
    /// exact). Errors carry the reader diagnostic text.
    pub fn from_gts_bytes(bytes: &[u8]) -> Result<Self> {
        let graph = purrdf::gts::read_all_segments(bytes).map_err(|e| {
            Diag::of_kind(GtsRead {
                detail: format!("cannot read GTS bundle: {e}"),
            })
        })?;
        let ds = purrdf::gts::dataset_from_gts_graph(&graph).map_err(|e| {
            Diag::of_kind(GtsRead {
                detail: format!("cannot fold GTS bundle: {e}"),
            })
        })?;
        Ok(Self { ds })
    }

    /// Resolve an IRI to its dataset-local term id, if interned.
    fn iri_id(&self, iri: &str) -> Option<TermId> {
        self.ds.term_id_by_value(&TermValue::iri(iri))
    }

    /// Every default-graph object term of `<subject> <pred> ?o`, resolved into an
    /// owned [`OwnedObject`], in dataset order.
    fn objects(&self, subject_iri: &str, pred: &str) -> Vec<OwnedObject> {
        let (Some(s), Some(p)) = (self.iri_id(subject_iri), self.iri_id(pred)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
            .map(|q| self.owned_object(q.o))
            .collect()
    }

    /// Every default-graph named-object IRI of `<subject> <pred> ?o`, in dataset
    /// order.
    fn named_objects(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        self.objects(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                OwnedObject::Named(iri) => Some(iri),
                _ => None,
            })
            .collect()
    }

    /// Every default-graph language-tagged/untagged literal of
    /// `<subject> <pred> ?o` as a [`LitDesc`] (lexical + optional language tag),
    /// in dataset order — the input to language selection.
    fn literal_descs(&self, subject_iri: &str, pred: &str) -> Vec<LitDesc> {
        self.objects(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                OwnedObject::Literal { lexical, language } => Some(LitDesc { lexical, language }),
                _ => None,
            })
            .collect()
    }

    /// Every distinct default-graph named subject carrying `?s <pred> <object>`.
    fn subjects_with_object(&self, pred: &str, object_iri: &str) -> Vec<String> {
        let (Some(p), Some(o)) = (self.iri_id(pred), self.iri_id(object_iri)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(None, Some(p), Some(o), GraphMatch::Default)
            .filter_map(|q| match self.ds.resolve(q.s) {
                TermRef::Iri(iri) => Some(iri.to_owned()),
                _ => None,
            })
            .collect()
    }

    /// Every distinct default-graph named subject carrying `?s <pred> ?o`.
    fn subjects_with_predicate(&self, pred: &str) -> BTreeSet<String> {
        let Some(p) = self.iri_id(pred) else {
            return BTreeSet::new();
        };
        self.ds
            .quads_for_pattern(None, Some(p), None, GraphMatch::Default)
            .filter_map(|q| match self.ds.resolve(q.s) {
                TermRef::Iri(iri) => Some(iri.to_owned()),
                _ => None,
            })
            .collect()
    }

    /// The INJECTIVE `documentation/term/{slug}` slug for a term IRI, read back
    /// from the docs projection's emitted `gmeow:documents` inverse in the folded
    /// bundle — the dogfooded, collision-free slug map (`documentation/term/{slug}
    /// gmeow:documents <term-iri>`). The doc-entry records live in the
    /// `graph/documentation` named graph, so this reads across ALL graphs
    /// ([`GraphMatch::Any`]) rather than the default-graph-only describe reads, and
    /// keeps only the `documentation/term/` doc-entry subject (excluding the
    /// evidence / changelog nodes that also `gmeow:documents` the term). `None` when
    /// the bundle carries no documentation entry for the term.
    pub fn documentation_term_slug(&self, term_iri: &str) -> Option<String> {
        let documents = format!("{NAMESPACE}documents");
        let term_prefix = format!("{NAMESPACE}documentation/term/");
        let (Some(p), Some(o)) = (self.iri_id(&documents), self.iri_id(term_iri)) else {
            return None;
        };
        let candidates: Vec<String> = self
            .ds
            .quads_for_pattern(None, Some(p), Some(o), GraphMatch::Any)
            .filter_map(|q| match self.ds.resolve(q.s) {
                TermRef::Iri(iri) if iri.starts_with(&term_prefix) => {
                    Some(iri[term_prefix.len()..].to_owned())
                }
                _ => None,
            })
            .collect();
        debug_assert!(
            candidates.len() <= 1,
            "term `{term_iri}` is documented by {} distinct documentation/term/ subjects \
             ({candidates:?}) — the injective-slug invariant requires at most one",
            candidates.len()
        );
        // Exactly one doc-entry record documents a given term; a BTree-min keeps
        // the read deterministic even if that invariant ever weakened.
        candidates.into_iter().min()
    }

    /// Resolve an object term id into an owned object value.
    fn owned_object(&self, id: TermId) -> OwnedObject {
        match self.ds.resolve(id) {
            TermRef::Iri(iri) => OwnedObject::Named(iri.to_owned()),
            TermRef::Literal {
                lexical, language, ..
            } => OwnedObject::Literal {
                lexical: lexical.to_owned(),
                language: language.map(str::to_owned),
            },
            _ => OwnedObject::Other,
        }
    }
}

/// An owned resolution of an RDF object term for the describe reads (blank nodes
/// and quoted triples collapse to [`OwnedObject::Other`] — describe never renders
/// them).
enum OwnedObject {
    Named(String),
    Literal {
        lexical: String,
        language: Option<String>,
    },
    Other,
}

/// Shorten an IRI to its display CURIE (`gmeow:` / `gufo:`), else return it whole.
fn short(iri: &str) -> String {
    if let Some(rest) = iri.strip_prefix(NAMESPACE) {
        format!("gmeow:{rest}")
    } else if let Some(rest) = iri.strip_prefix(GUFO) {
        format!("gufo:{rest}")
    } else {
        iri.to_owned()
    }
}

/// The set of public BCP-47 tags that actually carry literals in the snapshot,
/// each internal `x-gmeow-*` tag projected to its public form via `tag_map` (an
/// already-public tag with no internal counterpart is kept as-is — the retag
/// boundary). Mirrors the Python `available_languages` fold view.
fn available_languages(
    ds: &RdfDataset,
    tag_map: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for qr in ds.quad_refs() {
        if let TermRef::Literal {
            language: Some(lang),
            ..
        } = qr.o
        {
            let mapped = tag_map
                .get(lang)
                .cloned()
                .unwrap_or_else(|| lang.to_owned());
            set.insert(mapped.to_lowercase());
        }
    }
    set.into_iter().collect()
}

/// Resolve a user query to a `gmeow:` term IRI.
///
/// Accepts `gmeow:X`, a full IRI, a bare local name, or a case-insensitive
/// prefix; returns `(Some(iri), [])` on a unique match, else `(None, candidates)`
/// where `candidates` (bare local names, capped at 10) is non-empty on ambiguity
/// or a no-match-with-suggestions.
pub fn resolve_term(graph: &DescribeGraph, query: &str) -> (Option<String>, Vec<String>) {
    let mut text = query.trim();
    if let Some(rest) = text.strip_prefix("gmeow:") {
        text = rest;
    }
    if let Some(rest) = text.strip_prefix(NAMESPACE) {
        text = rest;
    }
    if text.is_empty() {
        // An empty query would prefix-match everything.
        return (None, Vec::new());
    }

    // The bare local names of every term that declares an owning slice — the
    // authored `gmeow:` vocabulary surface (no nested `/` path segments).
    let mut locals: Vec<String> = graph
        .subjects_with_predicate(RDFS_IS_DEFINED_BY)
        .into_iter()
        .filter_map(|s| s.strip_prefix(NAMESPACE).map(str::to_owned))
        .filter(|local| !local.contains('/'))
        .collect();
    locals.sort();
    locals.dedup();

    if locals.iter().any(|name| name == text) {
        return (Some(format!("{NAMESPACE}{text}")), Vec::new());
    }
    let lower = text.to_lowercase();
    let exact_ci: Vec<String> = locals
        .iter()
        .filter(|name| name.to_lowercase() == lower)
        .cloned()
        .collect();
    if let [only] = exact_ci.as_slice() {
        return (Some(format!("{NAMESPACE}{only}")), Vec::new());
    }
    let prefix: Vec<String> = locals
        .iter()
        .filter(|name| name.to_lowercase().starts_with(&lower))
        .cloned()
        .collect();
    if let [only] = prefix.as_slice() {
        return (Some(format!("{NAMESPACE}{only}")), Vec::new());
    }
    let candidates = if exact_ci.is_empty() {
        prefix
    } else {
        exact_ci
    };
    (None, candidates.into_iter().take(10).collect())
}

/// The language-selected string values for a multi-valued annotation predicate
/// (sorted, matching the Python `_selected_texts` + `sorted(...)`).
fn selected_texts(
    graph: &DescribeGraph,
    term: &str,
    predicate: &str,
    selector: &LangSelector,
    tag_map: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let descs = graph.literal_descs(term, predicate);
    if descs.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<String> = filter_literals(&descs, &selector.requested, tag_map)
        .into_iter()
        .map(|sel| descs[sel.index].lexical.clone())
        .collect();
    out.sort();
    out
}

/// The language-selected single value for a predicate, plus a `[fallback: en]`
/// presentation marker when the chosen literal fell back to the carrier language.
fn selected_single(
    graph: &DescribeGraph,
    term: &str,
    predicate: &str,
    selector: &LangSelector,
    tag_map: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let descs = graph.literal_descs(term, predicate);
    let sel = select_literal(&descs, &selector.requested, tag_map)?;
    Some(marked(&descs[sel.index].lexical, sel.is_fallback, "en"))
}

/// The SHACL→JSON-Schema keying namespaces (the `gmeow` primary prefix plus the
/// authored `logic`/`lang`/`math` prefixes) [`purrdf::shapes::json_schema::
/// Namespaces::def_key`] needs to compute a class's `$defs` key — the SAME table
/// `gmeow-pipeline`'s (private) `crate::gmeow_ns::gmeow_json_schema_namespaces`
/// builds. Declared here rather than imported because `gmeow-pipeline` depends on
/// `gmeow-docs` (importing it back would be circular); the four namespace
/// literals are already independently declared in both crates (mirrors
/// `crates/docs/src/model.rs`'s own `LOGIC_NS`/`LANG_NS`/`MATH_NS`). Construction
/// cannot fail: `gmeow` is always declared.
fn json_schema_namespaces() -> purrdf::Namespaces {
    const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
    const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
    const MATH_NS: &str = "https://blackcatinformatics.ca/math/";
    purrdf::Namespaces::new(
        "gmeow",
        &[
            ("gmeow".to_owned(), NAMESPACE.to_owned()),
            ("logic".to_owned(), LOGIC_NS.to_owned()),
            ("lang".to_owned(), LANG_NS.to_owned()),
            ("math".to_owned(), MATH_NS.to_owned()),
        ],
    )
    .expect("gmeow primary prefix is always declared")
}

/// Whether `term` (a documented CLASS IRI) names a `$defs` entry in
/// `modeled_defs` — the "this class has a generated Pydantic model" existence
/// signal EVERY term→model gate must share (§19 one-path): the docs-site card
/// (`crate::render::doc_term_card`) and the folded/MCP card
/// (`gmeow_pipeline::stages::export::term_to_card`) key `$defs` the SAME way, so
/// a term never disagrees on whether it carries a `python_model` link (issue:
/// Pydantic model surface, finding F3).
fn class_is_modeled(term: &str, modeled_defs: &BTreeSet<String>) -> bool {
    modeled_defs.contains(&json_schema_namespaces().def_key(term))
}

/// Compose the canonical [`Card`] for `term` from the graph.
///
/// `modeled_defs` is the JSON Schema `$defs` key set (the caller's
/// model-existence signal, e.g. `gmeow_pipeline::bundle_blobs::Bundle::
/// modeled_def_keys` over the same GTS bundle) — `describe`/`build_card` cannot
/// reach it on their own (a GTS bundle carries no SHACL/JSON-Schema surface as
/// queryable RDF triples), so the caller threads it in explicitly.
pub fn build_card(
    graph: &DescribeGraph,
    term: &str,
    selector: &LangSelector,
    tag_map: &std::collections::HashMap<String, String>,
    modeled_defs: &BTreeSet<String>,
) -> Card {
    let local = term.strip_prefix(NAMESPACE).unwrap_or(term).to_owned();

    // Vocabulary category + gUFO stereotype from rdf:type.
    let types = graph.named_objects(term, RDF_TYPE);
    let category = category_for(&types);
    let mut logic_stereotypes: Vec<String> = types
        .iter()
        .filter_map(|t| t.strip_prefix(GUFO))
        .filter(|name| GUFO_STEREOTYPES.contains(name))
        .map(|name| format!("gufo:{name}"))
        .collect();
    logic_stereotypes.sort();
    logic_stereotypes.dedup();

    // Label: language-selected, falling back to the local name (the Python
    // `card.label = local` path clears the fallback marker, so use a plain local).
    let label = selected_single(graph, term, RDFS_LABEL, selector, tag_map)
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| local.clone());

    // Super-terms (subclass ∪ subproperty), display CURIEs, sorted + deduped.
    let mut parents: Vec<String> = graph
        .named_objects(term, RDFS_SUBCLASS_OF)
        .into_iter()
        .chain(graph.named_objects(term, RDFS_SUBPROPERTY_OF))
        .map(|iri| short(&iri))
        .collect();
    parents.sort();
    parents.dedup();

    let domain = sorted_curies(graph.named_objects(term, RDFS_DOMAIN));
    let range = sorted_curies(graph.named_objects(term, RDFS_RANGE));

    let definition = selected_single(graph, term, SKOS_DEFINITION, selector, tag_map);

    let scope_notes = selected_texts(graph, term, SKOS_SCOPE_NOTE, selector, tag_map);
    let examples = selected_texts(graph, term, SKOS_EXAMPLE, selector, tag_map);
    let use_when = selected_texts(graph, term, GM_USE_WHEN, selector, tag_map);
    let avoid_when = selected_texts(graph, term, GM_AVOID_WHEN, selector, tag_map);
    let how_to_use = selected_texts(graph, term, GM_HOW_TO_USE, selector, tag_map);

    let use_for_consumer = sorted_curies(graph.named_objects(term, GM_USE_FOR_CONSUMER));
    let avoid_for_consumer = sorted_curies(graph.named_objects(term, GM_AVOID_FOR_CONSUMER));

    // The flat-first/reify-on-demand pairing, from BOTH directions, merged into
    // the canonical `related_terms` field (the one-path Card carries no direction
    // distinction; the retired renderer's split labels do not survive).
    let mut related: Vec<String> = graph
        .named_objects(term, GM_PAIRS_WITH)
        .into_iter()
        .chain(graph.subjects_with_object(GM_PAIRS_WITH, term))
        .map(|iri| short(&iri))
        .collect();
    related.sort();
    related.dedup();

    let box_roles = sorted_curies(graph.named_objects(term, GM_GRAPH_BOX_ROLE));

    // Owning slice: keep the full isDefinedBy IRI (the Pydantic module route needs
    // it) and derive the display name (its local segment) for the card header.
    let defined_by = graph
        .named_objects(term, RDFS_IS_DEFINED_BY)
        .into_iter()
        .next();
    let slice = defined_by.as_deref().map(|iri| {
        iri.trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(iri)
            .to_owned()
    });

    // The explicit term→model link: a modeled class carries the importable dotted
    // path of its generated Pydantic model plus a compact construct/validate
    // snippet, computed via the SAME emitter routing (never duplicated). Gated on
    // `class_is_modeled` (the class actually names a `$defs` entry) — an abstract
    // class with no SHACL NodeShape has NO generated model, and unconditionally
    // emitting the link (the pre-fix gate: `category == "Class" && defined_by.
    // is_some()`) fabricated an ImportError for a user who copied it (issue:
    // Pydantic model surface, finding F3). The slice defaults to "" (matching
    // `crate::render::doc_term_card`'s `DocTerm::owner_slice: String`) so the two
    // builders never disagree over a modeled class with no recovered slice.
    let (python_model, python_snippet) =
        if category == "Class" && class_is_modeled(term, modeled_defs) {
            let slice_iri = defined_by.as_deref().unwrap_or("");
            (
                Some(crate::card::python_model_path(slice_iri, term)),
                Some(crate::card::python_model_snippet(
                    slice_iri,
                    term,
                    &short(term),
                )),
            )
        } else {
            (None, None)
        };

    Card {
        category,
        iri: term.to_owned(),
        label: Some(label),
        slice,
        box_roles,
        definition,
        parents,
        domain,
        range,
        use_when,
        avoid_when,
        how_to_use,
        scope_notes,
        examples,
        logic_stereotypes,
        related_terms: related,
        use_for_consumer,
        avoid_for_consumer,
        aligns: Vec::new(),
        python_model,
        python_snippet,
        // Full-tier rich panels: never populated on the describe/site path.
        ..Card::default()
    }
}

/// Map IRIs to display CURIEs, sorted + deduped.
fn sorted_curies(iris: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = iris.iter().map(|iri| short(iri)).collect();
    out.sort();
    out.dedup();
    out
}

/// The canonical vocabulary category for a term's `rdf:type` set.
fn category_for(types: &[String]) -> String {
    let has = |iri: &str| types.iter().any(|t| t == iri);
    if has(OWL_CLASS) {
        "Class".to_owned()
    } else if has(OWL_OBJECT_PROPERTY) || has(OWL_DATATYPE_PROPERTY) || has(OWL_ANNOTATION_PROPERTY)
    {
        "Property".to_owned()
    } else if has(RDFS_DATATYPE) {
        "Datatype".to_owned()
    } else {
        "Individual".to_owned()
    }
}

/// The `gmeow describe` entry point: resolve, assemble, and render one term card
/// from a GTS bundle's bytes.
///
/// * `query` — the user's term request (`gmeow:X`, an IRI, a local name, or a
///   case-insensitive prefix).
/// * `gts_bytes` — a GTS bundle (the offline `gmeow.gts`, or a user `--gts`).
/// * `lang` — the already-resolved language request (`None` → the English
///   carrier); the env/`--lang` precedence is the caller's (the consumer bin's)
///   concern.
/// * `modeled_defs` — the JSON Schema `$defs` key set for THIS bundle (e.g.
///   `gmeow_pipeline::bundle_blobs::Bundle::modeled_def_keys`), the
///   model-existence signal [`build_card`] gates a class's `python_model` link
///   on. `describe` cannot derive this itself (a GTS bundle carries no SHACL/
///   JSON-Schema surface as queryable RDF triples), so the caller — which reads
///   the SAME bundle bytes — computes and threads it in.
///
/// Returns `(text, exit_code)`: the rendered Markdown card with code `0`, or a
/// diagnostic message with a non-zero code on a load / unknown-term /
/// unknown-language failure.
pub fn describe(
    query: &str,
    gts_bytes: &[u8],
    lang: Option<&str>,
    modeled_defs: &BTreeSet<String>,
) -> (String, i32) {
    let graph = match DescribeGraph::from_gts_bytes(gts_bytes) {
        Ok(graph) => graph,
        Err(e) => return (e.to_string(), 1),
    };

    let (term, candidates) = resolve_term(&graph, query);
    let Some(term) = term else {
        if candidates.is_empty() {
            return (format!("no GMEOW term matches '{query}'"), 1);
        }
        let options = candidates
            .iter()
            .map(|c| format!("  gmeow:{c}"))
            .collect::<Vec<_>>()
            .join("\n");
        return (
            format!("ambiguous or unknown term '{query}' — candidates:\n{options}"),
            1,
        );
    };

    // The tag map + available-language set drive language resolution. The carrier
    // internal→BCP-47 map is a BUNDLE-WIDE fact: the generated `gmeow:bcp47Tag`
    // projection rides the named `lang-projection-corpus` graph, so the map is read
    // from a FLATTENED (all-graphs-unioned) projection of the bundle — the
    // default-graph alone omits the corpus, leaving the fr/zh carriers invisible.
    // This mirrors the consumer bin's `bundle_tag_map`.
    let flat = match purrdf::gts::flattened_dataset_from_bytes(gts_bytes) {
        Ok(ds) => ds,
        Err(e) => return (format!("cannot fold bundle for language map: {e}"), 1),
    };
    let nt = match purrdf::serialize_dataset(
        &flat,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    ) {
        Ok(bytes) => bytes,
        Err(e) => return (format!("cannot project bundle for language map: {e}"), 1),
    };
    let tag_map = match load_tag_map(&nt, "n-triples") {
        Ok(map) => map,
        Err(e) => return (format!("cannot build language tag map: {e}"), 1),
    };
    // The requestable set is the UNION of (a) the known carrier public tags — the
    // framework's shippable translation targets (en/fr/zh), always requestable so a
    // carrier with no direct content still resolves and falls back to the English
    // carrier — and (b) the public tags that actually carry a literal in this
    // snapshot. Validating against the has-content set alone wrongly rejects a known
    // carrier (fr/zh) whose content is English-only.
    let mut available: BTreeSet<String> = available_languages(&graph.ds, &tag_map)
        .into_iter()
        .collect();
    available.extend(tag_map.values().map(|v| v.to_ascii_lowercase()));
    let available: Vec<String> = available.into_iter().collect();

    let selector = match resolve_lang_input(lang, &tag_map, Some(&available)) {
        Ok(selector) => selector,
        Err(unknown) => {
            return (
                format!(
                    "unknown language tag '{}'. Available languages: {}",
                    unknown.tag,
                    unknown.available.join(", ")
                ),
                1,
            );
        }
    };

    let card = build_card(&graph, &term, &selector, &tag_map, modeled_defs);
    let local = term.strip_prefix(NAMESPACE).unwrap_or(&term);
    (
        render_card(&format!("gmeow:{local}"), &card, CardDetail::Standard),
        0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::RdfLookaside;

    /// The GTS write profile for test fixtures (arbitrary, deterministic).
    const TEST_PROFILE: &str = "purrdf-test";

    /// The controlled multilingual fixture, mirroring `_multilingual_gts` in
    /// `tests/test_cli.py`: a `gmeow:SampleTerm` class with English (always) and
    /// optional French / Mandarin labels + definitions, plus the three
    /// `gmeow:Language` individuals that seed the tag map. Emitted as GTS bytes.
    fn multilingual_gts(include_fr: bool, include_zh: bool) -> Vec<u8> {
        let mut nt = String::new();
        let term = format!("{NAMESPACE}SampleTerm");
        nt.push_str(&format!("<{term}> <{RDF_TYPE}> <{OWL_CLASS}> .\n"));
        nt.push_str(&format!(
            "<{term}> <{RDFS_LABEL}> \"sample label\"@x-gmeow-english .\n"
        ));
        nt.push_str(&format!(
            "<{term}> <{SKOS_DEFINITION}> \"English definition text.\"@x-gmeow-english .\n"
        ));
        if include_fr {
            nt.push_str(&format!(
                "<{term}> <{RDFS_LABEL}> \"étiquette échantillon\"@x-gmeow-french .\n"
            ));
            nt.push_str(&format!(
                "<{term}> <{SKOS_DEFINITION}> \"Définition en français.\"@x-gmeow-french .\n"
            ));
        }
        if include_zh {
            nt.push_str(&format!(
                "<{term}> <{RDFS_LABEL}> \"样本标签\"@x-gmeow-mandarin .\n"
            ));
            nt.push_str(&format!(
                "<{term}> <{SKOS_DEFINITION}> \"中文定义。\"@x-gmeow-mandarin .\n"
            ));
        }
        nt.push_str(&format!(
            "<{term}> <{RDFS_IS_DEFINED_BY}> <{NAMESPACE}slices/lifecycle> .\n"
        ));

        // Carrier varieties seed the tag map. All three are always present; the
        // English + French carriers always carry a language-tagged label (so en
        // and fr are "available" — carry literals — regardless of whether the TERM
        // has that content), while the Mandarin carrier's tagged label is gated
        // on `include_zh` (so zh is available only when Mandarin content exists).
        // Each internal x-gmeow-* tag rides lang:carrierTag and its generated
        // (folded) external tag rides gmeow:bcp47Tag on a lang:LanguageVariety —
        // the post-graft shape the tag map is built from.
        const LANG_VARIETY: &str = "https://blackcatinformatics.ca/lang/LanguageVariety";
        const CARRIER_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
        for (local, internal, bcp, label) in [
            ("gmeowEnglish", "x-gmeow-english", "en", Some("English")),
            ("gmeowFrench", "x-gmeow-french", "fr", Some("français")),
            (
                "gmeowMandarin",
                "x-gmeow-mandarin",
                "zh",
                if include_zh { Some("中文") } else { None },
            ),
        ] {
            let s = format!("https://blackcatinformatics.ca/lang/{local}");
            nt.push_str(&format!("<{s}> <{RDF_TYPE}> <{LANG_VARIETY}> .\n"));
            nt.push_str(&format!("<{s}> <{CARRIER_TAG}> \"{internal}\" .\n"));
            nt.push_str(&format!("<{s}> <{NAMESPACE}bcp47Tag> \"{bcp}\" .\n"));
            if let Some(label) = label {
                nt.push_str(&format!("<{s}> <{RDFS_LABEL}> \"{label}\"@{internal} .\n"));
            }
        }

        let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("fixture N-Triples must parse");
        purrdf::gts_write::to_gts(&ds, &RdfLookaside::default(), TEST_PROFILE)
            .expect("fixture must serialize to GTS")
    }

    #[test]
    fn documentation_term_slug_reads_the_documents_inverse() {
        // A doc-entry record documents a term IRI in the graph/documentation NAMED
        // graph; an evidence node documents the SAME term and must be excluded (only
        // the `documentation/term/` subject is the page slug).
        let term = format!("{NAMESPACE}AcceptanceStatus");
        let entry = format!("{NAMESPACE}documentation/term/acceptancestatus-class");
        let evidence =
            format!("{NAMESPACE}documentation/evidence/acceptancestatus-class/competency");
        let documents = format!("{NAMESPACE}documents");
        let doc_graph = format!("{NAMESPACE}graph/documentation");
        let nq = format!(
            "<{entry}> <{documents}> <{term}> <{doc_graph}> .\n\
             <{evidence}> <{documents}> <{term}> <{doc_graph}> .\n"
        );
        let ds = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
            .expect("fixture N-Quads must parse");
        let bytes = purrdf::gts_write::to_gts(&ds, &RdfLookaside::default(), TEST_PROFILE)
            .expect("fixture must serialize to GTS");
        let graph = DescribeGraph::from_gts_bytes(&bytes).expect("load");

        assert_eq!(
            graph.documentation_term_slug(&term).as_deref(),
            Some("acceptancestatus-class"),
            "must read the injective slug from the documents inverse, excluding evidence nodes"
        );
        assert_eq!(
            graph.documentation_term_slug(&format!("{NAMESPACE}NoSuchTerm")),
            None
        );
    }

    #[test]
    fn describe_known_term_returns_prose_and_zero() {
        let gts = multilingual_gts(true, true);
        let (text, code) = describe("SampleTerm", &gts, None, &BTreeSet::new());
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("gmeow:SampleTerm"), "{text}");
        assert!(text.contains("English definition text."), "{text}");
        assert!(text.contains("category: Class"), "{text}");
        assert!(text.contains("slice: lifecycle"), "{text}");
    }

    /// `class_is_modeled` gate (issue: Pydantic model surface, finding F3): a
    /// Class with NO `$defs` entry for its `def_key` must never carry a
    /// `python_model` line, even though the pre-fix gate (`category == "Class" &&
    /// defined_by.is_some()`) would have fabricated one for every documented
    /// class regardless of whether a generated model actually exists. A class
    /// whose `def_key` IS in `modeled_defs` gets the link.
    #[test]
    fn describe_gates_python_model_on_schema_defs_membership() {
        let gts = multilingual_gts(true, true);

        // Empty `$defs` set: SampleTerm is a documented Class, but unmodeled.
        let (text, code) = describe("SampleTerm", &gts, None, &BTreeSet::new());
        assert_eq!(code, 0, "{text}");
        assert!(
            !text.to_lowercase().contains("python model"),
            "an unmodeled class must never carry a python_model line:\n{text}"
        );

        // SampleTerm's bare local name IS its `def_key` in the primary `gmeow`
        // namespace, so this membership set says "SampleTerm has a model".
        let mut modeled = BTreeSet::new();
        modeled.insert("SampleTerm".to_string());
        let (text, code) = describe("SampleTerm", &gts, None, &modeled);
        assert_eq!(code, 0, "{text}");
        assert!(
            text.contains("**Python model:** `gmeow_models.lifecycle.SampleTerm`"),
            "a modeled class must carry the python_model line:\n{text}"
        );
    }

    #[test]
    fn describe_renders_french_without_fallback() {
        let gts = multilingual_gts(true, true);
        let (text, code) = describe("SampleTerm", &gts, Some("fr"), &BTreeSet::new());
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("Définition en français."), "{text}");
        assert!(!text.contains("fallback: en"), "{text}");
    }

    #[test]
    fn describe_renders_mandarin_without_fallback() {
        let gts = multilingual_gts(true, true);
        let (text, code) = describe("SampleTerm", &gts, Some("zh"), &BTreeSet::new());
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("中文定义。"), "{text}");
        assert!(!text.contains("fallback: en"), "{text}");
    }

    #[test]
    fn describe_falls_back_to_english_when_language_absent() {
        // English-only fixture, French requested → the carrier fallback marker.
        let gts = multilingual_gts(false, false);
        let (text, code) = describe("SampleTerm", &gts, Some("fr"), &BTreeSet::new());
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("English definition text."), "{text}");
        assert!(text.contains("fallback: en"), "{text}");
    }

    #[test]
    fn describe_unknown_language_nonzero_and_lists_carriers() {
        // Mandarin literals absent, but zh is a framework CARRIER (a shippable
        // translation target), so it stays requestable — a request falls back to
        // English rather than hard-failing, and every carrier is listed. A
        // truly-unknown tag still hard-fails.
        let gts = multilingual_gts(true, false);
        let (text, code) = describe("SampleTerm", &gts, Some("notatag"), &BTreeSet::new());
        assert_ne!(code, 0, "{text}");
        assert!(
            text.to_lowercase().contains("unknown language tag"),
            "{text}"
        );
        // All three carriers are always requestable (en first, then lexicographic).
        assert!(text.contains("Available languages: en, fr, zh"), "{text}");

        // The contentless zh carrier resolves with the English fallback marker,
        // never an "unknown language" hard-fail.
        let (zh_text, zh_code) = describe("SampleTerm", &gts, Some("zh"), &BTreeSet::new());
        assert_eq!(
            zh_code, 0,
            "a contentless carrier must fall back: {zh_text}"
        );
        assert!(zh_text.contains("fallback: en"), "{zh_text}");
    }

    #[test]
    fn describe_empty_lang_selects_english_carrier() {
        // An explicit empty request maps to the default English carrier.
        let gts = multilingual_gts(true, true);
        let (text, code) = describe("SampleTerm", &gts, Some(""), &BTreeSet::new());
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("English definition text."), "{text}");
        assert!(!text.contains("fallback: en"), "{text}");
    }

    #[test]
    fn describe_unknown_term_returns_nonzero() {
        let gts = multilingual_gts(true, true);
        let (text, code) = describe("NoSuchTermAtAll", &gts, None, &BTreeSet::new());
        assert_ne!(code, 0);
        assert!(text.contains("NoSuchTermAtAll"), "{text}");
    }

    #[test]
    fn describe_ambiguous_prefix_lists_candidates() {
        // `Sample` is not an exact term but prefixes exactly one local name, so it
        // resolves; a shorter, colliding query would list candidates. Here we prove
        // the case-insensitive exact-name path works for the mixed-case query.
        let gts = multilingual_gts(true, true);
        let (_, code) = describe("sampleterm", &gts, None, &BTreeSet::new());
        assert_eq!(code, 0);
    }

    #[test]
    fn resolve_term_handles_prefix_and_curie_forms() {
        let gts = multilingual_gts(true, true);
        let graph = DescribeGraph::from_gts_bytes(&gts).expect("load");
        let (curie, _) = resolve_term(&graph, "gmeow:SampleTerm");
        assert_eq!(
            curie.as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/SampleTerm")
        );
        let (prefix, _) = resolve_term(&graph, "Sample");
        assert_eq!(
            prefix.as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/SampleTerm")
        );
        let (empty, cands) = resolve_term(&graph, "   ");
        assert!(empty.is_none());
        assert!(cands.is_empty());
    }

    #[test]
    fn describe_invalid_gts_bytes_is_nonzero() {
        let (text, code) = describe("SampleTerm", b"not a gts bundle", None, &BTreeSet::new());
        assert_ne!(code, 0);
        assert!(!text.is_empty());
    }
}
