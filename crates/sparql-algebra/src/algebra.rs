// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SPARQL query algebra (W3C SPARQL 1.1 §18.2), gmeow-owned and RDF 1.2-native.
//!
//! This is the *algebra* form, not a raw syntax tree: solution modifiers
//! (`DISTINCT`, `ORDER BY`, `LIMIT`/`OFFSET`, `GROUP BY`) are encoded as
//! [`GraphPattern`] nodes wrapping the `WHERE` algebra, exactly as the standard
//! translation prescribes. That is why a [`Query::Select`] holds only its root
//! `pattern` and a consumer walks *into* the pattern to find `Project`/`Distinct`/
//! `Slice`/`OrderBy`/`Group`.
//!
//! ## S6 extension seam (#912)
//!
//! This algebra is intentionally a faithful, standard, *evaluable* IR — the form
//! the downstream evaluator S6 (`sparql-eval`) consumes. The greenfield lever for
//! exploiting the native OWL/EL-DL reasoner (e.g. routing `rdfs:subClassOf*` to
//! the DL subsumption closure rather than evaluating the path structurally, or
//! making the entailment regime a first-class concern) is an *evaluation*-time
//! decision and belongs in S6: it would annotate or wrap these nodes there. S5
//! keeps the door open by owning its own enums (free to grow variants/annotations
//! later) rather than cloning a fixed external type.

use crate::ast::{
    GroundTerm, Literal, NamedNode, NamedNodePattern, TermPattern, TriplePattern, Variable,
};

/// A parsed SPARQL query. The four query forms differ only in their head; the
/// `WHERE` clause and all solution modifiers live inside `pattern` as algebra.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Query {
    /// `SELECT` query. `pattern` is the full modifier-wrapped algebra.
    Select {
        /// The root graph pattern (already wrapped by projection/modifiers).
        pattern: GraphPattern,
        /// An explicit `BASE` IRI, if the prologue declared one.
        base_iri: Option<NamedNode>,
    },
    /// `CONSTRUCT` query. `template` is the output triple template.
    Construct {
        /// The `CONSTRUCT { ... }` triple template.
        template: Vec<TriplePattern>,
        /// The `WHERE` algebra.
        pattern: GraphPattern,
        /// An explicit `BASE` IRI, if any.
        base_iri: Option<NamedNode>,
    },
    /// `DESCRIBE` query.
    Describe {
        /// The `WHERE` algebra (or the unit pattern for a bare `DESCRIBE <iri>`).
        pattern: GraphPattern,
        /// The resources to describe (IRIs and/or variables).
        targets: Vec<NamedNodePattern>,
        /// An explicit `BASE` IRI, if any.
        base_iri: Option<NamedNode>,
    },
    /// `ASK` query.
    Ask {
        /// The `WHERE` algebra.
        pattern: GraphPattern,
        /// An explicit `BASE` IRI, if any.
        base_iri: Option<NamedNode>,
    },
}

