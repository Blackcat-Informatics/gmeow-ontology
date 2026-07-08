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
    ConstraintComponent, ConstraintIr, Formula, LogicProgram, PropertyConstraintIr, ShaclNodeKind,
    ShapeTarget, ShapeValue, Term, ValidationShapeIr,
};

use super::sparql_lower::{sparql_literal, sparql_predicate};

/// The prefix header prepended to a multi-shape SHACL document.
const SHACL_PREFIXES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
     @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n";

/// Emit an IRI as a Turtle term. A CURIE under one of the document's declared prefixes
/// (`sh:` / `xsd:`, from [`SHACL_PREFIXES`]) is emitted verbatim; every other value is an
/// absolute IRI — including a non-hierarchical one like `urn:uuid:…` or `mailto:…` that has no
/// `://` — and is angle-bracketed so the Turtle/SPARQL stays valid.
fn iri_term(s: &str) -> String {
    if s.starts_with("sh:") || s.starts_with("xsd:") {
        s.to_owned()
    } else {
        format!("<{s}>")
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
        // A precision satellite projects to the same numeric facets as a magnitude range — it is
        // faithfully expressible in SHACL Core (no residue).
        ConstraintComponent::PrecisionRange {
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
        ConstraintComponent::Class(c) => vec![format!("sh:class {}", iri_term(c))],
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
        // Project only the coded symbols as sh:in; the ordinal integers and their ordering have
        // no SHACL form — carried in the loss ledger by shacl_residue.
        ConstraintComponent::OrdinalSet { pairs } => {
            let items = pairs
                .iter()
                .map(|(_, c)| iri_term(c))
                .collect::<Vec<_>>()
                .join(" ");
            vec![format!("sh:in ( {items} )")]
        }
        // An openEHR datetime validity pattern is a format template (e.g. `yyyy-mm-ddTHH:MM:SS`),
        // NOT an XPath regular expression. Emitting it as `sh:pattern` would match those literal
        // characters and reject every valid datetime — an inverted constraint, not a lossy one.
        // Nothing faithful survives in SHACL Core, so it is carried in the loss ledger by
        // `shacl_residue`, never emitted as a broken constraint (cf. `TerminologyBinding` above).
        ConstraintComponent::DateTimePattern(_) => Vec::new(),
        // A fixed required value (closed-world `owl:hasValue`).
        ConstraintComponent::HasValue(v) => vec![format!("sh:hasValue {}", shape_value_term(v))],
        // A qualified value-shape count (`owl:someValuesFrom` → min 1; `owl:onClass` +
        // `owl:qualifiedCardinality` → the count) — the values satisfying the inner shape are
        // counted, NOT all values. The inner shape's own component lines nest in the `[ … ]`.
        ConstraintComponent::QualifiedValueShape { shape, min, max } => {
            let inner: Vec<String> = shape.iter().flat_map(component_lines).collect();
            let mut v = vec![format!("sh:qualifiedValueShape [ {} ]", inner.join(" ; "))];
            if let Some(n) = min {
                v.push(format!("sh:qualifiedMinCount {n}"));
            }
            if let Some(n) = max {
                v.push(format!("sh:qualifiedMaxCount {n}"));
            }
            v
        }
        // A negated constraint (`owl:disjointWith`/`owl:complementOf`/`owl:AllDisjointClasses`
        // pair → `sh:not [ sh:class D ]`).
        ConstraintComponent::Not(inner) => {
            vec![format!("sh:not [ {} ]", component_lines(inner).join(" ; "))]
        }
    }
}

/// The `sh:path` term for a property shape — a bare predicate, or an `sh:inversePath` blank
/// node when the path is inverted (the `owl:InverseFunctionalProperty` reading).
fn path_term(p: &PropertyConstraintIr) -> String {
    if p.inverse {
        format!("sh:path [ sh:inversePath {} ]", iri_term(&p.path))
    } else {
        format!("sh:path {}", iri_term(&p.path))
    }
}

/// The `[ … ]` blank-node property-shape block for one constrained path.
fn property_shape_block(p: &PropertyConstraintIr) -> String {
    let mut lines = vec![path_term(p)];
    if let Some(n) = p.min_count {
        lines.push(format!("sh:minCount {n}"));
    }
    if let Some(n) = p.max_count {
        lines.push(format!("sh:maxCount {n}"));
    }
    for c in &p.components {
        lines.extend(component_lines(c));
    }
    // RDF-1.2 statement-layer extension: the reifier of each `focus`→`path`→`value` statement must
    // conform to `sh:reifierShape`, and `sh:reificationRequired true` demands ≥1 reifier. The
    // native engine reads these only from a single forward-predicate property shape, so they are
    // suppressed on an inverse path (which `PropertyConstraintIr::with_reifier` already rejects).
    if !p.inverse {
        if let Some(rs) = &p.reifier_shape {
            lines.push(format!("sh:reifierShape {}", iri_term(rs)));
        }
        if p.reification_required {
            lines.push("sh:reificationRequired true".to_owned());
        }
    }
    if let Some(sev) = p.severity {
        lines.push(format!("sh:severity sh:{}", sev.as_str()));
    }
    if let Some(msg) = &p.message {
        lines.push(format!("sh:message \"{}\"", esc_str(msg)));
    }
    format!("[ {} ]", lines.join(" ; "))
}

/// Project one [`ValidationShapeIr`] to a SHACL Core `sh:NodeShape` (Turtle, no prefixes). An
/// `rdfs:label` (when present) is emitted with the fully-qualified predicate so the surface
/// stays valid without an `rdfs:` prefix declaration in the default (prefix-free) header.
pub fn project_validation_shape_shacl(shape: &ValidationShapeIr) -> String {
    let mut pos: Vec<String> = vec!["a sh:NodeShape".to_owned()];
    if let Some(label) = &shape.label {
        pos.push(format!(
            "<http://www.w3.org/2000/01/rdf-schema#label> \"{}\"",
            esc_str(label)
        ));
    }
    match &shape.target {
        ShapeTarget::Class(c) => pos.push(format!("sh:targetClass {}", iri_term(c))),
        ShapeTarget::SubjectsOf(p) => pos.push(format!("sh:targetSubjectsOf {}", iri_term(p))),
        ShapeTarget::ObjectsOf(p) => pos.push(format!("sh:targetObjectsOf {}", iri_term(p))),
        ShapeTarget::ValueKeyed { predicate, value } => pos.push(format!(
            "sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE {{ ?this {} {} }}\"\"\" ]",
            iri_term(predicate),
            iri_term(value)
        )),
    }
    // Focus-node-level constraints (domain/range/disjointness) — emitted directly on the node
    // shape, not inside a property block.
    for c in &shape.node_components {
        pos.extend(component_lines(c));
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

/// The SHACL Core residue of ONE constraint component reachable at `path`, appended to `out`.
///
/// The `match` is deliberately **exhaustive** (no `_` catch-all): a new [`ConstraintComponent`]
/// variant is a compile error until it is explicitly classified as faithful (`=> {}`) or as a
/// carried-and-flagged drop, so the loss ledger's "never dropped in silence" contract cannot be
/// defeated by a future variant. The two structural wrappers ([`ConstraintComponent::Not`],
/// [`ConstraintComponent::QualifiedValueShape`]) recurse into their inner shape, so a lossy
/// component nested inside a negation or a qualified value-shape is flagged at every depth (the
/// depth-honest realization of [`ConstraintComponent::is_lossy`], which also recurses).
fn shacl_component_residue(path: &str, c: &ConstraintComponent, out: &mut Vec<String>) {
    match c {
        ConstraintComponent::Pattern { regex, .. } => out.push(format!(
            "sh:pattern on {path} carries regex-dialect residue (SHACL uses the XPath \
             regular-expression flavour; the source dialect may differ): {regex}"
        )),
        ConstraintComponent::TerminologyBinding {
            terminology_id,
            codes,
        } => out.push(format!(
            "terminology binding on {path} to {terminology_id} ({} code(s)) has no faithful \
             SHACL Core form; carried in the canonical logic: layer",
            codes.len()
        )),
        ConstraintComponent::OrdinalSet { pairs } => out.push(format!(
            "ordinal set on {path} projects only its coded symbols (sh:in); the ordinal \
             integer values ({}) and their ordering have no SHACL/ShEx form and are \
             carried in the canonical logic: layer",
            pairs
                .iter()
                .map(|(v, _)| v.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        ConstraintComponent::DateTimePattern(pat) => out.push(format!(
            "datetime validity pattern ({pat}) on {path} is a format template, not an XPath \
             regex; it has no faithful SHACL/ShEx form (emitting it as sh:pattern would \
             reject every valid datetime), so its meaning is carried in the canonical \
             logic: layer"
        )),
        // A structural wrapper's residue is exactly its inner shape's residue.
        ConstraintComponent::Not(inner) => shacl_component_residue(path, inner, out),
        ConstraintComponent::QualifiedValueShape { shape, .. } => {
            for inner in shape {
                shacl_component_residue(path, inner, out);
            }
        }
        // Faithful in SHACL Core — no residue. Listed explicitly (not a `_` arm) so a NEW
        // component variant forces a faithful-or-residue decision at compile time.
        ConstraintComponent::NumericRange { .. }
        | ConstraintComponent::PrecisionRange { .. }
        | ConstraintComponent::Datatype(_)
        | ConstraintComponent::Class(_)
        | ConstraintComponent::NodeKindShacl(_)
        | ConstraintComponent::In(_)
        | ConstraintComponent::MinLength(_)
        | ConstraintComponent::MaxLength(_)
        | ConstraintComponent::LanguageIn(_)
        | ConstraintComponent::DateTimeRange { .. }
        | ConstraintComponent::HasValue(_) => {}
    }
}

/// The per-shape loss-ledger residue for the SHACL Core target: the constructs SHACL Core
/// cannot faithfully hold, carried and flagged (never dropped in silence). A shape with no
/// lossy component yields an empty vector (the `ValidationOnly` polarity with no residue).
pub fn shacl_residue(shape: &ValidationShapeIr) -> Vec<String> {
    let mut residue = Vec::new();
    // A standpoint-indexed shape holds only under its standpoint (world); a standpoint-blind
    // SHACL/ShEx engine would apply it universally. There is no SHACL/ShEx standpoint facet, so
    // the scope is carried in the canonical logic: layer, never silently flattened to universal.
    if let Some(sp) = &shape.standpoint {
        residue.push(format!(
            "standpoint scope {sp} has no SHACL/ShEx form; the shape would be applied universally \
             by a standpoint-blind engine, so its scope is carried in the canonical logic: layer"
        ));
    }
    for p in &shape.properties {
        for c in &p.components {
            shacl_component_residue(&p.path, c, &mut residue);
        }
    }
    // Focus-node-level components (domain/range/disjointness) are also lossy if they nest a lossy
    // construct — scanned through the same exhaustive helper so a node-level drop is never silent.
    for c in &shape.node_components {
        shacl_component_residue("the focus node", c, &mut residue);
    }
    residue
}

/// The prefix header prepended to a multi-shape ShEx (ShExC) document.
const SHEX_PREFIXES: &str = "PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n\n";

/// A ShExC cardinality suffix for a triple constraint (`?`, `*`, `+`, `{m,n}`); an empty
/// string is ShEx's default exactly-one.
fn shex_cardinality(min: Option<u32>, max: Option<u32>) -> String {
    match (min, max) {
        (None, None) => String::new(),
        (Some(1), Some(1)) => String::new(),
        (Some(0), Some(1)) => " ?".to_owned(),
        (Some(0), None) => " *".to_owned(),
        (Some(1), None) => " +".to_owned(),
        (Some(lo), Some(hi)) if lo == hi => format!(" {{{lo}}}"),
        (Some(lo), Some(hi)) => format!(" {{{lo},{hi}}}"),
        (Some(lo), None) => format!(" {{{lo},}}"),
        (None, Some(hi)) => format!(" {{0,{hi}}}"),
    }
}

/// The ShEx node constraint (value expression) for one property shape, over the fragment
/// ShEx can faithfully express: value sets, datatype + numeric facets, string patterns and
/// lengths, and node kinds. Constructs ShEx cannot hold (datetime ranges, `languageIn`,
/// terminology bindings) fall through to a permissive base and are declared in
/// [`shex_residue`].
fn shex_value_expr(p: &PropertyConstraintIr) -> String {
    // A value set (or a fixed single value) is itself the node constraint.
    for c in &p.components {
        if let ConstraintComponent::In(vs) = c {
            let items = vs
                .iter()
                .map(shape_value_term)
                .collect::<Vec<_>>()
                .join(" ");
            return format!("[{items}]");
        }
        if let ConstraintComponent::OrdinalSet { pairs } = c {
            let items = pairs
                .iter()
                .map(|(_, s)| iri_term(s))
                .collect::<Vec<_>>()
                .join(" ");
            return format!("[{items}]");
        }
        // A fixed required value (`sh:hasValue`) is a one-element ShEx value set.
        if let ConstraintComponent::HasValue(v) = c {
            return format!("[{}]", shape_value_term(v));
        }
    }
    let mut datatype = String::new();
    let mut nodekind: Option<&str> = None;
    let mut facets: Vec<String> = Vec::new();
    for c in &p.components {
        match c {
            ConstraintComponent::Datatype(d) => datatype = iri_term(d),
            ConstraintComponent::NumericRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => {
                if let Some(lo) = min {
                    let f = if *min_inclusive {
                        "MININCLUSIVE"
                    } else {
                        "MINEXCLUSIVE"
                    };
                    facets.push(format!("{f} {}", format_bound(*lo)));
                }
                if let Some(hi) = max {
                    let f = if *max_inclusive {
                        "MAXINCLUSIVE"
                    } else {
                        "MAXEXCLUSIVE"
                    };
                    facets.push(format!("{f} {}", format_bound(*hi)));
                }
            }
            // A precision satellite projects to the same ShEx numeric facets as a magnitude range.
            ConstraintComponent::PrecisionRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => {
                if let Some(lo) = min {
                    let f = if *min_inclusive {
                        "MININCLUSIVE"
                    } else {
                        "MINEXCLUSIVE"
                    };
                    facets.push(format!("{f} {}", format_bound(*lo)));
                }
                if let Some(hi) = max {
                    let f = if *max_inclusive {
                        "MAXINCLUSIVE"
                    } else {
                        "MAXEXCLUSIVE"
                    };
                    facets.push(format!("{f} {}", format_bound(*hi)));
                }
            }
            ConstraintComponent::Pattern { regex, flags } => {
                // ShExC delimits a regex with `/…/`; a literal `/` in the pattern MUST be escaped
                // as `\/` (and ONLY that — escaping `\` would change the regex semantics).
                let delimited = regex.replace('/', "\\/");
                facets.push(format!("/{delimited}/{}", flags.as_deref().unwrap_or("")))
            }
            ConstraintComponent::MinLength(n) => facets.push(format!("MINLENGTH {n}")),
            ConstraintComponent::MaxLength(n) => facets.push(format!("MAXLENGTH {n}")),
            ConstraintComponent::NodeKindShacl(k) => {
                nodekind = Some(match k {
                    ShaclNodeKind::Iri => "IRI",
                    ShaclNodeKind::Literal => "LITERAL",
                    ShaclNodeKind::BlankNode => "BNODE",
                    _ => "NONLITERAL",
                })
            }
            // A class-membership constraint: ShEx has no `sh:class` facet, so the values are
            // only constrained to IRIs here; the class itself is declared in shex_residue.
            ConstraintComponent::Class(_) => nodekind = Some("IRI"),
            // A qualified value shape whose inner is a class/datatype constrains the counted
            // value's kind; ShEx expresses the value's node kind (IRI for a class, the datatype
            // for a datatype) but not the qualified COUNT independently of the triple-constraint
            // cardinality — the count is declared in shex_residue.
            ConstraintComponent::QualifiedValueShape { shape, .. } => {
                for inner in shape {
                    match inner {
                        ConstraintComponent::Datatype(d) => datatype = iri_term(d),
                        ConstraintComponent::Class(_) => nodekind = Some("IRI"),
                        ConstraintComponent::NodeKindShacl(k) => {
                            nodekind = Some(match k {
                                ShaclNodeKind::Iri => "IRI",
                                ShaclNodeKind::Literal => "LITERAL",
                                ShaclNodeKind::BlankNode => "BNODE",
                                _ => "NONLITERAL",
                            })
                        }
                        _ => {}
                    }
                }
            }
            // Not faithfully expressible in ShEx — declared in shex_residue. A DateTimePattern is a
            // format template, not a regex, so emitting it as a `/…/` facet would reject every
            // valid datetime; its meaning is carried in the canonical logic: layer. ShEx Core has
            // no negation, so `Not` is carried in the ledger; `HasValue` was handled above.
            ConstraintComponent::DateTimeRange { .. }
            | ConstraintComponent::DateTimePattern(_)
            | ConstraintComponent::LanguageIn(_)
            | ConstraintComponent::TerminologyBinding { .. }
            | ConstraintComponent::In(_)
            | ConstraintComponent::OrdinalSet { .. }
            | ConstraintComponent::HasValue(_)
            | ConstraintComponent::Not(_) => {}
        }
    }
    let base = if !datatype.is_empty() {
        datatype
    } else if let Some(nk) = nodekind {
        nk.to_owned()
    } else {
        ".".to_owned()
    };
    if facets.is_empty() {
        base
    } else if base == "." {
        // No datatype or node-kind base: the facets themselves ARE the node
        // constraint (ShExC `xsFacet+`, e.g. a bare `/…/` pattern or a MINLENGTH).
        // A leading `.` (the any-node shapeAtom) would be a second, illegal
        // shapeAtom juxtaposed with the facet and the document would not parse.
        facets.join(" ")
    } else {
        format!("{base} {}", facets.join(" "))
    }
}

/// Project one [`ValidationShapeIr`] to a ShEx shape expression (ShExC, no prefixes). The
/// target-class association is external in ShEx (a ShapeMap), so it is emitted as a comment.
pub fn project_validation_shape_shex(shape: &ValidationShapeIr) -> String {
    let mut out = String::new();
    match &shape.target {
        ShapeTarget::Class(c) => out.push_str(&format!(
            "# targetClass {} (associate via ShapeMap)\n",
            iri_term(c)
        )),
        ShapeTarget::SubjectsOf(p) => out.push_str(&format!(
            "# targetSubjectsOf {} (associate via ShapeMap)\n",
            iri_term(p)
        )),
        ShapeTarget::ObjectsOf(p) => out.push_str(&format!(
            "# targetObjectsOf {} (associate via ShapeMap)\n",
            iri_term(p)
        )),
        // A value-keyed (SPARQL) target has no ShEx form; shex_residue records it.
        ShapeTarget::ValueKeyed { .. } => {}
    }
    out.push_str(&format!("{} {{\n", iri_term(&shape.iri)));
    for p in &shape.properties {
        out.push_str(&format!(
            "  {} {}{} ;\n",
            iri_term(&p.path),
            shex_value_expr(p),
            shex_cardinality(p.min_count, p.max_count)
        ));
    }
    out.push_str("}\n");
    out
}

/// Project every validation shape in `program` to a single ShEx document. A shape-free
/// program yields the empty string.
pub fn project_validation_shapes_shex(program: &LogicProgram) -> String {
    if program.validation_shapes.is_empty() {
        return String::new();
    }
    let mut out = String::from(SHEX_PREFIXES);
    for (i, s) in program.validation_shapes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&project_validation_shape_shex(s));
    }
    out
}

/// The per-shape loss-ledger residue for the ShEx target — **strictly larger** than the
/// SHACL Core residue, because ShEx has no SPARQL target, no RDF-1.2 statement layer, no
/// `languageIn`, and no datetime-range facet. Everything SHACL loses (patterns, terminology)
/// plus these ShEx-only drops is carried and flagged, never dropped in silence.
pub fn shex_residue(shape: &ValidationShapeIr) -> Vec<String> {
    let mut residue = shacl_residue(shape);
    if let ShapeTarget::ValueKeyed { .. } = &shape.target {
        residue.push(
            "value-keyed target has no ShEx form (ShEx associates shapes via an external \
             ShapeMap, not a SPARQL target); carried in the canonical logic: layer"
                .to_owned(),
        );
    }
    // A focus-node-level constraint (domain/range/disjointness) has no ShEx shape-level form —
    // ShEx associates a shape via an external ShapeMap, so a `sh:targetSubjectsOf`/`ObjectsOf`
    // selector and any node-level `sh:class`/`sh:not` are carried in the canonical logic: layer.
    if !shape.node_components.is_empty() {
        residue.push(format!(
            "{} focus-node-level constraint(s) (domain/range/disjointness) have no ShEx \
             shape-level form; carried in the canonical logic: layer",
            shape.node_components.len()
        ));
    }
    for p in &shape.properties {
        if p.reifier_shape.is_some() || p.reification_required {
            residue.push(format!(
                "RDF-1.2 reifier/reification-required condition on {} has no ShEx form; carried in \
                 the canonical logic: layer",
                p.path
            ));
        }
        for c in &p.components {
            // Exhaustive (no `_` catch-all): a new ConstraintComponent variant must be classified
            // as a ShEx-only drop or as ShEx-faithful before it compiles. Constructs SHACL loses
            // (Pattern/TerminologyBinding/OrdinalSet/DateTimePattern) are already carried by the
            // `shacl_residue(shape)` base above, so they add no *further* ShEx drop here (`=> {}`).
            match c {
                ConstraintComponent::DateTimeRange { .. } => residue.push(format!(
                    "datetime range on {} has no ShEx facet; only the value's presence is \
                     projected, the interval is carried in the canonical logic: layer",
                    p.path
                )),
                ConstraintComponent::LanguageIn(_) => residue.push(format!(
                    "languageIn on {} has no ShEx form; carried in the canonical logic: layer",
                    p.path
                )),
                ConstraintComponent::Class(class) => residue.push(format!(
                    "sh:class {class} on {} has no ShEx facet; ShEx constrains the value to an IRI \
                     only, the class membership is carried in the canonical logic: layer",
                    p.path
                )),
                // The two structural wrappers flag at the WRAPPER level and do NOT recurse into
                // their inner shape (unlike `shacl_component_residue`, which does): ShEx Core has
                // no negation and no qualified-value-shape at all, so the whole wrapper is dropped
                // — the wrapper-level residue subsumes any inner-component residue. Recursing here
                // would double-flag the same lost construct; do not "fix" it into a recursion.
                ConstraintComponent::QualifiedValueShape { min, max, .. } => residue.push(format!(
                    "qualified value-shape count (min={min:?}, max={max:?}) on {} has no \
                     independent ShEx form (ShEx cardinality is on the triple constraint); the \
                     qualified count is carried in the canonical logic: layer",
                    p.path
                )),
                ConstraintComponent::Not(_) => residue.push(format!(
                    "negated constraint (sh:not) on {} has no ShEx Core form (ShEx Core has no \
                     negation); carried in the canonical logic: layer",
                    p.path
                )),
                // ShEx-faithful, or already carried by the shacl_residue base — no *additional*
                // ShEx-only drop. Listed explicitly so a NEW variant forces a decision.
                ConstraintComponent::NumericRange { .. }
                | ConstraintComponent::PrecisionRange { .. }
                | ConstraintComponent::Datatype(_)
                | ConstraintComponent::NodeKindShacl(_)
                | ConstraintComponent::In(_)
                | ConstraintComponent::Pattern { .. }
                | ConstraintComponent::MinLength(_)
                | ConstraintComponent::MaxLength(_)
                | ConstraintComponent::TerminologyBinding { .. }
                | ConstraintComponent::OrdinalSet { .. }
                | ConstraintComponent::DateTimePattern(_)
                | ConstraintComponent::HasValue(_) => {}
            }
        }
    }
    residue
}

// --------------------------------------------------------------------------- //
// Procedural constraints — logic:Constraint → sh:SPARQLConstraint (the validation
// twin of the SHACL-AF rule projection: those DERIVE, these VALIDATE)
// --------------------------------------------------------------------------- //

/// The prefix header of the whole-program procedural-constraint document. Always emitted
/// (even for a constraint-free program) so the corpus stays byte-stable: a constraint-free
/// program yields exactly this header, and each authored `logic:Constraint` appends one
/// `sh:NodeShape` block below it.
const PROCEDURAL_HEADER: &str = "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
     # Procedural-constraint projection of the canonical logic: program: each closed-world\n\
     # logic:Constraint integrity condition projected to a sh:SPARQLConstraint NodeShape\n\
     # carrying logic:formalizes (the validation twin of the SHACL-AF sh:SPARQLRule surface;\n\
     # Principle 17 — logic: is canonical, SHACL is the projection; design/LOGIC-VALIDATION.md).\n\
     @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix sh:    <http://www.w3.org/ns/shacl#> .\n\
     @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n";

/// The local name of a `gmeow:`/`logic:` IRI (the part after the last `/` or `#`).
fn local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// The local name with its first character upper-cased (`counterGoal` → `CounterGoal`).
fn pascal(iri: &str) -> String {
    let l = local_name(iri);
    let mut c = l.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The deterministic `gmeow:` shape IRI a constraint projects to (`{Name}ProceduralConstraintShape`).
fn procedural_shape_iri(c: &ConstraintIr) -> String {
    format!(
        "https://blackcatinformatics.ca/gmeow/{}ProceduralConstraintShape",
        pascal(&c.iri)
    )
}

/// Render one FOL [`Term`] as a SPARQL subject/object token: the focus variable renders as
/// the SHACL pre-bound `$this`, any other variable keeps its `?name` form, an IRI is
/// angle-bracketed, and a data literal is single-quoted (with an optional `^^<datatype>`).
/// A sequence marker has no single-term SPARQL form and is refused (carried as residue).
fn constraint_term(t: &Term, focus: &str) -> Result<String, String> {
    match t {
        Term::Var(n) if n == focus => Ok("$this".to_owned()),
        Term::Var(n) => Ok(format!("?{n}")),
        Term::Iri(i) => Ok(format!("<{i}>")),
        Term::Literal { lexical, datatype } => {
            let lit = sparql_literal(lexical);
            match datatype {
                Some(dt) => Ok(format!("{lit}^^<{dt}>")),
                None => Ok(lit),
            }
        }
        Term::SequenceMarker(n) => Err(format!(
            "sequence marker ...{n} has no single-term SPARQL triple form"
        )),
    }
}

/// Render one binary atomic predication as a SPARQL triple pattern `subj pred obj .`. A
/// non-binary atom (unary or n ≥ 3) or a sequence-marker argument has no direct SPARQL
/// triple form and is refused so the constraint is carried-and-flagged rather than emitted
/// as a broken query.
fn constraint_atom(f: &Formula, focus: &str) -> Result<String, String> {
    let Formula::Atom { relation, args } = f else {
        return Err("expected an atomic predication".to_owned());
    };
    let Term::Iri(pred) = relation else {
        return Err("atom relation must be an IRI".to_owned());
    };
    if args.len() != 2 {
        return Err(format!(
            "atom <{pred}> has arity {}, only a binary atom lowers to a SPARQL triple pattern",
            args.len()
        ));
    }
    let s = constraint_term(&args[0], focus)?;
    let p = sparql_predicate(pred);
    let o = constraint_term(&args[1], focus)?;
    Ok(format!("{s} {p} {o} ."))
}

/// Lower a formula to the SPARQL group-graph-pattern fragments that hold **iff the formula
/// is satisfied** (for the pre-bound focus `$this`). The reused NNF/BGP/`FILTER NOT EXISTS`
/// machinery: a positive atom is a triple pattern, an existential is its (BGP-existential)
/// body, a disjunction is a `UNION`, a negation flips to [`lower_negative`]. A universal in
/// positive position has no bounded SPARQL BGP form (it would need a double negation over an
/// open domain) and is refused so the constraint is carried-and-flagged.
fn lower_positive(f: &Formula, focus: &str) -> Result<Vec<String>, String> {
    match f {
        Formula::Atom { .. } => Ok(vec![constraint_atom(f, focus)?]),
        Formula::And(fs) => {
            let mut out = Vec::new();
            for x in fs {
                out.extend(lower_positive(x, focus)?);
            }
            Ok(out)
        }
        Formula::Or(fs) => {
            let mut branches = Vec::with_capacity(fs.len());
            for x in fs {
                branches.push(format!("{{ {} }}", lower_positive(x, focus)?.join(" ")));
            }
            Ok(vec![branches.join(" UNION ")])
        }
        // An existential body's variables are ordinary SPARQL variables — a BGP is
        // existential by default, so `∃v. φ` is exactly the positive lowering of `φ`.
        Formula::Exists { body, .. } => lower_positive(body, focus),
        Formula::Not(inner) => lower_negative(inner, focus),
        // `a → b` ≡ `¬a ∨ b`: the branch where the antecedent fails UNION the branch where
        // the consequent holds.
        Formula::Implies(a, b) => Ok(vec![format!(
            "{{ {} }} UNION {{ {} }}",
            lower_negative(a, focus)?.join(" "),
            lower_positive(b, focus)?.join(" ")
        )]),
        Formula::Forall { .. } => Err(
            "a universal in positive position has no bounded SPARQL BGP form (it would require a \
             double negation over an open domain)"
                .to_owned(),
        ),
        Formula::Iff(..) => Err("a biconditional has no SPARQL constraint-body form".to_owned()),
    }
}

/// Lower a formula to the SPARQL fragments that hold **iff the formula is violated** (`¬φ`),
/// the NNF of the negation: `¬Atom`/`¬∃` → `FILTER NOT EXISTS`, `¬¬` → positive, `¬∀` → the
/// existential witness of the negated body, `¬(a→b)` → `a ∧ ¬b`, `¬(a∨b)` →
/// `FILTER NOT EXISTS { {a} UNION {b} }`, `¬(a∧b)` → `{¬a} UNION {¬b}`.
fn lower_negative(f: &Formula, focus: &str) -> Result<Vec<String>, String> {
    match f {
        Formula::Atom { .. } => Ok(vec![format!(
            "FILTER NOT EXISTS {{ {} }}",
            constraint_atom(f, focus)?
        )]),
        Formula::Not(inner) => lower_positive(inner, focus),
        Formula::Exists { body, .. } => Ok(vec![format!(
            "FILTER NOT EXISTS {{ {} }}",
            lower_positive(body, focus)?.join(" ")
        )]),
        // `¬∀v.φ ≡ ∃v.¬φ`: the negated body is the existential witness.
        Formula::Forall { body, .. } => lower_negative(body, focus),
        // `¬(a → b) ≡ a ∧ ¬b`.
        Formula::Implies(a, b) => {
            let mut out = lower_positive(a, focus)?;
            out.extend(lower_negative(b, focus)?);
            Ok(out)
        }
        // `¬(a ∨ b) ≡ ¬a ∧ ¬b`: no solution to `(a UNION b)`.
        Formula::Or(fs) => {
            let mut branches = Vec::with_capacity(fs.len());
            for x in fs {
                branches.push(format!("{{ {} }}", lower_positive(x, focus)?.join(" ")));
            }
            Ok(vec![format!(
                "FILTER NOT EXISTS {{ {} }}",
                branches.join(" UNION ")
            )])
        }
        // `¬(a ∧ b) ≡ ¬a ∨ ¬b`.
        Formula::And(fs) => {
            let mut branches = Vec::with_capacity(fs.len());
            for x in fs {
                branches.push(format!("{{ {} }}", lower_negative(x, focus)?.join(" ")));
            }
            Ok(vec![branches.join(" UNION ")])
        }
        Formula::Iff(..) => Err("a biconditional has no SPARQL constraint-body form".to_owned()),
    }
}

/// The SPARQL WHERE group-graph-pattern selecting the focus nodes that VIOLATE `constraint`:
/// `guard(this) ∧ ¬φ(this)`, i.e. the guard lowered positively (binding `$this` and any
/// guard-scoped variable) followed by the NNF negation of the per-focus condition `φ`.
fn violation_where(constraint: &ConstraintIr) -> Result<String, String> {
    let Formula::Forall { vars, body } = &constraint.integrity else {
        return Err(
            "integrity must be a range-restricted ∀-guarded condition (the top node is not a ∀)"
                .to_owned(),
        );
    };
    let focus = vars
        .first()
        .ok_or_else(|| "integrity ∀ binds no focus variable".to_owned())?;
    let Formula::Implies(guard, phi) = body.as_ref() else {
        return Err(
            "integrity ∀ body must be a guarded implication (guard → condition)".to_owned(),
        );
    };
    let mut pats = lower_positive(guard, focus)?;
    pats.extend(lower_negative(phi, focus)?);
    Ok(pats.join(" "))
}

/// The `sh:target*` clause for a constraint's focus selector.
fn procedural_target_clause(t: &ShapeTarget) -> String {
    match t {
        ShapeTarget::Class(c) => format!("sh:targetClass {}", iri_term(c)),
        ShapeTarget::SubjectsOf(p) => format!("sh:targetSubjectsOf {}", iri_term(p)),
        ShapeTarget::ObjectsOf(p) => format!("sh:targetObjectsOf {}", iri_term(p)),
        ShapeTarget::ValueKeyed { predicate, value } => format!(
            "sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE {{ ?this {} {} }}\"\"\" ]",
            iri_term(predicate),
            iri_term(value)
        ),
    }
}

/// The gmeow-domain term the shape declares it `logic:formalizes` — the constraint's explicit
/// `formalizes` back-reference when present, else the focus-selector term (the class or
/// predicate the constraint ranges over), so every projected shape self-identifies its canon.
fn procedural_formalizes_term(c: &ConstraintIr) -> String {
    if let Some(f) = &c.formalizes {
        return f.clone();
    }
    match &c.target {
        ShapeTarget::Class(x) | ShapeTarget::SubjectsOf(x) | ShapeTarget::ObjectsOf(x) => x.clone(),
        ShapeTarget::ValueKeyed { predicate, .. } => predicate.clone(),
    }
}

/// Try to render one `logic:Constraint` block, or return the reason its integrity exceeds the
/// range-restricted guarded SPARQL-constraint fragment (so the caller carries it as flagged
/// residue rather than emitting a broken query).
fn try_project_block(c: &ConstraintIr) -> Result<String, String> {
    let where_body = violation_where(c)?;
    let shape = procedural_shape_iri(c);
    let formalizes = procedural_formalizes_term(c);
    let sev = c.severity.as_str();
    let target = procedural_target_clause(&c.target);
    let message_line = match &c.message {
        Some(m) => format!("        sh:message \"{}\" ;\n", esc_str(m)),
        None => String::new(),
    };
    Ok(format!(
        "<{shape}>\n    a sh:NodeShape ;\n    logic:formalizes <{formalizes}> ;\n    {target} ;\n    \
         sh:sparql [\n        a sh:SPARQLConstraint ;\n        sh:severity sh:{sev} ;\n{message_line}        \
         sh:select \"\"\"SELECT $this WHERE {{ {where_body} }}\"\"\" ;\n    ] .\n"
    ))
}

/// Project ONE [`ConstraintIr`] to its `sh:SPARQLConstraint` `sh:NodeShape` block (no header).
/// A constraint whose integrity exceeds the projectable fragment yields the empty string — it
/// is carried-and-flagged by [`procedural_constraint_residue`] in the loss ledger, never
/// emitted as a broken query. Reuse [`project_procedural_constraints`] for the whole-program,
/// header-carrying, byte-deterministic document.
pub fn project_procedural_constraint(c: &ConstraintIr) -> String {
    try_project_block(c).unwrap_or_default()
}

/// Project every [`ConstraintIr`] in `program` to a single whole-program procedural-constraint
/// Turtle document: the prefix header followed by one IRI-sorted, blank-node-free
/// `sh:SPARQLConstraint` NodeShape block per projectable constraint. A constraint-free program
/// (or one whose every constraint exceeds the fragment) yields the header alone, so a
/// constraint-free corpus stays byte-stable.
pub fn project_procedural_constraints(program: &LogicProgram) -> String {
    let mut blocks: Vec<(String, String)> = program
        .constraints
        .iter()
        .filter_map(|c| {
            try_project_block(c)
                .ok()
                .map(|b| (procedural_shape_iri(c), b))
        })
        .collect();
    blocks.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::from(PROCEDURAL_HEADER);
    for (_, block) in blocks {
        out.push('\n');
        out.push_str(&block);
    }
    out
}

/// The per-constraint SHACL-Core (SPARQL) loss-ledger residue for the `procedural-constraint`
/// target: one flagged note per `logic:Constraint` whose integrity exceeds the range-restricted
/// guarded fragment the `sh:SPARQLConstraint` surface can carry (full-FOL / aggregate-comparison
/// / variadic conditions), tagged with its [`crate::ir::FormulaShape`] set — carried-and-flagged
/// in the canonical logic: layer, never dropped in silence. A program whose every constraint is
/// projectable yields an empty vector.
pub fn procedural_constraint_residue(program: &LogicProgram) -> Vec<String> {
    program
        .constraints
        .iter()
        .filter_map(|c| match try_project_block(c) {
            Ok(_) => None,
            Err(reason) => {
                let tags = c
                    .integrity
                    .shape_tags()
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("+");
                Some(format!(
                    "logic:Constraint <{}> [{tags}] exceeds the range-restricted guarded SPARQL \
                     constraint fragment ({reason}); it is carried in the canonical logic: layer as \
                     flagged unsupported residue",
                    c.iri
                ))
            }
        })
        .collect()
}

/// The blanket ShEx residue for the `procedural-constraint` target: a `sh:SPARQLConstraint` is
/// a SPARQL query surface, and ShEx has no SPARQL-constraint form at all, so EVERY projected
/// procedural constraint is unsupported by ShEx. Emitted once per constraint so the drop is
/// disclosed and never silent. A constraint-free program yields an empty vector.
pub fn procedural_constraint_shex_residue(program: &LogicProgram) -> Vec<String> {
    program
        .constraints
        .iter()
        .map(|c| {
            format!(
                "logic:Constraint <{}> projects to a sh:SPARQLConstraint (a SPARQL query); ShEx has \
                 no SPARQL-constraint form (logic:unsupported), so it is carried in the canonical \
                 logic: layer",
                c.iri
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ConstraintProvenance, ShaclNodeKind, ShaclSeverity};

    fn prop(path: &str, comps: Vec<ConstraintComponent>) -> PropertyConstraintIr {
        PropertyConstraintIr::new(path, None, None, None, comps).unwrap()
    }

    fn shape(iri: &str, class: &str, props: Vec<PropertyConstraintIr>) -> ValidationShapeIr {
        ValidationShapeIr::new(iri, ShapeTarget::Class(class.to_owned()), props, None).unwrap()
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
        // The reifier component is a PROPERTY-shape condition (keyed to a `sh:path`): the native
        // SHACL 1.2 engine reads `sh:reifierShape`/`sh:reificationRequired` only from a
        // single-predicate property shape, so they must emit INSIDE the `sh:property [ … ]` block.
        let property = PropertyConstraintIr::new("https://ex/p", None, None, None, vec![])
            .unwrap()
            .with_reifier(Some("https://ex/ReifierShape".into()), true)
            .unwrap();
        let s = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![property],
            None,
        )
        .unwrap();
        let ttl = project_validation_shape_shacl(&s);
        assert!(
            ttl.contains("sh:reifierShape <https://ex/ReifierShape>"),
            "{ttl}"
        );
        assert!(ttl.contains("sh:reificationRequired true"), "{ttl}");
        assert!(ttl.contains("sh:path <https://ex/p>"), "{ttl}");
    }

    #[test]
    fn reifier_condition_is_rejected_on_an_inverse_path() {
        // The reifier component has no meaning on an inverse path (the engine hard-errors), so the
        // IR refuses to attach it there rather than emit a surface the engine rejects.
        let inverse = PropertyConstraintIr::new("https://ex/p", None, None, None, vec![])
            .unwrap()
            .inverted();
        assert!(
            inverse
                .with_reifier(Some("https://ex/R".into()), true)
                .is_err()
        );
    }

    #[test]
    fn standpoint_scope_is_carried_as_projection_residue() {
        // A standpoint-indexed shape (exercising the standpoint + reifier + reification fields
        // together) has no faithful SHACL/ShEx form: its scope must be recorded in the loss
        // ledger, never silently flattened to a universal shape.
        let sp = "https://blackcatinformatics.ca/gmeow/clinicalStandpoint";
        let property = PropertyConstraintIr::new("https://ex/p", None, None, None, vec![])
            .unwrap()
            .with_reifier(Some("https://ex/ReifierShape".into()), true)
            .unwrap();
        let s = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![property],
            Some(sp.into()),
        )
        .unwrap();
        assert_eq!(s.standpoint.as_deref(), Some(sp));
        let shacl = shacl_residue(&s);
        assert!(
            shacl
                .iter()
                .any(|r| r.contains(sp) && r.contains("standpoint")),
            "standpoint scope must be recorded in the SHACL residue: {shacl:?}"
        );
        // ShEx residue is a superset, so it inherits the standpoint residue too.
        let shex = shex_residue(&s);
        assert!(
            shex.iter().any(|r| r.contains(sp)),
            "standpoint scope must also be in the ShEx residue: {shex:?}"
        );
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
    fn ordinal_set_projects_sh_in_symbols_and_ledgers_integers() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/value",
                vec![ConstraintComponent::OrdinalSet {
                    pairs: vec![
                        (1, "https://ex/terminology/local/at0014".into()),
                        (2, "https://ex/terminology/local/at0015".into()),
                    ],
                }],
            )],
        );
        let ttl = project_validation_shape_shacl(&s);
        assert!(
            ttl.contains(
                "sh:in ( <https://ex/terminology/local/at0014> \
                 <https://ex/terminology/local/at0015> )"
            ),
            "{ttl}"
        );
        assert!(!ttl.contains(" 1 ") && !ttl.contains(" 2 "), "{ttl}");
        let residue = shacl_residue(&s);
        assert_eq!(
            residue.len(),
            1,
            "ordinal set must be ledgered: {residue:?}"
        );
        assert!(residue[0].contains("1"), "{residue:?}");
        assert!(residue[0].contains("2"), "{residue:?}");
    }

    #[test]
    fn datetime_pattern_emits_no_shacl_constraint_but_is_ledgered() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/value",
                vec![ConstraintComponent::DateTimePattern(
                    "yyyy-mm-ddTHH:MM:SS".into(),
                )],
            )],
        );
        // An openEHR validity pattern is a format template, not an XPath regex; emitting it as
        // `sh:pattern` would reject every valid datetime. Nothing is emitted; the meaning is
        // carried only in the loss ledger.
        let ttl = project_validation_shape_shacl(&s);
        assert!(
            !ttl.contains("sh:pattern"),
            "datetime pattern must NOT be emitted as a SHACL constraint: {ttl}"
        );
        let residue = shacl_residue(&s);
        assert_eq!(
            residue.len(),
            1,
            "datetime pattern must be ledgered: {residue:?}"
        );
        assert!(residue[0].contains("validity pattern"), "{residue:?}");
    }

    #[test]
    fn has_value_qualified_and_not_emit_shacl() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/p",
                vec![
                    ConstraintComponent::HasValue(ShapeValue::Iri("https://ex/fixed".into())),
                    ConstraintComponent::QualifiedValueShape {
                        shape: vec![ConstraintComponent::Class("https://ex/Q".into())],
                        min: Some(1),
                        max: None,
                    },
                    ConstraintComponent::Not(Box::new(ConstraintComponent::Class(
                        "https://ex/D".into(),
                    ))),
                ],
            )],
        );
        let ttl = project_validation_shape_shacl(&s);
        assert!(ttl.contains("sh:hasValue <https://ex/fixed>"), "{ttl}");
        assert!(
            ttl.contains("sh:qualifiedValueShape [ sh:class <https://ex/Q> ]"),
            "{ttl}"
        );
        assert!(ttl.contains("sh:qualifiedMinCount 1"), "{ttl}");
        assert!(ttl.contains("sh:not [ sh:class <https://ex/D> ]"), "{ttl}");
    }

    #[test]
    fn domain_range_targets_and_node_components_emit_shacl() {
        // rdfs:domain P C → targetSubjectsOf P + node-level sh:class C.
        let domain_shape = ValidationShapeIr::new(
            "https://ex/p-domain-shape",
            ShapeTarget::SubjectsOf("https://ex/p".into()),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(vec![ConstraintComponent::Class("https://ex/C".into())])
        .unwrap();
        let ttl = project_validation_shape_shacl(&domain_shape);
        assert!(ttl.contains("sh:targetSubjectsOf <https://ex/p>"), "{ttl}");
        assert!(ttl.contains("sh:class <https://ex/C>"), "{ttl}");
        // rdfs:range P C → targetObjectsOf P.
        let range_shape = ValidationShapeIr::new(
            "https://ex/p-range-shape",
            ShapeTarget::ObjectsOf("https://ex/p".into()),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(vec![ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri)])
        .unwrap();
        let ttl = project_validation_shape_shacl(&range_shape);
        assert!(ttl.contains("sh:targetObjectsOf <https://ex/p>"), "{ttl}");
        assert!(ttl.contains("sh:nodeKind sh:IRI"), "{ttl}");
    }

    #[test]
    fn inverse_path_severity_message_and_label_emit_shacl() {
        let p = PropertyConstraintIr::new(
            "https://ex/p",
            None,
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![],
        )
        .unwrap()
        .inverted()
        .with_severity(ShaclSeverity::Warning)
        .with_message("each object has at most one subject via p")
        .unwrap();
        let s = shape("https://ex/S", "https://ex/C", vec![p])
            .with_label("inverse-functional shape")
            .unwrap();
        let ttl = project_validation_shape_shacl(&s);
        assert!(
            ttl.contains("sh:path [ sh:inversePath <https://ex/p> ]"),
            "{ttl}"
        );
        assert!(ttl.contains("sh:severity sh:Warning"), "{ttl}");
        assert!(
            ttl.contains("sh:message \"each object has at most one subject via p\""),
            "{ttl}"
        );
        assert!(
            ttl.contains("rdf-schema#label> \"inverse-functional shape\""),
            "{ttl}"
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

#[cfg(test)]
mod shex_tests {
    use super::*;
    use crate::ir::ShaclNodeKind;

    fn prop(path: &str, comps: Vec<ConstraintComponent>) -> PropertyConstraintIr {
        PropertyConstraintIr::new(path, None, None, None, comps).unwrap()
    }

    fn shape(iri: &str, class: &str, props: Vec<PropertyConstraintIr>) -> ValidationShapeIr {
        ValidationShapeIr::new(iri, ShapeTarget::Class(class.to_owned()), props, None).unwrap()
    }

    #[test]
    fn quantity_interval_projects_shex_numeric_facets() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
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
        let shex = project_validation_shape_shex(&s);
        assert!(shex.contains("MININCLUSIVE 0"), "{shex}");
        assert!(shex.contains("MAXEXCLUSIVE 1000"), "{shex}");
        assert!(
            shex.contains("<http://www.w3.org/2001/XMLSchema#decimal>"),
            "{shex}"
        );
    }

    #[test]
    fn value_set_projects_shex_value_set() {
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
        let shex = project_validation_shape_shex(&s);
        assert!(
            shex.contains("[<https://ex/at0004> <https://ex/at0005>]"),
            "{shex}"
        );
    }

    #[test]
    fn cardinality_maps_to_shex_suffix() {
        assert_eq!(shex_cardinality(Some(0), Some(1)), " ?");
        assert_eq!(shex_cardinality(Some(1), None), " +");
        assert_eq!(shex_cardinality(Some(0), None), " *");
        assert_eq!(shex_cardinality(Some(1), Some(1)), "");
        assert_eq!(shex_cardinality(Some(2), Some(4)), " {2,4}");
    }

    #[test]
    fn node_kind_and_pattern_project_to_shex() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/name",
                vec![
                    ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal),
                    ConstraintComponent::Pattern {
                        regex: "^[A-Z]+$".into(),
                        flags: None,
                    },
                ],
            )],
        );
        let shex = project_validation_shape_shex(&s);
        assert!(
            shex.contains("LITERAL") || shex.contains("/^[A-Z]+$/"),
            "{shex}"
        );
        // The pattern residue is inherited from SHACL (regex dialect).
        assert!(
            shex_residue(&s).iter().any(|r| r.contains("regex-dialect")),
            "{:?}",
            shex_residue(&s)
        );
    }

    #[test]
    fn shex_regex_escapes_the_slash_delimiter() {
        // A `/` inside the pattern must be escaped as `\/`, else it prematurely closes the
        // ShExC `/…/` regex literal and corrupts the shape.
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/path",
                vec![ConstraintComponent::Pattern {
                    regex: "^a/b$".into(),
                    flags: None,
                }],
            )],
        );
        let shex = project_validation_shape_shex(&s);
        assert!(
            shex.contains("/^a\\/b$/"),
            "the `/` delimiter must be escaped: {shex}"
        );
    }

    #[test]
    fn shex_residue_is_strictly_larger_than_shacl_for_datetime() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/when",
                vec![ConstraintComponent::DateTimeRange {
                    min: Some("2020-01-01T00:00:00Z".into()),
                    max: None,
                    min_inclusive: true,
                    max_inclusive: false,
                }],
            )],
        );
        // SHACL Core expresses the datetime range (no residue); ShEx cannot (residue).
        assert!(
            shacl_residue(&s).is_empty(),
            "shacl: {:?}",
            shacl_residue(&s)
        );
        assert!(
            shex_residue(&s)
                .iter()
                .any(|r| r.contains("datetime range")),
            "shex: {:?}",
            shex_residue(&s)
        );
    }

    #[test]
    fn reifier_and_value_keyed_target_are_shex_residue() {
        let property = PropertyConstraintIr::new("https://ex/p", None, None, None, vec![])
            .unwrap()
            .with_reifier(Some("https://ex/R".into()), true)
            .unwrap();
        let s = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::ValueKeyed {
                predicate: "https://ex/kind".into(),
                value: "https://ex/Bp".into(),
            },
            vec![property],
            None,
        )
        .unwrap();
        let r = shex_residue(&s);
        assert!(r.iter().any(|x| x.contains("value-keyed target")), "{r:?}");
        assert!(r.iter().any(|x| x.contains("reifier")), "{r:?}");
    }

    #[test]
    fn has_value_projects_shex_singleton_value_set() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/p",
                vec![ConstraintComponent::HasValue(ShapeValue::Iri(
                    "https://ex/v".into(),
                ))],
            )],
        );
        let shex = project_validation_shape_shex(&s);
        assert!(shex.contains("[<https://ex/v>]"), "{shex}");
    }

    #[test]
    fn qualified_count_and_negation_are_shex_residue() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/p",
                vec![
                    ConstraintComponent::QualifiedValueShape {
                        shape: vec![ConstraintComponent::Class("https://ex/Q".into())],
                        min: Some(1),
                        max: None,
                    },
                    ConstraintComponent::Not(Box::new(ConstraintComponent::Class(
                        "https://ex/D".into(),
                    ))),
                ],
            )],
        );
        let r = shex_residue(&s);
        assert!(
            r.iter().any(|x| x.contains("qualified value-shape count")),
            "{r:?}"
        );
        assert!(r.iter().any(|x| x.contains("negated constraint")), "{r:?}");
    }

    #[test]
    fn residue_classification_is_pinned_and_nested_lossy_is_never_silently_dropped() {
        // Guards the exhaustive residue classifiers (shacl_component_residue / shex_residue): a
        // lossy component is flagged, a faithful one is not, and — critically — a lossy component
        // NESTED inside a `sh:not` is still flagged. Before the classifiers were made exhaustive
        // and recursive, the trailing `_ => {}` catch-all dropped a `Not(Pattern)` in silence,
        // defeating the loss ledger's "carried and flagged, never dropped" contract.
        let faithful = shape(
            "https://ex/faithful",
            "https://ex/C",
            vec![prop(
                "https://ex/p",
                vec![ConstraintComponent::Datatype(
                    "http://www.w3.org/2001/XMLSchema#string".into(),
                )],
            )],
        );
        assert!(
            shacl_residue(&faithful).is_empty(),
            "a faithful sh:datatype must carry no SHACL residue: {:?}",
            shacl_residue(&faithful)
        );

        let lossy = shape(
            "https://ex/lossy",
            "https://ex/C",
            vec![prop(
                "https://ex/p",
                vec![ConstraintComponent::Pattern {
                    regex: "^A".into(),
                    flags: None,
                }],
            )],
        );
        assert!(
            shacl_residue(&lossy)
                .iter()
                .any(|x| x.contains("sh:pattern")),
            "a top-level sh:pattern must be flagged: {:?}",
            shacl_residue(&lossy)
        );

        // The regression the exhaustiveness+recursion fix closes: a lossy component inside `sh:not`.
        let nested = shape(
            "https://ex/nested",
            "https://ex/C",
            vec![prop(
                "https://ex/p",
                vec![ConstraintComponent::Not(Box::new(
                    ConstraintComponent::Pattern {
                        regex: "^A".into(),
                        flags: None,
                    },
                ))],
            )],
        );
        assert!(
            shacl_residue(&nested)
                .iter()
                .any(|x| x.contains("sh:pattern")),
            "a Pattern nested inside sh:not must NOT be silently dropped: {:?}",
            shacl_residue(&nested)
        );
    }

    #[test]
    fn domain_target_node_component_is_shex_residue() {
        let s = ValidationShapeIr::new(
            "https://ex/p-domain-shape",
            ShapeTarget::SubjectsOf("https://ex/p".into()),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(vec![ConstraintComponent::Class("https://ex/C".into())])
        .unwrap();
        let shex = project_validation_shape_shex(&s);
        assert!(shex.contains("# targetSubjectsOf <https://ex/p>"), "{shex}");
        assert!(
            shex_residue(&s)
                .iter()
                .any(|x| x.contains("focus-node-level")),
            "{:?}",
            shex_residue(&s)
        );
    }

    #[test]
    fn empty_program_yields_empty_shex_document() {
        let prog = LogicProgram::new(vec![], vec![], vec![], None);
        assert_eq!(project_validation_shapes_shex(&prog), "");
    }

    #[test]
    fn facet_only_shex_omits_the_any_node_shapeatom() {
        // A property whose only ShEx-expressible constraint is a facet (here a
        // pattern) has no datatype/node-kind base. The facets alone ARE the ShExC
        // node constraint (`xsFacet+`); a leading `.` (the any-node shapeAtom)
        // would be a second, juxtaposed shapeAtom and the document would not parse.
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/path",
                vec![ConstraintComponent::Pattern {
                    regex: ".*".into(),
                    flags: None,
                }],
            )],
        );
        let shex = project_validation_shape_shex(&s);
        assert!(
            !shex.contains(". /"),
            "a facet-only constraint must not be prefixed with the `.` any shapeAtom: {shex}"
        );
        assert!(shex.contains("/.*/"), "{shex}");
    }
}

