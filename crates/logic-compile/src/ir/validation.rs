// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Validation shapes — the closed-world SHACL / ShEx-shaped subset of the IR.
//!
//! A [`ValidationShapeIr`] is the IR realization of [`NodeKind::ValidationShape`]: a
//! closed-world data-shape condition that targets a class (or a value-keyed selection)
//! and states, per constrained path, the cardinality / datatype / node-kind / value-set /
//! pattern / language / datetime / terminology conditions its focus nodes must satisfy.
//! Its violation is a *finding*, not a derivation. It is the canonical authoring ground;
//! the SHACL Core and ShEx surfaces are its **projections** (Principle 17), lowered once so
//! they cannot drift. See `design/LOGIC-VALIDATION.md`.
//!
//! No `Eq`/`Hash` derive on the value-carrying types: a numeric bound is `f64` (mirrors
//! [`super::LogicAxiom`]). Identity is the content-addressed `content_key`, folded
//! deterministically with the collections sorted at construction so supply order never
//! matters. A child module reaches the parent's private [`super::SEP`] separator and
//! [`super::opt_axis_key`] signed-zero helper directly, so the key style matches the rest
//! of the IR verbatim.

use super::{NodeKind, SEP, opt_axis_key};
use gmeow_errors::Diag;

/// Length-prefix a free-form fragment so field boundaries can never collide when fragments are
/// concatenated into a content key: `{predicate:"a=b", value:"c"}` and
/// `{predicate:"a", value:"b=c"}` MUST fold to distinct keys, not the same `a=b=c`.
fn key_field(s: &str) -> String {
    format!("{}:{s}", s.len())
}

/// Concatenate already-formatted fragments unambiguously — a count prefix plus every fragment
/// length-prefixed — so neither the element count nor any element boundary can be forged by a
/// value that happens to contain the delimiter.
fn key_list<I: IntoIterator<Item = String>>(items: I) -> String {
    let items: Vec<String> = items.into_iter().collect();
    let body: String = items.iter().map(|s| key_field(s)).collect();
    format!("{}[{body}]", items.len())
}

/// The focus-node selector of a [`ValidationShapeIr`].
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ShapeTarget {
    /// Focus nodes are instances of this class IRI (`sh:targetClass`).
    Class(String),
    /// Focus nodes are the SUBJECTS of a predicate (`sh:targetSubjectsOf`): the closed-world
    /// reading of an `rdfs:domain P C` axiom (every subject of `P` must satisfy the shape).
    SubjectsOf(String),
    /// Focus nodes are the OBJECTS of a predicate (`sh:targetObjectsOf`): the closed-world
    /// reading of an `rdfs:range P C` axiom (every object of `P` must satisfy the shape).
    ObjectsOf(String),
    /// Focus nodes are selected by a required value on a predicate (projected to an
    /// `sh:SPARQLTarget`): the discriminating predicate IRI and the value IRI it must carry.
    ValueKeyed {
        /// The discriminating predicate IRI.
        predicate: String,
        /// The value IRI a focus node must carry on `predicate`.
        value: String,
    },
    /// Focus nodes are DIRECT instances of a class — typed the class but NOT also typed a proper
    /// subclass of it (projected to an `sh:SPARQLTarget` with a subclass-excluding
    /// `FILTER NOT EXISTS`). This is the subclass-refining reading a bare `sh:targetClass` cannot
    /// hold: a node typed both the class and a more-specific subclass is validated by the
    /// subclass's own shape, not the base one.
    DirectClass(String),
    /// Focus nodes are selected by a raw `SELECT ?this WHERE { … }` query (projected verbatim to an
    /// `sh:SPARQLTarget`). The catch-all target for a constraint whose focus set has no class /
    /// domain / range form — e.g. "any node carrying a predicate under a forbidden namespace". The
    /// string is the whole select body (binding `?this`).
    Sparql(String),
}

impl ShapeTarget {
    /// A deterministic content-key fragment (variant-tagged so the variants never collide).
    /// `pub(crate)` so the sibling [`super::ConstraintIr`] can fold a target into its own
    /// content key the same way [`ValidationShapeIr`] does.
    pub(crate) fn content_key(&self) -> String {
        match self {
            ShapeTarget::Class(c) => format!("class={}", key_field(c)),
            ShapeTarget::SubjectsOf(p) => format!("subjectsof={}", key_field(p)),
            ShapeTarget::ObjectsOf(p) => format!("objectsof={}", key_field(p)),
            ShapeTarget::ValueKeyed { predicate, value } => {
                format!("valuekeyed={}{}", key_field(predicate), key_field(value))
            }
            ShapeTarget::DirectClass(c) => format!("directclass={}", key_field(c)),
            ShapeTarget::Sparql(s) => format!("sparqltarget={}", key_field(s)),
        }
    }

    /// The enforcement-key fragment for a target — identical to [`Self::content_key`] (a
    /// target's identity IS its enforcement content: it fully determines the focus-node
    /// selection). Exposed to the crate so the shape-subsumption projection can fold the
    /// target into its enforcement key without re-deriving the tagging.
    pub(crate) fn enforcement_key(&self) -> String {
        self.content_key()
    }
}

/// The `sh:nodeKind` vocabulary (verbatim SHACL local names).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ShaclNodeKind {
    /// `sh:IRI`.
    Iri,
    /// `sh:Literal`.
    Literal,
    /// `sh:BlankNode`.
    BlankNode,
    /// `sh:IRIOrLiteral`.
    IriOrLiteral,
    /// `sh:BlankNodeOrIRI`.
    BlankNodeOrIri,
    /// `sh:BlankNodeOrLiteral`.
    BlankNodeOrLiteral,
}

impl ShaclNodeKind {
    /// The SHACL local name (e.g. `IRI`, `BlankNodeOrIRI`).
    pub fn as_str(&self) -> &'static str {
        match self {
            ShaclNodeKind::Iri => "IRI",
            ShaclNodeKind::Literal => "Literal",
            ShaclNodeKind::BlankNode => "BlankNode",
            ShaclNodeKind::IriOrLiteral => "IRIOrLiteral",
            ShaclNodeKind::BlankNodeOrIri => "BlankNodeOrIRI",
            ShaclNodeKind::BlankNodeOrLiteral => "BlankNodeOrLiteral",
        }
    }
}

/// The `sh:severity` vocabulary (verbatim SHACL local names). Carried per property shape so
/// a projected surface reproduces the authored severity of a shape a bespoke renderer emitted.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ShaclSeverity {
    /// `sh:Violation` (the SHACL default).
    Violation,
    /// `sh:Warning`.
    Warning,
    /// `sh:Info`.
    Info,
}