/// A node of the SPARQL graph-pattern algebra (§18.2). The empty pattern (the
/// identity table `Z`) is represented as `Bgp { patterns: vec![] }`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GraphPattern {
    /// A basic graph pattern: a conjunction of triple patterns.
    Bgp {
        /// The triple patterns (RDF 1.2 quoted triples admitted).
        patterns: Vec<TriplePattern>,
    },
    /// A property-path constraint `subject path object`.
    Path {
        /// The path's subject term.
        subject: TermPattern,
        /// The property path.
        path: PropertyPathExpression,
        /// The path's object term.
        object: TermPattern,
    },
    /// Conjunction (`Join`) of two patterns.
    Join {
        /// Left operand.
        left: Box<GraphPattern>,
        /// Right operand.
        right: Box<GraphPattern>,
    },
    /// `OPTIONAL` (left outer join), with an optional join condition (a `FILTER`
    /// lifted into the `OPTIONAL` per §18.2.2.3).
    LeftJoin {
        /// Left (required) operand.
        left: Box<GraphPattern>,
        /// Right (optional) operand.
        right: Box<GraphPattern>,
        /// The join-condition expression, if the `OPTIONAL` had a `FILTER`.
        expression: Option<Expression>,
    },
    /// A correlated/lateral join (`LATERAL`), kept for algebra completeness.
    Lateral {
        /// Left operand.
        left: Box<GraphPattern>,
        /// Right operand, evaluated per left solution.
        right: Box<GraphPattern>,
    },
    /// `FILTER expr` over an inner pattern.
    Filter {
        /// The filter expression.
        expr: Expression,
        /// The pattern being filtered.
        inner: Box<GraphPattern>,
    },
    /// `UNION` of two patterns.
    Union {
        /// Left operand.
        left: Box<GraphPattern>,
        /// Right operand.
        right: Box<GraphPattern>,
    },
    /// `GRAPH name { ... }`.
    Graph {
        /// The named-graph IRI or variable.
        name: NamedNodePattern,
        /// The inner pattern scoped to that graph.
        inner: Box<GraphPattern>,
    },
    /// `BIND(expression AS variable)` — `Extend` in algebra.
    Extend {
        /// The pattern being extended.
        inner: Box<GraphPattern>,
        /// The newly bound variable.
        variable: Variable,
        /// The expression whose value it binds.
        expression: Expression,
    },
    /// `MINUS` (set difference on compatible solutions).
    Minus {
        /// Left operand.
        left: Box<GraphPattern>,
        /// Right operand (solutions to subtract).
        right: Box<GraphPattern>,
    },
    /// `SERVICE` (federated query). In scope structurally; the evaluator may
    /// reject it. `silent` is the `SILENT` flag.
    Service {
        /// The service endpoint IRI or variable.
        name: NamedNodePattern,
        /// The pattern sent to the endpoint.
        inner: Box<GraphPattern>,
        /// Whether the `SILENT` keyword was present.
        silent: bool,
    },
    /// Inline `VALUES` data.
    Values {
        /// The column variables.
        variables: Vec<Variable>,
        /// The rows; `None` is `UNDEF`.
        bindings: Vec<Vec<Option<GroundTerm>>>,
    },
    /// `ORDER BY`.
    OrderBy {
        /// The pattern being ordered.
        inner: Box<GraphPattern>,
        /// The ordered list of sort keys.
        expression: Vec<OrderExpression>,
    },
    /// Projection (`SELECT` variable list, or `SELECT *`).
    Project {
        /// The pattern being projected.
        inner: Box<GraphPattern>,
        /// The projected variables.
        variables: Vec<Variable>,
    },
    /// `DISTINCT`.
    Distinct {
        /// The pattern whose solutions are de-duplicated.
        inner: Box<GraphPattern>,
    },
    /// `REDUCED`.
    Reduced {
        /// The pattern whose solutions may be de-duplicated.
        inner: Box<GraphPattern>,
    },
    /// `LIMIT`/`OFFSET`.
    Slice {
        /// The pattern being sliced.
        inner: Box<GraphPattern>,
        /// The `OFFSET` (0 if absent).
        start: usize,
        /// The `LIMIT`, if present.
        length: Option<usize>,
    },
    /// `GROUP BY` + aggregates.
    Group {
        /// The pattern being grouped.
        inner: Box<GraphPattern>,
        /// The grouping key variables.
        variables: Vec<Variable>,
        /// The `(output variable, aggregate)` pairs.
        aggregates: Vec<(Variable, AggregateExpression)>,
    },
}

