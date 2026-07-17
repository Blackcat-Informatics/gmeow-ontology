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

use gmeow_errors::Diag;

/// The null-byte field separator used by every `sort_key` (Python `"\x00"`).
const SEP: char = '\u{0}';

/// The `logic:` namespace; `iri()` helpers expand a local enum name to its IRI.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE` (and the runtime
/// `gmeow_logic::provenance::LOGIC_NAMESPACE`).
pub const LOGIC_NAMESPACE: &str = "https://blackcatinformatics.ca/logic/";

/// The `logic:ResourcePolicy` facet value (local name) that licenses native
/// operational builtins. Only the procedural preset expands to it
/// (`logic:expandsToFacet`).
pub const PROCEDURAL_EXECUTION_FACET: &str = "ProceduralExecution";

// --------------------------------------------------------------------------- //
// Enums — single source of truth, local names taken verbatim from module.ttl
// --------------------------------------------------------------------------- //

/// The six historical reasoning **preset** ids — the named `logic:ReasoningPreset`
/// individuals (formerly `logic:SemanticProfile`).
///
/// The string form ([`SemanticProfileId::as_str`]) is the local name (no
/// `logic:` prefix), taken verbatim from `slices/grounding/logic/module.ttl` — any
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
    /// (`logic:ProceduralExecution`) and therefore licenses native builtins.
    ///
    /// This mirrors the `logic:expandsToFacet` bundle authored in `module.ttl` (only
    /// `ProceduralPrologProfile` expands to `logic:ProceduralExecution`); the
    /// `procedural_preset_carries_procedural_execution_facet` test ties this Rust fact
    /// to the ontology surface so the two cannot silently diverge.
    pub fn permits_procedural_execution(self) -> bool {
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
/// local name taken verbatim from `slices/grounding/logic/module.ttl`; any change there must
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
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

    /// Parse a local name back to the enum (inverse of [`Self::as_str`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "ExactPreservation" => Self::Exact,
            "SoundUnderApproximation" => Self::SoundUnder,
            "CompleteOverApproximation" => Self::CompleteOver,
            "ValidationOnly" => Self::ValidationOnly,
            "InconsistencyPreserving" => Self::InconsistencyPreserving,
            "InconsistencyReflecting" => Self::InconsistencyReflecting,
            "Unsupported" => Self::Unsupported,
            _ => return None,
        })
    }
}

impl PreservationKind {
    /// Every preservation kind, ordered BOTTOM (most-preserving) to TOP
    /// (least-preserving) — drives the exhaustive lattice-law test and any
    /// worst-preservation fold over the loss ledger.
    pub const ALL: [PreservationKind; 7] = [
        PreservationKind::Exact,
        PreservationKind::SoundUnder,
        PreservationKind::CompleteOver,
        PreservationKind::InconsistencyPreserving,
        PreservationKind::InconsistencyReflecting,
        PreservationKind::ValidationOnly,
        PreservationKind::Unsupported,
    ];

    /// Position on the preservation-strength chain: `0` = most-preserving
    /// ([`Exact`](Self::Exact), the BOTTOM), `6` = least-preserving
    /// ([`Unsupported`](Self::Unsupported), the legalization floor / TOP). The chain
    /// is deliberate: `Exact` (0) is lossless; `SoundUnder` (1) and `CompleteOver`
    /// (2) are one-sided entailment approximations (sound-but-incomplete then
    /// complete-but-unsound), still carrying entailment; `InconsistencyPreserving`
    /// (3) and `InconsistencyReflecting` (4) carry only the (in)consistency verdict
    /// (reflecting is weaker than preserving); `ValidationOnly` (5) carries no
    /// entailment at all; `Unsupported` (6) carries nothing but the residue flag.
    ///
    /// The order among the middle five is a documented design choice; the
    /// load-bearing property is the endpoints and the monotone rank, so the
    /// [`join`](gmeow_errors::BoundedLattice::join) of two witnesses returns the
    /// LESS-preserving (higher-rank) one — worst-preservation-wins.
    fn preservation_rank(self) -> u8 {
        match self {
            Self::Exact => 0,
            Self::SoundUnder => 1,
            Self::CompleteOver => 2,
            Self::InconsistencyPreserving => 3,
            Self::InconsistencyReflecting => 4,
            Self::ValidationOnly => 5,
            Self::Unsupported => 6,
        }
    }
}

impl gmeow_errors::BoundedLattice for PreservationKind {
    /// The most-preserving reading: [`Exact`](Self::Exact).
    const BOTTOM: Self = PreservationKind::Exact;
    /// The least-preserving reading (the legalization floor):
    /// [`Unsupported`](Self::Unsupported).
    const TOP: Self = PreservationKind::Unsupported;

    /// **Worst-preservation-wins**: the join of two loss witnesses at one ledger
    /// anchor is the LESS-preserving (higher-rank) reading, so the surviving
    /// preservation is the worse of the two — a projection is never reported as
    /// better-preserving than its worst disclosed loss.
    fn join(self, other: Self) -> Self {
        if self.preservation_rank() >= other.preservation_rank() {
            self
        } else {
            other
        }
    }

    /// The dual: the more-preserving (lower-rank) reading.
    fn meet(self, other: Self) -> Self {
        if self.preservation_rank() <= other.preservation_rank() {
            self
        } else {
            other
        }
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
    pub fn new(label: impl Into<String>) -> gmeow_errors::Result<Self> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "ComplexityClass.label must be a non-empty string".to_owned(),
            }));
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
    /// IRI string of the `logic:Module` (Common Logic module / named theory) this
    /// statement is asserted in — the theory-structuring context dimension, orthogonal
    /// to the epistemic `standpoint`/`modality` dimension (a module is *where* a sentence
    /// lives, not *under whose perspective* it holds). `None` = the top-level module.
    pub module: Option<String>,
}

impl ContextualScope {
    /// Construct, validating the confidence range.
    pub fn new(
        standpoint: Option<String>,
        time: Option<String>,
        confidence: Option<f64>,
        modality: LogicModality,
        provenance: Option<String>,
        module: Option<String>,
    ) -> gmeow_errors::Result<Self> {
        if let Some(c) = confidence
            && !(0.0..=1.0).contains(&c)
        {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: format!("ContextualScope.confidence must be in [0, 1], got {c}"),
            }));
        }
        Ok(Self {
            standpoint,
            time,
            confidence,
            modality,
            provenance,
            module,
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
            "st={}{SEP}t={}{SEP}c={conf}{SEP}m={}{SEP}p={}{SEP}mod={}",
            self.standpoint.as_deref().unwrap_or(""),
            self.time.as_deref().unwrap_or(""),
            self.modality.as_str(),
            self.provenance.as_deref().unwrap_or(""),
            self.module.as_deref().unwrap_or(""),
        )
    }
}

// --------------------------------------------------------------------------- //
// Core IR nodes
// --------------------------------------------------------------------------- //

/// Render a Python `bool` exactly as its `Display` (`True` / `False`) — the
/// `sort_key` byte-parity hinge.
fn py_bool(b: bool) -> &'static str {
    if b { "True" } else { "False" }
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
    ) -> gmeow_errors::Result<Self> {
        let subject = subject.into();
        let predicate = predicate.into();
        if subject.is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "LogicAxiom.subject must be a non-empty IRI string".to_owned(),
            }));
        }
        if predicate.is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "LogicAxiom.predicate must be a non-empty IRI string".to_owned(),
            }));
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
    ) -> gmeow_errors::Result<Self> {
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

/// A stratified **aggregation** (reduce) specification on a [`LogicRule`]: the rule's head
/// binds `result_var` to `function` applied to `aggregate_var` over the groups formed by
/// `group_keys`. This is the canonical `logic:` representation of the "reduce" half of the
/// computation surface (the "map" half is an ordinary derivation rule); it lowers to an
/// aggregating rule and to a SHACL-AF `GROUP BY` sub-`SELECT`. A rule without an
/// `AggregateSpec` is an ordinary Horn rule, so the existing corpus is byte-identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateSpec {
    /// The aggregate function, an upper-case name (`SUM`, `COUNT`, `MIN`, `MAX`, `AVG`).
    pub function: String,
    /// The body variable being aggregated (e.g. `?x`).
    pub aggregate_var: String,
    /// The head variable the aggregate result binds (e.g. `?total`).
    pub result_var: String,
    /// The group-by variables, in canonical (sorted) order.
    pub group_keys: Vec<String>,
}

impl AggregateSpec {
    /// Construct, canonicalizing the group-key order (a `GROUP BY` is set-semantic, so the key
    /// order does not change meaning — sorting makes the content identity and the projections
    /// deterministic).
    pub fn new(
        function: impl Into<String>,
        aggregate_var: impl Into<String>,
        result_var: impl Into<String>,
        group_keys: Vec<String>,
    ) -> Self {
        let mut group_keys = group_keys;
        group_keys.sort();
        group_keys.dedup();
        Self {
            function: function.into(),
            aggregate_var: aggregate_var.into(),
            result_var: result_var.into(),
            group_keys,
        }
    }

