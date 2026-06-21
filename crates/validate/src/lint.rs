// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free engine for the structural, naming, and ownership lints (#579).
//!
//! These three lints — ported byte-exact from `src/gmeow_tools/validate.py`'s
//! `structural_lint`, `term_naming_lint`, and `slice_ownership_lint` — run over
//! an oxigraph [`Store`] built from the merged ontology sources. The Python
//! repr-exact language-tag diagnostics (Check 1 / Check 2) are reproduced via
//! [`py_str_repr`], which mirrors CPython's `str.__repr__` so the rdflib
//! `Literal` repr framing is preserved on the rare violation paths.
//!
//! Engine-core separation: this module imports no pyo3. The [`crate::py`]
//! bindings adapt these functions to Python.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use oxigraph::model::{Literal, NamedOrBlankNode, Term};
use oxigraph::store::Store;
use regex::Regex;

use crate::model::{owl, rdf, rdfs, skos};

/// Strongly-typed configuration for the three lints, supplied by the Python
/// caller from its single-source-of-truth constants. No untyped dict bag — every
/// field is explicit so the FFI boundary stays legible.
#[derive(Debug, Clone)]
pub struct LintConfig {
    /// The GMEOW vocabulary namespace (`config.NAMESPACE`).
    pub namespace: String,
    /// The GMEOW ontology IRI (`config.ONTOLOGY_IRI`).
    pub ontology_iri: String,
    /// CamelCase selector tokens that mark a privileged name (`_SELECTOR_TOKENS`).
    pub selector_tokens: BTreeSet<String>,
    /// Core-slice IRIs — the set whose membership grades a term as Tier-1.
    pub core_slice_iris: HashSet<String>,
    /// Standard annotation predicates whose literals are policed by Check 2.
    /// Defaults to [`default_annotation_predicates`] — this crate is the single
    /// source of truth (#630); Python reads the set from here, it is no longer
    /// pushed in from `language_tags`.
    pub annotation_predicates: HashSet<String>,
}

/// The canonical annotation predicates whose literals the Check-2 language-tag
/// policy polices — `rdfs:label`, `skos:definition`, `rdfs:comment`, `dcterms:title`,
/// `dcterms:description`. This crate owns the registry (#630); the Python
/// `language_tags` helpers read it back through the PyO3 `annotation_predicates`
/// surface rather than maintaining a parallel constant.
#[must_use]
pub fn default_annotation_predicates() -> Vec<String> {
    [
        "http://www.w3.org/2000/01/rdf-schema#label",
        "http://www.w3.org/2004/02/skos/core#definition",
        "http://www.w3.org/2000/01/rdf-schema#comment",
        "http://purl.org/dc/terms/title",
        "http://purl.org/dc/terms/description",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

/// The structural kind of a GMEOW term — the priority order is the index here,
/// most-specific first (`_TERM_KIND_ORDER`).
const TERM_KIND_ORDER: [&str; 6] = [
    "ontology",
    "class",
    "property",
    "annotation property",
    "datatype",
    "individual",
];

fn kind_rank(kind: &str) -> usize {
    TERM_KIND_ORDER
        .iter()
        .position(|k| *k == kind)
        .expect("kind must be one of TERM_KIND_ORDER")
}

/// A `{"errors": [...], "warnings": [...]}` report.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LintReport {
    /// Error diagnostics.
    pub errors: Vec<String>,
    /// Warning diagnostics.
    pub warnings: Vec<String>,
}

/// Return whether an IRI is the GMEOW root or lives in its namespace
/// (mirrors `_is_gmeow_term`).
fn is_gmeow_term(iri: &str, cfg: &LintConfig) -> bool {
    iri.starts_with(&cfg.namespace) || iri == cfg.ontology_iri
}

/// Return the primary structural kind of a GMEOW term from its `rdf:type` set
/// (mirrors `_term_kind`).
fn term_kind(types: &HashSet<String>) -> &'static str {
    if types.contains(owl::ONTOLOGY.as_str()) {
        return "ontology";
    }
    if types.contains(owl::CLASS.as_str()) {
        return "class";
    }
    if types.contains(owl::ANNOTATION_PROPERTY.as_str()) {
        return "annotation property";
    }
    if types.contains(owl::OBJECT_PROPERTY.as_str())
        || types.contains(owl::DATATYPE_PROPERTY.as_str())
    {
        return "property";
    }
    if types.contains(rdfs::DATATYPE.as_str()) {
        return "datatype";
    }
    "individual"
}

