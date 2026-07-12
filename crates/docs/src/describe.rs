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
use gmeow_logic_compile::ingest::{ns_to_prefix, registry_iri, sssom_id};
use gmeow_validate::language_tags::{
    LangSelector, LitDesc, filter_literals, load_tag_map, marked, resolve_lang_input,
    select_literal,
};

use crate::card::{Card, CardDetail, CardFormat, card_json, card_toon, render_card};
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

/// Shorten an IRI to its display CURIE via the canonical prefix registry — every
/// registered namespace (`gmeow:`/`logic:`/`math:`/`lang:` and every external
/// vocabulary in `PREFIX_REGISTRY`, `gufo:` among them), falling back to the bare
/// IRI when no registered namespace prefixes it. This is the single source of
/// truth for CURIE shortening (the same `sssom_id` the canonical-Turtle renderer
/// uses), so a describe card and a projection agree by construction.
fn short(iri: &str) -> String {
    sssom_id(iri, ns_to_prefix())
}

/// The local name of a term IRI: the remainder after stripping the longest
/// registered namespace (`ns_to_prefix()` is sorted longest-namespace-first, so
/// the most specific CURIE wins). Returns the whole IRI when no registered
/// namespace prefixes it (which always contains `/`, so such IRIs are excluded
/// from the resolvable term set — see [`term_iris`]).
fn local_name(iri: &str) -> &str {
    for (ns, _prefix) in ns_to_prefix() {
        if let Some(local) = iri.strip_prefix(ns) {
            return local;
        }
    }
    iri
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

/// The resolvable term set: every default-graph subject that carries a Tier-1
/// describable predicate (`rdfs:isDefinedBy`, `rdfs:label`, or `skos:definition`)
/// and whose registry-local name is a simple local (no nested `/` path segment).
///
/// The set is NAMESPACE-AGNOSTIC — a term in ANY registered namespace
/// (`gmeow:`/`logic:`/`math:`/`lang:` and beyond) is describable — so a new
/// grounding namespace needs no change here. Using the union of Tier-1 predicates
/// (not `isDefinedBy` alone) means a term authored with a label/definition but no
/// owning-slice link is still resolvable.
fn term_iris(graph: &DescribeGraph) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for pred in [RDFS_IS_DEFINED_BY, RDFS_LABEL, SKOS_DEFINITION] {
        for subject in graph.subjects_with_predicate(pred) {
            if !local_name(&subject).contains('/') {
                out.insert(subject);
            }
        }
    }
    out
}

/// The outcome of resolving a user query against the bundle's term set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A unique term IRI.
    Resolved(String),
    /// A bare local name matches terms in more than one namespace. Carries the
    /// candidate display CURIEs (sorted, deduped) — resolution HARD-FAILS rather
    /// than silently pick one (`.goals` NO OPTIONALITY).
    Ambiguous { candidates: Vec<String> },
    /// No term matches. Carries up to ten prefix-match display-CURIE suggestions
    /// (empty when there is nothing close).
    NotFound { suggestions: Vec<String> },
}

