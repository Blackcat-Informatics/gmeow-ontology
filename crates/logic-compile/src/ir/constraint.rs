// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Procedural constraints — the closed-world, integrity-condition subset of the IR.
//!
//! A [`ConstraintIr`] is the IR realization of [`NodeKind::Constraint`]: a closed-world
//! *integrity condition* whose violation is a **finding**, not a derivation (contrast
//! [`super::LogicRule`], whose satisfaction *produces* a head). It reuses the realized
//! first-order [`Formula`] core verbatim — the integrity condition is an outer
//! range-restricted `∀`-guarded [`Formula::Forall`] whose body is the per-focus condition —
//! and reuses [`ShapeTarget`] / [`ShaclSeverity`] verbatim from the sibling
//! [`ValidationShapeIr`](super::ValidationShapeIr). It is NOT a new canonical construct and
//! NOT a parallel shape DSL: it is the typed home for the closed-world *procedural* checks
//! (choice groups, guarded requiredness, disjunctive requiredness, cross-node co-occurrence,
//! forbidden patterns, …) that later tasks project to `sh:SPARQLConstraint`.
//!
//! Identity is the content-addressed [`ConstraintIr::content_key`], folded over the
//! iri + target + integrity-formula key + severity. The advisory `message` is
//! **load-bearing-false** — it never enters the content key (two constraints differing only
//! in their message share an identity). The `formalizes` back-reference mirrors the
//! `logic:formalizes` *annotation* property (which carries "no DL or EL profile weight"), so
//! it is likewise annotation-level and excluded from the content key.

use gmeow_errors::Diag;

use super::validation::{ShaclSeverity, ShapeTarget};
use super::{Formula, SEP, Term};

/// Build an IR-grade [`Diag`] (the sole first-party error type — the Phase-6 Diag substrate)
/// for a malformed procedural constraint.
fn ir_err(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::Ir {
        detail: detail.into(),
    })
}

/// The `rdf:type` IRI — the relation of a class-membership guard atom `rdf:type(this, C)`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The `logic:directType(this, C)` guard marker deriving a subclass-excluding
/// [`ShapeTarget::DirectClass`] (mirrors the projector-side constant in `projections::shapes`).
const LOGIC_DIRECT_TYPE: &str = "https://blackcatinformatics.ca/logic/directType";
/// The `logic:sparqlTarget(this, "SELECT …")` guard marker deriving a raw [`ShapeTarget::Sparql`]
/// (mirrors the projector-side constant in `projections::shapes`).
const LOGIC_SPARQL_TARGET: &str = "https://blackcatinformatics.ca/logic/sparqlTarget";

/// The relational comparator of an [`AggregateComparison`] — the SPARQL `HAVING` operator the
/// aggregate value is tested against. Named the FOL way (equality / inequality / ordering), with
/// both the SPARQL rendering and its logical [`Self::negated`] (used to select the VIOLATING rows
/// of a `sh:SPARQLConstraint`, whose `sh:select` returns focus nodes that FAIL the invariant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateComparator {
    /// `=` — the aggregate equals the right-hand side.
    Eq,
    /// `!=` — the aggregate differs from the right-hand side.
    Ne,
    /// `<` — the aggregate is strictly below the right-hand side.
    Lt,
    /// `<=` — the aggregate is at most the right-hand side.
    Le,
    /// `>` — the aggregate is strictly above the right-hand side.
    Gt,
    /// `>=` — the aggregate is at least the right-hand side.
    Ge,
}

impl AggregateComparator {
    /// The SPARQL relational operator token.
    pub fn as_sparql(&self) -> &'static str {
        match self {
            AggregateComparator::Eq => "=",
            AggregateComparator::Ne => "!=",
            AggregateComparator::Lt => "<",
            AggregateComparator::Le => "<=",
            AggregateComparator::Gt => ">",
            AggregateComparator::Ge => ">=",
        }
    }

    /// The logical negation — the operator that selects the rows VIOLATING the authored invariant
    /// (`=` ↦ `!=`, `<` ↦ `>=`, …). The `sh:SPARQLConstraint` `sh:select` returns violations, so
    /// the projected `HAVING` uses the negated operator.
    pub fn negated(&self) -> AggregateComparator {
        match self {
            AggregateComparator::Eq => AggregateComparator::Ne,
            AggregateComparator::Ne => AggregateComparator::Eq,
            AggregateComparator::Lt => AggregateComparator::Ge,
            AggregateComparator::Le => AggregateComparator::Gt,
            AggregateComparator::Gt => AggregateComparator::Le,
            AggregateComparator::Ge => AggregateComparator::Lt,
        }
    }

    /// Parse an authored comparator symbol (ASCII or the Unicode `≠`/`≤`/`≥`), or `None`.
    pub fn from_symbol(s: &str) -> Option<AggregateComparator> {
        match s.trim() {
            "=" | "==" => Some(AggregateComparator::Eq),
            "!=" | "≠" | "<>" => Some(AggregateComparator::Ne),
            "<" => Some(AggregateComparator::Lt),
            "<=" | "≤" => Some(AggregateComparator::Le),
            ">" => Some(AggregateComparator::Gt),
            ">=" | "≥" => Some(AggregateComparator::Ge),
            _ => None,
        }
    }

    /// The byte-stable content-key token (the ASCII SPARQL operator).
    fn as_key(&self) -> &'static str {
        self.as_sparql()
    }
}

/// The right-hand side an [`AggregateComparison`] tests the aggregate against: a compared
/// property of the focus node, or a fixed literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateRhs {
    /// The value of this predicate on the focus (`$this <predicate> ?rhs`); the aggregate is
    /// compared to `?rhs`.
    Property(String),
    /// A fixed literal value (lexical form plus optional datatype IRI).
    Literal {
        /// The literal's lexical form.
        lexical: String,
        /// The datatype IRI, or `None` for a plain literal.
        datatype: Option<String>,
    },
}