    /// The append-only key segment for this spec (folded into a rule's keys only when present).
    fn key_segment(&self) -> String {
        format!(
            "{}{SEP}{}{SEP}{}{SEP}{}",
            self.function,
            self.aggregate_var,
            self.result_var,
            self.group_keys.join("|")
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
    /// The stratified aggregation (reduce) specification, when this rule is a reduce rule.
    /// Default-absent: an ordinary Horn rule carries `None`, so the existing corpus is
    /// byte-identical. Folded into the rule's keys only when present (append-only).
    pub aggregation: Option<AggregateSpec>,
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
            aggregation: None,
        }
    }

    /// Set this rule's [`NodeKind`] (builder; default [`NodeKind::DerivationRule`]).
    /// Kept off [`Self::new`] so existing call sites and the byte-pinned default-kind key
    /// are unchanged.
    pub fn with_node_kind(mut self, node_kind: NodeKind) -> Self {
        self.node_kind = node_kind;
        self
    }

    /// Attach a stratified aggregation (reduce) spec (builder; default `None`). Kept off
    /// [`Self::new`] so existing call sites and the byte-pinned no-aggregation key are unchanged.
    pub fn with_aggregation(mut self, aggregation: AggregateSpec) -> Self {
        self.aggregation = Some(aggregation);
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
        // The aggregation (reduce) spec: append AFTER the node-kind segment, only when present,
        // so a non-aggregating rule keeps its historical key.
        if let Some(agg) = &self.aggregation {
            base.push(SEP);
            base.push_str("agg=");
            base.push_str(&agg.key_segment());
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
        // Append-only: the aggregation (reduce) spec, when present.
        if let Some(agg) = &self.aggregation {
            key.push(SEP);
            key.push_str(&format!("agg[{}]", agg.key_segment()));
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

    /// `true` iff this contract licenses native procedural builtins — i.e. its
    /// resource/execution policy carries the procedural-execution facet
    /// ([`PROCEDURAL_EXECUTION_FACET`], `logic:ProceduralExecution`).
    pub fn permits_procedural_execution(&self) -> bool {
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
/// than this would cause [`datalog_text`](crate::projections::paths::datalog_text)
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
    ) -> gmeow_errors::Result<Self> {
        let iri = iri.into();
        if iri.is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "PathShapeIr.iri must be a non-empty IRI string".to_owned(),
            }));
        }
        if min_depth < 1 {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "PathShapeIr.min_depth must be >= 1".to_owned(),
            }));
        }
        // Cap min_depth too (CWE-400): an unbounded path (max_depth = None) with a
        // huge min_depth still unrolls a 1..min_depth edge chain in datalog_text,
        // exhausting memory. The cap on max_depth alone does not bound this.
        if min_depth as usize > MAX_PATH_DEPTH {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: format!(
                    "PathShapeIr.min_depth ({min_depth}) exceeds the hard cap of {MAX_PATH_DEPTH}; \
                 no legitimate graph walk needs a deeper minimum"
                ),
            }));
        }
        if let Some(m) = max_depth {
            if min_depth > m {
                return Err(Diag::of_kind(crate::error::Ir {
                    detail: format!(
                        "PathShapeIr min_depth ({min_depth}) must not exceed max_depth ({m})"
                    ),
                }));
            }
            if m as usize > MAX_PATH_DEPTH {
                return Err(Diag::of_kind(crate::error::Ir {
                    detail: format!(
                        "PathShapeIr max_depth ({m}) exceeds the hard cap of {MAX_PATH_DEPTH}; \
                     use an unbounded path (max_depth = None) for deeper traversals"
                    ),
                }));
            }
        }
        if let PathBase::NamedPredicate(p) = &base
            && p.is_empty()
        {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "PathShapeIr named-predicate step must be a non-empty IRI".to_owned(),
            }));
        }
        if let Some(ns) = &namespace_scope {
            if ns.trim().is_empty() {
                return Err(Diag::of_kind(crate::error::Ir {
                    detail:
                        "PathShapeIr.namespace_scope must be a non-empty IRI string when present; \
                     pass None to leave it unset"
                            .to_owned(),
                }));
            }
            // namespace_scope only scopes a wildcard step (projections apply it
            // solely to wildcards). Carrying it on a named-predicate path is
            // malformed — reject rather than silently ignore it.
            if let PathBase::NamedPredicate(_) = &base {
                return Err(Diag::of_kind(crate::error::Ir {
                    detail: "PathShapeIr.namespace_scope is only meaningful for a wildcard step \
                     (logic:pathWildcard true); a named-predicate path must not carry one"
                        .to_owned(),
                }));
            }
        }
        // An empty/whitespace depth_param collides with None in content_key()
        // (content-addressing determinism hazard): Some("") and None must never
        // produce the same key. Reject it so Some("") can never be constructed.
        if let Some(dp) = &depth_param
            && dp.trim().is_empty()
        {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "PathShapeIr.depth_param must be a non-empty string when present; \
                     pass None to leave it unset"
                    .to_owned(),
            }));
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
// `slices/grounding/logic/module.ttl` (the `*_values_match_module_ttl` tests pin each set).
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

/// The realized body of a correspondence leg (`logic:getLeg` / `logic:putLeg`): the
/// `logic:TransactionProgram` a leg IRI resolves to, expressed in the canonical `logic:`
/// composite-path vocabulary (`gm:SeqPath` / `gm:InversePath` / `gm:AltPath`).
///
/// This is the canonical structured form; the SPARQL property-path string
/// ([`super::projections::paths::leg_path_canonical`]) is its **projection** (Principle 17),
/// used only as the content-addressed key the round-trip gate compares. Identity is the
/// projected canonical text, so two bodies are "the same leg" iff their normalized path
/// expressions are graph-isomorphic — never a hash of surrounding metadata.
///
/// A lawful `put` leg is the structural [`LegPath::invert`] of its `get` leg: that is what
/// makes `put ∘ get = id` a *decidable* canonical-IR identity (the spec's graph-iso check)
/// rather than a data-execution round-trip (the F3 executor, off this path).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LegPath {
    /// A single forward predicate step (`gm:SeqPath` member / bare predicate IRI).
    Step(String),
    /// The structural reverse of a sub-path (`gm:InversePath`): `^p`.
    Inverse(Box<LegPath>),
    /// Left-to-right sequential composition (`gm:SeqPath`): `a / b / …`.
    Seq(Vec<LegPath>),
    /// Alternation (`gm:AltPath`): `a | b | …`.
    Alt(Vec<LegPath>),
}

impl LegPath {
    /// The structural reverse of this path — the lawful inverse leg. `reverse` is an
    /// involution: `reverse(^x) = x`, `reverse(a/b/c) = ^c / ^b / ^a`, and `reverse` of an
    /// alternation reverses each branch. A lawful `put` leg equals `get.invert()`, so the
    /// round-trip gate verifies `put == get.invert()` over the normalized canonical form.
    pub fn invert(&self) -> LegPath {
        match self {
            LegPath::Step(_) => LegPath::Inverse(Box::new(self.clone())),
            // reverse(^x) = x — never accumulate a double inverse.
            LegPath::Inverse(inner) => (**inner).clone(),
            LegPath::Seq(parts) => LegPath::Seq(parts.iter().rev().map(LegPath::invert).collect()),
            LegPath::Alt(parts) => LegPath::Alt(parts.iter().map(LegPath::invert).collect()),
        }
    }

    /// The canonical normal form: cancel double inverses (`^^x → x`), flatten nested
    /// `Seq`/`Alt`, and drop singleton `Seq`/`Alt`. Two paths with the same normal form are
    /// the same leg — the decidable identity the round-trip / mnemomorphism gates compare.
    pub fn normalize(&self) -> LegPath {
        match self {
            LegPath::Step(p) => LegPath::Step(p.clone()),
            LegPath::Inverse(inner) => match inner.normalize() {
                // ^^x → x
                LegPath::Inverse(x) => *x,
                other => LegPath::Inverse(Box::new(other)),
            },
            LegPath::Seq(parts) => {
                let mut flat = Vec::new();
                for p in parts {
                    match p.normalize() {
                        LegPath::Seq(inner) => flat.extend(inner),
                        other => flat.push(other),
                    }
                }
                if flat.len() == 1 {
                    flat.pop().expect("len checked")
                } else {
                    LegPath::Seq(flat)
                }
            }
            LegPath::Alt(parts) => {
                let mut flat = Vec::new();
                for p in parts {
                    match p.normalize() {
                        LegPath::Alt(inner) => flat.extend(inner),
                        other => flat.push(other),
                    }
                }
                if flat.len() == 1 {
                    flat.pop().expect("len checked")
                } else {
                    LegPath::Alt(flat)
                }
            }
        }
    }
}

