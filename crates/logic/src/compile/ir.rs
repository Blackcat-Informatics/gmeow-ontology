// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed intermediate representation (IR) for the GMEOW Logic compiler.
//!
//! The canonical IR (the Python duplicate `logic_ir.py` was retired in #727).
//! This module is **pure data** — no I/O, no graph parsing, no side effects.
//!
//! # Canonicalization contract
//!
//! [`LogicProgram`] equality is content-addressed and order-independent: two
//! programs with the same axioms/rules/contracts constructed in a different order
//! compare equal and produce the same canonical key.  This is achieved by storing
//! all collection fields as **sorted vectors**, built by the canonicalizing
//! constructors ([`LogicProgram::new`], [`LogicRule::new`]).  Sorting is **stable**
//! and keyed on [`LogicAxiom::sort_key`] / [`LogicRule::sort_key`] /
//! [`ReasoningContract::sort_key`].  The axiom/rule keys reproduce the Python
//! `_sort_key()` byte for byte (null-byte separators; Python `bool` `Display`
//! `True`/`False`; corpus-safety: `negated` / `distinct` are appended to the key
//! only when set, so every pre-#502/#503 program keeps its exact historical key
//! string and the downstream artifacts stay byte-identical).  The contract key is
//! greenfield (#767): it has no Python byte form, only internal determinism.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The null-byte field separator used by every `sort_key` (Python `"\x00"`).
const SEP: char = '\u{0}';

/// The `logic:` namespace; `iri()` helpers expand a local enum name to its IRI.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE` (and [`crate::provenance::LOGIC_NAMESPACE`]).
pub const LOGIC_NAMESPACE: &str = "https://blackcatinformatics.ca/logic/";

/// The `logic:ResourcePolicy` facet value (local name) that licenses operational SLD
/// cut + builtins — the facet on which cut-confinement (AC-2) is decided.  Distinct
/// from the budget/bound property: a budgeted contract does not, by that fact, license
/// cut.  Only the procedural preset expands to it (`logic:expandsToFacet`).
pub const PROCEDURAL_EXECUTION_FACET: &str = "ProceduralExecution";

// --------------------------------------------------------------------------- //
// Enums — single source of truth, local names taken verbatim from module.ttl
// --------------------------------------------------------------------------- //

/// The six historical reasoning **preset** ids — the named `logic:ReasoningPreset`
/// individuals (#767; formerly `logic:SemanticProfile`).
///
/// The string form ([`SemanticProfileId::as_str`]) is the local name (no
/// `logic:` prefix), taken verbatim from `slices/core/logic/module.ttl` — any
/// change there must be reflected here.  A preset is sugar the front-end expands
/// to a full [`ReasoningContract`] facet selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticProfileId {
    /// `logic:PositiveHornProfile`.
    PositiveHorn,
    /// `logic:StratifiedNAFProfile`.
    StratifiedNaf,
    /// `logic:WellFoundedProfile`.
    WellFounded,
    /// `logic:StableModelProfile`.
    StableModel,
    /// `logic:ProceduralPrologProfile`.
    ProceduralProlog,
    /// `logic:ProbabilisticProfile`.
    Probabilistic,
}

impl SemanticProfileId {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PositiveHorn => "PositiveHornProfile",
            Self::StratifiedNaf => "StratifiedNAFProfile",
            Self::WellFounded => "WellFoundedProfile",
            Self::StableModel => "StableModelProfile",
            Self::ProceduralProlog => "ProceduralPrologProfile",
            Self::Probabilistic => "ProbabilisticProfile",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// `true` iff this preset's facet bundle carries the procedural-execution facet
    /// (`logic:ProceduralExecution`) and therefore licenses SLD cut.
    ///
    /// This mirrors the `logic:expandsToFacet` bundle authored in `module.ttl` (only
    /// `ProceduralPrologProfile` expands to `logic:ProceduralExecution`); the
    /// `procedural_preset_carries_procedural_execution_facet` test ties this Rust fact
    /// to the ontology surface so the two cannot silently diverge.  The cut gate
    /// (`profile_gate`) decides via this facet-derived predicate, not a raw name match.
    pub fn permits_cut(self) -> bool {
        matches!(self, Self::ProceduralProlog)
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "PositiveHornProfile" => Self::PositiveHorn,
            "StratifiedNAFProfile" => Self::StratifiedNaf,
            "WellFoundedProfile" => Self::WellFounded,
            "StableModelProfile" => Self::StableModel,
            "ProceduralPrologProfile" => Self::ProceduralProlog,
            "ProbabilisticProfile" => Self::Probabilistic,
            _ => return None,
        })
    }
}

