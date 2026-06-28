// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed intermediate representation (IR) for the GMEOW Logic compiler.
//!
//! The canonical IR (the Python duplicate `logic_ir.py` has been retired).
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
//! `True`/`False`; corpus-safety: `negated` / `distinct` / `load_bearing` /
//! `node_kind` are appended to the key only when non-default, so every historical
//! program keeps its exact historical key string and the downstream artifacts stay
//! byte-identical).  The contract key is greenfield: it has no Python byte form, only
//! internal determinism.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The null-byte field separator used by every `sort_key` (Python `"\x00"`).
const SEP: char = '\u{0}';

/// The `logic:` namespace; `iri()` helpers expand a local enum name to its IRI.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE` (and the runtime
/// `gmeow_logic::provenance::LOGIC_NAMESPACE`).
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
/// individuals (formerly `logic:SemanticProfile`).
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

/// The `logic:NodeKind` taxonomy: the typed-sum tag every IR node declares.
///
/// The IR is a typed sum, not an untyped triple bag — each node names its kind, and the
/// kind governs what may be done with it (a constraint is not a derivation rule, a query
/// operator is not a program operator, a meta-level quotation does not collapse into the
/// object-level assertion it quotes).  The string form ([`NodeKind::as_str`]) is the
/// local name taken verbatim from `slices/core/logic/module.ttl`; any change there must
/// be reflected here (the `node_kind_values_match_module_ttl` test pins it).
///
/// [`NodeKind::Correspondence`] is the **reserved** ninth kind: a law-bearing,
/// possibly-lossy alignment between a source and a target pattern.  Its full machinery
/// (law-spine, relation lattice, `get`/`put` legs, quantitative axes) is the
/// correspondence calculus (`design/LOGIC-CORRESPONDENCE.md`); this slice only reserves
/// the slot so identity and ordering are kind-aware before that body lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum NodeKind {
    /// `logic:ObjectLevelFormula` — an ordinary first-order formula over the domain.
    /// The axiom default, and the canonical `NodeKind::default()`.
    #[default]
    ObjectLevelFormula,
    /// `logic:MetaLevelFormula` — a formula *about* formulas (stays meta).
    MetaLevelFormula,
    /// `logic:Constraint` — an integrity condition whose violation is a finding.
    Constraint,
    /// `logic:DerivationRule` — a head entailed from a body (the productive subset).
    DerivationRule,
    /// `logic:Query` — a goal to be resolved, with its answer shape.
    Query,
    /// `logic:TransactionProgram` — a state-changing composite over the path semantics.
    TransactionProgram,
    /// `logic:ActionSchema` — a named precondition/effect/invariant template.
    ActionSchema,
    /// `logic:ValidationShape` — a closed-world data-shape condition (the SHACL subset).
    ValidationShape,
    /// `logic:Correspondence` — the reserved ninth kind (the correspondence calculus
    /// fills its body; see `design/LOGIC-CORRESPONDENCE.md`).
    Correspondence,
}