/// Resolve a user query to a bundle term IRI, driven by the canonical prefix
/// registry and the bundle's own term set (see [`term_iris`]).
///
/// Accepts a full IRI, a registered CURIE (`gmeow:X`, `logic:X`, `math:X`,
/// `lang:X`, and any other `PREFIX_REGISTRY` prefix), or a bare local name. A bare
/// local name is matched across ALL bundled namespaces — case-sensitive exact,
/// then case-insensitive exact, then a unique case-insensitive prefix. A name that
/// exactly matches terms in more than one namespace is [`Resolution::Ambiguous`]:
/// there is NO silent `gmeow:` precedence. A unique case-insensitive prefix still
/// resolves (the intended fuzzy-completion UX); multiple prefix matches yield
/// [`Resolution::NotFound`] with candidate suggestions (a prefix is a fuzzy query,
/// not a claimed exact name).
pub fn resolve_term(graph: &DescribeGraph, query: &str) -> Resolution {
    let text = query.trim();
    if text.is_empty() {
        return Resolution::NotFound {
            suggestions: Vec::new(),
        };
    }
    let terms = term_iris(graph);

    // 1. Full IRI — a direct membership test.
    if text.starts_with("http://") || text.starts_with("https://") {
        return if terms.contains(text) {
            Resolution::Resolved(text.to_owned())
        } else {
            Resolution::NotFound {
                suggestions: Vec::new(),
            }
        };
    }

    // 2. Registered CURIE `prefix:local` — expand via the registry, then test
    // membership. A known prefix whose expansion is absent is NotFound (not a bare
    // local-name search): the user named a namespace explicitly.
    if let Some((prefix, local)) = text.split_once(':')
        && let Some(ns) = registry_iri(prefix)
    {
        let iri = format!("{ns}{local}");
        return if terms.contains(&iri) {
            Resolution::Resolved(iri)
        } else {
            Resolution::NotFound {
                suggestions: Vec::new(),
            }
        };
    }

    // 3. Bare local name across all bundled namespaces.
    let lower = text.to_lowercase();
    let mut exact: Vec<String> = Vec::new();
    let mut exact_ci: Vec<String> = Vec::new();
    for iri in &terms {
        let local = local_name(iri);
        if local == text {
            exact.push(iri.clone());
        }
        if local.to_lowercase() == lower {
            exact_ci.push(iri.clone());
        }
    }
    // Case-sensitive exact wins first; a cross-namespace collision hard-fails.
    if let [only] = exact.as_slice() {
        return Resolution::Resolved(only.clone());
    }
    if exact.len() > 1 {
        return Resolution::Ambiguous {
            candidates: sorted_curies(exact),
        };
    }
    // Then case-insensitive exact.
    if let [only] = exact_ci.as_slice() {
        return Resolution::Resolved(only.clone());
    }
    if exact_ci.len() > 1 {
        return Resolution::Ambiguous {
            candidates: sorted_curies(exact_ci),
        };
    }
    // Finally, a unique case-insensitive prefix resolves; multiple are suggestions.
    let prefix_matches: Vec<String> = terms
        .iter()
        .filter(|iri| local_name(iri).to_lowercase().starts_with(&lower))
        .cloned()
        .collect();
    if let [only] = prefix_matches.as_slice() {
        return Resolution::Resolved(only.clone());
    }
    let mut suggestions = sorted_curies(prefix_matches);
    suggestions.truncate(10);
    Resolution::NotFound { suggestions }
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

/// Compose the canonical [`Card`] for `term` from the graph.
pub fn build_card(
    graph: &DescribeGraph,
    term: &str,
    selector: &LangSelector,
    tag_map: &std::collections::HashMap<String, String>,
) -> Card {
    let local = local_name(term).to_owned();

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

    // Owning slice display name (the local segment of the isDefinedBy IRI).
    let slice = graph
        .named_objects(term, RDFS_IS_DEFINED_BY)
        .into_iter()
        .next()
        .map(|iri| {
            iri.trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(&iri)
                .to_owned()
        });

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

/// The classified outcome of a [`describe`] call. Lets the CLI map each failure to
/// its OWN typed diagnostic code — rather than lumping load / unknown-language /
/// unresolved / ambiguous failures under a single code — and pick the process exit
/// code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescribeStatus {
    /// A term card was rendered.
    Ok,
    /// The query matched no bundle term.
    Unresolved,
    /// A bare local name matched terms in more than one namespace (no silent pick).
    Ambiguous,
    /// The `--lang` request named a tag the bundle does not carry.
    UnknownLanguage,
    /// The GTS bundle could not be loaded, folded, or projected.
    LoadFailed,
}

impl DescribeStatus {
    /// The process exit code: `0` on success, `1` on any failure. The greppable
    /// distinction between failure kinds is the CLI's typed diagnostic CODE, not
    /// the exit integer, so all failures share a single non-zero exit.
    pub fn exit_code(self) -> i32 {
        match self {
            DescribeStatus::Ok => 0,
            _ => 1,
        }
    }
}

/// The `gmeow describe` entry point: resolve, assemble, and render one term card
/// from a GTS bundle's bytes.
///
/// * `query` — the user's term request (a registered CURIE `gmeow:X`/`logic:X`/
///   `math:X`/`lang:X`, a full IRI, a bare local name, or a case-insensitive
///   prefix).
/// * `gts_bytes` — a GTS bundle (the offline `gmeow.gts`, or a user `--gts`).
/// * `lang` — the already-resolved language request (`None` → the English
///   carrier); the env/`--lang` precedence is the caller's (the consumer bin's)
///   concern.
/// * `format` — the output serialization ([`CardFormat::Prose`] Markdown,
///   [`CardFormat::Json`], or [`CardFormat::Toon`]). The format governs only a
///   successful card render; every failure keeps its diagnostic text and status.
///
/// Returns `(text, status)`: the rendered card with [`DescribeStatus::Ok`], or a
/// diagnostic message with the classifying [`DescribeStatus`] on a load /
/// unresolved / ambiguous / unknown-language failure.
pub fn describe(
    query: &str,
    gts_bytes: &[u8],
    lang: Option<&str>,
    format: CardFormat,
) -> (String, DescribeStatus) {
    let graph = match DescribeGraph::from_gts_bytes(gts_bytes) {
        Ok(graph) => graph,
        Err(e) => return (e.to_string(), DescribeStatus::LoadFailed),
    };

    let term = match resolve_term(&graph, query) {
        Resolution::Resolved(iri) => iri,
        Resolution::Ambiguous { candidates } => {
            let options = candidates
                .iter()
                .map(|c| format!("  {c}"))
                .collect::<Vec<_>>()
                .join("\n");
            return (
                format!(
                    "ambiguous term '{query}' — it names terms in multiple namespaces:\n{options}"
                ),
                DescribeStatus::Ambiguous,
            );
        }
        Resolution::NotFound { suggestions } => {
            if suggestions.is_empty() {
                return (
                    format!("no GMEOW term matches '{query}'"),
                    DescribeStatus::Unresolved,
                );
            }
            let options = suggestions
                .iter()
                .map(|c| format!("  {c}"))
                .collect::<Vec<_>>()
                .join("\n");
            return (
                format!("no exact GMEOW term matches '{query}' — did you mean:\n{options}"),
                DescribeStatus::Unresolved,
            );
        }
    };

    // The tag map + available-language set drive language resolution. The carrier
    // internal→BCP-47 map is a BUNDLE-WIDE fact: the generated `gmeow:bcp47Tag`
    // projection rides the named `lang-projection-corpus` graph, so the map is read
    // from a FLATTENED (all-graphs-unioned) projection of the bundle — the
    // default-graph alone omits the corpus, leaving the fr/zh carriers invisible.
    // This mirrors the consumer bin's `bundle_tag_map`.
    let flat = match purrdf::gts::flattened_dataset_from_bytes(gts_bytes) {
        Ok(ds) => ds,
        Err(e) => {
            return (
                format!("cannot fold bundle for language map: {e}"),
                DescribeStatus::LoadFailed,
            );
        }
    };
    let nt = match purrdf::serialize_dataset(
        &flat,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    ) {
        Ok(bytes) => bytes,
        Err(e) => {
            return (
                format!("cannot project bundle for language map: {e}"),
                DescribeStatus::LoadFailed,
            );
        }
    };
    let tag_map = match load_tag_map(&nt, "n-triples") {
        Ok(map) => map,
        Err(e) => {
            return (
                format!("cannot build language tag map: {e}"),
                DescribeStatus::LoadFailed,
            );
        }
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
                DescribeStatus::UnknownLanguage,
            );
        }
    };

    let card = build_card(&graph, &term, &selector, &tag_map);
    // The prose card header is the term's own display CURIE (`gmeow:Entity`,
    // `lang:Denotation`, …), shortened through the canonical registry. The
    // structured formats carry the full IRI (and every card field) directly.
    let text = match format {
        CardFormat::Prose => render_card(&short(&term), &card, CardDetail::Standard),
        CardFormat::Json => card_json(&card),
        CardFormat::Toon => card_toon(&card),
    };
    (text, DescribeStatus::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::RdfLookaside;

    /// The GTS write profile for test fixtures (arbitrary, deterministic).
    const TEST_PROFILE: &str = "purrdf-test";

    /// Prose-format `describe` — the default used by every language/resolution test
    /// (the JSON/TOON formats are exercised by the CLI live-binary suite).
    fn describe_prose(query: &str, gts: &[u8], lang: Option<&str>) -> (String, DescribeStatus) {
        describe(query, gts, lang, CardFormat::Prose)
    }

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
        let (text, code) = describe_prose("SampleTerm", &gts, None);
        assert_eq!(code, DescribeStatus::Ok, "{text}");
        assert!(text.contains("gmeow:SampleTerm"), "{text}");
        assert!(text.contains("English definition text."), "{text}");
        assert!(text.contains("category: Class"), "{text}");
        assert!(text.contains("slice: lifecycle"), "{text}");
    }

    #[test]
    fn describe_renders_french_without_fallback() {
        let gts = multilingual_gts(true, true);
        let (text, code) = describe_prose("SampleTerm", &gts, Some("fr"));
        assert_eq!(code, DescribeStatus::Ok, "{text}");
        assert!(text.contains("Définition en français."), "{text}");
        assert!(!text.contains("fallback: en"), "{text}");
    }

    #[test]
    fn describe_renders_mandarin_without_fallback() {
        let gts = multilingual_gts(true, true);
        let (text, code) = describe_prose("SampleTerm", &gts, Some("zh"));
        assert_eq!(code, DescribeStatus::Ok, "{text}");
        assert!(text.contains("中文定义。"), "{text}");
        assert!(!text.contains("fallback: en"), "{text}");
    }

    #[test]
    fn describe_falls_back_to_english_when_language_absent() {
        // English-only fixture, French requested → the carrier fallback marker.
        let gts = multilingual_gts(false, false);
        let (text, code) = describe_prose("SampleTerm", &gts, Some("fr"));
        assert_eq!(code, DescribeStatus::Ok, "{text}");
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
        let (text, code) = describe_prose("SampleTerm", &gts, Some("notatag"));
        assert_ne!(code, DescribeStatus::Ok, "{text}");
        assert!(
            text.to_lowercase().contains("unknown language tag"),
            "{text}"
        );
        // All three carriers are always requestable (en first, then lexicographic).
        assert!(text.contains("Available languages: en, fr, zh"), "{text}");

        // The contentless zh carrier resolves with the English fallback marker,
        // never an "unknown language" hard-fail.
        let (zh_text, zh_code) = describe_prose("SampleTerm", &gts, Some("zh"));
        assert_eq!(
            zh_code,
            DescribeStatus::Ok,
            "a contentless carrier must fall back: {zh_text}"
        );
        assert!(zh_text.contains("fallback: en"), "{zh_text}");
    }

    #[test]
    fn describe_empty_lang_selects_english_carrier() {
        // An explicit empty request maps to the default English carrier.
        let gts = multilingual_gts(true, true);
        let (text, code) = describe_prose("SampleTerm", &gts, Some(""));
        assert_eq!(code, DescribeStatus::Ok, "{text}");
        assert!(text.contains("English definition text."), "{text}");
        assert!(!text.contains("fallback: en"), "{text}");
    }

    #[test]
    fn describe_unknown_term_returns_nonzero() {
        let gts = multilingual_gts(true, true);
        let (text, code) = describe_prose("NoSuchTermAtAll", &gts, None);
        assert_eq!(code, DescribeStatus::Unresolved);
        assert!(text.contains("NoSuchTermAtAll"), "{text}");
    }

    #[test]
    fn describe_ambiguous_prefix_lists_candidates() {
        // `Sample` is not an exact term but prefixes exactly one local name, so it
        // resolves; a shorter, colliding query would list candidates. Here we prove
        // the case-insensitive exact-name path works for the mixed-case query.
        let gts = multilingual_gts(true, true);
        let (_, code) = describe_prose("sampleterm", &gts, None);
        assert_eq!(code, DescribeStatus::Ok);
    }

    /// The resolved IRI of a [`Resolution::Resolved`], else `None`.
    fn resolved_iri(r: Resolution) -> Option<String> {
        match r {
            Resolution::Resolved(iri) => Some(iri),
            _ => None,
        }
    }

    #[test]
    fn resolve_term_handles_prefix_and_curie_forms() {
        let gts = multilingual_gts(true, true);
        let graph = DescribeGraph::from_gts_bytes(&gts).expect("load");
        assert_eq!(
            resolved_iri(resolve_term(&graph, "gmeow:SampleTerm")).as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/SampleTerm")
        );
        // Full-IRI form resolves directly.
        assert_eq!(
            resolved_iri(resolve_term(
                &graph,
                "https://blackcatinformatics.ca/gmeow/SampleTerm"
            ))
            .as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/SampleTerm")
        );
        // A unique case-insensitive prefix resolves.
        assert_eq!(
            resolved_iri(resolve_term(&graph, "Sample")).as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/SampleTerm")
        );
        // An empty query is NotFound with no suggestions.
        match resolve_term(&graph, "   ") {
            Resolution::NotFound { suggestions } => assert!(suggestions.is_empty()),
            other => panic!("empty query must be NotFound, got {other:?}"),
        }
    }

    #[test]
    fn describe_invalid_gts_bytes_is_nonzero() {
        let (text, code) = describe_prose("SampleTerm", b"not a gts bundle", None);
        assert_ne!(code, DescribeStatus::Ok);
        assert_eq!(code, DescribeStatus::LoadFailed);
        assert!(!text.is_empty());
    }

    /// The committed, shipped bundle bytes (`generated/dist/gmeow.gts`) — the exact
    /// bytes `gmeow-cli` embeds. Used by the coherence gate.
    fn shipped_bundle_bytes() -> Vec<u8> {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generated/dist/gmeow.gts");
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
    }

    /// The namespace of an IRI: everything up to and including its last `/` or `#`.
    fn namespace_of(iri: &str) -> &str {
        match iri.rfind(['#', '/']) {
            Some(i) => &iri[..=i],
            None => iri,
        }
    }

    /// COHERENCE GATE: every vocabulary term (OWL class/property) in the shipped
    /// bundle must live in a namespace the canonical `PREFIX_REGISTRY` knows. A
    /// describable term in an UNREGISTERED namespace can neither be resolved by CURIE
    /// nor shortened for display — it would be silently undescribable. This turns
    /// "every bundled term resolves" into a machine-checked invariant: add a fifth
    /// grounding slice to the bundle without registering its prefix, and this HARD
    /// FAILS (rather than the term silently vanishing from `describe`/MCP).
    #[test]
    fn every_bundled_term_namespace_is_registered() {
        use gmeow_logic_compile::ingest::PREFIX_REGISTRY;

        let bytes = shipped_bundle_bytes();
        let graph = DescribeGraph::from_gts_bytes(&bytes).expect("load shipped bundle");

        let mut term_subjects: BTreeSet<String> = BTreeSet::new();
        for ty in [
            OWL_CLASS,
            OWL_OBJECT_PROPERTY,
            OWL_DATATYPE_PROPERTY,
            OWL_ANNOTATION_PROPERTY,
        ] {
            term_subjects.extend(graph.subjects_with_object(RDF_TYPE, ty));
        }
        assert!(
            !term_subjects.is_empty(),
            "the shipped bundle declared no OWL terms — the gate would be vacuous"
        );

        let registered: BTreeSet<&str> = PREFIX_REGISTRY.iter().map(|(_, ns)| *ns).collect();
        let unregistered: BTreeSet<&str> = term_subjects
            .iter()
            .map(|s| namespace_of(s))
            .filter(|ns| !registered.contains(ns))
            .collect();
        assert!(
            unregistered.is_empty(),
            "describable OWL terms live in namespaces absent from the canonical PREFIX_REGISTRY \
             (they can neither resolve by CURIE nor shorten — register their prefixes): {unregistered:?}"
        );

        // Positive guard: the four grounding namespaces are actually present as term
        // subjects (else the gate is vacuously green for a namespace that silently
        // dropped out of the bundle).
        for prefix in ["gmeow", "logic", "math", "lang"] {
            let ns = registry_iri(prefix).expect("grounding prefix registered");
            assert!(
                term_subjects.iter().any(|s| s.starts_with(ns)),
                "no describable OWL term found in the `{prefix}:` grounding namespace"
            );
        }
    }
}
