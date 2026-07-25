// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `export` export leaf: CSVW/Markdown/JSONL/llms.txt + the
//! dataset/semantic-web tiers (N-Quads, TriG, statements JSONL, SKOS, OBO Graphs,
//! ShEx) under git-ignored `dist/`.
//!
//! The native flat-export leaf: reads ONLY the committed GTS snapshot (the narrow
//! waist) through a fold view that mirrors
//! `gmeow_tools.gts_views.FoldView`, collects every class/property/individual as a
//! [`Term`], then renders the flattened views. Outputs live under git-ignored
//! `dist/`, so there is NO committed byte-parity gate — the bar is
//! structurally-valid, deterministic, non-empty output faithful to the Python
//! generator's format. Everything is sorted (BTreeMap/BTreeSet) for determinism.
//!
//! The lossless N-Quads / TriG forms delegate to the gmeow-gts Rust serializers
//! (`purrdf::gts::nquads::to_nquads` / `purrdf::gts::trig::to_trig`), with internal
//! `x-gmeow-*` language tags remapped to public BCP-47 at the projection boundary
//! exactly as the Python `write_nquads` / `write_trig` do.
//!
//! SKOS ([`render_skos`]), OBO Graphs ([`render_obographs`]), and CSVW
//! ([`render_csvw`]) are purrdf 0.7.0 native projections
//! (`purrdf::project_skos` / `purrdf::project_obo_graphs` /
//! `purrdf::project_csvw_exact`), retiring the former hand-rolled SKOS Turtle / OBO
//! Graphs JSON writers and the curated class/property/individual CSV + CSVW
//! descriptor tables. gmeow owns only the scoping decision (see
//! [`skos_source_dataset`] / [`obo_graphs_source_dataset`] / [`default_graph_dataset`])
//! and the caller-owned semantic-role vocabulary (see [`skos_config`] /
//! [`obo_graphs_config`] / [`csvw_config`]); purrdf owns the encoding. The CSVW
//! surface is a deliberate scope change: it now emits purrdf's generic, always-
//! lossless RDF-1.2-in-CSV package (`csvw-metadata.json` +
//! `terms.csv`/`quads.csv`/`reifiers.csv`/`annotations.csv`) instead of the retired
//! curated term-dictionary tables.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gmeow_logic_compile::ingest::{ns_to_prefix, registry_iri};
use gmeow_validate::language_tags::{
    self, LitDesc, filter_literals as authority_filter_literals, is_internal_tag,
    marked as authority_marked, select_literal as authority_select_literal,
};
use purrdf::{RdfDataset, TermId, TermRef};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

include!("lpg_prefixes.rs");

pub const DIST_DIR: &str = "dist";

const OWL: &str = "http://www.w3.org/2002/07/owl#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
const ALIGNMENTS_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/alignments";
const DOCUMENTATION_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/documentation";

const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// The lowered-logic (OntoUML/UFO discipline) namespace; co-asserted `rdf:type`
/// values under it become the term's logic stereotypes. Mirrors
/// `gmeow_docs::model::LOGIC_NS`.
use gmeow_ns::LOGIC_NS;

/// The carrier variety class: the internal `x-gmeow-*` tag rides `lang:carrierTag`
/// on a `lang:LanguageVariety` since the lang: graft, and the generated
/// `gmeow:bcp47Tag` is folded onto the SAME variety by the `bcp47` projection.
const LANGUAGE_VARIETY_CLASS: &str = "https://blackcatinformatics.ca/lang/LanguageVariety";
const CARRIER_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
const BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";

// ── curie ──────────────────────────────────────────────────────────────────────

pub(crate) fn curie(iri: &str) -> String {
    for (prefix, ns) in PREFIXES_BY_LEN.iter() {
        if let Some(rest) = iri.strip_prefix(ns) {
            return format!("{prefix}:{rest}");
        }
    }
    iri.to_string()
}

/// The GMEOW grounding namespaces whose OWL terms are describable and visible to the
/// consumer/MCP term surface — the super-ontology's own vocabulary. Resolved from the
/// canonical [`gmeow_logic_compile::ingest::PREFIX_REGISTRY`] so a new grounding
/// namespace is a one-line registry change, not a code change here (MAXIMAL GROUNDING:
/// the `math`/`logic`/`lang` grounding slices are first-class, not gmeow-only).
fn grounding_namespaces() -> [&'static str; 4] {
    ["gmeow", "logic", "math", "lang"]
        .map(|p| registry_iri(p).expect("a grounding prefix must be in the canonical registry"))
}

/// The registry-local name of a term IRI — the remainder after stripping the longest
/// registered namespace (`ns_to_prefix()` is longest-first). Returns the whole IRI when
/// no registered namespace prefixes it.
fn registry_local(iri: &str) -> &str {
    for (ns, _prefix) in ns_to_prefix() {
        if let Some(local) = iri.strip_prefix(ns) {
            return local;
        }
    }
    iri
}

// ── FoldView: read-side idioms over a folded gts Graph (mirror gts_views.py) ───

pub(crate) struct FoldView<'a> {
    dataset: &'a RdfDataset,
    iri_index: BTreeMap<&'a str, usize>,
    /// scope (graph IRI or "" for default) → subject tid → [(p, o)]
    spo: BTreeMap<String, BTreeMap<usize, Vec<(usize, usize)>>>,
    /// scope → (p, o) → [subject]
    po: BTreeMap<String, BTreeMap<(usize, usize), Vec<usize>>>,
    tag_map: BTreeMap<String, String>,
    /// Cached `HashMap` form of `tag_map` — built once at construction and
    /// passed by reference to `select_literal` / `filter_literals` to avoid
    /// re-allocating on every per-literal call in the hot export fold.
    tag_map_hash: HashMap<String, String>,
    /// Requested public BCP-47 tags in precedence order (mirrors
    /// `LangSelector.requested`). The export generator uses `["en"]`; the MCP
    /// consumer threads a per-call selection (e.g. `["fr"]`).
    requested: Vec<String>,
}

pub(crate) const DEFAULT_SCOPE: &str = "";
pub(crate) const ALL_SCOPE: &str = "__all__";

impl<'a> FoldView<'a> {
    /// The English-only default view — `requested == ["en"]`. Every existing
    /// export-leaf caller keeps this exact behavior.
    pub(crate) fn new(dataset: &'a RdfDataset) -> Self {
        Self::with_requested(dataset, vec!["en".to_string()])
    }

    /// A selector-aware view over the in-memory carrier dataset (GTS is exit-only):
    /// literal selection honors `requested` (public BCP-47 tags in precedence order)
    /// before the English / first-tagged / untagged fallback chain. Mirrors
    /// `language_tags.select_literal` / `filter_literals`. The term ids are the
    /// dataset's frozen [`TermId`] ordinals.
    pub(crate) fn with_requested(dataset: &'a RdfDataset, requested: Vec<String>) -> Self {
        let mut iri_index: BTreeMap<&'a str, usize> = BTreeMap::new();
        for tid in 0..dataset.term_count() {
            if let TermRef::Iri(v) = dataset.resolve(TermId::from_index(tid as u32)) {
                iri_index.entry(v).or_insert(tid);
            }
        }
        let requested = if requested.is_empty() {
            vec!["en".to_string()]
        } else {
            requested.iter().map(|r| r.to_ascii_lowercase()).collect()
        };
        let mut view = FoldView {
            dataset,
            iri_index,
            spo: BTreeMap::new(),
            po: BTreeMap::new(),
            tag_map: BTreeMap::new(),
            tag_map_hash: HashMap::new(),
            requested,
        };
        view.build_indexes();
        view.tag_map = view.build_tag_map();
        view.tag_map_hash = view
            .tag_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        view
    }

    fn build_indexes(&mut self) {
        // Per-scope spo/po, plus an ALL scope spanning every graph.
        let mut spo: BTreeMap<String, BTreeMap<usize, Vec<(usize, usize)>>> = BTreeMap::new();
        let mut po: BTreeMap<String, BTreeMap<(usize, usize), Vec<usize>>> = BTreeMap::new();
        for q in self.dataset.quads() {
            let (s, p, o) = (q.s.index(), q.p.index(), q.o.index());
            let scope = match q.g {
                None => DEFAULT_SCOPE.to_string(),
                Some(gid) => self.iri_or_empty(gid.index()),
            };
            for key in [scope.clone(), ALL_SCOPE.to_string()] {
                spo.entry(key.clone())
                    .or_default()
                    .entry(s)
                    .or_default()
                    .push((p, o));
                po.entry(key)
                    .or_default()
                    .entry((p, o))
                    .or_default()
                    .push(s);
            }
        }
        self.spo = spo;
        self.po = po;
    }

    /// Resolve a term id to its borrowed view over the frozen dataset arena.
    fn tref(&self, tid: usize) -> TermRef<'a> {
        self.dataset.resolve(TermId::from_index(tid as u32))
    }
    /// The IRI string of `tid` (or `""` for any non-IRI term).
    fn iri_or_empty(&self, tid: usize) -> String {
        match self.tref(tid) {
            TermRef::Iri(s) => s.to_string(),
            _ => String::new(),
        }
    }
    pub(crate) fn is_iri(&self, tid: usize) -> bool {
        matches!(self.tref(tid), TermRef::Iri(_))
    }
    pub(crate) fn is_bnode(&self, tid: usize) -> bool {
        matches!(self.tref(tid), TermRef::Blank { .. })
    }
    pub(crate) fn is_literal(&self, tid: usize) -> bool {
        matches!(self.tref(tid), TermRef::Literal { .. })
    }
    pub(crate) fn lex(&self, tid: usize) -> &'a str {
        match self.tref(tid) {
            TermRef::Iri(s) => s,
            TermRef::Blank { label, .. } => label,
            TermRef::Literal { lexical, .. } => lexical,
            TermRef::Triple { .. } => "",
        }
    }
    fn lang(&self, tid: usize) -> Option<&'a str> {
        match self.tref(tid) {
            TermRef::Literal { language, .. } => language,
            _ => None,
        }
    }
    fn datatype(&self, tid: usize) -> String {
        match self.tref(tid) {
            TermRef::Literal { datatype, .. } => self.iri_or_empty(datatype.index()),
            _ => String::new(),
        }
    }
    pub(crate) fn tid_of_iri(&self, iri: &str) -> Option<usize> {
        self.iri_index.get(iri).copied()
    }

    /// The RDF-1.2 annotation side-table as `(reifier, predicate, object)` tid triples.
    pub(crate) fn annotations(&self) -> Vec<(usize, usize, usize)> {
        self.dataset
            .annotations()
            .map(|(r, p, o)| (r.index(), p.index(), o.index()))
            .collect()
    }

    /// The RDF-1.2 reifier side-table as `(reifier, (s, p, o))` tid rows.
    pub(crate) fn reifiers(&self) -> Vec<(usize, (usize, usize, usize))> {
        self.dataset
            .reifiers()
            .filter_map(|(rid, triple)| match self.dataset.resolve(triple) {
                TermRef::Triple { s, p, o } => {
                    Some((rid.index(), (s.index(), p.index(), o.index())))
                }
                _ => None,
            })
            .collect()
    }

    /// Subjects with `rdf:type <class_iri>` in scope, id-sorted unique.
    pub(crate) fn subjects_by_type(&self, class_iri: &str, scope: &str) -> Vec<usize> {
        let (Some(type_tid), Some(class_tid)) =
            (self.tid_of_iri(RDF_TYPE), self.tid_of_iri(class_iri))
        else {
            return Vec::new();
        };
        let mut out: BTreeSet<usize> = BTreeSet::new();
        if let Some(idx) = self.po.get(scope)
            && let Some(subjects) = idx.get(&(type_tid, class_tid))
        {
            out.extend(subjects.iter().copied());
        }
        out.into_iter().collect()
    }

    /// Objects of `(s, p, ?)` in scope, id-sorted unique.
    pub(crate) fn objects(&self, s_tid: usize, p_iri: &str, scope: &str) -> Vec<usize> {
        let Some(p_tid) = self.tid_of_iri(p_iri) else {
            return Vec::new();
        };
        let mut out: BTreeSet<usize> = BTreeSet::new();
        if let Some(idx) = self.spo.get(scope)
            && let Some(rows) = idx.get(&s_tid)
        {
            for &(p, o) in rows {
                if p == p_tid {
                    out.insert(o);
                }
            }
        }
        out.into_iter().collect()
    }

    /// One object of `(s, p, ?)` — the nq-token-smallest (never graph order).
    fn value(&self, s_tid: usize, p_iri: &str, scope: &str) -> Option<usize> {
        let candidates = self.objects(s_tid, p_iri, scope);
        candidates
            .into_iter()
            .min_by(|&a, &b| self.nq_token(a).cmp(&self.nq_token(b)))
    }

    /// All `(p, o)` pairs for a subject in scope, id-sorted unique.
    pub(crate) fn predicate_objects(&self, s_tid: usize, scope: &str) -> Vec<(usize, usize)> {
        let mut out: BTreeSet<(usize, usize)> = BTreeSet::new();
        if let Some(idx) = self.spo.get(scope)
            && let Some(rows) = idx.get(&s_tid)
        {
            out.extend(rows.iter().copied());
        }
        out.into_iter().collect()
    }

    pub(crate) fn has(&self, s_tid: usize, p_iri: &str, o_tid: usize, scope: &str) -> bool {
        let Some(p_tid) = self.tid_of_iri(p_iri) else {
            return false;
        };
        self.spo
            .get(scope)
            .and_then(|idx| idx.get(&s_tid))
            .map(|rows| rows.contains(&(p_tid, o_tid)))
            .unwrap_or(false)
    }

    fn rdf_list(&self, head_tid: usize, scope: &str) -> Vec<usize> {
        let nil = self.tid_of_iri(RDF_NIL);
        let mut out: Vec<usize> = Vec::new();
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut current = Some(head_tid);
        while let Some(c) = current {
            if Some(c) == nil || seen.contains(&c) {
                break;
            }
            seen.insert(c);
            if let Some(first) = self.value(c, RDF_FIRST, scope) {
                out.push(first);
            }
            current = self.value(c, RDF_REST, scope);
        }
        out
    }

    /// The canonical N-Triples token — stable sort/display key (mirror term_token).
    fn nq_token(&self, tid: usize) -> String {
        match self.tref(tid) {
            TermRef::Iri(s) => format!("<{s}>"),
            TermRef::Blank { label, .. } => format!("_:{label}"),
            TermRef::Literal {
                lexical,
                language,
                datatype,
                ..
            } => {
                let lex = nt_escape(lexical);
                if let Some(lang) = language {
                    format!("\"{lex}\"@{lang}")
                } else {
                    let dt = self.iri_or_empty(datatype.index());
                    if dt == format!("{XSD}string") {
                        format!("\"{lex}\"")
                    } else {
                        format!("\"{lex}\"^^<{dt}>")
                    }
                }
            }
            TermRef::Triple { .. } => format!("<<tid:{tid}>>"),
        }
    }

    fn tag_map(&self) -> &BTreeMap<String, String> {
        &self.tag_map
    }

    fn build_tag_map(&self) -> BTreeMap<String, String> {
        let mut out: BTreeMap<String, String> = BTreeMap::new();
        // The internal→BCP-47 pair rides the carrier VARIETY: lang:carrierTag (the
        // x-gmeow-* tag) and the folded gmeow:bcp47Tag both sit on the same
        // lang:LanguageVariety, so scan the variety subjects.
        for variety_tid in self.subjects_by_type(LANGUAGE_VARIETY_CLASS, ALL_SCOPE) {
            let internal = self.value(variety_tid, CARRIER_TAG, ALL_SCOPE);
            let bcp = self.value(variety_tid, BCP47_TAG, ALL_SCOPE);
            if let (Some(i), Some(b)) = (internal, bcp) {
                out.insert(self.lex(i).to_string(), self.lex(b).to_string());
            }
        }
        out
    }

    /// Selector-aware single text + fallback flag (English-only default selector).
    /// Mirrors `public_text_with_fallback` with `selector.requested == ("en",)`.
    pub(crate) fn public_text_with_fallback(&self, s_tid: usize, p_iri: &str) -> (String, bool) {
        let candidates: Vec<usize> = self
            .objects(s_tid, p_iri, DEFAULT_SCOPE)
            .into_iter()
            .filter(|&o| self.is_literal(o))
            .collect();
        match self.select_literal(&candidates) {
            Some((text, _lang, fallback)) => (text, fallback),
            None => (String::new(), false),
        }
    }

    /// All requested-language texts (`en` only) + fallback. Mirrors `public_texts`.
    /// Returns `(text, bcp47_tag, is_fallback)` rows.
    fn public_texts(&self, s_tid: usize, p_iri: &str) -> Vec<(String, Option<String>, bool)> {
        let candidates: Vec<usize> = self
            .objects(s_tid, p_iri, DEFAULT_SCOPE)
            .into_iter()
            .filter(|&o| self.is_literal(o))
            .collect();
        self.filter_literals(&candidates)
    }

    /// Public BCP-47 tag for a literal tid (or `None`).
    fn bcp47_for(&self, tid: usize) -> Option<String> {
        let lang = self.lang(tid)?;
        if is_internal_tag(lang) {
            Some(
                self.tag_map
                    .get(lang)
                    .cloned()
                    .unwrap_or_else(|| lang.to_string()),
            )
        } else {
            Some(lang.to_string())
        }
    }

    /// Build the canonical-authority [`LitDesc`] slice for a list of literal tids
    /// (lexical form + language tag), plus the matching `HashMap` tag map the
    /// authority API takes. The returned descriptors share the `candidates`
    /// indexing, so a [`language_tags::Selection`] `index` maps straight back to
    /// `candidates[index]`.
    fn lit_descs(&self, candidates: &[usize]) -> Vec<LitDesc> {
        candidates
            .iter()
            .map(|&tid| LitDesc {
                lexical: self.lex(tid).to_string(),
                language: self.lang(tid).map(str::to_string),
            })
            .collect()
    }

    /// Resolve a [`language_tags::Selection`] back into the FoldView row shape
    /// `(text, public_bcp47, is_fallback)`. The public tag is the literal's bucket
    /// tag ([`Self::bcp47_for`]) — `Some` for any tagged literal — not the
    /// authority's `retag_to` (which is `None` for already-public tags).
    fn selection_row(
        &self,
        candidates: &[usize],
        sel: &language_tags::Selection,
    ) -> (String, Option<String>, bool) {
        let tid = candidates[sel.index];
        (
            self.lex(tid).to_string(),
            self.bcp47_for(tid),
            sel.is_fallback,
        )
    }

    /// `select_literal`: the single best literal for `self.requested`, via the
    /// canonical [`language_tags::select_literal`] authority.
    fn select_literal(&self, candidates: &[usize]) -> Option<(String, Option<String>, bool)> {
        let descs = self.lit_descs(candidates);
        authority_select_literal(&descs, &self.requested, &self.tag_map_hash)
            .map(|sel| self.selection_row(candidates, &sel))
    }

    /// `filter_literals`: every requested-language literal (or the fallback), via
    /// the canonical [`language_tags::filter_literals`] authority.
    fn filter_literals(&self, candidates: &[usize]) -> Vec<(String, Option<String>, bool)> {
        let descs = self.lit_descs(candidates);
        authority_filter_literals(&descs, &self.requested, &self.tag_map_hash)
            .iter()
            .map(|sel| self.selection_row(candidates, sel))
            .collect()
    }
}

