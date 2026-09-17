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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// An aggregate-BALANCE satellite on a [`ConstraintIr`]: the double-entry balance invariant
/// "within each group, Σ over the focus's postings of the debit-partition amounts equals Σ of the
/// credit-partition amounts". Like [`AggregateComparison`], the realized FOL [`Formula`] core has no
/// aggregate node, so a balance integrity is carried HERE as a structured satellite and lowered to a
/// `SELECT $this … GROUP BY $this ?group HAVING(sumDebits != sumCredits)` `sh:SPARQLConstraint`. It
/// generalizes the single-aggregate [`AggregateComparison`] to a *partitioned two-sum equality over a
/// value-key group*: the focus's postings (`posting_predicate`) are partitioned by
/// `partition_predicate` into a debit side (`debit_value`) and a credit side (`credit_value`); each
/// posting's numeric amount is read via `amount_node_predicate` then `value_predicate`; the group key
/// is read via `amount_node_predicate` then `group_predicate`; and the two partition-sums must be
/// EQUAL within every group.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AggregateBalance {
    /// The predicate from the focus to each posting (`$this <posting_predicate> ?posting`).
    pub posting_predicate: String,
    /// The predicate on a posting whose value selects its partition (debit vs credit).
    pub partition_predicate: String,
    /// The `partition_predicate` value marking a DEBIT posting.
    pub debit_value: String,
    /// The `partition_predicate` value marking a CREDIT posting.
    pub credit_value: String,
    /// The predicate from a posting to its amount node (`?posting <amount_node_predicate> ?amount`).
    pub amount_node_predicate: String,
    /// The predicate from the amount node to its numeric value (`?amount <value_predicate> ?val`).
    pub value_predicate: String,
    /// The predicate from the amount node to the group key (`?amount <group_predicate> ?group`).
    pub group_predicate: String,
}