/// All `rdf:type` object IRIs of `subject`, as a set (blank/literal types are
/// impossible for `rdf:type` objects we care about, but non-IRI objects are
/// simply skipped — matching `set(graph.objects(term, RDF.type))` where the
/// kind probe only ever compares against named OWL/RDFS classes).
fn rdf_types(store: &Store, subject: &NamedOrBlankNode) -> HashSet<String> {
    let mut out = HashSet::new();
    for quad in store
        .quads_for_pattern(Some(subject.as_ref()), Some(rdf::TYPE), None, None)
        .flatten()
    {
        if let Term::NamedNode(n) = quad.object {
            out.insert(n.into_string());
        }
    }
    out
}

/// Map every GMEOW-namespaced term with an `rdf:type` to its primary kind
/// (mirrors `_collect_typed_terms`). The returned map is keyed by term IRI.
pub fn collect_typed_terms(store: &Store, cfg: &LintConfig) -> BTreeMap<String, String> {
    let mut terms: BTreeMap<String, String> = BTreeMap::new();
    let typed_queries = [
        owl::ONTOLOGY,
        owl::CLASS,
        owl::OBJECT_PROPERTY,
        owl::DATATYPE_PROPERTY,
        owl::ANNOTATION_PROPERTY,
        rdfs::DATATYPE,
    ];
    for rdf_type in typed_queries {
        for quad in store
            .quads_for_pattern(None, Some(rdf::TYPE), Some(rdf_type.into()), None)
            .flatten()
        {
            let subject = match &quad.subject {
                NamedOrBlankNode::NamedNode(n) => n.as_str().to_owned(),
                NamedOrBlankNode::BlankNode(_) => continue,
            };
            if !is_gmeow_term(&subject, cfg) {
                continue;
            }
            let kind = term_kind(&rdf_types(store, &quad.subject));
            match terms.get(&subject) {
                Some(current) if kind_rank(kind) >= kind_rank(current) => {}
                _ => {
                    terms.insert(subject, kind.to_owned());
                }
            }
        }
    }
    // Any remaining GMEOW subjects with an explicit rdf:type → individual.
    for quad in store
        .quads_for_pattern(None, Some(rdf::TYPE), None, None)
        .flatten()
    {
        if let NamedOrBlankNode::NamedNode(n) = &quad.subject {
            let iri = n.as_str();
            if is_gmeow_term(iri, cfg) && !terms.contains_key(iri) {
                terms.insert(iri.to_owned(), "individual".to_owned());
            }
        }
    }
    terms
}

/// Whether `(subject, predicate, *)` has at least one triple.
fn has_predicate(
    store: &Store,
    subject_iri: &str,
    predicate: oxigraph::model::NamedNodeRef,
) -> bool {
    let subject = oxigraph::model::NamedNode::new_unchecked(subject_iri);
    store
        .quads_for_pattern(Some((&subject).into()), Some(predicate), None, None)
        .next()
        .is_some()
}

/// Object IRIs of `(subject_iri, predicate, ?)` (named-node objects only).
fn object_iris(
    store: &Store,
    subject_iri: &str,
    predicate: oxigraph::model::NamedNodeRef,
) -> HashSet<String> {
    let subject = oxigraph::model::NamedNode::new_unchecked(subject_iri);
    let mut out = HashSet::new();
    for quad in store
        .quads_for_pattern(Some((&subject).into()), Some(predicate), None, None)
        .flatten()
    {
        if let Term::NamedNode(n) = quad.object {
            out.insert(n.into_string());
        }
    }
    out
}

/// Whether `(subject_iri, rdf:type, type_iri)` exists.
fn has_type(
    store: &Store,
    subject_iri: oxigraph::model::NamedNodeRef,
    type_iri: oxigraph::model::NamedNodeRef,
) -> bool {
    store
        .quads_for_pattern(
            Some(subject_iri.into()),
            Some(rdf::TYPE),
            Some(type_iri.into()),
            None,
        )
        .any(|r| r.is_ok())
}