/// A SPARQL property-path expression (§18.1.7 / §9).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PropertyPathExpression {
    /// A single predicate IRI.
    NamedNode(NamedNode),
    /// `^path` — inverse.
    Reverse(Box<PropertyPathExpression>),
    /// `p1 / p2` — sequence.
    Sequence(Box<PropertyPathExpression>, Box<PropertyPathExpression>),
    /// `p1 | p2` — alternative.
    Alternative(Box<PropertyPathExpression>, Box<PropertyPathExpression>),
    /// `path*` — zero or more.
    ZeroOrMore(Box<PropertyPathExpression>),
    /// `path+` — one or more.
    OneOrMore(Box<PropertyPathExpression>),
    /// `path?` — zero or one.
    ZeroOrOne(Box<PropertyPathExpression>),
    /// `!(p1|...|pn)` — negated property set.
    NegatedPropertySet(Vec<NamedNode>),
    /// `path{min,max}` — **bounded repetition** (a GMEOW extension *beyond* SPARQL
    /// 1.1 §9, which has only `*`/`+`/`?`).  `max == None` means unbounded (`{n,}`);
    /// `max == Some(min)` is exactly-`n` (`{n}`).  The invariant `min <= max` (when
    /// `max` is `Some`) is enforced at construction by the parser.
    Range {
        /// The repeated sub-path.
        inner: Box<PropertyPathExpression>,
        /// Inclusive lower bound on repetitions.
        min: u32,
        /// Inclusive upper bound; `None` ⇒ unbounded.
        max: Option<u32>,
    },
    /// A **predicate wildcard** matching ANY predicate (a GMEOW extension beyond
    /// SPARQL 1.1 §9, which can only name predicates).  Optionally scoped to a
    /// predicate namespace IRI prefix (`namespace`), bounding the otherwise
    /// unbounded fan-out.
    Wildcard {
        /// A predicate-namespace IRI prefix the wildcard is restricted to, or
        /// `None` for any namespace.
        namespace: Option<NamedNode>,
    },
}

impl core::fmt::Display for PropertyPathExpression {
    /// Serialize a property path to its SPARQL surface syntax.  The standard
    /// operators round-trip with the parser; the two GMEOW extensions render as
    /// `path{min,max}` (bounded repetition — round-trips) and `<any>` / `<any:ns>`
    /// (predicate wildcard — **emit-only**, no parse, per `LOGIC-PATHS.md`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NamedNode(n) => write!(f, "<{}>", n.as_str()),
            Self::Reverse(a) => write!(f, "^{}", PathElt(a)),
            Self::Sequence(a, b) => write!(f, "{a}/{b}"),
            Self::Alternative(a, b) => write!(f, "{a}|{b}"),
            Self::ZeroOrMore(a) => write!(f, "{}*", PathElt(a)),
            Self::OneOrMore(a) => write!(f, "{}+", PathElt(a)),
            Self::ZeroOrOne(a) => write!(f, "{}?", PathElt(a)),
            Self::Range { inner, min, max } => match max {
                Some(m) if *m == *min => write!(f, "{}{{{min}}}", PathElt(inner)),
                Some(m) => write!(f, "{}{{{min},{m}}}", PathElt(inner)),
                None => write!(f, "{}{{{min},}}", PathElt(inner)),
            },
            Self::NegatedPropertySet(nodes) => {
                let inner = nodes
                    .iter()
                    .map(|n| format!("<{}>", n.as_str()))
                    .collect::<Vec<_>>()
                    .join("|");
                write!(f, "!({inner})")
            }
            Self::Wildcard { namespace } => match namespace {
                Some(ns) => write!(f, "<any:{}>", ns.as_str()),
                None => write!(f, "<any>"),
            },
        }
    }
}

/// Wraps a property path in parentheses when it must be grouped to sit under a
/// postfix operator (`*`/`+`/`?`/`{n,m}`) or `^` — i.e. when it is a sequence or
/// alternative (the lower-precedence binary operators).
struct PathElt<'a>(&'a PropertyPathExpression);

impl core::fmt::Display for PathElt<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            PropertyPathExpression::Sequence(..) | PropertyPathExpression::Alternative(..) => {
                write!(f, "({})", self.0)
            }
            other => write!(f, "{other}"),
        }
    }
}