impl NodeKind {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ObjectLevelFormula => "ObjectLevelFormula",
            Self::MetaLevelFormula => "MetaLevelFormula",
            Self::Constraint => "Constraint",
            Self::DerivationRule => "DerivationRule",
            Self::Query => "Query",
            Self::TransactionProgram => "TransactionProgram",
            Self::ActionSchema => "ActionSchema",
            Self::ValidationShape => "ValidationShape",
            Self::Correspondence => "Correspondence",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "ObjectLevelFormula" => Self::ObjectLevelFormula,
            "MetaLevelFormula" => Self::MetaLevelFormula,
            "Constraint" => Self::Constraint,
            "DerivationRule" => Self::DerivationRule,
            "Query" => Self::Query,
            "TransactionProgram" => Self::TransactionProgram,
            "ActionSchema" => Self::ActionSchema,
            "ValidationShape" => Self::ValidationShape,
            "Correspondence" => Self::Correspondence,
            _ => return None,
        })
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The seven `logic:PreservationKind` named individuals.
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
    /// `logic:Unsupported` — the construct cannot be expressed in the target at all;
    /// the legalization floor, carried and flagged as residue, never silently dropped.
    Unsupported,
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
            Self::Unsupported => "Unsupported",
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
    /// `true` when this is a negation-as-failure body literal. Defaults to
    /// `false`; append-only in the sort key for corpus safety.
    pub negated: bool,
    /// Contextual scope for this axiom.
    pub scope: ContextualScope,
    /// The `logic:NodeKind` this axiom declares (default
    /// [`NodeKind::ObjectLevelFormula`]).  Folded into the sort/content key only when
    /// non-default, so the historical (all-`ObjectLevelFormula`) corpus is byte-stable.
    pub node_kind: NodeKind,
    /// Whether this axiom's annotation is load-bearing (`logic:loadBearing`): an in-band
    /// complement or quantitative axis the inverse leg needs for `put∘get = id`, versus a
    /// droppable display hint (the default, `false`).  Without this bit a section /
    /// retraction (perfect-subsumption) claim cannot be verified.  Folded into the keys
    /// only when `true`.
    pub load_bearing: bool,
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
            node_kind: NodeKind::ObjectLevelFormula,
            load_bearing: false,
        })
    }

    /// Set this axiom's [`NodeKind`] (builder; default
    /// [`NodeKind::ObjectLevelFormula`]).  Kept off [`Self::new`] so existing call sites
    /// and the byte-pinned default-kind key are unchanged.
    pub fn with_node_kind(mut self, node_kind: NodeKind) -> Self {
        self.node_kind = node_kind;
        self
    }

    /// Mark this axiom's annotation load-bearing (builder; default `false` =
    /// droppable).
    pub fn with_load_bearing(mut self, load_bearing: bool) -> Self {
        self.load_bearing = load_bearing;
        self
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
    /// Corpus-safety: `negated`, then `load_bearing`, then `node_kind` are appended only
    /// when non-default, in that **fixed order** (frozen once committed — reordering
    /// would churn every non-default node).  An all-default axiom keeps a byte-identical
    /// key, so the golden corpus is unchanged.  The scope is intentionally excluded.
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
        // FIXED segment order: load_bearing (when true) THEN node_kind (when != the
        // axiom default ObjectLevelFormula).  Do not reorder — it pins the key.
        if self.load_bearing {
            base.push(SEP);
            base.push_str(py_bool(self.load_bearing));
        }
        if self.node_kind != NodeKind::ObjectLevelFormula {
            base.push(SEP);
            base.push_str(self.node_kind.as_str());
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
/// inequality guards are canonicalized (each pair sorted internally, the
/// whole set sorted).  Construct via [`LogicRule::new`] so the invariants hold.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicRule {
    /// The derived axiom (consequent).
    pub head: LogicAxiom,
    /// The condition axioms (antecedents), in canonical order.
    pub body: Vec<LogicAxiom>,
    /// Inequality body guards: each pair of variable strings must bind to
    /// unequal values.  Canonicalized; append-only in the sort key.
    pub distinct_pairs: Vec<(String, String)>,
    /// Contextual scope for this rule.
    pub scope: ContextualScope,
    /// The `logic:NodeKind` this rule declares (default [`NodeKind::DerivationRule`]).
    /// A **distinct default sentinel** from [`LogicAxiom`]'s (`ObjectLevelFormula`); the
    /// two must not be conflated.  A rule therefore has two independent kind fold-points:
    /// its head axiom's kind (folded via `head.sort_key()`) and this, its own rule kind.
    /// Folded into the rule's keys only when non-default.
    pub node_kind: NodeKind,
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
            node_kind: NodeKind::DerivationRule,
        }
    }

    /// Set this rule's [`NodeKind`] (builder; default [`NodeKind::DerivationRule`]).
    /// Kept off [`Self::new`] so existing call sites and the byte-pinned default-kind key
    /// are unchanged.
    pub fn with_node_kind(mut self, node_kind: NodeKind) -> Self {
        self.node_kind = node_kind;
        self
    }

    /// Stable sort key — the golden-pinned key format.
    /// Corpus-safety: the distinct-pairs segment, then the rule's own `node_kind`, are
    /// appended only when non-default, in that order.  An all-default rule keeps its
    /// historical key.
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
        // The rule's OWN node kind (a distinct fold-point from the head axiom's kind,
        // which is already folded via head.sort_key() above): append AFTER the
        // distinct-pairs segment, only when != the rule default DerivationRule.
        if self.node_kind != NodeKind::DerivationRule {
            base.push(SEP);
            base.push_str(self.node_kind.as_str());
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
        let mut key = format!(
            "head[{}]{SEP}body[{body}]{SEP}distinct[{distinct}]{SEP}scope[{}]",
            self.head.content_key(),
            self.scope.content_key(),
        );
        // Append-only: the rule's own node kind, when != the DerivationRule default.
        if self.node_kind != NodeKind::DerivationRule {
            key.push(SEP);
            key.push_str(&format!("kind[{}]", self.node_kind.as_str()));
        }
        key
    }
}