/// A resolvable `logic:TransactionProgram` node: a leg IRI bound to its realized
/// [`LegPath`] body. The registry the `logic:getLeg` / `logic:putLeg` IRIs on a
/// [`Correspondence`] resolve through, so the round-trip gate can compose the actual leg
/// bodies rather than compare opaque IRIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionProgramIr {
    /// IRI of the leg program individual (what a `logic:getLeg` / `logic:putLeg` names).
    pub iri: String,
    /// The realized leg path.
    pub body: LegPath,
}

impl TransactionProgramIr {
    /// The content key: the IRI bound to the canonical (normalized, projected) path text.
    /// Two leg programs are the same iff they share this key.
    pub fn content_key(&self) -> String {
        format!(
            "{}={}",
            self.iri,
            crate::projections::paths::leg_path_canonical(&self.body)
        )
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

    /// Whether the rung is injective enough to carry a full round-trip / section claim:
    /// the top three rungs (iso, section/retraction, well-behaved lens). Matched
    /// EXPLICITLY rather than via the derived `Ord` (`<= WellBehavedLens`) so a future
    /// reordering of the spine variants cannot silently change injectivity classification —
    /// the load-bearing predicate every correspondence gate keys on.
    pub fn is_injective_rung(self) -> bool {
        matches!(
            self,
            Self::Isomorphism | Self::SectionRetraction | Self::WellBehavedLens
        )
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

/// One declared query-class case for executed correspondence recovery.
///
/// The case is neutral evidence: its transform may discharge a genuine recovery or refute a
/// lossy one.  It therefore never substitutes for the correspondence's `mnemomorphic` claim;
/// the native executor decides the claim from the case's behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryCaseIr {
    /// IRI of the first-class `logic:RecoveryCase` node.
    pub iri: String,
    /// The ordered, universally quantified source-to-view transform
    /// (`logic:recoveryTransform`).  The native correspondence executor accepts the
    /// positive-conjunctive binary fragment and derives the candidate put direction from it;
    /// a case may therefore either discharge recovery or produce a countermodel.
    pub transform: Formula,
}

impl RecoveryCaseIr {
    /// Build a recovery case, rejecting an empty identity.  Executability of the formula is
    /// deliberately checked by the native executor: the IR carries full `logic:Formula`, while
    /// an out-of-fragment recovery claim must become an explicit violated/unknown discharge,
    /// never disappear during parsing.
    pub fn new(iri: impl Into<String>, transform: Formula) -> gmeow_errors::Result<Self> {
        let iri = iri.into();
        if iri.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "RecoveryCaseIr.iri must be a non-empty IRI string".to_owned(),
            }));
        }
        Ok(Self { iri, transform })
    }

    /// Deterministic full-content identity.
    pub fn content_key(&self) -> String {
        format!("{}{}{}", self.iri, SEP, self.transform.content_key())
    }
}

/// A `logic:Correspondence` IR node — the ninth node kind realized: an asymmetric lens
/// (the `get`/`put` legs) wrapped in a relation/axes/laws/standpoint envelope.
///
/// Identity is content-addressed: the IRI is the sort key (compared directly on the
/// `iri` field) and [`Correspondence::content_key`] folds every field deterministically (the
/// `law_claims` and recovery cases are canonicalized at construction). No `Eq`/`Hash` derive:
/// the quantitative axes are `f64` (mirrors [`LogicAxiom`]).
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
    /// IRI of the standpoint (`gmeow:accordingTo`); `None` ⇒ unspecified standpoint
    /// (unspecified, not universal).
    pub according_to: Option<String>,
    /// The declared preservation judgment (`logic:preservationKind`) — the loss residue
    /// this correspondence's lowering carries (Principle 17: the logic core is canonical,
    /// every dialect a lossy projection). `None` when the correspondence authors no rung; a
    /// lossy correspondence authoring a non-[`PreservationKind::Exact`] kind is folded into
    /// the loss ledger as ONE per-correspondence preservation row (the canonical doc's "one
    /// preservation row per correspondence"), so the dropped construct is never DARK.
    pub preservation: Option<PreservationKind>,
    /// The source endpoint of a term-level correspondence (`logic:sourceEndpoint`).
    /// This is distinct from [`Self::get_leg`]: an endpoint names the term or pattern being
    /// related, while a leg names the executable transaction program that performs a
    /// projection. `None` for correspondences whose endpoints are expressed only by their
    /// executable legs.
    pub source_endpoint: Option<String>,
    /// The target endpoint of a term-level correspondence (`logic:targetEndpoint`).
    /// Kept separate from [`Self::put_leg`] for the same reason as
    /// [`Self::source_endpoint`].
    pub target_endpoint: Option<String>,
    /// Whether this correspondence belongs to the co-foundational grounding seam and is
    /// therefore also projected as a `logic:GroundingCorrespondence`. Ordinary consumer
    /// mappings remain plain `logic:Correspondence` nodes.
    pub grounding: bool,
    /// Executable recovery cases (`logic:recoveryCase`) declaring the complete source graph
    /// patterns over which this correspondence's recovery law is decided.  Presence does NOT
    /// assert recoverability: a lossy correspondence may carry a deliberately refuting case.
    /// Sorted by IRI and unique at construction through [`Self::with_recovery_cases`].
    pub recovery_cases: Vec<RecoveryCaseIr>,
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
        preservation: Option<PreservationKind>,
    ) -> gmeow_errors::Result<Self> {
        let iri = iri.into();
        if iri.is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Correspondence.iri must be a non-empty IRI string".to_owned(),
            }));
        }
        // Some("") collides with None in content_key() (content-addressing determinism
        // hazard): reject an empty/whitespace optional string so Some("") is never built.
        for (field, val) in [
            ("get_leg", &get_leg),
            ("put_leg", &put_leg),
            ("according_to", &according_to),
        ] {
            if let Some(s) = val
                && s.trim().is_empty()
            {
                return Err(Diag::of_kind(crate::error::Ir {
                    detail: format!(
                        "Correspondence.{field} must be a non-empty IRI string when present; \
                         pass None to leave it unset"
                    ),
                }));
            }
        }
        // The unit-interval axes must be a finite value in [0, 1]; `weight` is a finite
        // ranking, not range-bound.  NaN/infinite would break content-key determinism.
        for (field, val) in [
            ("confidence", confidence),
            ("evidence_strength", evidence_strength),
            ("probability", probability),
        ] {
            if let Some(x) = val
                && !(0.0..=1.0).contains(&x)
            {
                return Err(Diag::of_kind(crate::error::Ir {
                    detail: format!("Correspondence.{field} must be in [0, 1], got {x}"),
                }));
            }
        }
        if let Some(w) = weight
            && !w.is_finite()
        {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: format!("Correspondence.weight must be finite, got {w}"),
            }));
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
            preservation,
            source_endpoint: None,
            target_endpoint: None,
            grounding: false,
            recovery_cases: Vec::new(),
        })
    }

    /// Attach the two term/pattern endpoints of this correspondence.
    ///
    /// Endpoints are all-or-nothing: a one-sided term correspondence is not a meaningful
    /// bridge and would make the shipped correspondence graph impossible to traverse.
    /// Empty endpoint IRIs are rejected for the same content-identity reason as empty leg
    /// IRIs in [`Self::new`].
    pub fn with_endpoints(
        mut self,
        source_endpoint: impl Into<String>,
        target_endpoint: impl Into<String>,
    ) -> gmeow_errors::Result<Self> {
        let source_endpoint = source_endpoint.into();
        let target_endpoint = target_endpoint.into();
        if source_endpoint.trim().is_empty() || target_endpoint.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Correspondence endpoints must both be non-empty IRI strings".to_owned(),
            }));
        }
        self.source_endpoint = Some(source_endpoint);
        self.target_endpoint = Some(target_endpoint);
        Ok(self)
    }

    /// Mark this node as a co-foundational grounding correspondence.
    pub fn as_grounding(mut self) -> Self {
        self.grounding = true;
        self
    }

    /// Attach the executable recovery cases for this correspondence.
    ///
    /// Case identity is authored and therefore must be unique.  Two nodes with the same IRI
    /// are ambiguous even when their formulas happen to match, so duplicates hard-fail rather
    /// than falling through an order-dependent first-wins path.
    pub fn with_recovery_cases(
        mut self,
        mut recovery_cases: Vec<RecoveryCaseIr>,
    ) -> gmeow_errors::Result<Self> {
        recovery_cases.sort_by(|a, b| a.iri.cmp(&b.iri));
        if let Some(duplicate) = recovery_cases
            .windows(2)
            .find(|pair| pair[0].iri == pair[1].iri)
            .map(|pair| pair[0].iri.clone())
        {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: format!(
                    "Correspondence recovery-case IRI <{duplicate}> is duplicated; case identity must be unique"
                ),
            }));
        }
        self.recovery_cases = recovery_cases;
        Ok(self)
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
        let endpoints = match (&self.source_endpoint, &self.target_endpoint) {
            (Some(source), Some(target)) => format!("{SEP}source={source}{SEP}target={target}"),
            (None, None) => String::new(),
            _ => unreachable!("Correspondence endpoints are constructed all-or-nothing"),
        };
        let grounding = self
            .grounding
            .then_some(format!("{SEP}grounding=True"))
            .unwrap_or_default();
        let recovery = if self.recovery_cases.is_empty() {
            String::new()
        } else {
            format!(
                "{SEP}recovery={}",
                self.recovery_cases
                    .iter()
                    .map(RecoveryCaseIr::content_key)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        format!(
            "{}{SEP}rel={}{SEP}class={}{SEP}kind={}{SEP}mnemo={}{SEP}det={}{SEP}\
             get={}{SEP}put={}{SEP}conf={}{SEP}ev={}{SEP}w={}{SEP}prob={}{SEP}\
             at={}{SEP}pres={}{SEP}laws={claims}{endpoints}{grounding}{recovery}",
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
            self.preservation.map(|p| p.as_str()).unwrap_or(""),
        )
    }
}