/// N-Triples literal escaping for the nq token (subset rdflib emits).
fn nt_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

// ── Term model (mirror export.Term + as_record) ────────────────────────────────

#[derive(Clone, Default)]
pub(crate) struct Term {
    pub(crate) category: &'static str, // "class" | "property" | "individual"
    pub(crate) iri: String,
    pub(crate) curie: String,
    pub(crate) label: String,
    pub(crate) definition: String,
    pub(crate) labels: BTreeMap<String, String>,
    pub(crate) definitions: BTreeMap<String, String>,
    pub(crate) label_fallback: bool,
    pub(crate) definition_fallback: bool,
    pub(crate) parents: Vec<String>,
    pub(crate) prop_kind: &'static str,
    pub(crate) domain: String,
    pub(crate) range: String,
    pub(crate) functional: bool,
    pub(crate) sub_property_of: Vec<String>,
    pub(crate) types: Vec<String>,
    pub(crate) alignments: Vec<String>,
    pub(crate) box_roles: Vec<String>,
    pub(crate) scope_notes: Vec<String>,
    pub(crate) examples: Vec<String>,
    pub(crate) use_when: Vec<String>,
    pub(crate) avoid_when: Vec<String>,
    pub(crate) how_to_use: Vec<String>,
    pub(crate) use_for_consumer: Vec<String>,
    pub(crate) avoid_for_consumer: Vec<String>,
    /// `logic:*` stereotype CURIEs co-asserted as `rdf:type` (sorted/deduped).
    /// Mirrors `gmeow_docs::model::logic_stereotypes` so the shared term card
    /// carries the lowered OntoUML/UFO discipline.
    pub(crate) logic_stereotypes: Vec<String>,
    /// Related-term CURIEs: the union of `skos:related`, `gmeow:pairsWith`, and
    /// `rdfs:seeAlso` objects (sorted/deduped). Read per-term directly from the
    /// folded default graph; not bidirectionally reconciled (see G1 note).
    pub(crate) related_terms: Vec<String>,
    /// The owning slice IRI, recovered from the `gmeow:graph/documentation` named
    /// graph (`gmeow:DocumentedTerm` ⨝ `gmeow:docOwnerSlice`, keyed by the term
    /// IRI via `gmeow:documents`). Empty when the doc graph is absent. The shared
    /// term card renders its local name as the `slice:` header, matching
    /// the docs-site card's `local_name(owner_slice)` — so the MCP card carries
    /// the same slice provenance the published docs do.
    pub(crate) owner_slice: String,
}

// ── JSON helpers (json.dumps ensure_ascii=False, insertion-ordered objects) ────

enum J {
    Bool(bool),
    Str(String),
    /// A pre-formatted numeric token rendered verbatim (xsd:integer/decimal lexeme).
    RawNum(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    /// `json.dumps(obj, ensure_ascii=False)` compact form (`, ` / `: ` separators).
    fn compact(&self, out: &mut String) {
        self.compact_opt(out, false);
    }

    /// `json.dumps(obj)` compact form with the DEFAULT `ensure_ascii=True` — every
    /// non-ASCII scalar is `\uXXXX`-escaped (astral chars as surrogate pairs).
    /// The MCP `lookup_term` tool returns `json.dumps(result)` (no
    /// `ensure_ascii=False`), so its envelope is ASCII-escaped — unlike the
    /// OKF index, which is explicitly `ensure_ascii=False`. Only the MCP consumer
    /// surface (`mcp`/`test`) calls it.
    fn compact_ascii(&self, out: &mut String) {
        self.compact_opt(out, true);
    }

    fn compact_opt(&self, out: &mut String, ascii: bool) {
        let key_or_str = |s: &str| {
            if ascii {
                json_str_ascii(s)
            } else {
                json_str(s)
            }
        };
        match self {
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Str(s) => out.push_str(&key_or_str(s)),
            J::RawNum(n) => out.push_str(n),
            J::Arr(items) => {
                out.push('[');
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    it.compact_opt(out, ascii);
                }
                out.push(']');
            }
            J::Obj(entries) => {
                out.push('{');
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&key_or_str(k));
                    out.push_str(": ");
                    v.compact_opt(out, ascii);
                }
                out.push('}');
            }
        }
    }
}

fn jarr_str(items: &[String]) -> J {
    J::Arr(items.iter().map(|s| J::Str(s.clone())).collect())
}

/// `json.dumps(s, ensure_ascii=False)` of a string.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `json.dumps(s)` of a string with the DEFAULT `ensure_ascii=True`: ASCII
/// printables `0x20..=0x7E` (bar `"`/`\`) pass through; everything else is
/// `\uXXXX`-escaped, with astral scalars (> U+FFFF) as UTF-16 surrogate pairs —
/// byte-for-byte what CPython's `json` encoder emits.
fn json_str_ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (0x20..0x7f).contains(&(c as u32)) => out.push(c),
            c => {
                let cp = c as u32;
                if cp <= 0xffff {
                    out.push_str(&format!("\\u{cp:04x}"));
                } else {
                    let v = cp - 0x10000;
                    let hi = 0xd800 + (v >> 10);
                    let lo = 0xdc00 + (v & 0x3ff);
                    out.push_str(&format!("\\u{hi:04x}\\u{lo:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

// ── collect_terms (mirror export.collect_terms; English-only selector) ─────────

fn fold_curies(view: &FoldView, s_tid: usize, p_iri: &str) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for o in view.objects(s_tid, p_iri, DEFAULT_SCOPE) {
        if view.is_iri(o) {
            out.insert(curie(view.lex(o)));
        }
    }
    out.into_iter().collect()
}

fn fold_public_texts(view: &FoldView, s_tid: usize, p_iri: &str) -> Vec<String> {
    view.public_texts(s_tid, p_iri)
        .into_iter()
        .map(|(text, _lang, _fallback)| text)
        .collect()
}

const ALIGN_TAGS: &[(&str, &str)] = &[
    ("equivalentClass", "equivalentClass"),
    ("equivalentProperty", "equivalentProperty"),
    ("subClassOf", "subClassOf"),
    ("subPropertyOf", "subPropertyOf"),
    ("closeMatch", "closeMatch"),
    ("exactMatch", "exactMatch"),
    ("relatedMatch", "relatedMatch"),
];

fn align_tag(pred_iri: &str) -> String {
    let local = pred_iri.rsplit(['#', '/']).next().unwrap_or(pred_iri);
    for (key, tag) in ALIGN_TAGS {
        if *key == local
            && (pred_iri == format!("{OWL}{key}")
                || pred_iri == format!("{RDFS}{key}")
                || pred_iri == format!("{SKOS}{key}"))
        {
            return tag.to_string();
        }
    }
    curie(pred_iri)
}

fn fold_alignments(view: &FoldView, s_tid: usize) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for (p, o) in view.predicate_objects(s_tid, ALIGNMENTS_GRAPH) {
        let tag = align_tag(view.lex(p));
        out.insert(format!("{tag}={}", curie(view.lex(o))));
    }
    out.into_iter().collect()
}

fn fold_describe_node(view: &FoldView, tid: usize) -> String {
    if view.is_iri(tid) {
        return curie(view.lex(tid));
    }
    if view.is_bnode(tid) {
        if let Some(head) = view.value(tid, &format!("{OWL}unionOf"), DEFAULT_SCOPE) {
            return view
                .rdf_list(head, DEFAULT_SCOPE)
                .into_iter()
                .map(|item| fold_describe_node(view, item))
                .collect::<Vec<_>>()
                .join(" | ");
        }
        if let Some(head) = view.value(tid, &format!("{OWL}intersectionOf"), DEFAULT_SCOPE) {
            return view
                .rdf_list(head, DEFAULT_SCOPE)
                .into_iter()
                .map(|item| fold_describe_node(view, item))
                .collect::<Vec<_>>()
                .join(" & ");
        }
    }
    view.lex(tid).to_string()
}

fn term_label_def(view: &FoldView, t: usize, term: &mut Term) {
    let label_iri = format!("{RDFS}label");
    let definition_iri = format!("{SKOS}definition");
    let (label, label_fb) = view.public_text_with_fallback(t, &label_iri);
    let (definition, def_fb) = view.public_text_with_fallback(t, &definition_iri);
    term.label = label;
    term.label_fallback = label_fb;
    term.definition = definition;
    term.definition_fallback = def_fb;
    for (text, lang, fallback) in view.public_texts(t, &label_iri) {
        if let Some(l) = lang
            && !fallback
            && !term.labels.contains_key(&l)
        {
            term.labels.insert(l, text);
        }
    }
    for (text, lang, fallback) in view.public_texts(t, &definition_iri) {
        if let Some(l) = lang
            && !fallback
            && !term.definitions.contains_key(&l)
        {
            term.definitions.insert(l, text);
        }
    }
}

/// The `logic:*` stereotype CURIEs of a subject: its `rdf:type` values under the
/// `logic:` namespace, CURIE-rendered (sorted/deduped). Mirrors
/// `gmeow_docs::model::logic_stereotypes`.
fn fold_logic_stereotypes(view: &FoldView, t: usize) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for o in view.objects(t, RDF_TYPE, DEFAULT_SCOPE) {
        if view.is_iri(o) && view.lex(o).starts_with(LOGIC_NS) {
            out.insert(curie(view.lex(o)));
        }
    }
    out.into_iter().collect()
}

/// The related-term CURIEs of a subject: the union of `skos:related`,
/// `gmeow:pairsWith`, and `rdfs:seeAlso` objects (sorted/deduped). Read per-term
/// directly (NOT bidirectionally reconciled — see G1 note). Mirrors the forward
/// read in `gmeow_docs::model::extract_terms`.
fn fold_related_terms(view: &FoldView, t: usize) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for pred in [
        format!("{SKOS}related"),
        format!("{NAMESPACE}pairsWith"),
        format!("{RDFS}seeAlso"),
    ] {
        for o in view.objects(t, &pred, DEFAULT_SCOPE) {
            if view.is_iri(o) {
                out.insert(curie(view.lex(o)));
            }
        }
    }
    out.into_iter().collect()
}

/// Build a `term-IRI → owning-slice-IRI` map from the `gmeow:graph/documentation`
/// named graph (`gmeow:DocumentedTerm` ⨝ `gmeow:docOwnerSlice`, keyed by
/// `gmeow:documents`). This is the term→slice provenance the docs generator
/// dogfoods into the bundle (`gmeow_docs::rdf::to_gmeow_rdf`), recovered here so
/// the folded/MCP term card carries the same slice the published docs do. Empty
/// when the documentation graph is absent (then the card omits the slice line).
fn fold_owner_slice_map(view: &FoldView) -> std::collections::HashMap<String, String> {
    let documents = format!("{NAMESPACE}documents");
    let owner_slice = format!("{NAMESPACE}docOwnerSlice");
    let mut map = std::collections::HashMap::new();
    for s in view.subjects_by_type(&format!("{NAMESPACE}DocumentedTerm"), DOCUMENTATION_GRAPH) {
        let (Some(iri_tid), Some(slice_tid)) = (
            view.value(s, &documents, DOCUMENTATION_GRAPH),
            view.value(s, &owner_slice, DOCUMENTATION_GRAPH),
        ) else {
            continue;
        };
        map.insert(
            view.lex(iri_tid).to_string(),
            view.lex(slice_tid).to_string(),
        );
    }
    map
}

/// The local name of an IRI: the tail after the last `/` or `#`. Mirrors
/// `gmeow_docs::render::local_name` so the folded card's `slice:` value matches
/// the docs-site card byte-for-byte.
fn slice_local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

fn fold_advisory(view: &FoldView, t: usize, term: &mut Term) {
    term.box_roles = fold_curies(view, t, &format!("{NAMESPACE}graphBoxRole"));
    term.scope_notes = fold_public_texts(view, t, &format!("{SKOS}scopeNote"));
    term.examples = fold_public_texts(view, t, &format!("{SKOS}example"));
    term.use_when = fold_public_texts(view, t, &format!("{NAMESPACE}useWhen"));
    term.avoid_when = fold_public_texts(view, t, &format!("{NAMESPACE}avoidWhen"));
    term.how_to_use = fold_public_texts(view, t, &format!("{NAMESPACE}howToUse"));
    term.use_for_consumer = fold_curies(view, t, &format!("{NAMESPACE}useForConsumer"));
    term.avoid_for_consumer = fold_curies(view, t, &format!("{NAMESPACE}avoidForConsumer"));
    term.logic_stereotypes = fold_logic_stereotypes(view, t);
    term.related_terms = fold_related_terms(view, t);
}

const PROPERTY_KINDS: &[(&str, &str)] = &[
    ("ObjectProperty", "object"),
    ("DatatypeProperty", "datatype"),
    ("AnnotationProperty", "annotation"),
];

pub(crate) fn collect_terms(view: &FoldView) -> Vec<Term> {
    // Every GMEOW grounding namespace is a term surface, not just `gmeow:` — the
    // `logic:`/`math:`/`lang:` grounding slices are describable and MCP-visible.
    let grounding = grounding_namespaces();
    let in_namespace = |view: &FoldView, tid: usize| {
        view.is_iri(tid) && {
            let iri = view.lex(tid);
            grounding.iter().any(|ns| iri.starts_with(ns))
        }
    };

    let classes: BTreeSet<usize> = view
        .subjects_by_type(&format!("{OWL}Class"), DEFAULT_SCOPE)
        .into_iter()
        .filter(|&t| in_namespace(view, t))
        .collect();

    let mut properties: BTreeMap<usize, &'static str> = BTreeMap::new();
    for (ptype, kind) in PROPERTY_KINDS {
        for t in view.subjects_by_type(&format!("{OWL}{ptype}"), DEFAULT_SCOPE) {
            if in_namespace(view, t) {
                properties.insert(t, kind);
            }
        }
    }

    let functional_tid = view.tid_of_iri(&format!("{OWL}FunctionalProperty"));

    let mut terms: Vec<Term> = Vec::new();

    for &t in &classes {
        let mut term = Term {
            category: "class",
            iri: view.lex(t).to_string(),
            curie: curie(view.lex(t)),
            parents: fold_curies(view, t, &format!("{RDFS}subClassOf")),
            alignments: fold_alignments(view, t),
            ..Term::default()
        };
        fold_advisory(view, t, &mut term);
        term_label_def(view, t, &mut term);
        terms.push(term);
    }

    for (&t, &kind) in &properties {
        let domain_tid = view.value(t, &format!("{RDFS}domain"), DEFAULT_SCOPE);
        let range_tid = view.value(t, &format!("{RDFS}range"), DEFAULT_SCOPE);
        let functional =
            matches!(functional_tid, Some(ft) if view.has(t, RDF_TYPE, ft, DEFAULT_SCOPE));
        let mut term = Term {
            category: "property",
            iri: view.lex(t).to_string(),
            curie: curie(view.lex(t)),
            prop_kind: kind,
            domain: domain_tid
                .map(|d| fold_describe_node(view, d))
                .unwrap_or_default(),
            range: range_tid
                .map(|r| fold_describe_node(view, r))
                .unwrap_or_default(),
            functional,
            sub_property_of: fold_curies(view, t, &format!("{RDFS}subPropertyOf")),
            alignments: fold_alignments(view, t),
            ..Term::default()
        };
        fold_advisory(view, t, &mut term);
        term_label_def(view, t, &mut term);
        terms.push(term);
    }

    // Individuals: subjects typed by an in-namespace class, not themselves declared.
    let declared: BTreeSet<usize> = classes
        .iter()
        .copied()
        .chain(properties.keys().copied())
        .collect();
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    for &cls in &classes {
        let cls_iri = view.lex(cls).to_string();
        for t in view.subjects_by_type(&cls_iri, DEFAULT_SCOPE) {
            if !in_namespace(view, t) || declared.contains(&t) || seen.contains(&t) {
                continue;
            }
            seen.insert(t);
            let types: Vec<String> = {
                let mut s: BTreeSet<String> = BTreeSet::new();
                for o in view.objects(t, RDF_TYPE, DEFAULT_SCOPE) {
                    if classes.contains(&o) {
                        s.insert(curie(view.lex(o)));
                    }
                }
                s.into_iter().collect()
            };
            let mut term = Term {
                category: "individual",
                iri: view.lex(t).to_string(),
                curie: curie(view.lex(t)),
                types,
                alignments: fold_alignments(view, t),
                ..Term::default()
            };
            fold_advisory(view, t, &mut term);
            term_label_def(view, t, &mut term);
            terms.push(term);
        }
    }

    // Stamp each term with its owning slice, recovered from the documentation
    // graph's term→slice provenance (the docs generator dogfoods this into the
    // bundle). Built once, then applied by IRI — so every consumer of a folded
    // `Term` (the MCP card, JSONL, llms.txt) sees the same slice the docs do.
    let slice_map = fold_owner_slice_map(view);
    for term in &mut terms {
        if let Some(slice) = slice_map.get(&term.iri) {
            term.owner_slice = slice.clone();
        }
    }

    terms.sort_by(|a, b| (a.category, &a.curie).cmp(&(b.category, &b.curie)));
    terms
}

