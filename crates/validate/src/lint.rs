// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free engine for the structural and naming lints.
//!
//! These two lints run over a native [`RdfDataset`] built from the merged ontology
//! sources. The stable language-tag diagnostics (Check 1 / Check 2) use
//! [`py_str_repr`] to preserve the established quoted-literal output framing on
//! the rare violation paths.
//!
//! Engine-core separation: this module is pure Rust with no binding surface.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use regex::Regex;

use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, Grade, Severity, StageId, Standpoint, register_code,
};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef};

use gmeow_math::Rational;

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
    /// source of truth; Python reads the set from here, it is no longer
    /// pushed in from `language_tags`.
    pub annotation_predicates: HashSet<String>,
}

/// The canonical annotation predicates whose literals the Check-2 language-tag
/// policy polices — `rdfs:label`, `skos:definition`, `rdfs:comment`, `dcterms:title`,
/// `dcterms:description`. This crate owns the registry; the Python
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

/// The registered finding codes the two lints emit. Each distinct lint CHECK owns
/// its own code so genuinely distinct findings never hash-cons-merge on the ledger
/// (the fingerprint keys on `(code, category, location, focus)`, never the message):
/// two findings from one check on different terms are kept apart by their `focus`,
/// and two findings from the same check on the same term but with different messages
/// (the multi-message checks — the graphBoxRole quartet, the two language-tag passes,
/// the three RenderingAsIdentity arms, the dimensional-inhomogeneity arms) are kept
/// apart by their code.
mod codes {
    pub const MISSING_LABEL: &str = "validate.lint.missing-label";
    pub const MISSING_DEFINITION: &str = "validate.lint.missing-definition";
    pub const MISSING_IS_DEFINED_BY: &str = "validate.lint.missing-is-defined-by";
    pub const NON_IRI_GRAPH_BOX_ROLE: &str = "validate.lint.non-iri-graph-box-role";
    pub const GRAPH_BOX_ROLE_NOT_REGISTERED: &str = "validate.lint.graph-box-role-not-registered";
    pub const MISSING_GRAPH_BOX_ROLE: &str = "validate.lint.missing-graph-box-role";
    pub const MISSING_USE_WHEN: &str = "validate.lint.missing-use-when";
    pub const MISSING_HOW_TO_USE: &str = "validate.lint.missing-how-to-use";
    pub const HOW_TO_USE_WITHOUT_EXAMPLE: &str = "validate.lint.how-to-use-without-example";
    pub const DANGLING_SUBTERM_TARGET: &str = "validate.lint.dangling-subterm-target";
    pub const SYSTEMATIC_DOCUMENTATION_GAP: &str = "validate.lint.systematic-documentation-gap";
    pub const GMEOW_PREDICATE_EXTERNAL_LANG_TAG: &str =
        "validate.lint.gmeow-predicate-external-lang-tag";
    pub const ANNOTATION_EXTERNAL_LANG_TAG: &str = "validate.lint.annotation-external-lang-tag";
    pub const LANG_UNDECLARED_LOWERING_STAGE: &str = "validate.lint.lang.undeclared-lowering-stage";
    pub const LANG_SILENT_DISAMBIGUATION: &str = "validate.lint.lang.silent-disambiguation";
    pub const LANG_ONE_WAY_BRIDGE: &str = "validate.lint.lang.one-way-bridge";
    pub const LANG_SILENT_INGEST_DROP: &str = "validate.lint.lang.silent-ingest-drop";
    pub const LANG_INLINE_BLOB_PAYLOAD: &str = "validate.lint.lang.inline-blob-payload";
    pub const LANG_NON_CONTIGUOUS_SLOTS: &str = "validate.lint.lang.non-contiguous-slots";
    pub const LANG_UNATTRIBUTED_ENGINE_CLAIM: &str = "validate.lint.lang.unattributed-engine-claim";
    pub const LANG_SILENT_PROMOTION: &str = "validate.lint.lang.silent-promotion";
    pub const LANG_SURFACE_LEAK: &str = "validate.lint.lang.surface-leak-in-content-key";
    pub const LANG_RENDERING_AS_IDENTITY_SELF: &str =
        "validate.lint.lang.rendering-as-identity-self";
    pub const LANG_RENDERING_AS_IDENTITY_SAMEAS: &str =
        "validate.lint.lang.rendering-as-identity-sameas";
    pub const LANG_RENDERING_AS_IDENTITY_FORM: &str =
        "validate.lint.lang.rendering-as-identity-form";
    pub const LANG_MISSING_PRESERVATION_KIND: &str = "validate.lint.lang.missing-preservation-kind";
    pub const LANG_UNDECLARED_UNSUPPORTED_CONSTRUCT: &str =
        "validate.lint.lang.undeclared-unsupported-construct";
    pub const LANG_UNRECORDED_EPISTEMIC_LOSS: &str = "validate.lint.lang.unrecorded-epistemic-loss";
    pub const LANG_PROJECTION_SILENT_DISAMBIGUATION: &str =
        "validate.lint.lang.projection-silent-disambiguation";
    pub const LANG_EXACT_PRESERVATION_VIOLATED: &str =
        "validate.lint.lang.exact-preservation-violated";
    pub const MATH_MALFORMED_DIMENSION_VECTOR: &str =
        "validate.lint.math.malformed-dimension-vector";
    pub const MATH_MALFORMED_DIMENSION_ZERO_DENOMINATOR: &str =
        "validate.lint.math.malformed-dimension-zero-denominator";
    pub const MATH_INHOMOGENEITY_UNDIMENSIONED: &str =
        "validate.lint.math.dimensional-inhomogeneity-undimensioned";
    pub const MATH_INHOMOGENEITY_DIFFERING: &str =
        "validate.lint.math.dimensional-inhomogeneity-differing";
    pub const MATH_INTEGRAL_UNDIMENSIONED_PART: &str =
        "validate.lint.math.integral-undimensioned-part";
    pub const MATH_INTEGRAL_COMPOSITION_MISMATCH: &str =
        "validate.lint.math.integral-composition-mismatch";
    pub const MATH_UNLIFTABLE_INGEST: &str = "validate.lint.math.unliftable-ingest";
    pub const MATH_PROBABILITY_OUT_OF_BOUNDS: &str = "validate.lint.math.probability-out-of-bounds";
    pub const MATH_PROBABILITY_PARAMETER_CONSTRAINT: &str =
        "validate.lint.math.probability-distribution-parameter-constraint";
    pub const MATH_PROBABILITY_MISSING_MODEL_LOWERING: &str =
        "validate.lint.math.probability-missing-model-lowering";
    pub const MATH_PROBABILITY_INCOMPLETE_DEPENDENCY_MODEL: &str =
        "validate.lint.math.probability-incomplete-dependency-model";
    pub const MATH_PROBABILITY_EXACT_PRESERVATION_VIOLATED: &str =
        "validate.lint.math.probability-exact-preservation-violated";
    pub const MATH_PROJECTION_CONFIDENCE_AS_PROBABILITY: &str =
        "validate.lint.math.projection-confidence-as-probability";
    pub const MATH_PROJECTION_DROPPED_PARAMETERIZATION: &str =
        "validate.lint.math.projection-dropped-parameterization";
    pub const NAMING_SELECTOR_TOKEN: &str = "validate.lint.naming.selector-token";
}

/// A stateless view over a [`DiagLedger`] of graded lint findings.
///
/// The report holds exactly one hash-consed ledger and NO independent string store:
/// [`errors`](LintReport::errors) / [`warnings`](LintReport::warnings) project the
/// finding messages back out of the ledger in its deterministic `(stage, fingerprint)`
/// order, filtered by severity. Every finding is a graded [`Diag`] — a structural-
/// discipline error (Severity::Error, ModelingDisciplineViolation, Binding) or a
/// policy warning (Severity::Warning, PolicyWarning, Perspectival) — interned under
/// the stable `validate.lint` stage.
#[derive(Debug, Default, Clone)]
pub struct LintReport {
    ledger: DiagLedger,
}

/// The stage every lint finding is attached under.
fn lint_stage() -> StageId {
    StageId::new("validate.lint")
}

impl LintReport {
    /// Intern a structural-discipline error finding (severity Error, a blocking
    /// ModelingDisciplineViolation, Binding standpoint), keyed for hash-consing by
    /// its registered `code` and the `focus` node the message is about.
    fn push_error(&mut self, code: &str, focus: impl Into<String>, message: String) {
        let diag = Diag::new(
            register_code(code),
            Grade::new(
                Severity::Error,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Binding,
            ),
            message,
        )
        .with_focus(focus);
        self.ledger.attach(diag, lint_stage());
    }

    /// Intern a policy warning finding (severity Warning, PolicyWarning, Perspectival
    /// standpoint) — surfaced but never gate-fatal — keyed by `code` and `focus`.
    fn push_warning(&mut self, code: &str, focus: impl Into<String>, message: String) {
        let diag = Diag::new(
            register_code(code),
            Grade::new(
                Severity::Warning,
                FindingCategory::PolicyWarning,
                Standpoint::Perspectival,
            ),
            message,
        )
        .with_focus(focus);
        self.ledger.attach(diag, lint_stage());
    }

    /// The hash-consed ledger of this report's graded lint diagnostics, so the
    /// run-level orchestration can fold it into the single unified run ledger via
    /// [`DiagLedger::union`] — carrying the rich `validate.lint.*` diags (code,
    /// category, standpoint, focus) rather than re-stringifying them.
    #[must_use]
    pub fn ledger(&self) -> &DiagLedger {
        &self.ledger
    }

    /// The Error-severity finding messages, in the ledger's deterministic order.
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        self.messages(Severity::Error)
    }

    /// The Warning-severity finding messages, in the ledger's deterministic order.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        self.messages(Severity::Warning)
    }

    fn messages(&self, severity: Severity) -> Vec<String> {
        self.ledger
            .emit_sorted()
            .into_iter()
            .filter(|node| node.grade.severity == severity)
            .flat_map(|node| node.observations.iter().map(|o| o.message.clone()))
            .collect()
    }
}

/// Return whether an IRI is the GMEOW root or lives in its namespace
/// (mirrors `_is_gmeow_term`).
fn is_gmeow_term(iri: &str, cfg: &LintConfig) -> bool {
    iri.starts_with(&cfg.namespace) || iri == cfg.ontology_iri
}

/// Return the primary structural kind of a GMEOW term from its `rdf:type` set
/// (mirrors `_term_kind`).
fn term_kind(types: &HashSet<String>) -> &'static str {
    if types.contains(owl::ONTOLOGY) {
        return "ontology";
    }
    if types.contains(owl::CLASS) {
        return "class";
    }
    if types.contains(owl::ANNOTATION_PROPERTY) {
        return "annotation property";
    }
    if types.contains(owl::OBJECT_PROPERTY) || types.contains(owl::DATATYPE_PROPERTY) {
        return "property";
    }
    if types.contains(rdfs::DATATYPE) {
        return "datatype";
    }
    "individual"
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

// ─────────────────────────────────────────────────────────────────────────────
// The structural / naming lints over a native (`purrdf::RdfDataset`) graph.
//
// Every check, error/warning TEXT, severity, and emission ORDER is byte-identical to
// the legacy oxigraph `Store` implementation it replaced.
//
// Graph handling: the legacy pipeline built its store with
// `store_from_dataset(.., FlattenToDefaultGraph)`, so EVERY quad — including those
// authored in a named graph (e.g. the release attestation N-Quads) — was visible in
// the single default graph. These functions match with [`GraphMatch::Any`] so they
// read across all graphs, exactly as the flattened store did. For a plain-Turtle
// input (single default graph) `Any` and `Default` coincide.

/// Resolve an IRI value to its dataset-local [`purrdf::TermId`], if interned.
fn ds_iri_id(ds: &RdfDataset, iri: &str) -> Option<purrdf::TermId> {
    ds.term_id_by_value(&purrdf::TermValue::iri(iri))
}

/// All `rdf:type` object IRIs of `subject_iri`, as a set (native twin of [`rdf_types`]).
fn ds_rdf_types(ds: &RdfDataset, subject_iri: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let (Some(s_id), Some(type_id)) = (ds_iri_id(ds, subject_iri), ds_iri_id(ds, rdf::TYPE)) else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(s_id), Some(type_id), None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = ds.resolve(q.o) {
            out.insert(iri.to_owned());
        }
    }
    out
}

/// Map every GMEOW-namespaced typed term to its primary kind. Keyed by term IRI;
/// `BTreeMap` iterates sorted.
#[must_use]
pub fn collect_typed_terms_dataset(ds: &RdfDataset, cfg: &LintConfig) -> BTreeMap<String, String> {
    let mut terms: BTreeMap<String, String> = BTreeMap::new();
    let Some(type_id) = ds_iri_id(ds, rdf::TYPE) else {
        return terms;
    };
    let typed_queries = [
        owl::ONTOLOGY,
        owl::CLASS,
        owl::OBJECT_PROPERTY,
        owl::DATATYPE_PROPERTY,
        owl::ANNOTATION_PROPERTY,
        rdfs::DATATYPE,
    ];
    for rdf_type in typed_queries {
        let Some(t_id) = ds_iri_id(ds, rdf_type) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(type_id), Some(t_id), GraphMatch::Any) {
            let TermRef::Iri(subject) = ds.resolve(q.s) else {
                continue;
            };
            if !is_gmeow_term(subject, cfg) {
                continue;
            }
            let kind = term_kind(&ds_rdf_types(ds, subject));
            let subject = subject.to_owned();
            match terms.get(&subject) {
                Some(current) if kind_rank(kind) >= kind_rank(current) => {}
                _ => {
                    terms.insert(subject, kind.to_owned());
                }
            }
        }
    }
    // Any remaining GMEOW subjects with an explicit rdf:type → individual.
    for q in ds.quads_for_pattern(None, Some(type_id), None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = ds.resolve(q.s)
            && is_gmeow_term(iri, cfg)
            && !terms.contains_key(iri)
        {
            terms.insert(iri.to_owned(), "individual".to_owned());
        }
    }
    terms
}

/// Whether `(subject_iri, predicate_iri, *)` has at least one triple (native twin of
/// [`has_predicate`]).
fn ds_has_predicate(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> bool {
    let (Some(s_id), Some(p_id)) = (ds_iri_id(ds, subject_iri), ds_iri_id(ds, predicate_iri))
    else {
        return false;
    };
    ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Any)
        .next()
        .is_some()
}

/// Object IRIs of `(subject_iri, predicate_iri, ?)` (named-node objects only).
fn ds_object_iris(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let (Some(s_id), Some(p_id)) = (ds_iri_id(ds, subject_iri), ds_iri_id(ds, predicate_iri))
    else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = ds.resolve(q.o) {
            out.insert(iri.to_owned());
        }
    }
    out
}

/// Whether `(subject_iri, rdf:type, type_iri)` exists (native twin of [`has_type`]).
fn ds_has_type(ds: &RdfDataset, subject_iri: &str, type_iri: &str) -> bool {
    let (Some(s_id), Some(type_id), Some(t_id)) = (
        ds_iri_id(ds, subject_iri),
        ds_iri_id(ds, rdf::TYPE),
        ds_iri_id(ds, type_iri),
    ) else {
        return false;
    };
    ds.quads_for_pattern(Some(s_id), Some(type_id), Some(t_id), GraphMatch::Any)
        .next()
        .is_some()
}