/// The canonical reasoning-configuration IR: an independent selection
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
    /// Greenfield: there is no Python byte form to match; the only contract
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
// Predicate-path shapes
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

/// A named, parametric predicate-path traversal specification (`logic:PathShape`):
/// a reusable, by-name graph walk carrying a base step, a bounded depth
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
    /// * `min_depth` must be ≥ 1 and must not exceed [`MAX_PATH_DEPTH`] (hard cap
    ///   — an unbounded path with a huge `min_depth` still unrolls a `min_depth`-hop
    ///   chain, CWE-400);
    /// * when `max_depth` is `Some(m)`, `min_depth` must not exceed `m`;
    /// * when `max_depth` is `Some(m)`, `m` must not exceed [`MAX_PATH_DEPTH`]
    ///   (hard cap — prevents runaway Datalog unrolling, CWE-400);
    /// * a [`PathBase::NamedPredicate`] must be non-empty;
    /// * `namespace_scope`, when present, must be a non-empty IRI and is only
    ///   admissible for a [`PathBase::Wildcard`] step (it scopes the wildcard);
    /// * `depth_param`, when present, must be a non-empty string (an empty string
    ///   collides with `None` in the content key — a determinism hazard).
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
        // Cap min_depth too (CWE-400): an unbounded path (max_depth = None) with a
        // huge min_depth still unrolls a 1..min_depth edge chain in datalog_text,
        // exhausting memory. The cap on max_depth alone does not bound this.
        if min_depth as usize > MAX_PATH_DEPTH {
            return Err(format!(
                "PathShapeIr.min_depth ({min_depth}) exceeds the hard cap of {MAX_PATH_DEPTH}; \
                 no legitimate graph walk needs a deeper minimum"
            ));
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
                    "PathShapeIr.namespace_scope must be a non-empty IRI string when present; \
                     pass None to leave it unset"
                        .to_owned(),
                );
            }
            // namespace_scope only scopes a wildcard step (projections apply it
            // solely to wildcards). Carrying it on a named-predicate path is
            // malformed — reject rather than silently ignore it.
            if let PathBase::NamedPredicate(_) = &base {
                return Err(
                    "PathShapeIr.namespace_scope is only meaningful for a wildcard step \
                     (logic:pathWildcard true); a named-predicate path must not carry one"
                        .to_owned(),
                );
            }
        }
        // An empty/whitespace depth_param collides with None in content_key()
        // (content-addressing determinism hazard): Some("") and None must never
        // produce the same key. Reject it so Some("") can never be constructed.
        if let Some(dp) = &depth_param {
            if dp.trim().is_empty() {
                return Err(
                    "PathShapeIr.depth_param must be a non-empty string when present; \
                     pass None to leave it unset"
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
// The correspondence calculus (the ninth node kind's spine)
// --------------------------------------------------------------------------- //
//
// A [`Correspondence`] is the IR realization of `NodeKind::Correspondence`: an
// asymmetric lens (`get`/`put` legs) wrapped in a meta-formula envelope — a typed
// relation on an ordered lattice, an algebraic class on the ordered law-spine, the
// separated quantitative axes, claimed laws with discharge verdict, and a standpoint
// index.  Every facet is a closed value enum whose local names are taken verbatim from
// `slices/core/logic/module.ttl` (the `*_values_match_module_ttl` tests pin each set).
// See `design/LOGIC-CORRESPONDENCE.md`.

/// The `logic:CorrespondenceRelation` lattice: `Equiv` ⊐ {`Subsumes`, `SubsumedBy`} ⊐
/// `Overlaps` ⊐ `RelatedMatch`, with `Disjoint` the negative pole.  Variant order is
/// the lattice order (strongest first), so the derived `Ord` ranks relations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CorrespondenceRelation {
    /// `logic:Equiv` — same extension (lattice top).
    Equiv,
    /// `logic:Subsumes` — source broader than the view.
    Subsumes,
    /// `logic:SubsumedBy` — source narrower than the view.
    SubsumedBy,
    /// `logic:Overlaps` — shared instances, neither subsuming.
    Overlaps,
    /// `logic:RelatedMatch` — associated, no logical alignment.
    RelatedMatch,
    /// `logic:Disjoint` — asserted non-alignment (negative pole).
    Disjoint,
}

impl CorrespondenceRelation {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Equiv => "Equiv",
            Self::Subsumes => "Subsumes",
            Self::SubsumedBy => "SubsumedBy",
            Self::Overlaps => "Overlaps",
            Self::RelatedMatch => "RelatedMatch",
            Self::Disjoint => "Disjoint",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "Equiv" => Self::Equiv,
            "Subsumes" => Self::Subsumes,
            "SubsumedBy" => Self::SubsumedBy,
            "Overlaps" => Self::Overlaps,
            "RelatedMatch" => Self::RelatedMatch,
            "Disjoint" => Self::Disjoint,
            _ => return None,
        })
    }
}

