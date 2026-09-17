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
use gmeow_math::dimension::DimVector;

use crate::model::{logic, owl, rdf, rdfs, skos};

/// Strongly-typed configuration for the three lints — no untyped dict bag,
/// every field is explicit and typed.
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
    /// source of truth for the set.
    pub annotation_predicates: HashSet<String>,
}

/// The annotation predicates whose literals the Check-2 external-language-tag
/// policy polices — a **view over the single localizable authority**
/// ([`crate::localizable::LOCALIZABLE_PREDICATES`]), never a parallel constant.
/// Check-2 skips GMEOW-namespace predicates by its own namespace guard (they are
/// Check-1's concern), so the GMEOW members of the authority are harmless no-ops
/// here; the effective surface is the standard cross-vocabulary annotation
/// predicates plus the SKOS lexical predicates.
#[must_use]
pub fn default_annotation_predicates() -> Vec<String> {
    crate::localizable::LOCALIZABLE_PREDICATES
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
///
/// Public so conformance tests can isolate findings by their stable code rather
/// than by matching message text.
pub mod codes {
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
    pub const MATH_UNLIFTABLE_INGEST: &str = "validate.lint.math.unliftable-ingest";
    pub const MATH_STRING_ONLY_COMPUTABLE_EXPRESSION: &str =
        "validate.lint.math.string-only-computable-expression";
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
    pub const MATH_MISSING_PRESERVATION_KIND: &str = "validate.lint.math.missing-preservation-kind";
    pub const MATH_UNDECLARED_UNSUPPORTED_CONSTRUCT: &str =
        "validate.lint.math.undeclared-unsupported-construct";
    pub const MATH_UNRECORDED_PROJECTION_LOSS: &str =
        "validate.lint.math.unrecorded-projection-loss";
    pub const MATH_UNGROUNDED_RESULT_CLAIM: &str = "validate.lint.math.ungrounded-result-claim";
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

/// Whether a named graph is an **external-publication / lowering fanout graph** — a
/// commitment-shifted external view that DELIBERATELY carries public BCP-47 language
/// tags for external consumers, so the internal `x-gmeow-` carrier-tag discipline does
/// not apply to it. Three families qualify:
///
/// * the `research-objects` RO-Crate / DCAT export (`graph/fanout/research-objects/…`),
///   whose serializer retags each authored A-Box `x-gmeow-*` literal to its public
///   BCP-47 form — a published RO-Crate MUST NOT ship a private-use tag its consumers
///   cannot read;
/// * the **FnO** function lowering (`…/*.fno.ttl`) and the **EDOAL** alignment lowering
///   (`…/*.edoal`) — the "generated lowerings" of Principle 17 (with SSSOM), external
///   interchange surfaces described in a foreign standard vocabulary for foreign tools.
///
/// The exemption is keyed on the **containing named graph**, never the subject: the same
/// GMEOW term legitimately appears carrier-tagged in its internal slice graph and public-
/// tagged in its external projection here, and only the projection is exempt — so this
/// never masks an internal-graph violation. It exempts ONLY the language-tag discipline;
/// the structural-annotation contract (label / definition / isDefinedBy / graphBoxRole)
/// still applies, because these graphs ride in the linted bundle.
fn is_external_publication_graph(graph_iri: &str, cfg: &LintConfig) -> bool {
    let Some(rest) = graph_iri.strip_prefix(&format!("{}graph/", cfg.namespace)) else {
        return false;
    };
    rest.starts_with("fanout/research-objects/")
        || rest.ends_with(".fno.ttl")
        || rest.ends_with(".edoal")
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
            // A term is typed in the canonical `logic:` spelling; lower each typing
            // marker to its `owl:` view so `term_kind` (keyed on the `owl:`
            // constants) classifies both spellings identically.
            out.insert(gmeow_ns::to_owl_view(iri).to_owned());
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
    // Discover typed subjects under BOTH the canonical `logic:` typing markers and
    // their generated `owl:` views (a slice authors `logic:Class`, not `owl:Class`,
    // after the surface flip); `term_kind`/`ds_rdf_types` then lower each to a single
    // kind. A subject missed here would fall through to the `individual` default.
    let typed_queries = [
        owl::ONTOLOGY,
        owl::CLASS,
        owl::OBJECT_PROPERTY,
        owl::DATATYPE_PROPERTY,
        owl::ANNOTATION_PROPERTY,
        rdfs::DATATYPE,
        gmeow_ns::LOGIC_ONTOLOGY,
        gmeow_ns::LOGIC_CLASS,
        gmeow_ns::LOGIC_OBJECT_PROPERTY,
        gmeow_ns::LOGIC_DATATYPE_PROPERTY,
        gmeow_ns::LOGIC_ANNOTATION_PROPERTY,
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

    // 3. Dangling GMEOW subclass/subproperty targets — both the canonical
    // `logic:subClassOf`/`logic:subPropertyOf` edges and their `rdfs:` projection
    // (gmeow_ns::subsumption_predicates doctrine; crates/ns/src/lib.rs:106-166).
    for predicate in gmeow_ns::SUB_CLASS_OF
        .into_iter()
        .chain(gmeow_ns::SUB_PROPERTY_OF)
    {
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

    // 4. Comprehensiveness heuristic — scan both class-subsumption spellings
    // (canonical `logic:subClassOf` + projected `rdfs:subClassOf`) into ONE
    // `parent_to_children` map so a re-authored taxonomy stays visible.
    let mut parent_to_children: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for predicate in gmeow_ns::SUB_CLASS_OF {
        let Some(p_id) = ds_iri_id(ds, predicate) else {
            continue;
        };
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
                    .insert(child.to_owned());
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

        // Exemption: external-publication fanout graphs (RO-Crate / research-object
        // exports) are commitment-shifted external views that DELIBERATELY carry
        // public BCP-47 tags for external consumers — the internal `x-gmeow-` carrier
        // discipline does not police them. Keyed on the containing named graph, so an
        // internal-graph violation of the same term is still caught.
        if let Some(g_id) = q.g
            && let TermRef::Iri(graph_iri) = ds.resolve(g_id)
            && is_external_publication_graph(graph_iri, cfg)
        {
            continue;
        }

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

    // math: ingestion-bridge gate — a bridge run (the mnemomorphic put leg of a
    // logic:Correspondence) lifts fully or hard-fails; a run retaining a source but
    // producing no structured math: codomain has silently dropped its content.
    check_math_ingest_invariants(ds, &mut report);

    // math: expression-AST source-lint gate — the native Rust twin of the SHACL-Core-
    // derived math:StringOnlyComputableExpressionConstraint: a math:MathematicalExpression
    // claiming to be computable (math:normalForm, math:compilesToLogicFormula, or
    // math:expressionType) must carry at least one structured-child edge
    // (math:argumentSlot, math:boundVariable, math:hasMathematicalSymbol, or
    // math:literalValue) or it is represented only by a string.
    check_math_expression_invariants(ds, &mut report);

    // math: mathematical-core native cardinality/framing gate — the native Rust twin of
    // several SHACL-Core-derived class restrictions, run over the LIVE dataset rather
    // than a generated shape surface: an extended-real slot's malformed value, an
    // unbacked analytic property, an unbound closed form, an underspecified
    // compactification/interval/limit-result/measure-evaluation/piecewise function, an
    // unframed arithmetic operator, and an ungrounded statistical/probabilistic result
    // claim.
    check_math_core_invariants(ds, cfg, &mut report);

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
use gmeow_ns::LANG_NS;
use gmeow_ns::LOGIC_NS;

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
/// `lang:renderedContent` (a), is `logic:sameAs` (or its `owl:sameAs` view) its own
/// `lang:renderedContent` (b),
/// or whose `lang:renderingForm` equals its `lang:renderedContent` (c).
fn check_rendering_as_identity(ds: &RdfDataset, report: &mut LintReport) {
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
        // (b) rendering is sameAs its own renderedContent. A slice authors the
        // canonical `logic:sameAs`; its generated OWL view is `owl:sameAs`.
        let mut same_as = ds_object_iris(ds, &subj, logic::SAME_AS);
        same_as.extend(ds_object_iris(ds, &subj, owl::SAME_AS));
        for c in content.intersection(&same_as) {
            report.push_error(
                codes::LANG_RENDERING_AS_IDENTITY_SAMEAS,
                format!("{subj}\t{c}"),
                format!(
                    "lang:RenderingAsIdentity: rendering {subj} is asserted logic:sameAs or \
                     owl:sameAs its own lang:renderedContent {c}; the rendering has become identity"
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
use gmeow_ns::MATH_NS;

fn math_iri(term: &str) -> String {
    format!("{MATH_NS}{term}")
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

/// The exact-rational ℚ⁷ exponent vector of a dimension IRI, read out of the dataset
/// and expressed as the shared [`gmeow_math::dimension::DimVector`] — the ONE source
/// this probability-layer gate computes dimensions through (no duplicate ℚ⁷ algebra
/// lives here; the type, `add`/`sub`/`commensurable`/`render` are `gmeow-math`'s). A
/// base dimension is a unit basis vector; `math:dimensionless` (or any
/// `math:Dimensionless`) is zero; a `math:DerivedDimension` sums `power * e_base` over
/// its `math:baseDimensionExponent` cells. Returns `None` for a dimension whose
/// structure is ill-formed (a non-base exponent target, a missing/non-integer/
/// zero-denominator power, or arithmetic overflow) or whose kind cannot be computed,
/// so an unrelated node never yields a false positive — the ill-formed structural
/// cases are surfaced explicitly by the SHACL `DimensionExponentShape` and the native
/// `math:` measure-and-dimension reasoned gate (which runs at reason-verify speed,
/// computing through this same `gmeow-math` source), so a `None` here means "already
/// reported elsewhere", never "silently dropped".
fn dimension_vector(ds: &RdfDataset, dim_iri: &str) -> Option<DimVector> {
    if let Some(v) = DimVector::base_unit(dim_iri) {
        return Some(v);
    }
    if dim_iri == math_iri("dimensionless") || ds_has_type(ds, dim_iri, &math_iri("Dimensionless"))
    {
        return Some(DimVector::zero());
    }
    if !ds_has_type(ds, dim_iri, &math_iri("DerivedDimension")) {
        return None;
    }
    let mut v = DimVector::zero();
    for cell in ds_object_iris_sorted(ds, dim_iri, &math_iri("baseDimensionExponent")) {
        let base = ds_object_iris_sorted(ds, &cell, &math_iri("exponentOfDimension"))
            .into_iter()
            .next()?;
        let bi = gmeow_math::dimension::base_dimension_index(&base)?;
        let num = ds_object_literals(ds, &cell, &math_iri("exponentNumerator"))
            .into_iter()
            .find_map(|l| l.parse::<i128>().ok())?;
        let den = ds_object_literals(ds, &cell, &math_iri("exponentDenominator"))
            .into_iter()
            .find_map(|l| l.parse::<i128>().ok())?;
        v.add_exponent(bi, Rational::new(num, den).ok()?).ok()?;
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
    use gmeow_ns::GMEOW_NS;
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

/// The `math:` expression-AST invariants the charter designates as native
/// Rust-validator primary gates over `math:MathematicalExpression`. Currently the
/// source-lint half of `math:StringOnlyComputableExpression` (the SHACL-Core-derived
/// half is `math:StringOnlyComputableExpressionConstraint` in `module.ttl`).
fn check_math_expression_invariants(ds: &RdfDataset, report: &mut LintReport) {
    check_string_only_computable_expression(ds, report);
}

/// `math:StringOnlyComputableExpression` — a `math:MathematicalExpression` carrying at
/// least one of the three "computable" trigger edges (`math:normalForm`,
/// `math:compilesToLogicFormula`, `math:expressionType`) claims to be more than an
/// opaque string: it claims a computable normal form, a logic:Formula compilation, or a
/// classified expression type. That claim is only warranted if the expression ALSO
/// carries at least one structured-child edge (`math:argumentSlot`, `math:boundVariable`,
/// `math:hasMathematicalSymbol`, `math:literalValue`) — an actual AST structure to back
/// it. An expression with a trigger edge but none of the four structured-child edges is
/// represented only by a string: the computable claim is unwarranted. Mirrors
/// `math:StringOnlyComputableExpressionConstraint`'s guard/or-of-exists shape exactly
/// (same trigger set, same structured-child set), authored here as its native-Rust twin
/// because the source-lint charter tier for this failure class is "source-lint + SHACL
/// Core", not a SHACL-only shape.
fn check_string_only_computable_expression(ds: &RdfDataset, report: &mut LintReport) {
    let triggers = [
        math_iri("normalForm"),
        math_iri("compilesToLogicFormula"),
        math_iri("expressionType"),
    ];
    let structured_children = [
        math_iri("argumentSlot"),
        math_iri("boundVariable"),
        math_iri("hasMathematicalSymbol"),
        math_iri("literalValue"),
    ];
    for expr in ds_subjects_of_type(ds, &math_iri("MathematicalExpression")) {
        let has_trigger = triggers.iter().any(|p| ds_has_predicate(ds, &expr, p));
        if !has_trigger {
            continue;
        }
        let has_structured_child = structured_children
            .iter()
            .any(|p| ds_has_predicate(ds, &expr, p));
        if has_structured_child {
            continue;
        }
        report.push_error(
            codes::MATH_STRING_ONLY_COMPUTABLE_EXPRESSION,
            expr.clone(),
            format!(
                "math:StringOnlyComputableExpression: expression {expr} carries a \
                 math:normalForm, math:compilesToLogicFormula, or math:expressionType edge but \
                 none of math:argumentSlot, math:boundVariable, math:hasMathematicalSymbol, or \
                 math:literalValue — it is represented only by a string"
            ),
        );
    }
}

/// The `math:` mathematical-core native invariant that genuinely needs Rust execution
/// rather than a declarative axiom: the statistical-result-claim vantage-grounding guard
/// ([`check_ungrounded_result_claim`]). The extended-real-slot value guard
/// (`math:ExtendedRealValueConstraint`), the analytic-property law/boundary guard
/// (`math:AnalyticPropertyBackedConstraint`), the closed-form-function body/argument
/// guard, the compactification four-role (and conformal-factor) guard, the interval
/// four-field guard, the limit-result outcome/value-agreement guard
/// (`math:LimitResultOutcomeValueConstraint`), the measure-evaluation three-role (and
/// `math:MeasureResultNonNegativeConstraint` non-negativity) guard, the
/// piecewise-function at-least-one-piece guard, and the arithmetic-operation
/// domain/codomain guard are each fully owned by an EL-safe OWL/RDFS exact-one
/// restriction (paired `owl:maxQualifiedCardinality`/`owl:minQualifiedCardinality`
/// restrictions on the class in `module.ttl`, the derive-source for the generated SHACL)
/// or a `logic:Constraint` SHACL-SPARQL twin already authored in `module.ttl` — the
/// declarative tier decides them outright, so no Rust side-channel exists for them here.
fn check_math_core_invariants(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    check_ungrounded_result_claim(ds, cfg, report);
}

/// `math:UngroundedResultClaim` — a `gmeow:Observation` naming a result through
/// `gmeow:observationResult` must itself carry a `gmeow:vantage`; a held statistical or
/// probabilistic result claim is an Observation with a vantage, never an unconditional
/// property of the result object. Purely native (no SHACL target shape, no module.ttl
/// SHACL-deriving axiom) — the "Rust validator" tier, mirroring
/// [`check_unliftable_ingest`]'s architecture: a genuine cross-node obligation over
/// `gmeow:Observation`/`gmeow:observationResult`/`gmeow:vantage`, none of which is
/// `math:`-specific.
fn check_ungrounded_result_claim(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    let observation_result = format!("{}observationResult", cfg.namespace);
    let vantage = format!("{}vantage", cfg.namespace);
    let observation = format!("{}Observation", cfg.namespace);
    for obs in ds_subjects_of_type(ds, &observation) {
        if ds_has_predicate(ds, &obs, &observation_result) && !ds_has_predicate(ds, &obs, &vantage)
        {
            report.push_error(
                codes::MATH_UNGROUNDED_RESULT_CLAIM,
                obs.clone(),
                format!(
                    "math:UngroundedResultClaim: observation {obs} names a result through \
                     gmeow:observationResult but carries no gmeow:vantage; a held statistical or \
                     probabilistic result claim is an Observation with a vantage, never an \
                     unconditional property of the result object"
                ),
            );
        }
    }
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
    // numerator/denominator pair, else its math:quantityValue decimal) and compared by
    // exact-rational order; an unreadable magnitude is skipped, never coerced.
    for node in ds_subjects_of_type(ds, &math_iri("ProbabilityValue")) {
        let Some(mag) = read_magnitude(ds, &node, &[math_iri("quantityValue")]) else {
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
            && let Some(mag) = read_magnitude(ds, &q, &[math_iri("quantityValue")])
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
                rvec.add(&rvec).ok()
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

/// The `math:` projection-side invariants: the join-requiring native gates over
/// `math:ProjectionRecord` loss-ledger carriers. Kept purely native (no SHACL target
/// shape) exactly like the four `lang:` projection gates, because each requires a join
/// the closed-world shape language cannot express. Runs bundle-wide (`GraphMatch::Any`).
fn check_math_projection_invariants(ds: &RdfDataset, report: &mut LintReport) {
    check_math_projection_confidence_as_probability(ds, report);
    check_math_projection_dropped_parameterization(ds, report);
    check_math_missing_preservation_kind(ds, report);
    check_math_undeclared_unsupported_construct(ds, report);
    check_math_unrecorded_projection_loss(ds, report);
}

/// `math:MissingPreservationKind` — every `math:ProjectionRecord` declares a
/// `logic:preservationKind` (the `logic:` loss-ledger vocabulary, reused verbatim); a
/// record with none has entered the loss ledger carrying an undeclared preservation
/// judgment. Mirrors `lang:MissingPreservationKind`'s architecture one stratum over.
fn check_math_missing_preservation_kind(ds: &RdfDataset, report: &mut LintReport) {
    let preservation_kind = logic_iri("preservationKind");
    for r in ds_subjects_of_type(ds, &math_iri("ProjectionRecord")) {
        if !ds_has_predicate(ds, &r, &preservation_kind) {
            report.push_error(
                codes::MATH_MISSING_PRESERVATION_KIND,
                r.clone(),
                format!(
                    "math:MissingPreservationKind: projection record {r} declares no \
                     logic:preservationKind; every projection declares its preservation kind (the \
                     logic: loss-ledger vocabulary, reused verbatim)"
                ),
            );
        }
    }
}

/// `math:UndeclaredUnsupportedConstruct` — a LOSSY `math:ProjectionRecord` (a declared
/// `logic:preservationKind` other than `logic:ExactPreservation`) must enumerate every
/// construct it drops through `logic:unsupportedConstruct`; a lossy record naming none
/// has claimed a completeness its own preservation kind denies. Mirrors
/// `lang:UndeclaredUnsupportedConstruct` one stratum over.
fn check_math_undeclared_unsupported_construct(ds: &RdfDataset, report: &mut LintReport) {
    let preservation_kind = logic_iri("preservationKind");
    let exact = logic_iri("ExactPreservation");
    let unsupported = logic_iri("unsupportedConstruct");
    for r in ds_subjects_of_type(ds, &math_iri("ProjectionRecord")) {
        let kinds = ds_object_iris(ds, &r, &preservation_kind);
        let lossy = !kinds.is_empty() && !kinds.contains(&exact);
        if lossy && !ds_has_predicate(ds, &r, &unsupported) {
            report.push_error(
                codes::MATH_UNDECLARED_UNSUPPORTED_CONSTRUCT,
                r.clone(),
                format!(
                    "math:UndeclaredUnsupportedConstruct: lossy projection record {r} (a \
                     logic:preservationKind other than logic:ExactPreservation) enumerates no \
                     logic:unsupportedConstruct; a lossy projection drops nothing or names \
                     everything it drops"
                ),
            );
        }
    }
}

/// `math:UnrecordedProjectionLoss` — a LOSSY `math:ProjectionRecord` whose
/// `math:projectionSource` is a `math:MathematicalExpression` (a structured AST, not a
/// bare display string) must name that flattening among its `logic:unsupportedConstruct`
/// entries — collapsing a structured expression tree to `math:projectionTargetName`'s
/// string-only external target is exactly the loss the ledger exists to record.
fn check_math_unrecorded_projection_loss(ds: &RdfDataset, report: &mut LintReport) {
    let preservation_kind = logic_iri("preservationKind");
    let exact = logic_iri("ExactPreservation");
    let projection_source = math_iri("projectionSource");
    let unsupported = logic_iri("unsupportedConstruct");
    for r in ds_subjects_of_type(ds, &math_iri("ProjectionRecord")) {
        let kinds = ds_object_iris(ds, &r, &preservation_kind);
        if kinds.is_empty() || kinds.contains(&exact) {
            continue;
        }
        let drop_iris = ds_object_iris(ds, &r, &unsupported);
        let drop_literals: Vec<String> = ds_object_literals(ds, &r, &unsupported)
            .into_iter()
            .map(|d| d.to_lowercase())
            .collect();
        for src in ds_object_iris_sorted(ds, &r, &projection_source) {
            if !ds_has_type(ds, &src, &math_iri("MathematicalExpression")) {
                continue;
            }
            let local = src
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(src.as_str())
                .to_lowercase();
            let recorded = drop_iris.contains(&src)
                || (!local.is_empty()
                    && drop_literals.iter().any(|d| {
                        d.split(|c: char| !c.is_alphanumeric())
                            .any(|tok| tok == local)
                    }))
                || drop_literals.iter().any(|d| {
                    d.split(|c: char| !c.is_alphanumeric())
                        .any(|tok| tok == "expression" || tok == "ast" || tok == "structural")
                });
            if !recorded {
                report.push_error(
                    codes::MATH_UNRECORDED_PROJECTION_LOSS,
                    format!("{r}\t{src}"),
                    format!(
                        "math:UnrecordedProjectionLoss: lossy projection record {r} flattens its \
                         expression-AST source {src} without recording the structural loss among \
                         its logic:unsupportedConstruct entries — collapsing a structured AST to a \
                         string-only external target is exactly the loss the ledger exists to \
                         record"
                    ),
                );
            }
        }
    }
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
/// (`set(_collect_typed_terms(graph))`) — the term universe used for guide/markdown
/// anchor-reference resolution.
pub fn declared_terms_dataset(ds: &RdfDataset, cfg: &LintConfig) -> Vec<String> {
    collect_typed_terms_dataset(ds, cfg).into_keys().collect()
}

#[cfg(test)]
mod source_contracts;

#[path = "lint.tests.rs"]
#[cfg(test)]
mod tests;