/// Run the structural lint over a frozen [`RdfDataset`].
///
/// Byte-identical errors/warnings to the `Store` version; reads across all graphs
/// ([`GraphMatch::Any`]) so a named-graph input is linted exactly as the old
/// flattened store was.
pub fn structural_lint_dataset(ds: &RdfDataset, cfg: &LintConfig) -> LintReport {
    let mut report = LintReport::default();
    let typed = collect_typed_terms_dataset(ds, cfg);
    let graph_box_role = format!("{}graphBoxRole", cfg.namespace);
    let graph_box_role_class = format!("{}GraphBoxRole", cfg.namespace);

    // Precompute the self-description A-Box: subjects `rdfs:isDefinedBy <ns>self`.
    let self_ontology = format!("{}self", cfg.namespace);
    let mut self_defined: HashSet<String> = HashSet::new();
    if let (Some(p_id), Some(self_id)) = (
        ds_iri_id(ds, rdfs::IS_DEFINED_BY),
        ds_iri_id(ds, &self_ontology),
    ) {
        for q in ds.quads_for_pattern(None, Some(p_id), Some(self_id), GraphMatch::Any) {
            if let TermRef::Iri(subject) = ds.resolve(q.s) {
                self_defined.insert(subject.to_owned());
            }
        }
    }

    // Precompute slice/graph provenance sets in one scan of `rdfs:isDefinedBy`.
    let slice_prefix = format!("{}slices/", cfg.namespace);
    let graph_prefix = format!("{}graph/", cfg.namespace);
    let mut slice_defined: HashSet<String> = HashSet::new();
    let mut graph_defined: HashSet<String> = HashSet::new();
    if let Some(p_id) = ds_iri_id(ds, rdfs::IS_DEFINED_BY) {
        for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
            let TermRef::Iri(object) = ds.resolve(q.o) else {
                continue;
            };
            let TermRef::Iri(subject) = ds.resolve(q.s) else {
                continue;
            };
            if object.starts_with(slice_prefix.as_str()) {
                slice_defined.insert(subject.to_owned());
            } else if object.starts_with(graph_prefix.as_str()) {
                graph_defined.insert(subject.to_owned());
            }
        }
    }

    // Subjects self-declaring `gmeow:graphBoxRole gmeow:boxABox`.
    let abox_role = format!("{}boxABox", cfg.namespace);
    let mut abox_declared: HashSet<String> = HashSet::new();
    if let (Some(p_id), Some(role_id)) = (ds_iri_id(ds, &graph_box_role), ds_iri_id(ds, &abox_role))
    {
        for q in ds.quads_for_pattern(None, Some(p_id), Some(role_id), GraphMatch::Any) {
            if let TermRef::Iri(subject) = ds.resolve(q.s) {
                abox_declared.insert(subject.to_owned());
            }
        }
    }

    // 1. Per-term required annotations (BTreeMap iterates sorted by IRI).
    for (term, kind) in &typed {
        if term == &self_ontology || self_defined.contains(term) {
            continue;
        }
        let assertional = kind == "individual"
            && !slice_defined.contains(term)
            && graph_defined.contains(term)
            && abox_declared.contains(term);
        if !ds_has_predicate(ds, term, rdfs::LABEL) {
            report.push_error(
                codes::MISSING_LABEL,
                term.clone(),
                format!("{kind} {term} is missing rdfs:label"),
            );
        }
        if !assertional && !ds_has_predicate(ds, term, skos::DEFINITION) {
            report.push_error(
                codes::MISSING_DEFINITION,
                term.clone(),
                format!("{kind} {term} is missing skos:definition"),
            );
        }
        if !ds_has_predicate(ds, term, rdfs::IS_DEFINED_BY) {
            report.push_error(
                codes::MISSING_IS_DEFINED_BY,
                term.clone(),
                format!("{kind} {term} is missing rdfs:isDefinedBy"),
            );
        }
        let mut has_role = false;
        if let (Some(s_id), Some(p_id)) = (ds_iri_id(ds, term), ds_iri_id(ds, &graph_box_role)) {
            for q in ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Any) {
                has_role = true;
                let role = match ds.resolve(q.o) {
                    TermRef::Iri(role) => role.to_owned(),
                    other => {
                        let disp = ds_object_display(other);
                        report.push_error(
                            codes::NON_IRI_GRAPH_BOX_ROLE,
                            format!("{term}\t{disp}"),
                            format!("{kind} {term} has non-IRI gmeow:graphBoxRole value {disp}"),
                        );
                        continue;
                    }
                };
                if !ds_has_type(ds, &role, &graph_box_role_class) {
                    report.push_error(
                        codes::GRAPH_BOX_ROLE_NOT_REGISTERED,
                        format!("{term}\t{role}"),
                        format!(
                            "{kind} {term} has gmeow:graphBoxRole value {role} that is not a gmeow:GraphBoxRole",
                        ),
                    );
                }
            }
        }
        if !has_role {
            report.push_error(
                codes::MISSING_GRAPH_BOX_ROLE,
                term.clone(),
                format!("{kind} {term} is missing gmeow:graphBoxRole"),
            );
        }
    }

    let declared: HashSet<&String> = typed.keys().collect();

    // 2. Tier-1 depth warnings.
    let use_when = format!("{}useWhen", cfg.namespace);
    let how_to_use = format!("{}howToUse", cfg.namespace);
    for (term, kind) in &typed {
        if kind != "class" && kind != "property" {
            continue;
        }
        let defined_by = ds_object_iris(ds, term, rdfs::IS_DEFINED_BY);
        if !defined_by.iter().any(|d| cfg.core_slice_iris.contains(d)) {
            continue;
        }
        if !ds_has_predicate(ds, term, &use_when) {
            report.push_warning(
                codes::MISSING_USE_WHEN,
                term.clone(),
                format!("{kind} {term} is missing gmeow:useWhen (Tier-1 depth)"),
            );
        }
        let has_how_to_use = ds_has_predicate(ds, term, &how_to_use);
        if !has_how_to_use {
            report.push_warning(
                codes::MISSING_HOW_TO_USE,
                term.clone(),
                format!("{kind} {term} is missing gmeow:howToUse (Tier-1 depth)"),
            );
        } else if !ds_has_predicate(ds, term, skos::EXAMPLE) {
            report.push_warning(
                codes::HOW_TO_USE_WITHOUT_EXAMPLE,
                term.clone(),
                format!("{kind} {term} has gmeow:howToUse but no skos:example (Tier-1 depth)"),
            );
        }
    }

    // 3. Dangling GMEOW subclass/subproperty targets.
    for predicate in [rdfs::SUB_CLASS_OF, rdfs::SUB_PROPERTY_OF] {
        let Some(p_id) = ds_iri_id(ds, predicate) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
            if let TermRef::Iri(target) = ds.resolve(q.o)
                && is_gmeow_term(target, cfg)
                && !declared.contains(&target.to_owned())
            {
                report.push_error(
                    codes::DANGLING_SUBTERM_TARGET,
                    format!("{predicate}\t{target}"),
                    format!(
                        "dangling {pred} target (undeclared GMEOW term): {target}",
                        pred = predicate,
                    ),
                );
            }
        }
    }

    // 4. Comprehensiveness heuristic.
    let mut parent_to_children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(p_id) = ds_iri_id(ds, rdfs::SUB_CLASS_OF) {
        for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
            let TermRef::Iri(child) = ds.resolve(q.s) else {
                continue;
            };
            let TermRef::Iri(parent) = ds.resolve(q.o) else {
                continue;
            };
            if is_gmeow_term(child, cfg) && is_gmeow_term(parent, cfg) {
                parent_to_children
                    .entry(parent.to_owned())
                    .or_default()
                    .push(child.to_owned());
            }
        }
    }
    for (parent, children) in &parent_to_children {
        if children.len() < 3 {
            continue;
        }
        let missing = children
            .iter()
            .filter(|c| !ds_has_predicate(ds, c, skos::DEFINITION))
            .count();
        if missing >= 3 {
            report.push_warning(
                codes::SYSTEMATIC_DOCUMENTATION_GAP,
                parent.clone(),
                format!(
                    "class {parent} has {missing} of {total} direct subclasses missing \
                     skos:definition (systematic documentation gap)",
                    total = children.len(),
                ),
            );
        }
    }

    // 5. Language-tag discipline over ALL triples.
    let x_gmeow = Regex::new(r"(?i)^x-gmeow-[a-z0-9\-]+$").expect("static regex");
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        let TermRef::Iri(predicate_iri) = ds.resolve(q.p) else {
            continue;
        };
        let object = ds.resolve(q.o);
        let TermRef::Literal {
            lexical, language, ..
        } = object
        else {
            continue;
        };

        // Check 1: literal on a GMEOW-namespace predicate.
        if predicate_iri.starts_with(&cfg.namespace)
            && let Some(lang) = language
            && !x_gmeow.is_match(lang)
        {
            let subject = ds_subject_display(ds.resolve(q.s));
            report.push_error(
                codes::GMEOW_PREDICATE_EXTERNAL_LANG_TAG,
                format!("{subject}\t{predicate_iri}\t{lang}\t{lexical}"),
                format!(
                    "literal {lit_repr} (on subject {subject}, predicate {predicate_iri}) \
                         carries external or invalid language tag '{lang}'; GMEOW internal \
                         data must use the private-use 'x-gmeow-' prefix.",
                    lit_repr = lang_literal_repr(lexical, lang),
                ),
            );
        }

        // Check 2: standard annotation predicate on a GMEOW-authored subject.
        if let TermRef::Iri(subj) = ds.resolve(q.s)
            && is_gmeow_term(subj, cfg)
            && let Some(msg) =
                ds_check_annotation_literal(subj, predicate_iri, lexical, language, cfg, &x_gmeow)
        {
            report.push_error(
                codes::ANNOTATION_EXTERNAL_LANG_TAG,
                format!(
                    "{subj}\t{predicate_iri}\t{lang}\t{lexical}",
                    lang = language.unwrap_or_default(),
                ),
                msg,
            );
        }
    }

    // lang: meaning-stratum native gates (charter primary gates): compositional-
    // lowering preservation, co-resident-reading non-collapse, and the whole-bundle
    // one-way lang:->logic: bridge acyclicity.
    check_lang_meaning_invariants(ds, cfg, &mut report);

    // lang: form-stratum native gates (charter primary gates): a document-scale
    // surface holds its bytes by reference (never inline payload), and a composed
    // form's slot indexes are zero-based and contiguous (enforced unconditionally).
    check_lang_form_invariants(ds, &mut report);

    // lang: ingestion-stratum native gates (charter primary gates): the external-
    // engine handoff — engine output enters as vantage-held readings (never
    // unattributed structure), promotion from an engine reading to a slice
    // assertion is an explicit provenance-carrying act, and an ingested surface is
    // never left in analysis limbo (silently dropped content).
    check_lang_ingestion_invariants(ds, cfg, &mut report);

    // lang: translation-stratum native gates (charter primary gates): the crossing
    // layer keeps content identity structural (never keyed on surface material) and
    // a rendering names its content without ever standing in for that content's
    // identity.
    check_lang_translation_invariants(ds, cfg, &mut report);

    // lang: projection-stratum native gates (charter primary gates): the lossy-
    // lowering contract over the projection corpus — every emission declares its
    // preservation kind, a lossy emission enumerates the constructs it drops, a
    // form-view emission enumerates the epistemic strata it flattens, a per-reading
    // emission emits one row per co-resident reading (never a silent winner), and a
    // declared-exact emission whose measured round-trip is refuted is caught.
    check_lang_projection_invariants(ds, cfg, &mut report);

    // math: measure-and-dimension reasoned gate — dimensional homogeneity computed
    // from the exact-rational (ℚ⁷) exponent vectors, not asserted data.
    check_math_dimension_invariants(ds, &mut report);

    // math: ingestion-bridge gate — a bridge run (the mnemomorphic put leg of a
    // logic:Correspondence) lifts fully or hard-fails; a run retaining a source but
    // producing no structured math: codomain has silently dropped its content.
    check_math_ingest_invariants(ds, &mut report);

    // math: probability-layer reasoned gate — the closed-unit-interval bound, the
    // role-carried positivity/dimension constraints on distribution parameters, the
    // mandatory logic: lowering of a referenced probability model, the structural
    // completeness of a dependency model, and the exact-preservation↔mass-sums-to-one
    // overclaim on a joint probability table. Each is computed from the exact-rational
    // carrier, not asserted data, and holds bundle-wide (`GraphMatch::Any`).
    check_math_probability_invariants(ds, &mut report);

    // math: projection-side reasoned gate — the two join-requiring native checks over
    // math:ProjectionRecord loss-ledger carriers: a projection converting a source
    // confidence into a math:ProbabilityValue without a declared mapping, and a lossy
    // projection dropping a source math:Distribution's parameterization without
    // enumerating it in logic:unsupportedConstruct. Purely native (no SHACL target
    // shape), exactly like the four lang: native projection gates.
    check_math_projection_invariants(ds, &mut report);

    report
}

/// Namespace roots for the `lang:`/`logic:` meaning-stratum invariants.
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The document-scale threshold, in bytes, for the `lang:InlineBlobPayload` gate: a
/// `lang:SurfaceForm` whose inline `lang:surfaceText` exceeds this holds document-scale
/// payload inline instead of by reference (`lang:surfaceBlob`). This MUST equal the
/// pipeline's `DOCUMENT_SCALE_BYTES` (`crates/pipeline/src/stages/lang_form.rs`, which
/// mints the `lang:surfaceBlob` handle once a surface crosses it); the two are kept in
/// sync by hand — one hard-coded, documented constant, never a tunable knob.
const DOCUMENT_SCALE_BYTES: usize = 4096;

fn lang_iri(term: &str) -> String {
    format!("{LANG_NS}{term}")
}

fn logic_iri(term: &str) -> String {
    format!("{LOGIC_NS}{term}")
}

/// Subjects carrying an explicit `rdf:type` of `type_iri`.
fn ds_subjects_of_type(ds: &RdfDataset, type_iri: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(type_id), Some(t_id)) = (ds_iri_id(ds, rdf::TYPE), ds_iri_id(ds, type_iri)) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(type_id), Some(t_id), GraphMatch::Any) {
        if let TermRef::Iri(s) = ds.resolve(q.s) {
            out.push(s.to_owned());
        }
    }
    // `GraphMatch::Any` visits every graph, so a typing triple repeated across the
    // default graph and named graphs (or across imported fixtures) yields the same
    // subject more than once. Collapse them so downstream gates lint — and report —
    // each subject exactly once.
    out.sort();
    out.dedup();
    out
}

/// The `lang:` meaning-stratum invariants the charter designates as native
/// Rust-validator primary gates (realized here rather than in SHACL), plus the
/// whole-bundle one-way-bridge acyclicity. Runs over the merged dataset, so the
/// invariants hold bundle-wide, not merely per fixture.
fn check_lang_meaning_invariants(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    check_undeclared_lowering(ds, report);
    check_silent_disambiguation(ds, cfg, report);
    check_one_way_bridge(ds, report);
}

/// `lang:UndeclaredLoweringStage` — every `lang:Denotation` whose kind bridges
/// into `logic:` declares a `logic:preservationKind`. Being a lowering is derived
/// from the kind, never an optional flag, so the gate cannot fail open.
fn check_undeclared_lowering(ds: &RdfDataset, report: &mut LintReport) {
    let bridge_kinds = [
        lang_iri("denotesLogicFormula"),
        lang_iri("denotesLogicTerm"),
        lang_iri("denotesLogicType"),
        lang_iri("denotesQuery"),
    ];
    let denotation_kind = lang_iri("denotationKind");
    let preservation_kind = logic_iri("preservationKind");
    for subj in ds_subjects_of_type(ds, &lang_iri("Denotation")) {
        let kinds = ds_object_iris(ds, &subj, &denotation_kind);
        let bridges = kinds.iter().any(|k| bridge_kinds.iter().any(|b| b == k));
        if bridges && !ds_has_predicate(ds, &subj, &preservation_kind) {
            report.push_error(
                codes::LANG_UNDECLARED_LOWERING_STAGE,
                subj.clone(),
                format!(
                    "lang:UndeclaredLoweringStage: denotation {subj} bridges into logic: \
                     (lang:denotationKind) but declares no logic:preservationKind"
                ),
            );
        }
    }
}

/// `lang:SilentDisambiguation` — an interpretation act that resolves to a single
/// reading among two or more co-resident readings must be backed by a vantage-held
/// `gmeow:Observation` (through `lang:aboutReading`, with `gmeow:vantage`);
/// otherwise it has silently collapsed the ambiguity.
fn check_silent_disambiguation(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    let produced = lang_iri("producedReading");
    let resolved = lang_iri("resolvedReading");
    let about_reading = lang_iri("aboutReading");
    let vantage = format!("{}vantage", cfg.namespace);
    for act in ds_subjects_of_type(ds, &lang_iri("InterpretationAct")) {
        let readings = ds_object_iris(ds, &act, &produced);
        if readings.len() < 2 {
            continue;
        }
        for chosen in ds_object_iris(ds, &act, &resolved) {
            if !reading_claim_is_grounded(ds, &about_reading, &chosen, &vantage) {
                report.push_error(
                    codes::LANG_SILENT_DISAMBIGUATION,
                    format!("{act}\t{chosen}"),
                    format!(
                        "lang:SilentDisambiguation: interpretation act {act} resolves to reading \
                         {chosen} among {} co-resident readings with no vantage-held observation \
                         grounding the choice",
                        readings.len()
                    ),
                );
            }
        }
    }
}

/// Whether some subject names `chosen` through `lang:aboutReading` and carries a
/// `gmeow:vantage` — a grounded reading-correctness claim.
fn reading_claim_is_grounded(
    ds: &RdfDataset,
    about_reading: &str,
    chosen: &str,
    vantage: &str,
) -> bool {
    let (Some(p_id), Some(o_id)) = (ds_iri_id(ds, about_reading), ds_iri_id(ds, chosen)) else {
        return false;
    };
    for q in ds.quads_for_pattern(None, Some(p_id), Some(o_id), GraphMatch::Any) {
        if let TermRef::Iri(obs) = ds.resolve(q.s)
            && ds_has_predicate(ds, obs, vantage)
        {
            return true;
        }
    }
    false
}

/// One-way bridge acyclicity (Principle 19): no `logic:`-namespaced subject carries
/// a `lang:`-namespaced predicate. The bridge runs `lang:` -> `logic:` through
/// `lang:denotationTarget` and never reverses.
fn check_one_way_bridge(ds: &RdfDataset, report: &mut LintReport) {
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        let (TermRef::Iri(s), TermRef::Iri(p)) = (ds.resolve(q.s), ds.resolve(q.p)) else {
            continue;
        };
        if s.starts_with(LOGIC_NS) && p.starts_with(LANG_NS) {
            report.push_error(
                codes::LANG_ONE_WAY_BRIDGE,
                format!("{s}\t{p}"),
                format!(
                    "lang: one-way bridge violated: logic: subject {s} carries lang: predicate {p} \
                     (Principle 19: the lang:->logic: bridge never reverses)"
                ),
            );
        }
    }
}

/// The `lang:` ingestion-stratum invariants the charter designates as native
/// Rust-validator gates for the external-NLP-engine handoff (realized here rather
/// than in SHACL, since the engine seam is a Rust seam). Runs over the merged
/// dataset, so the invariants hold bundle-wide, not merely per fixture.
fn check_lang_ingestion_invariants(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    check_unattributed_engine_claim(ds, cfg, report);
    check_silent_promotion(ds, cfg, report);
    check_silent_ingest_drop(ds, report);
}

/// `lang:SilentIngestDrop` — an ingester lifts fully or hard-fails; it never silently
/// drops material. The bridges enforce this at the seam (a lift that cannot represent a
/// construct raises a typed `IngestDiagnostic` carrying this class rather than emitting a
/// plausible-but-wrong structure). At the dataset level the honest complement is a
/// surface left in analysis limbo: a `lang:SurfaceForm` that neither `lang:realizes` an
/// analyzed `lang:Form` NOR is typed `lang:UnanalyzedProse` has entered the graph with its
/// analysis silently dropped — neither lifted nor explicitly marked unanalyzed. A surface
/// is analyzed or explicitly unanalyzed, never silently either.
fn check_silent_ingest_drop(ds: &RdfDataset, report: &mut LintReport) {
    let realizes = lang_iri("realizes");
    let unanalyzed = lang_iri("UnanalyzedProse");
    for surface in ds_subjects_of_type(ds, &lang_iri("SurfaceForm")) {
        let realizes_a_form = ds_has_predicate(ds, &surface, &realizes);
        let is_unanalyzed = ds_has_type(ds, &surface, &unanalyzed);
        if !realizes_a_form && !is_unanalyzed {
            report.push_error(
                codes::LANG_SILENT_INGEST_DROP,
                surface.clone(),
                format!(
                    "lang:SilentIngestDrop: surface {surface} neither lang:realizes an analyzed \
                     lang:Form nor is typed lang:UnanalyzedProse; an ingested surface left in \
                     analysis limbo has silently dropped its content (an ingester lifts fully or \
                     hard-fails, never silently either)"
                ),
            );
        }
    }
}

/// The `lang:` form-stratum invariants the charter designates as native Rust-validator
/// primary gates (realized here rather than in SHACL). Runs over the merged dataset, so
/// the invariants hold bundle-wide, not merely per fixture.
fn check_lang_form_invariants(ds: &RdfDataset, report: &mut LintReport) {
    check_inline_blob_payload(ds, report);
    check_noncontiguous_form_slots(ds, report);
}

/// `lang:InlineBlobPayload` — document-scale surfaces hold a content-addressed blob
/// reference (`lang:surfaceBlob`), never inline payload bytes. Flag a `lang:SurfaceForm`
/// whose inline `lang:surfaceText` byte-length EXCEEDS [`DOCUMENT_SCALE_BYTES`] — a
/// document-scale surface that inlined its bytes instead of holding them by reference. The
/// threshold is the SAME value the pipeline's lang-form producer mints the
/// `lang:surfaceBlob` handle at, so the gate and the producer agree on the boundary.
fn check_inline_blob_payload(ds: &RdfDataset, report: &mut LintReport) {
    let surface_text = lang_iri("surfaceText");
    for surface in ds_subjects_of_type(ds, &lang_iri("SurfaceForm")) {
        for text in ds_object_literals(ds, &surface, &surface_text) {
            if text.len() > DOCUMENT_SCALE_BYTES {
                report.push_error(
                    codes::LANG_INLINE_BLOB_PAYLOAD,
                    format!("{surface}\t{}", text.len()),
                    format!(
                        "lang:InlineBlobPayload: surface {surface} carries a document-scale \
                         lang:surfaceText inline ({} bytes > {DOCUMENT_SCALE_BYTES}); document-scale \
                         surfaces hold their bytes by reference through lang:surfaceBlob, never inline",
                        text.len()
                    ),
                );
            }
        }
    }
}