impl fmt::Display for CorrespondenceRelation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `logic:MorphismClass` ordered law-spine — the seven rungs capping how much
/// invertibility a correspondence may lawfully claim, strongest first.  The derived
/// `Ord` is the spine order; composition can only weaken the rung, never strengthen it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MorphismClass {
    /// `logic:Isomorphism` — full round-trip both ways (top rung).
    Isomorphism,
    /// `logic:SectionRetraction` — perfect subsumption (`put ∘ get = id_S`).
    SectionRetraction,
    /// `logic:WellBehavedLens` — GetPut + PutGet (PutPut optional).
    WellBehavedLens,
    /// `logic:LossyLens` — non-injective `get`; one direction faithful.
    LossyLens,
    /// `logic:Prism` — partial map on a sum/optional, in-focus variant only.
    Prism,
    /// `logic:AffineCorrespondence` — co-projection onto a shared component.
    AffineCorrespondence,
    /// `logic:BridgeView` — commitment-shifting comorphism, no preservation (floor).
    BridgeView,
}

impl MorphismClass {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Isomorphism => "Isomorphism",
            Self::SectionRetraction => "SectionRetraction",
            Self::WellBehavedLens => "WellBehavedLens",
            Self::LossyLens => "LossyLens",
            Self::Prism => "Prism",
            Self::AffineCorrespondence => "AffineCorrespondence",
            Self::BridgeView => "BridgeView",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "Isomorphism" => Self::Isomorphism,
            "SectionRetraction" => Self::SectionRetraction,
            "WellBehavedLens" => Self::WellBehavedLens,
            "LossyLens" => Self::LossyLens,
            "Prism" => Self::Prism,
            "AffineCorrespondence" => Self::AffineCorrespondence,
            "BridgeView" => Self::BridgeView,
            _ => return None,
        })
    }
}