#[cfg(test)]
mod procedural_tests {
    use super::*;
    use crate::ir::{Formula, ShaclSeverity, Term};

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const WIDGET: &str = "https://ex/Widget";

    fn tvar(n: &str) -> Term {
        Term::Var(n.to_owned())
    }
    fn tiri(n: &str) -> Term {
        Term::Iri(n.to_owned())
    }
    fn atom(rel: &str, a: Term, b: Term) -> Formula {
        Formula::atom(tiri(rel), vec![a, b]).unwrap()
    }
    fn exists(v: &str, body: Formula) -> Formula {
        Formula::Exists {
            vars: vec![v.to_owned()],
            body: Box::new(body),
        }
    }
    fn forall(v: &str, body: Formula) -> Formula {
        Formula::Forall {
            vars: vec![v.to_owned()],
            body: Box::new(body),
        }
    }

    /// Wrap a per-focus condition `phi(this)` in the range-restricted guard
    /// `∀ this. rdf:type(this, ex:Widget) → phi`, and build the constraint.
    fn guarded(iri: &str, phi: Formula) -> ConstraintIr {
        let integrity = Formula::Forall {
            vars: vec!["this".to_owned()],
            body: Box::new(Formula::Implies(
                Box::new(atom(RDF_TYPE, tvar("this"), tiri(WIDGET))),
                Box::new(phi),
            )),
        };
        ConstraintIr::new(iri, integrity, ShaclSeverity::Violation, None).unwrap()
    }