impl AggregateBalance {
    /// Construct, validating every predicate / partition value is a non-empty IRI.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        posting_predicate: impl Into<String>,
        partition_predicate: impl Into<String>,
        debit_value: impl Into<String>,
        credit_value: impl Into<String>,
        amount_node_predicate: impl Into<String>,
        value_predicate: impl Into<String>,
        group_predicate: impl Into<String>,
    ) -> gmeow_errors::Result<Self> {
        let out = Self {
            posting_predicate: posting_predicate.into(),
            partition_predicate: partition_predicate.into(),
            debit_value: debit_value.into(),
            credit_value: credit_value.into(),
            amount_node_predicate: amount_node_predicate.into(),
            value_predicate: value_predicate.into(),
            group_predicate: group_predicate.into(),
        };
        for (field, v) in [
            ("posting_predicate", &out.posting_predicate),
            ("partition_predicate", &out.partition_predicate),
            ("debit_value", &out.debit_value),
            ("credit_value", &out.credit_value),
            ("amount_node_predicate", &out.amount_node_predicate),
            ("value_predicate", &out.value_predicate),
            ("group_predicate", &out.group_predicate),
        ] {
            if v.trim().is_empty() {
                return Err(ir_err(format!(
                    "AggregateBalance.{field} must be a non-empty IRI"
                )));
            }
        }
        Ok(out)
    }

    /// The append-only content-key segment for this satellite.
    fn content_key(&self) -> String {
        format!(
            "posting={}{SEP}part={}{SEP}debit={}{SEP}credit={}{SEP}amount={}{SEP}value={}{SEP}group={}",
            key_field(&self.posting_predicate),
            key_field(&self.partition_predicate),
            key_field(&self.debit_value),
            key_field(&self.credit_value),
            key_field(&self.amount_node_predicate),
            key_field(&self.value_predicate),
            key_field(&self.group_predicate),
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Additional `logic:formalizes` back-references beyond the primary [`Self::formalizes`]: a
    /// constraint may formalize several gmeow-domain terms at once (e.g. the canonical class it
    /// governs AND the legacy hand-authored shape it reproduces). Each is projected as its own
    /// `logic:formalizes` line. Annotation-level, so excluded from the content key. Sorted,
    /// deduplicated, and never overlapping the primary.
    pub also_formalizes: Vec<String>,
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
    /// The aggregate-BALANCE satellite (`None` ⇒ not a balance constraint). Carries the
    /// partitioned two-sum equality; lowered to a `GROUP BY`/`HAVING` `sh:SPARQLConstraint`.
    /// Folded into [`Self::content_key`] only when present (append-only).
    pub aggregate_balance: Option<AggregateBalance>,
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
            also_formalizes: Vec::new(),
            failure_class: None,
            aggregate: None,
            join_aggregate: None,
            aggregate_balance: None,
        })
    }

    /// Attach the aggregate-balance satellite (the partitioned two-sum equality the `GROUP BY`/
    /// `HAVING` SPARQL projection lowers). Chainable; folded into the content key.
    pub fn with_aggregate_balance(mut self, balance: AggregateBalance) -> Self {
        self.aggregate_balance = Some(balance);
        self
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

    /// Attach additional `logic:formalizes` back-references (beyond the primary). Each must be a
    /// non-empty IRI; the primary is filtered out, and the remainder is sorted and deduplicated so
    /// the projection is deterministic. Chainable; annotation-level (never perturbs the content key).
    pub fn with_also_formalizes(
        mut self,
        also: impl IntoIterator<Item = String>,
    ) -> gmeow_errors::Result<Self> {
        let primary = self.formalizes.clone();
        let mut extra: Vec<String> = Vec::new();
        for term in also {
            if term.trim().is_empty() {
                return Err(ir_err(
                    "ConstraintIr.with_also_formalizes: every formalized term must be a non-empty \
                     IRI",
                ));
            }
            if primary.as_deref() == Some(term.as_str()) {
                continue;
            }
            extra.push(term);
        }
        extra.sort();
        extra.dedup();
        self.also_formalizes = extra;
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

    /// **Hard-fail** when more than one of the three aggregate satellites ([`Self::aggregate`],
    /// [`Self::join_aggregate`], [`Self::aggregate_balance`]) is `Some`. The SPARQL projection
    /// (`projections::shapes::constraint_select`) dispatches by PRIORITY — join_aggregate, then
    /// aggregate_balance, then aggregate — so a constraint carrying more than one would otherwise
    /// have its lower-priority satellite(s) silently dropped from the projected shape: a
    /// no-optionality violation (`.goals`), not a permitted profile choice. Callers MUST invoke
    /// this at the projection chokepoint (every satellite is attached by a chainable `with_*`
    /// builder AFTER [`Self::new`] returns, so `new` itself cannot observe the conflict) so the
    /// malformed constraint hard-fails instead of silently picking one.
    pub fn ensure_single_satellite(&self) -> gmeow_errors::Result<()> {
        let mut present: Vec<&str> = Vec::new();
        if self.aggregate.is_some() {
            present.push("aggregate");
        }
        if self.join_aggregate.is_some() {
            present.push("join_aggregate");
        }
        if self.aggregate_balance.is_some() {
            present.push("aggregate_balance");
        }
        if present.len() > 1 {
            return Err(ir_err(format!(
                "ConstraintIr {} carries {} coexisting aggregate satellites ({}); at most one of \
                 aggregate/join_aggregate/aggregate_balance may be set on a single constraint, \
                 else the SPARQL projection's priority dispatch would silently drop the \
                 lower-priority satellite(s)",
                self.iri,
                present.len(),
                present.join(", "),
            )));
        }
        Ok(())
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
            key_field(self.integrity.content_key().as_str()),
            self.severity.as_str(),
        );
        // Append-only: an aggregate-free constraint keeps the byte-identical historical key.
        let with_agg = match &self.aggregate {
            Some(agg) => format!("{base}{SEP}agg={}", key_field(&agg.content_key())),
            None => base,
        };
        // Append-only: a join-aggregate-free constraint keeps the byte-identical historical key.
        let with_join = match &self.join_aggregate {
            Some(ja) => format!("{with_agg}{SEP}joinagg={}", key_field(&ja.content_key())),
            None => with_agg,
        };
        // Append-only again: a balance-free constraint keeps the prior key.
        match &self.aggregate_balance {
            Some(bal) => format!("{with_join}{SEP}balance={}", key_field(&bal.content_key())),
            None => with_join,
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
    // The guard is a single atom, a conjunction of atoms, or an existential wrapping such a
    // conjunction (`∃x. C(x) ∧ P(x, this)` — the focus is the OBJECT of a predicate whose subject
    // is separately typed). Gather every atom, descending through `∃` and `∧`, so an object-of
    // membership guard yields a well-formed target.
    fn collect_guard_atoms<'a>(f: &'a Formula, out: &mut Vec<&'a Formula>) {
        match f {
            Formula::Atom { .. } => out.push(f),
            Formula::And(fs) => {
                for x in fs {
                    collect_guard_atoms(x, out);
                }
            }
            Formula::Exists { body, .. } => collect_guard_atoms(body, out),
            _ => {}
        }
    }
    let mut guard_atoms: Vec<&Formula> = Vec::new();
    collect_guard_atoms(antecedent.as_ref(), &mut guard_atoms);
    if guard_atoms.is_empty() {
        return Err(ir_err(
            "ConstraintIr integrity guard must be an atom, a conjunction of atoms, or an \
             existential over such a conjunction that range-restricts the focus variable",
        ));
    }

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
            && let Term::Literal(purrdf::RdfLiteral {
                lexical_form: lexical,
                ..
            }) = &args[1]
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

#[path = "constraint.tests.rs"]
#[cfg(test)]
mod tests;