impl fmt::Display for MorphismClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `logic:MorphismKind` qualifier, orthogonal to the rung: the
/// satisfaction-preserving / commitment-shifting split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MorphismKind {
    /// `logic:InstitutionMorphism` — satisfaction-preserving morphism.
    InstitutionMorphism,
    /// `logic:CommitmentShiftingBridge` — by-reference bridge; the loss ledger refuses
    /// `owl:equivalentClass` for it.
    CommitmentShiftingBridge,
}

impl MorphismKind {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InstitutionMorphism => "InstitutionMorphism",
            Self::CommitmentShiftingBridge => "CommitmentShiftingBridge",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "InstitutionMorphism" => Self::InstitutionMorphism,
            "CommitmentShiftingBridge" => Self::CommitmentShiftingBridge,
            _ => return None,
        })
    }
}

impl fmt::Display for MorphismKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `logic:Determinacy` axis: whether the target relationship is ontically crisp or
/// vague (kept distinct from `logic:confidence`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Determinacy {
    /// `logic:Crisp` — ontically sharp.
    Crisp,
    /// `logic:Vague` — ontically fuzzy (pairs with `AffineCorrespondence`).
    Vague,
}

impl Determinacy {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Crisp => "Crisp",
            Self::Vague => "Vague",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "Crisp" => Self::Crisp,
            "Vague" => Self::Vague,
            _ => return None,
        })
    }
}

impl fmt::Display for Determinacy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `logic:CorrespondenceLaw` value class: the lens laws a correspondence may claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CorrespondenceLaw {
    /// `logic:GetPut` — `put(get(s), s) = s` (acquisition stability).
    GetPut,
    /// `logic:PutGet` — `get(put(v, s)) = v` (update faithfulness).
    PutGet,
    /// `logic:PutPut` — `put(v2, put(v1, s)) = put(v2, s)` (very-well-behaved).
    PutPut,
    /// `logic:SectionLaw` — `put ∘ get = id_S` (perfect subsumption).
    SectionLaw,
}

impl CorrespondenceLaw {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GetPut => "GetPut",
            Self::PutGet => "PutGet",
            Self::PutPut => "PutPut",
            Self::SectionLaw => "SectionLaw",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "GetPut" => Self::GetPut,
            "PutGet" => Self::PutGet,
            "PutPut" => Self::PutPut,
            "SectionLaw" => Self::SectionLaw,
            _ => return None,
        })
    }
}

impl fmt::Display for CorrespondenceLaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `logic:DischargeVerdict` value class (reused from the foundation's
/// non-entailment machinery): the result a law-claim check returns.  A typed IR mirror
/// of the `logic:DischargeVerdict` individuals in `module.ttl` — the foundation engine
/// works over the graph in string wire form; this is the IR-layer enum so a
/// `Correspondence` carries its law verdicts typed and content-addressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DischargeVerdict {
    /// `logic:ObligationDischarged` — conclusively checked within a declared condition.
    ObligationDischarged,
    /// `logic:ObligationUnknown` — not-yet-discharged, carried forward (never "proved
    /// absent"); the honest verdict for an unchecked or inconclusive law.
    ObligationUnknown,
    /// `logic:ObligationViolated` — the law is refuted (a hard error).
    ObligationViolated,
}

impl DischargeVerdict {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ObligationDischarged => "ObligationDischarged",
            Self::ObligationUnknown => "ObligationUnknown",
            Self::ObligationViolated => "ObligationViolated",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "ObligationDischarged" => Self::ObligationDischarged,
            "ObligationUnknown" => Self::ObligationUnknown,
            "ObligationViolated" => Self::ObligationViolated,
            _ => return None,
        })
    }
}