/// The structural lint over the merged store (mirrors `structural_lint`).
pub fn structural_lint(store: &Store, cfg: &LintConfig) -> LintReport {
    let mut report = LintReport::default();
    let typed = collect_typed_terms(store, cfg);
    let graph_box_role = ns_node(cfg, "graphBoxRole");
    let graph_box_role_class = ns_node(cfg, "GraphBoxRole");

    // The `self` self-description ontology (`<namespace>self`) holds the project's
    // own A-Box metadata — its contributions, citations, manifestations, license,
    // and version IRI. Those individuals are instance data, not vocabulary
    // surface, so the per-term annotation/graphBoxRole contract does not apply to
    // them (the triad governs the vocabulary surface only). This matters when
    // validating the committed GTS bundle, which folds in `metadata/gmeow-self.ttl`
    // that the Turtle source set excludes (#644). A term is exempt when it is the
    // `self` ontology header itself or is `rdfs:isDefinedBy <namespace>self`.
    let self_ontology = format!("{}self", cfg.namespace);
    let self_node = oxigraph::model::NamedNode::new_unchecked(&self_ontology);
    // Precompute the self-description A-Box once: one store scan instead of a
    // per-term query, and no `new_unchecked` on arbitrary subject strings.
    let mut self_defined: std::collections::HashSet<oxigraph::model::NamedNode> =
        std::collections::HashSet::new();
    for quad in store
        .quads_for_pattern(
            None,
            Some(rdfs::IS_DEFINED_BY),
            Some((&self_node).into()),
            None,
        )
        .flatten()
    {
        if let NamedOrBlankNode::NamedNode(subject) = quad.subject {
            self_defined.insert(subject);
        }
    }

    // 1. Per-term required annotations (sorted by IRI — BTreeMap iterates sorted).
    for (term, kind) in &typed {
        let subject = oxigraph::model::NamedNode::new_unchecked(term);
        if subject == self_node || self_defined.contains(&subject) {
            continue;
        }
        if !has_predicate(store, term, rdfs::LABEL) {
            report
                .errors
                .push(format!("{kind} {term} is missing rdfs:label"));
        }
        if !has_predicate(store, term, skos::DEFINITION) {
            report
                .errors
                .push(format!("{kind} {term} is missing skos:definition"));
        }
        if !has_predicate(store, term, rdfs::IS_DEFINED_BY) {
            report
                .errors
                .push(format!("{kind} {term} is missing rdfs:isDefinedBy"));
        }
        let mut has_role = false;
        for quad in store
            .quads_for_pattern(
                Some((&subject).into()),
                Some(graph_box_role.as_ref()),
                None,
                None,
            )
            .flatten()
        {
            has_role = true;
            let role = match &quad.object {
                Term::NamedNode(role) => role,
                other => {
                    report.errors.push(format!(
                        "{kind} {term} has non-IRI gmeow:graphBoxRole value {other}"
                    ));
                    continue;
                }
            };
            if !has_type(store, role.as_ref(), graph_box_role_class.as_ref()) {
                report.errors.push(format!(
                    "{kind} {term} has gmeow:graphBoxRole value {} that is not a gmeow:GraphBoxRole",
                    role.as_str()
                ));
            }
        }
        if !has_role {
            report
                .errors
                .push(format!("{kind} {term} is missing gmeow:graphBoxRole"));
        }
    }

    let declared: HashSet<&String> = typed.keys().collect();

    // 2. Tier-1 depth warnings.
    let use_when = ns_node(cfg, "useWhen");
    let how_to_use = ns_node(cfg, "howToUse");
    for (term, kind) in &typed {
        if kind != "class" && kind != "property" {
            continue;
        }
        let defined_by = object_iris(store, term, rdfs::IS_DEFINED_BY);
        if !defined_by.iter().any(|d| cfg.core_slice_iris.contains(d)) {
            continue;
        }
        if !has_predicate(store, term, use_when.as_ref()) {
            report.warnings.push(format!(
                "{kind} {term} is missing gmeow:useWhen (Tier-1 depth, #471)"
            ));
        }
        let has_how_to_use = has_predicate(store, term, how_to_use.as_ref());
        if !has_how_to_use {
            report.warnings.push(format!(
                "{kind} {term} is missing gmeow:howToUse (Tier-1 depth, #471)"
            ));
        } else if !has_predicate(store, term, skos::EXAMPLE) {
            report.warnings.push(format!(
                "{kind} {term} has gmeow:howToUse but no skos:example (Tier-1 depth, #471)"
            ));
        }
    }

    // 3. use/avoidForConsumer must point at a gmeow:ProjectionContext.
    let projection_context = ns_node(cfg, "ProjectionContext");
    for local in ["useForConsumer", "avoidForConsumer"] {
        let predicate = ns_node(cfg, local);
        for quad in store
            .quads_for_pattern(None, Some(predicate.as_ref()), None, None)
            .flatten()
        {
            let subject = subject_display(&quad.subject);
            let consumer = match &quad.object {
                Term::NamedNode(n) => Some(n.as_str().to_owned()),
                _ => None,
            };
            let is_projection_context = match &consumer {
                Some(c) => {
                    let cn = oxigraph::model::NamedNode::new_unchecked(c);
                    store
                        .quads_for_pattern(
                            Some((&cn).into()),
                            Some(rdf::TYPE),
                            Some(projection_context.as_ref().into()),
                            None,
                        )
                        .next()
                        .is_some()
                }
                None => false,
            };
            if !is_projection_context {
                let consumer_text = match &quad.object {
                    Term::NamedNode(n) => n.as_str().to_owned(),
                    Term::BlankNode(b) => format!("_:{}", b.as_str()),
                    Term::Literal(l) => l.value().to_owned(),
                    #[allow(unreachable_patterns)]
                    other => format!("{other}"),
                };
                report.errors.push(format!(
                    "{predicate_iri} on {subject} points to non-ProjectionContext value {consumer_text}",
                    predicate_iri = predicate.as_str(),
                ));
            }
        }
    }

    // 4. Dangling GMEOW subclass/subproperty targets.
    for predicate in [rdfs::SUB_CLASS_OF, rdfs::SUB_PROPERTY_OF] {
        for quad in store
            .quads_for_pattern(None, Some(predicate), None, None)
            .flatten()
        {
            if let Term::NamedNode(target) = &quad.object {
                let t = target.as_str();
                if is_gmeow_term(t, cfg) && !declared.contains(&t.to_owned()) {
                    report.errors.push(format!(
                        "dangling {pred} target (undeclared GMEOW term): {t}",
                        pred = predicate.as_str(),
                    ));
                }
            }
        }
    }

    // 5. Comprehensiveness heuristic.
    let mut parent_to_children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for quad in store
        .quads_for_pattern(None, Some(rdfs::SUB_CLASS_OF), None, None)
        .flatten()
    {
        let child = match &quad.subject {
            NamedOrBlankNode::NamedNode(n) => n.as_str().to_owned(),
            NamedOrBlankNode::BlankNode(_) => continue,
        };
        let parent = match &quad.object {
            Term::NamedNode(n) => n.as_str().to_owned(),
            _ => continue,
        };
        if is_gmeow_term(&child, cfg) && is_gmeow_term(&parent, cfg) {
            parent_to_children.entry(parent).or_default().push(child);
        }
    }
    for (parent, children) in &parent_to_children {
        if children.len() < 3 {
            continue;
        }
        let missing = children
            .iter()
            .filter(|c| !has_predicate(store, c, skos::DEFINITION))
            .count();
        if missing >= 3 {
            report.warnings.push(format!(
                "class {parent} has {missing} of {total} direct subclasses missing \
                 skos:definition (systematic documentation gap)",
                total = children.len(),
            ));
        }
    }

    // 6. Language-tag discipline over ALL triples.
    let x_gmeow = Regex::new(r"(?i)^x-gmeow-[a-z0-9\-]+$").expect("static regex");
    for quad in store.iter().flatten() {
        let predicate_iri = quad.predicate.as_str();
        let literal = match &quad.object {
            Term::Literal(l) => Some(l),
            _ => None,
        };

        // Check 1: literal on a GMEOW-namespace predicate.
        if let Some(lit) = literal {
            if predicate_iri.starts_with(&cfg.namespace) {
                if let Some(lang) = lit.language() {
                    if !x_gmeow.is_match(lang) {
                        let subject = subject_display(&quad.subject);
                        report.errors.push(format!(
                            "literal {lit_repr} (on subject {subject}, predicate {predicate_iri}) \
                             carries external or invalid language tag '{lang}'; GMEOW internal \
                             data must use the private-use 'x-gmeow-' prefix.",
                            lit_repr = literal_repr(lit),
                        ));
                    }
                }
            }
        }

        // Check 2: standard annotation predicate on a GMEOW-authored subject.
        if let (NamedOrBlankNode::NamedNode(subj), Some(lit)) = (&quad.subject, literal) {
            if is_gmeow_term(subj.as_str(), cfg) {
                if let Some(msg) =
                    check_annotation_literal(subj.as_str(), predicate_iri, lit, cfg, &x_gmeow)
                {
                    report.errors.push(msg);
                }
            }
        }
    }

    report
}