    fn block(c: &ConstraintIr) -> String {
        let b = project_procedural_constraint(c);
        assert!(!b.is_empty(), "constraint {} must project a block", c.iri);
        b
    }

    #[test]
    fn every_block_is_a_sparql_constraint_nodeshape_carrying_formalizes() {
        let c = guarded(
            "https://ex/c2",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        );
        let b = block(&c);
        assert!(b.contains("a sh:NodeShape"), "{b}");
        assert!(b.contains("a sh:SPARQLConstraint"), "{b}");
        assert!(b.contains("sh:severity sh:Violation"), "{b}");
        // Self-identifies its canon: no explicit formalizes → the target class term.
        assert!(b.contains("logic:formalizes <https://ex/Widget>"), "{b}");
        assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
        assert!(b.contains("SELECT $this WHERE"), "{b}");
    }

    #[test]
    fn explicit_formalizes_overrides_the_target_term() {
        let c = guarded(
            "https://ex/cF",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        )
        .with_formalizes("https://ex/gmeow/SomeAxiom")
        .unwrap();
        assert!(
            block(&c).contains("logic:formalizes <https://ex/gmeow/SomeAxiom>"),
            "{}",
            block(&c)
        );
    }

    #[test]
    fn p1_choice_group_xor_lowers_to_a_union_under_not_exists() {
        // φ = (∃A ∧ ¬∃B) ∨ (¬∃A ∧ ∃B); ¬φ (both-or-neither) = NOT EXISTS { left UNION right }.
        let ea = || exists("a", atom("https://ex/hasA", tvar("this"), tvar("a")));
        let eb = || exists("b", atom("https://ex/hasB", tvar("this"), tvar("b")));
        let left = Formula::And(vec![ea(), Formula::Not(Box::new(eb()))]);
        let right = Formula::And(vec![Formula::Not(Box::new(ea())), eb()]);
        let c = guarded("https://ex/c1", Formula::Or(vec![left, right]));
        let b = block(&c);
        assert!(b.contains("FILTER NOT EXISTS"), "{b}");
        assert!(b.contains("UNION"), "{b}");
        assert!(
            b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
            "{b}"
        );
    }