impl fmt::Display for SemanticProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The six `logic:PreservationKind` named individuals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PreservationKind {
    /// `logic:ExactPreservation`.
    Exact,
    /// `logic:SoundUnderApproximation`.
    SoundUnder,
    /// `logic:CompleteOverApproximation`.
    CompleteOver,
    /// `logic:ValidationOnly`.
    ValidationOnly,
    /// `logic:InconsistencyPreserving`.
    InconsistencyPreserving,
    /// `logic:InconsistencyReflecting`.
    InconsistencyReflecting,
}

impl PreservationKind {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exact => "ExactPreservation",
            Self::SoundUnder => "SoundUnderApproximation",
            Self::CompleteOver => "CompleteOverApproximation",
            Self::ValidationOnly => "ValidationOnly",
            Self::InconsistencyPreserving => "InconsistencyPreserving",
            Self::InconsistencyReflecting => "InconsistencyReflecting",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }
}

impl fmt::Display for PreservationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// World/modal kinds from the `logic:World` taxonomy.
///
/// [`LogicModality::None`] is the default, unmodalized reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum LogicModality {
    /// No modal annotation (`"none"`).
    #[default]
    None,
    /// `"alethic"`.
    Alethic,
    /// `"epistemic"`.
    Epistemic,
    /// `"doxastic"`.
    Doxastic,
    /// `"telic"`.
    Telic,
    /// `"deontic"`.
    Deontic,
    /// `"representational"`.
    Representational,
    /// `"counterfactual"`.
    Counterfactual,
}

impl LogicModality {
    /// The lowercase string form (the Python `StrEnum` value).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Alethic => "alethic",
            Self::Epistemic => "epistemic",
            Self::Doxastic => "doxastic",
            Self::Telic => "telic",
            Self::Deontic => "deontic",
            Self::Representational => "representational",
            Self::Counterfactual => "counterfactual",
        }
    }

    /// Parse the string form back to the enum (inverse of [`Self::as_str`]).
    pub fn from_str_value(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "alethic" => Self::Alethic,
            "epistemic" => Self::Epistemic,
            "doxastic" => Self::Doxastic,
            "telic" => Self::Telic,
            "deontic" => Self::Deontic,
            "representational" => Self::Representational,
            "counterfactual" => Self::Counterfactual,
            _ => return None,
        })
    }
}

impl fmt::Display for LogicModality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// --------------------------------------------------------------------------- //
// Supporting types
// --------------------------------------------------------------------------- //

/// A typed wrapper for `logic:complexityClass` values (free-text, e.g. `"PTIME"`,
/// `"N2EXPTIME"`, `"terminating/PTIME-data"`, `"undecidable"`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComplexityClass {
    label: String,
}

impl ComplexityClass {
    /// Construct, validating that the label is non-empty (after trimming),
    /// mirroring the Python `__post_init__`.
    pub fn new(label: impl Into<String>) -> Result<Self, String> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err("ComplexityClass.label must be a non-empty string".to_owned());
        }
        Ok(Self { label })
    }

    /// The complexity class label string.
    pub fn label(&self) -> &str {
        &self.label
    }
}