/// `lang:NonContiguousSlots` — a `lang:ComposedForm`'s `lang:formSlot` slot indexes
/// (`lang:slotIndex`) are zero-based and contiguous, enforced UNCONDITIONALLY (there is no
/// lax mode). Flag a composed form whose multiset of declared slot indexes is not exactly
/// `0, 1, …, n-1` for its `n` slots — a missing index 0, an internal gap, a non-zero
/// start, or a maximum index not equal to the slot count minus one. Constituent order is
/// identity-bearing, so a gap or non-zero start is always ill-formed.
fn check_noncontiguous_form_slots(ds: &RdfDataset, report: &mut LintReport) {
    let form_slot = lang_iri("formSlot");
    let slot_index = lang_iri("slotIndex");
    for form in ds_subjects_of_type(ds, &lang_iri("ComposedForm")) {
        let slots = ds_object_iris_sorted(ds, &form, &form_slot);
        if slots.is_empty() {
            continue;
        }
        // Collect the declared integer indexes across the form's slots. A slot with no
        // integer index cannot take a place in the contiguous order, so a missing index is
        // itself non-contiguity (the count of indexes then falls short of the slot count).
        let mut indexes: Vec<i64> = Vec::new();
        for slot in &slots {
            for lex in ds_object_literals(ds, slot, &slot_index) {
                if let Ok(i) = lex.trim().parse::<i64>() {
                    indexes.push(i);
                }
            }
        }
        indexes.sort_unstable();
        // Zero-based and contiguous: the sorted index multiset is exactly 0..slot_count.
        let contiguous = indexes.len() == slots.len()
            && indexes.iter().enumerate().all(|(i, &idx)| idx == i as i64);
        if !contiguous {
            report.push_error(
                codes::LANG_NON_CONTIGUOUS_SLOTS,
                form.clone(),
                format!(
                    "lang:NonContiguousSlots: composed form {form} has slot indexes {indexes:?} over \
                     {} slot(s); slot indexes are zero-based and contiguous (0, 1, …, n-1), enforced \
                     unconditionally — a gap, a non-zero start, or a missing index is ill-formed",
                    slots.len()
                ),
            );
        }
    }
}

/// `lang:UnattributedEngineClaim` — an external engine is an oracle that produces
/// claims, never an authority that produces facts, so every reading a `lang:
/// InterpretationAct` marked as an engine run (through `lang:interpretationEngine`)
/// produces MUST be a vantage-held reading (carrying `gmeow:vantage`). An engine
/// reading with no vantage has entered engine output as unattributed structure.
///
/// Keying on `lang:interpretationEngine` scopes the gate to engine runs, so a manual
/// or compositional interpretation act — whose co-resident readings are held through
/// a separate `gmeow:Observation` and may lawfully leave the non-preferred alternative
/// unclaimed — is never flagged.
fn check_unattributed_engine_claim(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    let engine = lang_iri("interpretationEngine");
    let produced = lang_iri("producedReading");
    let vantage = format!("{}vantage", cfg.namespace);
    for act in ds_subjects_of_type(ds, &lang_iri("InterpretationAct")) {
        if !ds_has_predicate(ds, &act, &engine) {
            continue;
        }
        for reading in ds_object_iris_sorted(ds, &act, &produced) {
            if !ds_has_predicate(ds, &reading, &vantage) {
                report.push_error(
                    codes::LANG_UNATTRIBUTED_ENGINE_CLAIM,
                    format!("{act}\t{reading}"),
                    format!(
                        "lang:UnattributedEngineClaim: engine interpretation act {act} produced \
                         reading {reading} with no gmeow:vantage; engine output enters as \
                         vantage-held readings, never unattributed structure"
                    ),
                );
            }
        }
    }
}

/// `lang:SilentPromotion` — promotion from an engine-claimed reading to a slice-
/// asserted analysis is an explicit provenance-carrying editorial act. A subject that
/// adopts a reading as canonical (through `lang:promotedReading`) MUST itself be a
/// `gmeow:Activity` carrying a `gmeow:vantage` (the editor who stands behind it);
/// a promotion from a subject that is not such an act has silently promoted the
/// reading, erasing the boundary between what an engine claimed and what the slice
/// asserts.
fn check_silent_promotion(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    let promoted = lang_iri("promotedReading");
    let activity = format!("{}Activity", cfg.namespace);
    let vantage = format!("{}vantage", cfg.namespace);
    let Some(p_id) = ds_iri_id(ds, &promoted) else {
        return;
    };
    let mut subjects: Vec<String> = Vec::new();
    for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
        if let TermRef::Iri(s) = ds.resolve(q.s) {
            subjects.push(s.to_owned());
        }
    }
    subjects.sort();
    subjects.dedup();
    for subj in subjects {
        let is_act = ds_has_type(ds, &subj, &activity);
        let is_vantage_held = ds_has_predicate(ds, &subj, &vantage);
        if !is_act || !is_vantage_held {
            for reading in ds_object_iris_sorted(ds, &subj, &promoted) {
                report.push_error(
                    codes::LANG_SILENT_PROMOTION,
                    format!("{subj}\t{reading}"),
                    format!(
                        "lang:SilentPromotion: subject {subj} promotes reading {reading} to a slice \
                         assertion but is not a provenance-carrying editorial act (a gmeow:Activity \
                         carrying a gmeow:vantage); promotion from an engine reading is an explicit \
                         provenance-carrying act"
                    ),
                );
            }
        }
    }
}

/// The `lang:` translation-stratum invariants the charter designates as native
/// Rust-validator primary gates (realized here rather than in SHACL). Runs over the
/// merged dataset, so the invariants hold bundle-wide, not merely per fixture.
fn check_lang_translation_invariants(ds: &RdfDataset, _cfg: &LintConfig, report: &mut LintReport) {
    check_surface_leak_in_content_key(ds, report);
    check_rendering_as_identity(ds, report);
}

/// `lang:SurfaceLeakInContentKey` — form identity is computed over structural
/// content alone and is independent of encoding, script, casing, and rendering. A
/// crossing (`lang:Translation`, `lang:TranslationUnit`, `lang:Rendering`, or
/// `lang:Paraphrase`) must reference structural forms and never inline
/// surface-stratum material as identity input. Flag any crossing subject that
/// directly carries a surface-stratum predicate.
fn check_surface_leak_in_content_key(ds: &RdfDataset, report: &mut LintReport) {
    let crossing_types = [
        lang_iri("Translation"),
        lang_iri("TranslationUnit"),
        lang_iri("Rendering"),
        lang_iri("Paraphrase"),
    ];
    let surface_predicates = [
        lang_iri("surfaceText"),
        lang_iri("inScript"),
        lang_iri("encoding"),
        lang_iri("unicodeNormalization"),
        lang_iri("collationLocale"),
    ];
    for type_iri in &crossing_types {
        for subj in ds_subjects_of_type(ds, type_iri) {
            for surface in &surface_predicates {
                if ds_has_predicate(ds, &subj, surface) {
                    report.push_error(
                        codes::LANG_SURFACE_LEAK,
                        format!("{subj}\t{surface}"),
                        format!(
                            "lang:SurfaceLeakInContentKey: crossing {subj} directly carries \
                             surface-stratum predicate {surface} as identity input; form identity \
                             is computed over structural content alone, independent of encoding, \
                             script, casing, and rendering"
                        ),
                    );
                }
            }
        }
    }
}

/// `lang:RenderingAsIdentity` — a rendering names the content it renders and never
/// substitutes for that content's identity. Flag a `lang:Rendering` that is its own
/// `lang:renderedContent` (a), is `owl:sameAs` its own `lang:renderedContent` (b),
/// or whose `lang:renderingForm` equals its `lang:renderedContent` (c).
fn check_rendering_as_identity(ds: &RdfDataset, report: &mut LintReport) {
    const OWL_SAMEAS: &str = "http://www.w3.org/2002/07/owl#sameAs";
    let rendered_content = lang_iri("renderedContent");
    let rendering_form = lang_iri("renderingForm");
    for subj in ds_subjects_of_type(ds, &lang_iri("Rendering")) {
        let content = ds_object_iris(ds, &subj, &rendered_content);
        // (a) rendering is its own renderedContent.
        if content.contains(&subj) {
            report.push_error(
                codes::LANG_RENDERING_AS_IDENTITY_SELF,
                subj.clone(),
                format!(
                    "lang:RenderingAsIdentity: rendering {subj} is its own lang:renderedContent \
                     (self-reference); a rendering names the content it renders, never itself"
                ),
            );
        }
        // (b) rendering is owl:sameAs its own renderedContent.
        let same_as = ds_object_iris(ds, &subj, OWL_SAMEAS);
        for c in content.intersection(&same_as) {
            report.push_error(
                codes::LANG_RENDERING_AS_IDENTITY_SAMEAS,
                format!("{subj}\t{c}"),
                format!(
                    "lang:RenderingAsIdentity: rendering {subj} is asserted owl:sameAs its own \
                     lang:renderedContent {c}; the rendering has become identity"
                ),
            );
        }
        // (c) renderingForm equals renderedContent.
        let form = ds_object_iris(ds, &subj, &rendering_form);
        for c in content.intersection(&form) {
            report.push_error(
                codes::LANG_RENDERING_AS_IDENTITY_FORM,
                format!("{subj}\t{c}"),
                format!(
                    "lang:RenderingAsIdentity: rendering {subj} has lang:renderingForm equal to its \
                     lang:renderedContent {c}; the form has collapsed into the content"
                ),
            );
        }
    }
}

/// The `lang:` projection-stratum invariants the charter designates as native
/// Rust-validator/projection-test primary gates (realized here rather than in SHACL,
/// since each carries a join the SHACL Core surface cannot express). Runs over the
/// merged dataset, so the lossy-lowering contract holds bundle-wide over the whole
/// projection corpus, not merely per fixture.
fn check_lang_projection_invariants(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    check_missing_preservation_kind(ds, report);
    check_undeclared_unsupported_construct(ds, report);
    check_unrecorded_epistemic_loss(ds, cfg, report);
    check_projection_silent_disambiguation(ds, report);
    check_exact_preservation_violated(ds, report);
}

/// `lang:MissingPreservationKind` — every `lang:ProjectionEmission` declares a
/// `logic:preservationKind` (reusing the `logic:` loss-ledger vocabulary verbatim). An
/// emission with none has entered the loss ledger carrying an undeclared preservation
/// judgment, so its semiotic loss is unqueryable.
fn check_missing_preservation_kind(ds: &RdfDataset, report: &mut LintReport) {
    let preservation_kind = logic_iri("preservationKind");
    for emission in ds_subjects_of_type(ds, &lang_iri("ProjectionEmission")) {
        if !ds_has_predicate(ds, &emission, &preservation_kind) {
            report.push_error(
                codes::LANG_MISSING_PRESERVATION_KIND,
                emission.clone(),
                format!(
                    "lang:MissingPreservationKind: projection emission {emission} declares no \
                     logic:preservationKind; every projection declares its preservation kind (the \
                     logic: loss-ledger vocabulary, reused verbatim)"
                ),
            );
        }
    }
}

/// Whether an emission's declared `logic:preservationKind` set marks it lossy: it names
/// at least one preservation kind and NONE of them is `logic:ExactPreservation`. An
/// emission with no preservation kind is out of scope here (that is
/// `lang:MissingPreservationKind`), so a lossy verdict is always over a declared kind.
fn emission_is_lossy(ds: &RdfDataset, emission: &str) -> bool {
    let preservation_kind = logic_iri("preservationKind");
    let exact = logic_iri("ExactPreservation");
    let kinds = ds_object_iris(ds, emission, &preservation_kind);
    !kinds.is_empty() && !kinds.contains(&exact)
}

/// The co-resident reading count of a source form: the number of distinct `lang:Reading`
/// subjects reading it through `lang:readingOf`, or — when no reading points at the form
/// directly — the number of distinct `lang:Analysis` nodes the form is scoped to through
/// `lang:inAnalysis`. Both encode ambiguity multiplicity; the larger is the count.
fn source_reading_count(ds: &RdfDataset, source: &str) -> usize {
    let reading_of = lang_iri("readingOf");
    let in_analysis = lang_iri("inAnalysis");
    let mut readings: HashSet<String> = HashSet::new();
    if let (Some(p_id), Some(o_id)) = (ds_iri_id(ds, &reading_of), ds_iri_id(ds, source)) {
        for q in ds.quads_for_pattern(None, Some(p_id), Some(o_id), GraphMatch::Any) {
            if let TermRef::Iri(r) = ds.resolve(q.s) {
                readings.insert(r.to_owned());
            }
        }
    }
    let analyses = ds_object_iris(ds, source, &in_analysis);
    readings.len().max(analyses.len())
}

/// `lang:UndeclaredUnsupportedConstruct` — a lossy `lang:ProjectionEmission` (a declared
/// `logic:preservationKind` that is not `logic:ExactPreservation`) enumerates every
/// construct it drops through `lang:unsupportedConstruct`. A lossy emission naming none has
/// claimed a completeness its own preservation kind denies — the overclaim floor, over
/// bundle data.
fn check_undeclared_unsupported_construct(ds: &RdfDataset, report: &mut LintReport) {
    let unsupported = lang_iri("unsupportedConstruct");
    for emission in ds_subjects_of_type(ds, &lang_iri("ProjectionEmission")) {
        if emission_is_lossy(ds, &emission)
            && ds_object_literals(ds, &emission, &unsupported).is_empty()
        {
            report.push_error(
                codes::LANG_UNDECLARED_UNSUPPORTED_CONSTRUCT,
                emission.clone(),
                format!(
                    "lang:UndeclaredUnsupportedConstruct: lossy projection emission {emission} (a \
                     logic:preservationKind other than logic:ExactPreservation) enumerates no \
                     lang:unsupportedConstruct; a projection drops nothing or names everything it drops"
                ),
            );
        }
    }
}

/// `lang:UnrecordedEpistemicLoss` — a form-view-flattening (lossy) `lang:ProjectionEmission`
/// whose `lang:projectsSource` carries epistemic structure (a `gmeow:vantage`, a
/// `lang:InterpretationAct`, two or more co-resident readings, or a `lang:Translation`) MUST
/// name that flattened stratum among its `lang:unsupportedConstruct` entries. An emission
/// that flattens the epistemic layer yet enumerates none of it has hidden the loss.
fn check_unrecorded_epistemic_loss(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    let projects_source = lang_iri("projectsSource");
    let unsupported = lang_iri("unsupportedConstruct");
    let vantage = format!("{}vantage", cfg.namespace);
    for emission in ds_subjects_of_type(ds, &lang_iri("ProjectionEmission")) {
        // Flattening is a loss; an exact emission preserves everything and flattens nothing.
        if !emission_is_lossy(ds, &emission) {
            continue;
        }
        let drops: Vec<String> = ds_object_literals(ds, &emission, &unsupported)
            .into_iter()
            .map(|d| d.to_lowercase())
            .collect();
        for source in ds_object_iris_sorted(ds, &emission, &projects_source) {
            // The epistemic strata the source carries, each paired with the keyword the drop
            // list must name to record having flattened it.
            let mut strata: Vec<&str> = Vec::new();
            if ds_has_predicate(ds, &source, &vantage) {
                strata.push("vantage");
            }
            if ds_has_type(ds, &source, &lang_iri("InterpretationAct")) {
                strata.push("interpretation");
            }
            if source_reading_count(ds, &source) >= 2 {
                strata.push("reading");
            }
            if ds_has_type(ds, &source, &lang_iri("Translation")) {
                strata.push("translation");
            }
            if strata.is_empty() {
                continue;
            }
            // EVERY flattened stratum must be recorded — not merely one of them. A source
            // carrying `[vantage, reading, translation]` that enumerates only `vantage`
            // silently flattens `reading` and `translation`, which is exactly the
            // `lang:UnrecordedEpistemicLoss` this gate forbids; `all` (not `any`) enforces it.
            let names_all_strata = strata.iter().all(|kw| drops.iter().any(|d| d.contains(kw)));
            if !names_all_strata {
                report.push_error(
                    codes::LANG_UNRECORDED_EPISTEMIC_LOSS,
                    format!("{emission}\t{source}"),
                    format!(
                        "lang:UnrecordedEpistemicLoss: form-view projection emission {emission} projects \
                         source {source} carrying epistemic structure ({strata:?}) but does not name all \
                         of it among its lang:unsupportedConstruct entries; a form-view emission \
                         enumerates every epistemic stratum it flattens"
                    ),
                );
            }
        }
    }
}

/// `lang:ProjectionSilentDisambiguation` — a per-reading `lang:ProjectionEmission` (one that
/// declares a `lang:emittedReadingCount`) emits one row per co-resident reading its
/// `lang:projectsSource` form holds. An emitted count LESS than the source's co-resident
/// reading count has collapsed the readings to a silently-chosen winner at the projection
/// seam — distinct from the bundle-wide `lang:SilentDisambiguation` (a meaning-layer collapse).
fn check_projection_silent_disambiguation(ds: &RdfDataset, report: &mut LintReport) {
    let projects_source = lang_iri("projectsSource");
    let emitted_reading_count = lang_iri("emittedReadingCount");
    for emission in ds_subjects_of_type(ds, &lang_iri("ProjectionEmission")) {
        // Only per-reading emissions declare an emitted-reading count; others are out of scope.
        let Some(emitted) = ds_object_literals(ds, &emission, &emitted_reading_count)
            .iter()
            .filter_map(|l| l.trim().parse::<i64>().ok())
            .max()
        else {
            continue;
        };
        for source in ds_object_iris_sorted(ds, &emission, &projects_source) {
            let co_resident = source_reading_count(ds, &source) as i64;
            if emitted < co_resident {
                report.push_error(
                    codes::LANG_PROJECTION_SILENT_DISAMBIGUATION,
                    format!("{emission}\t{source}"),
                    format!(
                        "lang:ProjectionSilentDisambiguation: per-reading projection emission {emission} \
                         declares lang:emittedReadingCount {emitted} for source {source} holding \
                         {co_resident} co-resident readings; a per-reading projection emits one row per \
                         reading, never a silently-chosen winner"
                    ),
                );
            }
        }
    }
}