impl AggregateRhs {
    /// The byte-stable content-key fragment (variant-tagged so a property IRI never collides with a
    /// literal of the same text).
    fn content_key(&self) -> String {
        match self {
            AggregateRhs::Property(p) => format!("prop={}", key_field(p)),
            AggregateRhs::Literal { lexical, datatype } => format!(
                "lit={}{SEP}{}",
                key_field(lexical),
                key_field(datatype.as_deref().unwrap_or(""))
            ),
        }
    }
}

/// An aggregate-comparison satellite on a [`ConstraintIr`]: the closed-world integrity condition
/// "`function([DISTINCT] path)` over the focus `comparator` `compare_to`". The realized FOL
/// [`Formula`] core has no aggregate node (an aggregate is a reduce, not a first-order predication;
/// mirrors [`super::AggregateSpec`], the `LogicRule` reduce spec, which is likewise a satellite and
/// not a `Formula` construct), so an aggregate integrity is carried HERE as a structured satellite
/// and lowered to a `SELECT $this … GROUP BY $this HAVING(…)` `sh:SPARQLConstraint` — reusing the
/// SHACL-AF `GROUP BY` machinery rather than a bespoke aggregate formula lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateComparison {
    /// The aggregate function, an upper-case SPARQL name (`COUNT`, `SUM`, `MIN`, `MAX`).
    pub function: String,
    /// Whether the aggregate is over `DISTINCT` values (`COUNT(DISTINCT ?x)`).
    pub distinct: bool,
    /// The predicate IRI whose objects over the focus are aggregated (`$this <path> ?x`).
    pub path: String,
    /// The comparator the aggregate is tested against (the authored invariant operator).
    pub comparator: AggregateComparator,
    /// The right-hand side the aggregate is compared to.
    pub compare_to: AggregateRhs,
}

impl AggregateComparison {
    /// Construct, validating the function is one of the supported aggregates and the path is a
    /// non-empty IRI. A `Property` right-hand side must likewise be a non-empty IRI.
    pub fn new(
        function: impl Into<String>,
        distinct: bool,
        path: impl Into<String>,
        comparator: AggregateComparator,
        compare_to: AggregateRhs,
    ) -> gmeow_errors::Result<Self> {
        let function = function.into().to_ascii_uppercase();
        if !matches!(function.as_str(), "COUNT" | "SUM" | "MIN" | "MAX") {
            return Err(ir_err(format!(
                "AggregateComparison.function '{function}' must be one of COUNT/SUM/MIN/MAX"
            )));
        }
        let path = path.into();
        if path.trim().is_empty() {
            return Err(ir_err(
                "AggregateComparison.path must be a non-empty predicate IRI",
            ));
        }
        if let AggregateRhs::Property(p) = &compare_to
            && p.trim().is_empty()
        {
            return Err(ir_err(
                "AggregateComparison.compare_to property must be a non-empty IRI",
            ));
        }
        Ok(Self {
            function,
            distinct,
            path,
            comparator,
            compare_to,
        })
    }

    /// The append-only content-key segment for this satellite.
    fn content_key(&self) -> String {
        format!(
            "fn={}{SEP}distinct={}{SEP}path={}{SEP}cmp={}{SEP}{}",
            self.function,
            self.distinct,
            key_field(&self.path),
            self.comparator.as_key(),
            self.compare_to.content_key(),
        )
    }
}

/// One leg (hop) of a [`JoinAggregate`]'s multi-hop join: a reified relation record whose two role
/// predicates chain the endpoints and whose `value` predicate carries the numeric leaf value
/// multiplied into the group product. For the general-CW ∂²=0 check a leg is an incidence record —
/// `source` = `incidenceCoface` (record → higher cell), `target` = `incidenceFace` (record → lower
/// cell), `value` = `incidenceSign` — so two chained legs traverse coface → cell → far-face and the
/// group product is `sign₁ · sign₂`. The chain's shared join variable is `leg[k].target =
/// leg[k+1].source`; there is no cartesian product over cells, so the projected SPARQL scales with
/// the number of incidence RECORDS, not with cells².
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinLeg {
    /// Optional class the record node is typed with (an index-friendly anchor and a well-formedness
    /// guard; `None` ⇒ the record is bound only by its role/value predicates).
    pub record_type: Option<String>,
    /// The predicate from the record to this leg's SOURCE endpoint (`?record <source> ?from`). The
    /// first leg's source binds to the focus `$this`; every later leg's source binds to the
    /// preceding leg's target (the shared join variable).
    pub source: String,
    /// The predicate from the record to this leg's TARGET endpoint (`?record <target> ?to`). The
    /// last leg's target is the far endpoint of the group key.
    pub target: String,
    /// The predicate from the record to the numeric leaf value multiplied into the group product
    /// (`?record <value> ?v`).
    pub value: String,
}

impl JoinLeg {
    /// Construct a join leg, validating the three role/value predicates are non-empty IRIs (and the
    /// optional record type, when present, is a non-empty IRI).
    pub fn new(
        record_type: Option<String>,
        source: impl Into<String>,
        target: impl Into<String>,
        value: impl Into<String>,
    ) -> gmeow_errors::Result<Self> {
        let source = source.into();
        let target = target.into();
        let value = value.into();
        for (label, p) in [("source", &source), ("target", &target), ("value", &value)] {
            if p.trim().is_empty() {
                return Err(ir_err(format!(
                    "JoinLeg.{label} must be a non-empty predicate IRI"
                )));
            }
        }
        if let Some(rt) = &record_type
            && rt.trim().is_empty()
        {
            return Err(ir_err(
                "JoinLeg.record_type must be a non-empty class IRI when present; pass None to \
                 leave it unset",
            ));
        }
        Ok(Self {
            record_type,
            source,
            target,
            value,
        })
    }

    /// The byte-stable content-key fragment for this leg (order-significant within the chain).
    fn content_key(&self) -> String {
        format!(
            "rt={}{SEP}s={}{SEP}t={}{SEP}v={}",
            key_field(self.record_type.as_deref().unwrap_or("")),
            key_field(&self.source),
            key_field(&self.target),
            key_field(&self.value),
        )
    }
}