impl fmt::Display for ComplexityClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// Contextual scope annotations shared by axioms and rules.
///
/// All fields are optional (`None` = not declared).  Note: the scope is **not**
/// part of any `sort_key` (mirroring Python) — it participates in equality and the
/// canonical content key, but not in canonical *ordering*.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContextualScope {
    /// IRI string of the standpoint (`logic:World` individual).
    pub standpoint: Option<String>,
    /// An opaque time-expression string (ISO 8601 or EDTF).
    pub time: Option<String>,
    /// The `logic:confidence` value in `[0, 1]`.
    pub confidence: Option<f64>,
    /// The modal kind of the axiom/rule world context.
    pub modality: LogicModality,
    /// IRI string of the provenance source / asserting agent.
    pub provenance: Option<String>,
}

impl ContextualScope {
    /// Construct, validating the confidence range, mirroring Python `__post_init__`.
    pub fn new(
        standpoint: Option<String>,
        time: Option<String>,
        confidence: Option<f64>,
        modality: LogicModality,
        provenance: Option<String>,
    ) -> Result<Self, String> {
        if let Some(c) = confidence {
            if !(0.0..=1.0).contains(&c) {
                return Err(format!(
                    "ContextualScope.confidence must be in [0, 1], got {c}"
                ));
            }
        }
        Ok(Self {
            standpoint,
            time,
            confidence,
            modality,
            provenance,
        })
    }

    /// A deterministic content key for this scope (used by the canonical content
    /// key and the IR-isomorphism gate, not by `sort_key`).
    fn content_key(&self) -> String {
        // `confidence` is formatted with Rust's shortest round-trip float `Display`.
        // The scope key is only ever compared Rust-against-Rust (the isomorphism
        // gate and canonical equality), never serialized into a byte-pinned
        // artifact, so its exact float spelling is internal.
        let conf = match self.confidence {
            Some(c) => {
                let c = if c == 0.0 { 0.0 } else { c }; // collapse -0.0 -> 0.0
                c.to_string()
            }
            None => String::new(),
        };
        format!(
            "st={}{SEP}t={}{SEP}c={conf}{SEP}m={}{SEP}p={}",
            self.standpoint.as_deref().unwrap_or(""),
            self.time.as_deref().unwrap_or(""),
            self.modality.as_str(),
            self.provenance.as_deref().unwrap_or(""),
        )
    }
}

// --------------------------------------------------------------------------- //
// Core IR nodes
// --------------------------------------------------------------------------- //

/// Render a Python `bool` exactly as its `Display` (`True` / `False`) — the
/// `sort_key` byte-parity hinge.
fn py_bool(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// A single `logic:` axiom with contextual scope: a (possibly non-ground)
/// subject-predicate-object assertion in the `logic:` vocabulary.
///
/// Variables are encoded as `?`-prefixed strings inside `subject` / `obj` (there
/// is no separate term enum at the compile layer — the lowering to the evaluable
/// IR splits `?x` → variable, else IRI/literal).
#[derive(Debug, Clone, PartialEq)]
pub struct LogicAxiom {
    /// IRI string (or `?var`) of the axiom subject.
    pub subject: String,
    /// IRI string of the axiom predicate.
    pub predicate: String,
    /// IRI string, `?var`, or literal value of the axiom object.
    pub obj: String,
    /// `true` when `obj` is a literal (data value), `false` when it is an IRI.
    pub obj_is_literal: bool,
    /// `true` when this is a negation-as-failure body literal (#502). Defaults to
    /// `false`; append-only in the sort key for corpus safety.
    pub negated: bool,
    /// Contextual scope for this axiom.
    pub scope: ContextualScope,
}

impl LogicAxiom {
    /// Construct, validating non-empty subject/predicate (Python `__post_init__`).
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        obj: impl Into<String>,
        obj_is_literal: bool,
        negated: bool,
        scope: ContextualScope,
    ) -> Result<Self, String> {
        let subject = subject.into();
        let predicate = predicate.into();
        if subject.is_empty() {
            return Err("LogicAxiom.subject must be a non-empty IRI string".to_owned());
        }
        if predicate.is_empty() {
            return Err("LogicAxiom.predicate must be a non-empty IRI string".to_owned());
        }
        Ok(Self {
            subject,
            predicate,
            obj: obj.into(),
            obj_is_literal,
            negated,
            scope,
        })
    }

    /// Convenience constructor for a positive, unscoped axiom.
    pub fn ground(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        obj: impl Into<String>,
        obj_is_literal: bool,
    ) -> Result<Self, String> {
        Self::new(
            subject,
            predicate,
            obj,
            obj_is_literal,
            false,
            ContextualScope::default(),
        )
    }

    /// Stable sort key for canonical ordering — the golden-pinned key format.
    /// Corpus-safety: `negated` is appended only when `true`.  The scope is
    /// intentionally excluded.
    pub fn sort_key(&self) -> String {
        let mut base = format!(
            "{}{SEP}{}{SEP}{}{SEP}{}",
            self.subject,
            self.predicate,
            self.obj,
            py_bool(self.obj_is_literal),
        );
        if self.negated {
            base.push(SEP);
            base.push_str(py_bool(self.negated));
        }
        base
    }

    /// A deterministic full-content key (sort key + scope) for canonical equality
    /// and the IR-isomorphism gate.
    fn content_key(&self) -> String {
        format!(
            "{}{SEP}|scope|{SEP}{}",
            self.sort_key(),
            self.scope.content_key()
        )
    }
}