/// Build the neutral, shared [`gmeow_docs::card::Card`] from a folded [`Term`].
///
/// This is the ONE card builder for the folded/MCP side; together with
/// `gmeow_docs::render::doc_term_card` (the docs-site side) it feeds the SINGLE
/// `gmeow_docs::card::render_card_body` renderer, so the MCP card and the site
/// card never diverge again (§19 one-path).
///
/// Values are resolved to display strings (CURIEs / pre-described domain/range).
/// The category is human-cased to match the docs side (`Class`/`Property`/…).
///
/// Slice provenance: the owning slice is recovered from the documentation
/// graph's `gmeow:docOwnerSlice` (stamped onto every folded `Term` in
/// [`collect_terms`]) and rendered as its local name — matching the docs-site
/// card's `local_name(owner_slice)`. So the MCP card and the published docs
/// carry the SAME slice. When the term has no recovered slice (e.g. a doc graph
/// is absent), `Card::slice` is `None` and the header line is omitted — never a
/// blank value.
/// Whether `iri` names a `$defs` entry in `modeled_defs` (the JSON Schema
/// `$defs` key set — [`crate::bundle_blobs::Bundle::modeled_def_keys`]), keyed
/// through the SAME namespace table the SHACL→JSON-Schema compiler used
/// ([`gmeow_ns::gmeow_json_schema_namespaces`]) — the "this class has a
/// generated Pydantic model" existence signal `term_to_card`'s `python_model`
/// gate reads. Shared with `gmeow_docs::render::doc_term_card` and
/// `gmeow_docs::describe::build_card` in spirit (never in code — the crate
/// boundary is one-directional), so all three builders agree (issue: Pydantic
/// model surface, finding F3).
fn class_is_modeled(iri: &str, modeled_defs: &BTreeSet<String>) -> bool {
    modeled_defs.contains(&gmeow_ns::gmeow_json_schema_namespaces().def_key(iri))
}

pub(crate) fn term_to_card(t: &Term, modeled_defs: &BTreeSet<String>) -> gmeow_docs::card::Card {
    let category = match t.category {
        "class" => "Class",
        "property" => "Property",
        "individual" => "Individual",
        other => other,
    }
    .to_string();
    let label = if t.label.is_empty() || t.label == t.curie {
        None
    } else {
        Some(marked(&t.label, t.label_fallback))
    };
    let definition = if t.definition.is_empty() {
        None
    } else {
        Some(marked(&t.definition, t.definition_fallback))
    };
    // Parents: subClassOf for a class, subPropertyOf for a property.
    let parents = if t.category == "property" {
        t.sub_property_of.clone()
    } else {
        t.parents.clone()
    };
    // Domain / range are single pre-described display strings on the folded Term.
    let domain = if t.domain.is_empty() {
        Vec::new()
    } else {
        vec![t.domain.clone()]
    };
    let range = if t.range.is_empty() {
        Vec::new()
    } else {
        vec![t.range.clone()]
    };
    // The explicit term→model link (§19): a modeled class carries the importable
    // dotted path of its generated Pydantic model plus a compact construct/validate
    // snippet, from the SAME emitter routing the docs-site Python tab uses (never
    // duplicated). Gated on `class_is_modeled` (the class actually names a `$defs`
    // entry) rather than merely "is a Class with an owning slice" — an abstract
    // class with no SHACL NodeShape has NO generated model, and unconditionally
    // emitting the link fabricated an ImportError for a user who copied it (issue:
    // Pydantic model surface, finding F3).
    let (python_model, python_snippet) =
        if t.category == "class" && class_is_modeled(&t.iri, modeled_defs) {
            (
                Some(gmeow_docs::card::python_model_path(&t.owner_slice, &t.iri)),
                Some(gmeow_docs::card::python_model_snippet(
                    &t.owner_slice,
                    &t.iri,
                    &t.curie,
                )),
            )
        } else {
            (None, None)
        };
    // Individuals carry `types` rather than parents; surface them as Related-style
    // "a Type" is not a card field, so fold types into related_terms is wrong;
    // instead they ride the `(a …)` signature suffix on the heading. Keep the
    // card body parent-free for individuals.
    gmeow_docs::card::Card {
        category,
        iri: t.iri.clone(),
        label,
        // Recovered term→slice provenance (see the builder doc above): the local
        // name of the owning slice IRI, matching the docs-site card. `None` only
        // when the fold carries no slice for this term — never a blank line.
        slice: if t.owner_slice.is_empty() {
            None
        } else {
            Some(slice_local_name(&t.owner_slice).to_string())
        },
        box_roles: t.box_roles.clone(),
        definition,
        parents,
        domain,
        range,
        use_when: t.use_when.clone(),
        avoid_when: t.avoid_when.clone(),
        how_to_use: t.how_to_use.clone(),
        scope_notes: t.scope_notes.clone(),
        examples: t.examples.clone(),
        logic_stereotypes: t.logic_stereotypes.clone(),
        related_terms: t.related_terms.clone(),
        use_for_consumer: t.use_for_consumer.clone(),
        avoid_for_consumer: t.avoid_for_consumer.clone(),
        aligns: t.alignments.clone(),
        python_model,
        python_snippet,
        // Full-tier rich panels: the folded builder has no documentation-graph
        // access; the MCP `doc_card` full tier populates them from the graph.
        ..gmeow_docs::card::Card::default()
    }
}

pub(crate) fn fold_meta(view: &FoldView) -> Result<(String, String), gmeow_errors::Diag> {
    let onto = view.tid_of_iri(ONTOLOGY_IRI).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("ontology header {ONTOLOGY_IRI} not present in the snapshot"),
        })
    })?;
    let title = view
        .value(onto, "http://purl.org/dc/terms/title", DEFAULT_SCOPE)
        .map(|t| view.lex(t).to_string());
    let version = view
        .value(onto, &format!("{OWL}versionInfo"), DEFAULT_SCOPE)
        .map(|t| view.lex(t).to_string());
    match (title, version) {
        (Some(t), Some(v)) => Ok((t, v)),
        _ => Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: "ontology header lacks dcterms:title / owl:versionInfo".into(),
        })),
    }
}

// ── as_record (JSONL) ──────────────────────────────────────────────────────────

fn term_record(t: &Term) -> J {
    let mut rec: Vec<(String, J)> = vec![
        ("category".into(), J::Str(t.category.to_string())),
        ("curie".into(), J::Str(t.curie.clone())),
        ("iri".into(), J::Str(t.iri.clone())),
        ("label".into(), J::Str(t.label.clone())),
        ("definition".into(), J::Str(t.definition.clone())),
    ];
    if !t.labels.is_empty() {
        rec.push((
            "labels".into(),
            J::Obj(
                t.labels
                    .iter()
                    .map(|(k, v)| (k.clone(), J::Str(v.clone())))
                    .collect(),
            ),
        ));
    }
    if !t.definitions.is_empty() {
        rec.push((
            "definitions".into(),
            J::Obj(
                t.definitions
                    .iter()
                    .map(|(k, v)| (k.clone(), J::Str(v.clone())))
                    .collect(),
            ),
        ));
    }
    if t.label_fallback {
        rec.push(("labelFallback".into(), J::Bool(true)));
    }
    if t.definition_fallback {
        rec.push(("definitionFallback".into(), J::Bool(true)));
    }
    match t.category {
        "class" => rec.push(("subClassOf".into(), jarr_str(&t.parents))),
        "property" => {
            rec.push(("propertyKind".into(), J::Str(t.prop_kind.to_string())));
            rec.push(("domain".into(), J::Str(t.domain.clone())));
            rec.push(("range".into(), J::Str(t.range.clone())));
            rec.push(("functional".into(), J::Bool(t.functional)));
            rec.push(("subPropertyOf".into(), jarr_str(&t.sub_property_of)));
        }
        _ => rec.push(("types".into(), jarr_str(&t.types))),
    }
    let extra: &[(&str, &Vec<String>)] = &[
        ("alignments", &t.alignments),
        ("boxRoles", &t.box_roles),
        ("scopeNotes", &t.scope_notes),
        ("examples", &t.examples),
        ("useWhen", &t.use_when),
        ("avoidWhen", &t.avoid_when),
        ("howToUse", &t.how_to_use),
        ("useForConsumer", &t.use_for_consumer),
        ("avoidForConsumer", &t.avoid_for_consumer),
        ("logicStereotypes", &t.logic_stereotypes),
        ("relatedTerms", &t.related_terms),
    ];
    for (key, vals) in extra {
        if !vals.is_empty() {
            rec.push(((*key).into(), jarr_str(vals)));
        }
    }
    J::Obj(rec)
}

// ── CSVW (purrdf project_csvw_exact — the generic lossless RDF-1.2-in-CSV package) ──

fn err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-export-export".to_string(),
        message: message.into(),
    })
}

/// Extract the RDF DEFAULT-GRAPH quads (+ their reifiers/annotations) verbatim into
/// a standalone [`RdfDataset`] — the same default-graph scope [`DEFAULT_SCOPE`]
/// reads elsewhere in this leaf (the ontology's own TBox/ABox; ~98K quads, never the
/// ~2.3M-quad whole carrier's reasoning/diagnostics/corpora graphs — see `lpg.rs`'s
/// module doc for why an unscoped carrier projection balloons to gigabytes).
fn default_graph_dataset(
    dataset: &RdfDataset,
) -> Result<std::sync::Arc<RdfDataset>, gmeow_errors::Diag> {
    use purrdf::RdfDatasetBuilder;
    let mut b = RdfDatasetBuilder::new();
    for q in dataset.owned_quads() {
        if q.graph_name.is_none() {
            b.push_owned_quad(&q);
        }
    }
    for r in dataset.owned_reifiers() {
        if r.graph.is_none() {
            b.push_owned_reifier(&r);
        }
    }
    for a in dataset.owned_annotations() {
        if a.graph.is_none() {
            b.push_owned_annotation(&a);
        }
    }
    b.freeze()
        .map_err(|e| err(format!("default-graph dataset freeze: {e}")))
}

/// The gmeow-owned [`purrdf::CsvwConfig`]: the exact profile ignores `vocabulary`/
/// `mode` (those drive the OTHER RDF↔CSVW-standard conversion this leaf never uses),
/// but `CsvwConfig` is one shared mandatory-everything struct, so real W3C namespace
/// IRIs are still supplied (never a fabricated placeholder).
fn csvw_config() -> Result<purrdf::CsvwConfig, gmeow_errors::Diag> {
    let limits = purrdf::ProjectionLimits::new(
        16,            // max_artifacts: exactly 5 (metadata + terms/quads/reifiers/annotations)
        1_000_000_000, // max_artifact_bytes: the gmeow-cli `export` command re-derives its
        // source via `purrdf::gts::flattened_dataset_from_bytes` (every named graph
        // folded into the default graph — see that fn's own doc), so
        // `default_graph_dataset` sees the WHOLE ~2.1M-quad carrier there, not the
        // pipeline stage's own ~98K-quad true default graph. Measured worst case
        // (the full committed `gmeow.gts`, flattened): ~627MB `quads.csv`, ~133MB
        // `terms.csv`, ~761MB total package, ~34s. These bounds carry headroom above
        // that measured worst case.
        1_500_000_000, // max_total_bytes
        1_600_000_000, // max_archive_bytes
        16,            // max_term_depth
    )
    .map_err(|e| err(format!("CSVW ProjectionLimits: {e}")))?;
    let context = purrdf::CsvwContext::new("http://www.w3.org/ns/csvw", BTreeMap::new())
        .map_err(|e| err(format!("CsvwContext: {e}")))?;
    let vocabulary = purrdf::CsvwVocabulary::new(
        "http://www.w3.org/ns/csvw#",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
        RDFS,
        XSD,
    )
    .map_err(|e| err(format!("CsvwVocabulary: {e}")))?;
    purrdf::CsvwConfig::new(
        format!("{NAMESPACE}dist/csvw/"),
        context,
        format!("{NAMESPACE}dist/csvw/table-group"),
        vocabulary,
        purrdf::CsvwMode::Standard,
        limits,
        8_000_000, // max_records: covers the flattened-carrier worst case above
    )
    .map_err(|e| err(format!("CsvwConfig: {e}")))
}

/// Render the generic lossless CSVW package (`csvw-metadata.json` +
/// `terms.csv`/`quads.csv`/`reifiers.csv`/`annotations.csv`) from the carrier
/// `dataset`, scoped to the default graph (see [`default_graph_dataset`]). Replaces
/// the retired hand-rolled `write_class_csv`/`write_property_csv`/
/// `write_individual_csv`/`write_csvw` curated term tables — a deliberate drop of
/// the curated columns in favor of purrdf's exact, always-lossless RDF encoding.
fn render_csvw(dataset: &RdfDataset) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let scoped = default_graph_dataset(dataset)?;
    let config = csvw_config()?;
    let projection = purrdf::project_csvw_exact(scoped.as_ref(), &config)
        .map_err(|e| err(format!("project_csvw_exact: {e}")))?;
    Ok(projection
        .package
        .artifacts()
        .map(|(path, bytes)| (format!("{DIST_DIR}/csvw/{path}"), bytes.to_vec()))
        .collect())
}

// ── JSONL ───────────────────────────────────────────────────────────────────────

fn write_jsonl(terms: &[Term]) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();
    for t in terms {
        let mut s = String::new();
        term_record(t).compact(&mut s);
        lines.push(s);
    }
    (lines.join("\n") + "\n").into_bytes()
}

// ── Markdown term reference ─────────────────────────────────────────────────────

/// Append the language-fallback marker. The export leaf's fallback chain always
/// resolves through the English carrier, so the marker language is always `en`;
/// the policy itself lives in the [`language_tags::marked`] authority.
fn marked(text: &str, fallback: bool) -> String {
    authority_marked(text, fallback, "en")
}