/// `lang:ExactPreservationViolated` — a `lang:ProjectionEmission` claiming
/// `logic:preservationKind` `logic:ExactPreservation` whose MEASURED `lang:roundTripHolds`
/// is false has made an exactness claim its own round-trip refutes. The measurement is
/// computed, not asserted; the exactness claim, not the measurement, is the fault.
fn check_exact_preservation_violated(ds: &RdfDataset, report: &mut LintReport) {
    let preservation_kind = logic_iri("preservationKind");
    let exact = logic_iri("ExactPreservation");
    let round_trip_holds = lang_iri("roundTripHolds");
    for emission in ds_subjects_of_type(ds, &lang_iri("ProjectionEmission")) {
        if !ds_object_iris(ds, &emission, &preservation_kind).contains(&exact) {
            continue;
        }
        let refuted = ds_object_literals(ds, &emission, &round_trip_holds)
            .iter()
            .any(|v| v.trim().eq_ignore_ascii_case("false"));
        if refuted {
            report.push_error(
                codes::LANG_EXACT_PRESERVATION_VIOLATED,
                emission.clone(),
                format!(
                    "lang:ExactPreservationViolated: projection emission {emission} claims \
                     logic:ExactPreservation but its measured lang:roundTripHolds is false; an exactness \
                     claim its own round-trip refutes"
                ),
            );
        }
    }
}

/// Namespace root for the `math:` measure-and-dimension invariants.
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

fn math_iri(term: &str) -> String {
    format!("{MATH_NS}{term}")
}

/// The seven SI base dimensions, in canonical ℚ⁷ index order. A dimension vector
/// is a length-7 array of exact rationals over these generators.
const BASE_DIMENSIONS: [&str; 7] = [
    "lengthDimension",
    "massDimension",
    "timeDimension",
    "electricCurrentDimension",
    "temperatureDimension",
    "amountOfSubstanceDimension",
    "luminousIntensityDimension",
];

/// Position of a base-dimension IRI in the canonical ℚ⁷ order, if it is one of the
/// seven SI generators.
fn base_dimension_index(iri: &str) -> Option<usize> {
    BASE_DIMENSIONS.iter().position(|b| iri == math_iri(b))
}

/// The exact-rational exponent scalar of the dimension vector space. This is the
/// shared [`gmeow_math::Rational`] — an `i128`-backed, gcd-normalized rational with
/// a positive denominator, so `PartialEq`/`Eq` is value equality: dimensions are
/// equal exactly when their exponent vectors are equal (the derived
/// `math:commensurableWith`), and exact rationals (not `xsd:decimal`) keep that
/// equality precise for fractional dimensions such as T^(-1/2). There is no
/// duplicate rational type here — the native math: gate computes THROUGH the same
/// exact-rational carrier the affect-intensity and Gram-matrix loaders use.
type DimVector = [Rational; 7];

fn zero_vector() -> DimVector {
    [Rational::zero(); 7]
}

/// Componentwise exact-rational vector sum — the group operation of the dimension
/// vector space (a product of dimensions adds their exponent vectors). `None` on
/// overflow.
fn add_vectors(a: &DimVector, b: &DimVector) -> Option<DimVector> {
    let mut out = zero_vector();
    for i in 0..7 {
        out[i] = a[i].checked_add(b[i]).ok()?;
    }
    Some(out)
}

/// Literal lexical values of `(subject, predicate, ?)` (literals only).
fn ds_object_literals(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(s_id), Some(p_id)) = (ds_iri_id(ds, subject_iri), ds_iri_id(ds, predicate_iri))
    else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Any) {
        if let TermRef::Literal { lexical, .. } = ds.resolve(q.o) {
            out.push(lexical.to_owned());
        }
    }
    out
}