/// A join-aggregate satellite on a [`ConstraintIr`]: "over an N-hop JOIN (N ≥ 2) whose legs chain
/// through a shared intermediate endpoint, `function` the PRODUCT of the joined leaf values, GROUP
/// BY the (focus, far-endpoint) key, and FIRE when the group value fails `comparator` `threshold`."
/// It is the generalization of [`AggregateComparison`] from a single-predicate focus aggregate to a
/// multi-hop-join product aggregate, and the canonical home of the general-CW ∂²=0 conformance check
/// (a SUM of incidence-sign products over composable cells that must equal 0). Like
/// [`AggregateComparison`] the realized FOL [`Formula`] core has no aggregate/join node, so the
/// structured join is carried HERE and lowered to a `SELECT $this ?far … GROUP BY $this ?far
/// HAVING(…)` `sh:SPARQLConstraint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinAggregate {
    /// The aggregate function, an upper-case SPARQL name (`SUM` for ∂²; `COUNT`/`MIN`/`MAX` accepted).
    pub function: String,
    /// The ordered join legs (at least two — a single hop is not a JOIN). `leg[k].target` is the
    /// shared join variable that `leg[k+1].source` re-binds.
    pub legs: Vec<JoinLeg>,
    /// The comparator the group aggregate is tested against — the authored INVARIANT operator (the
    /// CONFORMING condition, e.g. `=` for "sum = 0"); the projected `HAVING` uses its negation
    /// because a `sh:select` returns the VIOLATING groups.
    pub comparator: AggregateComparator,
    /// The lexical form of the fixed literal threshold the aggregate is compared to (e.g. `0`).
    pub threshold_lexical: String,
    /// The threshold literal's datatype IRI (`None` ⇒ a plain literal).
    pub threshold_datatype: Option<String>,
}

impl JoinAggregate {
    /// Construct, validating the function is a supported aggregate, there are at least two legs (a
    /// genuine multi-hop join), and the threshold lexical form is non-empty.
    pub fn new(
        function: impl Into<String>,
        legs: Vec<JoinLeg>,
        comparator: AggregateComparator,
        threshold_lexical: impl Into<String>,
        threshold_datatype: Option<String>,
    ) -> gmeow_errors::Result<Self> {
        let function = function.into().to_ascii_uppercase();
        if !matches!(function.as_str(), "COUNT" | "SUM" | "MIN" | "MAX") {
            return Err(ir_err(format!(
                "JoinAggregate.function '{function}' must be one of COUNT/SUM/MIN/MAX"
            )));
        }
        if legs.len() < 2 {
            return Err(ir_err(format!(
                "JoinAggregate needs at least two join legs to be a multi-hop JOIN; found {}",
                legs.len()
            )));
        }
        let threshold_lexical = threshold_lexical.into();
        if threshold_lexical.trim().is_empty() {
            return Err(ir_err(
                "JoinAggregate.threshold_lexical must be a non-empty literal (the fixed comparison \
                 value, e.g. 0)",
            ));
        }
        Ok(Self {
            function,
            legs,
            comparator,
            threshold_lexical,
            threshold_datatype,
        })
    }

    /// The append-only content-key segment for this satellite (order-significant leg chain folded in).
    fn content_key(&self) -> String {
        let mut legs = String::new();
        for (i, l) in self.legs.iter().enumerate() {
            if i > 0 {
                legs.push(SEP);
            }
            legs.push_str(&key_field(&l.content_key()));
        }
        format!(
            "fn={}{SEP}legs={}{SEP}cmp={}{SEP}thr={}{SEP}{}",
            self.function,
            key_field(&legs),
            self.comparator.as_key(),
            key_field(&self.threshold_lexical),
            key_field(self.threshold_datatype.as_deref().unwrap_or("")),
        )
    }
}

/// Length-prefix a free-form fragment so field boundaries can never collide when fragments
/// are concatenated into a content key (mirrors the `validation` module's helper verbatim).
fn key_field(s: &str) -> String {
    format!("{}:{s}", s.len())
}

/// A named closed-world procedural constraint (`logic:Constraint`): the typed home for a
/// closed-world integrity condition whose violation is a finding. The canonical form the
/// `sh:SPARQLConstraint` surface projects from. Identity is the content-addressed
/// [`Self::content_key`]; the `iri` is the sort key.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintIr {
    /// IRI string of the constraint individual (identity / sort key).
    pub iri: String,
    /// The closed-world integrity condition: an outer range-restricted `∀`-guarded
    /// [`Formula::Forall`] whose body is the per-focus condition. Reuses the realized FOL
    /// [`Formula`] core verbatim — no bespoke constraint AST.
    pub integrity: Formula,
    /// The focus-node selector, DERIVED from the outermost `∀`'s guard atom (the class the
    /// bound `$this`-analogue is `rdf:type`-restricted to, or the predicate it is the
    /// subject / object of). Never authored directly — [`Self::new`] extracts it and
    /// hard-fails if the integrity is not a range-restricted `∀`-guarded condition.
    pub target: ShapeTarget,
    /// The `sh:severity` a violation reports at.
    pub severity: ShaclSeverity,
    /// The advisory violation message (`None` ⇒ none). **Load-bearing-false**: carried for
    /// validation-failure UX but MUST NOT enter [`Self::content_key`], so two constraints
    /// differing only in message share one identity.
    pub message: Option<String>,
    /// The gmeow-domain term this constraint formalizes (`None` ⇒ none) — the back-reference
    /// later projected as `logic:formalizes`. Annotation-level (like the `logic:formalizes`
    /// annotation property, which carries no DL/EL profile weight), so excluded from the
    /// content key.
    pub formalizes: Option<String>,
    /// Typed conformance failure raised by the projected constraint shape. Annotation-level and
    /// deliberately excluded from the formula's semantic identity.
    pub failure_class: Option<String>,
    /// The aggregate-comparison satellite (`None` ⇒ an ordinary formula constraint). The realized
    /// FOL [`Formula`] core has no aggregate node, so an aggregate integrity is carried here as a
    /// structured [`AggregateComparison`] and lowered to a `GROUP BY`/`HAVING`
    /// `sh:SPARQLConstraint`. Folded into [`Self::content_key`] only when present (append-only:
    /// absent ⇒ the byte-identical historical key).
    pub aggregate: Option<AggregateComparison>,
    /// The join-aggregate satellite (`None` ⇒ not a join-aggregate constraint). Carries the
    /// multi-hop-join product aggregate that generalizes [`Self::aggregate`], lowered to a
    /// `GROUP BY $this ?far HAVING(…)` `sh:SPARQLConstraint`. Folded into [`Self::content_key`]
    /// only when present (append-only: absent ⇒ the byte-identical historical key).
    pub join_aggregate: Option<JoinAggregate>,
}

