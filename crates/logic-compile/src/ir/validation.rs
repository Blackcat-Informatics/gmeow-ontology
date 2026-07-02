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

use super::{opt_axis_key, NodeKind, SEP};

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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShapeTarget {
    /// Focus nodes are instances of this class IRI (`sh:targetClass`).
    Class(String),
    /// Focus nodes are selected by a required value on a predicate (projected to an
    /// `sh:SPARQLTarget`): the discriminating predicate IRI and the value IRI it must carry.
    ValueKeyed {
        /// The discriminating predicate IRI.
        predicate: String,
        /// The value IRI a focus node must carry on `predicate`.
        value: String,
    },
}

impl ShapeTarget {
    /// A deterministic content-key fragment (variant-tagged so a `Class` and a
    /// `ValueKeyed` never collide).
    fn content_key(&self) -> String {
        match self {
            ShapeTarget::Class(c) => format!("class={}", key_field(c)),
            ShapeTarget::ValueKeyed { predicate, value } => {
                format!("valuekeyed={}{}", key_field(predicate), key_field(value))
            }
        }
    }
}

/// The `sh:nodeKind` vocabulary (verbatim SHACL local names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// A single member of a closed value set (`sh:in`): an IRI or a typed/lang literal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Clone, PartialEq)]
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
}

impl ConstraintComponent {
    /// Whether this component is inherently lossy under any shape projection — a pattern
    /// (regex-dialect residue) or a terminology binding (external terminology). Used by the
    /// derivation/ledger so a lossy component is never claimed exact.
    pub fn is_lossy(&self) -> bool {
        matches!(
            self,
            ConstraintComponent::Pattern { .. } | ConstraintComponent::TerminologyBinding { .. }
        )
    }

    /// Canonicalize the component's inner collections — sort the `In` value-set members, the
    /// `LanguageIn` tags, and the `TerminologyBinding` codes — so supply order never affects
    /// identity, and reject an `In` literal that illegally carries BOTH a datatype and a
    /// language tag (the [`ShapeValue::Literal`] invariant is datatype XOR lang). Called at
    /// construction so the stored form — not just the key — is canonical.
    fn normalize(&mut self) -> Result<(), String> {
        match self {
            ConstraintComponent::In(vs) => {
                for v in vs.iter() {
                    if let ShapeValue::Literal {
                        datatype: Some(_),
                        lang: Some(_),
                        ..
                    } = v
                    {
                        return Err("ConstraintComponent::In: a value-set literal may carry a \
                                    datatype XOR a language tag, never both"
                            .to_owned());
                    }
                }
                vs.sort_by_cached_key(ShapeValue::content_key);
            }
            ConstraintComponent::LanguageIn(langs) => langs.sort(),
            ConstraintComponent::TerminologyBinding { codes, .. } => codes.sort(),
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
        }
    }
}

/// A property shape: the closed-world constraints on the values reachable via one
/// predicate path (`sh:PropertyShape`).
#[derive(Debug, Clone, PartialEq)]
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
    ) -> Result<Self, String> {
        let path = path.into();
        if path.trim().is_empty() {
            return Err("PropertyConstraintIr.path must be a non-empty IRI string".to_owned());
        }
        if let (Some(lo), Some(hi)) = (min_count, max_count) {
            if lo > hi {
                return Err(format!(
                    "PropertyConstraintIr min_count ({lo}) must not exceed max_count ({hi})"
                ));
            }
        }
        // A provenance without a cardinality is a determinism hazard (it would perturb the
        // key while claiming nothing); a cardinality without a provenance leaves the ledger
        // polarity undecidable. Bind the two together.
        let has_card = min_count.is_some() || max_count.is_some();
        if has_card != cardinality_provenance.is_some() {
            return Err(
                "PropertyConstraintIr.cardinality_provenance must be Some iff a \
                 min_count/max_count is present"
                    .to_owned(),
            );
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
        })
    }

    /// A deterministic content-key over a FIXED field order. Every free-form field is
    /// length-prefixed and the component list is count-prefixed ([`key_field`] / [`key_list`])
    /// so no path or component value can forge a field boundary.
    fn content_key(&self) -> String {
        let comps = key_list(self.components.iter().map(ConstraintComponent::content_key));
        format!(
            "path={}{SEP}min={}{SEP}max={}{SEP}prov={}{SEP}comps={comps}",
            key_field(&self.path),
            self.min_count.map(|n| n.to_string()).unwrap_or_default(),
            self.max_count.map(|n| n.to_string()).unwrap_or_default(),
            self.cardinality_provenance
                .map(|p| p.as_str())
                .unwrap_or(""),
        )
    }
}