impl fmt::Display for DischargeVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `logic:DischargeCondition` value class (reused from the foundation): the
/// condition under which a law claim's verdict is conclusively checkable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DischargeCondition {
    /// `logic:DischargeCertifiedFragment`.
    DischargeCertifiedFragment,
    /// `logic:DischargeFiniteClosure`.
    DischargeFiniteClosure,
    /// `logic:DischargeSyntacticReachability`.
    DischargeSyntacticReachability,
    /// `logic:DischargeConservativeExtension`.
    DischargeConservativeExtension,
    /// `logic:DischargeBoundedCorpus`.
    DischargeBoundedCorpus,
}

impl DischargeCondition {
    /// The local name exactly as it appears in `module.ttl`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DischargeCertifiedFragment => "DischargeCertifiedFragment",
            Self::DischargeFiniteClosure => "DischargeFiniteClosure",
            Self::DischargeSyntacticReachability => "DischargeSyntacticReachability",
            Self::DischargeConservativeExtension => "DischargeConservativeExtension",
            Self::DischargeBoundedCorpus => "DischargeBoundedCorpus",
        }
    }

    /// The full IRI (`LOGIC_NAMESPACE + local_name`).
    pub fn iri(&self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.as_str())
    }

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "DischargeCertifiedFragment" => Self::DischargeCertifiedFragment,
            "DischargeFiniteClosure" => Self::DischargeFiniteClosure,
            "DischargeSyntacticReachability" => Self::DischargeSyntacticReachability,
            "DischargeConservativeExtension" => Self::DischargeConservativeExtension,
            "DischargeBoundedCorpus" => Self::DischargeBoundedCorpus,
            _ => return None,
        })
    }
}

impl fmt::Display for DischargeCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `logic:LawClaim`: a claimed [`CorrespondenceLaw`] with its discharge state — the
/// [`DischargeVerdict`] and, when checked under one, the [`DischargeCondition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LawClaimIr {
    /// The lens law being claimed (`logic:lawClaimed`).
    pub law: CorrespondenceLaw,
    /// The verdict the check returned (`logic:lawDischargeVerdict`).
    pub verdict: DischargeVerdict,
    /// The condition the verdict was established under (`logic:lawDischargeCondition`);
    /// `None` for an authored-but-unverified claim.
    pub condition: Option<DischargeCondition>,
}

impl LawClaimIr {
    /// A deterministic content / sort key (law, then verdict, then condition).
    pub fn sort_key(&self) -> String {
        format!(
            "{}{SEP}{}{SEP}{}",
            self.law.as_str(),
            self.verdict.as_str(),
            self.condition.map(|c| c.as_str()).unwrap_or(""),
        )
    }
}

/// Format an optional unit-interval axis for the content key, collapsing `-0.0` to
/// `0.0` (signed-zero determinism) and rendering `None` as the empty string.
fn opt_axis_key(v: Option<f64>) -> String {
    match v {
        Some(x) => {
            let x = if x == 0.0 { 0.0 } else { x };
            x.to_string()
        }
        None => String::new(),
    }
}