/// Port of `language_tags.check_annotation_literal`. Returns the external-tag
/// error only when the literal has a language, is not an internal `x-gmeow-*`
/// tag, the predicate is NOT in the GMEOW namespace, and the predicate IS one of
/// the standard annotation predicates.
///
/// `internal_re` is the pre-compiled `x-gmeow-*` regex passed in by the caller
/// so it is not recompiled on every invocation (R11 / R12 hoist).
fn check_annotation_literal(
    subject: &str,
    predicate: &str,
    obj: &Literal,
    cfg: &LintConfig,
    internal_re: &Regex,
) -> Option<String> {
    let lang = obj.language()?;
    if internal_re.is_match(lang) {
        return None;
    }
    if predicate.starts_with(&cfg.namespace) {
        return None;
    }
    if !cfg.annotation_predicates.contains(predicate) {
        return None;
    }
    Some(format!(
        "literal {lit_repr} (on subject {subject}, predicate {predicate}) carries external \
         language tag '{lang}'; GMEOW-authored terms must use the private-use 'x-gmeow-' prefix \
         on standard annotation predicates.",
        lit_repr = literal_repr(obj),
    ))
}

/// Render an oxigraph [`Literal`] the way rdflib's `repr(Literal)` renders it
/// for a language-tagged literal: `rdflib.term.Literal('value', lang='xx')`.
///
/// Both language-tag checks only fire on language-tagged literals, so the
/// `datatype=` repr branch is unreachable here — every literal reaching this
/// function has a language tag. The lexical value is rendered with
/// [`py_str_repr`] (CPython `str.__repr__`), matching rdflib byte-for-byte.
fn literal_repr(lit: &Literal) -> String {
    let lang = lit.language().unwrap_or("");
    format!(
        "rdflib.term.Literal({value}, lang={lang})",
        value = py_str_repr(lit.value()),
        lang = py_str_repr(lang),
    )
}