    #[test]
    fn p2_guarded_implication_lowers_to_filter_not_exists_companion() {
        let c = guarded(
            "https://ex/c2",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        );
        assert!(
            block(&c).contains("FILTER NOT EXISTS { $this <https://ex/companion> ?c . }"),
            "{}",
            block(&c)
        );
    }

    #[test]
    fn p3_disjunctive_requiredness_lowers_to_not_exists_union() {
        // φ = ∃A ∨ ∃B; ¬φ = NOT EXISTS { {A} UNION {B} }.
        let c = guarded(
            "https://ex/c3",
            Formula::Or(vec![
                exists("a", atom("https://ex/hasA", tvar("this"), tvar("a"))),
                exists("b", atom("https://ex/hasB", tvar("this"), tvar("b"))),
            ]),
        );
        let b = block(&c);
        assert!(b.contains("FILTER NOT EXISTS {"), "{b}");
        assert!(b.contains("UNION"), "{b}");
    }

    #[test]
    fn p4_path_value_type_membership_lowers_to_a_path_and_not_exists_type() {
        // φ = ∀v. part(this,v) → Part(v); ¬φ = this part ?v . NOT EXISTS { ?v a Part }.
        let c = guarded(
            "https://ex/c4",
            forall(
                "v",
                Formula::Implies(
                    Box::new(atom("https://ex/part", tvar("this"), tvar("v"))),
                    Box::new(atom(RDF_TYPE, tvar("v"), tiri("https://ex/Part"))),
                ),
            ),
        );
        let b = block(&c);
        assert!(b.contains("$this <https://ex/part> ?v ."), "{b}");
        assert!(
            b.contains("FILTER NOT EXISTS { ?v a <https://ex/Part> . }"),
            "{b}"
        );
    }