impl ConstraintIr {
    /// Construct a procedural constraint, DERIVING [`Self::target`] from the integrity
    /// formula's outermost `∀` guard. **Hard-fails** with a clear message when `integrity`
    /// is not a range-restricted, `∀`-guarded condition — i.e. it must be
    /// `∀ this. guard(this) → condition(this)` where `guard(this)` names either a class
    /// membership (`rdf:type(this, C)` ⇒ [`ShapeTarget::Class`]) or a predicate the focus is
    /// the subject / object of (⇒ [`ShapeTarget::SubjectsOf`] / [`ShapeTarget::ObjectsOf`]).
    /// Validates the IRI is a non-empty string.
    pub fn new(
        iri: impl Into<String>,
        integrity: Formula,
        severity: ShaclSeverity,
        message: Option<String>,
    ) -> gmeow_errors::Result<Self> {
        let iri = iri.into();
        if iri.trim().is_empty() {
            return Err(ir_err("ConstraintIr.iri must be a non-empty IRI string"));
        }
        if let Some(msg) = &message
            && msg.trim().is_empty()
        {
            return Err(ir_err(
                "ConstraintIr.message must be a non-empty string when present; pass None to \
                 leave it unset",
            ));
        }
        let target = target_from_integrity(&integrity)?;
        Ok(Self {
            iri,
            integrity,
            target,
            severity,
            message,
            formalizes: None,
            failure_class: None,
            aggregate: None,
            join_aggregate: None,
        })
    }

    /// Attach the aggregate-comparison satellite (the structured `GROUP BY`/`HAVING` form the
    /// SPARQL projection lowers). Chainable; folded into the content key. The integrity formula
    /// still carries the honest reified FOL rendering of the same condition (so the FOL canon is
    /// complete), while this satellite drives the real SPARQL-aggregate projection.
    pub fn with_aggregate(mut self, aggregate: AggregateComparison) -> Self {
        self.aggregate = Some(aggregate);
        self
    }

    /// Attach the join-aggregate satellite (the structured multi-hop-join `GROUP BY`/`HAVING` form
    /// the SPARQL projection lowers). Chainable; folded into the content key. The integrity formula
    /// still carries the honest reified FOL rendering of the same condition, while this satellite
    /// drives the real join + product + aggregate SPARQL projection.
    pub fn with_join_aggregate(mut self, join_aggregate: JoinAggregate) -> Self {
        self.join_aggregate = Some(join_aggregate);
        self
    }

    /// Attach the `logic:formalizes` back-reference (the gmeow-domain term the constraint
    /// formalizes). Chainable; annotation-level, so it never perturbs the content key. A
    /// blank term is rejected (a required back-reference that says nothing is a determinism
    /// hazard, not a silent no-op).
    pub fn with_formalizes(mut self, formalizes: impl Into<String>) -> gmeow_errors::Result<Self> {
        let formalizes = formalizes.into();
        if formalizes.trim().is_empty() {
            return Err(ir_err(
                "ConstraintIr.with_formalizes: the formalized term must be a non-empty IRI",
            ));
        }
        self.formalizes = Some(formalizes);
        Ok(self)
    }

    /// Attach the unique typed conformance-failure class projected with this constraint.
    pub fn with_failure_class(
        mut self,
        failure_class: impl Into<String>,
    ) -> gmeow_errors::Result<Self> {
        let failure_class = failure_class.into();
        if failure_class.trim().is_empty() {
            return Err(ir_err(
                "ConstraintIr.with_failure_class: failure class must be a non-empty IRI",
            ));
        }
        if self.failure_class.is_some() {
            return Err(ir_err(format!(
                "ConstraintIr {} has duplicate gmeow:enforcesFailureClass metadata",
                self.iri
            )));
        }
        self.failure_class = Some(failure_class);
        Ok(self)
    }

    /// Stable sort key for canonical ordering — the constraint IRI is unique.
    pub fn sort_key(&self) -> String {
        self.iri.clone()
    }

    /// A deterministic full-content key for canonical equality. Public to the crate so
    /// [`super::LogicProgram::canonical_key`] can fold it into the program key at the fixed
    /// tail. Folded over `iri` + `target` + `integrity`'s alpha/order-normalized key +
    /// `severity`. The advisory `message` and the annotation-level `formalizes` are
    /// **excluded** by design.
    pub(crate) fn content_key(&self) -> String {
        let base = format!(
            "iri={}{SEP}{}{SEP}integrity={}{SEP}sev={}",
            key_field(&self.iri),
            self.target.content_key(),
            key_field(&self.integrity.content_key()),
            self.severity.as_str(),
        );
        // Append-only: an aggregate-free constraint keeps the byte-identical historical key.
        let with_agg = match &self.aggregate {
            Some(agg) => format!("{base}{SEP}agg={}", key_field(&agg.content_key())),
            None => base,
        };
        // Append-only: a join-aggregate-free constraint keeps the byte-identical historical key.
        match &self.join_aggregate {
            Some(ja) => format!("{with_agg}{SEP}joinagg={}", key_field(&ja.content_key())),
            None => with_agg,
        }
    }
}