/// Mirror CPython's `str.__repr__` (`repr()` of a `str`).
///
/// Quote choice: single quotes by default; switch to double quotes if the string
/// contains a single quote but no double quote. Inside the chosen quote, escape
/// backslash, the active quote char, and the C-style escapes `\t \n \r`; other
/// control / non-printable characters use `\xHH` / `\uHHHH` / `\UHHHHHHHH`.
/// Printable non-ASCII (per Unicode) is emitted verbatim, matching CPython.
fn py_str_repr(s: &str) -> String {
    let has_single = s.contains('\'');
    let has_double = s.contains('"');
    let quote = if has_single && !has_double { '"' } else { '\'' };

    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if is_py_printable(c) => out.push(c),
            c => {
                let cp = c as u32;
                if cp <= 0xff {
                    out.push_str(&format!("\\x{cp:02x}"));
                } else if cp <= 0xffff {
                    out.push_str(&format!("\\u{cp:04x}"));
                } else {
                    out.push_str(&format!("\\U{cp:08x}"));
                }
            }
        }
    }
    out.push(quote);
    out
}

/// Approximate CPython's `str.isprintable()` for a single char: a char is
/// printable unless it is in a "Other" or "Separator" Unicode category, except
/// ASCII space (U+0020), which is printable. ASCII control chars and the common
/// separators/format chars are non-printable; the rest of the BMP/astral
/// printable range is emitted verbatim.
fn is_py_printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    if c.is_control() {
        return false;
    }
    if c.is_whitespace() {
        // Non-space whitespace (separators) are escaped by CPython repr.
        return false;
    }
    // Format / unassigned / surrogate-ish: oxigraph values are valid scalar
    // values, so treat the remaining assigned chars as printable. This is exact
    // for every literal the lints actually emit (the violation paths are
    // exercised by ASCII fixtures and never fire on the clean tree).
    true
}

/// Render a triple subject like the Python `_ox_term_display`/`str(subject)`:
/// NamedNode → its IRI; BlankNode → `_:b`.
fn subject_display(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(n) => n.as_str().to_owned(),
        NamedOrBlankNode::BlankNode(b) => format!("_:{}", b.as_str()),
    }
}