/// A `logic:Correspondence` IR node — the ninth node kind realized: an asymmetric lens
/// (the `get`/`put` legs) wrapped in a relation/axes/laws/standpoint envelope.
///
/// Identity is content-addressed: [`Correspondence::sort_key`] is the IRI and
/// [`Correspondence::content_key`] folds every field deterministically (the `law_claims`
/// are sorted and deduped at construction, so two correspondences differing only in the
/// order their claims were supplied compare equal).  No `Eq`/`Hash` derive: the
/// quantitative axes are `f64` (mirrors [`LogicAxiom`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Correspondence {
    /// IRI string of the correspondence individual (identity).
    pub iri: String,
    /// The typed relation on the lattice (`logic:correspondenceRelation`).
    pub relation: CorrespondenceRelation,
    /// The rung on the ordered law-spine (`logic:morphismClass`).
    pub morphism_class: MorphismClass,
    /// The satisfaction-preserving / commitment-shifting qualifier (`logic:morphismKind`).
    pub morphism_kind: MorphismKind,
    /// Whether the forward leg retains a source witness (`logic:mnemomorphic`); the bit
    /// that lets a correspondence claim `SectionLaw`.  Defaults `false`.
    pub mnemomorphic: bool,
    /// Whether the target relationship is crisp or vague (`logic:hasDeterminacy`).
    pub determinacy: Option<Determinacy>,
    /// IRI of the `logic:TransactionProgram` realizing the get leg (`logic:getLeg`).
    pub get_leg: Option<String>,
    /// IRI of the `logic:TransactionProgram` realizing the put leg (`logic:putLeg`).
    pub put_leg: Option<String>,
    /// The claimed lens laws with discharge state (`logic:hasLawClaim`); sorted+deduped.
    pub law_claims: Vec<LawClaimIr>,
    /// `logic:confidence` — curator's epistemic confidence in `[0, 1]`.
    pub confidence: Option<f64>,
    /// `logic:evidenceStrength` — provenance-derived warrant in `[0, 1]`.
    pub evidence_strength: Option<f64>,
    /// `logic:weight` — solver ranking (finite; not range-bound).
    pub weight: Option<f64>,
    /// `logic:probability` — only under a declared dependency model; in `[0, 1]`.
    pub probability: Option<f64>,
    /// IRI of the standpoint (`logic:accordingTo`); `None` ⇒ unspecified standpoint
    /// (unspecified, not universal).
    pub according_to: Option<String>,
}