fn append_md_advisory(lines: &mut Vec<String>, t: &Term) {
    if !t.box_roles.is_empty() {
        lines.push(format!(
            "\n*Box roles:* {}",
            t.box_roles
                .iter()
                .map(|r| format!("`{r}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for (label, values) in [
        ("Scope", &t.scope_notes),
        ("Example", &t.examples),
        ("Use when", &t.use_when),
        ("Avoid when", &t.avoid_when),
        ("How to use", &t.how_to_use),
    ] {
        if !values.is_empty() {
            lines.push(format!("\n*{label}:* {}", values.join(" ")));
        }
    }
    if !t.use_for_consumer.is_empty() {
        lines.push(format!(
            "\n*Use for consumers:* {}",
            t.use_for_consumer
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !t.avoid_for_consumer.is_empty() {
        lines.push(format!(
            "\n*Avoid for consumers:* {}",
            t.avoid_for_consumer
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

fn write_markdown(terms: &[Term], title: &str, version: &str) -> Vec<u8> {
    let classes: Vec<&Term> = terms.iter().filter(|t| t.category == "class").collect();
    let properties: Vec<&Term> = terms.iter().filter(|t| t.category == "property").collect();
    let individuals: Vec<&Term> = terms
        .iter()
        .filter(|t| t.category == "individual")
        .collect();
    let mut lines: Vec<String> = vec![
        format!("# {title} — term reference"),
        String::new(),
        format!(
            "Generated from the GMEOW {version} vocabulary ({} classes, {} properties, {} individuals). The RDF 1.2 grounding slices are canonical.",
            classes.len(),
            properties.len(),
            individuals.len()
        ),
        String::new(),
        "## Classes".into(),
        String::new(),
    ];
    for t in &classes {
        let head = marked(
            if t.label.is_empty() {
                &t.curie
            } else {
                &t.label
            },
            t.label_fallback,
        );
        lines.push(format!("### {head} (`{}`)", t.curie));
        if !t.definition.is_empty() {
            lines.push(format!(
                "\n{}",
                marked(&t.definition, t.definition_fallback)
            ));
        }
        append_md_advisory(&mut lines, t);
        if !t.parents.is_empty() {
            lines.push(format!(
                "\n*Subclass of:* {}",
                t.parents
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !t.alignments.is_empty() {
            lines.push(format!(
                "\n*Aligns:* {}",
                t.alignments
                    .iter()
                    .map(|a| format!("`{a}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines.push(String::new());
    }
    lines.push("## Properties".into());
    lines.push(String::new());
    for t in &properties {
        let head = marked(
            if t.label.is_empty() {
                &t.curie
            } else {
                &t.label
            },
            t.label_fallback,
        );
        lines.push(format!("### {head} (`{}`)", t.curie));
        if !t.definition.is_empty() {
            lines.push(format!(
                "\n{}",
                marked(&t.definition, t.definition_fallback)
            ));
        }
        append_md_advisory(&mut lines, t);
        let mut meta = format!("*{} property*", t.prop_kind);
        if !t.domain.is_empty() || !t.range.is_empty() {
            let d = if t.domain.is_empty() { "?" } else { &t.domain };
            let r = if t.range.is_empty() { "?" } else { &t.range };
            meta.push_str(&format!(" — `{d}` → `{r}`"));
        }
        if t.functional {
            meta.push_str(" (functional)");
        }
        lines.push(format!("\n{meta}"));
        if !t.alignments.is_empty() {
            lines.push(format!(
                "\n*Aligns:* {}",
                t.alignments
                    .iter()
                    .map(|a| format!("`{a}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines.push(String::new());
    }
    if !individuals.is_empty() {
        lines.push("## Individuals".into());
        lines.push(String::new());
        for t in &individuals {
            let head = marked(
                if t.label.is_empty() {
                    &t.curie
                } else {
                    &t.label
                },
                t.label_fallback,
            );
            lines.push(format!("### {head} (`{}`)", t.curie));
            if !t.definition.is_empty() {
                lines.push(format!(
                    "\n{}",
                    marked(&t.definition, t.definition_fallback)
                ));
            }
            append_md_advisory(&mut lines, t);
            if !t.types.is_empty() {
                lines.push(format!(
                    "\n*Type:* {}",
                    t.types
                        .iter()
                        .map(|x| format!("`{x}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            lines.push(String::new());
        }
    }
    (lines.join("\n") + "\n").into_bytes()
}

// ── llms.txt (standard llmstxt.org form) ───────────────────────────────
//
// All three llms.txt-family surfaces — this dist export, the live MCP consumer
// index, and the docs SITE index (`gmeow_docs::render::llms_txt`) — share ONE
// format via `gmeow_docs::llms`: an `# H1` + `> {GMEOW_SUMMARY}` blockquote +
// `## Section` markdown-link bullets. The ONLY thing that varies is whether a
// bullet carries a site URL (`Some` for the MCP index, recovered from the doc
// graph's `gmeow:docUrl`; `None` here — the dist tarball is not a site to link
// into). These had previously silently diverged (`⊑` vs `subClassOf`, `→` vs
// `->`, a hand-rolled header), each with its own copy of the summary sentence.

/// The llmstxt.org signature suffix for a term: ` (⊑ parents)` for a class,
/// ` [domain → range]` (each side `?` when absent) `(functional)?` for a
/// property, ` (a types)` for an individual. Empty when the term has none.
fn llms_signature(t: &Term) -> String {
    match t.category {
        "property" => {
            let has_sig = !t.domain.is_empty() || !t.range.is_empty();
            if !has_sig && !t.functional {
                return String::new();
            }
            let sig = if has_sig {
                let d = if t.domain.is_empty() { "?" } else { &t.domain };
                let r = if t.range.is_empty() { "?" } else { &t.range };
                format!(" [{d} → {r}]")
            } else {
                String::new()
            };
            let func = if t.functional { " (functional)" } else { "" };
            format!("{sig}{func}")
        }
        "class" => {
            if t.parents.is_empty() {
                String::new()
            } else {
                format!(" (⊑ {})", t.parents.join(", "))
            }
        }
        _ => {
            if t.types.is_empty() {
                String::new()
            } else {
                format!(" (a {})", t.types.join(", "))
            }
        }
    }
}

/// The bullet note for a term: its definition (falling back to the label), with
/// the `[fallback: en]` marker when resolved via the English fallback. NO
/// box-roles suffix (those live in the per-term card / `llms-full.txt`).
fn llms_note(t: &Term) -> String {
    let base = if t.definition.is_empty() {
        &t.label
    } else {
        &t.definition
    };
    marked(base, t.definition_fallback || t.label_fallback)
}

/// Build the `Classes` / `Properties` / `Individuals` sections from the folded
/// term model, shared by the dist export and the MCP index. `doc_urls` maps a
/// term IRI to its published site URL; `Some` makes the bullets markdown links
/// (the MCP index), `None` leaves them linkless (the dist dump).
fn llms_sections(
    terms: &[Term],
    doc_urls: Option<&std::collections::HashMap<String, String>>,
) -> Vec<gmeow_docs::llms::LlmsSection> {
    let bullet = |t: &Term| gmeow_docs::llms::LlmsBullet {
        text: t.curie.clone(),
        url: doc_urls.and_then(|m| m.get(&t.iri).cloned()),
        signature: llms_signature(t),
        note: gmeow_docs::llms::cap_note(&llms_note(t)),
    };
    let section = |heading: &str, category: &str| {
        let bullets: Vec<_> = terms
            .iter()
            .filter(|t| t.category == category)
            .map(&bullet)
            .collect();
        (heading.to_string(), bullets)
    };
    [
        section("Classes", "class"),
        section("Properties", "property"),
        section("Individuals", "individual"),
    ]
    .into_iter()
    .filter(|(_, bullets)| !bullets.is_empty())
    .map(|(heading, bullets)| gmeow_docs::llms::LlmsSection { heading, bullets })
    .collect()
}

/// The shared `Vocabulary {version}. Namespace …` prose line for the index forms.
fn llms_prose(version: &str, suffix: &str) -> Vec<String> {
    vec![format!(
        "Vocabulary {version}. Namespace: {NAMESPACE}. {suffix}"
    )]
}

fn write_llms_txt(terms: &[Term], title: &str, version: &str) -> Vec<u8> {
    let prose = llms_prose(
        version,
        "The RDF 1.2 grounding slices are canonical; this is a self-contained vocabulary index.",
    );
    let mut sections = llms_sections(terms, None);
    sections.push(gmeow_docs::llms::standing_reference_section());
    gmeow_docs::llms::render_index(title, &prose, &sections).into_bytes()
}

/// The `llms-full.txt` surface: the complete, link-free inlined index — the
/// standard header (with the canonical summary blockquote) then every term as a
/// `### {curie}{signature}` block with its full card body, rendered through the
/// shared `gmeow_docs::card` renderer. Emitted in a deterministic total order and
/// bounded by [`gmeow_docs::llms::LLMS_FULL_TOKEN_BUDGET`] (the elided remainder is
/// disclosed, never silently dropped). Shared by the flat `dist/llms-full.txt`
/// export and the MCP `llms_full` surface — the folded-`Term` twin of the
/// docs-site `gmeow_docs::render::llms_full_txt`.
pub(crate) fn consumer_llms_full(
    terms: &[Term],
    title: &str,
    version: &str,
    modeled_defs: &BTreeSet<String>,
) -> String {
    let prose = llms_prose(
        version,
        "Complete inlined form — every term, its definition, and its usage advice in full, \
         bounded by a fixed token budget.",
    );
    let mut out = gmeow_docs::llms::llms_header(title, &prose);
    out.push_str("## Terms\n\n");
    // Emit whole term blocks in a deterministic total order (CURIE then IRI) so the
    // token-budget elision boundary is byte-stable regardless of the input ordering.
    let mut ordered: Vec<&Term> = terms.iter().collect();
    ordered.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    let budget = gmeow_docs::llms::LLMS_FULL_TOKEN_BUDGET;
    let mut used = gmeow_docs::llms::estimate_tokens(&out);
    let mut emitted = 0usize;
    for t in &ordered {
        let block = format!(
            "### {}{}\n\n{}\n",
            t.curie,
            llms_signature(t),
            gmeow_docs::card::render_card_body(
                &term_to_card(t, modeled_defs),
                gmeow_docs::card::CardDetail::Standard,
            )
        );
        let cost = gmeow_docs::llms::estimate_tokens(&block);
        // Always emit at least one block; otherwise stop before the budget is
        // exceeded (a hard cap, never a mid-block truncation).
        if emitted > 0 && used + cost > budget {
            break;
        }
        out.push_str(&block);
        used += cost;
        emitted += 1;
    }
    let elided = ordered.len() - emitted;
    if elided > 0 {
        // Disclose the cap rather than silently truncating (no silent caps): the
        // elided terms remain reachable via the MCP lookup tools and the docs site.
        out.push_str(&format!(
            "> {elided} of {} terms elided to fit the {budget}-token llms-full budget; \
             resolve any omitted term via the MCP `lookup_term` / `doc_card` tools or the \
             full documentation site.\n",
            ordered.len()
        ));
    }
    // The standing reference pages + offline-snippet-corpus note, built from the
    // SAME shared list the docs-site `llms_full_txt` and both `llms.txt`-family
    // consumer surfaces render, so this surface cannot silently omit them again.
    out.push('\n');
    out.push_str(&gmeow_docs::llms::render_section(
        &gmeow_docs::llms::standing_reference_section(),
    ));
    out
}

// ── MCP consumer surfaces ────────────────────────────────────────────────────
//
// The native MCP server exposes five `export`-backed surfaces: `lookup_term`,
// `llms_txt`, `llms_full`, `doc_card`, and `okf_index`. These renderers emit the
// standard llmstxt.org format via the shared `gmeow_docs::llms` skeleton (`# H1`
// + `> {GMEOW_SUMMARY}` blockquote + `## Section` markdown-link bullets, `⊑`
// subclass marker, `→` property arrow) — the same format the dist export and
// docs SITE index produce, differing only in whether bullets carry a site URL.
// The per-term card (`doc_card`) and the inlined `llms_full` blocks render
// through the single `gmeow_docs::card` renderer, so the MCP card is the genuine
// twin of the docs-site `card.md`.
//
// Consumed unconditionally by `crate::mcp` (the native MCP server, no longer
// gated behind `python`) and by the byte-format goldens under `test`.

pub(crate) use consumer::{
    ConsumerResolution, consumer_llms_txt, doc_card_build, doc_url_map, lookup_envelope,
    okf_index_envelope, resolve_term_iri,
};

mod consumer {
    use super::*;

    /// The outcome of resolving a consumer query against the folded term set — the
    /// MCP-surface twin of [`gmeow_docs::describe::Resolution`]. A bare local name
    /// that exactly matches terms in more than one namespace HARD-FAILS with the
    /// sorted candidate CURIEs rather than silently picking one (`.goals` NO
    /// OPTIONALITY); the `gmeow describe` CLI enforces the identical contract.
    /// Generic over the resolved payload so the IRI path (`&str`) and the owned-IRI
    /// MCP wrapper (`String`) share one taxonomy.
    pub(crate) enum ConsumerResolution<T> {
        /// A unique term (or its unique IRI).
        Resolved(T),
        /// A bare local name matched terms in more than one namespace: the sorted,
        /// deduped candidate display CURIEs the caller must disambiguate between.
        Ambiguous { candidates: Vec<String> },
        /// No term matches the query.
        NotFound,
    }

    #[cfg(test)]
    impl<T> ConsumerResolution<T> {
        /// The resolved payload, or `None` for `Ambiguous`/`NotFound` — for the tests
        /// that only exercise the unique-resolution path.
        pub(crate) fn resolved(self) -> Option<T> {
            match self {
                ConsumerResolution::Resolved(v) => Some(v),
                _ => None,
            }
        }
    }

    /// Resolve a CURIE / local name / IRI / case-insensitive label — or a single
    /// unambiguous CURIE/label prefix — to a term. Mirrors the `describe` precedence:
    /// case-SENSITIVE exact matches win first; only when the same query exactly names
    /// terms in more than one namespace do we HARD-FAIL [`ConsumerResolution::Ambiguous`]
    /// rather than silently pick the first (`.goals` NO OPTIONALITY). When no exact
    /// match exists a unique case-insensitive prefix still resolves. Shared by
    /// `lookup_term` and the `doc_card` surface.
    fn resolve_term<'a>(terms: &'a [Term], query: &str) -> ConsumerResolution<&'a Term> {
        let needle = query.trim();
        if needle.is_empty() {
            return ConsumerResolution::NotFound;
        }
        let mut exact: Vec<&Term> = Vec::new();
        let mut exact_ci: Vec<&Term> = Vec::new();
        let mut prefix: Vec<&Term> = Vec::new();
        for term in terms {
            // Bare local-name matching spans every registered namespace (not only
            // `gmeow:`), so `lang:Denotation`'s bare `Denotation` resolves too.
            let local = registry_local(&term.iri);
            let candidates = [
                term.curie.as_str(),
                term.iri.as_str(),
                local,
                term.label.as_str(),
            ];
            if candidates.iter().any(|c| !c.is_empty() && *c == needle) {
                exact.push(term);
            }
            if candidates
                .iter()
                .any(|c| !c.is_empty() && c.eq_ignore_ascii_case(needle))
            {
                exact_ci.push(term);
            }
            let starts_with_ci = |s: &str| {
                s.get(..needle.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(needle))
            };
            if starts_with_ci(&term.curie) || starts_with_ci(&term.label) {
                prefix.push(term);
            }
        }
        // Case-sensitive exact wins first; fall to case-insensitive exact only when no
        // case-sensitive exact matched (mirrors `describe`).
        let matched = if !exact.is_empty() { exact } else { exact_ci };
        // Distinct by term IRI: a single term can match via curie AND local name.
        let mut distinct: Vec<&Term> = Vec::new();
        for term in matched {
            if !distinct.iter().any(|t| t.iri == term.iri) {
                distinct.push(term);
            }
        }
        match distinct.as_slice() {
            [only] => return ConsumerResolution::Resolved(only),
            [] => {}
            _ => {
                let mut candidates: Vec<String> =
                    distinct.iter().map(|t| t.curie.clone()).collect();
                candidates.sort();
                candidates.dedup();
                return ConsumerResolution::Ambiguous { candidates };
            }
        }
        // No exact match: a unique case-insensitive prefix resolves (the fuzzy-
        // completion UX); anything else is NotFound.
        if let [only] = prefix.as_slice() {
            ConsumerResolution::Resolved(only)
        } else {
            ConsumerResolution::NotFound
        }
    }

    /// Resolve a CURIE / local name / IRI / label (or unambiguous prefix) to its
    /// canonical term IRI, via the SAME [`resolve_term`] path `lookup_term` and
    /// `doc_card` use. Propagates ambiguity (never collapses it to NotFound) so the
    /// caller HARD-FAILS a cross-namespace collision with a typed diagnostic rather
    /// than fabricating a silent pick. Borrows the IRI from `terms` — zero-copy on
    /// this path; callers that must escape the borrow (e.g. past the cache lock the
    /// terms slice is borrowed from) own the ONE necessary allocation at their
    /// boundary instead.
    pub(crate) fn resolve_term_iri<'a>(
        terms: &'a [Term],
        query: &str,
    ) -> ConsumerResolution<&'a str> {
        match resolve_term(terms, query) {
            ConsumerResolution::Resolved(term) => ConsumerResolution::Resolved(term.iri.as_str()),
            ConsumerResolution::Ambiguous { candidates } => {
                ConsumerResolution::Ambiguous { candidates }
            }
            ConsumerResolution::NotFound => ConsumerResolution::NotFound,
        }
    }

    /// `lookup_term`: resolve a query to its `as_record()` JSON with
    /// `"ok": true` appended, or the
    /// `{"ok": false, "error": "Term not found: <query>"}` envelope.
    pub(crate) fn lookup_envelope(terms: &[Term], query: &str) -> String {
        let term = match resolve_term(terms, query) {
            ConsumerResolution::Resolved(term) => term,
            ConsumerResolution::Ambiguous { candidates } => {
                return lookup_ambiguous(query, &candidates);
            }
            ConsumerResolution::NotFound => return lookup_not_found(query),
        };
        // `result = term.as_record(); result["ok"] = True` — `ok` is appended LAST.
        let J::Obj(mut rec) = term_record(term) else {
            unreachable!("term_record always yields a JSON object")
        };
        rec.push(("ok".to_string(), J::Bool(true)));
        // The consumer tool returns `json.dumps(result)` — default ensure_ascii.
        let mut out = String::new();
        J::Obj(rec).compact_ascii(&mut out);
        out
    }

    fn lookup_not_found(query: &str) -> String {
        let mut out = String::new();
        J::Obj(vec![
            ("ok".to_string(), J::Bool(false)),
            (
                "error".to_string(),
                J::Str(format!("Term not found: {query}")),
            ),
        ])
        .compact_ascii(&mut out);
        out
    }

    /// The `lookup_term` envelope for a bare local name that collides across
    /// namespaces: a distinct `{"ok": false, "error": "ambiguous term '<q>': <c1>,
    /// <c2>, ..."}` (candidates already sorted) — a HARD FAIL, never a silent pick.
    fn lookup_ambiguous(query: &str, candidates: &[String]) -> String {
        let mut out = String::new();
        J::Obj(vec![
            ("ok".to_string(), J::Bool(false)),
            (
                "error".to_string(),
                J::Str(format!(
                    "ambiguous term '{query}': {}",
                    candidates.join(", ")
                )),
            ),
        ])
        .compact_ascii(&mut out);
        out
    }

    /// `llms_txt`: the standard llmstxt.org vocabulary index,
    /// rendered through the shared `gmeow_docs::llms` emitter so it matches the
    /// docs-site and dist forms byte-for-byte modulo URLs. `doc_urls` maps a term
    /// IRI to its published site URL (recovered from the doc graph via
    /// [`doc_url_map`]); present URLs make the bullets links into the same pages
    /// the docs site serves.
    pub(crate) fn consumer_llms_txt(
        terms: &[Term],
        title: &str,
        version: &str,
        doc_urls: &std::collections::HashMap<String, String>,
    ) -> String {
        let prose = llms_prose(
            version,
            "The RDF 1.2 grounding slices are canonical; this index links into the published documentation.",
        );
        let mut sections = llms_sections(terms, Some(doc_urls));
        // The standing reference pages + offline-snippet-corpus note, built from
        // the SAME shared list the docs-site `llms_txt`/`llms_full_txt` render —
        // the MCP consumer surface previously omitted these entirely.
        sections.push(gmeow_docs::llms::standing_reference_section());
        gmeow_docs::llms::render_index(title, &prose, &sections)
    }

    /// `doc_card`: resolve `query` to a term and build its `# {curie}{signature}`
    /// title and the COMPACT shared [`gmeow_docs::card::Card`] — the live MCP twin
    /// of the docs-site `terms/{slug}/card.md`, through the ONE shared
    /// `gmeow_docs::card` builder + renderer (§19 one-path). `None` when the query
    /// does not resolve to exactly one term.
    ///
    /// This builds the COMPACT card only (no full-tier rich panels) — it has just
    /// the folded `Term` set, not the documentation graph. The MCP `doc_card` tool
    /// populates the rich panels for [`gmeow_docs::card::CardDetail::Full`] from the
    /// documentation graph, then renders through the SAME `render_card`; the site
    /// path renders this compact card at `Standard`.
    ///
    /// Distinguishes [`ConsumerResolution::Ambiguous`] from
    /// [`ConsumerResolution::NotFound`] so the MCP `doc_card` tool can surface a
    /// cross-namespace collision as a typed ambiguity error, never a silent pick.
    pub(crate) fn doc_card_build(
        terms: &[Term],
        query: &str,
        modeled_defs: &BTreeSet<String>,
    ) -> ConsumerResolution<(String, gmeow_docs::card::Card)> {
        match resolve_term(terms, query) {
            ConsumerResolution::Resolved(t) => {
                let title = format!("{}{}", t.curie, llms_signature(t));
                ConsumerResolution::Resolved((title, super::term_to_card(t, modeled_defs)))
            }
            ConsumerResolution::Ambiguous { candidates } => {
                ConsumerResolution::Ambiguous { candidates }
            }
            ConsumerResolution::NotFound => ConsumerResolution::NotFound,
        }
    }

    /// Build a `term-IRI → site URL` map from the `gmeow:graph/documentation`
    /// named graph in the folded snapshot (`gmeow:documents` ⨝ `gmeow:docUrl` on
    /// each `gmeow:DocumentedTerm`). Lets the MCP index link into the published
    /// docs site using the SAME URLs the site itself emits. Empty when the doc
    /// graph is absent (then the index renders linkless).
    pub(crate) fn doc_url_map(view: &FoldView) -> std::collections::HashMap<String, String> {
        let documents = format!("{NAMESPACE}documents");
        let doc_url = format!("{NAMESPACE}docUrl");
        let mut map = std::collections::HashMap::new();
        for s in view.subjects_by_type(&format!("{NAMESPACE}DocumentedTerm"), DOCUMENTATION_GRAPH) {
            let (Some(iri_tid), Some(url_tid)) = (
                view.value(s, &documents, DOCUMENTATION_GRAPH),
                view.value(s, &doc_url, DOCUMENTATION_GRAPH),
            ) else {
                continue;
            };
            map.insert(view.lex(iri_tid).to_string(), view.lex(url_tid).to_string());
        }
        map
    }

    fn okf_category_dir(category: &str) -> &'static str {
        match category {
            "class" => "classes",
            "property" => "properties",
            _ => "individuals",
        }
    }

    fn okf_category_type(category: &str) -> &'static str {
        match category {
            "class" => "Class",
            "property" => "Property",
            _ => "Individual",
        }
    }

    /// The OKF document stem: the CURIE local part (`gmeow:Foo` → `Foo`).
    fn okf_slug(curie_str: &str) -> &str {
        curie_str
            .split_once(':')
            .map(|(_, rest)| rest)
            .unwrap_or(curie_str)
    }

    /// `okf_index`: the OKF manifest envelope
    /// `{ok, format, lossy, count, documents:[{path, type, title, resource}]}`.
    /// The per-document `path`s mirror the bundle layout the `okf` export leaf renders.
    pub(crate) fn okf_index_envelope(terms: &[Term]) -> String {
        let documents: Vec<J> = terms
            .iter()
            .map(|t| {
                let path = format!(
                    "gmeow-okf/{}/{}.md",
                    okf_category_dir(t.category),
                    okf_slug(&t.curie)
                );
                let title = if t.label.is_empty() {
                    t.curie.clone()
                } else {
                    t.label.clone()
                };
                J::Obj(vec![
                    ("path".to_string(), J::Str(path)),
                    (
                        "type".to_string(),
                        J::Str(okf_category_type(t.category).to_string()),
                    ),
                    ("title".to_string(), J::Str(title)),
                    ("resource".to_string(), J::Str(t.iri.clone())),
                ])
            })
            .collect();
        let count = documents.len();
        let mut out = String::new();
        J::Obj(vec![
            ("ok".to_string(), J::Bool(true)),
            ("format".to_string(), J::Str("okf".to_string())),
            ("lossy".to_string(), J::Bool(true)),
            ("count".to_string(), J::RawNum(count.to_string())),
            ("documents".to_string(), J::Arr(documents)),
        ])
        .compact(&mut out);
        out
    }
}

// ── dataset forms: N-Quads / TriG (native serializers over the carrier) ─────────

/// Rebuild the carrier dataset with internal `x-gmeow-*` language tags remapped to
/// public BCP-47 (the projection boundary). Only literal language tags change.
fn dataset_with_public_tags(
    dataset: &RdfDataset,
    tag_map: &BTreeMap<String, String>,
) -> Result<std::sync::Arc<RdfDataset>, gmeow_errors::Diag> {
    use purrdf::RdfDatasetBuilder;
    use purrdf::model::RdfTerm;
    let retag = |term: RdfTerm| -> RdfTerm {
        if let RdfTerm::Literal(mut lit) = term {
            if let Some(public) = lit.language.as_ref().and_then(|l| tag_map.get(l)) {
                lit.language = Some(public.clone());
            }
            RdfTerm::Literal(lit)
        } else {
            term
        }
    };
    let mut b = RdfDatasetBuilder::new();
    for mut q in dataset.owned_quads() {
        q.object = retag(q.object);
        b.push_owned_quad(&q);
    }
    for mut r in dataset.owned_reifiers() {
        r.statement.object = retag(r.statement.object);
        b.push_owned_reifier(&r);
    }
    for mut a in dataset.owned_annotations() {
        a.object = retag(a.object);
        b.push_owned_annotation(&a);
    }
    b.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("public-tag dataset freeze: {e}"),
        })
    })
}

fn write_nquads(
    dataset: &RdfDataset,
    tag_map: &BTreeMap<String, String>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let public = dataset_with_public_tags(dataset, tag_map)?;
    purrdf::serialize_dataset(
        &public,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("n-quads serialize: {e}"),
        })
    })
}

fn write_trig(
    dataset: &RdfDataset,
    tag_map: &BTreeMap<String, String>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let public = dataset_with_public_tags(dataset, tag_map)?;
    purrdf::serialize_dataset(&public, "application/trig", purrdf::SerializeGraph::Dataset).map_err(
        |e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("trig serialize: {e}"),
            })
        },
    )
}

// ── statements JSONL ─────────────────────────────────────────────────────────────

/// `python_value` with public lang-tag remap (mirror `_public_value`).
fn public_value_json(view: &FoldView, tid: usize) -> J {
    if view.is_literal(tid) {
        let dt = view.datatype(tid);
        let lex = view.lex(tid);
        if dt == format!("{XSD}integer")
            || dt == format!("{XSD}decimal")
            || dt == format!("{XSD}double")
            || dt == format!("{XSD}float")
        {
            J::RawNum(lex.to_string())
        } else if dt == format!("{XSD}boolean") {
            J::Bool(matches!(lex.to_ascii_lowercase().as_str(), "true" | "1"))
        } else if let Some(lang) = view.lang(tid) {
            let public = view
                .tag_map
                .get(lang)
                .cloned()
                .unwrap_or_else(|| lang.to_string());
            J::Obj(vec![
                ("value".into(), J::Str(lex.to_string())),
                ("lang".into(), J::Str(public)),
            ])
        } else {
            J::Str(lex.to_string())
        }
    } else if view.is_iri(tid) {
        J::Str(curie(view.lex(tid)))
    } else if view.is_bnode(tid) {
        J::Str(format!("_:{}", view.lex(tid)))
    } else {
        J::Str(view.nq_token(tid))
    }
}

fn write_statements_jsonl(view: &FoldView) -> Vec<u8> {
    // grouped: reifier → predicate-curie → [values]
    let mut grouped: BTreeMap<usize, BTreeMap<String, Vec<J>>> = BTreeMap::new();
    for (r, p, v) in view.annotations() {
        let key = curie(view.lex(p));
        grouped
            .entry(r)
            .or_default()
            .entry(key)
            .or_default()
            .push(public_value_json(view, v));
    }
    // reifiers sorted by nq_token of the reifier id.
    let mut reifiers: Vec<(usize, (usize, usize, usize))> = view.reifiers();
    reifiers.sort_by_key(|a| view.nq_token(a.0));
    let mut rows: Vec<String> = Vec::new();
    for (rid, (s, p, o)) in &reifiers {
        let mut annotations: Vec<(String, J)> = Vec::new();
        if let Some(per_pred) = grouped.get(rid) {
            for (key, values) in per_pred {
                let val = if values.len() == 1 {
                    clone_j(&values[0])
                } else {
                    // sorted(values, key=str)
                    let mut sortable: Vec<(String, &J)> =
                        values.iter().map(|v| (j_str_key(v), v)).collect();
                    sortable.sort_by(|a, b| a.0.cmp(&b.0));
                    J::Arr(sortable.into_iter().map(|(_, v)| clone_j(v)).collect())
                };
                annotations.push((key.clone(), val));
            }
        }
        let record = J::Obj(vec![
            ("id".into(), public_value_json(view, *rid)),
            ("subject".into(), public_value_json(view, *s)),
            ("predicate".into(), J::Str(curie(view.lex(*p)))),
            ("object".into(), public_value_json(view, *o)),
            ("annotations".into(), J::Obj(annotations)),
        ]);
        let mut s = String::new();
        record.compact(&mut s);
        rows.push(s);
    }
    (rows.join("\n") + "\n").into_bytes()
}

// ── SKOS extract (purrdf project_skos) ────────────────────────────────────────────

const SKOS_MATCHES: &[&str] = &["exactMatch", "closeMatch", "relatedMatch"];

// Every role purrdf requires but gmeow's data never populates still names the REAL
// SKOS predicate (never a fabricated placeholder) — purrdf's role vocabulary is
// mandatory-and-complete by design (see the `SkosSourceRoles`/`SkosTargetRoles` doc
// comments); an unpopulated role simply never matches a source quad.
const SKOS_ROLE_NARROWER: &str = "http://www.w3.org/2004/02/skos/core#narrower";
const SKOS_ROLE_RELATED: &str = "http://www.w3.org/2004/02/skos/core#related";
const SKOS_ROLE_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
const SKOS_ROLE_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const SKOS_ROLE_BROAD_MATCH: &str = "http://www.w3.org/2004/02/skos/core#broadMatch";
const SKOS_ROLE_NARROW_MATCH: &str = "http://www.w3.org/2004/02/skos/core#narrowMatch";
const SKOS_ROLE_RELATED_MATCH: &str = "http://www.w3.org/2004/02/skos/core#relatedMatch";
const SKOS_ROLE_IN_SCHEME: &str = "http://www.w3.org/2004/02/skos/core#inScheme";
const SKOS_ROLE_HAS_TOP_CONCEPT: &str = "http://www.w3.org/2004/02/skos/core#hasTopConcept";
const SKOS_ROLE_TOP_CONCEPT_OF: &str = "http://www.w3.org/2004/02/skos/core#topConceptOf";
const SKOS_ROLE_ALT_LABEL: &str = "http://www.w3.org/2004/02/skos/core#altLabel";
const SKOS_ROLE_HIDDEN_LABEL: &str = "http://www.w3.org/2004/02/skos/core#hiddenLabel";
const SKOS_ROLE_NOTATION: &str = "http://www.w3.org/2004/02/skos/core#notation";
const SKOS_ROLE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#note";
const SKOS_ROLE_CHANGE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#changeNote";
const SKOS_ROLE_EDITORIAL_NOTE: &str = "http://www.w3.org/2004/02/skos/core#editorialNote";
const SKOS_ROLE_EXAMPLE: &str = "http://www.w3.org/2004/02/skos/core#example";
const SKOS_ROLE_HISTORY_NOTE: &str = "http://www.w3.org/2004/02/skos/core#historyNote";
const SKOS_ROLE_SCOPE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#scopeNote";

/// The gmeow-owned [`purrdf::SkosConfig`]: SOURCE interprets gmeow's own
/// `owl:Class`/`rdfs:label`/`rdfs:subClassOf` taxonomy (never standard SKOS
/// predicates on the source side — that is gmeow's actual vocabulary); TARGET emits
/// the real `skos:Concept`/`skos:prefLabel`/`skos:broader` surface. `skos:definition`
/// and the three match predicates are identity source→target: gmeow's alignments
/// graph already carries real `skos:exactMatch`/`closeMatch`/`relatedMatch` rows.
fn skos_config() -> Result<purrdf::SkosConfig, gmeow_errors::Diag> {
    let source_classes =
        purrdf::SkosClassRoles::new(RDF_TYPE, format!("{OWL}Class"), format!("{OWL}Ontology"))
            .map_err(|e| err(format!("SkosClassRoles (source): {e}")))?;
    let target_classes = purrdf::SkosClassRoles::new(
        RDF_TYPE,
        format!("{SKOS}Concept"),
        format!("{SKOS}ConceptScheme"),
    )
    .map_err(|e| err(format!("SkosClassRoles (target): {e}")))?;

    let source_labels = purrdf::SkosLabelRoles::new(
        format!("{RDFS}label"),
        SKOS_ROLE_ALT_LABEL,
        SKOS_ROLE_HIDDEN_LABEL,
        SKOS_ROLE_NOTATION,
    )
    .map_err(|e| err(format!("SkosLabelRoles (source): {e}")))?;
    let target_labels = purrdf::SkosLabelRoles::new(
        format!("{SKOS}prefLabel"),
        SKOS_ROLE_ALT_LABEL,
        SKOS_ROLE_HIDDEN_LABEL,
        SKOS_ROLE_NOTATION,
    )
    .map_err(|e| err(format!("SkosLabelRoles (target): {e}")))?;

    let documentation = purrdf::SkosDocumentationRoles::new(
        SKOS_ROLE_NOTE,
        SKOS_ROLE_CHANGE_NOTE,
        format!("{SKOS}definition"),
        SKOS_ROLE_EDITORIAL_NOTE,
        SKOS_ROLE_EXAMPLE,
        SKOS_ROLE_HISTORY_NOTE,
        SKOS_ROLE_SCOPE_NOTE,
    )
    .map_err(|e| err(format!("SkosDocumentationRoles: {e}")))?;

    let source_relations = purrdf::SkosRelationRoles::new(
        format!("{RDFS}subClassOf"),
        SKOS_ROLE_NARROWER,
        SKOS_ROLE_RELATED,
        SKOS_ROLE_CLOSE_MATCH,
        SKOS_ROLE_EXACT_MATCH,
        SKOS_ROLE_BROAD_MATCH,
        SKOS_ROLE_NARROW_MATCH,
        SKOS_ROLE_RELATED_MATCH,
        SKOS_ROLE_IN_SCHEME,
        SKOS_ROLE_HAS_TOP_CONCEPT,
        SKOS_ROLE_TOP_CONCEPT_OF,
    )
    .map_err(|e| err(format!("SkosRelationRoles (source): {e}")))?;
    let target_relations = purrdf::SkosRelationRoles::new(
        format!("{SKOS}broader"),
        SKOS_ROLE_NARROWER,
        SKOS_ROLE_RELATED,
        SKOS_ROLE_CLOSE_MATCH,
        SKOS_ROLE_EXACT_MATCH,
        SKOS_ROLE_BROAD_MATCH,
        SKOS_ROLE_NARROW_MATCH,
        SKOS_ROLE_RELATED_MATCH,
        SKOS_ROLE_IN_SCHEME,
        SKOS_ROLE_HAS_TOP_CONCEPT,
        SKOS_ROLE_TOP_CONCEPT_OF,
    )
    .map_err(|e| err(format!("SkosRelationRoles (target): {e}")))?;

    let source = purrdf::SkosSourceRoles::new(
        source_classes,
        source_labels,
        documentation.clone(),
        source_relations,
    )
    .map_err(|e| err(format!("SkosSourceRoles: {e}")))?;
    let target = purrdf::SkosTargetRoles::new(
        target_classes,
        target_labels,
        documentation,
        target_relations,
    )
    .map_err(|e| err(format!("SkosTargetRoles: {e}")))?;

    let limits = purrdf::ProjectionLimits::new(8, 128_000_000, 256_000_000, 300_000_000, 16)
        .map_err(|e| err(format!("SKOS ProjectionLimits: {e}")))?;
    purrdf::SkosConfig::new(
        source,
        target,
        ONTOLOGY_IRI,
        purrdf::SkosGraphSelection::DefaultGraph,
        limits,
        500_000,
    )
    .map_err(|e| err(format!("SkosConfig: {e}")))
}

/// Build the exact scoped RDF-1.2 source [`RdfDataset`] [`skos_config`]'s role model
/// consumes — mirroring the retired hand-rolled `write_skos`'s reads exactly:
/// gmeow-namespace `owl:Class` subjects from the default graph, their
/// `rdfs:label`/`skos:definition` texts (one per public language, matching
/// `write_skos`'s per-language dedup), `rdfs:subClassOf` edges restricted to
/// in-scope classes, and the `exactMatch`/`closeMatch`/`relatedMatch` rows from
/// [`ALIGNMENTS_GRAPH`]. purrdf's SKOS projector has no "broader-less ⇒ top concept"
/// inference (unlike the retired writer), so a broader-less class's top-concept
/// membership is asserted explicitly as a synthetic `skos:hasTopConcept` source row.
fn skos_source_dataset(view: &FoldView) -> Result<std::sync::Arc<RdfDataset>, gmeow_errors::Diag> {
    use purrdf::{RdfDatasetBuilder, RdfLiteral};

    let mut classes: Vec<usize> = view
        .subjects_by_type(&format!("{OWL}Class"), DEFAULT_SCOPE)
        .into_iter()
        .filter(|&t| view.is_iri(t) && view.lex(t).starts_with(NAMESPACE))
        .collect();
    classes.sort_by_key(|&a| view.lex(a));
    let class_iris: BTreeSet<&str> = classes.iter().map(|&t| view.lex(t)).collect();

    let mut builder = RdfDatasetBuilder::new();
    let rdf_type = builder.intern_iri(RDF_TYPE);
    let owl_class = builder.intern_iri(&format!("{OWL}Class"));
    let rdfs_label = builder.intern_iri(&format!("{RDFS}label"));
    let skos_definition = builder.intern_iri(&format!("{SKOS}definition"));
    let rdfs_sub_class_of = builder.intern_iri(&format!("{RDFS}subClassOf"));
    let has_top_concept = builder.intern_iri(SKOS_ROLE_HAS_TOP_CONCEPT);
    let scheme = builder.intern_iri(ONTOLOGY_IRI);

    for &t in &classes {
        let subject = builder.intern_iri(view.lex(t));
        builder.push_quad(subject, rdf_type, owl_class, None);

        let mut seen_labels: BTreeSet<String> = BTreeSet::new();
        for (text, lang, _fallback) in view.public_texts(t, &format!("{RDFS}label")) {
            if let Some(l) = lang
                && seen_labels.insert(l.clone())
            {
                let obj = builder.intern_literal(RdfLiteral {
                    lexical_form: text,
                    datatype: None,
                    language: Some(l),
                    direction: None,
                });
                builder.push_quad(subject, rdfs_label, obj, None);
            }
        }
        let mut seen_defs: BTreeSet<String> = BTreeSet::new();
        for (text, lang, _fallback) in view.public_texts(t, &format!("{SKOS}definition")) {
            if let Some(l) = lang
                && seen_defs.insert(l.clone())
            {
                let obj = builder.intern_literal(RdfLiteral {
                    lexical_form: text,
                    datatype: None,
                    language: Some(l),
                    direction: None,
                });
                builder.push_quad(subject, skos_definition, obj, None);
            }
        }

        let mut broader: BTreeSet<&str> = BTreeSet::new();
        for o in view.objects(t, &format!("{RDFS}subClassOf"), DEFAULT_SCOPE) {
            if view.is_iri(o) && class_iris.contains(view.lex(o)) {
                broader.insert(view.lex(o));
            }
        }
        if broader.is_empty() {
            let obj = builder.intern_iri(view.lex(t));
            builder.push_quad(scheme, has_top_concept, obj, None);
        } else {
            for b in broader {
                let obj = builder.intern_iri(b);
                builder.push_quad(subject, rdfs_sub_class_of, obj, None);
            }
        }

        for (p, o) in view.predicate_objects(t, ALIGNMENTS_GRAPH) {
            let p_local = view.lex(p).rsplit('#').next().unwrap_or("");
            if SKOS_MATCHES.contains(&p_local) && view.is_iri(o) {
                let pred = builder.intern_iri(view.lex(p));
                let obj = builder.intern_iri(view.lex(o));
                builder.push_quad(subject, pred, obj, None);
            }
        }
    }

    builder
        .freeze()
        .map_err(|e| err(format!("SKOS source dataset freeze: {e}")))
}

/// Group a purrdf [`purrdf::LossLedger`] by `(code, note)` and trace it — no runtime
/// RDF→(SKOS|OBO Graphs) lowering loss is ever silently dropped. Mirrors `lpg.rs`'s
/// `report_lpg_losses`; `tracing`'s `target:` field must be a literal, so `surface`
/// rides as an ordinary field instead of a per-call `target`.
fn report_projection_losses(surface: &str, ledger: &purrdf::LossLedger) {
    let mut grouped: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for loss in ledger.entries() {
        let subject = loss
            .location
            .as_deref()
            .and_then(|location| location.subject.as_deref())
            .unwrap_or("<unlocated>");
        grouped
            .entry((loss.code.as_ref(), loss.note.as_ref()))
            .or_default()
            .push(subject);
    }
    for ((construct, reason), mut subjects) in grouped {
        subjects.sort_unstable();
        subjects.dedup();
        let examples = subjects
            .iter()
            .take(5)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if subjects.len() > 5 {
            format!(" (+{} more)", subjects.len() - 5)
        } else {
            String::new()
        };
        tracing::info!(
            target: "export_projection_loss",
            surface = surface,
            construct = construct,
            subjects = subjects.len(),
            reason = reason,
            examples = %format!("{examples}{suffix}"),
            "lossy drop projecting the scoped default-graph RDF",
        );
    }
}

fn render_skos(view: &FoldView) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let source = skos_source_dataset(view)?;
    let config = skos_config()?;
    let projection = purrdf::project_skos(source.as_ref(), &config)
        .map_err(|e| err(format!("project_skos: {e}")))?;
    report_projection_losses("skos", &projection.loss_ledger);
    Ok(projection.turtle)
}

// ── OBO Graphs JSON (purrdf project_obo_graphs) ───────────────────────────────────

/// The gmeow-owned [`purrdf::OboGraphsConfig`]: standard RDF/RDFS/OWL vocabulary
/// (real IRIs throughout — OBO Graphs 0.3.2 has no gmeow-specific roles). The
/// metadata roles gmeow's filtered source never populates (synonyms/xref/subset)
/// still name the real oboInOwl vocabulary, matching [`skos_config`]'s
/// never-fabricate-a-placeholder discipline.
fn obo_graphs_config() -> Result<purrdf::OboGraphsConfig, gmeow_errors::Diag> {
    let rdf = purrdf::OboRdfRoles::new(
        RDF_TYPE,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies",
        RDF_FIRST,
        RDF_REST,
        RDF_NIL,
        format!("{XSD}string"),
        format!("{XSD}boolean"),
    )
    .map_err(|e| err(format!("OboRdfRoles: {e}")))?;
    let owl = purrdf::OboOwlRoles::new(
        format!("{RDFS}label"),
        format!("{RDFS}comment"),
        format!("{RDFS}subClassOf"),
        format!("{RDFS}subPropertyOf"),
        format!("{RDFS}domain"),
        format!("{RDFS}range"),
        format!("{OWL}Ontology"),
        format!("{OWL}Class"),
        format!("{OWL}NamedIndividual"),
        format!("{OWL}ObjectProperty"),
        format!("{OWL}AnnotationProperty"),
        format!("{OWL}DatatypeProperty"),
        format!("{OWL}equivalentClass"),
        format!("{OWL}intersectionOf"),
        format!("{OWL}Restriction"),
        format!("{OWL}onProperty"),
        format!("{OWL}someValuesFrom"),
        format!("{OWL}allValuesFrom"),
        format!("{OWL}propertyChainAxiom"),
        format!("{OWL}deprecated"),
    )
    .map_err(|e| err(format!("OboOwlRoles: {e}")))?;
    let metadata = purrdf::OboMetadataRoles::new(
        format!("{SKOS}definition"),
        "http://www.geneontology.org/formats/oboInOwl#hasExactSynonym",
        "http://www.geneontology.org/formats/oboInOwl#hasBroadSynonym",
        "http://www.geneontology.org/formats/oboInOwl#hasNarrowSynonym",
        "http://www.geneontology.org/formats/oboInOwl#hasRelatedSynonym",
        "http://www.geneontology.org/formats/oboInOwl#hasSynonymType",
        "http://www.geneontology.org/formats/oboInOwl#hasDbXref",
        "http://www.geneontology.org/formats/oboInOwl#inSubset",
        format!("{OWL}versionInfo"),
    )
    .map_err(|e| err(format!("OboMetadataRoles: {e}")))?;
    let vocabulary = purrdf::OboGraphsVocabulary::new(rdf, owl, metadata)
        .map_err(|e| err(format!("OboGraphsVocabulary: {e}")))?;
    let limits = purrdf::ProjectionLimits::new(8, 128_000_000, 256_000_000, 300_000_000, 16)
        .map_err(|e| err(format!("OBO Graphs ProjectionLimits: {e}")))?;
    purrdf::OboGraphsConfig::new(ONTOLOGY_IRI, vocabulary, limits, 500_000)
        .map_err(|e| err(format!("OboGraphsConfig: {e}")))
}

/// Build the exact scoped RDF-1.2 source [`RdfDataset`] [`obo_graphs_config`]'s role
/// model consumes — mirroring the retired hand-rolled `write_obographs`'s reads
/// exactly: gmeow-namespace `owl:Class` subjects from the default graph, their
/// `rdfs:label`/`skos:definition` texts, and IRI-only `rdfs:subClassOf` edges (never
/// a blank-node restriction — purrdf's OBO Graphs mapper parses `owl:Restriction`
/// blank-node shapes strictly and hard-fails on an unrecognized OWL expression
/// shape, so this leaf never feeds it one). The ontology header's `owl:versionInfo`
/// is carried onto the graph itself.
fn obo_graphs_source_dataset(
    view: &FoldView,
    version: &str,
) -> Result<std::sync::Arc<RdfDataset>, gmeow_errors::Diag> {
    use purrdf::{RdfDatasetBuilder, RdfLiteral};

    let label_iri = format!("{RDFS}label");
    let definition_iri = format!("{SKOS}definition");
    let mut classes: Vec<usize> = view
        .subjects_by_type(&format!("{OWL}Class"), DEFAULT_SCOPE)
        .into_iter()
        .filter(|&t| view.is_iri(t) && view.lex(t).starts_with(NAMESPACE))
        .collect();
    classes.sort_by(|&a, &b| view.lex(a).cmp(view.lex(b)));

    let mut builder = RdfDatasetBuilder::new();
    let rdf_type = builder.intern_iri(RDF_TYPE);
    let owl_class = builder.intern_iri(&format!("{OWL}Class"));
    let rdfs_label = builder.intern_iri(&label_iri);
    let skos_definition = builder.intern_iri(&definition_iri);
    let rdfs_sub_class_of = builder.intern_iri(&format!("{RDFS}subClassOf"));
    let owl_version_info = builder.intern_iri(&format!("{OWL}versionInfo"));
    let graph_id = builder.intern_iri(ONTOLOGY_IRI);

    let version_obj = builder.intern_literal(RdfLiteral::simple(version));
    builder.push_quad(graph_id, owl_version_info, version_obj, None);

    for &t in &classes {
        let subject = builder.intern_iri(view.lex(t));
        builder.push_quad(subject, rdf_type, owl_class, None);

        let (label, _fb) = view.public_text_with_fallback(t, &label_iri);
        if !label.is_empty() {
            let obj = builder.intern_literal(RdfLiteral::simple(label));
            builder.push_quad(subject, rdfs_label, obj, None);
        }
        let (definition, _fb) = view.public_text_with_fallback(t, &definition_iri);
        if !definition.is_empty() {
            let obj = builder.intern_literal(RdfLiteral::simple(definition));
            builder.push_quad(subject, skos_definition, obj, None);
        }
        for o in view.objects(t, &format!("{RDFS}subClassOf"), DEFAULT_SCOPE) {
            if view.is_iri(o) {
                let obj = builder.intern_iri(view.lex(o));
                builder.push_quad(subject, rdfs_sub_class_of, obj, None);
            }
        }
    }

    builder
        .freeze()
        .map_err(|e| err(format!("OBO Graphs source dataset freeze: {e}")))
}

fn render_obographs(view: &FoldView, version: &str) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let source = obo_graphs_source_dataset(view, version)?;
    let config = obo_graphs_config()?;
    let projection = purrdf::project_obo_graphs(source.as_ref(), &config)
        .map_err(|e| err(format!("project_obo_graphs: {e}")))?;
    report_projection_losses("obographs", &projection.loss_ledger);
    projection
        .document
        .to_canonical_json(&config)
        .map_err(|e| err(format!("OBO Graphs canonical JSON: {e}")))
}

// ── ShEx ─────────────────────────────────────────────────────────────────────────

fn shex_domains(view: &FoldView, prop: usize, class_iris: &BTreeSet<String>) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for d in view.objects(prop, &format!("{RDFS}domain"), DEFAULT_SCOPE) {
        let candidates: Vec<usize> = if view.is_iri(d) {
            vec![d]
        } else if view.is_bnode(d) {
            match view.value(d, &format!("{OWL}unionOf"), DEFAULT_SCOPE) {
                Some(head) => view.rdf_list(head, DEFAULT_SCOPE),
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        for c in candidates {
            if view.is_iri(c) && class_iris.contains(view.lex(c)) {
                out.insert(view.lex(c).to_string());
            }
        }
    }
    out.into_iter().collect()
}

fn write_shex(view: &FoldView) -> Vec<u8> {
    let class_iris: BTreeSet<String> = view
        .subjects_by_type(&format!("{OWL}Class"), DEFAULT_SCOPE)
        .into_iter()
        .filter(|&t| view.is_iri(t) && view.lex(t).starts_with(NAMESPACE))
        .map(|t| view.lex(t).to_string())
        .collect();
    let functional_tid = view.tid_of_iri(&format!("{OWL}FunctionalProperty"));

    let mut per_class: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (ptype, kind) in [
        ("ObjectProperty", "object"),
        ("DatatypeProperty", "datatype"),
    ] {
        for prop in view.subjects_by_type(&format!("{OWL}{ptype}"), DEFAULT_SCOPE) {
            if !view.lex(prop).starts_with(NAMESPACE) {
                continue;
            }
            let range_tid = view.value(prop, &format!("{RDFS}range"), DEFAULT_SCOPE);
            let value_expr = match range_tid {
                Some(rt) if view.is_iri(rt) => {
                    let range_iri = view.lex(rt).to_string();
                    if class_iris.contains(&range_iri) {
                        format!("@{}", curie(&range_iri))
                    } else if kind == "datatype" {
                        curie(&range_iri)
                    } else {
                        "IRI".to_string()
                    }
                }
                _ => {
                    if kind == "object" {
                        "IRI".to_string()
                    } else {
                        "LITERAL".to_string()
                    }
                }
            };
            let card = if matches!(functional_tid, Some(ft) if view.has(prop, RDF_TYPE, ft, DEFAULT_SCOPE))
            {
                "?"
            } else {
                "*"
            };
            let constraint = format!("{} {value_expr} {card}", curie(view.lex(prop)));
            for domain_iri in shex_domains(view, prop, &class_iris) {
                per_class
                    .entry(curie(&domain_iri))
                    .or_default()
                    .push(constraint.clone());
            }
        }
    }

    let mut lines: Vec<String> = vec![
        "# ShEx shapes for the GMEOW vocabulary — a LOSSY projection:".into(),
        "# one shape per class that is the (named or union-expanded) domain of".into(),
        "# an object/datatype property; functional → '?', else '*'. OWL".into(),
        "# restrictions, pure blank-node domains, and annotation properties".into(),
        "# are not translated. The RDF 1.2 grounding slices are canonical.".into(),
        format!("PREFIX gmeow: <{NAMESPACE}>"),
        "PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>".into(),
        // A datatype property whose rdfs:range is `rdfs:Literal` projects to a
        // bare `rdfs:Literal` node constraint; declare the prefix so the emitted
        // ShExC is well-formed (purrdf's parser rejects an undeclared CURIE).
        format!("PREFIX rdfs: <{RDFS}>"),
        "PREFIX gufo: <http://purl.org/nemo/gufo#>".into(),
        String::new(),
    ];
    for (cls, constraints) in &per_class {
        lines.push(format!("{cls} {{"));
        let mut unique: Vec<String> = constraints
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        unique.sort();
        for constraint in &unique {
            lines.push(format!("    {constraint} ;"));
        }
        let last = lines.len() - 1;
        lines[last] = lines[last].trim_end_matches(" ;").to_string();
        lines.push("}".into());
        lines.push(String::new());
    }
    let joined = lines.join("\n");
    (joined.trim_end_matches('\n').to_string() + "\n").into_bytes()
}

// ── J helpers for statements ─────────────────────────────────────────────────

fn clone_j(j: &J) -> J {
    match j {
        J::Bool(b) => J::Bool(*b),
        J::Str(s) => J::Str(s.clone()),
        J::RawNum(n) => J::RawNum(n.clone()),
        J::Arr(items) => J::Arr(items.iter().map(clone_j).collect()),
        J::Obj(entries) => J::Obj(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), clone_j(v)))
                .collect(),
        ),
    }
}

/// A `str(value)`-ish sort key for statement annotation list ordering.
fn j_str_key(j: &J) -> String {
    match j {
        J::Str(s) => s.clone(),
        J::RawNum(n) => n.clone(),
        J::Bool(b) => {
            if *b {
                "True".into()
            } else {
                "False".into()
            }
        }
        J::Obj(entries) => {
            // {'value': ..., 'lang': ...} → python dict repr is order-dependent; use a stable key.
            entries
                .iter()
                .map(|(k, v)| format!("{k}={}", j_str_key(v)))
                .collect::<Vec<_>>()
                .join(",")
        }
        J::Arr(items) => items.iter().map(j_str_key).collect::<Vec<_>>().join(","),
    }
}

// ── render all + Stage impl ──────────────────────────────────────────────────────

/// Render every flat-export artifact from the in-memory carrier dataset, in the
/// English-only default view. This is the pipeline stage's canonical producer
/// (committed `dist/` outputs are en-only); `--lang`-flexible callers route
/// through [`render_all_with_languages`].
pub(crate) fn render_all(
    dataset: &RdfDataset,
    modeled_defs: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    render_all_with_languages(dataset, &["en".to_string()], modeled_defs)
}

/// Render every flat-export artifact honoring a requested public-BCP-47 language
/// list (precedence order). The first entry drives the primary `label`/
/// `definition` selection (via [`FoldView::with_requested`]); every entry adds a
/// `label_<lang>` / `definition_<lang>` CSV column pair. An empty list falls back
/// to `["en"]`. Mirrors the Python `export` command's `selector` threading.
/// `modeled_defs` is the JSON Schema `$defs` key set
/// ([`crate::bundle_blobs::Bundle::modeled_def_keys`]) `llms-full.txt`'s inlined
/// cards gate their `python_model` link on (see [`class_is_modeled`]).
pub(crate) fn render_all_with_languages(
    dataset: &RdfDataset,
    languages: &[String],
    modeled_defs: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let requested: Vec<String> = if languages.is_empty() {
        vec!["en".to_string()]
    } else {
        languages.to_vec()
    };
    let view = FoldView::with_requested(dataset, requested.clone());
    let (title, version) = fold_meta(&view)?;
    let terms = collect_terms(&view);

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    out.extend(render_csvw(dataset)?);
    out.insert(format!("{DIST_DIR}/gmeow-terms.jsonl"), write_jsonl(&terms));
    out.insert(
        format!("{DIST_DIR}/gmeow-terms.md"),
        write_markdown(&terms, &title, &version),
    );
    out.insert(
        format!("{DIST_DIR}/llms.txt"),
        write_llms_txt(&terms, &title, &version),
    );
    out.insert(
        format!("{DIST_DIR}/llms-full.txt"),
        consumer_llms_full(&terms, &title, &version, modeled_defs).into_bytes(),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow.nq"),
        write_nquads(dataset, view.tag_map())?,
    );
    out.insert(
        format!("{DIST_DIR}/gmeow.trig"),
        write_trig(dataset, view.tag_map())?,
    );
    out.insert(
        format!("{DIST_DIR}/gmeow-statements.jsonl"),
        write_statements_jsonl(&view),
    );
    out.insert(format!("{DIST_DIR}/gmeow-skos.ttl"), render_skos(&view)?);
    out.insert(
        format!("{DIST_DIR}/gmeow-obographs.json"),
        render_obographs(&view, &version)?,
    );
    out.insert(format!("{DIST_DIR}/gmeow.shex"), write_shex(&view));
    Ok(out)
}

/// Collect `(title, version, terms)` from a folded gts graph — the shared term
/// surface consumed by both the flat-export leaf and the OKF leaf.
pub(crate) fn collect_term_surface(
    dataset: &RdfDataset,
) -> Result<(String, String, Vec<Term>), gmeow_errors::Diag> {
    let view = FoldView::new(dataset);
    let (title, version) = fold_meta(&view)?;
    let terms = collect_terms(&view);
    Ok((title, version, terms))
}

/// Read the committed fold from `generated/dist/gmeow.gts` under `root` as a native
/// `RdfDataset`. Used by the leaf unit tests (logic-vs-canonical against the committed
/// file); the runtime path reads THIS run's snapshot carrier via [`read_fold_upstream`].
#[cfg(test)]
pub(crate) fn read_fold(
    root: &std::path::Path,
) -> Result<std::sync::Arc<RdfDataset>, gmeow_errors::Diag> {
    let gts = std::fs::read(root.join("generated/dist/gmeow.gts"))?;
    let bundle = purrdf::import_gts_events(&gts).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("read gmeow.gts: {e}"),
        })
    })?;
    Ok(bundle.dataset)
}

/// Borrow THIS run's carrier dataset. The runtime path every fold-reading
/// export leaf (export / okf) uses: the `stage-snapshot` product carries the
/// terminal carrier `RdfDataset` directly, so the leaves read ONE shared dataset off
/// the bundle instead of re-parsing the `gmeow.gts` bytes (GTS is exit-only).
pub(crate) fn read_fold_upstream(
    upstream: &std::collections::BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<RdfDataset>, gmeow_errors::Diag> {
    crate::stages::carrier::snapshot_dataset(upstream)
}

/// THIS run's JSON Schema `$defs` key set, read directly off the in-memory
/// `stage-export-json-schema` product (never a stale disk read of the previously
/// committed `generated/schemas/gmeow.schema.json`) — the model-existence signal
/// [`class_is_modeled`] gates `python_model` on. Hard-fails if the declared
/// upstream product or its `gmeow.schema.json` artifact is missing
/// (no-optionality): [`ExportStage`] declares this dependency explicitly, so its
/// absence is a genuine wiring defect, never an honest absence.
pub(crate) fn modeled_defs_from_upstream(
    upstream: &std::collections::BTreeMap<String, StageProduct>,
) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let bytes = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::JSON_SCHEMA_PATH))
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-export-export".to_string(),
                message: "missing stage-export-json-schema gmeow.schema.json artifact for the \
                          model-existence gate"
                    .to_string(),
            })
        })?;
    let parsed: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-export-export".to_string(),
            message: format!("parse gmeow.schema.json for the model-existence gate: {e}"),
        })
    })?;
    Ok(parsed
        .get("$defs")
        .and_then(|v| v.as_object())
        .map(|d| d.keys().cloned().collect())
        .unwrap_or_default())
}