/// Build a GMEOW-namespaced [`NamedNode`](oxigraph::model::NamedNode).
fn ns_node(cfg: &LintConfig, local: &str) -> oxigraph::model::NamedNode {
    oxigraph::model::NamedNode::new_unchecked(format!("{}{}", cfg.namespace, local))
}

/// CamelCase token splitter — a hand-rolled port of `_CAMEL_SPLIT`
/// (`[A-Z]?[a-z0-9]+|[A-Z]+(?![a-z])`), since the `regex` crate has no
/// look-ahead. Mirrors CPython `re.findall` leftmost, alternative-ordered
/// semantics: at each position try alt 1 (`[A-Z]?[a-z0-9]+`), else alt 2 (a
/// greedy uppercase run that gives back its last char when followed by a
/// lowercase, the `(?![a-z])` backtrack). Returns lowercased tokens.
fn camel_tokens(local: &str) -> Vec<String> {
    let chars: Vec<char> = local.chars().collect();
    let is_upper = |c: char| c.is_ascii_uppercase();
    let is_lower_or_digit = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let is_lower = |c: char| c.is_ascii_lowercase();

    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        // Alt 1: [A-Z]?[a-z0-9]+
        let mut j = i;
        if is_upper(chars[j]) {
            j += 1;
        }
        if j < chars.len() && is_lower_or_digit(chars[j]) {
            while j < chars.len() && is_lower_or_digit(chars[j]) {
                j += 1;
            }
            tokens.push(chars[start..j].iter().collect::<String>().to_lowercase());
            i = j;
            continue;
        }
        // Alt 2: [A-Z]+(?![a-z]) — greedy uppercase run, give back last char
        // if it is immediately followed by a lowercase.
        if is_upper(chars[i]) {
            let mut k = i;
            while k < chars.len() && is_upper(chars[k]) {
                k += 1;
            }
            // Backtrack one char while the char after the run is a lowercase.
            if k > i + 1 && k < chars.len() && is_lower(chars[k]) {
                k -= 1;
            }
            // After (possible) backtrack, the match is chars[i..k]; the negative
            // lookahead is satisfied because either k == len, or chars[k] is not
            // a lowercase (a single leftover uppercase before a lowercase still
            // matches: (?![a-z]) only forbids a lowercase right after the run).
            if k > i {
                tokens.push(chars[i..k].iter().collect::<String>().to_lowercase());
                i = k;
                continue;
            }
        }
        // No alternative matched at this position; advance (the regex would skip
        // this char as a non-match boundary, e.g. an underscore or symbol).
        i += 1;
    }
    tokens
}

/// The term-naming lint (mirrors `term_naming_lint`): a selector-privileging
/// local name with no `gmeow:namingNote` justification is an error.
pub fn term_naming_lint(store: &Store, cfg: &LintConfig) -> LintReport {
    let mut report = LintReport::default();
    let naming_note = ns_node(cfg, "namingNote");
    let typed = collect_typed_terms(store, cfg);
    for (term, kind) in &typed {
        let local = term.strip_prefix(&cfg.namespace).unwrap_or(term);
        let tokens: HashSet<String> = camel_tokens(local).into_iter().collect();
        let mut offending: Vec<&String> = cfg
            .selector_tokens
            .iter()
            .filter(|t| tokens.contains(*t))
            .collect();
        if offending.is_empty() {
            continue;
        }
        if has_predicate(store, term, naming_note.as_ref()) {
            continue;
        }
        offending.sort();
        let first = offending[0];
        report.errors.push(format!(
            "{kind} gmeow:{local} carries the selector token '{first}' (Principle 9: co-equal \
             claims have no primary/preferred/default/main); rename it, or justify a \
             value-vocabulary use with gmeow:namingNote"
        ));
    }
    report
}

/// One slice module to ownership-check: the module file's quads-bearing path
/// label and its expected owning-slice IRI.
#[derive(Debug, Clone)]
pub struct ModuleSpec {
    /// The display path of the module (`str(module)` in Python).
    pub module_path: String,
    /// The IRI the module's terms must declare as `rdfs:isDefinedBy`.
    pub expected_slice_iri: String,
}