impl ShaclSeverity {
    /// The SHACL local name (`Violation`, `Warning`, `Info`).
    pub fn as_str(&self) -> &'static str {
        match self {
            ShaclSeverity::Violation => "Violation",
            ShaclSeverity::Warning => "Warning",
            ShaclSeverity::Info => "Info",
        }
    }

    /// Parse a severity from its SHACL local name (the inverse of [`Self::as_str`]); `None`
    /// for an unrecognized token. The frontend uses this to read an authored
    /// `logic:severity "Violation"|"Warning"|"Info"` literal on a `logic:Constraint`.
    pub fn from_local(s: &str) -> Option<Self> {
        match s {
            "Violation" => Some(ShaclSeverity::Violation),
            "Warning" => Some(ShaclSeverity::Warning),
            "Info" => Some(ShaclSeverity::Info),
            _ => None,
        }
    }
}

/// A single member of a closed value set (`sh:in`): an IRI or a typed/lang literal.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ShapeValue {
    /// An IRI term.
    Iri(String),
    /// A literal term: lexical form with an optional datatype IRI XOR language tag.
    Literal {
        /// The lexical form.
        lexical: String,
        /// The datatype IRI (`None` for a plain / lang-tagged literal).
        datatype: Option<String>,
        /// The language tag (`None` for a plain / typed literal).
        lang: Option<String>,
    },
}

impl ShapeValue {
    /// A deterministic content-key fragment (variant-tagged).
    fn content_key(&self) -> String {
        match self {
            ShapeValue::Iri(i) => format!("iri={}", key_field(i)),
            ShapeValue::Literal {
                lexical,
                datatype,
                lang,
            } => format!(
                "lit={}dt={}lang={}",
                key_field(lexical),
                key_field(datatype.as_deref().unwrap_or("")),
                key_field(lang.as_deref().unwrap_or("")),
            ),
        }
    }
}

/// Where a constraint component was lifted from — the discriminant that drives the
/// loss-ledger polarity. An OWL restriction is **open-world**, so its closed-world shape
/// reading is a distinct legitimate reading (`logic:ValidationOnly`), never the axiom's
/// meaning — it is never claimed exact. An OPT/ADL-native constraint
/// (`occurrences`/`existence`/magnitude) is natively **closed-world**, so it may be
/// discharged exactly. See `design/LOGIC-VALIDATION.md` ("Where the loss is").
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ConstraintProvenance {
    /// Lifted from an OWL cardinality/restriction axiom (open-world source).
    OwlRestriction,
    /// Lifted from an OPT/ADL native closed-world constraint.
    OptNative,
}

impl ConstraintProvenance {
    /// A stable content-key tag.
    fn as_str(&self) -> &'static str {
        match self {
            ConstraintProvenance::OwlRestriction => "owl",
            ConstraintProvenance::OptNative => "opt",
        }
    }
}