/// Hard-fail if the same `logic:RecoveryCase` IRI is declared by two DIFFERENT
/// correspondences in `correspondences`. Called from [`LogicProgram::with_correspondences`]
/// — the one place a program's whole correspondence set is assembled and therefore the
/// only place a cross-correspondence collision can be seen at all.
///
/// [`Correspondence::with_recovery_cases`] already rejects a duplicate IRI WITHIN one
/// correspondence at construction, so any duplicate this function finds is necessarily
/// owned by two distinct correspondences; not deduping (silently keeping the first) would
/// hide a real authoring error, since the RDF subject would then alias two intended
/// `logic:recoveryTransform` meanings.
fn assert_unique_recovery_case_iris(
    correspondences: &[Correspondence],
) -> gmeow_errors::Result<()> {
    let mut cases: Vec<(&str, &str)> = correspondences
        .iter()
        .flat_map(|c| {
            c.recovery_cases
                .iter()
                .map(move |case| (case.iri.as_str(), c.iri.as_str()))
        })
        .collect();
    cases.sort_by(|a, b| a.0.cmp(b.0));
    if let Some(pair) = cases.windows(2).find(|w| w[0].0 == w[1].0) {
        let (case_iri, first_owner) = pair[0];
        let (_, second_owner) = pair[1];
        return Err(Diag::of_kind(crate::error::Ir {
            detail: format!(
                "logic:RecoveryCase IRI <{case_iri}> is declared by two different \
                 correspondences (<{first_owner}> and <{second_owner}>); recovery-case \
                 identity must be unique across the whole program, not merely within one \
                 correspondence"
            ),
        }));
    }
    Ok(())
}

// --------------------------------------------------------------------------- //
// Full first-order formula AST (the typed full-FOL core)
// --------------------------------------------------------------------------- //

/// A first-order **term**: the leaf of the [`Formula`] AST.
///
/// Closed by construction — there is **no predicate-variable variant**, which is
/// precisely what keeps the object level first-order (`design/LOGIC-IR.md` §"What the
/// IR is"): a relation or type quantified over is reified to its HiLog individual
/// (`logic:instanceOf` / `logic:orderedType` / `logic:Type`) and appears here as
/// [`Term::Iri`], never as a higher-typed slot.  Variables carry their **authored**
/// name (no `?` sigil — that is a surface convention); the canonical key replaces the
/// name with a binder-relative token so alpha-equivalent formulas share identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Term {
    /// A bound or free variable (authored name, no `?` sigil).
    Var(String),
    /// An IRI constant — an individual, or a reified relation/type (HiLog).
    Iri(String),
    /// A data literal: lexical form plus an optional datatype IRI (`None` = a plain
    /// literal). An empty `lexical` is a legal RDF literal; a `Some("")` datatype is not.
    Literal {
        /// The literal's lexical form.
        lexical: String,
        /// The datatype IRI, or `None` for a plain literal.
        datatype: Option<String>,
    },
    /// A sequence marker (Common Logic `...x`): a variadic placeholder that binds a
    /// **sequence** of terms, not a single term. A distinct variant so the AST cannot
    /// confuse a single-term variable with a sequence one.
    SequenceMarker(String),
    /// A compound **function-term application** `f(t₀, …, tₙ)` — a function symbol applied
    /// to one or more argument terms (`cons(H, T)`, `s(X)`). `symbol` is the IRI of the
    /// reified function symbol (a `logic:Type` individual, mirroring how [`Formula::Atom`]
    /// reifies its relation), so the object level stays first-order: there is no
    /// function-variable slot. `args` is a non-empty, ordered list of sub-terms and MAY
    /// itself contain a nested [`Term::App`], so a nested term like `cons(H, cons(1, nil))`
    /// round-trips. A *nullary* application is not admitted — a 0-ary function symbol is a
    /// constant and is spelled [`Term::Iri`], so admitting an empty-`args` application would
    /// give one constant two canonical identities.
    App {
        /// The reified function symbol's IRI (a `logic:Type` individual, never a variable).
        symbol: String,
        /// The ordered, non-empty argument terms (each may itself be an [`Term::App`]).
        args: Vec<Term>,
    },
}

impl Term {
    /// A variable term, rejecting an empty/whitespace-only name (a blank name collides
    /// with absence and breaks the alpha-normalized key's determinism).
    pub fn var(name: impl Into<String>) -> gmeow_errors::Result<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Term::Var name must be non-empty".to_owned(),
            }));
        }
        Ok(Self::Var(name))
    }

    /// An IRI term, rejecting an empty/whitespace-only IRI.
    pub fn iri(iri: impl Into<String>) -> gmeow_errors::Result<Self> {
        let iri = iri.into();
        if iri.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Term::Iri must be a non-empty IRI string".to_owned(),
            }));
        }
        Ok(Self::Iri(iri))
    }

    /// A literal term. The lexical form may be empty (a legal RDF literal); a present
    /// datatype must be a non-empty IRI (`Some("")` would collide with `None`).
    pub fn literal(
        lexical: impl Into<String>,
        datatype: Option<String>,
    ) -> gmeow_errors::Result<Self> {
        if let Some(dt) = &datatype
            && dt.trim().is_empty()
        {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Term::Literal datatype must be a non-empty IRI when present; pass None"
                    .to_owned(),
            }));
        }
        Ok(Self::Literal {
            lexical: lexical.into(),
            datatype,
        })
    }

    /// A sequence-marker term, rejecting an empty/whitespace-only name.
    pub fn sequence_marker(name: impl Into<String>) -> gmeow_errors::Result<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Term::SequenceMarker name must be non-empty".to_owned(),
            }));
        }
        Ok(Self::SequenceMarker(name))
    }

    /// A compound function-term application, rejecting an empty/whitespace-only symbol IRI
    /// and a nullary argument list. A 0-ary application collides with a bare [`Self::Iri`]
    /// constant, so arity ≥ 1 is enforced to keep one constant from acquiring two canonical
    /// identities; an empty symbol collides with absence.
    pub fn app(symbol: impl Into<String>, args: Vec<Term>) -> gmeow_errors::Result<Self> {
        let symbol = symbol.into();
        if symbol.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Term::App symbol must be a non-empty function-symbol IRI".to_owned(),
            }));
        }
        if args.is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Term::App requires at least one argument; a nullary application is a \
                 constant and must be a Term::Iri"
                    .to_owned(),
            }));
        }
        Ok(Self::App { symbol, args })
    }

    /// `true` for a marker that binds a sequence rather than a single term.
    fn is_sequence_marker(&self) -> bool {
        matches!(self, Self::SequenceMarker(_))
    }

    /// `true` for a compound function-term application (`f(t₀, …, tₙ)`). Such a term exceeds
    /// the function-free Horn/Datalog fragment, so an atom carrying one is not a trivial
    /// triple (see [`Formula::is_trivially_horn`]).
    fn is_application(&self) -> bool {
        matches!(self, Self::App { .. })
    }

    /// The canonical key fragment for this term under a binding environment `env`
    /// (innermost binder last). A `Var`/`SequenceMarker` whose name is bound resolves
    /// to its binder-relative token; a free one resolves to a stable `free_<name>`.
    /// The leading tag letter keeps the four term kinds from ever colliding (so a
    /// variable and a sequence marker of the same name are distinct).
    fn key_in(&self, env: &[(String, String)]) -> String {
        match self {
            Self::Var(n) => format!("V{SEP}{}", resolve_binding(env, n)),
            Self::Iri(i) => format!("I{SEP}{i}"),
            Self::Literal { lexical, datatype } => {
                format!("L{SEP}{lexical}{SEP}{}", datatype.as_deref().unwrap_or(""))
            }
            Self::SequenceMarker(n) => format!("S{SEP}{}", resolve_binding(env, n)),
            // A function-term application keys as its symbol plus its arity plus each
            // argument's env-aware key (in order): the arity prefix keeps `f(a, b)` from
            // ever colliding with a differently-nested term, and the recursive `key_in`
            // makes a bound variable inside an argument alpha-normalize like anywhere else.
            Self::App { symbol, args } => {
                let mut inner = String::new();
                for a in args {
                    inner.push(SEP);
                    inner.push_str(&a.key_in(env));
                }
                format!("A{SEP}{symbol}{SEP}{}{inner}", args.len())
            }
        }
    }
}