/// The slice-ownership lint (mirrors `slice_ownership_lint`). Each module is
/// parsed alone; a GMEOW subject whose `rdfs:isDefinedBy` is not exactly the
/// owning slice IRI is an error (#329).
pub fn slice_ownership_lint(modules: &[(ModuleSpec, Store)], cfg: &LintConfig) -> LintReport {
    let mut report = LintReport::default();
    for (spec, store) in modules {
        for quad in store
            .quads_for_pattern(None, Some(rdfs::IS_DEFINED_BY), None, None)
            .flatten()
        {
            let subject = match &quad.subject {
                NamedOrBlankNode::NamedNode(n) => n.as_str().to_owned(),
                NamedOrBlankNode::BlankNode(_) => continue,
            };
            if !is_gmeow_term(&subject, cfg) {
                continue;
            }
            let obj_text = match &quad.object {
                Term::NamedNode(n) => n.as_str().to_owned(),
                Term::BlankNode(b) => format!("_:{}", b.as_str()),
                Term::Literal(l) => l.value().to_owned(),
                #[allow(unreachable_patterns)]
                other => format!("{other}"),
            };
            if obj_text != spec.expected_slice_iri {
                report.errors.push(format!(
                    "{module}: {subject} rdfs:isDefinedBy {obj} — must equal the owning slice \
                     IRI {slice_iri} (#329)",
                    module = spec.module_path,
                    obj = obj_text,
                    slice_iri = spec.expected_slice_iri,
                ));
            }
        }
    }
    report
}

