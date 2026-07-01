// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SHACL Core projection of a `logic:` validation shape — `sh:NodeShape` / `sh:PropertyShape`.
//!
//! This is the **constraint** peer of the derivation surface in [`super::shacl_af`]: that
//! module projects the productive subset (`derivation rule` → `sh:SPARQLRule`, *these
//! derive*); this one projects the integrity subset ([`ValidationShapeIr`] →
//! `sh:NodeShape`, *these validate*). It is one of two lowerings of the same canonical
//! [`ValidationShapeIr`] (the ShEx surface is the other), so the two surfaces cannot drift
//! (Principle 17; `design/LOGIC-VALIDATION.md`). The surface is **emit-only** — there is no
//! parse-back from `sh:NodeShape` into a `logic:` validation shape; the canon is the
//! authoring ground (Principle 4).
//!
//! Two components carry residue the SHACL Core surface cannot faithfully hold:
//! [`ConstraintComponent::Pattern`] (the regex dialect differs — SHACL uses the XPath
//! flavour) and [`ConstraintComponent::TerminologyBinding`] (an external terminology has no
//! closed shape form). [`shacl_residue`] enumerates those drops so the loss ledger records
//! them; they are never dropped in silence.

use crate::ir::{
    ConstraintComponent, LogicProgram, PropertyConstraintIr, ShapeTarget, ShapeValue,
    ValidationShapeIr,
};

/// The prefix header prepended to a multi-shape SHACL document.
const SHACL_PREFIXES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
     @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n";

/// Emit an IRI as a Turtle term: a full IRI (containing `://`) is angle-bracketed; anything
/// else (a prefixed name like `xsd:decimal`) is emitted verbatim.
fn iri_term(s: &str) -> String {
    if s.contains("://") {
        format!("<{s}>")
    } else {
        s.to_owned()
    }
}

/// Turtle string-literal escaping (for `sh:pattern`, `sh:flags`, language tags, SPARQL
/// selects). Escapes backslash, quote, and the C0 control chars a Turtle string forbids raw.
fn esc_str(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Format a numeric bound as a bare Turtle literal — an integer literal when the value is
/// whole (and within `i64`), else a plain decimal. Mirrors the ADL/OPT magnitude lowering so
/// the derived SHACL matches the direct-emit oracle byte-for-byte on the interval case.
fn format_bound(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.0e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// A single `sh:in` / value-set member as a Turtle term.
fn shape_value_term(v: &ShapeValue) -> String {
    match v {
        ShapeValue::Iri(i) => iri_term(i),
        ShapeValue::Literal {
            lexical,
            datatype,
            lang,
        } => {
            let mut s = format!("\"{}\"", esc_str(lexical));
            if let Some(l) = lang {
                s.push('@');
                s.push_str(l);
            } else if let Some(dt) = datatype {
                s.push_str("^^");
                s.push_str(&iri_term(dt));
            }
            s
        }
    }
}

/// The `sh:` predicate/object lines one constraint component contributes to a property
/// shape. A [`ConstraintComponent::TerminologyBinding`] contributes nothing here (it is
/// lossy for SHACL Core; see [`shacl_residue`]).
fn component_lines(c: &ConstraintComponent) -> Vec<String> {
    match c {
        ConstraintComponent::NumericRange {
            min,
            max,
            min_inclusive,
            max_inclusive,
        } => {
            let mut v = Vec::new();
            if let Some(lo) = min {
                let p = if *min_inclusive {
                    "minInclusive"
                } else {
                    "minExclusive"
                };
                v.push(format!("sh:{p} {}", format_bound(*lo)));
            }
            if let Some(hi) = max {
                let p = if *max_inclusive {
                    "maxInclusive"
                } else {
                    "maxExclusive"
                };
                v.push(format!("sh:{p} {}", format_bound(*hi)));
            }
            v
        }
        ConstraintComponent::Datatype(d) => vec![format!("sh:datatype {}", iri_term(d))],
        ConstraintComponent::NodeKindShacl(k) => vec![format!("sh:nodeKind sh:{}", k.as_str())],
        ConstraintComponent::In(vs) => {
            let items = vs
                .iter()
                .map(shape_value_term)
                .collect::<Vec<_>>()
                .join(" ");
            vec![format!("sh:in ( {items} )")]
        }
        ConstraintComponent::Pattern { regex, flags } => {
            let mut v = vec![format!("sh:pattern \"{}\"", esc_str(regex))];
            if let Some(f) = flags {
                v.push(format!("sh:flags \"{}\"", esc_str(f)));
            }
            v
        }
        ConstraintComponent::MinLength(n) => vec![format!("sh:minLength {n}")],
        ConstraintComponent::MaxLength(n) => vec![format!("sh:maxLength {n}")],
        ConstraintComponent::LanguageIn(langs) => {
            let items = langs
                .iter()
                .map(|l| format!("\"{}\"", esc_str(l)))
                .collect::<Vec<_>>()
                .join(" ");
            vec![format!("sh:languageIn ( {items} )")]
        }
        ConstraintComponent::DateTimeRange {
            min,
            max,
            min_inclusive,
            max_inclusive,
        } => {
            let mut v = Vec::new();
            if let Some(lo) = min {
                let p = if *min_inclusive {
                    "minInclusive"
                } else {
                    "minExclusive"
                };
                v.push(format!("sh:{p} \"{}\"^^xsd:dateTime", esc_str(lo)));
            }
            if let Some(hi) = max {
                let p = if *max_inclusive {
                    "maxInclusive"
                } else {
                    "maxExclusive"
                };
                v.push(format!("sh:{p} \"{}\"^^xsd:dateTime", esc_str(hi)));
            }
            v
        }
        // Lossy for SHACL Core: an external terminology has no faithful closed shape form.
        // Carried in the loss ledger by shacl_residue, never emitted as a silent constraint.
        ConstraintComponent::TerminologyBinding { .. } => Vec::new(),
    }
}

/// The `[ … ]` blank-node property-shape block for one constrained path.
fn property_shape_block(p: &PropertyConstraintIr) -> String {
    let mut lines = vec![format!("sh:path {}", iri_term(&p.path))];
    if let Some(n) = p.min_count {
        lines.push(format!("sh:minCount {n}"));
    }
    if let Some(n) = p.max_count {
        lines.push(format!("sh:maxCount {n}"));
    }
    for c in &p.components {
        lines.extend(component_lines(c));
    }
    format!("[ {} ]", lines.join(" ; "))
}

/// Project one [`ValidationShapeIr`] to a SHACL Core `sh:NodeShape` (Turtle, no prefixes).
pub fn project_validation_shape_shacl(shape: &ValidationShapeIr) -> String {
    let mut pos: Vec<String> = vec!["a sh:NodeShape".to_owned()];
    match &shape.target {
        ShapeTarget::Class(c) => pos.push(format!("sh:targetClass {}", iri_term(c))),
        ShapeTarget::ValueKeyed { predicate, value } => pos.push(format!(
            "sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE {{ ?this {} {} }}\"\"\" ]",
            iri_term(predicate),
            iri_term(value)
        )),
    }
    if let Some(rs) = &shape.reifier_shape {
        pos.push(format!("sh:reifierShape {}", iri_term(rs)));
    }
    if shape.reification_required {
        pos.push("sh:reificationRequired true".to_owned());
    }
    for p in &shape.properties {
        pos.push(format!("sh:property {}", property_shape_block(p)));
    }
    format!("{} {} .\n", iri_term(&shape.iri), pos.join(" ;\n    "))
}

/// Project every validation shape in `program` to a single SHACL Core Turtle document (with
/// the prefix header), in the program's canonical shape order. A shape-free program yields
/// the empty string (nothing to emit — the pipeline writes no file).
pub fn project_validation_shapes_shacl(program: &LogicProgram) -> String {
    if program.validation_shapes.is_empty() {
        return String::new();
    }
    let mut out = String::from(SHACL_PREFIXES);
    for (i, s) in program.validation_shapes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&project_validation_shape_shacl(s));
    }
    out
}