/// A value-level constraint component on a property shape. The **closed** sum covers the
/// SHACL Core / ShEx-expressible fragment plus the ADL2/OPT constraint node kinds.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConstraintComponent {
    /// A numeric interval. Bounds are optional (half-open intervals are admissible);
    /// each `*_inclusive` selects the SHACL facet
    /// (`sh:minInclusive`/`sh:minExclusive`, `sh:maxInclusive`/`sh:maxExclusive`).
    NumericRange {
        /// Lower bound (`None` ⇒ unbounded below).
        min: Option<f64>,
        /// Upper bound (`None` ⇒ unbounded above).
        max: Option<f64>,
        /// Whether the lower bound is inclusive (`sh:minInclusive` vs `sh:minExclusive`).
        min_inclusive: bool,
        /// Whether the upper bound is inclusive (`sh:maxInclusive` vs `sh:maxExclusive`).
        max_inclusive: bool,
    },
    /// A numeric interval over an openEHR `C_DV_QUANTITY.precision` decimal-place-count, kept
    /// distinct from [`ConstraintComponent::NumericRange`] so a precision satellite never aliases
    /// the magnitude range (which would trip the OPT recovery ambiguity guard). Projected to the
    /// same `sh:minInclusive`/`sh:maxInclusive` numeric facets — it is NOT lossy under projection.
    PrecisionRange {
        /// Lower bound (`None` ⇒ unbounded below).
        min: Option<f64>,
        /// Upper bound (`None` ⇒ unbounded above).
        max: Option<f64>,
        /// Whether the lower bound is inclusive (`sh:minInclusive` vs `sh:minExclusive`).
        min_inclusive: bool,
        /// Whether the upper bound is inclusive (`sh:maxInclusive` vs `sh:maxExclusive`).
        max_inclusive: bool,
    },
    /// A datatype constraint (`sh:datatype`): the datatype IRI.
    Datatype(String),
    /// A class-membership constraint (`sh:class`): values must be instances of this class
    /// IRI. The closed-world reading of an OWL `someValuesFrom` / `allValuesFrom` restriction.
    Class(String),
    /// A node-kind constraint (`sh:nodeKind`).
    NodeKindShacl(ShaclNodeKind),
    /// A closed value set (`sh:in`), sorted at construction.
    In(Vec<ShapeValue>),
    /// A regular-expression pattern (`sh:pattern`) with optional flags (`sh:flags`).
    /// Lossy: the regex dialect residue is recorded in the loss ledger.
    Pattern {
        /// The regular expression.
        regex: String,
        /// Optional SHACL `sh:flags` string.
        flags: Option<String>,
    },
    /// A minimum string length (`sh:minLength`).
    MinLength(u32),
    /// A maximum string length (`sh:maxLength`).
    MaxLength(u32),
    /// A language-tag allow-list (`sh:languageIn`), sorted at construction.
    LanguageIn(Vec<String>),
    /// A datetime interval over `xsd:dateTime` lexical bounds — the temporal peer of
    /// [`ConstraintComponent::NumericRange`].
    DateTimeRange {
        /// Lower bound lexical (`None` ⇒ unbounded below).
        min: Option<String>,
        /// Upper bound lexical (`None` ⇒ unbounded above).
        max: Option<String>,
        /// Whether the lower bound is inclusive.
        min_inclusive: bool,
        /// Whether the upper bound is inclusive.
        max_inclusive: bool,
    },
    /// An external terminology binding (an ADL2 `term_binding` / `C_TERMINOLOGY_CODE`).
    /// Lossy: an external terminology has no faithful closed shape form, so the reference is
    /// carried and flagged in the loss ledger.
    TerminologyBinding {
        /// The terminology identifier (e.g. `SNOMED-CT`, `openehr`).
        terminology_id: String,
        /// The bound codes, sorted at construction.
        codes: Vec<String>,
    },
    /// An openEHR `C_DV_ORDINAL` value set: (ordinal integer, coded-symbol IRI) pairs, sorted at
    /// construction. SHACL Core / ShEx can express the coded symbols as an `sh:in`, but NOT the
    /// ordinal integer values or their ordering — that residue is recorded in the loss ledger.
    OrdinalSet {
        /// The (ordinal integer, coded-symbol IRI) pairs, sorted at construction.
        pairs: Vec<(i64, String)>,
    },
    /// An openEHR `C_DATE_TIME` validity pattern (e.g. `yyyy-mm-ddTHH:MM:SS`) — the required
    /// datetime precision/format. Distinct from a regex [`ConstraintComponent::Pattern`] so
    /// recovery does not collapse it to a string pattern; projected to `sh:pattern` on the
    /// lexical (lossy: an openEHR validity pattern is not an XPath regex).
    DateTimePattern(String),
    /// A fixed required value (`sh:hasValue`): the closed-world reading of an `owl:hasValue`
    /// restriction — every focus node must have this exact value on the path.
    HasValue(ShapeValue),
    /// A qualified value-shape constraint (`sh:qualifiedValueShape [ … ]` with
    /// `sh:qualifiedMinCount`/`sh:qualifiedMaxCount`): the faithful closed-world reading of an
    /// `owl:someValuesFrom` (min 1) and of an `owl:onClass` + `owl:qualifiedCardinality`
    /// restriction — a count of values that additionally satisfy an inner shape, NOT a plain
    /// count over all values. The inner shape is a small set of components (typically one
    /// `Class`/`Datatype`/`NodeKindShacl`), sorted at construction.
    QualifiedValueShape {
        /// The inner value-shape the counted values must satisfy.
        shape: Vec<ConstraintComponent>,
        /// `sh:qualifiedMinCount` (`None` ⇒ unconstrained below).
        min: Option<u32>,
        /// `sh:qualifiedMaxCount` (`None` ⇒ unconstrained above).
        max: Option<u32>,
    },
    /// A negated constraint (`sh:not [ … ]`): the closed-world reading of `owl:disjointWith`
    /// (`sh:not [ sh:class D ]`), `owl:complementOf`, and each pair of a named
    /// `owl:AllDisjointClasses`. The inner component is what a focus node must NOT satisfy.
    Not(Box<ConstraintComponent>),
    /// A disjunction (`sh:or ( [ … ] [ … ] )`): the closed-world reading of an `owl:unionOf`
    /// class expression — a focus value must satisfy AT LEAST ONE branch. Each element is one
    /// branch (rendered as its own `[ … ]` shape block); branches sorted at construction.
    Or(Vec<ConstraintComponent>),
    /// An exclusive disjunction (`sh:xone ( [ … ] [ … ] )`): the closed-world reading of an
    /// `owl:disjointUnionOf` class expression — a focus value must satisfy EXACTLY ONE branch.
    /// Each element is one branch; branches sorted at construction.
    Xone(Vec<ConstraintComponent>),
    /// A NODE-level disjunction over required property paths
    /// (`sh:or ( [ sh:path P1 ; sh:minCount 1 ] [ sh:path P2 ; sh:minCount 1 ] … )`): the focus
    /// node must carry AT LEAST ONE of the alternative predicates. The closed-world reading of a
    /// class-level `rdfs:subClassOf [ owl:unionOf ( [ owl:onProperty P1 ; owl:someValuesFrom
    /// owl:Thing ] … ) ]` axiom (an either-of-these-properties existence obligation). Unlike
    /// [`ConstraintComponent::Or`] — whose branches constrain one property's VALUES — each branch
    /// here names a whole property path required with `sh:minCount 1`. Paths sorted + deduped at
    /// construction. Meaningful only in a shape's node-level component list.
    OrProperties(Vec<String>),
    /// A per-property unique-language facet (`sh:uniqueLang true`): no two language-tagged literal
    /// values on the path may share a language tag. The closed-world grounding of the
    /// localizable-prose convention (a `logic:UniqueLangConstraint` sugar record). Faithful in
    /// SHACL Core; it has NO ShEx form (a disclosed ShEx drop, carried in the loss ledger).
    UniqueLang,
}

impl ConstraintComponent {
    /// Whether this component is inherently lossy under any shape projection — a pattern
    /// (regex-dialect residue) or a terminology binding (external terminology). Used by the
    /// derivation/ledger so a lossy component is never claimed exact.
    pub fn is_lossy(&self) -> bool {
        match self {
            ConstraintComponent::Pattern { .. }
            | ConstraintComponent::TerminologyBinding { .. }
            | ConstraintComponent::OrdinalSet { .. }
            | ConstraintComponent::DateTimePattern(_) => true,
            // A nested constraint is lossy iff its inner shape is.
            ConstraintComponent::QualifiedValueShape { shape, .. } => {
                shape.iter().any(ConstraintComponent::is_lossy)
            }
            ConstraintComponent::Not(inner) => inner.is_lossy(),
            // A disjunction is lossy iff some branch is (the wrappers are faithful in SHACL Core).
            ConstraintComponent::Or(branches) | ConstraintComponent::Xone(branches) => {
                branches.iter().any(ConstraintComponent::is_lossy)
            }
            _ => false,
        }
    }