/// The declared-term IRI set (`set(_collect_typed_terms(graph))`) — exposed for
/// `guide_anchor_lint`'s anchor resolution (which keeps its markdown logic in
/// Python).
pub fn declared_terms(store: &Store, cfg: &LintConfig) -> Vec<String> {
    collect_typed_terms(store, cfg).into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::{RdfFormat, RdfParser};

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";
    const ONT: &str = "https://blackcatinformatics.ca/gmeow";

    fn cfg() -> LintConfig {
        LintConfig {
            namespace: NS.to_owned(),
            ontology_iri: ONT.to_owned(),
            selector_tokens: ["primary", "preferred", "default", "main"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            core_slice_iris: HashSet::new(),
            annotation_predicates: [
                "http://www.w3.org/2000/01/rdf-schema#label",
                "http://www.w3.org/2004/02/skos/core#definition",
                "http://www.w3.org/2000/01/rdf-schema#comment",
                "http://purl.org/dc/terms/title",
                "http://purl.org/dc/terms/description",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }

    fn store_from(ttl: &str) -> Store {
        let store = Store::new().unwrap();
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(ttl.as_bytes())
        {
            store.insert(&triple.unwrap()).unwrap();
        }
        store
    }

    const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix ex: <https://example.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n";

    const ROLE: &str = "ex:boxTBox a gmeow:GraphBoxRole .\n";

    #[test]
    fn structural_flags_missing_definition() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Undocumented a owl:Class ;\n\
               rdfs:label \"x\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> ;\n\
               rdfs:subClassOf owl:Thing .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(report.errors.iter().any(|e| e.contains("skos:definition")));
    }

    #[test]
    fn structural_clean_for_well_formed_term() {
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn structural_flags_missing_graph_box_role() {
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("missing gmeow:graphBoxRole")));
    }

    #[test]
    fn structural_exempts_self_description_abox() {
        // A-Box individuals defined by the `self` self-description ontology are
        // project metadata, not vocabulary surface, so the per-term annotation /
        // graphBoxRole contract must not fire on them (#644). The same individual
        // shape would be flagged if it were ordinary vocabulary.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:self#contribution-bii a gmeow:Contribution ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/self> .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(
            !report.errors.iter().any(|e| e.contains("contribution-bii")),
            "self-description A-Box must be exempt: {:?}",
            report.errors
        );
    }

    #[test]
    fn structural_still_flags_vocabulary_individual() {
        // A gmeow individual NOT defined by `self` (ordinary controlled
        // vocabulary) is still held to the contract — the exemption is narrow.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:roleAuthor a owl:NamedIndividual ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/creative-works> .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("roleAuthor") && e.contains("graphBoxRole")),
            "ordinary vocabulary individual must still be linted: {:?}",
            report.errors
        );
    }

    #[test]
    fn structural_rejects_untyped_graph_box_role() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               gmeow:graphBoxRole ex:notARole ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("not a gmeow:GraphBoxRole")));
    }

    #[test]
    fn structural_accepts_mixed_case_private_tag() {
        let store = store_from(&format!(
            "{PREFIXES}\
             <https://example.org/name> gmeow:fullName \"Japanese\"@x-GMEOW-Japanese .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn structural_rejects_external_tag_on_gmeow_predicate() {
        let store = store_from(&format!(
            "{PREFIXES}\
             <https://example.org/name> gmeow:fullName \"Japanese\"@ja .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("external or invalid language tag")));
        // Exact rdflib repr framing.
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("literal rdflib.term.Literal('Japanese', lang='ja')")));
    }

    #[test]
    fn structural_rejects_en_on_gmeow_label() {
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}\
             gmeow:TestTerm a owl:Class ;\n\
               rdfs:label \"Name\"@en ;\n\
               skos:definition \"A test term.\" ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("external language tag 'en'") && e.contains("label")));
    }

    #[test]
    fn structural_accepts_x_gmeow_english_on_label() {
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}\
             gmeow:TestTerm a owl:Class ;\n\
               rdfs:label \"Name\"@x-gmeow-english ;\n\
               skos:definition \"A test term.\"@x-gmeow-english ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint(&store, &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn naming_lint_flags_primary_without_note() {
        let store = store_from(&format!("{PREFIXES}gmeow:PrimaryThing a owl:Class .\n"));
        let report = term_naming_lint(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("selector token 'primary'")));
    }

    #[test]
    fn naming_lint_respects_naming_note() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:scriptRolePrimary a owl:Class ;\n\
               gmeow:namingNote \"value vocabulary\" .\n"
        ));
        let report = term_naming_lint(&store, &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn ownership_flags_foreign_owner() {
        let module = store_from(&format!(
            "{PREFIXES}gmeow:Term rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/other> .\n"
        ));
        let spec = ModuleSpec {
            module_path: "slices/g/mine/module.ttl".to_owned(),
            expected_slice_iri: "https://blackcatinformatics.ca/gmeow/slices/mine".to_owned(),
        };
        let report = slice_ownership_lint(&[(spec, module)], &cfg());
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("must equal the owning slice IRI"));
    }

    #[test]
    fn ownership_passes_matching_owner() {
        let module = store_from(&format!(
            "{PREFIXES}gmeow:Term rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/mine> .\n"
        ));
        let spec = ModuleSpec {
            module_path: "slices/g/mine/module.ttl".to_owned(),
            expected_slice_iri: "https://blackcatinformatics.ca/gmeow/slices/mine".to_owned(),
        };
        let report = slice_ownership_lint(&[(spec, module)], &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn collect_typed_terms_resolves_multityped() {
        // A subject typed as both Class and Individual resolves to class (lower rank).
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Thing a owl:Class , gmeow:SomeIndividualType .\n"
        ));
        let terms = collect_typed_terms(&store, &cfg());
        assert_eq!(
            terms.get("https://blackcatinformatics.ca/gmeow/Thing"),
            Some(&"class".to_owned())
        );
    }

    #[test]
    fn camel_tokens_matches_python() {
        let cases: &[(&str, &[&str])] = &[
            ("PrimaryThing", &["primary", "thing"]),
            ("scriptRolePrimary", &["script", "role", "primary"]),
            ("sourceTierPrimary", &["source", "tier", "primary"]),
            ("HTTPSConnection", &["https", "connection"]),
            ("IRI", &["iri"]),
            ("primary", &["primary"]),
            ("Primary", &["primary"]),
            ("XMLHttpRequest", &["xml", "http", "request"]),
            ("fooBARBaz", &["foo", "bar", "baz"]),
            ("ABCdef", &["ab", "cdef"]),
            ("a1B2c3", &["a1", "b2c3"]),
            ("mainDefault", &["main", "default"]),
            ("URLPreferred", &["url", "preferred"]),
        ];
        for (input, expected) in cases {
            let got = camel_tokens(input);
            let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
            assert_eq!(&got, &want, "input {input}");
        }
    }

    #[test]
    fn py_str_repr_quote_choice() {
        assert_eq!(py_str_repr("Japanese"), "'Japanese'");
        assert_eq!(py_str_repr("it's"), "\"it's\"");
        assert_eq!(py_str_repr("has \"q\""), "'has \"q\"'");
        assert_eq!(py_str_repr("back\\slash"), "'back\\\\slash'");
        assert_eq!(py_str_repr("new\nline"), "'new\\nline'");
        assert_eq!(py_str_repr("tab\there"), "'tab\\there'");
        assert_eq!(py_str_repr("uniécode"), "'uniécode'");
    }
}