/// A single `logic:` rule: a head axiom derived from body axioms.
///
/// The `body` is stored in canonical (sorted) order; the `distinct_pairs`
/// inequality guards (#503) are canonicalized (each pair sorted internally, the
/// whole set sorted).  Construct via [`LogicRule::new`] so the invariants hold.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicRule {
    /// The derived axiom (consequent).
    pub head: LogicAxiom,
    /// The condition axioms (antecedents), in canonical order.
    pub body: Vec<LogicAxiom>,
    /// Inequality body guards (#503): each pair of variable strings must bind to
    /// unequal values.  Canonicalized; append-only in the sort key.
    pub distinct_pairs: Vec<(String, String)>,
    /// Contextual scope for this rule.
    pub scope: ContextualScope,
}

impl LogicRule {
    /// Construct, canonicalizing the body order and the inequality guards
    /// (mirrors the Python `__post_init__`).
    pub fn new(
        head: LogicAxiom,
        body: Vec<LogicAxiom>,
        distinct_pairs: Vec<(String, String)>,
        scope: ContextualScope,
    ) -> Self {
        let mut body = body;
        body.sort_by_cached_key(LogicAxiom::sort_key);

        // Inequality is symmetric: sort the two members WITHIN each pair, then
        // sort the set of pairs.
        let mut pairs: Vec<(String, String)> = distinct_pairs
            .into_iter()
            .map(|(a, b)| if a <= b { (a, b) } else { (b, a) })
            .collect();
        pairs.sort();

        Self {
            head,
            body,
            distinct_pairs: pairs,
            scope,
        }
    }

    /// Stable sort key — the golden-pinned key format.
    /// Corpus-safety: the distinct-pairs segment is appended only when non-empty.
    pub fn sort_key(&self) -> String {
        let body_key = self
            .body
            .iter()
            .map(|a| a.sort_key())
            .collect::<Vec<_>>()
            .join("|");
        let mut base = format!("{}{SEP}{body_key}", self.head.sort_key());
        if !self.distinct_pairs.is_empty() {
            let distinct_key = self
                .distinct_pairs
                .iter()
                .map(|(a, b)| format!("{a}{SEP}{b}"))
                .collect::<Vec<_>>()
                .join("|");
            base.push(SEP);
            base.push_str(&distinct_key);
        }
        base
    }