/// The `stage-export-export` export-leaf stage.
pub struct ExportStage {
    consumes: Vec<String>,
}

impl ExportStage {
    /// Construct the stage; it consumes THIS run's snapshot fold plus the
    /// `stage-export-json-schema` product, whose freshly-emitted `$defs` drive the
    /// `llms-full.txt` cards' `python_model` gate (see [`class_is_modeled`]) —
    /// without this edge the stage would only ever see the PREVIOUS run's
    /// committed schema (or none on a first run).
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-export-json-schema".to_string(),
                "stage-snapshot".to_string(),
            ],
        }
    }
}

impl Default for ExportStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ExportStage {
    fn id(&self) -> &str {
        "stage-export-export"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "export.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let graph = read_fold_upstream(input.upstream)?;
        let modeled_defs = modeled_defs_from_upstream(input.upstream)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_all(graph.as_ref(), &modeled_defs)?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// The REAL committed `generated/schemas/gmeow.schema.json` `$defs` key set —
    /// the same model-existence signal production reads, for tests exercising
    /// `term_to_card`/`consumer_llms_full`/`doc_card_build` against the real
    /// `english_terms()` corpus (so a modeled term like `gmeow:EntityExistence`
    /// still carries its `python_model` link in these tests, not a synthetic one).
    fn repo_modeled_defs() -> BTreeSet<String> {
        let path = repo_root().join("generated/schemas/gmeow.schema.json");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let parsed: serde_json::Value =
            serde_json::from_slice(&bytes).expect("gmeow.schema.json parses as JSON");
        parsed
            .get("$defs")
            .and_then(|v| v.as_object())
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default()
    }

    #[test]
    fn json_str_ascii_matches_cpython_short_escapes() {
        // `json.dumps(s)` (ensure_ascii=True) uses the short escapes `\b`/`\f`/
        // `\n`/`\r`/`\t` for these control chars, NOT `\uXXXX`. Pin byte-parity.
        assert_eq!(json_str_ascii("\u{08}"), r#""\b""#);
        assert_eq!(json_str_ascii("\u{0c}"), r#""\f""#);
        assert_eq!(json_str_ascii("\n\r\t"), r#""\n\r\t""#);
        // Other C0 controls still fall through to lowercase `\uXXXX`.
        assert_eq!(json_str_ascii("\u{00}\u{1f}"), "\"\\u0000\\u001f\"");
    }

    #[test]
    fn export_produces_structurally_valid_artifacts() {
        let root = repo_root();
        let graph = read_fold(&root).expect("read fold");
        let arts = render_all(&graph, &repo_modeled_defs()).expect("render");

        // All expected logical paths present and non-empty.
        let expected = [
            "csvw/csvw-metadata.json",
            "csvw/terms.csv",
            "csvw/quads.csv",
            "csvw/reifiers.csv",
            "csvw/annotations.csv",
            "gmeow-terms.jsonl",
            "gmeow-terms.md",
            "llms.txt",
            "llms-full.txt",
            "gmeow.nq",
            "gmeow.trig",
            "gmeow-statements.jsonl",
            "gmeow-skos.ttl",
            "gmeow-obographs.json",
            "gmeow.shex",
        ];
        for name in expected {
            let path = format!("{DIST_DIR}/{name}");
            let bytes = arts.get(&path).unwrap_or_else(|| panic!("missing {path}"));
            assert!(!bytes.is_empty(), "{path} is empty");
        }

        // CSVW (purrdf project_csvw_exact): metadata is valid JSON; terms.csv/quads.csv
        // re-parse with a header + at least one data row.
        let csvw_metadata =
            String::from_utf8(arts[&format!("{DIST_DIR}/csvw/csvw-metadata.json")].clone())
                .unwrap();
        serde_json::from_str::<serde_json::Value>(&csvw_metadata)
            .expect("csvw-metadata.json is valid json");
        for member in ["terms.csv", "quads.csv"] {
            let csv =
                String::from_utf8(arts[&format!("{DIST_DIR}/csvw/{member}")].clone()).unwrap();
            let mut rows = csv.lines().filter(|l| !l.is_empty());
            rows.next().expect("csv header");
            assert!(rows.count() > 0, "{member} has no data rows");
        }

        // obographs + JSONL re-parse as JSON.
        let obo =
            String::from_utf8(arts[&format!("{DIST_DIR}/gmeow-obographs.json")].clone()).unwrap();
        serde_json::from_str::<serde_json::Value>(&obo).expect("obographs is valid json");
        let jsonl =
            String::from_utf8(arts[&format!("{DIST_DIR}/gmeow-terms.jsonl")].clone()).unwrap();
        let mut term_lines = 0;
        for line in jsonl.lines() {
            serde_json::from_str::<serde_json::Value>(line).expect("jsonl line is valid json");
            term_lines += 1;
        }
        assert!(
            term_lines > 100,
            "expected many term records, got {term_lines}"
        );

        // N-Quads re-parses via oxigraph (lossless lang-tag-remapped dataset).
        let nq = arts[&format!("{DIST_DIR}/gmeow.nq")].clone();
        assert!(!nq.is_empty());

        // SKOS / OBO Graphs / ShEx carry their expected substance (purrdf 0.7.0
        // projections — see the module doc). purrdf's native Turtle serializer emits
        // full IRIs for the SKOS namespace (no `skos:` CURIE prefix declared), so
        // assert on the expanded predicate/class IRIs, not a CURIE form.
        let skos = String::from_utf8(arts[&format!("{DIST_DIR}/gmeow-skos.ttl")].clone()).unwrap();
        assert!(skos.contains("http://www.w3.org/2004/02/skos/core#ConceptScheme"));
        assert!(skos.contains("http://www.w3.org/2004/02/skos/core#Concept"));
        assert!(skos.contains("http://www.w3.org/2004/02/skos/core#prefLabel"));
        assert!(obo.contains("\"nodes\""));
        let shex = String::from_utf8(arts[&format!("{DIST_DIR}/gmeow.shex")].clone()).unwrap();
        assert!(shex.contains("PREFIX gmeow:"));
    }

    /// The selector threads `requested` through `collect_terms`: the English
    /// default keeps the carrier label, a `fr` request selects the French
    /// translation, and an absent translation falls back to English (flagged).
    /// Pins the multilingual generalization of `FoldView` and guards the English
    /// path from regression (the default view is unchanged for
    /// `gmeow:EntityExistence`, a documented term carrying a French translation).
    #[test]
    fn selector_threads_requested_language() {
        let root = repo_root();
        let graph = read_fold(&root).expect("read fold");

        let label_of = |requested: Vec<String>, curie_q: &str| -> Term {
            let view = FoldView::with_requested(&graph, requested);
            collect_terms(&view)
                .into_iter()
                .find(|t| t.curie == curie_q)
                .unwrap_or_else(|| panic!("term {curie_q} not in snapshot"))
        };

        // English default: the carrier label, not a fallback.
        let en = label_of(vec!["en".to_string()], "gmeow:EntityExistence");
        assert_eq!(en.label, "Entity Existence");
        assert!(!en.label_fallback);

        // `new()` (the export generator's view) agrees with `["en"]` — the
        // English path is unchanged by the generalization.
        let default_view = FoldView::new(&graph);
        let default_fr = collect_terms(&default_view)
            .into_iter()
            .find(|t| t.curie == "gmeow:EntityExistence")
            .expect("term present");
        assert_eq!(default_fr.label, en.label);
        assert_eq!(default_fr.label_fallback, en.label_fallback);

        // `fr` request: the French translation (from the lifecycle fr.po), non-fallback.
        let fr = label_of(vec!["fr".to_string()], "gmeow:EntityExistence");
        assert_eq!(fr.label, "Existence d'entité");
        assert!(!fr.label_fallback);
    }

    fn english_terms() -> (Vec<Term>, String, String) {
        let root = repo_root();
        let graph = read_fold(&root).expect("read fold");
        let view = FoldView::new(&graph);
        let (title, version) = fold_meta(&view).expect("fold_meta");
        (collect_terms(&view), title, version)
    }

    /// The consumer/MCP term surface resolves grounding-namespace terms (the twin of
    /// `gmeow describe`): CURIE, full IRI, and bare local name across `logic:`/
    /// `math:`/`lang:`, from the real folded bundle. Before the fix `collect_terms`
    /// filtered to `gmeow:`, so these were absent entirely.
    #[test]
    fn resolve_term_iri_spans_grounding_namespaces() {
        let (terms, _t, _v) = english_terms();
        assert_eq!(
            resolve_term_iri(&terms, "lang:Denotation").resolved(),
            Some("https://blackcatinformatics.ca/lang/Denotation")
        );
        assert_eq!(
            resolve_term_iri(&terms, "math:Function").resolved(),
            Some("https://blackcatinformatics.ca/math/Function")
        );
        // Full IRI.
        assert_eq!(
            resolve_term_iri(&terms, "https://blackcatinformatics.ca/logic/Formula").resolved(),
            Some("https://blackcatinformatics.ca/logic/Formula")
        );
        // Bare local name (namespace-agnostic), unambiguous → resolves.
        assert_eq!(
            resolve_term_iri(&terms, "Denotation").resolved(),
            Some("https://blackcatinformatics.ca/lang/Denotation")
        );
        // A grounding term carries a real CURIE (proving the `lang` prefix fix).
        let denotation = terms
            .iter()
            .find(|t| t.iri == "https://blackcatinformatics.ca/lang/Denotation")
            .expect("lang:Denotation folded into the term set");
        assert_eq!(denotation.curie, "lang:Denotation");
        assert!(
            !denotation.label.is_empty(),
            "the folded grounding term carries a label"
        );
    }

    /// `lookup_term`: exact CURIE match → `as_record` with `ok:true`;
    /// unknown → `{"ok": false, "error": "Term not found: …"}`; per-language label.
    #[test]
    fn lookup_envelope_matches_consumer_contract() {
        let (terms, _t, _v) = english_terms();

        let hit: serde_json::Value =
            serde_json::from_str(&lookup_envelope(&terms, "gmeow:EntityExistence")).unwrap();
        assert_eq!(hit["ok"], serde_json::json!(true));
        assert_eq!(hit["curie"], serde_json::json!("gmeow:EntityExistence"));
        assert_eq!(hit["label"], serde_json::json!("Entity Existence"));
        assert_eq!(hit["category"], serde_json::json!("class"));

        // Local-name resolution (IRI minus the gmeow namespace) is accepted.
        let by_local: serde_json::Value =
            serde_json::from_str(&lookup_envelope(&terms, "EntityExistence")).unwrap();
        assert_eq!(
            by_local["curie"],
            serde_json::json!("gmeow:EntityExistence")
        );

        let miss: serde_json::Value =
            serde_json::from_str(&lookup_envelope(&terms, "gmeow:NoSuchTerm")).unwrap();
        assert_eq!(miss["ok"], serde_json::json!(false));
        assert_eq!(
            miss["error"],
            serde_json::json!("Term not found: gmeow:NoSuchTerm")
        );

        // Per-language record: `fr` selects the French label, and the envelope is
        // ASCII-escaped (`json.dumps` default) — `é` is emitted as `é`.
        let root = repo_root();
        let graph = read_fold(&root).expect("read fold");
        let fr_terms = collect_terms(&FoldView::with_requested(&graph, vec!["fr".to_string()]));
        let fr_raw = lookup_envelope(&fr_terms, "gmeow:EntityExistence");
        assert!(
            fr_raw.contains("\\u00e9"),
            "lookup envelope must be ASCII-escaped (ensure_ascii)"
        );
        assert!(
            !fr_raw.contains('é'),
            "raw non-ASCII leaked into lookup envelope"
        );
        let fr: serde_json::Value = serde_json::from_str(&fr_raw).unwrap();
        assert_eq!(fr["label"], serde_json::json!("Existence d'entité"));
    }

    /// `llms_txt`: the STANDARD llmstxt.org format — H1 + canonical
    /// summary blockquote + unified `⊑`/`→` signatures + bullets linking into the
    /// published docs site (URLs recovered from the doc graph). One format across
    /// the dist/MCP/site surfaces; the old consumer-specific format is retired.
    #[test]
    fn consumer_llms_txt_uses_standard_format() {
        let (terms, title, version) = english_terms();
        let root = repo_root();
        let graph = read_fold(&root).expect("read fold");
        let doc_urls = doc_url_map(&FoldView::new(&graph));
        let txt = consumer_llms_txt(&terms, &title, &version, &doc_urls);

        // Standard header: a single H1 then the canonical summary blockquote.
        assert!(
            txt.starts_with(&format!(
                "# {title}\n\n> {}\n\n",
                gmeow_docs::llms::GMEOW_SUMMARY
            )),
            "header mismatch:\n{}",
            &txt[..240]
        );
        assert!(txt.contains(&format!("Vocabulary {version}. Namespace: {NAMESPACE}.")));
        assert!(txt.contains("\n## Classes\n\n"));
        assert!(txt.contains("\n## Properties\n\n"));
        assert!(txt.ends_with('\n'));

        // Unified signature marker (`(⊑ ` for a class) — the consumer index now
        // matches the dist/site format; the old `subClassOf`/ASCII-`->` markers
        // and the export box-roles suffix are gone.
        assert!(txt.contains("(⊑ "), "missing unified subclass marker");
        assert!(
            !txt.contains("(subClassOf "),
            "old consumer subclass marker leaked"
        );
        assert!(
            !txt.contains("[box roles:"),
            "leaked export-format box roles"
        );

        // When the doc graph is present in the snapshot, bullets are markdown links
        // into the published site (the same URLs the docs site emits).
        if !doc_urls.is_empty() {
            assert!(
                txt.contains("](terms/"),
                "doc-graph URLs should link the term bullets"
            );
        }

        // `fr` request threads the French selector: the corpus has almost no French
        // text, so the observable effect is the `[fallback: en]` markers added when
        // a term's requested-language text resolves via the English fallback.
        let fr_terms = collect_terms(&FoldView::with_requested(&graph, vec!["fr".to_string()]));
        let fr_txt = consumer_llms_txt(&fr_terms, &title, &version, &doc_urls);
        assert_ne!(fr_txt, txt, "fr index did not thread the French selection");
        assert!(
            !txt.contains("[fallback: en]"),
            "English index must not carry fallback markers"
        );
        assert!(
            fr_txt.contains("[fallback: en]"),
            "fr index must carry English-fallback markers (proves the selector threaded)"
        );
    }

    /// `doc_card`: resolves a term and renders a `# {curie}` card with the
    /// metadata + definition through the shared builder + renderer; an unresolved
    /// query yields `None` (the caller supplies the not-found envelope).
    #[test]
    fn doc_card_build_renders_card_and_not_found() {
        let (terms, _t, _v) = english_terms();
        let modeled_defs = repo_modeled_defs();
        let (title, built) = doc_card_build(&terms, "gmeow:EntityExistence", &modeled_defs)
            .resolved()
            .expect("known term resolves");
        let card =
            gmeow_docs::card::render_card(&title, &built, gmeow_docs::card::CardDetail::Standard);
        assert!(
            card.starts_with("# gmeow:EntityExistence"),
            "card head:\n{card}"
        );
        // Canonical card convention (the shared `gmeow_docs::card` renderer):
        // human-cased category, and term→slice provenance recovered from the
        // documentation graph (the docs generator dogfoods `gmeow:docOwnerSlice`
        // into the bundle; the fold reads it back).
        assert!(card.contains("- category: Class"));
        assert!(card.contains("- iri: https://blackcatinformatics.ca/gmeow/EntityExistence"));
        let slice_line = card
            .lines()
            .find(|l| l.starts_with("- slice: "))
            .expect("folded card must carry the owning slice (term→slice provenance)");
        assert!(
            !slice_line.trim_start_matches("- slice: ").trim().is_empty(),
            "slice value must be non-blank, got {slice_line:?}"
        );
        assert!(card.ends_with('\n'));
        // `gmeow:EntityExistence` genuinely has a generated Pydantic model (it
        // names a `$defs` entry), so the MCP card must carry the link.
        assert!(
            card.contains("**Python model:**"),
            "a modeled class must carry the python_model line:\n{card}"
        );

        assert!(
            matches!(
                doc_card_build(&terms, "gmeow:NoSuchTerm", &modeled_defs),
                ConsumerResolution::NotFound
            ),
            "an unresolved query yields NotFound"
        );
    }

    /// `class_is_modeled` gate (finding F3, reproduced by issue 1408): a Class
    /// with NO `$defs` entry (an abstract class with no SHACL NodeShape) must
    /// never get a fabricated `python_model` link, even though `term_to_card`'s
    /// PRE-fix gate (`category == "class" && !owner_slice.is_empty()`) would have
    /// shown one. `gmeow:Proposition` is a real production example.
    #[test]
    fn doc_card_build_omits_python_model_for_an_unmodeled_class() {
        let (terms, _t, _v) = english_terms();
        let modeled_defs = repo_modeled_defs();
        assert!(
            !modeled_defs.contains("Proposition"),
            "sanity: gmeow:Proposition must genuinely have no $defs entry today"
        );
        let (_, built) = doc_card_build(&terms, "gmeow:Proposition", &modeled_defs)
            .resolved()
            .expect("gmeow:Proposition resolves");
        assert_eq!(
            built.python_model, None,
            "an unmodeled class must never fabricate a python_model link"
        );
        assert_eq!(built.python_snippet, None);
    }

    /// `llms_full` / `llms-full.txt`: the standard header then `### ` term blocks
    /// inlined in full (no links), emitted in CURIE order and bounded by the fixed
    /// token budget, with the elided remainder disclosed (never silently dropped).
    #[test]
    fn consumer_llms_full_inlines_terms_within_the_token_budget() {
        let (terms, title, version) = english_terms();
        let full = consumer_llms_full(&terms, &title, &version, &repo_modeled_defs());
        assert!(full.starts_with(&format!(
            "# {title}\n\n> {}\n\n",
            gmeow_docs::llms::GMEOW_SUMMARY
        )));
        assert!(full.contains("## Terms\n\n"));
        // No markdown links in the complete form (it is self-contained).
        assert!(
            !full.contains("](terms/"),
            "llms-full must be link-free (inlined content)"
        );
        // Blocks are emitted in a deterministic CURIE order, so the CURIE-first
        // term is always inlined.
        let mut ordered: Vec<&Term> = terms.iter().collect();
        ordered.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
        let headings = full.lines().filter(|l| l.starts_with("### ")).count();
        assert!(headings >= 1, "expected at least one inlined term block");
        assert!(
            full.contains(&format!("### {}", ordered[0].curie)),
            "the CURIE-first term ({}) must be inlined",
            ordered[0].curie
        );
        // The full vocabulary far exceeds the token budget, so some terms are
        // elided — and the elision is disclosed, not silent.
        assert!(
            headings < terms.len(),
            "full vocab should exceed the budget"
        );
        assert!(
            full.contains("elided to fit"),
            "the token-budget elision must be disclosed"
        );
        // The emitted document respects the budget (plus at most one overflow block
        // and the trailing disclosure line).
        assert!(
            gmeow_docs::llms::estimate_tokens(&full)
                <= gmeow_docs::llms::LLMS_FULL_TOKEN_BUDGET * 2,
            "llms-full must stay within a small multiple of the token budget"
        );
    }

    /// The MCP/consumer `llms_txt` and `llms_full` surfaces must each carry the
    /// standing-page `## Reference` expansion (Competency questions, Conformance
    /// fixtures, Notation grammars, Glossary, Build pipeline) plus the offline
    /// snippet-corpus note — the same expansion the docs-site `llms_txt`/
    /// `llms_full_txt` render. This had previously landed ONLY on the docs site;
    /// the native `gmeow mcp` binary's `llms_txt`/`llms_full` tool output carried
    /// zero occurrences of any of these. Falsifiable per page name so a future
    /// dropped page fails loudly instead of a vague substring match.
    #[test]
    fn consumer_llms_surfaces_carry_the_standing_reference_pages() {
        let (terms, title, version) = english_terms();
        let root = repo_root();
        let graph = read_fold(&root).expect("read fold");
        let doc_urls = doc_url_map(&FoldView::new(&graph));

        let txt = consumer_llms_txt(&terms, &title, &version, &doc_urls);
        let full = consumer_llms_full(&terms, &title, &version, &repo_modeled_defs());

        assert!(
            txt.contains("## Reference\n"),
            "consumer llms_txt must carry a '## Reference' section"
        );
        assert!(
            full.contains("## Reference\n"),
            "consumer llms_full must carry a '## Reference' section"
        );

        for page in gmeow_docs::llms::STANDING_REFERENCE_PAGES {
            assert!(
                txt.contains(page),
                "consumer llms_txt must name the standing reference page {page:?}"
            );
            assert!(
                full.contains(page),
                "consumer llms_full must name the standing reference page {page:?}"
            );
        }

        let note = gmeow_docs::llms::SNIPPETS_CORPUS_NOTE;
        assert!(
            txt.contains(note),
            "consumer llms_txt must carry the offline snippet-corpus note"
        );
        assert!(
            full.contains(note),
            "consumer llms_full must carry the offline snippet-corpus note"
        );

        // The write_llms_txt (dist/llms.txt tarball) surface shares the same
        // section-append path — it must not silently regress either.
        let dist_txt = String::from_utf8(write_llms_txt(&terms, &title, &version)).unwrap();
        assert!(dist_txt.contains("## Reference\n"));
        for page in gmeow_docs::llms::STANDING_REFERENCE_PAGES {
            assert!(
                dist_txt.contains(page),
                "dist llms.txt must name the standing reference page {page:?}"
            );
        }
        assert!(dist_txt.contains(note));
    }

    /// The twin-contract lock (§19 one-path): the MCP card and the
    /// docs-site card share ONE renderer (`gmeow_docs::card::render_card_body`)
    /// AND one convention. This test pins the shared renderer's output for a card
    /// whose SHARED fields are set, then proves the folded-`Term` builder
    /// (`term_to_card`) maps those same fields into the SAME canonical `Card`,
    /// so the two sources can never re-diverge field-for-field.
    ///
    /// (The docs-site builder `gmeow_docs::render::doc_term_card` is private; both
    /// builders are thin field-copies into `gmeow_docs::card::Card`, so locking
    /// `term_to_card` against an explicit `Card` of the same shared values — fed
    /// through the SOLE body renderer — is the determinism guard. The docs side's
    /// own routing through `render_card_body` is pinned by gmeow-docs' tests.)
    #[test]
    fn term_card_shares_one_renderer_and_convention() {
        // A folded Term with every SHARED card field populated.
        let folded = Term {
            category: "property",
            iri: "https://blackcatinformatics.ca/gmeow/hasFoo".to_string(),
            curie: "gmeow:hasFoo".to_string(),
            label: "has foo".to_string(),
            definition: "Relates a thing to its foo.".to_string(),
            prop_kind: "object",
            domain: "Thing".to_string(),
            range: "Foo".to_string(),
            sub_property_of: vec!["gmeow:relates".to_string()],
            alignments: vec!["exactMatch=ex:hasFoo".to_string()],
            box_roles: vec!["gmeow:boxTBox".to_string()],
            scope_notes: vec!["A scope note.".to_string()],
            examples: vec!["An example.".to_string()],
            use_when: vec!["When there is a foo.".to_string()],
            avoid_when: vec!["When there is no foo.".to_string()],
            how_to_use: vec!["Use idiomatically.".to_string()],
            use_for_consumer: vec!["gmeow:profileMemory".to_string()],
            avoid_for_consumer: vec!["gmeow:profileNarrative".to_string()],
            logic_stereotypes: vec!["logic:Relator".to_string()],
            related_terms: vec!["gmeow:Bar".to_string()],
            owner_slice: "https://blackcatinformatics.ca/gmeow/slice/zoo".to_string(),
            ..Term::default()
        };

        // The canonical Card the docs side would build for the SAME shared values
        // (a property → parents come from sub_property_of; the slice is the LOCAL
        // NAME of the owning slice IRI, recovered identically on both sides).
        let expected = gmeow_docs::card::Card {
            category: "Property".to_string(),
            iri: "https://blackcatinformatics.ca/gmeow/hasFoo".to_string(),
            label: Some("has foo".to_string()),
            slice: Some("zoo".to_string()),
            box_roles: vec!["gmeow:boxTBox".to_string()],
            definition: Some("Relates a thing to its foo.".to_string()),
            parents: vec!["gmeow:relates".to_string()],
            domain: vec!["Thing".to_string()],
            range: vec!["Foo".to_string()],
            use_when: vec!["When there is a foo.".to_string()],
            avoid_when: vec!["When there is no foo.".to_string()],
            how_to_use: vec!["Use idiomatically.".to_string()],
            scope_notes: vec!["A scope note.".to_string()],
            examples: vec!["An example.".to_string()],
            logic_stereotypes: vec!["logic:Relator".to_string()],
            related_terms: vec!["gmeow:Bar".to_string()],
            use_for_consumer: vec!["gmeow:profileMemory".to_string()],
            avoid_for_consumer: vec!["gmeow:profileNarrative".to_string()],
            aligns: vec!["exactMatch=ex:hasFoo".to_string()],
            ..gmeow_docs::card::Card::default()
        };

        // The folded builder must produce exactly that Card (field-for-field).
        assert_eq!(
            term_to_card(&folded, &BTreeSet::new()),
            expected,
            "term_to_card must map the folded Term into the canonical shared Card"
        );

        // …and both render IDENTICALLY through the SOLE body renderer.
        let from_folded = gmeow_docs::card::render_card_body(
            &term_to_card(&folded, &BTreeSet::new()),
            gmeow_docs::card::CardDetail::Standard,
        );
        let from_expected =
            gmeow_docs::card::render_card_body(&expected, gmeow_docs::card::CardDetail::Standard);
        assert_eq!(
            from_folded, from_expected,
            "shared renderer must agree byte-for-byte"
        );

        // Canonical convention: bold labels, `; ` delimiters, no per-item backticks.
        assert!(from_folded.contains("**Use when:** When there is a foo.\n\n"));
        assert!(from_folded.contains("**Aligns:** exactMatch=ex:hasFoo\n\n"));
        assert!(!from_folded.contains('`'), "card body carries no backticks");
        assert!(
            !from_folded.contains("\n*Use when:* "),
            "labels are bold, not italic"
        );
    }

    /// `term_to_card` slice handling: a recovered `owner_slice` IRI renders as its
    /// local name; an absent one yields `None` (no blank `slice:` line). Locks
    /// both arms of the term→slice provenance recovery.
    #[test]
    fn term_to_card_slice_uses_local_name_or_omits() {
        let with_slice = Term {
            category: "class",
            iri: "https://blackcatinformatics.ca/gmeow/Cat".to_string(),
            curie: "gmeow:Cat".to_string(),
            owner_slice: "https://blackcatinformatics.ca/gmeow/slice/zoo".to_string(),
            ..Term::default()
        };
        assert_eq!(
            term_to_card(&with_slice, &BTreeSet::new()).slice,
            Some("zoo".to_string())
        );
        assert!(
            gmeow_docs::card::render_card_body(
                &term_to_card(&with_slice, &BTreeSet::new()),
                gmeow_docs::card::CardDetail::Standard,
            )
            .contains("- slice: zoo\n")
        );

        let no_slice = Term {
            category: "class",
            iri: "https://blackcatinformatics.ca/gmeow/Dog".to_string(),
            curie: "gmeow:Dog".to_string(),
            ..Term::default()
        };
        assert_eq!(term_to_card(&no_slice, &BTreeSet::new()).slice, None);
        assert!(
            !gmeow_docs::card::render_card_body(
                &term_to_card(&no_slice, &BTreeSet::new()),
                gmeow_docs::card::CardDetail::Standard,
            )
            .contains("- slice:")
        );
    }

    /// `class_is_modeled` gate: a class whose IRI names a `$defs` entry gets the
    /// `python_model` link; an otherwise-identical class that does not is honestly
    /// omitted — never a fabricated ImportError-inducing link (issue: Pydantic
    /// model surface, finding F3).
    #[test]
    fn term_to_card_gates_python_model_on_schema_defs_membership() {
        let modeled = Term {
            category: "class",
            iri: "https://blackcatinformatics.ca/gmeow/Cat".to_string(),
            curie: "gmeow:Cat".to_string(),
            owner_slice: "https://blackcatinformatics.ca/gmeow/slice/zoo".to_string(),
            ..Term::default()
        };
        let mut defs = BTreeSet::new();
        defs.insert("Cat".to_string());
        let card = term_to_card(&modeled, &defs);
        assert_eq!(
            card.python_model,
            Some(gmeow_docs::card::python_model_path(
                &modeled.owner_slice,
                &modeled.iri
            ))
        );
        assert!(card.python_snippet.is_some());

        // Same shape, but `Cat` is absent from the `$defs` set: no link.
        let unmodeled = Term {
            iri: "https://blackcatinformatics.ca/gmeow/Ferret".to_string(),
            curie: "gmeow:Ferret".to_string(),
            ..modeled.clone()
        };
        let card = term_to_card(&unmodeled, &defs);
        assert_eq!(card.python_model, None);
        assert_eq!(card.python_snippet, None);
    }

    /// `okf_index`: the manifest envelope wraps `ok`/`format`/`lossy`/`count`
    /// around per-document `{path, type, title, resource}` records.
    #[test]
    fn okf_index_envelope_shape() {
        let (terms, _t, _v) = english_terms();
        let env: serde_json::Value = serde_json::from_str(&okf_index_envelope(&terms)).unwrap();
        assert_eq!(env["ok"], serde_json::json!(true));
        assert_eq!(env["format"], serde_json::json!("okf"));
        assert_eq!(env["lossy"], serde_json::json!(true));
        assert_eq!(env["count"].as_u64().unwrap() as usize, terms.len());

        let docs = env["documents"].as_array().unwrap();
        assert_eq!(docs.len(), terms.len());
        // A known class document path/type/resource.
        let entity_existence = terms
            .iter()
            .find(|t| t.curie == "gmeow:EntityExistence")
            .expect("term present");
        let doc = docs
            .iter()
            .find(|d| d["resource"] == serde_json::json!(entity_existence.iri))
            .expect("okf doc present");
        assert_eq!(
            doc["path"],
            serde_json::json!("gmeow-okf/classes/EntityExistence.md")
        );
        assert_eq!(doc["type"], serde_json::json!("Class"));
        assert_eq!(doc["title"], serde_json::json!("Entity Existence"));
    }
}