/// A named closed-world validation shape (`logic:ValidationShape`): the canonical form the
/// SHACL Core and ShEx surfaces project from. Identity is the content-addressed
/// [`Self::content_key`]; the `iri` is the sort key.
#[derive(Debug, Clone, PartialEq)]
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
    /// The IRI of a nested shape validating the reifier of a reified statement
    /// (`sh:reifierShape`); `None` ⇒ no reifier condition.
    pub reifier_shape: Option<String>,
    /// Whether a matching statement must be reified (`sh:reificationRequired`).
    pub reification_required: bool,
}

impl ValidationShapeIr {
    /// Construct a validation shape, hard-pinning `node_kind` to
    /// [`NodeKind::ValidationShape`] and sorting `properties` into canonical order so supply
    /// order never affects identity. Validates the IRI, target, and reifier fields.
    pub fn new(
        iri: impl Into<String>,
        target: ShapeTarget,
        properties: Vec<PropertyConstraintIr>,
        standpoint: Option<String>,
        reifier_shape: Option<String>,
        reification_required: bool,
    ) -> Result<Self, String> {
        let iri = iri.into();
        if iri.trim().is_empty() {
            return Err("ValidationShapeIr.iri must be a non-empty IRI string".to_owned());
        }
        match &target {
            ShapeTarget::Class(c) if c.trim().is_empty() => {
                return Err("ValidationShapeIr target class must be a non-empty IRI".to_owned());
            }
            ShapeTarget::ValueKeyed { predicate, value }
                if predicate.trim().is_empty() || value.trim().is_empty() =>
            {
                return Err(
                    "ValidationShapeIr value-keyed target needs a non-empty predicate and value"
                        .to_owned(),
                );
            }
            _ => {}
        }
        if let Some(rs) = &reifier_shape {
            if rs.trim().is_empty() {
                return Err(
                    "ValidationShapeIr.reifier_shape must be a non-empty IRI when present; pass \
                     None to leave it unset"
                        .to_owned(),
                );
            }
        }
        if let Some(sp) = &standpoint {
            if sp.trim().is_empty() {
                return Err(
                    "ValidationShapeIr.standpoint must be a non-empty IRI when present; pass None \
                     to leave it unset"
                        .to_owned(),
                );
            }
        }
        let mut properties = properties;
        properties.sort_by_cached_key(PropertyConstraintIr::content_key);
        Ok(Self {
            iri,
            target,
            properties,
            node_kind: NodeKind::ValidationShape,
            standpoint,
            reifier_shape,
            reification_required,
        })
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
    }

    /// A deterministic full-content key for canonical equality. Public to the crate so
    /// `LogicProgram::canonical_key` can fold it into the program key at the fixed tail.
    pub(crate) fn content_key(&self) -> String {
        let props = key_list(
            self.properties
                .iter()
                .map(PropertyConstraintIr::content_key),
        );
        format!(
            "{}{SEP}kind={}{SEP}{}{SEP}sp={}{SEP}reifier={}{SEP}reifreq={}{SEP}PROPS={props}",
            key_field(&self.iri),
            self.node_kind.as_str(),
            self.target.content_key(),
            key_field(self.standpoint.as_deref().unwrap_or("")),
            key_field(self.reifier_shape.as_deref().unwrap_or("")),
            self.reification_required,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(err.contains("XOR"), "got: {err}");
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