/// Resolve a variable/marker name against the binding environment (innermost first).
/// Bound names map to their binder-relative token; a free name maps to `free_<name>`
/// (free variables are part of meaning and are never renamed).
fn resolve_binding(env: &[(String, String)], name: &str) -> String {
    for (authored, token) in env.iter().rev() {
        if authored == name {
            return token.clone();
        }
    }
    format!("free_{name}")
}

/// A first-order **formula** — the full-FOL core the IR's spec promises
/// (`design/LOGIC-IR.md`). Horn+NAF (today's [`LogicAxiom`] / [`LogicRule`]) is a
/// recognized **sub-fragment** carried unchanged in [`LogicProgram::axioms`] /
/// [`LogicProgram::rules`]; this enum carries the formulas that exceed that fragment.
///
/// Canonical identity is computed by [`Formula::content_key`], which performs a single
/// env-aware normalizing walk: bound variables are alpha-renamed to binder-relative
/// tokens (so equal-up-to-renaming formulas share a key), commutative connectives
/// (`And`/`Or`/`Iff`) are flattened and order-normalized, and `Implies` order is kept.
/// The stored structure is intentionally **not** mutated at construction — a
/// sub-formula's canonical operand order can depend on an outer binder not yet known
/// when it is built, so normalization belongs in the key, not the data.
///
/// Identity always flows through [`Formula::content_key`], never through derived
/// `PartialEq` (which is structural, not alpha/order-aware) — exactly as the rest of the
/// IR uses `sort_key`/`content_key` as the authority. A binary [`Formula::Atom`] *is* a
/// Horn triple, so there is no separate triple leaf: the Horn sub-fragment lives in
/// [`LogicProgram::axioms`] / [`LogicProgram::rules`], and a trivially-Horn atom may not
/// enter [`LogicProgram::formulas`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Formula {
    /// An atomic predication `relation(arg₀, …, argₙ)`. `relation` is a [`Term::Iri`]
    /// (the reified relation / HiLog individual — the constructor rejects a
    /// `Var`/`Literal`/`SequenceMarker` in relation position, enforcing first-orderness);
    /// `args` is variadic and MAY contain a [`Term::SequenceMarker`], which is how the
    /// fixed arity-3 atom is generalized.
    Atom {
        /// The reified relation (always an [`Term::Iri`]).
        relation: Term,
        /// The (variadic) argument terms.
        args: Vec<Term>,
    },
    /// Strong / explicit negation `¬φ` — **distinct** from negation-as-failure
    /// ([`LogicAxiom::negated`]); the two are never conflated.
    Not(Box<Formula>),
    /// Conjunction `φ₁ ∧ … ∧ φₙ` (variadic, n ≥ 2). Commutative + associative
    /// (flattened and order-normalized in the key).
    And(Vec<Formula>),
    /// Disjunction `φ₁ ∨ … ∨ φₙ` (variadic, n ≥ 2). Commutative + associative.
    Or(Vec<Formula>),
    /// Material implication `φ → ψ` (ordered, non-commutative).
    Implies(Box<Formula>, Box<Formula>),
    /// Biconditional `φ ↔ ψ` (commutative — the pair is order-normalized in the key).
    Iff(Box<Formula>, Box<Formula>),
    /// Universal quantification `∀ vars . φ`. Multi-variable block order is significant
    /// (alpha-equivalence is renaming, not prefix permutation); nested binders normalize
    /// via binder depth.
    Forall {
        /// The bound variable names (authored).
        vars: Vec<String>,
        /// The quantified body.
        body: Box<Formula>,
    },
    /// Existential quantification `∃ vars . φ`. Skolem/witness identity is scoped at the
    /// evaluable-lowering layer; the AST only pins the binder and body.
    Exists {
        /// The bound variable names (authored).
        vars: Vec<String>,
        /// The quantified body.
        body: Box<Formula>,
    },
}

/// The closed set of first-order shape tags a Horn-fragment projection target discloses
/// when it cannot carry a `logic:Formula`. Each tag names a construct that pushes the
/// formula beyond the binary Horn+NAF fragment, so the loss ledger states *which* construct
/// was carried-and-flagged rather than emitting one opaque free-text note (free text makes
/// the goldens fragile). The string form ([`FormulaShape::as_str`]) is the byte-stable
/// ledger token; it mirrors the `logic:FormulaShape` individuals in `module.ttl`
/// (`formula_shape_values_match_module_ttl` pins the two in sync). Variants are declared in
/// `as_str`-lexical order so the derived `Ord` is the canonical ledger order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FormulaShape {
    /// A disjunction `∨` appears — a choice the determinate Horn body cannot express.
    Disjunctive,
    /// The formula is a connective/quantifier tree, not a flat predication.
    Nested,
    /// A quantifier `∀`/`∃` appears (nested or alternating beyond the implicit universal
    /// closure of a Horn rule).
    Quantified,
    /// Strong/explicit negation `¬` appears — distinct from negation-as-failure.
    StrongNegation,
    /// A genuinely unbounded predication: an atom carrying a sequence marker
    /// (Common Logic `...x`), whose argument list has no fixed arity. A *fixed*-arity
    /// n-ary atom (unary or n ≥ 3) is NOT this shape — it is evaluable via reification
    /// into a conjunction of binary atoms over a reifier node.
    Variadic,
}

impl FormulaShape {
    /// Every shape tag, in canonical (`as_str`-lexical) order.
    pub const ALL: [FormulaShape; 5] = [
        FormulaShape::Disjunctive,
        FormulaShape::Nested,
        FormulaShape::Quantified,
        FormulaShape::StrongNegation,
        FormulaShape::Variadic,
    ];