/// Derive the [`ShapeTarget`] from the outermost `∀` guard of a range-restricted integrity
/// condition, or hard-fail with a clear diagnostic. The accepted shape is
/// `∀ this[, …]. guard(this) → condition` where `guard(this)` is the antecedent of the `∀`
/// body's material implication — either a single atom or a conjunction of atoms — and names
/// how the focus `this` (the FIRST bound variable) ranges:
///
/// * `rdf:type(this, C)` ⇒ [`ShapeTarget::Class`] `C` (preferred when present),
/// * `P(this, _)` ⇒ [`ShapeTarget::SubjectsOf`] `P`,
/// * `P(_, this)` ⇒ [`ShapeTarget::ObjectsOf`] `P`.
fn target_from_integrity(integrity: &Formula) -> gmeow_errors::Result<ShapeTarget> {
    let Formula::Forall { vars, body } = integrity else {
        return Err(ir_err(
            "ConstraintIr integrity must be a range-restricted universal \
             (∀ this. guard(this) → condition); the top node is not a ∀",
        ));
    };
    let focus = vars.first().ok_or_else(|| {
        ir_err(
            "ConstraintIr integrity ∀ binds no focus variable; a range-restricted constraint needs \
             a bound $this-analogue",
        )
    })?;
    let Formula::Implies(antecedent, _consequent) = body.as_ref() else {
        return Err(ir_err(
            "ConstraintIr integrity must be a guarded implication \
             (∀ this. guard(this) → condition); the ∀ body is not a material implication",
        ));
    };
    // The guard is either a single atom or a conjunction of atoms; gather the atoms.
    let guard_atoms: Vec<&Formula> = match antecedent.as_ref() {
        atom @ Formula::Atom { .. } => vec![atom],
        Formula::And(fs) => fs.iter().collect(),
        _ => {
            return Err(ir_err(
                "ConstraintIr integrity guard must be an atom or a conjunction of atoms that \
                 range-restricts the focus variable",
            ));
        }
    };

    // Prefer a class-membership guard `rdf:type(this, C)`.
    for atom in &guard_atoms {
        if let Formula::Atom { relation, args } = atom
            && matches!(relation, Term::Iri(iri) if iri == RDF_TYPE)
            && args.len() == 2
            && matches!(&args[0], Term::Var(v) if v == focus)
            && let Term::Iri(class) = &args[1]
        {
            return Ok(ShapeTarget::Class(class.clone()));
        }
    }
    // A `sparqlTarget(this, "SELECT …")` marker carries a raw SPARQL focus selector (checked before
    // the generic subject branch, which would otherwise read it as `SubjectsOf`). Its second
    // argument is the literal select body.
    for atom in &guard_atoms {
        if let Formula::Atom { relation, args } = atom
            && matches!(relation, Term::Iri(iri) if iri == LOGIC_SPARQL_TARGET)
            && args.len() == 2
            && matches!(&args[0], Term::Var(v) if v == focus)
            && let Term::Literal { lexical, .. } = &args[1]
        {
            return Ok(ShapeTarget::Sparql(lexical.clone()));
        }
    }
    // A `directType(this, C)` marker range-restricts to the DIRECT instances of `C` (checked
    // before the generic subject branch, which would otherwise read it as `SubjectsOf`).
    for atom in &guard_atoms {
        if let Formula::Atom { relation, args } = atom
            && matches!(relation, Term::Iri(iri) if iri == LOGIC_DIRECT_TYPE)
            && args.len() == 2
            && matches!(&args[0], Term::Var(v) if v == focus)
            && let Term::Iri(class) = &args[1]
        {
            return Ok(ShapeTarget::DirectClass(class.clone()));
        }
    }
    // Else a binary predicate guard with the focus as its subject.
    for atom in &guard_atoms {
        if let Formula::Atom { relation, args } = atom
            && let Term::Iri(pred) = relation
            && args.len() == 2
            && matches!(&args[0], Term::Var(v) if v == focus)
        {
            return Ok(ShapeTarget::SubjectsOf(pred.clone()));
        }
    }
    // Else a binary predicate guard with the focus as its object.
    for atom in &guard_atoms {
        if let Formula::Atom { relation, args } = atom
            && let Term::Iri(pred) = relation
            && args.len() == 2
            && matches!(&args[1], Term::Var(v) if v == focus)
        {
            return Ok(ShapeTarget::ObjectsOf(pred.clone()));
        }
    }
    Err(ir_err(format!(
        "ConstraintIr integrity guard does not range-restrict the focus variable '{focus}': no \
         guard atom is rdf:type(this, C) or a binary predicate over this"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parse_logic_str;

    /// The `logic:` namespace prefix + rdf, used by every authored-RDF fixture below.
    const PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <https://ex/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
";

    /// Parse a `logic:` Turtle fixture and return its constraints (asserting no parse error).
    fn constraints_of(turtle: &str) -> Vec<ConstraintIr> {
        let src = format!("{PREFIXES}{turtle}");
        let (program, diagnostics) = parse_logic_str(&src, None).expect("fixture must parse");
        // A malformed-constraint fixture would surface a MALFORMED_CONSTRAINT warning; the
        // seven pattern fixtures are all well-formed, so none is expected.
        assert!(
            !diagnostics.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"),
            "unexpected MALFORMED_CONSTRAINT diagnostics: {diagnostics:?}"
        );
        program.constraints
    }

    /// A guarded `∀ this. rdf:type(this, ex:Widget) → <body>` scaffold, so each pattern
    /// fixture only has to author its per-focus condition `<body>`.
    fn guarded(iri: &str, body_ttl: &str, body_node: &str) -> String {
        format!(
            "\
{iri} a logic:Constraint ;
  logic:severity \"Violation\" ;
  logic:integrity {iri}_all .

{iri}_all a logic:Formula ;
  logic:forall {iri}_impl ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"this\" ] .

{iri}_impl a logic:Formula ;
  logic:antecedent {iri}_guard ;
  logic:consequent {body_node} .

{iri}_guard a logic:Formula ;
  logic:relation rdf:type ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termIri ex:Widget ] .

{body_ttl}
"
        )
    }

    #[test]
    fn p1_choice_group_exactly_one_round_trips() {
        // ∀ this. Widget(this) → ((∃a. hasA(this,a) ∧ ¬∃b. hasB(this,b))
        //                        ∨ (¬∃a. hasA(this,a) ∧ ∃b. hasB(this,b)))
        let body = "\
ex:c1_body a logic:Formula ;
  logic:or ex:c1_left , ex:c1_right .

ex:c1_left a logic:Formula ;
  logic:and ex:c1_a , ex:c1_notb .
ex:c1_right a logic:Formula ;
  logic:and ex:c1_nota , ex:c1_b .

ex:c1_a a logic:Formula ;
  logic:exists ex:c1_atomA ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"a\" ] .
ex:c1_b a logic:Formula ;
  logic:exists ex:c1_atomB ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c1_notb a logic:Formula ; logic:not ex:c1_b .
ex:c1_nota a logic:Formula ; logic:not ex:c1_a .

ex:c1_atomA a logic:Formula ;
  logic:relation ex:hasA ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"a\" ] .
ex:c1_atomB a logic:Formula ;
  logic:relation ex:hasB ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
        let cs = constraints_of(&guarded("ex:c1", body, "ex:c1_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
        // The key is stable across a re-parse of the identical source.
        let again = constraints_of(&guarded("ex:c1", body, "ex:c1_body"));
        assert_eq!(cs[0].content_key(), again[0].content_key());
    }

    #[test]
    fn p2_guarded_implication_round_trips() {
        // ∀ this. Widget(this) → ∃c. companion(this, c)
        let body = "\
ex:c2_body a logic:Formula ;
  logic:exists ex:c2_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:c2_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
        let cs = constraints_of(&guarded("ex:c2", body, "ex:c2_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
        assert!(cs[0].content_key().contains("class="));
    }

    #[test]
    fn authored_constraint_failure_class_dedupes_identical_values() {
        let body = "\
ex:fc_body a logic:Formula ;
  logic:exists ex:fc_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:fc_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
        let turtle = format!(
            "{}\nex:fc <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:Failure, ex:Failure .",
            guarded("ex:fc", body, "ex:fc_body")
        );
        let cs = constraints_of(&turtle);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].failure_class.as_deref(), Some("https://ex/Failure"));
    }

    #[test]
    fn authored_constraint_failure_class_rejects_distinct_values() {
        let body = "\
ex:fc_bad_body a logic:Formula ;
  logic:exists ex:fc_bad_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:fc_bad_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
        let turtle = format!(
            "{PREFIXES}{}\nex:fc_bad <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:FailureA, ex:FailureB .",
            guarded("ex:fc_bad", body, "ex:fc_bad_body")
        );
        let (program, diagnostics) = parse_logic_str(&turtle, None).expect("fixture must parse");
        assert!(program.constraints.is_empty());
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "MALFORMED_CONSTRAINT" && diagnostic.message.contains("distinct")
        }));
    }

    #[test]
    fn authored_constraint_failure_class_rejects_literal_value() {
        let body = "\
ex:fc_literal_body a logic:Formula ;
  logic:exists ex:fc_literal_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"c\" ] .
ex:fc_literal_atom a logic:Formula ;
  logic:relation ex:companion ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"c\" ] .";
        let turtle = format!(
            "{PREFIXES}{}\nex:fc_literal <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> \"Failure\" .",
            guarded("ex:fc_literal", body, "ex:fc_literal_body")
        );
        let (program, diagnostics) = parse_logic_str(&turtle, None).expect("fixture must parse");
        assert!(program.constraints.is_empty());
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "MALFORMED_CONSTRAINT"
                && diagnostic.message.contains("must be an IRI")
        }));
    }

    #[test]
    fn p3_disjunctive_requiredness_round_trips() {
        // ∀ this. Widget(this) → (∃a. hasA(this,a) ∨ ∃b. hasB(this,b))
        let body = "\
ex:c3_body a logic:Formula ;
  logic:or ex:c3_a , ex:c3_b .
ex:c3_a a logic:Formula ;
  logic:exists ex:c3_atomA ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"a\" ] .
ex:c3_b a logic:Formula ;
  logic:exists ex:c3_atomB ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c3_atomA a logic:Formula ;
  logic:relation ex:hasA ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"a\" ] .
ex:c3_atomB a logic:Formula ;
  logic:relation ex:hasB ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
        let cs = constraints_of(&guarded("ex:c3", body, "ex:c3_body"));
        assert_eq!(cs.len(), 1);
        // Disjunctive body ⇒ the integrity formula carries the Disjunctive shape tag.
        assert!(
            cs[0]
                .integrity
                .shape_tags()
                .contains(&crate::ir::FormulaShape::Disjunctive)
        );
    }

    #[test]
    fn p4_path_value_type_membership_round_trips() {
        // ∀ this. Widget(this) → ∀v. part(this, v) → Part(v)
        let body = "\
ex:c4_body a logic:Formula ;
  logic:forall ex:c4_inner ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"v\" ] .
ex:c4_inner a logic:Formula ;
  logic:antecedent ex:c4_path ;
  logic:consequent ex:c4_type .
ex:c4_path a logic:Formula ;
  logic:relation ex:part ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"v\" ] .
ex:c4_type a logic:Formula ;
  logic:relation rdf:type ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"v\" ] ,
                 [ logic:termIndex 1 ; logic:termIri ex:Part ] .";
        let cs = constraints_of(&guarded("ex:c4", body, "ex:c4_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
    }

    #[test]
    fn p5_cross_node_co_occurrence_round_trips() {
        // ∀ this. Widget(this) → ∀o. linked(this, o) → ∃m. marker(o, m)
        let body = "\
ex:c5_body a logic:Formula ;
  logic:forall ex:c5_inner ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"o\" ] .
ex:c5_inner a logic:Formula ;
  logic:antecedent ex:c5_link ;
  logic:consequent ex:c5_ex .
ex:c5_link a logic:Formula ;
  logic:relation ex:linked ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"o\" ] .
ex:c5_ex a logic:Formula ;
  logic:exists ex:c5_marker ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"m\" ] .
ex:c5_marker a logic:Formula ;
  logic:relation ex:marker ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"o\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"m\" ] .";
        let cs = constraints_of(&guarded("ex:c5", body, "ex:c5_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
    }

    #[test]
    fn p6_aggregate_comparison_round_trips() {
        // ∀ this. Widget(this) → ∃n. (partCount(this, n) ∧ atMost(n, "10"^^xsd:integer))
        //
        // NOTE (P6 aggregation finding): the realized FOL `Formula` core has NO aggregate /
        // reduce node — `AggregateSpec` is a `LogicRule`-only construct with no formula-level
        // analogue. An aggregate comparison is therefore authored the ONLY honest FOL way: as
        // an atomic predication over a reified aggregate relation (`partCount(this, n)`) plus a
        // comparison atom (`atMost(n, 10)`). This is a genuine FOL encoding, not a stub — it
        // round-trips with a stable key like every other pattern.
        let body = "\
ex:c6_body a logic:Formula ;
  logic:exists ex:c6_conj ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"n\" ] .
ex:c6_conj a logic:Formula ;
  logic:and ex:c6_count , ex:c6_cmp .
ex:c6_count a logic:Formula ;
  logic:relation ex:partCount ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"n\" ] .
ex:c6_cmp a logic:Formula ;
  logic:relation ex:atMost ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"n\" ] ,
                 [ logic:termIndex 1 ; logic:termLiteral \"10\" ;
                   logic:termLiteralDatatype xsd:integer ] .";
        let cs = constraints_of(&guarded("ex:c6", body, "ex:c6_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
        let again = constraints_of(&guarded("ex:c6", body, "ex:c6_body"));
        assert_eq!(cs[0].content_key(), again[0].content_key());
    }

    #[test]
    fn p7_forbidden_pattern_round_trips() {
        // ∀ this. Widget(this) → ¬∃b. forbidden(this, b)
        let body = "\
ex:c7_body a logic:Formula ; logic:not ex:c7_ex .
ex:c7_ex a logic:Formula ;
  logic:exists ex:c7_atom ;
  logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"b\" ] .
ex:c7_atom a logic:Formula ;
  logic:relation ex:forbidden ;
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] ,
                 [ logic:termIndex 1 ; logic:termVariable \"b\" ] .";
        let cs = constraints_of(&guarded("ex:c7", body, "ex:c7_body"));
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].target, ShapeTarget::Class("https://ex/Widget".into()));
        assert!(
            cs[0]
                .integrity
                .shape_tags()
                .contains(&crate::ir::FormulaShape::StrongNegation)
        );
    }

    #[test]
    fn subjects_of_and_objects_of_targets_are_derived_from_the_guard() {
        // A predicate-guard `P(this, _)` ⇒ SubjectsOf; `P(_, this)` ⇒ ObjectsOf.
        let this = Term::Var("this".into());
        let other = Term::Var("y".into());
        let pred = Term::Iri("https://ex/P".into());
        let guard_subj = Formula::atom(pred.clone(), vec![this.clone(), other.clone()]).unwrap();
        let guard_obj = Formula::atom(pred.clone(), vec![other, this.clone()]).unwrap();
        let cond = Formula::atom(Term::Iri("https://ex/ok".into()), vec![this.clone()]).unwrap();
        let mk = |guard: Formula| Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(Box::new(guard), Box::new(cond.clone()))),
        };
        let subj = ConstraintIr::new(
            "https://ex/cs",
            mk(guard_subj),
            ShaclSeverity::Violation,
            None,
        )
        .unwrap();
        assert_eq!(subj.target, ShapeTarget::SubjectsOf("https://ex/P".into()));
        let obj = ConstraintIr::new(
            "https://ex/co",
            mk(guard_obj),
            ShaclSeverity::Violation,
            None,
        )
        .unwrap();
        assert_eq!(obj.target, ShapeTarget::ObjectsOf("https://ex/P".into()));
    }

    #[test]
    fn message_is_excluded_from_content_key() {
        let this = Term::Var("this".into());
        let integrity = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        Term::Iri(RDF_TYPE.into()),
                        vec![this.clone(), Term::Iri("https://ex/W".into())],
                    )
                    .unwrap(),
                ),
                Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
            )),
        };
        let a = ConstraintIr::new(
            "https://ex/c",
            integrity.clone(),
            ShaclSeverity::Violation,
            Some("first message".into()),
        )
        .unwrap();
        let b = ConstraintIr::new(
            "https://ex/c",
            integrity,
            ShaclSeverity::Violation,
            Some("a completely different message".into()),
        )
        .unwrap();
        assert_eq!(
            a.content_key(),
            b.content_key(),
            "message must not affect the content key"
        );
        // Formalizes is likewise annotation-level and excluded.
        let c = a.clone().with_formalizes("https://ex/gmeow/Term").unwrap();
        assert_eq!(a.content_key(), c.content_key());
        let typed = a.clone().with_failure_class("https://ex/Failure").unwrap();
        assert_eq!(a.content_key(), typed.content_key());
        let err = typed
            .with_failure_class("https://ex/OtherFailure")
            .unwrap_err();
        assert!(err.message().contains("duplicate"));
    }

    #[test]
    fn target_extraction_hard_fails_on_a_non_guarded_formula() {
        // A bare atom (no ∀) is not a range-restricted constraint.
        let bare = Formula::atom(
            Term::Iri("https://ex/p".into()),
            vec![Term::Var("x".into()), Term::Var("y".into())],
        )
        .unwrap();
        let err =
            ConstraintIr::new("https://ex/c", bare, ShaclSeverity::Violation, None).unwrap_err();
        assert!(
            err.message().contains("range-restricted universal"),
            "got: {err}"
        );

        // A ∀ whose body is not an implication (no guard) also fails.
        let unguarded = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(
                Formula::atom(
                    Term::Iri("https://ex/p".into()),
                    vec![Term::Var("this".into())],
                )
                .unwrap(),
            ),
        };
        let err = ConstraintIr::new("https://ex/c", unguarded, ShaclSeverity::Violation, None)
            .unwrap_err();
        assert!(err.message().contains("guarded implication"), "got: {err}");
    }

    #[test]
    fn aggregate_satellite_participates_in_the_content_key() {
        // Two constraints identical but for their aggregate satellite must have distinct identities,
        // and an aggregate-free peer must keep the byte-identical historical key (append-only).
        let this = Term::Var("this".into());
        let integrity = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        Term::Iri(RDF_TYPE.into()),
                        vec![this.clone(), Term::Iri("https://ex/W".into())],
                    )
                    .unwrap(),
                ),
                Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
            )),
        };
        let base =
            ConstraintIr::new("https://ex/c", integrity, ShaclSeverity::Violation, None).unwrap();
        let eq = base.clone().with_aggregate(
            AggregateComparison::new(
                "COUNT",
                true,
                "https://ex/hasAxis",
                AggregateComparator::Eq,
                AggregateRhs::Property("https://ex/dimensionCount".into()),
            )
            .unwrap(),
        );
        // A different comparator ⇒ a different identity.
        let ne = base.clone().with_aggregate(
            AggregateComparison::new(
                "COUNT",
                true,
                "https://ex/hasAxis",
                AggregateComparator::Ne,
                AggregateRhs::Property("https://ex/dimensionCount".into()),
            )
            .unwrap(),
        );
        assert_ne!(base.content_key(), eq.content_key());
        assert_ne!(eq.content_key(), ne.content_key());
        assert!(eq.content_key().contains("agg="));
        assert!(!base.content_key().contains("agg="));
    }

    #[test]
    fn join_aggregate_satellite_participates_in_the_content_key_and_is_append_only() {
        // A join-aggregate satellite gives a distinct identity; an aggregate/join-free peer keeps
        // the byte-identical historical key (append-only), and two satellites differing only in a
        // leg's value predicate differ in identity.
        let this = Term::Var("this".into());
        let integrity = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        Term::Iri(RDF_TYPE.into()),
                        vec![this.clone(), Term::Iri("https://ex/TopCell".into())],
                    )
                    .unwrap(),
                ),
                Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
            )),
        };
        let base =
            ConstraintIr::new("https://ex/bsq", integrity, ShaclSeverity::Violation, None).unwrap();
        let leg = |value: &str| {
            JoinLeg::new(
                Some("https://ex/Incidence".to_owned()),
                "https://ex/incidenceCoface",
                "https://ex/incidenceFace",
                value,
            )
            .unwrap()
        };
        let ja = JoinAggregate::new(
            "SUM",
            vec![
                leg("https://ex/incidenceSign"),
                leg("https://ex/incidenceSign"),
            ],
            AggregateComparator::Eq,
            "0",
            Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
        )
        .unwrap();
        let with_ja = base.clone().with_join_aggregate(ja);
        assert!(!base.content_key().contains("joinagg="));
        assert!(with_ja.content_key().contains("joinagg="));
        assert_ne!(base.content_key(), with_ja.content_key());

        // A leg-value difference changes identity.
        let ja_alt = JoinAggregate::new(
            "SUM",
            vec![leg("https://ex/incidenceSign"), leg("https://ex/otherSign")],
            AggregateComparator::Eq,
            "0",
            Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
        )
        .unwrap();
        assert_ne!(
            with_ja.content_key(),
            base.clone().with_join_aggregate(ja_alt).content_key()
        );

        // A single hop is not a JOIN.
        assert!(
            JoinAggregate::new(
                "SUM",
                vec![leg("https://ex/incidenceSign")],
                AggregateComparator::Eq,
                "0",
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn aggregate_comparator_negation_and_symbol_parsing() {
        assert_eq!(AggregateComparator::Eq.negated(), AggregateComparator::Ne);
        assert_eq!(AggregateComparator::Lt.negated(), AggregateComparator::Ge);
        assert_eq!(
            AggregateComparator::from_symbol("≥"),
            Some(AggregateComparator::Ge)
        );
        assert_eq!(
            AggregateComparator::from_symbol("!="),
            Some(AggregateComparator::Ne)
        );
        assert_eq!(AggregateComparator::from_symbol("~"), None);
        assert!(
            AggregateComparison::new(
                "MEAN",
                false,
                "https://ex/p",
                AggregateComparator::Eq,
                AggregateRhs::Literal {
                    lexical: "1".into(),
                    datatype: None
                },
            )
            .is_err()
        );
    }

    #[test]
    fn empty_constraints_program_content_key_is_byte_identical() {
        // A program with no constraints must fold to the exact same canonical key as one
        // constructed before the constraints field existed — the append-only guarantee.
        use crate::ir::{LogicAxiom, LogicProgram};
        let ax = LogicAxiom::ground("https://ex/s", "https://ex/p", "https://ex/o", false).unwrap();
        let base = LogicProgram::new(vec![ax.clone()], vec![], vec![], None);
        let with_empty = LogicProgram::new(vec![ax], vec![], vec![], None).with_constraints(vec![]);
        assert_eq!(
            base.canonical_key(),
            with_empty.canonical_key(),
            "an empty-constraints program must keep the byte-identical historical key"
        );
        assert!(!base.canonical_key().contains("CONSTRAINTS"));
    }

    #[test]
    fn non_empty_constraints_perturb_the_program_key() {
        use crate::ir::LogicProgram;
        let this = Term::Var("this".into());
        let integrity = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        Term::Iri(RDF_TYPE.into()),
                        vec![this.clone(), Term::Iri("https://ex/W".into())],
                    )
                    .unwrap(),
                ),
                Box::new(Formula::atom(Term::Iri("https://ex/ok".into()), vec![this]).unwrap()),
            )),
        };
        let c =
            ConstraintIr::new("https://ex/c", integrity, ShaclSeverity::Violation, None).unwrap();
        let base = LogicProgram::new(vec![], vec![], vec![], None);
        let with_c = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        assert_ne!(base.canonical_key(), with_c.canonical_key());
        assert!(with_c.canonical_key().contains("CONSTRAINTS"));
    }
}