/// Object IRIs of `(subject, predicate, ?)`, sorted for deterministic iteration.
fn ds_object_iris_sorted(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> Vec<String> {
    let mut v: Vec<String> = ds_object_iris(ds, subject_iri, predicate_iri)
        .into_iter()
        .collect();
    v.sort();
    v
}

/// Canonical base-dimension symbols, in the same order as [`BASE_DIMENSIONS`]. Used
/// to render a dimension vector to its human-readable string for the drift check.
const BASE_SYMBOLS: [&str; 7] = ["L", "M", "T", "I", "\u{0398}", "N", "J"];

/// Render a dimension vector to its canonical `math:dimensionVector` string, e.g.
/// `"L·T-1"` for velocity or `"1"` for a dimensionless quantity. Exponent 1 is
/// elided; a non-unit denominator prints as `num/den`. This is the single source the
/// authored string must match — the string is a computed projection, not an
/// independent hand-authored fact.
fn render_dimension_vector(v: &DimVector) -> String {
    let mut parts: Vec<String> = Vec::new();
    for i in 0..7 {
        let r = v[i];
        let (num, den) = (r.numerator(), r.denominator());
        if num == 0 {
            continue;
        }
        let mut s = BASE_SYMBOLS[i].to_string();
        if !(num == 1 && den == 1) {
            if den == 1 {
                s.push_str(&num.to_string());
            } else {
                s.push_str(&format!("{num}/{den}"));
            }
        }
        parts.push(s);
    }
    if parts.is_empty() {
        "1".to_string()
    } else {
        parts.join("\u{00b7}")
    }
}

/// The exact-rational ℚ⁷ exponent vector of a dimension IRI. A base dimension is a
/// unit basis vector; `math:dimensionless` (or any `math:Dimensionless`) is zero; a
/// `math:DerivedDimension` sums `power * e_base` over its `math:baseDimensionExponent`
/// cells. Pure: returns `None` for a dimension whose structure is ill-formed (a
/// non-base exponent target, a missing/non-integer/zero-denominator power, or
/// arithmetic overflow) or whose kind cannot be computed, so an unrelated node never
/// yields a false positive. The ill-formed structural cases are surfaced explicitly,
/// not swallowed here — a non-base exponent target by the SHACL `DimensionExponentShape`
/// (`sh:class`), and a zero denominator by both that shape (`sh:not sh:hasValue 0`) and
/// the native zero-denominator scan in [`check_math_dimension_invariants`] — so a `None`
/// from a genuinely malformed cell means "already reported elsewhere", never "silently
/// dropped".
fn dimension_vector(ds: &RdfDataset, dim_iri: &str) -> Option<DimVector> {
    if let Some(i) = base_dimension_index(dim_iri) {
        let mut v = zero_vector();
        v[i] = Rational::new(1, 1).ok()?;
        return Some(v);
    }
    if dim_iri == math_iri("dimensionless") || ds_has_type(ds, dim_iri, &math_iri("Dimensionless"))
    {
        return Some(zero_vector());
    }
    if !ds_has_type(ds, dim_iri, &math_iri("DerivedDimension")) {
        return None;
    }
    let mut v = zero_vector();
    for cell in ds_object_iris_sorted(ds, dim_iri, &math_iri("baseDimensionExponent")) {
        let base = ds_object_iris_sorted(ds, &cell, &math_iri("exponentOfDimension"))
            .into_iter()
            .next()?;
        let bi = base_dimension_index(&base)?;
        let num = ds_object_literals(ds, &cell, &math_iri("exponentNumerator"))
            .into_iter()
            .find_map(|l| l.parse::<i128>().ok())?;
        let den = ds_object_literals(ds, &cell, &math_iri("exponentDenominator"))
            .into_iter()
            .find_map(|l| l.parse::<i128>().ok())?;
        v[bi] = v[bi].checked_add(Rational::new(num, den).ok()?).ok()?;
    }
    Some(v)
}

/// The single dimension IRI a dimensioned node carries through `math:hasDimension`
/// (lexically least if several — the shape forbids more than one).
fn node_dimension_iri(ds: &RdfDataset, node_iri: &str) -> Option<String> {
    ds_object_iris_sorted(ds, node_iri, &math_iri("hasDimension"))
        .into_iter()
        .next()
}

/// The `math:` measure-and-dimension reasoned gate. Dimensional homogeneity is
/// computed from the exact-rational exponent vectors of the ℚ-vector space of
/// dimensions — a reasoned check, not asserted data. Runs over the merged dataset
/// (`GraphMatch::Any`), so it holds bundle-wide.
fn check_math_dimension_invariants(ds: &RdfDataset, report: &mut LintReport) {
    // dimensionVector drift: an authored string must match the canonical render of the
    // structured exponents (the string is a projection, never a divergent second
    // source). Only SHACL cannot express this, so it lives here.
    if let Some(dv_pid) = ds_iri_id(ds, &math_iri("dimensionVector")) {
        let mut flagged: HashSet<String> = HashSet::new();
        let mut rows: Vec<(String, String)> = Vec::new();
        for q in ds.quads_for_pattern(None, Some(dv_pid), None, GraphMatch::Any) {
            let (TermRef::Iri(subj), TermRef::Literal { lexical, .. }) =
                (ds.resolve(q.s), ds.resolve(q.o))
            else {
                continue;
            };
            rows.push((subj.to_owned(), lexical.to_owned()));
        }
        rows.sort();
        for (subj, lexical) in rows {
            let Some(vec) = dimension_vector(ds, &subj) else {
                continue;
            };
            let canonical = render_dimension_vector(&vec);
            if canonical != lexical && flagged.insert(subj.clone()) {
                report.push_error(
                    codes::MATH_MALFORMED_DIMENSION_VECTOR,
                    subj.clone(),
                    format!(
                        "math:MalformedDimension: dimension {subj} declares math:dimensionVector \
                         \"{lexical}\" but its structured exponents render to \"{canonical}\" — the \
                         string is a computed projection, not an independent source"
                    ),
                );
            }
        }
    }

    // Zero-denominator exponent: an exact-rational power needs a non-zero denominator.
    // `dimension_vector` returns `None` on such a cell (Rational::new rejects a zero
    // denominator), which would let the cell be silently skipped by the homogeneity /
    // composition loops below; surface it here as math:MalformedDimension so a malformed
    // power hard-fails rather than fails open. The SHACL DimensionExponentShape forbids
    // it too (sh:not sh:hasValue 0) — this native scan is the whole-bundle twin, genuine
    // defense in depth rather than a false claim of one.
    {
        let mut cells = ds_subjects_of_type(ds, &math_iri("DimensionExponent"));
        cells.sort();
        for cell in cells {
            let has_zero_denominator =
                ds_object_literals(ds, &cell, &math_iri("exponentDenominator"))
                    .into_iter()
                    .any(|l| l.trim().parse::<i128>() == Ok(0));
            if has_zero_denominator {
                report.push_error(
                    codes::MATH_MALFORMED_DIMENSION_ZERO_DENOMINATOR,
                    cell.clone(),
                    format!(
                        "math:MalformedDimension: dimension-exponent cell {cell} declares \
                         math:exponentDenominator 0 — an exact-rational power needs a non-zero \
                         denominator; the cell is ill-formed"
                    ),
                );
            }
        }
    }

    // Homogeneity: every operand of a math:DimensionalExpression shares one dimension.
    for expr in ds_subjects_of_type(ds, &math_iri("DimensionalExpression")) {
        let operands = ds_object_iris_sorted(ds, &expr, &math_iri("homogeneousOperand"));
        let mut seen: Vec<(DimVector, String)> = Vec::new();
        let mut undimensioned: Vec<String> = Vec::new();
        for operand in operands {
            let Some(dim_iri) = node_dimension_iri(ds, &operand) else {
                // No math:hasDimension at all: an undimensioned operand cannot be shown
                // homogeneous with a dimensioned one. Do not fail open — collect it.
                // (A malformed dimension — hasDimension present but structurally broken —
                // is reported by the zero-denominator scan / SHACL, so `dimension_vector`
                // returning None below is a deliberate skip, not a fail-open.)
                undimensioned.push(operand);
                continue;
            };
            let Some(vec) = dimension_vector(ds, &dim_iri) else {
                continue;
            };
            if !seen.iter().any(|(v, _)| *v == vec) {
                seen.push((vec, dim_iri));
            }
        }
        if !undimensioned.is_empty() {
            report.push_error(
                codes::MATH_INHOMOGENEITY_UNDIMENSIONED,
                expr.clone(),
                format!(
                    "math:DimensionalInhomogeneity: dimensional expression {expr} combines \
                     undimensioned operand(s) [{}] — every math:homogeneousOperand must carry a \
                     math:hasDimension to be shown homogeneous",
                    undimensioned.join(", ")
                ),
            );
        }
        if seen.len() >= 2 {
            let mut dims: Vec<String> = seen.into_iter().map(|(_, d)| d).collect();
            dims.sort();
            report.push_error(
                codes::MATH_INHOMOGENEITY_DIFFERING,
                expr.clone(),
                format!(
                    "math:DimensionalInhomogeneity: dimensional expression {expr} combines operands \
                     of differing dimensions [{}]",
                    dims.join(", ")
                ),
            );
        }
    }

    // Integral parameter composition: dim(result) == dim(integrand) + dim(measure).
    for integral in ds_subjects_of_type(ds, &math_iri("Integral")) {
        let Some(result_dim) = node_dimension_iri(ds, &integral) else {
            continue;
        };
        let integrand = ds_object_iris_sorted(ds, &integral, &math_iri("integrand"))
            .into_iter()
            .next();
        let measure = ds_object_iris_sorted(ds, &integral, &math_iri("withRespectTo"))
            .into_iter()
            .next();
        let (Some(integrand), Some(measure)) = (integrand, measure) else {
            // Missing integrand/measure is math:IncompleteIntegral (SHACL IntegralShape).
            continue;
        };
        let (Some(idim), Some(mdim)) = (
            node_dimension_iri(ds, &integrand),
            node_dimension_iri(ds, &measure),
        ) else {
            // The integral declares a result dimension but its integrand or measure carries
            // none, so the composition cannot be checked. Do not fail open — an integral
            // engaged in dimensional bookkeeping must dimension the parts it composes.
            report.push_error(
                codes::MATH_INTEGRAL_UNDIMENSIONED_PART,
                integral.clone(),
                format!(
                    "math:DimensionalInhomogeneity: integral {integral} declares result dimension \
                     {result_dim} but its integrand ({integrand}) or measure ({measure}) carries no \
                     math:hasDimension, so the composition cannot be verified"
                ),
            );
            continue;
        };
        let (Some(rv), Some(iv), Some(mv)) = (
            dimension_vector(ds, &result_dim),
            dimension_vector(ds, &idim),
            dimension_vector(ds, &mdim),
        ) else {
            continue;
        };
        let Some(composed) = add_vectors(&iv, &mv) else {
            continue;
        };
        if rv != composed {
            report.push_error(
                codes::MATH_INTEGRAL_COMPOSITION_MISMATCH,
                integral.clone(),
                format!(
                    "math:DimensionalInhomogeneity: integral {integral} declares result dimension \
                     {result_dim} but its integrand ({idim}) and measure ({mdim}) compose to a \
                     different dimension"
                ),
            );
        }
    }
}

/// The `math:` ingestion-bridge invariants the BRIDGES charter designates as native
/// Rust-validator primary gates. Runs over the merged dataset (`GraphMatch::Any`), so the
/// invariants hold bundle-wide, not merely per fixture.
fn check_math_ingest_invariants(ds: &RdfDataset, report: &mut LintReport) {
    check_unliftable_ingest(ds, report);
}

/// `math:UnliftableIngest` — a bridge is the mnemomorphic `put` leg of a `logic:Correspondence`:
/// GMEOW is the source, the external artifact the view, and the lift is the up-projection (`put`),
/// never a `get` run backward (the calculus's named anti-pattern). A lawful `put` comes from a
/// retained mnemomorphic witness (`math:parseSource`), so a `math:IngestRun` that retains a source
/// but produces NO structured `math:` codomain — nothing is `gmeow:wasGeneratedBy` it — has silently
/// dropped everything it was meant to lift. That is the `unsupported` / `logic:ObligationViolated`
/// outcome the correspondence Overclaim and Mnemomorphism gates decide, projected to the process
/// layer: a bridge lifts fully or hard-fails, never emitting a degraded or empty lift. (A run that
/// retains no source at all is caught upstream by `math:UngroundedIngestRun`, the SHACL grounding
/// shape; and the partial-drop case — a lift that produced some codomain but dropped part without
/// enumerating the residue — is the correspondence Overclaim gate's job in the `logic:` layer. This
/// native twin catches the produced-nothing case bundle-wide.)
fn check_unliftable_ingest(ds: &RdfDataset, report: &mut LintReport) {
    const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
    let parse_source = math_iri("parseSource");
    let was_generated_by = format!("{GMEOW_NS}wasGeneratedBy");
    let wgb_pid = ds_iri_id(ds, &was_generated_by);

    // The abstract `math:IngestRun` and its three concrete bridge subclasses. Subclass
    // materialization is not assumed, so each concrete run type is scanned explicitly.
    let mut runs: Vec<String> = Vec::new();
    for ty in ["IngestRun", "RIngestRun", "ONNXIngestRun", "ProofIngestRun"] {
        runs.extend(ds_subjects_of_type(ds, &math_iri(ty)));
    }
    runs.sort();
    runs.dedup();

    for run in runs {
        // A run with no retained source is out of scope here — it is caught by the
        // `math:UngroundedIngestRun` grounding shape, not this gate.
        if !ds_has_predicate(ds, &run, &parse_source) {
            continue;
        }
        // Did the run produce a structured `math:` codomain? The produced object points back at
        // the run through `gmeow:wasGeneratedBy`, so an inverse lookup `(?, wasGeneratedBy, run)`
        // decides it.
        let produced = match (wgb_pid, ds_iri_id(ds, &run)) {
            (Some(p), Some(r)) => ds
                .quads_for_pattern(None, Some(p), Some(r), GraphMatch::Any)
                .next()
                .is_some(),
            _ => false,
        };
        if !produced {
            report.push_error(
                codes::MATH_UNLIFTABLE_INGEST,
                run.clone(),
                format!(
                    "math:UnliftableIngest: ingest run {run} retains a math:parseSource but produced no \
                     structured math: codomain (nothing is gmeow:wasGeneratedBy it) — the lift is \
                     unsupported and silently dropped its content; a bridge lifts fully or hard-fails, \
                     never emitting a degraded or empty lift"
                ),
            );
        }
    }
}

/// The `gmeow:` vocabulary namespace root — the canonical GMEOW IRI stem. A
/// `gmeow:Quantity` (the superclass of `math:ProbabilityValue`) carries its
/// magnitude in `gmeow:quantityValue`, distinct from the `math:Quantity`-scoped
/// `math:quantityValue`.
const GMEOW_NS_ROOT: &str = "https://blackcatinformatics.ca/gmeow/";

fn gmeow_iri(term: &str) -> String {
    format!("{GMEOW_NS_ROOT}{term}")
}

/// Parse a plain decimal literal (optional leading `-`/`+`, an integer part, an
/// optional `.frac`) into an EXACT [`Rational`]: the value is the digit string with
/// the point removed over `10^(count of fractional digits)`. Scientific notation
/// (`e`/`E`) and any otherwise-unparseable input yield `None` so an unreadable
/// magnitude is SKIPPED (never a false positive), never coerced.
fn decimal_to_rational(s: &str) -> Option<Rational> {
    let s = s.trim();
    if s.is_empty() || s.contains('e') || s.contains('E') {
        return None;
    }
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !frac_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits: String = format!("{int_part}{frac_part}");
    let mut num = digits.parse::<i128>().ok()?;
    if neg {
        num = num.checked_neg()?;
    }
    let den = 10i128.checked_pow(u32::try_from(frac_part.len()).ok()?)?;
    Rational::new(num, den).ok()
}

/// The exact-rational magnitude of a value node: if the node is a `math:RationalValue`,
/// its `math:numerator`/`math:denominator` pair; otherwise the first readable decimal
/// literal among `decimal_preds` (in order), parsed by [`decimal_to_rational`]. `None`
/// when no magnitude is readable, so an unreadable node is SKIPPED, never false-flagged.
fn read_magnitude(ds: &RdfDataset, node: &str, decimal_preds: &[String]) -> Option<Rational> {
    if ds_has_type(ds, node, &math_iri("RationalValue")) {
        let num = ds_object_literals(ds, node, &math_iri("numerator"))
            .into_iter()
            .find_map(|l| l.trim().parse::<i128>().ok())?;
        let den = ds_object_literals(ds, node, &math_iri("denominator"))
            .into_iter()
            .find_map(|l| l.trim().parse::<i128>().ok())?;
        return Rational::new(num, den).ok();
    }
    for pred in decimal_preds {
        if let Some(r) = ds_object_literals(ds, node, pred)
            .into_iter()
            .find_map(|l| decimal_to_rational(&l))
        {
            return Some(r);
        }
    }
    None
}

/// All named-node objects of any `(?, predicate, object)` triple across the dataset,
/// deduplicated and sorted. The dual of [`ds_subjects_of_type`] for the object slot —
/// used to enumerate the probability models a reasoning request references.
fn ds_objects_of_predicate(ds: &RdfDataset, predicate_iri: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(p_id) = ds_iri_id(ds, predicate_iri) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
        if let TermRef::Iri(o) = ds.resolve(q.o) {
            out.push(o.to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// All subjects of a `(subject, predicate, object)` triple with a fixed object IRI —
/// the inverse lookup used to walk from a distribution parameter back to its owning
/// distribution and thence to the random variable it parameterizes.
fn ds_subjects_with_object(ds: &RdfDataset, predicate_iri: &str, object_iri: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(p_id), Some(o_id)) = (ds_iri_id(ds, predicate_iri), ds_iri_id(ds, object_iri)) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(p_id), Some(o_id), GraphMatch::Any) {
        if let TermRef::Iri(s) = ds.resolve(q.s) {
            out.push(s.to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Structural completeness predicate for a `math:MarkovKernel`: a declared domain AND
/// codomain (`math:kernelDomain`, `math:kernelCodomain`). Shared VERBATIM between Gate 4
/// (`math:IncompleteDependencyModel`) and Gate 5 (`math:ExactPreservationViolated`) so the
/// two gates can never disagree on what "complete" means for a kernel.
fn markov_kernel_is_complete(ds: &RdfDataset, k: &str) -> bool {
    ds_has_predicate(ds, k, &math_iri("kernelDomain"))
        && ds_has_predicate(ds, k, &math_iri("kernelCodomain"))
}

/// Structural completeness predicate for a `math:BayesianNetwork` or `math:FactorGraph`: a
/// declared `math:dependencyGraph`. Shared VERBATIM between Gate 4 and Gate 5 so the two
/// gates can never disagree on what "complete" means for a dependency-graph model.
fn dependency_graph_is_complete(ds: &RdfDataset, node: &str) -> bool {
    ds_has_predicate(ds, node, &math_iri("dependencyGraph"))
}

/// Whether `node` declares `logic:preservationKind logic:ExactPreservation` DIRECTLY on
/// itself (the instance), as opposed to inheriting the declaration from its class's
/// TBox-level `logic:preservationKind logic:ExactPreservation` (module.ttl declares this
/// unconditionally on `math:BayesianNetwork`, `math:FactorGraph`, and `math:MarkovKernel` so
/// the charter's "conditional — exact once the completeness gate holds" row can be read off
/// the class at all). Only the instance-level declaration is an author's explicit exactness
/// CLAIM about THIS model; it is what Gate 5 checks against structural completeness to catch
/// the overclaim `math:ExactPreservationViolated`.
fn declares_exact_preservation_directly(ds: &RdfDataset, node: &str) -> bool {
    ds_object_iris(ds, node, &logic_iri("preservationKind"))
        .contains(&logic_iri("ExactPreservation"))
}

/// The `math:` probability-layer invariants the charter designates as native
/// Rust-validator primary gates, computed from the exact-rational carrier (never
/// asserted data). Runs over the merged dataset (`GraphMatch::Any`), so the invariants
/// hold bundle-wide, not merely per fixture.
fn check_math_probability_invariants(ds: &RdfDataset, report: &mut LintReport) {
    let (Some(zero), Some(one)) = (Rational::new(0, 1).ok(), Rational::new(1, 1).ok()) else {
        return;
    };

    // Gate 1 — math:ProbabilityOutOfBounds: a math:ProbabilityValue is ALWAYS in the
    // closed unit interval [0, 1]. Its magnitude is read exactly (a math:RationalValue
    // numerator/denominator pair, else its gmeow:quantityValue decimal) and compared by
    // exact-rational order; an unreadable magnitude is skipped, never coerced.
    for node in ds_subjects_of_type(ds, &math_iri("ProbabilityValue")) {
        let Some(mag) = read_magnitude(ds, &node, &[gmeow_iri("quantityValue")]) else {
            continue;
        };
        if mag < zero || mag > one {
            report.push_error(
                codes::MATH_PROBABILITY_OUT_OF_BOUNDS,
                node.clone(),
                format!(
                    "math:ProbabilityOutOfBounds: probability value {node} has magnitude {}/{} \
                     outside the closed unit interval [0,1]",
                    mag.numerator(),
                    mag.denominator()
                ),
            );
        }
    }

    // Gate 2 — math:DistributionParameterConstraint: a parameter's quantity must satisfy
    // the positivity and dimension constraints CARRIED on the role it fills. Positivity is
    // an exact `> 0` check; the dimension constraint is resolved by exact ℚ⁷ arithmetic
    // (same-as / square-of the random variable's dimension, or an absolute dimension).
    for p in ds_subjects_of_type(ds, &math_iri("DistributionParameter")) {
        let Some(role) = ds_object_iris_sorted(ds, &p, &math_iri("parameterRole"))
            .into_iter()
            .next()
        else {
            continue;
        };

        // Positivity: a role declaring math:requiresPositiveValue true forbids a quantity
        // whose exact magnitude is not strictly positive. xsd:boolean also serializes as
        // "1" (canonical is "true"/"false", but "0"/"1" are valid lexical forms), so both
        // are accepted.
        let requires_positive = ds_object_literals(ds, &role, &math_iri("requiresPositiveValue"))
            .iter()
            .any(|l| {
                let t = l.trim();
                t == "true" || t == "1"
            });
        if requires_positive
            && let Some(q) = ds_object_iris_sorted(ds, &p, &math_iri("parameterQuantity"))
                .into_iter()
                .next()
            && let Some(mag) = read_magnitude(
                ds,
                &q,
                &[gmeow_iri("quantityValue"), math_iri("quantityValue")],
            )
            && mag <= zero
        {
            report.push_error(
                codes::MATH_PROBABILITY_PARAMETER_CONSTRAINT,
                p.clone(),
                format!(
                    "math:DistributionParameterConstraint: parameter {p} fills a positive-required \
                     role but its quantity magnitude {}/{} is not > 0",
                    mag.numerator(),
                    mag.denominator()
                ),
            );
        }

        // Dimension: the role names the dimension its parameter's quantity must carry,
        // absolutely or by reference to the random variable's dimension.
        let Some(dspec) = ds_object_iris_sorted(ds, &role, &math_iri("quantityDimension"))
            .into_iter()
            .next()
        else {
            continue;
        };
        let Some(q) = ds_object_iris_sorted(ds, &p, &math_iri("parameterQuantity"))
            .into_iter()
            .next()
        else {
            continue;
        };
        let Some(pd) = node_dimension_iri(ds, &q) else {
            continue;
        };
        let Some(actual) = dimension_vector(ds, &pd) else {
            continue;
        };

        let same = dspec == math_iri("sameAsRandomVariableDimension");
        let square = dspec == math_iri("squareOfRandomVariableDimension");
        if same || square {
            // Resolve the random variable's dimension: parameter → owning distribution →
            // random variable. Any missing link is a deliberate skip (no false positive).
            let Some(dist) = ds_subjects_with_object(ds, &math_iri("hasDistributionParameter"), &p)
                .into_iter()
                .next()
            else {
                continue;
            };
            let Some(rv) = ds_subjects_with_object(ds, &math_iri("hasDistribution"), &dist)
                .into_iter()
                .find(|rv| ds_has_type(ds, rv, &math_iri("RandomVariable")))
            else {
                continue;
            };
            let Some(rvdim) = node_dimension_iri(ds, &rv) else {
                continue;
            };
            let Some(rvec) = dimension_vector(ds, &rvdim) else {
                continue;
            };
            let required = if square {
                add_vectors(&rvec, &rvec)
            } else {
                Some(rvec)
            };
            let Some(required) = required else {
                continue;
            };
            if required != actual {
                let relation = if square {
                    "the square"
                } else {
                    "the same dimension"
                };
                report.push_error(
                    codes::MATH_PROBABILITY_PARAMETER_CONSTRAINT,
                    p.clone(),
                    format!(
                        "math:DistributionParameterConstraint: parameter {p} must carry {relation} \
                         of the random variable's dimension but carries a different dimension"
                    ),
                );
            }
        } else if let Some(required) = dimension_vector(ds, &dspec)
            && required != actual
        {
            report.push_error(
                codes::MATH_PROBABILITY_PARAMETER_CONSTRAINT,
                p.clone(),
                format!(
                    "math:DistributionParameterConstraint: parameter {p} must carry dimension \
                     {dspec} but its quantity carries a different dimension"
                ),
            );
        }
    }

    // Gate 3 — math:MissingProbabilityModelLowering: a reasoning request references a
    // probability model through logic:probabilityModel; that model must declare its logic:
    // lowering (math:probabilityModelLowering) either directly or class-level (on one of
    // its rdf:types). Absent a lowering the engine reports unsupported, never assumes
    // independence — so the absence is a caught, typed failure.
    let lowering = math_iri("probabilityModelLowering");
    for o in ds_objects_of_predicate(ds, &logic_iri("probabilityModel")) {
        let direct = ds_has_predicate(ds, &o, &lowering);
        let via_type = ds_rdf_types(ds, &o)
            .iter()
            .any(|t| ds_has_predicate(ds, t, &lowering));
        if !direct && !via_type {
            report.push_error(
                codes::MATH_PROBABILITY_MISSING_MODEL_LOWERING,
                o.clone(),
                format!(
                    "math:MissingProbabilityModelLowering: reasoning request references probability \
                     model {o} with no declared logic: lowering (math:probabilityModelLowering)"
                ),
            );
        }
    }

    // Gate 4 — math:IncompleteDependencyModel: structural presence. A math:MarkovKernel
    // declares BOTH its domain and codomain; a math:BayesianNetwork declares its dependency
    // graph; a math:FactorGraph declares its dependency graph (the bipartite variable/factor
    // structure); a math:JointProbabilityTable tabulates at least one outcome. A model
    // missing any of these cannot fix a joint distribution.
    //
    // math:BayesianNetwork, math:FactorGraph, and math:MarkovKernel each carry
    // logic:preservationKind logic:ExactPreservation UNCONDITIONALLY at the TBox (class)
    // level (module.ttl) — that class-level declaration exists so the charter's "conditional"
    // lowering can be read off the class at all, not to license every instance as exact. An
    // instance that is structurally incomplete AND additionally declares
    // logic:ExactPreservation directly on ITSELF has made the honest-when-complete claim
    // explicit while failing the gate that would make it true: that is the overclaim Gate 5
    // (below) reports, not Gate 4's plain structural-incompleteness report, so Gate 4 skips
    // it here (`markov_kernel_is_complete` / `dependency_graph_is_complete` and
    // `declares_exact_preservation_directly` are shared verbatim with Gate 5 so the two gates
    // can never diverge on what "complete" or "declares exact" means).
    for k in ds_subjects_of_type(ds, &math_iri("MarkovKernel")) {
        if markov_kernel_is_complete(ds, &k) || declares_exact_preservation_directly(ds, &k) {
            continue;
        }
        let has_domain = ds_has_predicate(ds, &k, &math_iri("kernelDomain"));
        let has_codomain = ds_has_predicate(ds, &k, &math_iri("kernelCodomain"));
        let missing = match (has_domain, has_codomain) {
            (false, false) => "math:kernelDomain and math:kernelCodomain",
            (false, true) => "math:kernelDomain",
            (true, false) => "math:kernelCodomain",
            (true, true) => unreachable!(),
        };
        report.push_error(
            codes::MATH_PROBABILITY_INCOMPLETE_DEPENDENCY_MODEL,
            k.clone(),
            format!("math:IncompleteDependencyModel: Markov kernel {k} is missing {missing}"),
        );
    }
    for bn in ds_subjects_of_type(ds, &math_iri("BayesianNetwork")) {
        if dependency_graph_is_complete(ds, &bn) || declares_exact_preservation_directly(ds, &bn) {
            continue;
        }
        report.push_error(
            codes::MATH_PROBABILITY_INCOMPLETE_DEPENDENCY_MODEL,
            bn.clone(),
            format!(
                "math:IncompleteDependencyModel: Bayesian network {bn} declares no \
                 math:dependencyGraph"
            ),
        );
    }
    for fg in ds_subjects_of_type(ds, &math_iri("FactorGraph")) {
        if dependency_graph_is_complete(ds, &fg) || declares_exact_preservation_directly(ds, &fg) {
            continue;
        }
        report.push_error(
            codes::MATH_PROBABILITY_INCOMPLETE_DEPENDENCY_MODEL,
            fg.clone(),
            format!(
                "math:IncompleteDependencyModel: factor graph {fg} declares no \
                 math:dependencyGraph"
            ),
        );
    }
    for t in ds_subjects_of_type(ds, &math_iri("JointProbabilityTable")) {
        if ds_object_iris_sorted(ds, &t, &logic_iri("jointOutcome")).is_empty() {
            report.push_error(
                codes::MATH_PROBABILITY_INCOMPLETE_DEPENDENCY_MODEL,
                t.clone(),
                format!(
                    "math:IncompleteDependencyModel: joint probability table {t} has no tabulated \
                     outcomes (logic:jointOutcome)"
                ),
            );
        }
    }

    // Gate 5 — math:ExactPreservationViolated: a math:JointProbabilityTable declares
    // logic:ExactPreservation at the TBox level, so a tabulated instance whose outcome mass
    // does not sum to exactly one overclaims. Only tables WITH at least one outcome are in
    // scope here (the empty case is Gate 4's). An unreadable outcome probability skips the
    // whole table (no false positive).
    for t in ds_subjects_of_type(ds, &math_iri("JointProbabilityTable")) {
        let outcomes = ds_object_iris_sorted(ds, &t, &logic_iri("jointOutcome"));
        if outcomes.is_empty() {
            continue;
        }
        let mut sum = zero;
        let mut readable = true;
        for outcome in &outcomes {
            let prob = read_magnitude(ds, outcome, &[logic_iri("jointProbability")]);
            let Some(prob) = prob else {
                readable = false;
                break;
            };
            match sum.checked_add(prob) {
                Ok(s) => sum = s,
                Err(_) => {
                    readable = false;
                    break;
                }
            }
        }
        if readable && sum != one {
            report.push_error(
                codes::MATH_PROBABILITY_EXACT_PRESERVATION_VIOLATED,
                t.clone(),
                format!(
                    "math:ExactPreservationViolated: joint probability table {t} declares \
                     logic:ExactPreservation but its outcome mass sums to {}/{} \u{2260} 1",
                    sum.numerator(),
                    sum.denominator()
                ),
            );
        }
    }

    // Gate 5 (continued) — the "conditional" dependency models: math:BayesianNetwork,
    // math:FactorGraph, and math:MarkovKernel each declare logic:ExactPreservation
    // UNCONDITIONALLY at the class (TBox) level (module.ttl) — the lowering IS exact once
    // the model's completeness gate holds (Gate 4's `markov_kernel_is_complete` /
    // `dependency_graph_is_complete`), and IS NOT otherwise. An instance that is
    // structurally INCOMPLETE and additionally declares logic:ExactPreservation DIRECTLY on
    // itself has made that exactness claim explicit for a model that cannot honor it — the
    // preservation↔completeness overclaim math:ExactPreservationViolated. An incomplete
    // instance that does not itself declare logic:ExactPreservation is Gate 4's plain
    // structural-incompleteness report instead (skipped here to keep the two gates
    // mutually exclusive); a complete instance overclaims nothing regardless of what it
    // declares.
    for k in ds_subjects_of_type(ds, &math_iri("MarkovKernel")) {
        if markov_kernel_is_complete(ds, &k) || !declares_exact_preservation_directly(ds, &k) {
            continue;
        }
        report.push_error(
            codes::MATH_PROBABILITY_EXACT_PRESERVATION_VIOLATED,
            k.clone(),
            format!(
                "math:ExactPreservationViolated: Markov kernel {k} declares \
                 logic:ExactPreservation but is missing its declared domain or codomain \
                 (kernel totality cannot hold over an undeclared domain/codomain)"
            ),
        );
    }
    for bn in ds_subjects_of_type(ds, &math_iri("BayesianNetwork")) {
        if dependency_graph_is_complete(ds, &bn) || !declares_exact_preservation_directly(ds, &bn) {
            continue;
        }
        report.push_error(
            codes::MATH_PROBABILITY_EXACT_PRESERVATION_VIOLATED,
            bn.clone(),
            format!(
                "math:ExactPreservationViolated: Bayesian network {bn} declares \
                 logic:ExactPreservation but declares no math:dependencyGraph (DAG and CPT \
                 completeness cannot hold over an undeclared graph)"
            ),
        );
    }
    for fg in ds_subjects_of_type(ds, &math_iri("FactorGraph")) {
        if dependency_graph_is_complete(ds, &fg) || !declares_exact_preservation_directly(ds, &fg) {
            continue;
        }
        report.push_error(
            codes::MATH_PROBABILITY_EXACT_PRESERVATION_VIOLATED,
            fg.clone(),
            format!(
                "math:ExactPreservationViolated: factor graph {fg} declares \
                 logic:ExactPreservation but declares no math:dependencyGraph (finite \
                 normalized factors cannot hold over an undeclared factor structure)"
            ),
        );
    }
}

/// The `math:` projection-side invariants: the two join-requiring native gates over
/// `math:ProjectionRecord` loss-ledger carriers. Kept purely native (no SHACL target
/// shape) exactly like the four `lang:` projection gates, because each requires a join
/// the closed-world shape language cannot express. Runs bundle-wide (`GraphMatch::Any`).
fn check_math_projection_invariants(ds: &RdfDataset, report: &mut LintReport) {
    check_math_projection_confidence_as_probability(ds, report);
    check_math_projection_dropped_parameterization(ds, report);
}

/// `math:ProjectionConfidenceAsProbability` — a `math:ProjectionRecord` that declares it
/// converts a source confidence into a `math:ProbabilityValue`
/// (`math:projectsConfidenceAsProbability` true) MUST license that conversion with an
/// explicit `math:declaredConfidenceMapping`. A conversion with none erodes the `logic:`
/// probability/confidence boundary at the projection seam — the projection-side
/// counterpart of `math:ConfidenceAsProbability`.
fn check_math_projection_confidence_as_probability(ds: &RdfDataset, report: &mut LintReport) {
    let projects_confidence = math_iri("projectsConfidenceAsProbability");
    let declared_mapping = math_iri("declaredConfidenceMapping");
    for r in ds_subjects_of_type(ds, &math_iri("ProjectionRecord")) {
        // xsd:boolean also serializes as "1" (canonical is "true"/"false", but "0"/"1"
        // are valid lexical forms), so both are accepted.
        let converts = ds_object_literals(ds, &r, &projects_confidence)
            .iter()
            .any(|v| {
                let t = v.trim();
                t == "true" || t == "1"
            });
        if converts && !ds_has_predicate(ds, &r, &declared_mapping) {
            report.push_error(
                codes::MATH_PROJECTION_CONFIDENCE_AS_PROBABILITY,
                r.clone(),
                format!(
                    "math:ProjectionConfidenceAsProbability: projection {r} converts a confidence into \
                     a math:ProbabilityValue without a declared mapping (math:declaredConfidenceMapping)"
                ),
            );
        }
    }
}

/// `math:ProjectionDroppedParameterization` — a LOSSY `math:ProjectionRecord` (a declared
/// `logic:preservationKind` that is not `logic:ExactPreservation`) EACH of whose
/// `math:projectionSource` values is a `math:Distribution` carrying a
/// `math:distributionParameterization` MUST enumerate that parameterization among its
/// `logic:unsupportedConstruct` drops — as the parameterization IRI, or a string literal
/// naming it. A lossy projection that drops the parameterization without recording it has
/// performed the drop silently. Checked independently per `math:projectionSource` (a
/// record may declare several; the gate is not satisfied merely because the
/// alphabetically-first one is clean).
fn check_math_projection_dropped_parameterization(ds: &RdfDataset, report: &mut LintReport) {
    let preservation_kind = logic_iri("preservationKind");
    let exact = logic_iri("ExactPreservation");
    let projection_source = math_iri("projectionSource");
    let parameterization = math_iri("distributionParameterization");
    let unsupported = logic_iri("unsupportedConstruct");
    for r in ds_subjects_of_type(ds, &math_iri("ProjectionRecord")) {
        // Only a lossy projection can drop anything: it declares at least one preservation
        // kind and NONE of them is `logic:ExactPreservation`. An undeclared preservation
        // kind is out of scope here, never treated as lossy (no false positive).
        let kinds = ds_object_iris(ds, &r, &preservation_kind);
        if kinds.is_empty() || kinds.contains(&exact) {
            continue;
        }
        let sources = ds_object_iris_sorted(ds, &r, &projection_source);
        if sources.is_empty() {
            continue;
        }
        // The drop list: `logic:unsupportedConstruct` values, whether IRIs or string
        // literals (the property is a DatatypeProperty, but a drop may be recorded either
        // way, so the gate accepts both — the parameterization IRI, or a literal that
        // names the parameterization's local name as a whole token, not merely a
        // substring of some longer word).
        let drop_iris = ds_object_iris(ds, &r, &unsupported);
        let drop_literals: Vec<String> = ds_object_literals(ds, &r, &unsupported)
            .into_iter()
            .map(|d| d.to_lowercase())
            .collect();
        for src in &sources {
            if !ds_has_type(ds, src, &math_iri("Distribution")) {
                continue;
            }
            for param in ds_object_iris_sorted(ds, src, &parameterization) {
                let local = param
                    .rsplit(['/', '#'])
                    .next()
                    .unwrap_or(param.as_str())
                    .to_lowercase();
                let recorded = drop_iris.contains(&param)
                    || (!local.is_empty()
                        && drop_literals.iter().any(|d| {
                            d.split(|c: char| !c.is_alphanumeric())
                                .any(|tok| tok == local)
                        }));
                if !recorded {
                    report.push_error(
                        codes::MATH_PROJECTION_DROPPED_PARAMETERIZATION,
                        format!("{r}\t{param}"),
                        format!(
                            "math:ProjectionDroppedParameterization: lossy projection {r} drops \
                             the distribution parameterization of {src} without enumerating it \
                             in logic:unsupportedConstruct"
                        ),
                    );
                }
            }
        }
    }
}

/// Native twin of [`check_annotation_literal`].
fn ds_check_annotation_literal(
    subject: &str,
    predicate: &str,
    lexical: &str,
    language: Option<&str>,
    cfg: &LintConfig,
    internal_re: &Regex,
) -> Option<String> {
    let lang = language?;
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
        lit_repr = lang_literal_repr(lexical, lang),
    ))
}

/// Render a language-tagged literal the way [`literal_repr`] does:
/// `rdflib.term.Literal('value', lang='xx')`.
fn lang_literal_repr(lexical: &str, lang: &str) -> String {
    format!(
        "rdflib.term.Literal({value}, lang={lang})",
        value = py_str_repr(lexical),
        lang = py_str_repr(lang),
    )
}

/// Render a triple subject like [`subject_display`]: IRI → its IRI; blank → `_:b`.
fn ds_subject_display(subject: TermRef<'_>) -> String {
    match subject {
        TermRef::Iri(iri) => iri.to_owned(),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        // A triple-term subject (RDF 1.2) is not a normal lint subject; stringify.
        other => ds_object_display(other),
    }
}

/// A `Display`-style rendering of a non-IRI/non-blank object term, matching
/// oxigraph `Term`'s `Display` (N-Triples form) for the rare defensive non-IRI-value
/// diagnostic branches (`{other}` in the `Store` version). These paths never fire in
/// production — `gmeow:graphBoxRole`/consumer values are always IRIs — so this only
/// keeps the defensive arm faithful, never gates committed diagnostics.
fn ds_object_display(term: TermRef<'_>) -> String {
    match term {
        TermRef::Iri(iri) => format!("<{iri}>"),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        TermRef::Literal {
            lexical, language, ..
        } => match language {
            Some(lang) => format!("\"{lexical}\"@{lang}"),
            None => format!("\"{lexical}\""),
        },
        TermRef::Triple { .. } => "<<triple>>".to_owned(),
    }
}

/// The term-naming lint over a native [`RdfDataset`] (mirrors `term_naming_lint`):
/// a selector-privileging local name with no `gmeow:namingNote` justification is an
/// error. Error TEXT and emission order are byte-identical to the legacy `Store`
/// version.
pub fn term_naming_lint_dataset(ds: &RdfDataset, cfg: &LintConfig) -> LintReport {
    let mut report = LintReport::default();
    let naming_note = format!("{}namingNote", cfg.namespace);
    let typed = collect_typed_terms_dataset(ds, cfg);
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
        if ds_has_predicate(ds, term, &naming_note) {
            continue;
        }
        offending.sort();
        let first = offending[0];
        report.push_error(
            codes::NAMING_SELECTOR_TOKEN,
            term.clone(),
            format!(
                "{kind} gmeow:{local} carries the selector token '{first}' (Principle 9: co-equal \
                 claims have no primary/preferred/default/main); rename it, or justify a \
                 value-vocabulary use with gmeow:namingNote"
            ),
        );
    }
    report
}

/// The declared-term IRI set over a native [`RdfDataset`]
/// (`set(_collect_typed_terms(graph))`) — exposed for `guide_anchor_lint`'s anchor
/// resolution (which keeps its markdown logic in Python).
pub fn declared_terms_dataset(ds: &RdfDataset, cfg: &LintConfig) -> Vec<String> {
    collect_typed_terms_dataset(ds, cfg).into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::parse_dataset;
    use std::sync::Arc;

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

    fn store_from(ttl: &str) -> Arc<RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
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
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("skos:definition"))
        );
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
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report.errors().is_empty(), "errors: {:?}", report.errors());
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
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("missing gmeow:graphBoxRole"))
        );
    }

    #[test]
    fn structural_exempts_self_description_abox() {
        // A-Box individuals defined by the `self` self-description ontology are
        // project metadata, not vocabulary surface, so the per-term annotation /
        // graphBoxRole contract must not fire on them. The same individual
        // shape would be flagged if it were ordinary vocabulary.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:self#contribution-bii a gmeow:Contribution ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/self> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("contribution-bii")),
            "self-description A-Box must be exempt: {:?}",
            report.errors()
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
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("roleAuthor") && e.contains("graphBoxRole")),
            "ordinary vocabulary individual must still be linted: {:?}",
            report.errors()
        );
    }

    // A fully-annotated, slice-defined `gmeow:boxABox` role, mirroring its real
    // kernel definition. Generated A-Box subjects reference it; it stays on the
    // vocabulary tier (slice-defined) so it never pollutes assertional fixtures.
    const ABOX_ROLE: &str = "gmeow:boxABox a gmeow:GraphBoxRole ;\n\
           rdfs:label \"ABox role\" ;\n\
           skos:definition \"Assertional graph role.\" ;\n\
           rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/kernel> ;\n\
           gmeow:graphBoxRole ex:boxTBox .\n";

    #[test]
    fn structural_accepts_assertional_instance_without_definition() {
        // Generated A-Box payload (here a diagnostics Finding) anchored to its
        // named graph and self-declaring gmeow:boxABox is exempt from the
        // skos:definition requirement only — type, label, provenance, and a
        // valid box role are still present, so the subject is clean.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-0> a gmeow:Finding ;\n\
               rdfs:label \"SH001: example finding\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            !report.errors().iter().any(|e| e.contains("finding/abc-0")),
            "well-formed assertional instance must be clean: {:?}",
            report.errors()
        );
    }

    #[test]
    fn structural_flags_assertional_instance_missing_label() {
        // The assertional tier relaxes skos:definition, NOT the label — a
        // generated subject without rdfs:label is still under-specified.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-1> a gmeow:Finding ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("finding/abc-1") && e.contains("rdfs:label")),
            "assertional instance missing label must still error: {:?}",
            report.errors()
        );
    }

    #[test]
    fn structural_flags_assertional_instance_missing_box_role() {
        // Anchored to a graph but NOT self-declaring gmeow:boxABox: the
        // relaxation is not earned, so the full quartet applies and the missing
        // role (and definition) still fire.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-2> a gmeow:Finding ;\n\
               rdfs:label \"SH002: example finding\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("finding/abc-2") && e.contains("missing gmeow:graphBoxRole")),
            "assertional instance without boxABox must still error: {:?}",
            report.errors()
        );
    }

    #[test]
    fn structural_denies_relaxation_without_graph_provenance() {
        // Self-declares gmeow:boxABox and carries a label, but isDefinedBy points
        // at an arbitrary non-graph, non-slice IRI. "Not a slice" alone must NOT
        // earn the assertional relaxation — the skos:definition requirement still
        // applies, proving the relaxation is a positive, earned obligation.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-3> a gmeow:Finding ;\n\
               rdfs:label \"SH003: example finding\" ;\n\
               rdfs:isDefinedBy <https://example.org/somewhere> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("finding/abc-3") && e.contains("skos:definition")),
            "bogus provenance must not earn the relaxation: {:?}",
            report.errors()
        );
    }

    #[test]
    fn structural_keeps_slice_individual_on_vocabulary_tier() {
        // Branch-order invariant: a slice-defined individual that ALSO carries
        // gmeow:boxABox must stay on the vocabulary tier (slice check wins
        // first), so a missing skos:definition still fires.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             gmeow:sensitivityPublic a gmeow:SensitivityLevel ;\n\
               rdfs:label \"public\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/kernel> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("sensitivityPublic") && e.contains("skos:definition")),
            "slice individual must stay on the vocabulary tier: {:?}",
            report.errors()
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
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("not a gmeow:GraphBoxRole"))
        );
    }

    #[test]
    fn structural_accepts_mixed_case_private_tag() {
        let store = store_from(&format!(
            "{PREFIXES}\
             <https://example.org/name> gmeow:fullName \"Japanese\"@x-GMEOW-Japanese .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report.errors().is_empty(), "errors: {:?}", report.errors());
    }

    #[test]
    fn structural_rejects_external_tag_on_gmeow_predicate() {
        let store = store_from(&format!(
            "{PREFIXES}\
             <https://example.org/name> gmeow:fullName \"Japanese\"@ja .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("external or invalid language tag"))
        );
        // Exact rdflib repr framing.
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("literal rdflib.term.Literal('Japanese', lang='ja')"))
        );
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
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("external language tag 'en'") && e.contains("label"))
        );
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
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report.errors().is_empty(), "errors: {:?}", report.errors());
    }

    #[test]
    fn naming_lint_flags_primary_without_note() {
        let store = store_from(&format!("{PREFIXES}gmeow:PrimaryThing a owl:Class .\n"));
        let report = term_naming_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("selector token 'primary'"))
        );
    }

    #[test]
    fn naming_lint_respects_naming_note() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:sourceTierPrimary a owl:Class ;\n\
               gmeow:namingNote \"value vocabulary\" .\n"
        ));
        let report = term_naming_lint_dataset(&store, &cfg());
        assert!(report.errors().is_empty(), "errors: {:?}", report.errors());
    }

    #[test]
    fn collect_typed_terms_resolves_multityped() {
        // A subject typed as both Class and Individual resolves to class (lower rank).
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Thing a owl:Class , gmeow:SomeIndividualType .\n"
        ));
        let terms = collect_typed_terms_dataset(&store, &cfg());
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

    /// Parse a Turtle fixture into a frozen native dataset (no oxigraph round-trip).
    fn dataset_from(ttl: &str) -> std::sync::Arc<purrdf::RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
    }

    /// The native `structural_lint_dataset` twin must produce byte-identical
    /// errors/warnings to the `Store` version across a battery of fixtures.
    #[test]
    fn native_structural_lint_parity_with_store() {
        let fixtures = [
            // missing definition
            format!(
                "{PREFIXES}\
                 gmeow:Undocumented a owl:Class ;\n\
                   rdfs:label \"x\" ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> ;\n\
                   rdfs:subClassOf owl:Thing .\n"
            ),
            // well-formed (clean)
            format!(
                "{PREFIXES}{ROLE}\
                 gmeow:Documented a owl:Class ;\n\
                   rdfs:label \"Documented\" ;\n\
                   skos:definition \"A well-formed term.\" ;\n\
                   gmeow:graphBoxRole ex:boxTBox ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
            ),
            // external tag on gmeow predicate + en on label
            format!(
                "{PREFIXES}{ROLE}\
                 gmeow:TestTerm a owl:Class ;\n\
                   rdfs:label \"Name\"@en ;\n\
                   skos:definition \"A test term.\" ;\n\
                   gmeow:graphBoxRole ex:boxTBox ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n\
                 <https://example.org/name> gmeow:fullName \"Japanese\"@ja .\n"
            ),
            // untyped graphBoxRole
            format!(
                "{PREFIXES}\
                 gmeow:Documented a owl:Class ;\n\
                   rdfs:label \"Documented\" ;\n\
                   skos:definition \"A well-formed term.\" ;\n\
                   gmeow:graphBoxRole ex:notARole ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
            ),
        ];
        for ttl in &fixtures {
            let store = structural_lint_dataset(&store_from(ttl), &cfg());
            let native = structural_lint_dataset(&dataset_from(ttl), &cfg());
            // Compare as sorted sets: the per-check emission order differs between
            // oxigraph's `store.iter()` and the native freeze-sorted scan (Check 6,
            // the whole-graph language-tag pass), but the SET of diagnostics must be
            // identical. Downstream the report is normalized via `Finding::sort_key`
            // (`Report::normalize`), so committed bytes never depend on this order.
            let (mut se, mut ne) = (store.errors().clone(), native.errors().clone());
            se.sort();
            ne.sort();
            assert_eq!(se, ne, "errors diverged for: {ttl}");
            let (mut sw, mut nw) = (store.warnings().clone(), native.warnings().clone());
            sw.sort();
            nw.sort();
            assert_eq!(sw, nw, "warnings diverged for: {ttl}");
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

    // --- lang: meaning-stratum native gates ---------------------------------- #

    const LANG_PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix ex: <https://example.org/> .\n";

    #[test]
    fn undeclared_lowering_flags_bridge_denotation_without_preservation() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationKind lang:denotesLogicFormula ;\n\
               lang:denotedForm ex:f ;\n\
               lang:denotationTarget ex:formula ;\n\
               lang:denotationContext ex:ctx .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UndeclaredLoweringStage")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn undeclared_lowering_clean_when_preservation_declared() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationKind lang:denotesLogicFormula ;\n\
               lang:denotedForm ex:f ;\n\
               lang:denotationTarget ex:formula ;\n\
               lang:denotationContext ex:ctx ;\n\
               logic:preservationKind logic:ExactPreservation .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UndeclaredLoweringStage")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn silent_disambiguation_flags_ungrounded_resolution() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:producedReading ex:r1 , ex:r2 ;\n\
               lang:resolvedReading ex:r1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentDisambiguation")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn silent_disambiguation_clean_when_resolution_is_vantage_held() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:producedReading ex:r1 , ex:r2 ;\n\
               lang:resolvedReading ex:r1 .\n\
             ex:obs lang:aboutReading ex:r1 ;\n\
               gmeow:vantage ex:annotator .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentDisambiguation")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn gmn_compaction_silent_disambiguation_fixture_fires_exactly_that_gate() {
        // The GMN counter-example reuses the EXISTING bundle-wide
        // lang:SilentDisambiguation discipline verbatim: a compaction run whose
        // interpretation act collapses two co-resident readings to one with no
        // vantage-held observation. Like projection-silent-disambiguation.ttl it is
        // a native-gate fixture (no SHACL cell), so this test is its executable pin:
        // it fires lang:SilentDisambiguation and NO other lang: failure class — the
        // compaction record itself is deliberately well-formed.
        let ds = dataset_from(include_str!(
            "../../../slices/grounding/lang/tests/counter-examples/gmn-compaction-silent-disambiguation.ttl"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        let errors = report.errors();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("lang:SilentDisambiguation")),
            "fixture must fire lang:SilentDisambiguation: {errors:?}",
        );
        assert_eq!(
            errors.iter().filter(|e| e.contains("lang:")).count(),
            1,
            "fixture must isolate exactly the silent collapse: {errors:?}",
        );
    }

    #[test]
    fn one_way_bridge_flags_logic_subject_with_lang_predicate() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             logic:someObject lang:denotedForm ex:f .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("one-way bridge violated")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn one_way_bridge_clean_for_lang_to_logic_target() {
        // The lawful direction: a lang: denotation targeting a logic: object.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationTarget logic:someFormula .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("one-way bridge violated")),
            "errors: {:?}",
            report.errors()
        );
    }

    // --- lang: ingestion-stratum native gates -------------------------------- #

    #[test]
    fn unattributed_engine_claim_flags_engine_reading_without_vantage() {
        // An engine run (lang:interpretationEngine present) whose produced reading
        // carries no gmeow:vantage — engine output entered as unattributed structure.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:interpretationEngine ex:udParser ;\n\
               lang:producedReading ex:r1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnattributedEngineClaim")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn unattributed_engine_claim_clean_when_reading_is_vantage_held() {
        // The lawful engine handoff: each produced reading carries the engine's vantage.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:interpretationEngine ex:udParser ;\n\
               lang:producedReading ex:r1 .\n\
             ex:r1 a lang:Reading ; gmeow:vantage ex:udVantage .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnattributedEngineClaim")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn unattributed_engine_claim_ignores_non_engine_act() {
        // A manual/compositional act (no lang:interpretationEngine) may lawfully leave
        // a co-resident reading unclaimed — the gate must NOT fire on it.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:producedReading ex:r1 , ex:r2 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnattributedEngineClaim")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn silent_promotion_flags_promotion_without_editorial_act() {
        // A bare subject promotes a reading with no provenance-carrying act.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:slice lang:promotedReading ex:r1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentPromotion")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn silent_promotion_clean_when_promotion_is_a_vantage_held_activity() {
        // The lawful promotion: an explicit editorial gmeow:Activity carrying a vantage.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:promote a gmeow:Activity ;\n\
               gmeow:vantage ex:editor ;\n\
               lang:promotedReading ex:r1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentPromotion")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn silent_promotion_flags_activity_missing_vantage() {
        // An activity that promotes but carries no vantage is still a silent promotion.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:promote a gmeow:Activity ;\n\
               lang:promotedReading ex:r1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentPromotion")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn ingestion_counter_example_fixtures_fire_exactly_their_class() {
        // The slice-resident counter-examples for the two native ingestion gates each
        // fire exactly their named failure class (and nothing from the other gate),
        // so the (fixture, class) pair is load-bearing rather than decorative.
        let unattributed = include_str!(
            "../../../slices/grounding/lang/tests/counter-examples/ingestion-unattributed-engine-claim.ttl"
        );
        let report = structural_lint_dataset(&dataset_from(unattributed), &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnattributedEngineClaim")),
            "errors: {:?}",
            report.errors()
        );
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentPromotion")),
            "the unattributed-engine fixture must not also fire SilentPromotion: {:?}",
            report.errors()
        );

        let promotion = include_str!(
            "../../../slices/grounding/lang/tests/counter-examples/ingestion-silent-promotion.ttl"
        );
        let report = structural_lint_dataset(&dataset_from(promotion), &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentPromotion")),
            "errors: {:?}",
            report.errors()
        );
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnattributedEngineClaim")),
            "the silent-promotion fixture keeps its reading vantage-held: {:?}",
            report.errors()
        );
    }

    #[test]
    fn ambiguity_positive_fixture_is_clean_under_the_native_gates() {
        // Gate 5 (positive): the co-resident-readings fixture — an engine act producing
        // TWO vantage-held readings with NO resolved winner — trips none of the lang:
        // native gates.
        let fixture = include_str!(
            "../../../slices/grounding/lang/tests/conformance-fixtures/ambiguity-saw-her-duck.ttl"
        );
        let report = structural_lint_dataset(&dataset_from(fixture), &cfg());
        let report_errors = report.errors();
        let lang_errors: Vec<&String> = report_errors
            .iter()
            .filter(|e| e.contains("lang:") || e.contains("one-way bridge"))
            .collect();
        assert!(
            lang_errors.is_empty(),
            "the ambiguity fixture must be clean under the native lang: gates: {lang_errors:?}"
        );
    }

    // ---- lang: projection-stratum native gates (the lossy-lowering contract) ----

    #[test]
    fn projection_missing_preservation_kind_fires() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexeme .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:MissingPreservationKind")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_missing_preservation_kind_clean_when_declared() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexeme ;\n\
               logic:preservationKind logic:ExactPreservation .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:MissingPreservationKind")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_undeclared_unsupported_fires() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:grammarSrc a lang:Grammar .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"EBNF\" ;\n\
               lang:projectsSource ex:grammarSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UndeclaredUnsupportedConstruct")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_undeclared_unsupported_clean_when_enumerated() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:grammarSrc a lang:Grammar .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"EBNF\" ;\n\
               lang:projectsSource ex:grammarSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"left-recursion\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UndeclaredUnsupportedConstruct")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_unrecorded_epistemic_loss_fires() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:lexemeSrc a lang:Lexeme ;\n\
               gmeow:vantage ex:annotatorVantage .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexemeSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"inflection-tables\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnrecordedEpistemicLoss")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_unrecorded_epistemic_loss_clean_when_stratum_named() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:lexemeSrc a lang:Lexeme ;\n\
               gmeow:vantage ex:annotatorVantage .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexemeSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"vantage-held-readings\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnrecordedEpistemicLoss")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_unrecorded_epistemic_loss_fires_when_only_one_of_many_strata_named() {
        // A source flattening TWO strata (vantage + interpretation) whose emission names only
        // ONE (vantage) leaves interpretation silently unrecorded — the gate must fire. Under
        // the earlier `any` semantics this escaped; `all` closes it.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:lexemeSrc a lang:Lexeme, lang:InterpretationAct ;\n\
               gmeow:vantage ex:annotatorVantage .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexemeSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"vantage-flattened\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnrecordedEpistemicLoss")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_unrecorded_epistemic_loss_clean_when_all_strata_named() {
        // The same two-stratum source is clean only when the emission names BOTH strata.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:lexemeSrc a lang:Lexeme, lang:InterpretationAct ;\n\
               gmeow:vantage ex:annotatorVantage .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexemeSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"vantage-flattened\" ;\n\
               lang:unsupportedConstruct \"interpretation-act-dropped\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:UnrecordedEpistemicLoss")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_silent_disambiguation_fires() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:form a lang:ComposedForm .\n\
             ex:r1 a lang:Reading ; lang:readingOf ex:form .\n\
             ex:r2 a lang:Reading ; lang:readingOf ex:form .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"CoNLL-U\" ;\n\
               lang:projectsSource ex:form ;\n\
               logic:preservationKind logic:ExactPreservation ;\n\
               lang:emittedReadingCount 1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:ProjectionSilentDisambiguation")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_silent_disambiguation_clean_when_all_readings_emitted() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:form a lang:ComposedForm .\n\
             ex:r1 a lang:Reading ; lang:readingOf ex:form .\n\
             ex:r2 a lang:Reading ; lang:readingOf ex:form .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"CoNLL-U\" ;\n\
               lang:projectsSource ex:form ;\n\
               logic:preservationKind logic:ExactPreservation ;\n\
               lang:emittedReadingCount 2 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:ProjectionSilentDisambiguation")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_exact_preservation_violated_fires() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:grammar a lang:Grammar .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"GTS-grammar-surface\" ;\n\
               lang:projectsSource ex:grammar ;\n\
               logic:preservationKind logic:ExactPreservation ;\n\
               lang:roundTripHolds false .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:ExactPreservationViolated")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_exact_preservation_violated_clean_when_round_trip_holds() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:grammar a lang:Grammar .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"GTS-grammar-surface\" ;\n\
               lang:projectsSource ex:grammar ;\n\
               logic:preservationKind logic:ExactPreservation ;\n\
               lang:roundTripHolds true .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:ExactPreservationViolated")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn projection_counter_example_fixtures_fire_exactly_their_class() {
        // The four native-gate projection counter-examples each fire exactly their named
        // failure class (and no OTHER projection class), so each (fixture, class) pair is
        // load-bearing. (The MissingPreservationKind fixture rides the SHACL harness and is
        // covered by its inline native test above.)
        let cases: [(&str, &str, [&str; 3]); 4] = [
            (
                include_str!(
                    "../../../slices/grounding/lang/tests/counter-examples/projection-undeclared-unsupported.ttl"
                ),
                "lang:UndeclaredUnsupportedConstruct",
                [
                    "lang:UnrecordedEpistemicLoss",
                    "lang:ProjectionSilentDisambiguation",
                    "lang:ExactPreservationViolated",
                ],
            ),
            (
                include_str!(
                    "../../../slices/grounding/lang/tests/counter-examples/projection-unrecorded-epistemic-loss.ttl"
                ),
                "lang:UnrecordedEpistemicLoss",
                [
                    "lang:UndeclaredUnsupportedConstruct",
                    "lang:ProjectionSilentDisambiguation",
                    "lang:ExactPreservationViolated",
                ],
            ),
            (
                include_str!(
                    "../../../slices/grounding/lang/tests/counter-examples/projection-silent-disambiguation.ttl"
                ),
                "lang:ProjectionSilentDisambiguation",
                [
                    "lang:UndeclaredUnsupportedConstruct",
                    "lang:UnrecordedEpistemicLoss",
                    "lang:ExactPreservationViolated",
                ],
            ),
            (
                include_str!(
                    "../../../slices/grounding/lang/tests/counter-examples/projection-exact-preservation-violated.ttl"
                ),
                "lang:ExactPreservationViolated",
                [
                    "lang:UndeclaredUnsupportedConstruct",
                    "lang:UnrecordedEpistemicLoss",
                    "lang:ProjectionSilentDisambiguation",
                ],
            ),
        ];
        for (ttl, expected, forbidden) in cases {
            let report = structural_lint_dataset(&dataset_from(ttl), &cfg());
            assert!(
                report.errors().iter().any(|e| e.contains(expected)),
                "fixture must fire {expected}: {:?}",
                report.errors()
            );
            // The MissingPreservationKind gate must also stay silent (every fixture declares
            // its preservation kind).
            assert!(
                !report
                    .errors()
                    .iter()
                    .any(|e| e.contains("lang:MissingPreservationKind")),
                "fixture for {expected} declares a preservation kind: {:?}",
                report.errors()
            );
            for other in forbidden {
                assert!(
                    !report.errors().iter().any(|e| e.contains(other)),
                    "fixture for {expected} must not also fire {other}: {:?}",
                    report.errors()
                );
            }
        }
    }

    /// The five native probability-layer failure classes the isolation test polices.
    const MATH_PROBABILITY_CLASSES: [&str; 5] = [
        "math:ProbabilityOutOfBounds",
        "math:DistributionParameterConstraint",
        "math:MissingProbabilityModelLowering",
        "math:IncompleteDependencyModel",
        "math:ExactPreservationViolated",
    ];

    #[test]
    fn math_probability_counter_examples_fire_exactly_their_class() {
        // Each native-gate probability counter-example fires EXACTLY its named failure
        // class (and none of the other four probability classes), so each (fixture, class)
        // pair is load-bearing. All three distribution-parameter counter-examples fire the
        // shared math:DistributionParameterConstraint class (positivity arm vs the
        // absolute-dimension arm vs the relational — same-as/square-of — dimension arm).
        let cases: [(&str, &str); 12] = [
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/probability-out-of-bounds.ttl"
                ),
                "math:ProbabilityOutOfBounds",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/probability-out-of-bounds-signed.ttl"
                ),
                "math:ProbabilityOutOfBounds",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/distribution-parameter-negative.ttl"
                ),
                "math:DistributionParameterConstraint",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/distribution-parameter-wrong-dimension.ttl"
                ),
                "math:DistributionParameterConstraint",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/distribution-parameter-relational-dimension.ttl"
                ),
                "math:DistributionParameterConstraint",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/missing-probability-model-lowering.ttl"
                ),
                "math:MissingProbabilityModelLowering",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/incomplete-dependency-model.ttl"
                ),
                "math:IncompleteDependencyModel",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/factor-graph-incomplete.ttl"
                ),
                "math:IncompleteDependencyModel",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/exact-preservation-violated.ttl"
                ),
                "math:ExactPreservationViolated",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/bayesian-network-exact-preservation-violated.ttl"
                ),
                "math:ExactPreservationViolated",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/factor-graph-exact-preservation-violated.ttl"
                ),
                "math:ExactPreservationViolated",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/markov-kernel-exact-preservation-violated.ttl"
                ),
                "math:ExactPreservationViolated",
            ),
        ];
        for (ttl, expected) in cases {
            let report = structural_lint_dataset(&dataset_from(ttl), &cfg());
            assert!(
                report.errors().iter().any(|e| e.contains(expected)),
                "fixture must fire {expected}: {:?}",
                report.errors()
            );
            for other in MATH_PROBABILITY_CLASSES {
                if other == expected {
                    continue;
                }
                assert!(
                    !report.errors().iter().any(|e| e.contains(other)),
                    "fixture for {expected} must not also fire {other}: {:?}",
                    report.errors()
                );
            }
        }
    }

    #[test]
    fn math_probability_clean_fixtures_fire_no_probability_class() {
        // Each clean conformance fixture is the positive counterpart of one counter-example
        // and MUST raise none of the five native probability failure classes.
        let clean: [&str; 11] = [
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/probability-in-bounds.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/distribution-parameter-positive.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/distribution-parameter-right-dimension.ttl"
            ),
            // Positive counterpart of counter-examples/distribution-parameter-relational-dimension.ttl
            // for the RELATIONAL (math:sameAsRandomVariableDimension) dimension arm: the random
            // variable and its location parameter's quantity both carry the resolvable
            // math:lengthDimension, so the ℚ⁷ exact comparison agrees and raises nothing.
            include_str!(
                "../../../slices/grounding/math/tests/fixtures/random-variable-distribution.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/probability-model-lowering-declared.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/dependency-model-complete.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/factor-graph-complete.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/joint-table-mass-one.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/bayesian-network-exact-complete.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/factor-graph-exact-complete.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/markov-kernel-exact-complete.ttl"
            ),
        ];
        for ttl in clean {
            let report = structural_lint_dataset(&dataset_from(ttl), &cfg());
            for class in MATH_PROBABILITY_CLASSES {
                assert!(
                    !report.errors().iter().any(|e| e.contains(class)),
                    "clean fixture must not fire {class}: {:?}",
                    report.errors()
                );
            }
        }
    }

    /// The two native projection-side failure classes the isolation test polices.
    const MATH_PROJECTION_CLASSES: [&str; 2] = [
        "math:ProjectionConfidenceAsProbability",
        "math:ProjectionDroppedParameterization",
    ];

    #[test]
    fn math_projection_counter_examples_fire_exactly_their_class() {
        // Each projection-side counter-example fires EXACTLY its named failure class (and
        // not the other projection class), so each (fixture, class) pair is load-bearing.
        let cases: [(&str, &str); 2] = [
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/projection-confidence-as-probability.ttl"
                ),
                "math:ProjectionConfidenceAsProbability",
            ),
            (
                include_str!(
                    "../../../slices/grounding/math/tests/counter-examples/projection-dropped-parameterization.ttl"
                ),
                "math:ProjectionDroppedParameterization",
            ),
        ];
        for (ttl, expected) in cases {
            let report = structural_lint_dataset(&dataset_from(ttl), &cfg());
            assert!(
                report.errors().iter().any(|e| e.contains(expected)),
                "fixture must fire {expected}: {:?}",
                report.errors()
            );
            for other in MATH_PROJECTION_CLASSES {
                if other == expected {
                    continue;
                }
                assert!(
                    !report.errors().iter().any(|e| e.contains(other)),
                    "fixture for {expected} must not also fire {other}: {:?}",
                    report.errors()
                );
            }
        }
    }

    #[test]
    fn math_projection_clean_fixtures_fire_no_projection_class() {
        // Each clean conformance fixture is the positive counterpart of one projection
        // counter-example and MUST raise neither projection failure class.
        let clean: [&str; 2] = [
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/projection-confidence-mapping-declared.ttl"
            ),
            include_str!(
                "../../../slices/grounding/math/tests/conformance-fixtures/projection-parameterization-recorded.ttl"
            ),
        ];
        for ttl in clean {
            let report = structural_lint_dataset(&dataset_from(ttl), &cfg());
            for class in MATH_PROJECTION_CLASSES {
                assert!(
                    !report.errors().iter().any(|e| e.contains(class)),
                    "clean fixture must not fire {class}: {:?}",
                    report.errors()
                );
            }
        }
    }

    /// Prefixes for inline math: probability unit fixtures.
    const MATH_PROB_PREFIXES: &str = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix ex: <http://example.org/math/> .\n";

    #[test]
    fn probability_value_at_boundaries_is_clean() {
        // A math:ProbabilityValue at exactly 0 and exactly 1 sits on the closed interval's
        // boundary — the gate is inclusive, so neither fires math:ProbabilityOutOfBounds.
        let ds = dataset_from(&format!(
            "{MATH_PROB_PREFIXES}\
             ex:zero a math:ProbabilityValue ; gmeow:quantityValue \"0\" .\n\
             ex:one a math:ProbabilityValue ; gmeow:quantityValue \"1.0\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("math:ProbabilityOutOfBounds")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn rational_value_three_halves_fires_out_of_bounds() {
        // A math:ProbabilityValue carried as an exact math:RationalValue 3/2 is read from
        // its numerator/denominator pair (never a decimal) and exceeds 1.
        let ds = dataset_from(&format!(
            "{MATH_PROB_PREFIXES}\
             ex:p a math:ProbabilityValue , math:RationalValue ;\n\
               math:numerator 3 ; math:denominator 2 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report.errors().iter().any(|e| e
                .contains("math:ProbabilityOutOfBounds: probability value")
                && e.contains("3/2")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn exact_rational_half_plus_half_sums_to_one_clean() {
        // 1/2 + 1/2 = 1 exactly, so a joint table with that mass does not overclaim.
        let ds = dataset_from(&format!(
            "{MATH_PROB_PREFIXES}\
             ex:t a math:JointProbabilityTable ; logic:jointOutcome ex:a , ex:b .\n\
             ex:a logic:jointProbability 0.5 .\n\
             ex:b logic:jointProbability 0.5 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("math:ExactPreservationViolated")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn joint_mass_zero_point_nine_fires_exact_preservation() {
        // 0.5 + 0.4 = 0.9 ≠ 1 (exact-rational), so the table overclaims exact preservation.
        let ds = dataset_from(&format!(
            "{MATH_PROB_PREFIXES}\
             ex:t a math:JointProbabilityTable ; logic:jointOutcome ex:a , ex:b .\n\
             ex:a logic:jointProbability 0.5 .\n\
             ex:b logic:jointProbability 0.4 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("math:ExactPreservationViolated") && e.contains("9/10")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn decimal_to_rational_parses_plain_decimals_and_rejects_scientific() {
        assert_eq!(decimal_to_rational("1.5"), Rational::new(3, 2).ok());
        assert_eq!(decimal_to_rational("-1"), Rational::new(-1, 1).ok());
        assert_eq!(decimal_to_rational("0.72"), Rational::new(18, 25).ok());
        assert_eq!(decimal_to_rational("0"), Rational::new(0, 1).ok());
        assert_eq!(decimal_to_rational("1e3"), None);
        assert_eq!(decimal_to_rational("1.2E4"), None);
        assert_eq!(decimal_to_rational("abc"), None);
        assert_eq!(decimal_to_rational(""), None);
    }

    #[test]
    fn surface_leak_flags_crossing_carrying_surface_predicate() {
        // A translation unit that inlines surface-stratum material (lang:inScript)
        // as identity input rather than referencing structural forms.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:tu a lang:TranslationUnit ;\n\
               lang:translationSource ex:srcForm ;\n\
               lang:translationTarget ex:tgtForm ;\n\
               lang:inScript ex:latinScript .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SurfaceLeakInContentKey")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn surface_leak_clean_when_crossing_references_structural_forms() {
        // A well-formed crossing over structural forms, with no surface-stratum
        // predicate carried directly on the crossing itself.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:tu a lang:TranslationUnit ;\n\
               lang:translationSource ex:srcForm ;\n\
               lang:translationTarget ex:tgtForm ;\n\
               lang:translationCorrespondence ex:corr .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SurfaceLeakInContentKey")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn rendering_as_identity_flags_sameas_to_rendered_content() {
        // A rendering asserted owl:sameAs its own renderedContent — the rendering
        // becoming identity.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:r a lang:Rendering ;\n\
               lang:renderedContent ex:content ;\n\
               lang:renderingForm ex:form ;\n\
               owl:sameAs ex:content .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:RenderingAsIdentity")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn rendering_as_identity_clean_when_form_and_content_are_distinct() {
        // A rendering that names distinct content and form and asserts no identity
        // between the rendering and its content.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:r a lang:Rendering ;\n\
               lang:renderedContent ex:content ;\n\
               lang:renderingForm ex:form .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:RenderingAsIdentity")),
            "errors: {:?}",
            report.errors()
        );
    }

    // --- lang: form-stratum native gates (blob-by-reference + slot contiguity) - #

    #[test]
    fn inline_blob_payload_flags_document_scale_surface_text() {
        // A lang:SurfaceForm whose inline lang:surfaceText exceeds the document-scale
        // threshold — payload folded inline instead of held by reference.
        let big = "x".repeat(DOCUMENT_SCALE_BYTES + 1);
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:s a lang:SurfaceForm , lang:UnanalyzedProse ;\n\
               lang:surfaceText \"{big}\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:InlineBlobPayload")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn inline_blob_payload_clean_for_small_surface_and_for_blob_reference() {
        // A small inline surface stays inline (clean); a document-scale surface holding
        // its bytes by reference (lang:surfaceBlob, no inline lang:surfaceText) is also
        // clean — the gate flags only inline document-scale payload.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:small a lang:SurfaceForm , lang:UnanalyzedProse ;\n\
               lang:surfaceText \"cats chase mice\" .\n\
             ex:doc a lang:SurfaceForm , lang:UnanalyzedProse ;\n\
               lang:surfaceBlob \"blake3:deadbeef\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:InlineBlobPayload")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn noncontiguous_slots_flags_internal_gap() {
        // A composed form with slot indexes 0, 1, 3 — an internal gap at 2.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:cf a lang:ComposedForm ; lang:formSlot ex:s0 , ex:s1 , ex:s3 .\n\
             ex:s0 a lang:FormSlot ; lang:slotIndex 0 .\n\
             ex:s1 a lang:FormSlot ; lang:slotIndex 1 .\n\
             ex:s3 a lang:FormSlot ; lang:slotIndex 3 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:NonContiguousSlots")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn noncontiguous_slots_clean_for_zero_based_contiguous() {
        // A composed form with zero-based contiguous slot indexes 0, 1.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:cf a lang:ComposedForm ; lang:formSlot ex:s0 , ex:s1 .\n\
             ex:s0 a lang:FormSlot ; lang:slotIndex 0 .\n\
             ex:s1 a lang:FormSlot ; lang:slotIndex 1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:NonContiguousSlots")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn silent_ingest_drop_flags_surface_in_limbo() {
        // A lang:SurfaceForm that neither realizes a form nor is typed UnanalyzedProse.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:s a lang:SurfaceForm ;\n\
               lang:surfaceText \"cats chase mice\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentIngestDrop")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn silent_ingest_drop_clean_when_realizes_or_unanalyzed() {
        // Either honest analysis status clears the gate: a surface that realizes an
        // analyzed form, and a surface explicitly typed unanalyzed prose.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:s1 a lang:SurfaceForm ; lang:realizes ex:form .\n\
             ex:s2 a lang:SurfaceForm , lang:UnanalyzedProse ;\n\
               lang:surfaceText \"cats chase mice\" .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentIngestDrop")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn form_and_ingest_counter_example_fixtures_fire_exactly_their_class() {
        // Each slice-resident counter-example for the native form/ingestion gates fires
        // exactly its named failure class (and none of the sibling classes), so the
        // (fixture, class) pair is load-bearing. The blob-payload gate reuses slot-gap.ttl
        // for contiguity — the shipped non-contiguous (0, 1, 3) counter-example.
        let inline_blob = include_str!(
            "../../../slices/grounding/lang/tests/counter-examples/surface-inline-blob-payload.ttl"
        );
        let report = structural_lint_dataset(&dataset_from(inline_blob), &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:InlineBlobPayload")),
            "errors: {:?}",
            report.errors()
        );
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentIngestDrop")
                    || e.contains("lang:NonContiguousSlots")),
            "the inline-blob fixture must fire only lang:InlineBlobPayload: {:?}",
            report.errors()
        );

        let gap =
            include_str!("../../../slices/grounding/lang/tests/counter-examples/slot-gap.ttl");
        let report = structural_lint_dataset(&dataset_from(gap), &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:NonContiguousSlots")),
            "errors: {:?}",
            report.errors()
        );
        assert!(
            !report.errors().iter().any(
                |e| e.contains("lang:InlineBlobPayload") || e.contains("lang:SilentIngestDrop")
            ),
            "the slot-gap fixture must fire only lang:NonContiguousSlots: {:?}",
            report.errors()
        );

        let drop = include_str!(
            "../../../slices/grounding/lang/tests/counter-examples/ingest-silent-drop.ttl"
        );
        let report = structural_lint_dataset(&dataset_from(drop), &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("lang:SilentIngestDrop")),
            "errors: {:?}",
            report.errors()
        );
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:InlineBlobPayload")
                    || e.contains("lang:NonContiguousSlots")),
            "the silent-drop fixture must fire only lang:SilentIngestDrop: {:?}",
            report.errors()
        );
    }

    #[test]
    fn form_and_ingest_positive_controls_are_clean() {
        // The shipped conforming fixtures clear the native form/ingestion gates: a
        // zero-based contiguous composed form, and a raw surface typed unanalyzed prose.
        for fixture in [
            include_str!(
                "../../../slices/grounding/lang/tests/conformance-fixtures/slot-contiguous.ttl"
            ),
            include_str!(
                "../../../slices/grounding/lang/tests/conformance-fixtures/surface-analyzed.ttl"
            ),
        ] {
            let report = structural_lint_dataset(&dataset_from(fixture), &cfg());
            let report_errors = report.errors();
            let hits: Vec<&String> = report_errors
                .iter()
                .filter(|e| {
                    e.contains("lang:InlineBlobPayload")
                        || e.contains("lang:NonContiguousSlots")
                        || e.contains("lang:SilentIngestDrop")
                })
                .collect();
            assert!(
                hits.is_empty(),
                "positive control must clear the native form/ingestion gates: {hits:?}"
            );
        }
    }

    // --- math: measure-and-dimension reasoned gate --------------------------- #

    const MATH_PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix ex: <https://example.org/> .\n";

    /// A quantity of pure time (dimension T), used across the homogeneity tests.
    const TIME_QUANTITIES: &str = "ex:t1 a math:Quantity ; math:hasDimension math:timeDimension .\n\
         ex:t2 a math:Quantity ; math:hasDimension math:timeDimension .\n\
         ex:len a math:Quantity ; math:hasDimension math:lengthDimension .\n";

    fn has_inhomogeneity(report: &LintReport) -> bool {
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:DimensionalInhomogeneity"))
    }

    #[test]
    fn homogeneous_expression_is_clean() {
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{TIME_QUANTITIES}\
             ex:sum a math:DimensionalExpression ;\n\
               math:homogeneousOperand ex:t1 , ex:t2 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(!has_inhomogeneity(&report), "errors: {:?}", report.errors());
    }

    #[test]
    fn inhomogeneous_expression_is_flagged() {
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{TIME_QUANTITIES}\
             ex:bad a math:DimensionalExpression ;\n\
               math:homogeneousOperand ex:t1 , ex:len .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(has_inhomogeneity(&report), "errors: {:?}", report.errors());
    }

    /// The integrand/measure parameter slots compose to the integral's result
    /// dimension — proving the gate is wired across a *parameter*, not just an
    /// addition. Energy = ∫ (energy-density) d(volume): M·L⁻¹·T⁻² times L³ = M·L²·T⁻².
    const ENERGY_INTEGRAL: &str = "\
         ex:energyDim a math:DerivedDimension ;\n\
           math:baseDimensionExponent ex:mE1 , ex:lE2 , ex:tEm2 .\n\
         ex:mE1 a math:DimensionExponent ; math:exponentOfDimension math:massDimension ;\n\
           math:exponentNumerator 1 ; math:exponentDenominator 1 .\n\
         ex:lE2 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
           math:exponentNumerator 2 ; math:exponentDenominator 1 .\n\
         ex:tEm2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
           math:exponentNumerator -2 ; math:exponentDenominator 1 .\n\
         ex:densityDim a math:DerivedDimension ;\n\
           math:baseDimensionExponent ex:mD1 , ex:lDm1 , ex:tDm2 .\n\
         ex:mD1 a math:DimensionExponent ; math:exponentOfDimension math:massDimension ;\n\
           math:exponentNumerator 1 ; math:exponentDenominator 1 .\n\
         ex:lDm1 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
           math:exponentNumerator -1 ; math:exponentDenominator 1 .\n\
         ex:tDm2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
           math:exponentNumerator -2 ; math:exponentDenominator 1 .\n\
         ex:volumeDim a math:DerivedDimension ; math:baseDimensionExponent ex:lV3 .\n\
         ex:lV3 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
           math:exponentNumerator 3 ; math:exponentDenominator 1 .\n\
         ex:density a math:MeasurableFunction ; math:hasDimension ex:densityDim .\n\
         ex:vol a math:Measure ; math:hasDimension ex:volumeDim .\n";

    #[test]
    fn integral_with_composed_parameter_dimensions_is_clean() {
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{ENERGY_INTEGRAL}\
             ex:energy a math:Integral ;\n\
               math:integrand ex:density ;\n\
               math:withRespectTo ex:vol ;\n\
               math:hasDimension ex:energyDim .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(!has_inhomogeneity(&report), "errors: {:?}", report.errors());
    }

    #[test]
    fn integral_with_mismatched_result_dimension_is_flagged() {
        // Declare the result as time (T) instead of energy — the parameter slots do
        // not compose to it.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{ENERGY_INTEGRAL}\
             ex:energy a math:Integral ;\n\
               math:integrand ex:density ;\n\
               math:withRespectTo ex:vol ;\n\
               math:hasDimension math:timeDimension .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(has_inhomogeneity(&report), "errors: {:?}", report.errors());
    }

    #[test]
    fn dimension_vector_string_drift_is_flagged() {
        // Structured exponents render to "T-1"; the authored string says "L" — drift.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:freqDim a math:DerivedDimension ;\n\
               math:dimensionVector \"L\" ;\n\
               math:baseDimensionExponent ex:tm1 .\n\
             ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("math:MalformedDimension") && e.contains("dimensionVector")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn dimension_vector_string_matching_render_is_clean() {
        // Structured exponents render to "T-1"; the authored string matches.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:freqDim a math:DerivedDimension ;\n\
               math:dimensionVector \"T-1\" ;\n\
               math:baseDimensionExponent ex:tm1 .\n\
             ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("math:MalformedDimension")),
            "errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn fractional_dimensions_are_exact_and_distinct() {
        // √Hz is T^(-1/2); it is homogeneous with itself and distinct from T^(-1).
        let clean = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:asdDim a math:DerivedDimension ; math:baseDimensionExponent ex:th .\n\
             ex:th a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 2 .\n\
             ex:asdDim2 a math:DerivedDimension ; math:baseDimensionExponent ex:th2 .\n\
             ex:th2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 2 .\n\
             ex:q1 a math:Quantity ; math:hasDimension ex:asdDim .\n\
             ex:q2 a math:Quantity ; math:hasDimension ex:asdDim2 .\n\
             ex:ok a math:DimensionalExpression ; math:homogeneousOperand ex:q1 , ex:q2 .\n"
        ));
        assert!(
            !has_inhomogeneity(&structural_lint_dataset(&clean, &cfg())),
            "T^(-1/2) must be homogeneous with itself"
        );
        // Now mix T^(-1/2) with T^(-1): distinct, so inhomogeneous.
        let mixed = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:asdDim a math:DerivedDimension ; math:baseDimensionExponent ex:th .\n\
             ex:th a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 2 .\n\
             ex:freqDim a math:DerivedDimension ; math:baseDimensionExponent ex:tm1 .\n\
             ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 1 .\n\
             ex:q1 a math:Quantity ; math:hasDimension ex:asdDim .\n\
             ex:q3 a math:Quantity ; math:hasDimension ex:freqDim .\n\
             ex:bad a math:DimensionalExpression ; math:homogeneousOperand ex:q1 , ex:q3 .\n"
        ));
        assert!(
            has_inhomogeneity(&structural_lint_dataset(&mixed, &cfg())),
            "T^(-1/2) and T^(-1) must be inhomogeneous"
        );
    }

    #[test]
    fn zero_denominator_exponent_is_flagged() {
        // An exact-rational power with denominator 0 is ill-formed: dimension_vector
        // returns None on it, so the homogeneity/composition loops would skip it
        // silently. The native scan must surface it as math:MalformedDimension rather
        // than fail open.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:badDim a math:DerivedDimension ; math:baseDimensionExponent ex:zc .\n\
             ex:zc a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 0 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("math:MalformedDimension")
                    && e.contains("exponentDenominator 0")),
            "a zero-denominator exponent cell must raise math:MalformedDimension; errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn nonzero_denominator_exponent_is_clean() {
        // The well-formed twin: a legitimate 1/2 power must NOT trip the zero-denominator
        // scan (guards against an over-broad match on the denominator literal).
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:okDim a math:DerivedDimension ; math:baseDimensionExponent ex:hc .\n\
             ex:hc a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 2 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("math:MalformedDimension")),
            "a non-zero denominator must not raise math:MalformedDimension; errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn undimensioned_operand_is_flagged() {
        // An operand carrying no math:hasDimension must not let the expression fail open:
        // mixing a dimensioned operand with an undimensioned one is not homogeneous.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{TIME_QUANTITIES}\
             ex:mystery a math:Quantity .\n\
             ex:bad a math:DimensionalExpression ;\n\
               math:homogeneousOperand ex:t1 , ex:mystery .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            has_inhomogeneity(&report)
                && report
                    .errors()
                    .iter()
                    .any(|e| e.contains("undimensioned operand")),
            "an undimensioned operand must raise math:DimensionalInhomogeneity; errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn integral_with_undimensioned_part_is_flagged() {
        // The integral declares a result dimension but its measure carries none — the
        // composition cannot be verified, so it must not fail open.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{ENERGY_INTEGRAL}\
             ex:vol2 a math:Measure .\n\
             ex:energy a math:Integral ;\n\
               math:integrand ex:density ;\n\
               math:withRespectTo ex:vol2 ;\n\
               math:hasDimension ex:energyDim .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            has_inhomogeneity(&report) && report.errors().iter().any(|e| e.contains("carries no")),
            "an integral with an undimensioned measure must raise math:DimensionalInhomogeneity; \
             errors: {:?}",
            report.errors()
        );
    }

    #[test]
    fn unliftable_ingest_fires_when_run_produces_no_codomain() {
        // The slice-resident counter-example for the native math:UnliftableIngest gate: a bridge
        // run that retains a source witness (math:parseSource) and its full grounding frame — so it
        // is NOT math:UngroundedIngestRun — but lifts no structured math: codomain (nothing is
        // gmeow:wasGeneratedBy it), silently dropping its content. Authored in the slice, not
        // inline here, so the (fixture, native-lint) pair is load-bearing rather than a Rust demo.
        let unliftable = include_str!(
            "../../../slices/grounding/math/tests/counter-examples/ingest-run-unliftable.ttl"
        );
        let report = structural_lint_dataset(&dataset_from(unliftable), &cfg());
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.contains("math:UnliftableIngest")
                    && e.contains("http://example.org/math/run")),
            "the slice-resident produced-nothing ingest run (parseSource, no gmeow:wasGeneratedBy) \
             must raise math:UnliftableIngest; errors: {:?}",
            report.errors()
        );
        // It retains its source, so the SHACL grounding twin is out of scope: the native gate must
        // not double-report the run as math:UngroundedIngestRun.
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("math:UngroundedIngestRun")),
            "the produced-nothing fixture retains math:parseSource, so it must not fire \
             math:UngroundedIngestRun: {:?}",
            report.errors()
        );
    }

    #[test]
    fn unliftable_ingest_clean_when_run_produces_a_codomain() {
        // The same run, now lifting a structured math: object that points back through
        // gmeow:wasGeneratedBy: a full lift, no violation.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:rRun a math:RIngestRun ;\n\
               math:parseSource ex:srcWitness .\n\
             ex:srcWitness a math:MathematicalObject .\n\
             ex:fittedModel a math:FittedModel ;\n\
               gmeow:wasGeneratedBy ex:rRun .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("math:UnliftableIngest")),
            "an ingest run that lifts a structured math: codomain must NOT raise \
             math:UnliftableIngest; errors: {:?}",
            report.errors()
        );
    }
}