    /// The byte-stable ledger token, equal to the local name of the mirroring
    /// `logic:FormulaShape` individual in `module.ttl`.
    pub fn as_str(self) -> &'static str {
        match self {
            FormulaShape::Disjunctive => "Disjunctive",
            FormulaShape::Nested => "Nested",
            FormulaShape::Quantified => "Quantified",
            FormulaShape::StrongNegation => "StrongNegation",
            FormulaShape::Variadic => "Variadic",
        }
    }

    /// Parse a shape tag from its `module.ttl` local name; `None` if unrecognized.
    pub fn from_local(s: &str) -> Option<Self> {
        FormulaShape::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

impl Formula {
    /// An atomic predication, enforcing first-orderness: `relation` MUST be a
    /// [`Term::Iri`] (a reified relation / HiLog individual), never a variable, literal,
    /// or sequence marker.
    pub fn atom(relation: Term, args: Vec<Term>) -> gmeow_errors::Result<Self> {
        if !matches!(relation, Term::Iri(_)) {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "Formula::Atom relation must be a Term::Iri (the reified relation / HiLog \
                 individual); a predicate variable would break first-orderness"
                    .to_owned(),
            }));
        }
        Ok(Self::Atom { relation, args })
    }

    /// `true` when this formula is a *trivially-Horn* leaf that belongs in
    /// [`LogicProgram::axioms`], not [`LogicProgram::formulas`]: a [`Formula::Atom`] that
    /// is exactly a binary predication (an IRI relation with two *flat* args — neither a
    /// sequence marker nor a compound function-term application) — i.e. an ordinary triple.
    /// A function term exceeds the function-free Datalog fragment, so an atom carrying one is
    /// a genuine formula, not a triple. Such a node has a Horn home and must not enter the
    /// formula collection, where it would give one fact two distinct content keys.
    pub(crate) fn is_trivially_horn(&self) -> bool {
        match self {
            Self::Atom { relation, args } => {
                matches!(relation, Term::Iri(_))
                    && args.len() == 2
                    && !args
                        .iter()
                        .any(|a| a.is_sequence_marker() || a.is_application())
            }
            _ => false,
        }
    }

    /// Convert a *trivially-Horn* binary predication ([`Self::is_trivially_horn`]) into its
    /// proper [`LogicAxiom`] home — the sound resolution when a reified `logic:Formula` node
    /// turns out to be an ordinary triple (`relation` + two arguments). Returns `None` for any
    /// non-trivially-Horn formula (a connective, quantifier, negation, or fixed-arity n-ary
    /// atom) — those keep their formula identity — AND for a degenerate binary atom whose
    /// subject cannot be a triple subject (a literal or sequence marker in argument position 0),
    /// which is malformed rather than a fact.
    ///
    /// This is what lets the front-end enforce the [`LogicProgram::with_formulas`] invariant by
    /// ROUTING rather than by assumption: a trivially-Horn leaf is redirected to
    /// [`LogicProgram::axioms`] (where a fact belongs) instead of tripping the assertion. A
    /// variable argument is preserved with the `?name` sigil the axiom string encoding uses, so
    /// a reified binary atom with variables becomes a rule-shaped axiom, and a fully ground one
    /// becomes an EDB fact.
    pub fn as_horn_axiom(&self) -> Option<LogicAxiom> {
        let Self::Atom { relation, args } = self else {
            return None;
        };
        if !self.is_trivially_horn() {
            return None;
        }
        let Term::Iri(predicate) = relation else {
            return None;
        };
        // A triple subject is an IRI or a variable — never a literal, a sequence marker, or a
        // compound function term (`is_trivially_horn` already excludes an application-bearing
        // atom, so the `App` arm is a defensive, never-taken guard, not a live path).
        let subject = match &args[0] {
            Term::Iri(iri) => iri.clone(),
            Term::Var(name) => format!("?{name}"),
            Term::Literal { .. } | Term::SequenceMarker(_) | Term::App { .. } => return None,
        };
        let (obj, obj_is_literal) = match &args[1] {
            Term::Iri(iri) => (iri.clone(), false),
            Term::Var(name) => (format!("?{name}"), false),
            Term::Literal { lexical, .. } => (lexical.clone(), true),
            Term::SequenceMarker(_) | Term::App { .. } => return None,
        };
        LogicAxiom::ground(subject, predicate.clone(), obj, obj_is_literal).ok()
    }

    /// The closed [`FormulaShape`] tags this formula exhibits, ordered and deduped — the
    /// residue classification a Horn-fragment target discloses, never free text. A
    /// connective/quantifier is at least `Nested`, and a genuinely unbounded atom is
    /// `Variadic`. A bare *fixed-arity* predication (unary or n ≥ 3) is evaluable via
    /// reification and carries no shape tag: when such an atom nonetheless reaches
    /// residue (e.g. an ill-formed argument), the residue is named by its reason string,
    /// not by a shape tag.
    pub fn shape_tags(&self) -> Vec<FormulaShape> {
        let mut set = std::collections::BTreeSet::new();
        self.collect_shapes(&mut set);
        set.into_iter().collect()
    }

    fn collect_shapes(&self, out: &mut std::collections::BTreeSet<FormulaShape>) {
        match self {
            Self::Atom { args, .. } => {
                // `Variadic` now denotes only a *genuinely unbounded* atom — one carrying
                // a sequence marker. A fixed-arity n-ary atom (unary or arity ≥ 3) is
                // evaluable via reification (it lowers to a conjunction of binary atoms
                // over a reifier node) and so is NOT a residue shape.
                if args.iter().any(Term::is_sequence_marker) {
                    out.insert(FormulaShape::Variadic);
                }
            }
            Self::Not(b) => {
                out.insert(FormulaShape::StrongNegation);
                out.insert(FormulaShape::Nested);
                b.collect_shapes(out);
            }
            Self::And(fs) => {
                out.insert(FormulaShape::Nested);
                fs.iter().for_each(|f| f.collect_shapes(out));
            }
            Self::Or(fs) => {
                out.insert(FormulaShape::Disjunctive);
                out.insert(FormulaShape::Nested);
                fs.iter().for_each(|f| f.collect_shapes(out));
            }
            Self::Implies(a, b) | Self::Iff(a, b) => {
                out.insert(FormulaShape::Nested);
                a.collect_shapes(out);
                b.collect_shapes(out);
            }
            Self::Forall { body, .. } | Self::Exists { body, .. } => {
                out.insert(FormulaShape::Quantified);
                out.insert(FormulaShape::Nested);
                body.collect_shapes(out);
            }
        }
    }

    /// Alpha- and order-normalized content key — a pure function of the formula's
    /// *meaning* up to bound-variable renaming and commutative reordering. Two formulas
    /// equal up to those share this key; everything else (including free-variable
    /// renaming and `Implies` operand order) is preserved.
    pub fn content_key(&self) -> String {
        let mut env: Vec<(String, String)> = Vec::new();
        self.key_in(&mut env, 0)
    }

    /// Canonical sort key for ordering the [`LogicProgram::formulas`] collection. A
    /// formula has no separate identity field, so the full content key *is* the sort key
    /// (order-independent and alpha-stable).
    pub fn sort_key(&self) -> String {
        self.content_key()
    }

    /// The single predicate IRI this formula's CONCLUSION is *about*, when it has exactly one
    /// — the relation of the atomic head reached by peeling strong negation ([`Self::Not`]),
    /// quantifier prefixes ([`Self::Forall`] / [`Self::Exists`]), and the antecedent of an
    /// implication ([`Self::Implies`], whose consequent is the drawn head) off a
    /// [`Self::Atom`]. Returns `None` for a formula whose conclusion is genuinely compound (a
    /// conjunction, disjunction, or biconditional), which names several head predicates and so
    /// has no single principal one, and for the degenerate case of a non-IRI relation.
    ///
    /// This is the sound source for the `logic:obligationForbiddenPredicate` of an
    /// anti-conjecture `logic:NonEntailmentObligation` (the predicate the closure must never
    /// draw): a refuted universal/atomic claim about a predicate `p` forbids `p`; a refuted
    /// rule `∀…. body → head` forbids the head's predicate — exactly the shape of the charter's
    /// standing obligations (forbid the conclusion predicate a rule would draw). A refuted
    /// claim whose conclusion is genuinely compound names no single predicate, so its forbidden
    /// predicate is a reviewer decision, never engine-derivable — the caller hard-fails rather
    /// than fabricating one.
    pub fn principal_predicate(&self) -> Option<String> {
        match self {
            Self::Atom { relation, .. } => match relation {
                Term::Iri(iri) => Some(iri.clone()),
                _ => None,
            },
            Self::Not(body) | Self::Forall { body, .. } | Self::Exists { body, .. } => {
                body.principal_predicate()
            }
            // The rule head (consequent) is the conclusion an anti-conjecture obligation
            // forbids the closure from drawing; the antecedent is only the trigger.
            Self::Implies(_antecedent, consequent) => consequent.principal_predicate(),
            Self::And(_) | Self::Or(_) | Self::Iff(_, _) => None,
        }
    }

    /// The normalizing walk. `env` maps an authored bound name to its binder-relative
    /// token (innermost binder last); `depth` is the number of enclosing quantifier
    /// blocks (used to build de-Bruijn-style tokens `q{depth}_{i}`).
    fn key_in(&self, env: &mut Vec<(String, String)>, depth: usize) -> String {
        match self {
            Self::Atom { relation, args } => {
                let r = relation.key_in(env);
                let a = args
                    .iter()
                    .map(|t| t.key_in(env))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("ATOM{SEP}{r}{SEP}({a})")
            }
            Self::Not(f) => format!("NOT{SEP}{}", f.key_in(env, depth)),
            Self::And(fs) => commutative_key("AND", fs, env, depth),
            Self::Or(fs) => commutative_key("OR", fs, env, depth),
            Self::Implies(a, b) => {
                format!(
                    "IMPL{SEP}{}{SEP}{}",
                    a.key_in(env, depth),
                    b.key_in(env, depth)
                )
            }
            Self::Iff(a, b) => {
                let mut pair = [a.key_in(env, depth), b.key_in(env, depth)];
                pair.sort();
                format!("IFF{SEP}{}{SEP}{}", pair[0], pair[1])
            }
            Self::Forall { vars, body } => binder_key("ALL", vars, body, env, depth),
            Self::Exists { vars, body } => binder_key("EX", vars, body, env, depth),
        }
    }
}

/// Key a flattened, order-normalized commutative connective (`And`/`Or`). Same-tag
/// children are flattened (`And[And[a,b],c]` ≡ `And[a,b,c]`), each operand keyed at the
/// same depth (these connectives bind nothing), then sorted so operand order is immaterial.
fn commutative_key(
    tag: &str,
    fs: &[Formula],
    env: &mut Vec<(String, String)>,
    depth: usize,
) -> String {
    let mut operands: Vec<&Formula> = Vec::new();
    flatten_commutative(tag, fs, &mut operands);
    let mut keys = operands
        .iter()
        .map(|f| f.key_in(env, depth))
        .collect::<Vec<_>>();
    keys.sort();
    format!("{tag}{SEP}({})", keys.join(","))
}

/// Collect the operands of a commutative connective, flattening nested same-tag nodes.
fn flatten_commutative<'a>(tag: &str, fs: &'a [Formula], out: &mut Vec<&'a Formula>) {
    for f in fs {
        match (tag, f) {
            ("AND", Formula::And(inner)) => flatten_commutative(tag, inner, out),
            ("OR", Formula::Or(inner)) => flatten_commutative(tag, inner, out),
            _ => out.push(f),
        }
    }
}

/// Key a quantifier binder. Each bound variable gets a binder-relative token
/// `q{depth}_{i}` (block order significant); the body is keyed at `depth + 1` with the
/// new bindings pushed, then they are popped. The variable count is folded in so a
/// vacuous binder still alters identity.
fn binder_key(
    tag: &str,
    vars: &[String],
    body: &Formula,
    env: &mut Vec<(String, String)>,
    depth: usize,
) -> String {
    let base = env.len();
    for (i, v) in vars.iter().enumerate() {
        env.push((v.clone(), format!("q{depth}_{i}")));
    }
    let body_key = body.key_in(env, depth + 1);
    env.truncate(base);
    format!("{tag}{SEP}[{}]{SEP}{body_key}", vars.len())
}

