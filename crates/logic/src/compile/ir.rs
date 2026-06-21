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
//! programs with the same axioms/rules/profiles constructed in a different order
//! compare equal and produce the same canonical key.  This is achieved by storing
//! all collection fields as **sorted vectors**, built by the canonicalizing
//! constructors ([`LogicProgram::new`], [`LogicRule::new`]).  Sorting is **stable**
//! and keyed on [`LogicAxiom::sort_key`] / [`LogicRule::sort_key`] /
//! [`LogicProfile::sort_key`], which reproduce the Python `_sort_key()` byte for
//! byte (null-byte separators; Python `bool` `Display` `True`/`False`;
//! corpus-safety: `negated` / `distinct` are appended to the key only when set, so
//! every pre-#502/#503 program keeps its exact historical key string and the
//! downstream artifacts stay byte-identical).

use std::fmt;

/// The null-byte field separator used by every `sort_key` (Python `"\x00"`).
const SEP: char = '\u{0}';

/// The `logic:` namespace; `iri()` helpers expand a local enum name to its IRI.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE` (and [`crate::provenance::LOGIC_NAMESPACE`]).
pub const LOGIC_NAMESPACE: &str = "https://blackcatinformatics.ca/logic/";

// --------------------------------------------------------------------------- //
// Enums — single source of truth, local names taken verbatim from module.ttl
// --------------------------------------------------------------------------- //

/// The six `logic:SemanticProfile` named individuals.
///
/// The string form ([`SemanticProfileId::as_str`]) is the local name (no
/// `logic:` prefix), taken verbatim from `slices/core/logic/module.ttl` — any
/// change there must be reflected here.
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

/// A declared semantic profile with its (optional) complexity class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicProfile {
    /// The named `logic:SemanticProfile` individual.
    pub profile_id: SemanticProfileId,
    /// The `logic:complexityClass` value, or `None` if not declared.
    pub complexity: Option<ComplexityClass>,
}

impl LogicProfile {
    /// Construct a profile.
    pub fn new(profile_id: SemanticProfileId, complexity: Option<ComplexityClass>) -> Self {
        Self {
            profile_id,
            complexity,
        }
    }

    /// Stable sort key — the golden-pinned key format.
    pub fn sort_key(&self) -> String {
        let compl = match &self.complexity {
            Some(c) => c.label(),
            None => "",
        };
        format!("{}{SEP}{compl}", self.profile_id.as_str())
    }

    /// A deterministic full-content key (equals the sort key for profiles).
    fn content_key(&self) -> String {
        self.sort_key()
    }
}

// --------------------------------------------------------------------------- //
// Top-level container
// --------------------------------------------------------------------------- //

/// Top-level container for a compiled `logic:` program.
///
/// Aggregates axioms, rules, and profiles; the unit of comparison for the
/// round-trip isomorphism gate.  Construct via [`LogicProgram::new`] so the
/// canonicalization contract (sorted collections) holds.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicProgram {
    /// Axioms in canonical order.
    pub axioms: Vec<LogicAxiom>,
    /// Rules in canonical order.
    pub rules: Vec<LogicRule>,
    /// Profiles in canonical order.
    pub profiles: Vec<LogicProfile>,
    /// IRI of the source graph/document (optional provenance).
    pub source_iri: Option<String>,
}

impl LogicProgram {
    /// Construct, canonicalizing all collection fields into sorted vectors with a
    /// **stable** sort (mirrors the Python `__post_init__`).
    pub fn new(
        axioms: Vec<LogicAxiom>,
        rules: Vec<LogicRule>,
        profiles: Vec<LogicProfile>,
        source_iri: Option<String>,
    ) -> Self {
        let mut axioms = axioms;
        axioms.sort_by_cached_key(LogicAxiom::sort_key);
        let mut rules = rules;
        rules.sort_by_cached_key(LogicRule::sort_key);
        let mut profiles = profiles;
        profiles.sort_by_cached_key(LogicProfile::sort_key);
        Self {
            axioms,
            rules,
            profiles,
            source_iri,
        }
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
        let profiles = self
            .profiles
            .iter()
            .map(LogicProfile::content_key)
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "AXIOMS\n{axioms}\nRULES\n{rules}\nPROFILES\n{profiles}\nSOURCE\n{}",
            self.source_iri.as_deref().unwrap_or(""),
        )
    }
}

#[cfg(test)]
mod tests;