    /// A deterministic full-content key for canonical equality / the gate.
    fn content_key(&self) -> String {
        let body = self
            .body
            .iter()
            .map(LogicAxiom::content_key)
            .collect::<Vec<_>>()
            .join("|");
        let distinct = self
            .distinct_pairs
            .iter()
            .map(|(a, b)| format!("{a}{SEP}{b}"))
            .collect::<Vec<_>>()
            .join("|");
        format!(
            "head[{}]{SEP}body[{body}]{SEP}distinct[{distinct}]{SEP}scope[{}]",
            self.head.content_key(),
            self.scope.content_key(),
        )
    }
}

/// The canonical reasoning-configuration IR (#767): an independent selection
/// across the orthogonal reasoning facets, replacing the single monolithic
/// semantic-profile axis.
///
/// Facet values are carried as **local-name strings** (not enums) to honour the
/// OPEN facet value vocabulary — a new value individual minted in `module.ttl`
/// must join without a Rust schema change.  Single-valued facets are
/// `Option<String>`; set-valued facets are [`BTreeSet`] (sorted ⇒ deterministic);
/// the closure map is a [`BTreeMap`].  When a contract was authored as / expanded
/// from a named preset, [`Self::preset`] records it.
///
/// Construct via [`ReasoningContract::new`] (empty) and the `with_*` /
/// `set_*` builder methods; the front-end populates it from the graph.  Derives
/// `Eq` but **not** `Hash` (it holds `BTreeMap`, and nothing keys a `HashMap` on
/// it).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReasoningContract {
    /// Set when the contract was authored as / expanded from a named preset.
    pub preset: Option<SemanticProfileId>,

    // ── Single-valued facets (local name of the chosen facet value, or None). ──
    /// `logic:formulaFragment` value.
    pub formula_fragment: Option<String>,
    /// `logic:modelSemantics` value.
    pub model_semantics: Option<String>,
    /// `logic:truthAlgebra` value.
    pub truth_algebra: Option<String>,
    /// `logic:admissibleValuation` value.
    pub admissible_valuation: Option<String>,
    /// `logic:designatedValues` value.
    pub designated_values: Option<String>,
    /// `logic:evolution` value.
    pub evolution: Option<String>,
    /// `logic:argumentation` value.
    pub argumentation: Option<String>,
    /// `logic:revision` value.
    pub revision: Option<String>,
    /// `logic:equalityPolicy` value.
    pub equality_policy: Option<String>,
    /// `logic:defaultClosure` value — the closure-map default.
    pub default_closure: Option<String>,

    // ── Set-valued facets (sorted local names). ───────────────────────────────
    /// `logic:negationOperator` values.
    pub negation_operators: BTreeSet<String>,
    /// `logic:contextAxis` values.
    pub context_axes: BTreeSet<String>,
    /// `logic:uncertaintyMeasure` values.
    pub uncertainty_measures: BTreeSet<String>,
    /// `logic:resourcePolicy` values.
    pub resource_policies: BTreeSet<String>,
    /// `logic:projectionTarget` values.
    pub projection_targets: BTreeSet<String>,

    // ── Map-valued facet. ─────────────────────────────────────────────────────
    /// `logic:closureEntry` map: predicate/context key → closure value local name
    /// (`"OpenWorldClosure"` / `"ClosedWorldClosure"`).
    pub closure_entries: BTreeMap<String, String>,

    /// Carried decidability data (reviewer B2): the `logic:complexityClass` value.
    pub complexity: Option<ComplexityClass>,
}

impl ReasoningContract {
    /// An empty contract (no facets selected, no preset).
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty contract carrying only the given preset id.
    pub fn from_preset(preset: SemanticProfileId) -> Self {
        Self {
            preset: Some(preset),
            ..Self::default()
        }
    }

    /// `true` iff this contract licenses SLD cut (`!`) — i.e. its resource/execution
    /// policy carries the procedural-execution facet ([`PROCEDURAL_EXECUTION_FACET`],
    /// `logic:ProceduralExecution`).
    ///
    /// Cut-confinement (AC-2) is expressed in FACET terms: cut is the operational
    /// search-control of the procedural execution policy, NOT a property of the
    /// budget/bound (a budgeted contract does not, by that fact, license cut).  A
    /// contract assembled directly with the procedural-execution facet licenses cut
    /// even if it carries no `ProceduralPrologProfile` preset name.
    pub fn permits_cut(&self) -> bool {
        self.resource_policies
            .iter()
            .any(|r| r == PROCEDURAL_EXECUTION_FACET)
    }