// --------------------------------------------------------------------------- //
// Reasoning programs (`logic:ReasoningProgram`)
// --------------------------------------------------------------------------- //

/// The closed evaluation-strategy set a [`ReasoningProgramIr`] selects via
/// `logic:evaluationMode` — mirrors the `logic:EvaluationMode` individuals in
/// `module.ttl`. Deliberately has **no catch-all/unknown variant**: an unrecognized mode
/// IRI is unrepresentable in this type, so the front-end MUST reject it as a hard-fail
/// diagnostic at parse time rather than either defaulting silently or smuggling an opaque
/// string through as a would-be "mode" the rest of the compiler cannot dispatch on
/// (no-optionality: explicit feature selection is fine, silent degradation is not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationMode {
    /// `logic:BackwardEvaluation` — goal-directed SLG-WFS backward resolution: tabled
    /// (SLG) demand-driven answer keying, structured-term unification over
    /// [`Term::App`] arguments, explicit proof objects per derived answer, and
    /// three-valued well-founded negation for `logic:not`-negated body literals.
    Backward,
}

impl EvaluationMode {
    /// The `module.ttl` local name of the individual selecting this mode
    /// (`logic:BackwardEvaluation` ⇒ `"BackwardEvaluation"`).
    pub fn as_str(self) -> &'static str {
        match self {
            EvaluationMode::Backward => "BackwardEvaluation",
        }
    }

    /// Parse a `module.ttl` local name into a mode; `None` for anything outside the
    /// closed set. The caller turns `None` into a hard-fail diagnostic — never a silent
    /// default to [`EvaluationMode::Backward`].
    pub fn from_local(s: &str) -> Option<Self> {
        match s {
            "BackwardEvaluation" => Some(EvaluationMode::Backward),
            _ => None,
        }
    }
}

/// A compiled `logic:ReasoningProgram`: a named clause set plus a goal, evaluated under a
/// selected strategy. Reuses the SAME [`Formula`] type the forward relational-core lane
/// already lowers from for [`Self::clauses`] / [`Self::query`] / [`Self::verdict_probes`] —
/// there is no second, program-specific clause language. `logic:evaluationMode` is a
/// strategy SELECTOR over one strategy-neutral clause set, not a program-kind split, so a
/// future forward-chase [`EvaluationMode`] member evaluates the identical clauses without
/// re-authoring (see `module.ttl`'s `logic:ReasoningProgram` definition).
#[derive(Debug, Clone, PartialEq)]
pub struct ReasoningProgramIr {
    /// IRI of the `logic:ReasoningProgram` individual (identity / sort key).
    pub iri: String,
    /// The selected evaluation strategy (`logic:evaluationMode`; functional, exactly one).
    pub mode: EvaluationMode,
    /// The clause set (`logic:clause`): atomic facts and Horn rules (each the SAME
    /// [`Formula`] shape the forward relational-core lane consumes), in canonical
    /// (sort-key) order. Never empty — [`Self::new`] hard-fails a clause-free program
    /// (`logic:ReasoningProgramClauseConstraint`).
    pub clauses: Vec<Formula>,
    /// The single goal atom (`logic:programQuery`) the engine resolves against
    /// [`Self::clauses`]. Functional at the vocabulary level
    /// (`logic:ReasoningProgramQueryConstraint` + the `owl:FunctionalProperty`
    /// declaration), so a program carries EXACTLY one.
    pub query: Formula,
    /// Zero or more three-valued verdict probes (`logic:verdictProbe`), in canonical
    /// (sort-key) order. Each MUST be an atomic [`Formula::Atom`] — a probe reports a
    /// single atom's well-founded verdict, never a compound formula's.
    pub verdict_probes: Vec<Formula>,
    /// Per-variable order-sort declarations (`logic:variableSort` on a `logic:TermCarrier`
    /// reached from [`Self::clauses`] or [`Self::query`]): `(variable name, sort IRI)`
    /// pairs, deduplicated and sorted for determinism — the seed for the order-sorted
    /// unification context a backward-resolution engine builds. A variable with no
    /// authored sort simply has no entry (sort-checking is opt-in per variable, not a
    /// closed-world requirement).
    pub variable_sorts: Vec<(String, String)>,
}