    #[test]
    fn p5_cross_node_co_occurrence_lowers_to_a_path_and_nested_not_exists() {
        // φ = ∀o. linked(this,o) → ∃m. marker(o,m); ¬φ = this linked ?o . NOT EXISTS { ?o marker ?m }.
        let c = guarded(
            "https://ex/c5",
            forall(
                "o",
                Formula::Implies(
                    Box::new(atom("https://ex/linked", tvar("this"), tvar("o"))),
                    Box::new(exists("m", atom("https://ex/marker", tvar("o"), tvar("m")))),
                ),
            ),
        );
        let b = block(&c);
        assert!(b.contains("$this <https://ex/linked> ?o ."), "{b}");
        assert!(
            b.contains("FILTER NOT EXISTS { ?o <https://ex/marker> ?m . }"),
            "{b}"
        );
    }

    #[test]
    fn p7_forbidden_pattern_lowers_to_a_positive_witness_triple() {
        // φ = ¬∃b. forbidden(this,b); ¬φ = ∃b. forbidden(this,b) = $this <forbidden> ?b .
        let c = guarded(
            "https://ex/c7",
            Formula::Not(Box::new(exists(
                "b",
                atom("https://ex/forbidden", tvar("this"), tvar("b")),
            ))),
        );
        assert!(
            block(&c).contains("$this <https://ex/forbidden> ?b ."),
            "{}",
            block(&c)
        );
    }