/// A SPARQL expression (filter/bind/having/order/select-expression position).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expression {
    /// An IRI constant.
    NamedNode(NamedNode),
    /// A literal constant.
    Literal(Literal),
    /// A variable reference.
    Variable(Variable),
    /// `BOUND(?v)`.
    Bound(Variable),
    /// Logical `||`.
    Or(Box<Expression>, Box<Expression>),
    /// Logical `&&`.
    And(Box<Expression>, Box<Expression>),
    /// `=`.
    Equal(Box<Expression>, Box<Expression>),
    /// `sameTerm(a, b)`.
    SameTerm(Box<Expression>, Box<Expression>),
    /// `>`.
    Greater(Box<Expression>, Box<Expression>),
    /// `>=`.
    GreaterOrEqual(Box<Expression>, Box<Expression>),
    /// `<`.
    Less(Box<Expression>, Box<Expression>),
    /// `<=`.
    LessOrEqual(Box<Expression>, Box<Expression>),
    /// `+`.
    Add(Box<Expression>, Box<Expression>),
    /// `-` (binary).
    Subtract(Box<Expression>, Box<Expression>),
    /// `*`.
    Multiply(Box<Expression>, Box<Expression>),
    /// `/`.
    Divide(Box<Expression>, Box<Expression>),
    /// Unary `+`.
    UnaryPlus(Box<Expression>),
    /// Unary `-`.
    UnaryMinus(Box<Expression>),
    /// `!`.
    Not(Box<Expression>),
    /// `expr IN (list)`.
    In(Box<Expression>, Vec<Expression>),
    /// `IF(cond, then, else)`.
    If(Box<Expression>, Box<Expression>, Box<Expression>),
    /// `COALESCE(list)`.
    Coalesce(Vec<Expression>),
    /// A built-in or custom function call.
    FunctionCall(Function, Vec<Expression>),
    /// `EXISTS { pattern }` (`NOT EXISTS` is `Not(Exists(...))`).
    Exists(Box<GraphPattern>),
}

/// A SPARQL function: a built-in (`BuiltInCall`) or a custom IRI-named function.
///
/// Only [`Function::Custom`] carries an IRI; the built-ins are keyword-named and
/// reference no term. The set is complete (the full SPARQL 1.1 `BuiltInCall`
/// surface) so the algebra can subsume any in-corpus call without a fallback.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)] // self-describing 1:1 mappings of SPARQL built-in names
pub enum Function {
    Str,
    Lang,
    LangMatches,
    Datatype,
    Iri,
    Uri,
    BNode,
    Rand,
    Abs,
    Ceil,
    Floor,
    Round,
    Concat,
    SubStr,
    StrLen,
    Replace,
    UCase,
    LCase,
    EncodeForUri,
    Contains,
    StrStarts,
    StrEnds,
    StrBefore,
    StrAfter,
    Year,
    Month,
    Day,
    Hours,
    Minutes,
    Seconds,
    Timezone,
    Tz,
    Now,
    Uuid,
    StrUuid,
    Md5,
    Sha1,
    Sha256,
    Sha384,
    Sha512,
    StrLang,
    StrDt,
    IsIri,
    IsUri,
    IsBlank,
    IsLiteral,
    IsNumeric,
    Regex,
    /// `TRIPLE(s, p, o)` — RDF 1.2 triple-term constructor.
    Triple,
    /// `SUBJECT(t)` — RDF 1.2 triple-term accessor.
    Subject,
    /// `PREDICATE(t)` — RDF 1.2 triple-term accessor.
    Predicate,
    /// `OBJECT(t)` — RDF 1.2 triple-term accessor.
    Object,
    /// `isTRIPLE(t)` — RDF 1.2 triple-term test.
    IsTriple,
    /// A custom function identified by IRI.
    Custom(NamedNode),
}

/// A single `ORDER BY` sort key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OrderExpression {
    /// Ascending (`ASC(expr)` or a bare expression).
    Asc(Expression),
    /// Descending (`DESC(expr)`).
    Desc(Expression),
}

/// A `GROUP BY` aggregate.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AggregateExpression {
    /// `COUNT(*)`.
    CountStar {
        /// Whether `DISTINCT` was present.
        distinct: bool,
    },
    /// An aggregate over an expression, e.g. `SUM(?x)` or `COUNT(DISTINCT ?x)`.
    FunctionCall {
        /// Which aggregate function.
        function: AggregateFunction,
        /// The aggregated expression.
        expression: Box<Expression>,
        /// Whether `DISTINCT` was present.
        distinct: bool,
    },
}

/// The named SPARQL aggregate functions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AggregateFunction {
    /// `COUNT`.
    Count,
    /// `SUM`.
    Sum,
    /// `AVG`.
    Avg,
    /// `MIN`.
    Min,
    /// `MAX`.
    Max,
    /// `SAMPLE`.
    Sample,
    /// `GROUP_CONCAT`, with an optional `SEPARATOR`.
    GroupConcat {
        /// The `SEPARATOR` string, if given.
        separator: Option<String>,
    },
    /// A custom aggregate identified by IRI.
    Custom(NamedNode),
}