impl ReasoningProgramIr {
    /// Construct a reasoning program, canonicalizing [`Self::clauses`],
    /// [`Self::verdict_probes`], and [`Self::variable_sorts`] into sorted order.
    ///
    /// **Hard-fails** (mirrors [`LogicProgram::with_formulas`]'s trivially-Horn guard and
    /// `module.ttl`'s `logic:ReasoningProgramClauseConstraint` /
    /// `logic:ReasoningProgramQueryConstraint`) on:
    /// * an empty IRI or `clauses` (a program with nothing to resolve its goal against);
    /// * a `verdict_probes` entry that is not an atomic [`Formula::Atom`] (a probe reports
    ///   one atom's three-valued verdict, never a compound formula's);
    /// * the same variable name paired with two DIFFERENT sort IRIs in `variable_sorts`
    ///   (an ambiguous order-sort context the unifier cannot seed deterministically).
    pub fn new(
        iri: impl Into<String>,
        mode: EvaluationMode,
        clauses: Vec<Formula>,
        query: Formula,
        verdict_probes: Vec<Formula>,
        variable_sorts: Vec<(String, String)>,
    ) -> gmeow_errors::Result<Self> {
        let iri = iri.into();
        if iri.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: "ReasoningProgramIr.iri must be a non-empty IRI string".to_owned(),
            }));
        }
        if clauses.is_empty() {
            return Err(Diag::of_kind(crate::error::Ir {
                detail: format!(
                    "ReasoningProgramIr {iri} requires at least one logic:clause; a program \
                     with no clauses has nothing to resolve its goal against"
                ),
            }));
        }
        for probe in &verdict_probes {
            if !matches!(probe, Formula::Atom { .. }) {
                return Err(Diag::of_kind(crate::error::Ir {
                    detail: format!(
                        "ReasoningProgramIr {iri} logic:verdictProbe must be an atomic \
                         logic:Formula (a single predication); found a compound formula"
                    ),
                }));
            }
        }
        let mut clauses = clauses;
        clauses.sort_by_cached_key(Formula::sort_key);
        let mut verdict_probes = verdict_probes;
        verdict_probes.sort_by_cached_key(Formula::sort_key);
        let mut variable_sorts = variable_sorts;
        variable_sorts.sort();
        variable_sorts.dedup();
        for pair in variable_sorts.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(Diag::of_kind(crate::error::Ir {
                    detail: format!(
                        "ReasoningProgramIr {iri} logic:variableSort assigns variable {:?} two \
                         distinct sorts ({:?} and {:?}); an order-sort context must be \
                         unambiguous",
                        pair[0].0, pair[0].1, pair[1].1
                    ),
                }));
            }
        }
        Ok(Self {
            iri,
            mode,
            clauses,
            query,
            verdict_probes,
            variable_sorts,
        })
    }

    /// The content key: the IRI bound to the mode, the clause/query/probe content keys
    /// (each clause/probe already canonically sorted), and the variable-sort pairs. Two
    /// reasoning programs are the same iff they share this key.
    pub fn content_key(&self) -> String {
        let clauses = self
            .clauses
            .iter()
            .map(Formula::content_key)
            .collect::<Vec<_>>()
            .join(",");
        let probes = self
            .verdict_probes
            .iter()
            .map(Formula::content_key)
            .collect::<Vec<_>>()
            .join(",");
        let sorts = self
            .variable_sorts
            .iter()
            .map(|(v, s)| format!("{v}{SEP}{s}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{}{SEP}{}{SEP}[{clauses}]{SEP}{}{SEP}[{probes}]{SEP}[{sorts}]",
            self.iri,
            self.mode.as_str(),
            self.query.content_key(),
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
    /// Leg-program bodies (`logic:TransactionProgram`) a correspondence's `logic:getLeg` /
    /// `logic:putLeg` IRI resolves to, in canonical (IRI) order. Attached via
    /// [`LogicProgram::with_transaction_programs`]; empty for the historical leg-body-free
    /// corpus, so the canonical key is unchanged there.
    pub transaction_programs: Vec<TransactionProgramIr>,
    /// Full first-order [`Formula`] nodes that exceed the Horn+NAF sub-fragment, in
    /// canonical (sort-key) order. Attached via [`LogicProgram::with_formulas`]; empty
    /// for the historical Horn-only corpus, so the canonical key is byte-unchanged there.
    /// Horn+NAF stays in [`Self::axioms`] / [`Self::rules`] — a trivially-Horn leaf may
    /// not enter here.
    pub formulas: Vec<Formula>,
    /// Closed-world validation shapes (`logic:ValidationShape`) in canonical (IRI) order —
    /// the canonical form the SHACL Core / ShEx surfaces project from. Attached via
    /// [`LogicProgram::with_validation_shapes`]; empty for the historical shape-free corpus,
    /// so the canonical key is byte-unchanged there.
    pub validation_shapes: Vec<ValidationShapeIr>,
    /// Closed-world procedural constraints (`logic:Constraint`) in canonical (IRI) order —
    /// the typed home for integrity conditions whose violation is a finding, the canonical
    /// form the `sh:SPARQLConstraint` surface projects from. Attached via
    /// [`LogicProgram::with_constraints`]; empty for the historical constraint-free corpus,
    /// so the canonical key is byte-unchanged there.
    pub constraints: Vec<ConstraintIr>,
    /// Compiled `logic:ReasoningProgram` nodes in canonical (IRI) order — the authored
    /// clause-set-plus-goal surface that drives the native reasoning engine directly from
    /// slice content. Attached via [`LogicProgram::with_reasoning_programs`]; empty for the
    /// historical reasoning-program-free corpus, so the canonical key is byte-unchanged
    /// there.
    pub reasoning_programs: Vec<ReasoningProgramIr>,
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
            transaction_programs: Vec::new(),
            formulas: Vec::new(),
            validation_shapes: Vec::new(),
            constraints: Vec::new(),
            reasoning_programs: Vec::new(),
            source_iri,
        }
    }

    /// Attach the leg-program registry (`logic:TransactionProgram` bodies the get/put leg
    /// IRIs resolve to), canonicalizing into IRI order. Append-only: the byte-pinned
    /// canonical key of a leg-body-free program is unchanged.
    pub fn with_transaction_programs(
        mut self,
        transaction_programs: Vec<TransactionProgramIr>,
    ) -> Self {
        let mut transaction_programs = transaction_programs;
        transaction_programs.sort_by(|a, b| a.iri.cmp(&b.iri));
        self.transaction_programs = transaction_programs;
        self
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
    ///
    /// `logic:RecoveryCase` IRIs are global RDF subjects, so their uniqueness must hold
    /// across the WHOLE program, not merely within one correspondence:
    /// [`Correspondence::with_recovery_cases`] only ever sees its own owning
    /// correspondence's case list, so a case IRI reused by a SECOND correspondence would
    /// alias two distinct `logic:recoveryTransform` definitions onto one RDF subject — a
    /// non-injective projection. This is the one place every correspondence in the
    /// program is visible together, so the cross-correspondence collision is hard-failed
    /// here rather than silently accepted.
    pub fn with_correspondences(
        mut self,
        correspondences: Vec<Correspondence>,
    ) -> gmeow_errors::Result<Self> {
        let mut correspondences = correspondences;
        correspondences.sort_by(|a, b| a.iri.cmp(&b.iri));
        assert_unique_recovery_case_iris(&correspondences)?;
        self.correspondences = correspondences;
        Ok(self)
    }

    /// Attach the program's full-FOL [`Formula`] nodes, canonicalizing them into sorted
    /// order. Kept separate from [`Self::new`] so existing call sites are untouched and
    /// the byte-pinned canonical key of a formula-free (Horn-only) program is unchanged
    /// (the formulas segment is append-only when present).
    ///
    /// **Hard-fails** if a top-level formula is *trivially Horn* (a bare triple or binary
    /// atom): such a fact has a home in [`Self::axioms`], and admitting it here would give
    /// one fact two distinct content-addressed identities (a content-addressing
    /// split-brain). The frontend never emits one; this guard makes the invariant a check.
    pub fn with_formulas(mut self, formulas: Vec<Formula>) -> Self {
        for f in &formulas {
            assert!(
                !f.is_trivially_horn(),
                "LogicProgram.formulas may not hold a trivially-Horn leaf (a bare triple or \
                 binary atom); it belongs in LogicProgram.axioms — key={}",
                f.content_key()
            );
        }
        let mut formulas = formulas;
        formulas.sort_by_cached_key(Formula::sort_key);
        self.formulas = formulas;
        self
    }

    /// Attach the program's `logic:ValidationShape` nodes, canonicalizing them into IRI
    /// order. Kept separate from [`Self::new`] so existing call sites are untouched and the
    /// byte-pinned canonical key of a shape-free program is unchanged (the validation-shapes
    /// segment is append-only at the fixed tail when present).
    pub fn with_validation_shapes(mut self, validation_shapes: Vec<ValidationShapeIr>) -> Self {
        let mut validation_shapes = validation_shapes;
        // Sort by IRI directly (no key clone). The IRI is the shape's identity, so two shapes
        // sharing one would make `canonical_key` depend on supply order — a hard invariant
        // violation, not a recoverable state, so reject it rather than silently keep both.
        validation_shapes.sort_by(|a, b| a.iri.cmp(&b.iri));
        assert!(
            validation_shapes.windows(2).all(|w| w[0].iri != w[1].iri),
            "LogicProgram.validation_shapes must not contain duplicate shape IRIs"
        );
        self.validation_shapes = validation_shapes;
        self
    }

    /// Attach the program's `logic:Constraint` nodes, canonicalizing them into IRI order.
    /// Kept separate from [`Self::new`] so existing call sites are untouched and the
    /// byte-pinned canonical key of a constraint-free program is unchanged (the constraints
    /// segment is append-only at the fixed tail when present). Mirrors
    /// [`Self::with_validation_shapes`]: the IRI is the constraint's identity, so two
    /// constraints sharing one would make `canonical_key` depend on supply order — a hard
    /// invariant violation, rejected rather than silently kept.
    pub fn with_constraints(mut self, constraints: Vec<ConstraintIr>) -> Self {
        let mut constraints = constraints;
        constraints.sort_by(|a, b| a.iri.cmp(&b.iri));
        assert!(
            constraints.windows(2).all(|w| w[0].iri != w[1].iri),
            "LogicProgram.constraints must not contain duplicate constraint IRIs"
        );
        self.constraints = constraints;
        self
    }

    /// Attach the program's `logic:ReasoningProgram` nodes, canonicalizing them into IRI
    /// order. Kept separate from [`Self::new`] so existing call sites are untouched and the
    /// byte-pinned canonical key of a reasoning-program-free program is unchanged (the
    /// reasoning-programs segment is append-only at the fixed tail when present). Mirrors
    /// [`Self::with_constraints`]: the IRI is the program's identity, so two reasoning
    /// programs sharing one would make `canonical_key` depend on supply order — a hard
    /// invariant violation, rejected rather than silently kept.
    pub fn with_reasoning_programs(mut self, reasoning_programs: Vec<ReasoningProgramIr>) -> Self {
        let mut reasoning_programs = reasoning_programs;
        reasoning_programs.sort_by(|a, b| a.iri.cmp(&b.iri));
        assert!(
            reasoning_programs.windows(2).all(|w| w[0].iri != w[1].iri),
            "LogicProgram.reasoning_programs must not contain duplicate program IRIs"
        );
        self.reasoning_programs = reasoning_programs;
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
        // Append-only at the FIXED position after CORRESPONDENCES (frozen once committed):
        // a formula-free (Horn-only) program keeps its exact historical key.
        if !self.formulas.is_empty() {
            let forms = self
                .formulas
                .iter()
                .map(Formula::content_key)
                .collect::<Vec<_>>()
                .join("\n");
            key.push_str("\nFORMULAS\n");
            key.push_str(&forms);
        }
        // Append-only at the FIXED tail: a leg-body-free program keeps its exact key.
        if !self.transaction_programs.is_empty() {
            let legs = self
                .transaction_programs
                .iter()
                .map(TransactionProgramIr::content_key)
                .collect::<Vec<_>>()
                .join("\n");
            key.push_str("\nTRANSACTIONPROGRAMS\n");
            key.push_str(&legs);
        }
        // Append-only at the FIXED tail: a validation-shape-free program keeps its exact key.
        if !self.validation_shapes.is_empty() {
            let shapes = self
                .validation_shapes
                .iter()
                .map(ValidationShapeIr::content_key)
                .collect::<Vec<_>>()
                .join("\n");
            key.push_str("\nVALIDATIONSHAPES\n");
            key.push_str(&shapes);
        }
        // Append-only at the FIXED tail: a constraint-free program keeps its exact key.
        if !self.constraints.is_empty() {
            let constraints = self
                .constraints
                .iter()
                .map(ConstraintIr::content_key)
                .collect::<Vec<_>>()
                .join("\n");
            key.push_str("\nCONSTRAINTS\n");
            key.push_str(&constraints);
        }
        // Append-only at the FIXED tail: a reasoning-program-free program keeps its exact key.
        if !self.reasoning_programs.is_empty() {
            let programs = self
                .reasoning_programs
                .iter()
                .map(ReasoningProgramIr::content_key)
                .collect::<Vec<_>>()
                .join("\n");
            key.push_str("\nREASONINGPROGRAMS\n");
            key.push_str(&programs);
        }
        key
    }
}

mod constraint;
mod validation;
pub use constraint::{AggregateComparator, AggregateComparison, AggregateRhs, ConstraintIr};
pub use validation::{
    ConstraintComponent, ConstraintProvenance, PropertyConstraintIr, ShaclNodeKind, ShaclSeverity,
    ShapeTarget, ShapeValue, ValidationShapeIr,
};

#[cfg(test)]
mod tests;