    /// Canonicalize the component's inner collections — sort the `In` value-set members, the
    /// `LanguageIn` tags, and the `TerminologyBinding` codes — so supply order never affects
    /// identity, and reject an `In` literal that illegally carries BOTH a datatype and a
    /// language tag (the [`ShapeValue::Literal`] invariant is datatype XOR lang). Called at
    /// construction so the stored form — not just the key — is canonical.
    fn normalize(&mut self) -> gmeow_errors::Result<()> {
        match self {
            ConstraintComponent::In(vs) => {
                for v in vs.iter() {
                    if let ShapeValue::Literal {
                        datatype: Some(_),
                        lang: Some(_),
                        ..
                    } = v
                    {
                        return Err(Diag::of_kind(crate::error::Validation {
                            detail: "ConstraintComponent::In: a value-set literal may carry a \
                                    datatype XOR a language tag, never both"
                                .to_owned(),
                        }));
                    }
                }
                // Sort by the type's own derived `Ord`, NOT by `content_key`: the content key is
                // length-prefixed for unambiguous folding (`key_field`), which does not agree
                // with plain lexical order once members differ in byte length (e.g. two IRIs of
                // lengths 78 and 85 would fold-sort as "78:…" < "85:…", which happens to agree
                // here, but a 9-vs-10-length pair would not). The OPT reader canonicalizes its
                // own value sets with a plain `Vec<String>::sort()`; using the same natural order
                // here is what keeps the two sides content-identical after a round trip.
                vs.sort();
            }
            ConstraintComponent::LanguageIn(langs) => langs.sort(),
            ConstraintComponent::TerminologyBinding { codes, .. } => codes.sort(),
            ConstraintComponent::OrdinalSet { pairs } => pairs.sort(),
            ConstraintComponent::HasValue(ShapeValue::Literal {
                datatype: Some(_),
                lang: Some(_),
                ..
            }) => {
                return Err(Diag::of_kind(crate::error::Validation {
                    detail: "ConstraintComponent::HasValue: a literal value may carry a \
                            datatype XOR a language tag, never both"
                        .to_owned(),
                }));
            }
            ConstraintComponent::QualifiedValueShape { shape, .. } => {
                for c in shape.iter_mut() {
                    c.normalize()?;
                }
                shape.sort_by_cached_key(ConstraintComponent::content_key);
            }
            ConstraintComponent::Not(inner) => inner.normalize()?,
            ConstraintComponent::Or(branches) | ConstraintComponent::Xone(branches) => {
                for b in branches.iter_mut() {
                    b.normalize()?;
                }
                branches.sort_by_cached_key(ConstraintComponent::content_key);
            }
            ConstraintComponent::OrProperties(paths) => {
                paths.sort();
                paths.dedup();
                if paths.len() < 2 {
                    return Err(Diag::of_kind(crate::error::Validation {
                        detail: "ConstraintComponent::OrProperties: a property disjunction needs \
                                at least two distinct alternative paths"
                            .to_owned(),
                    }));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A deterministic, variant-tagged content-key fragment. Numeric bounds route through
    /// [`super::opt_axis_key`] so `-0.0` and `0.0` fold identically (signed-zero determinism);
    /// every free-form string routes through [`key_field`] / [`key_list`] so distinct IR states
    /// can never collapse to the same key.
    fn content_key(&self) -> String {
        match self {
            ConstraintComponent::NumericRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => format!(
                "range{SEP}min={}{SEP}max={}{SEP}mincl={min_inclusive}{SEP}maxcl={max_inclusive}",
                opt_axis_key(*min),
                opt_axis_key(*max),
            ),
            ConstraintComponent::PrecisionRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => format!(
                "precisionrange{SEP}min={}{SEP}max={}{SEP}mincl={min_inclusive}{SEP}maxcl={max_inclusive}",
                opt_axis_key(*min),
                opt_axis_key(*max),
            ),
            ConstraintComponent::Datatype(d) => format!("datatype={}", key_field(d)),
            ConstraintComponent::Class(c) => format!("class={}", key_field(c)),
            ConstraintComponent::NodeKindShacl(k) => format!("nodekind={}", k.as_str()),
            ConstraintComponent::In(vs) => {
                format!("in={}", key_list(vs.iter().map(ShapeValue::content_key)))
            }
            ConstraintComponent::Pattern { regex, flags } => format!(
                "pattern={}flags={}",
                key_field(regex),
                key_field(flags.as_deref().unwrap_or("")),
            ),
            ConstraintComponent::MinLength(n) => format!("minlength={n}"),
            ConstraintComponent::MaxLength(n) => format!("maxlength={n}"),
            ConstraintComponent::LanguageIn(langs) => {
                format!("languagein={}", key_list(langs.iter().cloned()))
            }
            ConstraintComponent::DateTimeRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => format!(
                "datetime{SEP}min={}{SEP}max={}{SEP}mincl={min_inclusive}{SEP}maxcl={max_inclusive}",
                key_field(min.as_deref().unwrap_or("")),
                key_field(max.as_deref().unwrap_or("")),
            ),
            ConstraintComponent::TerminologyBinding {
                terminology_id,
                codes,
            } => format!(
                "termbind={}codes={}",
                key_field(terminology_id),
                key_list(codes.iter().cloned()),
            ),
            ConstraintComponent::OrdinalSet { pairs } => format!(
                "ordinalset={}",
                key_list(pairs.iter().map(|(v, c)| format!(
                    "{}{}",
                    key_field(&v.to_string()),
                    key_field(c)
                ))),
            ),
            ConstraintComponent::DateTimePattern(p) => {
                format!("datetimepattern={}", key_field(p))
            }
            ConstraintComponent::HasValue(v) => format!("hasvalue={}", v.content_key()),
            ConstraintComponent::QualifiedValueShape { shape, min, max } => format!(
                "qvs={}{SEP}qmin={}{SEP}qmax={}",
                key_list(shape.iter().map(ConstraintComponent::content_key)),
                min.map(|n| n.to_string()).unwrap_or_default(),
                max.map(|n| n.to_string()).unwrap_or_default(),
            ),
            ConstraintComponent::Not(inner) => {
                format!("not={}", key_field(&inner.content_key()))
            }
            ConstraintComponent::Or(branches) => {
                format!(
                    "or={}",
                    key_list(branches.iter().map(ConstraintComponent::content_key))
                )
            }
            ConstraintComponent::Xone(branches) => {
                format!(
                    "xone={}",
                    key_list(branches.iter().map(ConstraintComponent::content_key))
                )
            }
            ConstraintComponent::OrProperties(paths) => {
                format!("orprops={}", key_list(paths.iter().cloned()))
            }
            ConstraintComponent::UniqueLang => "uniquelang".to_owned(),
        }
    }

    /// The enforcement-key fragment for a component — identical to [`Self::content_key`]: a
    /// value-level component carries no presentation/provenance tail, so its identity IS its
    /// enforcement content (it fully determines which values a validator flags). Exposed to
    /// the crate so the shape-subsumption projection folds it without re-deriving the tagging.
    pub(crate) fn enforcement_key(&self) -> String {
        self.content_key()
    }
}

/// A property shape: the closed-world constraints on the values reachable via one
/// predicate path (`sh:PropertyShape`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PropertyConstraintIr {
    /// The predicate IRI this property shape constrains (`sh:path`).
    pub path: String,
    /// Closed-world minimum cardinality (`sh:minCount`); `None` ⇒ unconstrained.
    pub min_count: Option<u32>,
    /// Closed-world maximum cardinality (`sh:maxCount`); `None` ⇒ unconstrained.
    pub max_count: Option<u32>,
    /// Provenance of the cardinality constraint (drives ledger polarity). Only meaningful
    /// when `min_count`/`max_count` is set; `None` when there is no cardinality constraint.
    pub cardinality_provenance: Option<ConstraintProvenance>,
    /// Value-level components (datatype, range, pattern, value set, …), sorted at
    /// construction.
    pub components: Vec<ConstraintComponent>,
    /// Whether the path is inverted (`sh:path [ sh:inversePath P ]`): the closed-world reading
    /// of `owl:InverseFunctionalProperty` (each object has ≤1 subject via `P`). Default `false`.
    pub inverse: bool,
    /// The IRI of a node shape validating the RDF-1.2 reifier of each `focus`→`path`→`value`
    /// statement (`sh:reifierShape`); `None` ⇒ no reifier condition. Lives on the property shape
    /// (not the node shape) because the SHACL 1.2 reifier component is keyed to the path's
    /// statement — the native engine reads it only from a single-predicate property shape.
    pub reifier_shape: Option<String>,
    /// Whether each matching `focus`→`path`→`value` statement must carry ≥1 RDF-1.2 reifier
    /// (`sh:reificationRequired`). Default `false`.
    pub reification_required: bool,
    /// The `sh:severity` of a violation (`None` ⇒ the SHACL default `sh:Violation`, emitted
    /// implicitly). Carried so a projected surface reproduces a bespoke renderer's severity.
    pub severity: Option<ShaclSeverity>,
    /// The `sh:message` attached to a violation (`None` ⇒ none). Carried for byte-parity with
    /// the bespoke frame/result renderers and for better validation-failure UX.
    pub message: Option<String>,
}

impl PropertyConstraintIr {
    /// Construct a property shape, sorting `components` into content-key order so supply
    /// order never affects identity, and validating the path is a non-empty IRI.
    pub fn new(
        path: impl Into<String>,
        min_count: Option<u32>,
        max_count: Option<u32>,
        cardinality_provenance: Option<ConstraintProvenance>,
        components: Vec<ConstraintComponent>,
    ) -> gmeow_errors::Result<Self> {
        let path = path.into();
        if path.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: "PropertyConstraintIr.path must be a non-empty IRI string".to_owned(),
            }));
        }
        if let (Some(lo), Some(hi)) = (min_count, max_count)
            && lo > hi
        {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: format!(
                    "PropertyConstraintIr min_count ({lo}) must not exceed max_count ({hi})"
                ),
            }));
        }
        // A provenance without a cardinality is a determinism hazard (it would perturb the
        // key while claiming nothing); a cardinality without a provenance leaves the ledger
        // polarity undecidable. Bind the two together.
        let has_card = min_count.is_some() || max_count.is_some();
        if has_card != cardinality_provenance.is_some() {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: "PropertyConstraintIr.cardinality_provenance must be Some iff a \
                 min_count/max_count is present"
                    .to_owned(),
            }));
        }
        let mut components = components;
        for component in &mut components {
            component.normalize()?;
        }
        components.sort_by_cached_key(ConstraintComponent::content_key);
        Ok(Self {
            path,
            min_count,
            max_count,
            cardinality_provenance,
            components,
            inverse: false,
            reifier_shape: None,
            reification_required: false,
            severity: None,
            message: None,
        })
    }

    /// Attach the RDF-1.2 reifier condition (`sh:reifierShape` / `sh:reificationRequired`) to this
    /// property shape. `reifier_shape` names the node shape each statement's reifier must conform to
    /// (`None` ⇒ no shape constraint); `reification_required` demands ≥1 reifier per statement.
    /// Chainable; leaves the content key byte-identical when both are default. Hard-fails on a
    /// no-op (neither set), a blank shape IRI, or an inverse path — the SHACL 1.2 reifier component
    /// is defined only on a single forward-predicate path, so an inverse path would emit a surface
    /// the native engine rejects.
    pub fn with_reifier(
        mut self,
        reifier_shape: Option<String>,
        reification_required: bool,
    ) -> gmeow_errors::Result<Self> {
        if reifier_shape.is_none() && !reification_required {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: "PropertyConstraintIr.with_reifier: at least one of reifier_shape / \
                 reification_required must be set (a no-op reifier condition is a determinism \
                 hazard)"
                    .to_owned(),
            }));
        }
        if let Some(rs) = &reifier_shape
            && rs.trim().is_empty()
        {
            return Err(Diag::of_kind(crate::error::Validation {
                detail:
                    "PropertyConstraintIr.with_reifier: reifier_shape must be a non-empty IRI when \
                 present; pass None to leave it unset"
                        .to_owned(),
            }));
        }
        if self.inverse {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: "PropertyConstraintIr.with_reifier: the SHACL 1.2 reifier component is defined only \
                 on a single forward-predicate path, not an inverse path"
                    .to_owned(),
            }));
        }
        self.reifier_shape = reifier_shape;
        self.reification_required = reification_required;
        Ok(self)
    }

    /// Mark the path inverted (`sh:path [ sh:inversePath P ]`) — the `owl:InverseFunctionalProperty`
    /// reading. Chainable so `new()`'s signature (and every existing caller) is untouched.
    pub fn inverted(mut self) -> Self {
        self.inverse = true;
        self
    }

    /// Attach an `sh:severity`. Chainable; leaves the content key byte-identical when unset.
    pub fn with_severity(mut self, severity: ShaclSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    /// Attach an `sh:message`. Chainable; leaves the content key byte-identical when unset.
    /// A blank message is rejected (a required presentation string that says nothing is a
    /// determinism hazard, not a silent no-op).
    pub fn with_message(mut self, message: impl Into<String>) -> gmeow_errors::Result<Self> {
        let message = message.into();
        if message.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: "PropertyConstraintIr.with_message: message must be non-empty".to_owned(),
            }));
        }
        self.message = Some(message);
        Ok(self)
    }

    /// A deterministic content-key over a FIXED field order. Every free-form field is
    /// length-prefixed and the component list is count-prefixed ([`key_field`] / [`key_list`])
    /// so no path or component value can forge a field boundary. The `inverse`/`severity`/
    /// `message` presentation fields are appended at the TAIL and **only when non-default**, so
    /// a plain property shape folds byte-identically to before these fields existed.
    fn content_key(&self) -> String {
        let comps = key_list(self.components.iter().map(ConstraintComponent::content_key));
        let mut key = format!(
            "path={}{SEP}min={}{SEP}max={}{SEP}prov={}{SEP}comps={comps}",
            key_field(&self.path),
            self.min_count.map(|n| n.to_string()).unwrap_or_default(),
            self.max_count.map(|n| n.to_string()).unwrap_or_default(),
            self.cardinality_provenance
                .map(|p| p.as_str())
                .unwrap_or(""),
        );
        if self.inverse {
            key.push_str(&format!("{SEP}inverse=true"));
        }
        if let Some(rs) = &self.reifier_shape {
            key.push_str(&format!("{SEP}reifier={}", key_field(rs)));
        }
        if self.reification_required {
            key.push_str(&format!("{SEP}reifreq=true"));
        }
        if let Some(sev) = self.severity {
            key.push_str(&format!("{SEP}sev={}", sev.as_str()));
        }
        if let Some(msg) = &self.message {
            key.push_str(&format!("{SEP}msg={}", key_field(msg)));
        }
        key
    }

    /// The enforcement key for a property shape — the subset of [`Self::content_key`] that
    /// determines which focus nodes a validator flags. It carries `path`, `min_count`,
    /// `max_count`, the value-level `components`, and the `inverse` / `reifier_shape` /
    /// `reification_required` enforcement flags, and it OMITS the presentation/provenance tail
    /// (`cardinality_provenance`, `severity`, `message`) — those change the ledger polarity and
    /// the rendered surface, never the findings. Two property shapes with equal enforcement keys
    /// flag exactly the same values on the same path over every graph.
    pub(crate) fn enforcement_key(&self) -> String {
        let comps = key_list(self.components.iter().map(ConstraintComponent::content_key));
        let mut key = format!(
            "path={}{SEP}min={}{SEP}max={}{SEP}comps={comps}",
            key_field(&self.path),
            self.min_count.map(|n| n.to_string()).unwrap_or_default(),
            self.max_count.map(|n| n.to_string()).unwrap_or_default(),
        );
        if self.inverse {
            key.push_str(&format!("{SEP}inverse=true"));
        }
        if let Some(rs) = &self.reifier_shape {
            key.push_str(&format!("{SEP}reifier={}", key_field(rs)));
        }
        if self.reification_required {
            key.push_str(&format!("{SEP}reifreq=true"));
        }
        key
    }
}