impl Correspondence {
    /// Construct a correspondence, validating identity, the optional-string
    /// determinism guards, and the unit-interval axes, and canonicalizing the law
    /// claims (sorted + deduped at construction).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        iri: impl Into<String>,
        relation: CorrespondenceRelation,
        morphism_class: MorphismClass,
        morphism_kind: MorphismKind,
        mnemomorphic: bool,
        determinacy: Option<Determinacy>,
        get_leg: Option<String>,
        put_leg: Option<String>,
        law_claims: Vec<LawClaimIr>,
        confidence: Option<f64>,
        evidence_strength: Option<f64>,
        weight: Option<f64>,
        probability: Option<f64>,
        according_to: Option<String>,
    ) -> Result<Self, String> {
        let iri = iri.into();
        if iri.is_empty() {
            return Err("Correspondence.iri must be a non-empty IRI string".to_owned());
        }
        // Some("") collides with None in content_key() (content-addressing determinism
        // hazard): reject an empty/whitespace optional string so Some("") is never built.
        for (field, val) in [
            ("get_leg", &get_leg),
            ("put_leg", &put_leg),
            ("according_to", &according_to),
        ] {
            if let Some(s) = val {
                if s.trim().is_empty() {
                    return Err(format!(
                        "Correspondence.{field} must be a non-empty IRI string when present; \
                         pass None to leave it unset"
                    ));
                }
            }
        }
        // The unit-interval axes must be a finite value in [0, 1]; `weight` is a finite
        // ranking, not range-bound.  NaN/infinite would break content-key determinism.
        for (field, val) in [
            ("confidence", confidence),
            ("evidence_strength", evidence_strength),
            ("probability", probability),
        ] {
            if let Some(x) = val {
                if !(0.0..=1.0).contains(&x) {
                    return Err(format!("Correspondence.{field} must be in [0, 1], got {x}"));
                }
            }
        }
        if let Some(w) = weight {
            if !w.is_finite() {
                return Err(format!("Correspondence.weight must be finite, got {w}"));
            }
        }
        let mut law_claims = law_claims;
        law_claims.sort_by_cached_key(LawClaimIr::sort_key);
        law_claims.dedup();
        Ok(Self {
            iri,
            relation,
            morphism_class,
            morphism_kind,
            mnemomorphic,
            determinacy,
            get_leg,
            put_leg,
            law_claims,
            confidence,
            evidence_strength,
            weight,
            probability,
            according_to,
        })
    }

    /// Stable sort key for canonical ordering — the correspondence IRI is unique.
    pub fn sort_key(&self) -> String {
        self.iri.clone()
    }

    /// A deterministic full-content key for canonical equality, folding every field
    /// with explicit `name=value` framing and empty-string defaults.
    fn content_key(&self) -> String {
        let claims = self
            .law_claims
            .iter()
            .map(LawClaimIr::sort_key)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{}{SEP}rel={}{SEP}class={}{SEP}kind={}{SEP}mnemo={}{SEP}det={}{SEP}\
             get={}{SEP}put={}{SEP}conf={}{SEP}ev={}{SEP}w={}{SEP}prob={}{SEP}\
             at={}{SEP}laws={claims}",
            self.iri,
            self.relation.as_str(),
            self.morphism_class.as_str(),
            self.morphism_kind.as_str(),
            py_bool(self.mnemomorphic),
            self.determinacy.map(|d| d.as_str()).unwrap_or(""),
            self.get_leg.as_deref().unwrap_or(""),
            self.put_leg.as_deref().unwrap_or(""),
            opt_axis_key(self.confidence),
            opt_axis_key(self.evidence_strength),
            opt_axis_key(self.weight),
            opt_axis_key(self.probability),
            self.according_to.as_deref().unwrap_or(""),
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
    /// Reasoning contracts in canonical order (was `profiles`).
    pub contracts: Vec<ReasoningContract>,
    /// Named/parametric predicate-path shapes in canonical order (`logic:PathShape`).
    /// Attached via [`LogicProgram::with_path_shapes`]; empty for the
    /// historical path-shape-free corpus, so the canonical key is unchanged there.
    pub path_shapes: Vec<PathShapeIr>,
    /// Correspondence-calculus nodes in canonical order (`logic:Correspondence`).
    /// Attached via [`LogicProgram::with_correspondences`]; empty for the
    /// historical correspondence-free corpus, so the canonical key is unchanged there.
    pub correspondences: Vec<Correspondence>,
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
            correspondences: Vec::new(),
            source_iri,
        }
    }

    /// Attach the program's `logic:PathShape` individuals, canonicalizing
    /// them into sorted order.  Kept separate from [`Self::new`] so existing call
    /// sites are untouched and the byte-pinned canonical key of a path-shape-free
    /// program is unchanged (the path-shapes segment is append-only when present).
    pub fn with_path_shapes(mut self, path_shapes: Vec<PathShapeIr>) -> Self {
        let mut path_shapes = path_shapes;
        path_shapes.sort_by_cached_key(PathShapeIr::sort_key);
        self.path_shapes = path_shapes;
        self
    }

    /// Attach the program's `logic:Correspondence` nodes, canonicalizing them into
    /// sorted order.  Kept separate from [`Self::new`] so existing call sites are
    /// untouched and the byte-pinned canonical key of a correspondence-free program is
    /// unchanged (the correspondences segment is append-only when present).
    pub fn with_correspondences(mut self, correspondences: Vec<Correspondence>) -> Self {
        let mut correspondences = correspondences;
        correspondences.sort_by_cached_key(Correspondence::sort_key);
        self.correspondences = correspondences;
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
        // Append-only: a correspondence-free program keeps its exact historical key.
        if !self.correspondences.is_empty() {
            let corr = self
                .correspondences
                .iter()
                .map(Correspondence::content_key)
                .collect::<Vec<_>>()
                .join("\n");
            key.push_str("\nCORRESPONDENCES\n");
            key.push_str(&corr);
        }
        key
    }
}

#[cfg(test)]
mod tests;