    /// Render the single-valued facet `Option<String>` fields in a FIXED order
    /// (the determinism hinge of both [`Self::sort_key`] and [`Self::content_key`]).
    fn singletons_segment(&self) -> String {
        // FIXED field order — do not reorder (it pins the key).
        [
            self.formula_fragment.as_deref(),
            self.model_semantics.as_deref(),
            self.truth_algebra.as_deref(),
            self.admissible_valuation.as_deref(),
            self.designated_values.as_deref(),
            self.evolution.as_deref(),
            self.argumentation.as_deref(),
            self.revision.as_deref(),
            self.equality_policy.as_deref(),
            self.default_closure.as_deref(),
        ]
        .map(|v| v.unwrap_or(""))
        .join(&SEP.to_string())
    }

    /// Render the set-valued facets in a FIXED facet order; each set iterates
    /// sorted (it is a [`BTreeSet`]), members joined by `|`.
    fn sets_segment(&self) -> String {
        let join = |set: &BTreeSet<String>| set.iter().cloned().collect::<Vec<_>>().join("|");
        // FIXED facet order.
        [
            join(&self.negation_operators),
            join(&self.context_axes),
            join(&self.uncertainty_measures),
            join(&self.resource_policies),
            join(&self.projection_targets),
        ]
        .join(&SEP.to_string())
    }

    /// Render the closure map in sorted-key order (it is a [`BTreeMap`]).
    fn closure_segment(&self) -> String {
        self.closure_entries
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Stable sort key over a FIXED field order — used for canonical ordering.
    ///
    /// Greenfield (#767): there is no Python byte form to match; the only contract
    /// is INTERNAL determinism (same contract ⇒ same key, any construction order,
    /// guaranteed by the `BTreeSet`/`BTreeMap` storage and the fixed segment order)
    /// and that a preset's expanded contract has a stable key.  Mirrors the
    /// SEP-joined style of the axiom/rule keys.
    pub fn sort_key(&self) -> String {
        let preset = self.preset.map(|p| p.as_str()).unwrap_or("");
        format!(
            "{preset}{SEP}{}{SEP}{}{SEP}{}",
            self.singletons_segment(),
            self.sets_segment(),
            self.closure_segment(),
        )
    }

    /// A deterministic full-content key (sort key + the carried complexity class).
    fn content_key(&self) -> String {
        let compl = self
            .complexity
            .as_ref()
            .map(ComplexityClass::label)
            .unwrap_or("");
        format!("{}{SEP}|compl|{SEP}{compl}", self.sort_key())
    }
}

// --------------------------------------------------------------------------- //
// Predicate-path shapes (#1010)
// --------------------------------------------------------------------------- //

/// The base step of a [`PathShapeIr`]: either one named predicate or a wildcard
/// matching any predicate. The two are structurally exclusive (a step is a named
/// edge XOR any edge); the front-end rejects a graph node that declares both.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathBase {
    /// A named-predicate step: the IRI string every hop traverses
    /// (`logic:pathStepPredicate`).
    NamedPredicate(String),
    /// A wildcard step matching ANY predicate (`logic:pathWildcard true`),
    /// optionally namespace-scoped on the carrying [`PathShapeIr`].
    Wildcard,
}