/// A named closed-world validation shape (`logic:ValidationShape`): the canonical form the
/// SHACL Core and ShEx surfaces project from. Identity is the content-addressed
/// [`Self::content_key`]; the `iri` is the sort key.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ValidationShapeIr {
    /// IRI string of the shape individual (identity / sort key).
    pub iri: String,
    /// The focus-node selector (`sh:targetClass` or an `sh:SPARQLTarget`).
    pub target: ShapeTarget,
    /// The per-path property shapes, sorted by content key at construction.
    pub properties: Vec<PropertyConstraintIr>,
    /// Always [`NodeKind::ValidationShape`] — the constructor hard-pins it. Carried so the
    /// node-kind discriminant is uniform with the axiom/rule surfaces and this once-dead
    /// enum variant is now genuinely constructed.
    pub node_kind: NodeKind,
    /// The standpoint IRI a shape holds under, when standpoint-indexed (`None` ⇒ universal).
    pub standpoint: Option<String>,
    /// Focus-NODE-level constraints (not on a property path): the `sh:class` / `sh:datatype` /
    /// `sh:nodeKind` / `sh:not` a focus node itself must satisfy. Populated by the domain /
    /// range / disjointness readings (`sh:targetSubjectsOf`/`ObjectsOf` + a node-level class).
    /// Empty by default so existing shapes' content keys are byte-unchanged. Sorted at
    /// construction.
    pub node_components: Vec<ConstraintComponent>,
    /// The `rdfs:label` of the shape (`None` ⇒ none). Carried for byte-parity with the bespoke
    /// frame renderer and for human-readable shapes. Empty/`None` by default.
    pub label: Option<String>,
    /// Typed conformance failure raised by this validation shape. Annotation-level metadata:
    /// projected to `gmeow:enforcesFailureClass`, but excluded from semantic identity.
    pub failure_class: Option<String>,
}