/// The per-shape loss-ledger residue for the SHACL Core target: the constructs SHACL Core
/// cannot faithfully hold, carried and flagged (never dropped in silence). A shape with no
/// lossy component yields an empty vector (the `ValidationOnly` polarity with no residue).
pub fn shacl_residue(shape: &ValidationShapeIr) -> Vec<String> {
    let mut residue = Vec::new();
    for p in &shape.properties {
        for c in &p.components {
            match c {
                ConstraintComponent::Pattern { regex, .. } => residue.push(format!(
                    "sh:pattern on {} carries regex-dialect residue (SHACL uses the XPath \
                     regular-expression flavour; the source dialect may differ): {regex}",
                    p.path
                )),
                ConstraintComponent::TerminologyBinding {
                    terminology_id,
                    codes,
                } => residue.push(format!(
                    "terminology binding on {} to {terminology_id} ({} code(s)) has no faithful \
                     SHACL Core form; carried in the canonical logic: layer",
                    p.path,
                    codes.len()
                )),
                _ => {}
            }
        }
    }
    residue
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ConstraintProvenance, ShaclNodeKind};

    fn prop(path: &str, comps: Vec<ConstraintComponent>) -> PropertyConstraintIr {
        PropertyConstraintIr::new(path, None, None, None, comps).unwrap()
    }

    fn shape(iri: &str, class: &str, props: Vec<PropertyConstraintIr>) -> ValidationShapeIr {
        ValidationShapeIr::new(
            iri,
            ShapeTarget::Class(class.to_owned()),
            props,
            None,
            None,
            false,
        )
        .unwrap()
    }

    #[test]
    fn half_open_quantity_interval_emits_min_inclusive_and_max_exclusive() {
        let s = shape(
            "https://ex/BpShape",
            "https://ex/Systolic",
            vec![prop(
                "https://ex/magnitude",
                vec![
                    ConstraintComponent::NumericRange {
                        min: Some(0.0),
                        max: Some(1000.0),
                        min_inclusive: true,
                        max_inclusive: false,
                    },
                    ConstraintComponent::Datatype(
                        "http://www.w3.org/2001/XMLSchema#decimal".into(),
                    ),
                ],
            )],
        );
        let ttl = project_validation_shape_shacl(&s);
        assert!(ttl.contains("a sh:NodeShape"), "{ttl}");
        assert!(
            ttl.contains("sh:targetClass <https://ex/Systolic>"),
            "{ttl}"
        );
        assert!(ttl.contains("sh:minInclusive 0"), "{ttl}");
        assert!(ttl.contains("sh:maxExclusive 1000"), "{ttl}");
        assert!(
            !ttl.contains("sh:maxInclusive"),
            "half-open upper must be exclusive: {ttl}"
        );
        assert!(
            ttl.contains("sh:datatype <http://www.w3.org/2001/XMLSchema#decimal>"),
            "{ttl}"
        );
    }

    #[test]
    fn cardinality_emits_min_and_max_count() {
        let p = PropertyConstraintIr::new(
            "https://ex/systolic",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OptNative),
            vec![],
        )
        .unwrap();
        let ttl = project_validation_shape_shacl(&shape("https://ex/S", "https://ex/C", vec![p]));
        assert!(ttl.contains("sh:minCount 1"), "{ttl}");
        assert!(ttl.contains("sh:maxCount 1"), "{ttl}");
    }

    #[test]
    fn inline_value_set_emits_sh_in_list() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/code",
                vec![ConstraintComponent::In(vec![
                    ShapeValue::Iri("https://ex/at0004".into()),
                    ShapeValue::Iri("https://ex/at0005".into()),
                ])],
            )],
        );
        let ttl = project_validation_shape_shacl(&s);
        assert!(
            ttl.contains("sh:in ( <https://ex/at0004> <https://ex/at0005> )"),
            "{ttl}"
        );
    }

    #[test]
    fn node_kind_and_pattern_emit_and_pattern_is_ledgered() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/name",
                vec![
                    ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal),
                    ConstraintComponent::Pattern {
                        regex: "^[A-Z].*$".into(),
                        flags: Some("i".into()),
                    },
                ],
            )],
        );
        let ttl = project_validation_shape_shacl(&s);
        assert!(ttl.contains("sh:nodeKind sh:Literal"), "{ttl}");
        assert!(ttl.contains("sh:pattern \"^[A-Z].*$\""), "{ttl}");
        assert!(ttl.contains("sh:flags \"i\""), "{ttl}");
        // The pattern is lossy → the ledger records the regex-dialect residue.
        let residue = shacl_residue(&s);
        assert_eq!(residue.len(), 1, "pattern must be ledgered: {residue:?}");
        assert!(residue[0].contains("regex-dialect residue"), "{residue:?}");
    }

    #[test]
    fn reifier_shape_and_requirement_emit_the_rdf12_extension() {
        let s = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![],
            None,
            Some("https://ex/ReifierShape".into()),
            true,
        )
        .unwrap();
        let ttl = project_validation_shape_shacl(&s);
        assert!(
            ttl.contains("sh:reifierShape <https://ex/ReifierShape>"),
            "{ttl}"
        );
        assert!(ttl.contains("sh:reificationRequired true"), "{ttl}");
    }

    #[test]
    fn value_keyed_target_emits_sparql_target() {
        let s = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::ValueKeyed {
                predicate: "https://ex/kind".into(),
                value: "https://ex/Bp".into(),
            },
            vec![],
            None,
            None,
            false,
        )
        .unwrap();
        let ttl = project_validation_shape_shacl(&s);
        assert!(ttl.contains("a sh:SPARQLTarget"), "{ttl}");
        assert!(
            ttl.contains("?this <https://ex/kind> <https://ex/Bp>"),
            "{ttl}"
        );
    }

    #[test]
    fn terminology_binding_is_not_emitted_but_is_ledgered() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/code",
                vec![ConstraintComponent::TerminologyBinding {
                    terminology_id: "SNOMED-CT".into(),
                    codes: vec!["271649006".into()],
                }],
            )],
        );
        let ttl = project_validation_shape_shacl(&s);
        assert!(
            !ttl.contains("SNOMED"),
            "terminology must not leak into SHACL: {ttl}"
        );
        let residue = shacl_residue(&s);
        assert_eq!(residue.len(), 1);
        assert!(
            residue[0].contains("no faithful SHACL Core form"),
            "{residue:?}"
        );
    }

    #[test]
    fn empty_program_yields_empty_document() {
        let prog = LogicProgram::new(vec![], vec![], vec![], None);
        assert_eq!(project_validation_shapes_shacl(&prog), "");
    }

    #[test]
    fn multi_shape_document_carries_prefixes_and_all_shapes() {
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_validation_shapes(vec![
            shape("https://ex/A", "https://ex/CA", vec![]),
            shape("https://ex/B", "https://ex/CB", vec![]),
        ]);
        let doc = project_validation_shapes_shacl(&prog);
        assert!(doc.contains("@prefix sh:"), "{doc}");
        assert!(doc.contains("<https://ex/A> a sh:NodeShape"), "{doc}");
        assert!(doc.contains("<https://ex/B> a sh:NodeShape"), "{doc}");
    }
}