/// A named, parametric predicate-path traversal specification (`logic:PathShape`,
/// #1010): a reusable, by-name graph walk carrying a base step, a bounded depth
/// range, an optional wildcard namespace scope, and a declared depth parameter.
///
/// This is the canonical form; the SPARQL property-path and Datalog renderings are
/// projections (Principle 17). The depth range is `min_depth ..= max_depth`, with
/// `max_depth == None` meaning unbounded (the `+` / transitive-closure reading).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathShapeIr {
    /// IRI string of the shape individual.
    pub iri: String,
    /// The base step (named predicate or wildcard).
    pub base: PathBase,
    /// Inclusive lower bound on traversal length (`logic:pathMinDepth`; default 1).
    pub min_depth: u32,
    /// Inclusive upper bound (`logic:pathMaxDepth`); `None` ⇒ unbounded.
    pub max_depth: Option<u32>,
    /// Predicate-namespace IRI prefix scoping a wildcard step
    /// (`logic:pathNamespaceScope`); only meaningful when `base` is
    /// [`PathBase::Wildcard`].
    pub namespace_scope: Option<String>,
    /// The declared depth parameter name (`logic:pathDepthParam`) a
    /// `logic:PathInvocation` binds; `None` ⇒ no exposed parameter.
    pub depth_param: Option<String>,
}

/// Hard cap on `max_depth` accepted by [`PathShapeIr::new`].  A value larger
/// than this would cause [`datalog_text`](super::super::projections::paths::datalog_text)
/// to unroll billions of rule lines, exhausting memory (CWE-400).  The cap is
/// conservative but generous: 1 000 hops covers every practical ontology
/// traversal; no legitimate graph walk needs more.
pub const MAX_PATH_DEPTH: usize = 1_000;

impl PathShapeIr {
    /// Construct, validating the depth range and base step:
    ///
    /// * `min_depth` must be ≥ 1;
    /// * when `max_depth` is `Some(m)`, `min_depth` must not exceed `m`;
    /// * when `max_depth` is `Some(m)`, `m` must not exceed [`MAX_PATH_DEPTH`]
    ///   (hard cap — prevents runaway Datalog unrolling, CWE-400);
    /// * a [`PathBase::NamedPredicate`] must be non-empty.
    pub fn new(
        iri: impl Into<String>,
        base: PathBase,
        min_depth: u32,
        max_depth: Option<u32>,
        namespace_scope: Option<String>,
        depth_param: Option<String>,
    ) -> Result<Self, String> {
        let iri = iri.into();
        if iri.is_empty() {
            return Err("PathShapeIr.iri must be a non-empty IRI string".to_owned());
        }
        if min_depth < 1 {
            return Err("PathShapeIr.min_depth must be >= 1".to_owned());
        }
        if let Some(m) = max_depth {
            if min_depth > m {
                return Err(format!(
                    "PathShapeIr min_depth ({min_depth}) must not exceed max_depth ({m})"
                ));
            }
            if m as usize > MAX_PATH_DEPTH {
                return Err(format!(
                    "PathShapeIr max_depth ({m}) exceeds the hard cap of {MAX_PATH_DEPTH}; \
                     use an unbounded path (max_depth = None) for deeper traversals"
                ));
            }
        }
        if let PathBase::NamedPredicate(p) = &base {
            if p.is_empty() {
                return Err("PathShapeIr named-predicate step must be a non-empty IRI".to_owned());
            }
        }
        if let Some(ns) = &namespace_scope {
            if ns.trim().is_empty() {
                return Err(
                    "PathShapeIr.namespace_scope must be a non-empty IRI string when present;                      pass None to leave it unset"
                        .to_owned(),
                );
            }
        }
        Ok(Self {
            iri,
            base,
            min_depth,
            max_depth,
            namespace_scope,
            depth_param,
        })
    }

    /// Stable sort key for canonical ordering — the shape IRI is unique.
    pub fn sort_key(&self) -> String {
        self.iri.clone()
    }

    /// A deterministic full-content key for canonical equality.
    fn content_key(&self) -> String {
        let base = match &self.base {
            PathBase::NamedPredicate(p) => format!("named={p}"),
            PathBase::Wildcard => "wildcard".to_owned(),
        };
        format!(
            "{}{SEP}{base}{SEP}min={}{SEP}max={}{SEP}ns={}{SEP}param={}",
            self.iri,
            self.min_depth,
            self.max_depth.map(|m| m.to_string()).unwrap_or_default(),
            self.namespace_scope.as_deref().unwrap_or(""),
            self.depth_param.as_deref().unwrap_or(""),
        )
    }
}