impl ValidationShapeIr {
    /// Construct a validation shape, hard-pinning `node_kind` to
    /// [`NodeKind::ValidationShape`] and sorting `properties` into canonical order so supply
    /// order never affects identity. Validates the IRI, target, and standpoint fields.
    pub fn new(
        iri: impl Into<String>,
        target: ShapeTarget,
        properties: Vec<PropertyConstraintIr>,
        standpoint: Option<String>,
    ) -> gmeow_errors::Result<Self> {
        let iri = iri.into();
        if iri.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: "ValidationShapeIr.iri must be a non-empty IRI string".to_owned(),
            }));
        }
        match &target {
            ShapeTarget::Class(c) if c.trim().is_empty() => {
                return Err(Diag::of_kind(crate::error::Validation {
                    detail: "ValidationShapeIr target class must be a non-empty IRI".to_owned(),
                }));
            }
            ShapeTarget::SubjectsOf(p) if p.trim().is_empty() => {
                return Err(Diag::of_kind(crate::error::Validation {
                    detail:
                        "ValidationShapeIr subjects-of target must be a non-empty predicate IRI"
                            .to_owned(),
                }));
            }
            ShapeTarget::ObjectsOf(p) if p.trim().is_empty() => {
                return Err(Diag::of_kind(crate::error::Validation {
                    detail: "ValidationShapeIr objects-of target must be a non-empty predicate IRI"
                        .to_owned(),
                }));
            }
            ShapeTarget::ValueKeyed { predicate, value }
                if predicate.trim().is_empty() || value.trim().is_empty() =>
            {
                return Err(Diag::of_kind(crate::error::Validation {
                    detail:
                        "ValidationShapeIr value-keyed target needs a non-empty predicate and value"
                            .to_owned(),
                }));
            }
            ShapeTarget::DirectClass(c) if c.trim().is_empty() => {
                return Err(Diag::of_kind(crate::error::Validation {
                    detail: "ValidationShapeIr direct-instance target must be a non-empty IRI"
                        .to_owned(),
                }));
            }
            ShapeTarget::Sparql(s) if s.trim().is_empty() => {
                return Err(Diag::of_kind(crate::error::Validation {
                    detail: "ValidationShapeIr sparql target must be a non-empty SELECT body"
                        .to_owned(),
                }));
            }
            _ => {}
        }
        if let Some(sp) = &standpoint
            && sp.trim().is_empty()
        {
            return Err(Diag::of_kind(crate::error::Validation {
                detail:
                    "ValidationShapeIr.standpoint must be a non-empty IRI when present; pass None \
                     to leave it unset"
                        .to_owned(),
            }));
        }
        let mut properties = properties;
        properties.sort_by_cached_key(PropertyConstraintIr::content_key);
        Ok(Self {
            iri,
            target,
            properties,
            node_kind: NodeKind::ValidationShape,
            standpoint,
            node_components: Vec::new(),
            label: None,
            failure_class: None,
        })
    }

    /// Attach focus-node-level constraints (`sh:class`/`sh:datatype`/`sh:nodeKind`/`sh:not` on
    /// the focus node itself, not a path) — the domain / range / disjointness node conditions.
    /// Chainable; normalizes + sorts the components so supply order never affects identity, and
    /// leaves the content key byte-identical when the list is empty.
    pub fn with_node_components(
        mut self,
        node_components: Vec<ConstraintComponent>,
    ) -> gmeow_errors::Result<Self> {
        let mut node_components = node_components;
        for component in &mut node_components {
            component.normalize()?;
        }
        node_components.sort_by_cached_key(ConstraintComponent::content_key);
        self.node_components = node_components;
        Ok(self)
    }

    /// Attach an `rdfs:label`. Chainable; leaves the content key byte-identical when unset. A
    /// blank label is rejected (a required presentation string that says nothing is a
    /// determinism hazard, not a silent no-op).
    pub fn with_label(mut self, label: impl Into<String>) -> gmeow_errors::Result<Self> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: "ValidationShapeIr.with_label: label must be non-empty".to_owned(),
            }));
        }
        self.label = Some(label);
        Ok(self)
    }

    /// Attach the unique typed conformance-failure class projected with this shape.
    pub fn with_failure_class(
        mut self,
        failure_class: impl Into<String>,
    ) -> gmeow_errors::Result<Self> {
        let failure_class = failure_class.into();
        if failure_class.trim().is_empty() {
            return Err(Diag::of_kind(crate::error::Validation {
                detail:
                    "ValidationShapeIr.with_failure_class: failure class must be a non-empty IRI"
                        .to_owned(),
            }));
        }
        if self.failure_class.is_some() {
            return Err(Diag::of_kind(crate::error::Validation {
                detail: format!(
                    "ValidationShapeIr {} has duplicate gmeow:enforcesFailureClass metadata",
                    self.iri
                ),
            }));
        }
        self.failure_class = Some(failure_class);
        Ok(self)
    }

    /// Stable sort key for canonical ordering — the shape IRI is unique.
    pub fn sort_key(&self) -> String {
        self.iri.clone()
    }

    /// Whether any component of any property is inherently lossy (pattern / terminology
    /// binding). A shape with a lossy component cannot claim an exact round-trip.
    pub fn has_lossy_component(&self) -> bool {
        self.properties
            .iter()
            .any(|p| p.components.iter().any(ConstraintComponent::is_lossy))
            || self
                .node_components
                .iter()
                .any(ConstraintComponent::is_lossy)
    }

    /// A deterministic full-content key for canonical equality. Public to the crate so
    /// `LogicProgram::canonical_key` can fold it into the program key at the fixed tail. The
    /// `node_components`/`label` fields are appended at the TAIL and **only when non-default**,
    /// so a shape without them folds byte-identically to before these fields existed (the
    /// content-addressed `LogicProgram` key cannot drift for the historical shape corpus).
    pub(crate) fn content_key(&self) -> String {
        let props = key_list(
            self.properties
                .iter()
                .map(PropertyConstraintIr::content_key),
        );
        let mut key = format!(
            "{}{SEP}kind={}{SEP}{}{SEP}sp={}{SEP}PROPS={props}",
            key_field(&self.iri),
            self.node_kind.as_str(),
            self.target.content_key(),
            key_field(self.standpoint.as_deref().unwrap_or("")),
        );
        if !self.node_components.is_empty() {
            key.push_str(&format!(
                "{SEP}NODECOMPS={}",
                key_list(
                    self.node_components
                        .iter()
                        .map(ConstraintComponent::content_key)
                )
            ));
        }
        if let Some(label) = &self.label {
            key.push_str(&format!("{SEP}label={}", key_field(label)));
        }
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_class_is_unique_annotation_metadata() {
        let bare = ValidationShapeIr::new(
            "https://ex/Shape",
            ShapeTarget::Class("https://ex/C".into()),
            vec![],
            None,
        )
        .unwrap();
        let keyed = bare
            .clone()
            .with_failure_class("https://ex/Failure")
            .unwrap();
        assert_eq!(bare.content_key(), keyed.content_key());
        let err = keyed
            .with_failure_class("https://ex/OtherFailure")
            .unwrap_err();
        assert!(err.message().contains("duplicate"));
    }

    #[test]
    fn value_set_members_are_order_independent() {
        let members = |a: &str, b: &str| {
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                None,
                None,
                vec![ConstraintComponent::In(vec![
                    ShapeValue::Iri(a.to_owned()),
                    ShapeValue::Iri(b.to_owned()),
                ])],
            )
            .unwrap()
        };
        let ab = members("https://ex/a", "https://ex/b");
        let ba = members("https://ex/b", "https://ex/a");
        assert_eq!(
            ab.content_key(),
            ba.content_key(),
            "value-set member order must not affect identity"
        );
        assert_eq!(ab, ba, "normalized components must be structurally equal");
    }

    #[test]
    fn value_set_literal_with_datatype_and_lang_is_rejected() {
        let err = PropertyConstraintIr::new(
            "https://ex/p",
            None,
            None,
            None,
            vec![ConstraintComponent::In(vec![ShapeValue::Literal {
                lexical: "x".into(),
                datatype: Some("https://ex/dt".into()),
                lang: Some("en".into()),
            }])],
        )
        .unwrap_err();
        assert!(err.message().contains("XOR"), "got: {err}");
    }

    #[test]
    fn value_keyed_target_key_is_unambiguous() {
        // The classic delimiter collision: `"a=b" + "c"` and `"a" + "b=c"` must fold to DISTINCT
        // keys, never the same `a=b=c`.
        let x = ShapeTarget::ValueKeyed {
            predicate: "a=b".into(),
            value: "c".into(),
        };
        let y = ShapeTarget::ValueKeyed {
            predicate: "a".into(),
            value: "b=c".into(),
        };
        assert_ne!(
            x.content_key(),
            y.content_key(),
            "distinct value-keyed targets must not share a content key"
        );
    }

    #[test]
    fn default_presentation_fields_leave_property_key_byte_identical() {
        // The type-migration / new-field hazard the empty-attach test does NOT catch: a plain
        // property shape (no inverse path, no severity, no message) must fold to the SAME bytes
        // the key produced before those fields existed — i.e. the tail markers must be ABSENT.
        let p = PropertyConstraintIr::new(
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/C".into())],
        )
        .unwrap();
        let key = p.content_key();
        assert!(
            !key.contains("inverse="),
            "default key must not carry inverse: {key}"
        );
        assert!(
            !key.contains("sev="),
            "default key must not carry severity: {key}"
        );
        assert!(
            !key.contains("msg="),
            "default key must not carry message: {key}"
        );
        // And the presentation setters DO perturb the key (falsifiable).
        assert_ne!(p.content_key(), p.clone().inverted().content_key());
        assert_ne!(
            p.content_key(),
            p.clone()
                .with_severity(ShaclSeverity::Warning)
                .content_key()
        );
    }

    #[test]
    fn default_node_components_and_label_leave_shape_key_byte_identical() {
        // A shape with no node_components and no label must fold to a key with NO NODECOMPS/label
        // tail — the guarantee that the historical shape corpus's content-addressed key cannot
        // drift now that these fields exist.
        let shape = ValidationShapeIr::new(
            "https://ex/S-shape",
            ShapeTarget::Class("https://ex/S".into()),
            vec![
                PropertyConstraintIr::new(
                    "https://ex/p",
                    None,
                    None,
                    None,
                    vec![ConstraintComponent::Class("https://ex/C".into())],
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        let key = shape.content_key();
        assert!(
            key.ends_with(&format!("PROPS={}", {
                key_list(
                    shape
                        .properties
                        .iter()
                        .map(PropertyConstraintIr::content_key),
                )
            })),
            "a plain shape's key must end at PROPS with no NODECOMPS/label tail: {key}"
        );
        assert!(!key.contains("NODECOMPS="));
        assert!(!key.contains("label="));
        // Attaching a node component / label DOES perturb the key.
        let with_nc = shape
            .clone()
            .with_node_components(vec![ConstraintComponent::Class("https://ex/D".into())])
            .unwrap();
        assert_ne!(shape.content_key(), with_nc.content_key());
        assert!(with_nc.content_key().contains("NODECOMPS="));
    }

    #[test]
    fn new_components_round_trip_content_key_and_order_independence() {
        // HasValue, QualifiedValueShape, and Not all fold deterministically, and a qualified
        // value shape's inner components are order-independent.
        let mk = |inner_a: &str, inner_b: &str| {
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                None,
                None,
                vec![
                    ConstraintComponent::HasValue(ShapeValue::Iri("https://ex/v".into())),
                    ConstraintComponent::QualifiedValueShape {
                        shape: vec![
                            ConstraintComponent::Class(inner_a.into()),
                            ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri),
                            ConstraintComponent::Datatype(inner_b.into()),
                        ],
                        min: Some(1),
                        max: None,
                    },
                    ConstraintComponent::Not(Box::new(ConstraintComponent::Class(
                        "https://ex/Disjoint".into(),
                    ))),
                ],
            )
            .unwrap()
        };
        // Inner shape supplied in two different orders → identical key (order-independence).
        let x = mk("https://ex/A", "https://ex/dt");
        let mut reordered = PropertyConstraintIr::new(
            "https://ex/p",
            None,
            None,
            None,
            vec![
                ConstraintComponent::Not(Box::new(ConstraintComponent::Class(
                    "https://ex/Disjoint".into(),
                ))),
                ConstraintComponent::QualifiedValueShape {
                    shape: vec![
                        ConstraintComponent::Datatype("https://ex/dt".into()),
                        ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri),
                        ConstraintComponent::Class("https://ex/A".into()),
                    ],
                    min: Some(1),
                    max: None,
                },
                ConstraintComponent::HasValue(ShapeValue::Iri("https://ex/v".into())),
            ],
        )
        .unwrap();
        // sanity: normalization made reordered structurally equal to x
        reordered.message = None;
        assert_eq!(x.content_key(), reordered.content_key());
        assert_eq!(x, reordered);
    }

    #[test]
    fn has_value_literal_with_datatype_and_lang_is_rejected() {
        let err = PropertyConstraintIr::new(
            "https://ex/p",
            None,
            None,
            None,
            vec![ConstraintComponent::HasValue(ShapeValue::Literal {
                lexical: "x".into(),
                datatype: Some("https://ex/dt".into()),
                lang: Some("en".into()),
            })],
        )
        .unwrap_err();
        assert!(err.message().contains("XOR"), "got: {err}");
    }

    #[test]
    fn new_targets_and_qualified_shape_are_lossy_transparent() {
        // A Not wrapping a lossy inner is lossy; a QualifiedValueShape over lossy inner is lossy.
        let not_pattern = ConstraintComponent::Not(Box::new(ConstraintComponent::Pattern {
            regex: "a".into(),
            flags: None,
        }));
        assert!(not_pattern.is_lossy());
        let qvs_clean = ConstraintComponent::QualifiedValueShape {
            shape: vec![ConstraintComponent::Class("https://ex/C".into())],
            min: Some(1),
            max: None,
        };
        assert!(!qvs_clean.is_lossy());
        // Subjects-of / objects-of targets validate their predicate.
        assert!(
            ValidationShapeIr::new(
                "https://ex/s",
                ShapeTarget::SubjectsOf("  ".into()),
                vec![],
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn or_and_xone_branches_are_order_independent_and_lossy_transparent() {
        // Branch supply order must not affect identity, and a lossy branch makes the whole
        // disjunction lossy (recursion through Or/Xone).
        let mk = |a: &str, b: &str| {
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                None,
                None,
                vec![ConstraintComponent::Or(vec![
                    ConstraintComponent::Class(a.into()),
                    ConstraintComponent::Class(b.into()),
                ])],
            )
            .unwrap()
        };
        assert_eq!(
            mk("https://ex/A", "https://ex/B").content_key(),
            mk("https://ex/B", "https://ex/A").content_key(),
            "Or branch order must not affect identity"
        );
        let clean =
            ConstraintComponent::Xone(vec![ConstraintComponent::Class("https://ex/A".into())]);
        assert!(!clean.is_lossy());
        let lossy = ConstraintComponent::Or(vec![ConstraintComponent::Pattern {
            regex: "^a".into(),
            flags: None,
        }]);
        assert!(
            lossy.is_lossy(),
            "a Pattern branch makes the disjunction lossy"
        );
    }

    #[test]
    fn language_and_terminology_codes_are_sorted_at_construction() {
        let p = PropertyConstraintIr::new(
            "https://ex/p",
            None,
            None,
            None,
            vec![ConstraintComponent::LanguageIn(vec![
                "fr".into(),
                "en".into(),
                "de".into(),
            ])],
        )
        .unwrap();
        match &p.components[0] {
            ConstraintComponent::LanguageIn(langs) => assert_eq!(langs, &["de", "en", "fr"]),
            other => panic!("expected LanguageIn, got {other:?}"),
        }
    }
}