    #[test]
    fn constraint_free_program_yields_a_byte_stable_header_only_doc() {
        let a = LogicProgram::new(vec![], vec![], vec![], None);
        let b = LogicProgram::new(vec![], vec![], vec![], None);
        let da = project_procedural_constraints(&a);
        let db = project_procedural_constraints(&b);
        assert_eq!(da, db, "a constraint-free program must be byte-stable");
        assert!(da.starts_with("# GENERATED"), "{da}");
        assert!(!da.contains("a sh:NodeShape"), "no shapes expected: {da}");
        assert!(da.contains("@prefix sh:"), "{da}");
    }

    #[test]
    fn whole_program_doc_is_iri_sorted_and_header_carrying() {
        let c_b = guarded(
            "https://ex/zeta",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        );
        let c_a = guarded(
            "https://ex/alpha",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        );
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c_b, c_a]);
        let doc = project_procedural_constraints(&prog);
        assert!(doc.starts_with("# GENERATED"), "{doc}");
        let ai = doc
            .find("AlphaProceduralConstraintShape")
            .expect("alpha shape");
        let zi = doc
            .find("ZetaProceduralConstraintShape")
            .expect("zeta shape");
        assert!(ai < zi, "shapes must be emitted in IRI-sorted order");
    }

    #[test]
    fn unsupported_integrity_is_carried_as_flagged_residue_not_emitted() {
        // A biconditional consequent exceeds the projectable NNF fragment.
        let c = guarded(
            "https://ex/cIff",
            Formula::Iff(
                Box::new(atom("https://ex/p", tvar("this"), tiri("https://ex/x"))),
                Box::new(atom("https://ex/q", tvar("this"), tiri("https://ex/y"))),
            ),
        );
        assert!(
            project_procedural_constraint(&c).is_empty(),
            "an unsupported constraint must not emit a block"
        );
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        let residue = procedural_constraint_residue(&prog);
        assert_eq!(residue.len(), 1, "{residue:?}");
        assert!(
            residue[0].contains("exceeds the range-restricted guarded SPARQL constraint fragment"),
            "{residue:?}"
        );
        // The whole-program doc drops it (header-only) — carried in the ledger, never emitted.
        assert!(
            !project_procedural_constraints(&prog).contains("a sh:NodeShape"),
            "the unsupported constraint must not reach the document"
        );
    }

    #[test]
    fn every_constraint_gets_a_blanket_shex_unsupported_note() {
        let c = guarded(
            "https://ex/c2",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        );
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        let shex = procedural_constraint_shex_residue(&prog);
        assert_eq!(shex.len(), 1, "{shex:?}");
        assert!(
            shex[0].contains("ShEx has no SPARQL-constraint form"),
            "{shex:?}"
        );
    }

    #[test]
    fn native_engine_flags_planted_violations_and_passes_clean_data() {
        use purrdf::shapes::engine::validate_graphs;

        // Two constraints over ex:Widget: (A) every widget must have a companion
        // (guarded implication), (B) a widget must not carry a `forbidden` edge.
        let a = guarded(
            "https://ex/mustHaveCompanion",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        );
        let b = guarded(
            "https://ex/noForbidden",
            Formula::Not(Box::new(exists(
                "b",
                atom("https://ex/forbidden", tvar("this"), tvar("b")),
            ))),
        );
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![a, b]);
        let shapes_ttl = project_procedural_constraints(&prog);

        // N-Triples data: one clean widget, one violating A (no companion), one violating B.
        let data = "\
<https://ex/goodW> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://ex/Widget> .\n\
<https://ex/goodW> <https://ex/companion> <https://ex/c0> .\n\
<https://ex/badNoCompanion> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://ex/Widget> .\n\
<https://ex/badForbidden> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://ex/Widget> .\n\
<https://ex/badForbidden> <https://ex/companion> <https://ex/c1> .\n\
<https://ex/badForbidden> <https://ex/forbidden> <https://ex/f0> .\n";

        let report = validate_graphs(data, &shapes_ttl).expect("validate");
        let flagged: Vec<String> = report
            .results
            .iter()
            .map(|r| r.focus_node.to_string())
            .collect();
        for bad in ["badNoCompanion", "badForbidden"] {
            assert!(
                flagged.iter().any(|f| f.contains(bad)),
                "the {bad} violation must be flagged; flagged: {flagged:?}"
            );
        }
        assert!(
            !flagged.iter().any(|f| f.contains("goodW")),
            "the clean widget must NOT be flagged; flagged: {flagged:?}"
        );
    }
}