// --------------------------------------------------------------------------- //
// Top-level container
// --------------------------------------------------------------------------- //

/// Top-level container for a compiled `logic:` program.
///
/// Aggregates axioms, rules, and reasoning contracts; the unit of comparison for
/// the round-trip isomorphism gate.  Construct via [`LogicProgram::new`] so the
/// canonicalization contract (sorted collections) holds.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicProgram {
    /// Axioms in canonical order.
    pub axioms: Vec<LogicAxiom>,
    /// Rules in canonical order.
    pub rules: Vec<LogicRule>,
    /// Reasoning contracts in canonical order (#767; was `profiles`).
    pub contracts: Vec<ReasoningContract>,
    /// Named/parametric predicate-path shapes in canonical order (`logic:PathShape`,
    /// #1010). Attached via [`LogicProgram::with_path_shapes`]; empty for the
    /// historical path-shape-free corpus, so the canonical key is unchanged there.
    pub path_shapes: Vec<PathShapeIr>,
    /// IRI of the source graph/document (optional provenance).
    pub source_iri: Option<String>,
}

impl LogicProgram {
    /// Construct, canonicalizing all collection fields into sorted vectors with a
    /// **stable** sort (mirrors the Python `__post_init__`).
    pub fn new(
        axioms: Vec<LogicAxiom>,
        rules: Vec<LogicRule>,
        contracts: Vec<ReasoningContract>,
        source_iri: Option<String>,
    ) -> Self {
        let mut axioms = axioms;
        axioms.sort_by_cached_key(LogicAxiom::sort_key);
        let mut rules = rules;
        rules.sort_by_cached_key(LogicRule::sort_key);
        let mut contracts = contracts;
        contracts.sort_by_cached_key(ReasoningContract::sort_key);
        Self {
            axioms,
            rules,
            contracts,
            path_shapes: Vec::new(),
            source_iri,
        }
    }

    /// Attach the program's `logic:PathShape` individuals (#1010), canonicalizing
    /// them into sorted order.  Kept separate from [`Self::new`] so existing call
    /// sites are untouched and the byte-pinned canonical key of a path-shape-free
    /// program is unchanged (the path-shapes segment is append-only when present).
    pub fn with_path_shapes(mut self, path_shapes: Vec<PathShapeIr>) -> Self {
        let mut path_shapes = path_shapes;
        path_shapes.sort_by_cached_key(PathShapeIr::sort_key);
        self.path_shapes = path_shapes;
        self
    }

    /// A single deterministic, order-independent content key for the whole
    /// program (the Rust analogue of the Python `canonical()` dict — used for
    /// content-hash comparison and equality assertions).  Because the collections
    /// are already canonically sorted, this is stable across construction orders.
    pub fn canonical_key(&self) -> String {
        let axioms = self
            .axioms
            .iter()
            .map(LogicAxiom::content_key)
            .collect::<Vec<_>>()
            .join("\n");
        let rules = self
            .rules
            .iter()
            .map(LogicRule::content_key)
            .collect::<Vec<_>>()
            .join("\n");
        let contracts = self
            .contracts
            .iter()
            .map(ReasoningContract::content_key)
            .collect::<Vec<_>>()
            .join("\n");
        let mut key = format!(
            "AXIOMS\n{axioms}\nRULES\n{rules}\nCONTRACTS\n{contracts}\nSOURCE\n{}",
            self.source_iri.as_deref().unwrap_or(""),
        );
        // Append-only: a path-shape-free program keeps its exact historical key.
        if !self.path_shapes.is_empty() {
            let shapes = self
                .path_shapes
                .iter()
                .map(PathShapeIr::content_key)
                .collect::<Vec<_>>()
                .join("\n");
            key.push_str("\nPATHSHAPES\n");
            key.push_str(&shapes);
        }
        key
    }
}

#[cfg(test)]
mod tests;
