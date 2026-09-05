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

use gmeow_errors::Diag;

use crate::ir::{
    AggregateBalance, AggregateComparison, AggregateRhs, ConstraintComponent, ConstraintIr,
    Formula, JoinAggregate, LogicProgram, PropertyConstraintIr, ShaclNodeKind, ShapeTarget,
    ShapeValue, Term, ValidationShapeIr,
};

use super::sparql_lower::{sparql_literal, sparql_predicate};

/// Build a projection-grade [`Diag`] (the sole first-party error type — the Phase-6 Diag
/// substrate) recording why a constraint's integrity exceeds the projectable SPARQL fragment.
/// The message is surfaced in the loss-ledger residue, never dropped in silence.
fn proj_err(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::Projection {
        detail: detail.into(),
    })
}

/// The `logic:` comparison / node-kind relations the procedural-constraint fragment lowers to a
/// SPARQL `FILTER` rather than a triple pattern. A binary comparison filters two already-bound
/// terms (the cross-node co-occurrence / inequality / ordering pattern); a unary node-kind test
/// filters one bound term. They are recognized here (not in a second Formula→SPARQL lowering) so a
/// `logic:Constraint` can express `?a = ?b`, a numeric bound `?a >= ?b`, or a node-kind restriction
/// `isIRI(?a)`. A comparison / node-kind atom binds nothing, so it never introduces a new variable
/// that must be triple-bound.
const LOGIC_TERM_EQUAL: &str = "https://blackcatinformatics.ca/logic/termEqual";
const LOGIC_TERM_DISTINCT: &str = "https://blackcatinformatics.ca/logic/termDistinct";
const LOGIC_TERM_LESS: &str = "https://blackcatinformatics.ca/logic/termLess";
const LOGIC_TERM_LESS_EQUAL: &str = "https://blackcatinformatics.ca/logic/termLessEqual";
const LOGIC_TERM_GREATER: &str = "https://blackcatinformatics.ca/logic/termGreater";
const LOGIC_TERM_GREATER_EQUAL: &str = "https://blackcatinformatics.ca/logic/termGreaterEqual";
const LOGIC_TERM_IS_IRI: &str = "https://blackcatinformatics.ca/logic/termIsIri";
const LOGIC_TERM_IS_LITERAL: &str = "https://blackcatinformatics.ca/logic/termIsLiteral";
const LOGIC_TERM_IS_BLANK_OR_IRI: &str = "https://blackcatinformatics.ca/logic/termIsBlankOrIri";
/// The `logic:` value-set membership relation `termIn(x, m1, m2, …)`, lowered to a SPARQL
/// `FILTER ( x IN (m1, m2, …) )` (negated: `NOT IN`). The first argument is the tested term; every
/// remaining argument is a set member (an IRI or a data literal). It lets a `logic:Constraint`
/// express a `sh:in`-style enumerated-value restriction the flat triple fragment cannot.
const LOGIC_TERM_IN: &str = "https://blackcatinformatics.ca/logic/termIn";
/// The `logic:` string-prefix relation `termStrStarts(x, "prefix")`, lowered to a SPARQL
/// `FILTER ( STRSTARTS(STR(x), 'prefix') )` (negated: `!STRSTARTS(…)`). The second argument is the
/// literal prefix. It expresses a `STRSTARTS`/`sh:pattern`-anchored string test over a bound term.
const LOGIC_TERM_STR_STARTS: &str = "https://blackcatinformatics.ca/logic/termStrStarts";
/// The `logic:` regular-expression relation `termRegex(x, "pattern")`, lowered to a SPARQL
/// `FILTER ( REGEX(STR(x), 'pattern') )` (negated: `!REGEX(…)`). The second argument is the literal
/// regex. It expresses a `sh:pattern`-style lexical match over a bound term.
const LOGIC_TERM_REGEX: &str = "https://blackcatinformatics.ca/logic/termRegex";
/// The `logic:` language-tag introspection relation `termLangMatches(x, "pattern")`, lowered to a
/// case-insensitive SPARQL `FILTER ( REGEX(LANG(x), 'pattern', 'i') )` (negated: `!REGEX(…)`). It
/// mirrors [`LOGIC_TERM_REGEX`] but matches against the value's LANGUAGE TAG (`LANG(x)`) rather than
/// its lexical form (`STR(x)`) — the term the private-use language-tag convention needs.
const LOGIC_TERM_LANG_MATCHES: &str = "https://blackcatinformatics.ca/logic/termLangMatches";
/// The `logic:` language-tag presence relation `termHasLang(x)`, lowered to a unary SPARQL
/// `FILTER ( LANG(x) != "" )` (negated: `LANG(x) = ""`). The companion to [`LOGIC_TERM_LANG_MATCHES`]:
/// it restricts a language-tag check to genuinely TAGGED literals, so a plain / typed literal (whose
/// `LANG` is the empty string) is never swept into a tag-pattern violation.
const LOGIC_TERM_HAS_LANG: &str = "https://blackcatinformatics.ca/logic/termHasLang";
/// The `logic:` transitive-reachability relation `transitiveReach(subject, pathPredicate, target)`,
/// lowered to a SPARQL one-or-more property path `subject <pathPredicate>+ target .`. The middle
/// argument is the path predicate IRI (not a bound term); the outer two are subject / object terms.
/// It lets a `logic:Constraint` express a transitive walk (subclass-chain membership, a dependency
/// cycle) the flat triple-pattern fragment cannot.
const LOGIC_TRANSITIVE_REACH: &str = "https://blackcatinformatics.ca/logic/transitiveReach";
/// The `logic:` arithmetic-sum relation `termSum(result, a, b)`, lowered to a SPARQL
/// `BIND ( ( a + b ) AS result )`. The first argument is the (fresh) result variable the sum binds;
/// the remaining two are the summed terms (bound variables or numeric literals). It lets a
/// `logic:Constraint` compute a derived quantity (`p + q`) and then compare it to another property
/// (via an existing comparison relation such as `termDistinct`) — the metric-signature
/// dimension-count invariant the flat triple fragment cannot express.
const LOGIC_TERM_SUM: &str = "https://blackcatinformatics.ca/logic/termSum";
/// The `logic:` variable-predicate link relation `linkVia(subject, predicateVar, object)`, lowered
/// to a SPARQL triple `subject ?predicateVar object .` whose PREDICATE slot is a bound variable
/// (`args[1]` must be a variable). A `Formula::atom` forbids a variable in relation position, so a
/// variable-predicate pattern (any edge out of the focus, whose predicate is then filtered by
/// namespace, or whose object is then typed) is carried here as a dedicated relation and lowered by
/// a dedicated projector arm rather than as an ordinary atom.
const LOGIC_LINK_VIA: &str = "https://blackcatinformatics.ca/logic/linkVia";
/// The `logic:` direct-instance guard relation `directType(this, C)` — a guard-only marker that
/// range-restricts the focus to the DIRECT instances of `C` (a [`ShapeTarget::DirectClass`]). It is
/// consumed by the target derivation and by the `sh:SPARQLTarget` clause (which does the
/// subclass-excluding selection); it has no data-triple form, so it is STRIPPED from the violation
/// `WHERE` body rather than lowered to a `$this <directType> <C>` triple that matches nothing.
const LOGIC_DIRECT_TYPE: &str = "https://blackcatinformatics.ca/logic/directType";
/// The `logic:` raw-sparql-target guard relation `sparqlTarget(this, "SELECT ?this WHERE { … }")`
/// — a guard-only marker whose literal second argument is the whole `sh:SPARQLTarget` select
/// ([`ShapeTarget::Sparql`]). Like `directType`, it selects the focus but has no data-triple form,
/// so it is STRIPPED from the violation `WHERE` body rather than lowered to a triple.
const LOGIC_SPARQL_TARGET: &str = "https://blackcatinformatics.ca/logic/sparqlTarget";

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
        // A disjunction (`owl:unionOf` → `sh:or ( [ … ] [ … ] )`): each branch is its own
        // `[ … ]` shape block, in the branches' canonical (content-key sorted) order.
        ConstraintComponent::Or(branches) => {
            let items = branches
                .iter()
                .map(|b| format!("[ {} ]", component_lines(b).join(" ; ")))
                .collect::<Vec<_>>()
                .join(" ");
            vec![format!("sh:or ( {items} )")]
        }
        // An exclusive disjunction (`owl:disjointUnionOf` → `sh:xone ( [ … ] [ … ] )`).
        ConstraintComponent::Xone(branches) => {
            let items = branches
                .iter()
                .map(|b| format!("[ {} ]", component_lines(b).join(" ; ")))
                .collect::<Vec<_>>()
                .join(" ");
            vec![format!("sh:xone ( {items} )")]
        }
        // A node-level property-alternatives disjunction (a class-level `rdfs:subClassOf
        // [ owl:unionOf ( [ owl:onProperty P ; owl:someValuesFrom owl:Thing ] … ) ]` axiom):
        // each branch is a whole property shape requiring its path with `sh:minCount 1`.
        ConstraintComponent::OrProperties(paths) => {
            let items = paths
                .iter()
                .map(|p| format!("[ sh:path {} ; sh:minCount 1 ]", iri_term(p)))
                .collect::<Vec<_>>()
                .join(" ");
            vec![format!("sh:or ( {items} )")]
        }
        // A per-property unique-language facet: at most one value per language tag.
        ConstraintComponent::UniqueLang => vec!["sh:uniqueLang true".to_owned()],
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
    if let Some(failure_class) = &shape.failure_class {
        pos.push(format!(
            "<https://blackcatinformatics.ca/gmeow/enforcesFailureClass> {}",
            iri_term(failure_class)
        ));
    }
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
        ShapeTarget::DirectClass(c) => pos.push(direct_class_target_clause(c)),
        ShapeTarget::Sparql(sel) => pos.push(format!(
            "sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"{sel}\"\"\" ]"
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
        // `sh:or` / `sh:xone` are faithful SHACL Core constructs; a lossy component nested in a
        // branch is flagged at branch depth (like the other wrappers, never silently dropped).
        ConstraintComponent::Or(branches) | ConstraintComponent::Xone(branches) => {
            for inner in branches {
                shacl_component_residue(path, inner, out);
            }
        }
        // Faithful in SHACL Core — no residue. Listed explicitly (not a `_` arm) so a NEW
        // component variant forces a faithful-or-residue decision at compile time.
        // `OrProperties` is the node-level `sh:or` over `[ sh:path P ; sh:minCount 1 ]`
        // branches — plain SHACL Core, fully faithful.
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
        | ConstraintComponent::HasValue(_)
        | ConstraintComponent::OrProperties(_)
        | ConstraintComponent::UniqueLang => {}
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
            | ConstraintComponent::OrProperties(_)
            | ConstraintComponent::LanguageIn(_)
            | ConstraintComponent::TerminologyBinding { .. }
            | ConstraintComponent::In(_)
            | ConstraintComponent::OrdinalSet { .. }
            | ConstraintComponent::HasValue(_)
            | ConstraintComponent::Not(_)
            // ShEx Core has alternation (`|`) but not exclusive-or; both are carried in the
            // canonical logic: layer rather than partially projected. Declared in shex_residue.
            | ConstraintComponent::Or(_)
            // `sh:uniqueLang` has no ShEx Core form; carried in the canonical logic: layer and
            // disclosed in shex_residue.
            | ConstraintComponent::UniqueLang
            | ConstraintComponent::Xone(_) => {}
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
        // A value-keyed / direct-instance / raw-sparql (SPARQL) target has no ShEx form;
        // shex_residue records it.
        ShapeTarget::ValueKeyed { .. } | ShapeTarget::DirectClass(_) | ShapeTarget::Sparql(_) => {}
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
                // ShEx Core has alternation but not exclusive-or; the disjunction (whether `sh:or`
                // or `sh:xone`) is carried whole in the canonical logic: layer, never partially
                // projected. Flagged at the wrapper level (like `Not`/`QualifiedValueShape`).
                ConstraintComponent::Or(_) => residue.push(format!(
                    "disjunction (sh:or) on {} is carried whole in the canonical logic: layer \
                     (no partial ShEx alternation is emitted)",
                    p.path
                )),
                ConstraintComponent::Xone(_) => residue.push(format!(
                    "exclusive disjunction (sh:xone) on {} has no ShEx Core form; carried in the \
                     canonical logic: layer",
                    p.path
                )),
                // A node-level property-alternatives disjunction never rides a property shape;
                // when a whole shape carries node-level components the ShEx projection already
                // flags them wholesale above. Flagged defensively at the wrapper level here.
                ConstraintComponent::OrProperties(_) => residue.push(format!(
                    "property-alternatives disjunction (sh:or over sh:path branches) on {} is \
                     carried whole in the canonical logic: layer",
                    p.path
                )),
                ConstraintComponent::UniqueLang => residue.push(format!(
                    "unique-language facet (sh:uniqueLang) on {} has no ShEx Core form; carried \
                     in the canonical logic: layer",
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

/// The namespace prefix of an IRI: everything up to and including the last `/` or `#`.
/// `.../math/FlagshipScenarioFailureClassConstraint` → `.../math/`.
fn namespace_of(iri: &str) -> &str {
    match iri.rfind(['/', '#']) {
        Some(idx) => &iri[..=idx],
        None => "",
    }
}

/// The deterministic shape IRI a constraint projects to (`{Name}ProceduralConstraintShape`),
/// minted in the constraint's OWN namespace so two constraints that share a local name across
/// namespaces (e.g. `lang:` and `math:` both declaring `FlagshipScenarioFailureClassConstraint`)
/// do not collide onto one RDF node — a collision would merge their `sh:targetClass`/`sh:sparql`
/// and mis-key one twin's findings.
fn procedural_shape_iri(c: &ConstraintIr) -> String {
    format!(
        "{}{}ProceduralConstraintShape",
        namespace_of(&c.iri),
        pascal(&c.iri)
    )
}

/// Render one FOL [`Term`] as a SPARQL subject/object token: the focus variable renders as
/// the SHACL pre-bound `$this`, any other variable keeps its `?name` form, an IRI is
/// angle-bracketed, and a data literal is single-quoted (with an optional `^^<datatype>`).
/// A sequence marker has no single-term SPARQL form and is refused (carried as residue).
fn constraint_term(t: &Term, focus: &str) -> gmeow_errors::Result<String> {
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
        Term::SequenceMarker(n) => Err(proj_err(format!(
            "sequence marker ...{n} has no single-term SPARQL triple form"
        ))),
        // A compound function term does not name a single node the way a variable/IRI/literal
        // does; SPARQL is function-free over the graph, so it has no single-term triple token.
        // Flattening an application into a reifier-node join is a lowering, not a rendering, so
        // it is refused here (carried as residue) rather than silently mis-projected.
        Term::App { symbol, .. } => Err(proj_err(format!(
            "compound function term {symbol}(…) has no single-term SPARQL triple form"
        ))),
    }
}

/// The SPARQL relational operator a binary comparison relation lowers to in POSITIVE position,
/// paired with the operator of its logical NEGATION (used when the atom appears under a `¬`, so the
/// NNF lowering never wraps a `FILTER` in a nonsensical `FILTER NOT EXISTS`). `None` for a relation
/// that is not a comparison.
fn binary_comparison_ops(pred: &str) -> Option<(&'static str, &'static str)> {
    match pred {
        LOGIC_TERM_EQUAL => Some(("=", "!=")),
        LOGIC_TERM_DISTINCT => Some(("!=", "=")),
        LOGIC_TERM_LESS => Some(("<", ">=")),
        LOGIC_TERM_LESS_EQUAL => Some(("<=", ">")),
        LOGIC_TERM_GREATER => Some((">", "<=")),
        LOGIC_TERM_GREATER_EQUAL => Some((">=", "<")),
        _ => None,
    }
}

/// The SPARQL lowering of a two-argument string relation, as `(outer func, inner accessor, optional
/// flags)`: `STRSTARTS(STR(x), pat)` / `REGEX(STR(x), pat)` / `REGEX(LANG(x), pat, 'i')`. The inner
/// accessor selects WHAT of the term is tested — its lexical form (`STR`) or its language tag
/// (`LANG`). `None` for a relation that is not a string test.
fn string_test_func(pred: &str) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    match pred {
        LOGIC_TERM_STR_STARTS => Some(("STRSTARTS", "STR", None)),
        LOGIC_TERM_REGEX => Some(("REGEX", "STR", None)),
        LOGIC_TERM_LANG_MATCHES => Some(("REGEX", "LANG", Some("i"))),
        _ => None,
    }
}

/// The SPARQL node-kind test a unary node-kind relation lowers to over the bound term `x`, paired
/// with its logical NEGATION. `None` for a relation that is not a node-kind test.
fn unary_nodekind_exprs(pred: &str, x: &str) -> Option<(String, String)> {
    match pred {
        LOGIC_TERM_IS_IRI => Some((format!("isIRI({x})"), format!("!isIRI({x})"))),
        LOGIC_TERM_IS_LITERAL => Some((format!("isLiteral({x})"), format!("!isLiteral({x})"))),
        LOGIC_TERM_IS_BLANK_OR_IRI => Some((
            format!("( isIRI({x}) || isBlank({x}) )"),
            format!("!( isIRI({x}) || isBlank({x}) )"),
        )),
        LOGIC_TERM_HAS_LANG => Some((format!("LANG({x}) != \"\""), format!("LANG({x}) = \"\""))),
        _ => None,
    }
}

/// Render a comparison / node-kind / string / value-set atom as a BARE SPARQL boolean expression
/// (no `FILTER ( … )` wrapper), in POSITIVE (`negate = false`) or NEGATED (`negate = true`) form.
/// Returns `None` when the atom's relation is not a recognized filter relation (so the caller falls
/// back to the triple-pattern lowering). This is the join-able unit the compound [`filter_expr`]
/// combiner glues with `&&` / `||`, and the wrapped [`try_filter_atom`] presents as one `FILTER`.
fn filter_atom_expr(
    f: &Formula,
    focus: &str,
    negate: bool,
) -> Option<gmeow_errors::Result<String>> {
    let Formula::Atom { relation, args } = f else {
        return None;
    };
    let Term::Iri(pred) = relation else {
        return None;
    };
    if let Some((pos, neg)) = binary_comparison_ops(pred) {
        if args.len() != 2 {
            return Some(Err(proj_err(format!(
                "comparison relation <{pred}> has arity {}, a binary comparison needs two operands",
                args.len()
            ))));
        }
        let op = if negate { neg } else { pos };
        let s = match constraint_term(&args[0], focus) {
            Ok(s) => s,
            Err(e) => return Some(Err(e)),
        };
        let o = match constraint_term(&args[1], focus) {
            Ok(o) => o,
            Err(e) => return Some(Err(e)),
        };
        return Some(Ok(format!("{s} {op} {o}")));
    }
    // Value-set membership `termIn(x, m1, m2, …)` → `x IN (m1, …)` (negated: `NOT IN`).
    if pred == LOGIC_TERM_IN {
        if args.len() < 2 {
            return Some(Err(proj_err(format!(
                "termIn has arity {}, it needs a tested term and at least one set member",
                args.len()
            ))));
        }
        let x = match constraint_term(&args[0], focus) {
            Ok(x) => x,
            Err(e) => return Some(Err(e)),
        };
        let mut members = Vec::with_capacity(args.len() - 1);
        for m in &args[1..] {
            match constraint_term(m, focus) {
                Ok(m) => members.push(m),
                Err(e) => return Some(Err(e)),
            }
        }
        let kw = if negate { "NOT IN" } else { "IN" };
        return Some(Ok(format!("{x} {kw} ({})", members.join(", "))));
    }
    // String tests `termStrStarts(x, "p")` / `termRegex(x, "p")` → `[!]STRSTARTS|REGEX(STR(x), 'p')`,
    // and `termLangMatches(x, "p")` → `[!]REGEX(LANG(x), 'p', 'i')` (against the value's language tag).
    if let Some((func, inner, flags)) = string_test_func(pred) {
        if args.len() != 2 {
            return Some(Err(proj_err(format!(
                "string relation <{pred}> has arity {}, it needs a tested term and a literal pattern",
                args.len()
            ))));
        }
        let x = match constraint_term(&args[0], focus) {
            Ok(x) => x,
            Err(e) => return Some(Err(e)),
        };
        let Term::Literal { lexical, .. } = &args[1] else {
            return Some(Err(proj_err(format!(
                "string relation <{pred}> argument 1 must be a literal pattern"
            ))));
        };
        let pat = sparql_literal(lexical);
        let flags_arg = flags
            .map(|f| format!(", {}", sparql_literal(f)))
            .unwrap_or_default();
        let expr = format!("{func}({inner}({x}), {pat}{flags_arg})");
        let expr = if negate { format!("!{expr}") } else { expr };
        return Some(Ok(expr));
    }
    if args.len() == 1 {
        let x = match constraint_term(&args[0], focus) {
            Ok(x) => x,
            Err(e) => return Some(Err(e)),
        };
        if let Some((pos, neg)) = unary_nodekind_exprs(pred, &x) {
            let expr = if negate { neg } else { pos };
            return Some(Ok(expr));
        }
    }
    None
}

/// Render a comparison / node-kind atom as a SPARQL `FILTER`, in POSITIVE (`negate = false`) or
/// NEGATED (`negate = true`) form. Returns `None` when the atom's relation is not a recognized
/// filter relation (so the caller falls back to the triple-pattern lowering).
fn try_filter_atom(f: &Formula, focus: &str, negate: bool) -> Option<gmeow_errors::Result<String>> {
    match filter_atom_expr(f, focus, negate)? {
        Ok(expr) => Some(Ok(format!("FILTER ( {expr} )"))),
        Err(e) => Some(Err(e)),
    }
}

/// Lower a formula built ENTIRELY of filter atoms (comparison / node-kind / string / value-set)
/// combined by `∧` / `∨` / `¬` to a single BARE SPARQL boolean expression — for `¬f` when
/// `negate` is set (De Morgan is pushed through the connectives so the negation stays a `FILTER`
/// expression, never a `FILTER NOT EXISTS` over a pattern that binds nothing). Returns `None` the
/// moment any leaf is NOT a filter atom (a triple pattern, a quantifier), so the caller keeps the
/// existing `UNION` / `FILTER NOT EXISTS` lowering for a disjunction that actually binds variables.
/// This is what lets a raw disjunction of bare filters (`?a < ?b ∨ ?c > ?d`) lower to one
/// `FILTER ( ?a < ?b || ?c > ?d )` instead of `{ FILTER(?a<?b) } UNION { FILTER(?c>?d) }` — UNION
/// arms that bind no focus and select nothing.
fn filter_expr(f: &Formula, focus: &str, negate: bool) -> Option<gmeow_errors::Result<String>> {
    // De Morgan: ∧ under ¬ becomes ∨ (and vice versa); the per-child negate flag flips.
    fn combine(
        parts: &[Formula],
        focus: &str,
        child_negate: bool,
        joiner: &str,
    ) -> Option<gmeow_errors::Result<String>> {
        let mut exprs = Vec::with_capacity(parts.len());
        for p in parts {
            match filter_expr(p, focus, child_negate)? {
                Ok(e) => exprs.push(format!("( {e} )")),
                Err(e) => return Some(Err(e)),
            }
        }
        Some(Ok(exprs.join(joiner)))
    }
    match f {
        Formula::Atom { .. } => filter_atom_expr(f, focus, negate),
        Formula::Not(inner) => filter_expr(inner, focus, !negate),
        Formula::And(fs) => {
            if negate {
                combine(fs, focus, true, " || ")
            } else {
                combine(fs, focus, false, " && ")
            }
        }
        Formula::Or(fs) => {
            if negate {
                combine(fs, focus, true, " && ")
            } else {
                combine(fs, focus, false, " || ")
            }
        }
        _ => None,
    }
}

/// Render one binary atomic predication as a SPARQL triple pattern `subj pred obj .`, OR a
/// comparison / node-kind atom as a `FILTER`. A non-binary, non-filter atom (unary or n ≥ 3) or a
/// sequence-marker argument has no direct SPARQL triple form and is refused so the constraint is
/// carried-and-flagged rather than emitted as a broken query.
fn constraint_atom(f: &Formula, focus: &str) -> gmeow_errors::Result<String> {
    if let Some(filter) = try_filter_atom(f, focus, false) {
        return filter;
    }
    let Formula::Atom { relation, args } = f else {
        return Err(proj_err("expected an atomic predication"));
    };
    let Term::Iri(pred) = relation else {
        return Err(proj_err("atom relation must be an IRI"));
    };
    // An arithmetic-sum atom lowers to a SPARQL `BIND ( ( a + b ) AS result )`; the first argument
    // is the result variable the sum binds, the other two are the summed terms.
    if pred == LOGIC_TERM_SUM {
        if args.len() != 3 {
            return Err(proj_err(format!(
                "termSum has arity {}, it needs (result, a, b)",
                args.len()
            )));
        }
        let Term::Var(_) = &args[0] else {
            return Err(proj_err("termSum result (argument 0) must be a variable"));
        };
        let result = constraint_term(&args[0], focus)?;
        let a = constraint_term(&args[1], focus)?;
        let b = constraint_term(&args[2], focus)?;
        return Ok(format!("BIND ( ( {a} + {b} ) AS {result} )"));
    }
    // A variable-predicate link atom lowers to a triple whose PREDICATE is a bound variable:
    // `subj ?predVar obj .`. The middle argument must be a variable (the predicate slot).
    if pred == LOGIC_LINK_VIA {
        if args.len() != 3 {
            return Err(proj_err(format!(
                "linkVia has arity {}, it needs (subject, predicateVar, object)",
                args.len()
            )));
        }
        let s = constraint_term(&args[0], focus)?;
        let Term::Var(pv) = &args[1] else {
            return Err(proj_err(
                "linkVia predicate (argument 1) must be a variable",
            ));
        };
        let o = constraint_term(&args[2], focus)?;
        return Ok(format!("{s} ?{pv} {o} ."));
    }
    // A transitive-reachability atom lowers to a one-or-more property path `subj <Q>+ obj .`; the
    // middle argument names the path predicate IRI (not a bound term).
    if pred == LOGIC_TRANSITIVE_REACH {
        if args.len() != 3 {
            return Err(proj_err(format!(
                "transitiveReach has arity {}, it needs (subject, pathPredicate, target)",
                args.len()
            )));
        }
        let s = constraint_term(&args[0], focus)?;
        let Term::Iri(path) = &args[1] else {
            return Err(proj_err(
                "transitiveReach path predicate (argument 1) must be an IRI",
            ));
        };
        let o = constraint_term(&args[2], focus)?;
        return Ok(format!("{s} <{path}>+ {o} ."));
    }
    if args.len() != 2 {
        return Err(proj_err(format!(
            "atom <{pred}> has arity {}, only a binary atom lowers to a SPARQL triple pattern",
            args.len()
        )));
    }
    let s = constraint_term(&args[0], focus)?;
    let o = constraint_term(&args[1], focus)?;
    if pred == RDF_TYPE {
        // A body-position `rdf:type` atom — a NON-focus class check (`?v a C`), a VARIABLE-class
        // check (`$this a ?openClass`), or a focus CO-TYPING check in the consequent (a Frege-style
        // disjointness `$this a lang:Form`). The subclass-EXCLUDING focus guard `rdf:type(this, C)`
        // never reaches here: it is derived into `sh:targetClass C` (engine-closed) and stripped by
        // `strip_direct_type_guard`. Everything that DOES reach here lowers into a `sh:sparql` /
        // `sh:SPARQLTarget` body, which the SHACL engine does NOT subclass-close, so it must close
        // the asserted `rdfs:subClassOf` chain itself with the `a/<subClassOf>*` property path
        // (the same idiom the OWL-disjointness / conditional-range projections use). The `*`
        // zero-or-more length includes the exact-type match, so a class with no subclasses behaves
        // identically to the plain `a`. This makes the projected body verdict-equivalent to the
        // retired whole-dataset `rdf:type` closure pass for positive and `FILTER NOT EXISTS` atoms
        // alike.
        return Ok(format!("{s} a/<{RDFS_SUBCLASS_OF}>* {o} ."));
    }
    let p = sparql_predicate(pred);
    Ok(format!("{s} {p} {o} ."))
}

/// Lower a formula to the SPARQL group-graph-pattern fragments that hold **iff the formula
/// is satisfied** (for the pre-bound focus `$this`). The reused NNF/BGP/`FILTER NOT EXISTS`
/// machinery: a positive atom is a triple pattern, an existential is its (BGP-existential)
/// body, a disjunction is a `UNION`, a negation flips to [`lower_negative`]. A universal in
/// positive position has no bounded SPARQL BGP form (it would need a double negation over an
/// open domain) and is refused so the constraint is carried-and-flagged.
fn lower_positive(f: &Formula, focus: &str) -> gmeow_errors::Result<Vec<String>> {
    match f {
        Formula::Atom { .. } => Ok(vec![constraint_atom(f, focus)?]),
        Formula::And(fs) => {
            let mut out = Vec::new();
            for x in fs {
                out.extend(lower_positive(x, focus)?);
            }
            // A `logic:and`'s conjuncts have no authored order (RDF is a set), but a SPARQL
            // `BIND ( … AS ?v )` must follow the triples that bind its inputs (and precede any
            // `FILTER` reading `?v`). When a `BIND` is present, reorder deterministically —
            // patterns, then binds, then filters. Absent a `BIND` the order is untouched, so every
            // existing constraint stays byte-identical (a group's `FILTER`s are group-scoped).
            if out.iter().any(|f| f.trim_start().starts_with("BIND")) {
                out.sort_by_key(|f| {
                    let t = f.trim_start();
                    if t.starts_with("BIND") {
                        1
                    } else if t.starts_with("FILTER") {
                        2
                    } else {
                        0
                    }
                });
            }
            Ok(out)
        }
        Formula::Or(fs) => {
            // A disjunction of BARE filters (no triple binds anything) must combine into one
            // `FILTER ( a || b )`, not `{ FILTER(a) } UNION { FILTER(b) }` — the latter's arms bind
            // no focus and select nothing. Fall back to `UNION` only when an arm binds a pattern.
            if let Some(expr) = filter_expr(f, focus, false) {
                return Ok(vec![format!("FILTER ( {} )", expr?)]);
            }
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
        Formula::Forall { .. } => Err(proj_err(
            "a universal in positive position has no bounded SPARQL BGP form (it would require a \
             double negation over an open domain)",
        )),
        Formula::Iff(..) => Err(proj_err(
            "a biconditional has no SPARQL constraint-body form",
        )),
    }
}

/// Lower a formula to the SPARQL fragments that hold **iff the formula is violated** (`¬φ`),
/// the NNF of the negation: `¬Atom`/`¬∃` → `FILTER NOT EXISTS`, `¬¬` → positive, `¬∀` → the
/// existential witness of the negated body, `¬(a→b)` → `a ∧ ¬b`, `¬(a∨b)` →
/// `FILTER NOT EXISTS { {a} UNION {b} }`, `¬(a∧b)` → `{¬a} UNION {¬b}`.
fn lower_negative(f: &Formula, focus: &str) -> gmeow_errors::Result<Vec<String>> {
    match f {
        // A comparison / node-kind atom negates to its negated `FILTER` (`?a >= ?b` ↦ `?a < ?b`,
        // `isIRI(?v)` ↦ `!isIRI(?v)`), NOT a `FILTER NOT EXISTS` over a triple that binds nothing.
        Formula::Atom { .. } if try_filter_atom(f, focus, true).is_some() => {
            Ok(vec![try_filter_atom(f, focus, true).expect("filter atom")?])
        }
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
        // `¬(a ∨ b) ≡ ¬a ∧ ¬b`: no solution to `(a UNION b)`. A pure-filter disjunction negates to
        // one `FILTER ( !(…) )` (De Morgan), never a `FILTER NOT EXISTS` over binding-free arms.
        Formula::Or(fs) => {
            if let Some(expr) = filter_expr(f, focus, true) {
                return Ok(vec![format!("FILTER ( {} )", expr?)]);
            }
            let mut branches = Vec::with_capacity(fs.len());
            for x in fs {
                branches.push(format!("{{ {} }}", lower_positive(x, focus)?.join(" ")));
            }
            Ok(vec![format!(
                "FILTER NOT EXISTS {{ {} }}",
                branches.join(" UNION ")
            )])
        }
        // `¬(a ∧ b ∧ …)`. A pure-filter conjunction negates to one `FILTER ( … || … )`.
        Formula::And(fs) => {
            if let Some(expr) = filter_expr(f, focus, true) {
                return Ok(vec![format!("FILTER ( {} )", expr?)]);
            }
            // `¬(c1 ∧ … ∧ cn) ≡ ¬c1 ∨ … ∨ ¬cn`. When EVERY negated conjunct is a
            // self-scoped pattern group — one that re-binds `$this` through its own positive
            // triple (a negated implication `{ a . ¬b }`, the ∀-of-implications shape such as
            // the math:LimitResult value/outcome-agreement law) — lower the disjunction as a
            // UNION of those groups: each arm binds `$this`, so the violation is checked per
            // focus node. When ANY negated conjunct is a bare FILTER (`¬atom = FILTER NOT
            // EXISTS { $this p o }`, which binds no variable), a UNION arm would be UNSCOPED —
            // SPARQL evaluates `Union(¬a, ¬b, …)` independently of the guard it joins, so any
            // sibling node satisfying that one conjunct clears it (the
            // orgbook_notability_mutation regression). In that case negate the WHOLE
            // conjunction as ONE scoped `FILTER NOT EXISTS { pos(a) pos(b) … }`, which keeps
            // `$this` bound (the FILTER rides the guard's group).
            let negated: Vec<Vec<String>> = fs
                .iter()
                .map(|c| lower_negative(c, focus))
                .collect::<gmeow_errors::Result<_>>()?;
            let all_scoped = negated.iter().all(|group| {
                group
                    .iter()
                    .any(|line| !line.trim_start().starts_with("FILTER"))
            });
            if all_scoped {
                let arms: Vec<String> = negated
                    .iter()
                    .map(|group| format!("{{ {} }}", group.join(" ")))
                    .collect();
                Ok(vec![arms.join(" UNION ")])
            } else {
                Ok(vec![format!(
                    "FILTER NOT EXISTS {{ {} }}",
                    lower_positive(f, focus)?.join(" ")
                )])
            }
        }
        Formula::Iff(..) => Err(proj_err(
            "a biconditional has no SPARQL constraint-body form",
        )),
    }
}

/// The `rdf:type` IRI — the relation of a class-membership guard atom `rdf:type(this, C)`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The `rdfs:subClassOf` IRI — the edge a body-position `rdf:type` atom closes over with the
/// `a/<subClassOf>*` property path so the projected `sh:sparql`/`sh:SPARQLTarget` body is
/// subclass-aware without a whole-dataset `rdf:type` pre-materialization pass.
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// Is `f` a guard-only selection marker (`directType` / `sparqlTarget`) — a relation that selects
/// the focus via the `sh:target` clause and has NO data-triple form (so it is stripped from the
/// violation `WHERE` body)?
fn is_marker_atom(f: &Formula) -> bool {
    matches!(f, Formula::Atom { relation: Term::Iri(p), .. }
        if p == LOGIC_DIRECT_TYPE || p == LOGIC_SPARQL_TARGET)
}

/// Is `f` the class-membership guard atom `rdf:type(focus, C)` (`C` an IRI)? It derives a
/// [`ShapeTarget::Class`] `sh:targetClass C`, which ALREADY selects the focus and — unlike a plain
/// BGP triple — follows `rdfs:subClassOf`. Re-emitting `$this a C` in the violation `WHERE` would
/// therefore wrongly exclude the subclass instances the `sh:targetClass` selects (e.g. a
/// `GroupHomomorphism` under a `math:Homomorphism`-targeted constraint), so it is stripped like a
/// selection marker.
fn is_class_guard_atom(f: &Formula, focus: &str) -> bool {
    matches!(f, Formula::Atom { relation: Term::Iri(p), args }
        if p == RDF_TYPE
            && args.len() == 2
            && matches!(&args[0], Term::Var(v) if v == focus)
            && matches!(&args[1], Term::Iri(_)))
}

/// Whether `f` is a guard atom that is realized by the `sh:target*` clause and so must NOT be
/// re-lowered into the violation `WHERE`: a `directType`/`sparqlTarget` marker (no data form) or the
/// `rdf:type(focus, C)` class-membership atom (subsumed by `sh:targetClass`, which follows
/// `rdfs:subClassOf`).
fn is_selector_atom(f: &Formula, focus: &str) -> bool {
    is_marker_atom(f) || is_class_guard_atom(f, focus)
}

/// Strip the target-selector atoms from a guard. `None` ⇒ the guard has no selector atom (lower it
/// unchanged); `Some(None)` ⇒ the guard was ONLY selector atoms (lower nothing); `Some(Some(rest))`
/// ⇒ the non-selector guard atoms that remain (a conjunction, or a single atom).
fn strip_direct_type_guard(guard: &Formula, focus: &str) -> Option<Option<Formula>> {
    match guard {
        f if is_selector_atom(f, focus) => Some(None),
        Formula::And(fs) if fs.iter().any(|f| is_selector_atom(f, focus)) => {
            let rest: Vec<Formula> = fs
                .iter()
                .filter(|f| !is_selector_atom(f, focus))
                .cloned()
                .collect();
            Some(match rest.len() {
                0 => None,
                1 => Some(rest.into_iter().next().expect("one")),
                _ => Some(Formula::And(rest)),
            })
        }
        _ => None,
    }
}

/// The SPARQL WHERE group-graph-pattern selecting the focus nodes that VIOLATE `constraint`:
/// `guard(this) ∧ ¬φ(this)`, i.e. the guard lowered positively (binding `$this` and any
/// guard-scoped variable) followed by the NNF negation of the per-focus condition `φ`.
fn violation_where(constraint: &ConstraintIr) -> gmeow_errors::Result<String> {
    let Formula::Forall { vars, body } = &constraint.integrity else {
        return Err(proj_err(
            "integrity must be a range-restricted ∀-guarded condition (the top node is not a ∀)",
        ));
    };
    let focus = vars
        .first()
        .ok_or_else(|| proj_err("integrity ∀ binds no focus variable"))?;
    let Formula::Implies(guard, phi) = body.as_ref() else {
        return Err(proj_err(
            "integrity ∀ body must be a guarded implication (guard → condition)",
        ));
    };
    // A `directType(this, C)` guard is a selection marker realized by the `sh:SPARQLTarget`
    // (subclass-excluding), not a data triple — strip it so it never lowers to a triple that
    // matches nothing. The remaining guard atoms (if any) still lower positively.
    let mut pats = match strip_direct_type_guard(guard, focus) {
        Some(rest) => match rest {
            Some(g) => lower_positive(&g, focus)?,
            None => Vec::new(),
        },
        None => lower_positive(guard, focus)?,
    };
    pats.extend(lower_negative(phi, focus)?);
    Ok(pats.join(" "))
}

/// Lower an [`AggregateComparison`] to a whole `SELECT $this … GROUP BY $this HAVING(…)` query
/// selecting the focus nodes that VIOLATE the invariant. The aggregated path binds `?value`; a
/// property right-hand side binds `?rhs` (added to the `GROUP BY` so it stays available in the
/// `HAVING`, on the assumption of one right-hand value per focus). Because a `sh:SPARQLConstraint`
/// `sh:select` returns violations, the `HAVING` uses the comparator's logical negation (`=` ↦
/// `!=`, `<` ↦ `>=`, …). Reuses the same `GROUP BY` sub-`SELECT` shape as the SHACL-AF reduce-rule
/// projection rather than a bespoke aggregate lowering.
fn aggregate_select(agg: &AggregateComparison) -> String {
    let agg_var = "?value";
    let inner = if agg.distinct {
        format!("{}(DISTINCT {agg_var})", agg.function)
    } else {
        format!("{}({agg_var})", agg.function)
    };
    let path = sparql_predicate(&agg.path);
    let mut where_pats = vec![format!("$this {path} {agg_var} .")];
    let mut group_by = String::from("$this");
    let rhs = match &agg.compare_to {
        AggregateRhs::Property(p) => {
            where_pats.push(format!("$this {} ?rhs .", sparql_predicate(p)));
            group_by.push_str(" ?rhs");
            "?rhs".to_owned()
        }
        AggregateRhs::Literal { lexical, datatype } => {
            let lit = sparql_literal(lexical);
            match datatype {
                Some(dt) => format!("{lit}^^<{dt}>"),
                None => lit,
            }
        }
    };
    let op = agg.comparator.negated().as_sparql();
    format!(
        "SELECT $this WHERE {{ {} }} GROUP BY {group_by} HAVING ( {inner} {op} {rhs} )",
        where_pats.join(" ")
    )
}

/// Lower a [`JoinAggregate`] to a whole `SELECT $this ?far … GROUP BY $this ?far HAVING(…)` query
/// selecting the (focus, far-endpoint) groups that VIOLATE the invariant. Each leg is a reified
/// relation record `?rK`: its source triple anchors the join on the ALREADY-BOUND endpoint (`$this`
/// for the first leg, the preceding leg's target `?j{K-1}` for every later leg), then the target
/// triple binds this leg's endpoint `?jK` and the value triple binds its leaf `?vK`. Anchoring the
/// source triple on the bound endpoint first makes the store use its incidence index (the
/// object-keyed lookup of records incident to a cell) instead of scanning all records, so the query
/// scales with the number of incidence RECORDS, not with cells² — there is no cartesian product.
/// The aggregate is the group `function` of the PRODUCT `?v1 * … * ?vN` of the joined leaf values;
/// the group key is `$this` (the first leg's source) and `?jN` (the last leg's target, the far
/// endpoint). Because a `sh:SPARQLConstraint` `sh:select` returns violations, the `HAVING` uses the
/// comparator's logical negation (`=` ↦ `!=`, …). Variable names are byte-deterministic (`?rK`
/// records, `?jK` endpoints, `?vK` values) so regeneration is stable.
fn join_aggregate_select(ja: &JoinAggregate) -> String {
    let mut where_pats: Vec<String> = Vec::new();
    let mut value_vars: Vec<String> = Vec::with_capacity(ja.legs.len());
    for (idx, leg) in ja.legs.iter().enumerate() {
        let k = idx + 1;
        let record = format!("?r{k}");
        // The source endpoint is the focus for the first leg, else the shared join variable the
        // preceding leg bound (`leg[k-1].target = leg[k].source`).
        let src = if idx == 0 {
            "$this".to_owned()
        } else {
            format!("?j{}", idx)
        };
        let tgt = format!("?j{k}");
        let val = format!("?v{k}");
        // Index-friendly join order: anchor on the bound source endpoint, then bind the target and
        // the leaf value.
        where_pats.push(format!(
            "{record} {} {src} .",
            sparql_predicate(&leg.source)
        ));
        where_pats.push(format!(
            "{record} {} {tgt} .",
            sparql_predicate(&leg.target)
        ));
        where_pats.push(format!("{record} {} {val} .", sparql_predicate(&leg.value)));
        if let Some(rt) = &leg.record_type {
            where_pats.push(format!("{record} a {} .", iri_term(rt)));
        }
        value_vars.push(val);
    }
    let far = format!("?j{}", ja.legs.len());
    let product = value_vars.join(" * ");
    let inner = format!("{}({product})", ja.function);
    let op = ja.comparator.negated().as_sparql();
    let threshold = match &ja.threshold_datatype {
        Some(dt) => format!("{}^^<{dt}>", sparql_literal(&ja.threshold_lexical)),
        None => sparql_literal(&ja.threshold_lexical),
    };
    format!(
        "SELECT $this {far} WHERE {{ {} }} GROUP BY $this {far} HAVING ( {inner} {op} {threshold} )",
        where_pats.join(" ")
    )
}

/// Lower an [`AggregateBalance`] to the double-entry violation `SELECT`: a `GROUP BY $this ?group`
/// sub-`SELECT` that sums the debit-partition and credit-partition amounts per group, wrapped by an
/// outer `FILTER(?sumDebits != ?sumCredits)` that selects the focus nodes whose books do NOT balance
/// in some group. Each posting's amount and group key hang off the shared amount node
/// (`amount_node_predicate`), so a value and its currency are always read from the same amount.
fn aggregate_balance_select(bal: &AggregateBalance) -> String {
    let posting = sparql_predicate(&bal.posting_predicate);
    let amount_node = sparql_predicate(&bal.amount_node_predicate);
    let partition = sparql_predicate(&bal.partition_predicate);
    let value = sparql_predicate(&bal.value_predicate);
    let group = sparql_predicate(&bal.group_predicate);
    let debit = iri_term(&bal.debit_value);
    let credit = iri_term(&bal.credit_value);
    format!(
        "SELECT $this WHERE {{ {{ SELECT $this ?group (SUM(?debitVal) AS ?sumDebits) \
         (SUM(?creditVal) AS ?sumCredits) WHERE {{ $this {posting} ?posting . \
         ?posting {amount_node} ?amount ; {partition} ?direction . \
         ?amount {value} ?val ; {group} ?group . \
         BIND(IF(?direction = {debit}, ?val, 0) AS ?debitVal) \
         BIND(IF(?direction = {credit}, ?val, 0) AS ?creditVal) }} \
         GROUP BY $this ?group }} FILTER(?sumDebits != ?sumCredits) }}"
    )
}

/// The whole `sh:select` query body of a constraint: the multi-hop-join `GROUP BY`/`HAVING` form
/// when the constraint carries a [`JoinAggregate`] satellite, the double-entry-balance
/// `GROUP BY`/`HAVING` form when it carries an [`AggregateBalance`] satellite, the single-path
/// aggregate `GROUP BY`/`HAVING` form when it carries an [`AggregateComparison`] satellite, else
/// the range-restricted `guard ∧ ¬φ` violation query lowered from the integrity formula.
fn constraint_select(c: &ConstraintIr) -> gmeow_errors::Result<String> {
    // Hard-fail rather than silently pick a winner: a constraint carrying more than one
    // aggregate satellite would otherwise have the lower-priority satellite(s) below silently
    // dropped from the projected shape (a no-optionality violation).
    c.ensure_single_satellite()?;
    if let Some(ja) = &c.join_aggregate {
        return Ok(join_aggregate_select(ja));
    }
    if let Some(bal) = &c.aggregate_balance {
        return Ok(aggregate_balance_select(bal));
    }
    match &c.aggregate {
        Some(agg) => Ok(aggregate_select(agg)),
        None => Ok(format!("SELECT $this WHERE {{ {} }}", violation_where(c)?)),
    }
}

/// The `sh:target [ a sh:SPARQLTarget … ]` clause selecting the DIRECT instances of a class: nodes
/// typed `c` but NOT also typed any proper subclass of `c` (a node with a more-specific type is
/// validated by that subclass's own shape). `rdfs:subClassOf` is the standard RDFS IRI.
fn direct_class_target_clause(c: &str) -> String {
    let ct = iri_term(c);
    format!(
        "sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"SELECT ?this WHERE {{ ?this a {ct} . \
         FILTER NOT EXISTS {{ ?this a ?sub . ?sub \
         <http://www.w3.org/2000/01/rdf-schema#subClassOf>+ {ct} . FILTER ( ?sub != {ct} ) }} }}\"\"\" ]"
    )
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
        ShapeTarget::DirectClass(c) => direct_class_target_clause(c),
        ShapeTarget::Sparql(sel) => {
            format!("sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"{sel}\"\"\" ]")
        }
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
        ShapeTarget::Class(x)
        | ShapeTarget::SubjectsOf(x)
        | ShapeTarget::ObjectsOf(x)
        | ShapeTarget::DirectClass(x) => x.clone(),
        ShapeTarget::ValueKeyed { predicate, .. } => predicate.clone(),
        // A raw-sparql target has no single domain term; a Sparql-targeted constraint always
        // carries an explicit `logic:formalizes` (handled above), so this falls back to its IRI.
        ShapeTarget::Sparql(_) => c.iri.clone(),
    }
}

/// Try to render one `logic:Constraint` block, or return the reason its integrity exceeds the
/// range-restricted guarded SPARQL-constraint fragment (so the caller carries it as flagged
/// residue rather than emitting a broken query).
fn try_project_block(c: &ConstraintIr) -> gmeow_errors::Result<String> {
    let select = constraint_select(c)?;
    let shape = procedural_shape_iri(c);
    let formalizes = procedural_formalizes_term(c);
    let sev = c.severity.as_str();
    let target = procedural_target_clause(&c.target);
    let message_line = match &c.message {
        Some(m) => format!("        sh:message \"{}\" ;\n", esc_str(m)),
        None => String::new(),
    };
    let failure_line = c.failure_class.as_ref().map_or_else(String::new, |fc| {
        format!("    gmeow:enforcesFailureClass <{fc}> ;\n")
    });
    // The primary back-reference plus every additional `logic:formalizes` term (a constraint may
    // formalize the canonical class it governs AND the legacy shape it reproduces). `also_formalizes`
    // is pre-sorted/deduped and never contains the primary, so the emission is deterministic.
    let also_lines = c
        .also_formalizes
        .iter()
        .map(|f| format!("    logic:formalizes <{f}> ;\n"))
        .collect::<String>();
    Ok(format!(
        "<{shape}>\n    a sh:NodeShape ;\n    logic:formalizes <{formalizes}> ;\n{also_lines}{failure_line}    {target} ;\n    \
         sh:sparql [\n        a sh:SPARQLConstraint ;\n        sh:severity sh:{sev} ;\n{message_line}        \
         sh:select \"\"\"{select}\"\"\" ;\n    ] .\n"
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
                let reason = reason.message();
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
    fn validation_shape_projects_failure_class_metadata() {
        let s = shape("https://ex/Shape", "https://ex/C", vec![])
            .with_failure_class("https://ex/Failure")
            .unwrap();
        let ttl = project_validation_shape_shacl(&s);
        assert!(ttl.contains("gmeow/enforcesFailureClass> <https://ex/Failure>"));
    }

    #[test]
    fn class_guarded_constraint_strips_the_type_atom_from_the_violation_where() {
        // A `∀ this. C(this) → ∃v. P(this, v)` constraint targets `sh:targetClass C` (which follows
        // rdfs:subClassOf). The violation WHERE must therefore NOT re-assert `$this a C` (a plain BGP
        // triple would wrongly exclude the subclass instances `sh:targetClass` selects); it keeps only
        // the negated condition.
        let this = Term::Var("this".into());
        let integrity = Formula::Forall {
            vars: vec!["this".into()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        Term::Iri(RDF_TYPE.into()),
                        vec![this.clone(), Term::Iri("https://ex/C".into())],
                    )
                    .unwrap(),
                ),
                Box::new(Formula::Exists {
                    vars: vec!["v".into()],
                    body: Box::new(
                        Formula::atom(
                            Term::Iri("https://ex/p".into()),
                            vec![this, Term::Var("v".into())],
                        )
                        .unwrap(),
                    ),
                }),
            )),
        };
        let c = ConstraintIr::new("https://ex/C1", integrity, ShaclSeverity::Violation, None)
            .unwrap()
            .with_formalizes("https://ex/C")
            .unwrap();
        let block = project_procedural_constraint(&c);
        assert!(
            block.contains("sh:targetClass <https://ex/C>"),
            "targetClass must select the focus: {block}"
        );
        assert!(
            !block.contains("$this a <https://ex/C>"),
            "the redundant class-guard triple must be stripped from the WHERE: {block}"
        );
        assert!(
            block.contains("FILTER NOT EXISTS { $this <https://ex/p> ?v"),
            "the negated existential condition must remain: {block}"
        );
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
    fn or_and_xone_emit_sh_or_and_sh_xone_branch_lists() {
        // `owl:unionOf` → `sh:or ( [ … ] [ … ] )`; `owl:disjointUnionOf` → `sh:xone ( … )`.
        // Branches serialize in canonical (content-key sorted) order, deterministically.
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![
                prop(
                    "https://ex/target",
                    vec![ConstraintComponent::Or(vec![
                        ConstraintComponent::Class("https://ex/B".into()),
                        ConstraintComponent::Class("https://ex/A".into()),
                    ])],
                ),
                prop(
                    "https://ex/kind",
                    vec![ConstraintComponent::Xone(vec![
                        ConstraintComponent::Class("https://ex/X".into()),
                        ConstraintComponent::Class("https://ex/Y".into()),
                    ])],
                ),
            ],
        );
        let ttl = project_validation_shape_shacl(&s);
        assert!(
            ttl.contains("sh:or ( [ sh:class <https://ex/A> ] [ sh:class <https://ex/B> ] )"),
            "{ttl}"
        );
        assert!(
            ttl.contains("sh:xone ( [ sh:class <https://ex/X> ] [ sh:class <https://ex/Y> ] )"),
            "{ttl}"
        );
        // A clean (non-lossy) disjunction carries no SHACL residue but IS a ShEx-only drop.
        assert!(shacl_residue(&s).is_empty(), "{:?}", shacl_residue(&s));
        assert!(
            shex_residue(&s).iter().any(|r| r.contains("sh:or"))
                && shex_residue(&s).iter().any(|r| r.contains("sh:xone")),
            "{:?}",
            shex_residue(&s)
        );
    }

    #[test]
    fn nested_lossy_branch_is_flagged_in_disjunction_residue() {
        // A Pattern nested inside a branch of sh:or must be flagged (never silently dropped).
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            vec![prop(
                "https://ex/p",
                vec![ConstraintComponent::Or(vec![
                    ConstraintComponent::Class("https://ex/A".into()),
                    ConstraintComponent::Pattern {
                        regex: "^x".into(),
                        flags: None,
                    },
                ])],
            )],
        );
        assert!(
            shacl_residue(&s)
                .iter()
                .any(|r| r.contains("regex-dialect residue")),
            "a Pattern in an sh:or branch must be flagged: {:?}",
            shacl_residue(&s)
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
    fn procedural_constraint_projects_failure_class_metadata() {
        let c = guarded(
            "https://ex/cFailure",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        )
        .with_failure_class("https://ex/Failure")
        .unwrap();
        assert!(
            block(&c).contains("gmeow:enforcesFailureClass <https://ex/Failure>"),
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

    /// A conjunction of PROHIBITIONS over MIXED argument positions lowers to one UNION arm
    /// per forbidden position, each arm binding `$this`.
    ///
    /// The shape a "never occupies any authority position" law takes:
    /// `∀this. C(this) → ¬∃p. bars(p, this) ∧ ¬∃q. proves(this, q) ∧ ¬∃r. decides(r, this)`.
    /// Its negation is a disjunction, and each disjunct is a bare positive triple that binds
    /// the focus in a DIFFERENT slot — twice as the object, once as the subject. That mix is
    /// the reason to pin it: an arm that lost its `$this` binding would select every node in
    /// the graph rather than the ones the guard admits, so the law would condemn the corpus.
    #[test]
    fn a_conjunction_of_prohibitions_lowers_to_one_scoped_union_arm_per_position() {
        let bars = Formula::Not(Box::new(exists(
            "p",
            atom("https://ex/authorizes", tvar("p"), tvar("this")),
        )));
        let proves = Formula::Not(Box::new(exists(
            "q",
            atom("https://ex/establishes", tvar("this"), tvar("q")),
        )));
        let decides = Formula::Not(Box::new(exists(
            "r",
            atom("https://ex/decidedBy", tvar("r"), tvar("this")),
        )));
        let c = guarded(
            "https://ex/cNeverAuthority",
            Formula::And(vec![bars, proves, decides]),
        );
        let b = block(&c);
        assert_eq!(
            b.matches(" UNION ").count(),
            2,
            "three forbidden positions join as two UNION separators; block was: {b}"
        );
        for (pred, subj, obj) in [
            ("https://ex/authorizes", "?p", "$this"),
            ("https://ex/establishes", "$this", "?q"),
            ("https://ex/decidedBy", "?r", "$this"),
        ] {
            assert!(
                b.contains(&format!("{{ {subj} <{pred}> {obj} . }}")),
                "each position must be its own arm binding $this in the slot the law names \
                 ({subj} <{pred}> {obj}); block was: {b}"
            );
        }
        assert!(
            !b.contains("FILTER NOT EXISTS"),
            "every arm binds the focus through a positive triple, so none may degrade to an \
             unscoped FILTER; block was: {b}"
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
            b.contains(
                "FILTER NOT EXISTS { ?v a/<http://www.w3.org/2000/01/rdf-schema#subClassOf>* \
                 <https://ex/Part> . }"
            ),
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
    fn inverse_atom_places_the_focus_in_object_position() {
        // An inverse occurrence `rel(?v, $this)` (the focus in ARGUMENT-1 / object position)
        // lowers with `$this` as the triple object — the grounding "grounded BY a separate
        // observation" pattern (`observationResult(?obs, $this)`) needs no property-path term,
        // only the honest argument order.
        let c = guarded(
            "https://ex/cInv",
            exists(
                "obs",
                atom("https://ex/observationResult", tvar("obs"), tvar("this")),
            ),
        );
        assert!(
            block(&c).contains("FILTER NOT EXISTS { ?obs <https://ex/observationResult> $this . }"),
            "{}",
            block(&c)
        );
    }

    #[test]
    fn term_distinct_with_a_literal_rhs_lowers_to_an_inequality_filter() {
        // A forbidden existential whose body pins a bound var to differ from a fixed literal:
        // φ = ¬∃q. (sigNeg(this,q) ∧ termDistinct(q, "0"^^integer));
        // ¬φ = ∃q. sigNeg(this,q) ∧ FILTER(?q != 0) — the metric-signature "q = 0" invariant.
        let xsd_int = "http://www.w3.org/2001/XMLSchema#integer";
        let body = Formula::And(vec![
            atom("https://ex/signatureNegative", tvar("this"), tvar("q")),
            Formula::atom(
                tiri(LOGIC_TERM_DISTINCT),
                vec![
                    tvar("q"),
                    Term::Literal {
                        lexical: "0".to_owned(),
                        datatype: Some(xsd_int.to_owned()),
                    },
                ],
            )
            .unwrap(),
        ]);
        let c = guarded("https://ex/cLit", Formula::Not(Box::new(exists("q", body))));
        let b = block(&c);
        assert!(
            b.contains("$this <https://ex/signatureNegative> ?q ."),
            "{b}"
        );
        assert!(
            b.contains(&format!("FILTER ( ?q != '0'^^<{xsd_int}> )")),
            "{b}"
        );
    }

    #[test]
    fn term_in_forbidden_membership_lowers_to_an_in_filter() {
        // φ = ¬∃v. (leak(this,v) ∧ termIn(v, {ex:a, ex:b})); ¬φ = ∃v. leak(this,v) ∧ ?v IN (…).
        let body = Formula::And(vec![
            atom("https://ex/leak", tvar("this"), tvar("v")),
            Formula::atom(
                tiri(LOGIC_TERM_IN),
                vec![tvar("v"), tiri("https://ex/a"), tiri("https://ex/b")],
            )
            .unwrap(),
        ]);
        let c = guarded("https://ex/cIn", Formula::Not(Box::new(exists("v", body))));
        let b = block(&c);
        assert!(b.contains("$this <https://ex/leak> ?v ."), "{b}");
        assert!(
            b.contains("FILTER ( ?v IN (<https://ex/a>, <https://ex/b>) )"),
            "{b}"
        );
    }

    #[test]
    fn term_in_required_membership_lowers_to_a_not_in_filter() {
        // φ = ∀v. tag(this,v) → termIn(v, {ex:a}); a value outside the set violates → ?v NOT IN (…).
        let inner = forall(
            "v",
            Formula::Implies(
                Box::new(atom("https://ex/tag", tvar("this"), tvar("v"))),
                Box::new(
                    Formula::atom(tiri(LOGIC_TERM_IN), vec![tvar("v"), tiri("https://ex/a")])
                        .unwrap(),
                ),
            ),
        );
        let c = guarded("https://ex/cReq", inner);
        let b = block(&c);
        assert!(b.contains("$this <https://ex/tag> ?v ."), "{b}");
        assert!(b.contains("FILTER ( ?v NOT IN (<https://ex/a>) )"), "{b}");
    }

    #[test]
    fn term_str_starts_and_regex_lower_to_string_filters() {
        let lit = |s: &str| Term::Literal {
            lexical: s.to_owned(),
            datatype: None,
        };
        // Forbidden prefix: ¬∃v. code(this,v) ∧ termStrStarts(v,"gmn:") → FILTER STRSTARTS.
        let body = Formula::And(vec![
            atom("https://ex/code", tvar("this"), tvar("v")),
            Formula::atom(tiri(LOGIC_TERM_STR_STARTS), vec![tvar("v"), lit("gmn:")]).unwrap(),
        ]);
        let c = guarded(
            "https://ex/cPrefix",
            Formula::Not(Box::new(exists("v", body))),
        );
        let b = block(&c);
        assert!(b.contains("FILTER ( STRSTARTS(STR(?v), 'gmn:') )"), "{b}");

        // Required regex: ∀v. code(this,v) → termRegex(v,"^[a-z]+$") → violation !REGEX.
        let inner = forall(
            "v",
            Formula::Implies(
                Box::new(atom("https://ex/code", tvar("this"), tvar("v"))),
                Box::new(
                    Formula::atom(tiri(LOGIC_TERM_REGEX), vec![tvar("v"), lit("^[a-z]+$")])
                        .unwrap(),
                ),
            ),
        );
        let c = guarded("https://ex/cRegex", inner);
        let b = block(&c);
        assert!(b.contains("FILTER ( !REGEX(STR(?v), '^[a-z]+$') )"), "{b}");
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
    fn coexisting_aggregate_satellites_hard_fail_at_projection() {
        // A constraint carrying BOTH a join_aggregate and an aggregate satellite would otherwise
        // have the projection dispatch's priority order silently drop the lower-priority
        // `aggregate` satellite. It must instead hard-fail — carried as flagged residue,
        // never silently projected with one satellite dropped.
        use crate::ir::{AggregateComparator, JoinLeg};

        let leg = JoinLeg::new(
            None,
            "https://ex/incidenceCoface",
            "https://ex/incidenceFace",
            "https://ex/incidenceSign",
        )
        .unwrap();
        let ja = JoinAggregate::new(
            "SUM",
            vec![leg.clone(), leg],
            AggregateComparator::Eq,
            "0",
            None,
        )
        .unwrap();
        let agg = AggregateComparison::new(
            "COUNT",
            false,
            "https://ex/part",
            AggregateComparator::Le,
            AggregateRhs::Literal {
                lexical: "10".into(),
                datatype: None,
            },
        )
        .unwrap();
        let c = guarded(
            "https://ex/cDualSatellite",
            exists("c", atom("https://ex/companion", tvar("this"), tvar("c"))),
        )
        .with_join_aggregate(ja)
        .with_aggregate(agg);

        assert!(
            c.ensure_single_satellite().is_err(),
            "a constraint carrying two aggregate satellites must fail the guard directly"
        );
        assert!(
            project_procedural_constraint(&c).is_empty(),
            "a dual-satellite constraint must not emit a block"
        );
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        let residue = procedural_constraint_residue(&prog);
        assert_eq!(residue.len(), 1, "{residue:?}");
        assert!(
            residue[0].contains("aggregate") && residue[0].contains("join_aggregate"),
            "the residue must name the coexisting satellites: {residue:?}"
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

        let report = validate_graphs(data, &shapes_ttl, None).expect("validate");
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

    // ── Constraint-sugar expansion (P1–P5, P7) + aggregate (P6) projection ────────

    /// The prefix header for the sugar fixtures.
    const SUGAR_PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <https://ex/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
";

    /// Parse a sugar fixture (asserting no MALFORMED_CONSTRAINT) and project its single constraint
    /// to a `sh:SPARQLConstraint` block.
    fn project_sugar(ttl: &str) -> String {
        let src = format!("{SUGAR_PREFIXES}{ttl}");
        let (program, diags) =
            crate::frontend::parse_logic_str(&src, None).expect("sugar fixture must parse");
        assert!(
            !diags.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"),
            "unexpected MALFORMED_CONSTRAINT diagnostics: {diags:?}"
        );
        assert_eq!(
            program.constraints.len(),
            1,
            "expected exactly one constraint"
        );
        let block = project_procedural_constraint(&program.constraints[0]);
        assert!(
            !block.is_empty(),
            "the sugar constraint must project a block"
        );
        block
    }

    #[test]
    fn value_set_membership_required_sugar_projects_a_not_in_filter() {
        let b = project_sugar(
            "ex:vsr a logic:ValueSetMembershipConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:register ;\n\
             logic:memberValue ex:alpha , ex:beta ;\n\
             logic:formalizes ex:Widget .",
        );
        assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
        assert!(b.contains("$this <https://ex/register> ?v ."), "{b}");
        assert!(
            b.contains("FILTER ( ?v NOT IN (<https://ex/alpha>, <https://ex/beta>) )"),
            "{b}"
        );
    }

    #[test]
    fn value_set_membership_forbidden_sugar_projects_an_in_filter() {
        let b = project_sugar(
            "ex:vsf a logic:ValueSetMembershipConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:leaks ;\n\
             logic:membershipMode \"forbidden\" ;\n\
             logic:memberValue ex:secret ;\n\
             logic:formalizes ex:Widget .",
        );
        assert!(b.contains("$this <https://ex/leaks> ?v ."), "{b}");
        assert!(b.contains("FILTER ( ?v IN (<https://ex/secret>) )"), "{b}");
    }

    #[test]
    fn string_pattern_sugar_projects_regex_and_prefix_filters() {
        let req = project_sugar(
            "ex:spr a logic:StringPatternConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:code ;\n\
             logic:stringOp \"regexRequired\" ;\n\
             logic:stringPattern \"^[A-Z]+$\" ;\n\
             logic:formalizes ex:Widget .",
        );
        assert!(
            req.contains("FILTER ( !REGEX(STR(?v), '^[A-Z]+$') )"),
            "{req}"
        );
        let forb = project_sugar(
            "ex:spf a logic:StringPatternConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:label ;\n\
             logic:stringOp \"prefixForbidden\" ;\n\
             logic:stringPattern \"tmp:\" ;\n\
             logic:formalizes ex:Widget .",
        );
        assert!(
            forb.contains("FILTER ( STRSTARTS(STR(?v), 'tmp:') )"),
            "{forb}"
        );
    }

    #[test]
    fn p1_choice_group_sugar_expands_and_projects() {
        let b = project_sugar(
            "ex:c1 a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"exactly-one\" ;\n\
             logic:formalizes ex:Widget .",
        );
        assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
        assert!(b.contains("logic:formalizes <https://ex/Widget>"), "{b}");
        // exactly-one → a UNION of per-predicate branches under FILTER NOT EXISTS.
        assert!(b.contains("UNION"), "{b}");
        assert!(
            b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
            "{b}"
        );
    }

    #[test]
    fn sugar_failure_class_dedupes_identical_values() {
        let block = project_sugar(
            "ex:fc a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"exactly-one\" ;\n\
             logic:formalizes ex:Widget ;\n\
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:Failure, ex:Failure .",
        );
        assert_eq!(
            block.matches("gmeow:enforcesFailureClass").count(),
            1,
            "{block}"
        );
    }

    #[test]
    fn sugar_failure_class_rejects_distinct_values() {
        let src = format!(
            "{SUGAR_PREFIXES}ex:fc_bad a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"exactly-one\" ;\n\
             logic:formalizes ex:Widget ;\n\
             <https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ex:FailureA, ex:FailureB ."
        );
        let (program, diagnostics) =
            crate::frontend::parse_logic_str(&src, None).expect("fixture parses");
        assert!(program.constraints.is_empty());
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "MALFORMED_CONSTRAINT" && diagnostic.message.contains("distinct")
        }));
    }

    #[test]
    fn p1_at_most_one_sugar_projects() {
        let b = project_sugar(
            "ex:c1b a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"at-most-one\" ;\n\
             logic:formalizes ex:Widget .",
        );
        // at-most-one over two predicates → the violation is the pair present together.
        assert!(
            b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
            "{b}"
        );
        assert!(b.contains("SELECT $this WHERE"), "{b}");
    }

    #[test]
    fn p1_at_least_one_sugar_projects_a_missing_all_alternatives_guard() {
        let b = project_sugar(
            "ex:c1c a logic:ChoiceGroupConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:choicePredicate ex:hasA , ex:hasB ;\n\
             logic:choiceMode \"at-least-one\" ;\n\
             logic:formalizes ex:Widget .",
        );
        // at-least-one → the violation is EVERY alternative absent: both predicates appear
        // under the negative (missing) side of the lowering.
        assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
        assert!(
            b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
            "{b}"
        );
        assert!(b.contains("NOT EXISTS"), "{b}");
    }

    #[test]
    fn p2_guarded_implication_sugar_expands_and_projects() {
        let b = project_sugar(
            "ex:c2 a logic:GuardedImplicationConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:trigger ex:isActive ;\n\
             logic:requires ex:companion ;\n\
             logic:formalizes ex:Widget .",
        );
        assert!(b.contains("sh:targetClass <https://ex/Widget>"), "{b}");
        // Guard: the trigger predicate is a positive triple; the missing companion is the violation.
        assert!(b.contains("<https://ex/isActive>"), "{b}");
        assert!(
            b.contains("FILTER NOT EXISTS") && b.contains("<https://ex/companion>"),
            "{b}"
        );
    }

    #[test]
    fn p2_guarded_implication_with_trigger_value_projects() {
        let b = project_sugar(
            "ex:c2v a logic:GuardedImplicationConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:trigger ex:kind ;\n\
             logic:triggerValue ex:Special ;\n\
             logic:requires ex:companion ;\n\
             logic:formalizes ex:Widget .",
        );
        // The pinned trigger value appears in the guard triple's object position.
        assert!(b.contains("<https://ex/kind> <https://ex/Special>"), "{b}");
    }

    #[test]
    fn predicate_presence_guarded_implication_targets_subjects_of_the_trigger() {
        // With NO logic:onClass the trigger predicate IS the range restriction: the guard is the
        // bare trigger atom, so the target derives as sh:targetSubjectsOf trigger (the grounding
        // "subjects of a claim predicate must carry a grounding field" pattern).
        let b = project_sugar(
            "ex:pp a logic:GuardedImplicationConstraint ;\n\
             logic:trigger ex:aboutReading ;\n\
             logic:requires ex:vantage ;\n\
             logic:formalizes ex:SomeShape .",
        );
        assert!(
            b.contains("sh:targetSubjectsOf <https://ex/aboutReading>"),
            "{b}"
        );
        assert!(b.contains("logic:formalizes <https://ex/SomeShape>"), "{b}");
        assert!(b.contains("$this <https://ex/aboutReading> ?t ."), "{b}");
        assert!(
            b.contains("FILTER NOT EXISTS") && b.contains("<https://ex/vantage>"),
            "{b}"
        );
    }

    #[test]
    fn p3_disjunctive_requiredness_sugar_expands_and_projects() {
        let b = project_sugar(
            "ex:c3 a logic:DisjunctiveRequirednessConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:anyOf ex:hasA , ex:hasB ;\n\
             logic:formalizes ex:Widget .",
        );
        // ≥1 required → violation is NONE present: FILTER NOT EXISTS { {hasA} UNION {hasB} }.
        assert!(b.contains("FILTER NOT EXISTS"), "{b}");
        assert!(b.contains("UNION"), "{b}");
        assert!(
            b.contains("<https://ex/hasA>") && b.contains("<https://ex/hasB>"),
            "{b}"
        );
    }

    #[test]
    fn p4_path_value_type_sugar_expands_and_projects() {
        let b = project_sugar(
            "ex:c4 a logic:PathValueTypeConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:valuePath ex:part ;\n\
             logic:valueClass ex:Part ;\n\
             logic:formalizes ex:part .",
        );
        assert!(b.contains("logic:formalizes <https://ex/part>"), "{b}");
        // Violation: ∃v. part(this,v) ∧ ¬ Part(v).
        assert!(b.contains("<https://ex/part>"), "{b}");
        assert!(
            b.contains("FILTER NOT EXISTS") && b.contains("<https://ex/Part>"),
            "{b}"
        );
    }

    #[test]
    fn p4_path_value_fixed_predicate_value_sugar_expands_and_projects() {
        // The fixed predicate=value variant: every value on the path must carry Q = o.
        // Violation: ∃v. inducedByForm(this,v) ∧ ¬ definiteness(v, positiveDefinite).
        let b = project_sugar(
            "ex:c4f a logic:PathValueTypeConstraint ;\n\
             logic:onClass ex:Norm ;\n\
             logic:valuePath ex:inducedByForm ;\n\
             logic:valuePredicate ex:definiteness ;\n\
             logic:valueObject ex:positiveDefinite ;\n\
             logic:formalizes ex:Norm .",
        );
        assert!(b.contains("$this <https://ex/inducedByForm> ?v ."), "{b}");
        assert!(
            b.contains(
                "FILTER NOT EXISTS { ?v <https://ex/definiteness> <https://ex/positiveDefinite> . }"
            ),
            "{b}"
        );
    }

    #[test]
    fn p5_cross_node_co_occur_sugar_expands_and_projects() {
        let b = project_sugar(
            "ex:c5 a logic:CrossNodeConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:roleA ex:left ;\n\
             logic:roleB ex:right ;\n\
             logic:crossMode \"co-occur\" ;\n\
             logic:formalizes ex:Widget .",
        );
        assert!(
            b.contains("<https://ex/left>") && b.contains("<https://ex/right>"),
            "{b}"
        );
        assert!(b.contains("UNION"), "{b}");
    }

    #[test]
    fn p5_cross_node_differ_sugar_projects_an_equality_filter() {
        let b = project_sugar(
            "ex:c5d a logic:CrossNodeConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:roleA ex:left ;\n\
             logic:roleB ex:right ;\n\
             logic:crossMode \"differ\" ;\n\
             logic:formalizes ex:Widget .",
        );
        // Violation of "roles must differ" = the two roles bind an EQUAL value.
        assert!(b.contains("FILTER ( ?a = ?b )"), "{b}");
        assert!(
            b.contains("<https://ex/left>") && b.contains("<https://ex/right>"),
            "{b}"
        );
    }

    #[test]
    fn p7_forbidden_pattern_sugar_expands_and_projects() {
        let b = project_sugar(
            "ex:c7 a logic:ForbiddenPatternConstraint ;\n\
             logic:onClass ex:Widget ;\n\
             logic:forbiddenPredicate ex:forbidden ;\n\
             logic:formalizes ex:Widget .",
        );
        // A forbidden-pattern violation is the PRESENCE of the pattern: the SHACL
        // sh:select returns the focus nodes that HAVE the forbidden predicate (a
        // positive BGP match), not a FILTER NOT EXISTS over its absence.
        assert!(
            b.contains("<https://ex/forbidden> ?") && !b.contains("NOT EXISTS"),
            "{b}"
        );
    }

    #[test]
    fn p6_aggregate_count_distinct_property_rhs_projects_group_by_having() {
        let b = project_sugar(
            "ex:agg1 a logic:AggregateConstraint ;\n\
             logic:onClass ex:Dimensional ;\n\
             logic:aggFunction \"COUNT\" ;\n\
             logic:aggDistinct true ;\n\
             logic:aggPath ex:hasAxis ;\n\
             logic:aggComparator \"=\" ;\n\
             logic:aggCompareTo ex:dimensionCount ;\n\
             logic:formalizes ex:Dimensional .",
        );
        assert!(b.contains("sh:targetClass <https://ex/Dimensional>"), "{b}");
        assert!(b.contains("COUNT(DISTINCT ?value)"), "{b}");
        // A property RHS binds ?rhs and joins it into the GROUP BY.
        assert!(b.contains("$this <https://ex/dimensionCount> ?rhs"), "{b}");
        assert!(b.contains("GROUP BY $this ?rhs"), "{b}");
        // The invariant is `=`, so the violation-selecting HAVING uses the negation `!=`.
        assert!(
            b.contains("HAVING ( COUNT(DISTINCT ?value) != ?rhs )"),
            "{b}"
        );
    }

    #[test]
    fn aggregate_balance_sugar_projects_partitioned_group_by_having() {
        let b = project_sugar(
            "ex:bal a logic:AggregateBalanceConstraint ;\n\
             logic:onClass ex:JournalEntry ;\n\
             logic:balancePostingPredicate ex:posting ;\n\
             logic:balancePartitionPredicate ex:direction ;\n\
             logic:balanceDebitValue ex:debit ;\n\
             logic:balanceCreditValue ex:credit ;\n\
             logic:balanceAmountNodePredicate ex:amount ;\n\
             logic:balanceValuePredicate ex:value ;\n\
             logic:balanceGroupPredicate ex:currency ;\n\
             logic:formalizes ex:JournalEntry .",
        );
        assert!(
            b.contains("sh:targetClass <https://ex/JournalEntry>"),
            "{b}"
        );
        assert!(
            b.contains("SELECT $this ?group (SUM(?debitVal) AS ?sumDebits) (SUM(?creditVal) AS ?sumCredits)"),
            "{b}"
        );
        assert!(b.contains("$this <https://ex/posting> ?posting ."), "{b}");
        assert!(
            b.contains("?amount <https://ex/value> ?val ; <https://ex/currency> ?group ."),
            "{b}"
        );
        assert!(
            b.contains("BIND(IF(?direction = <https://ex/debit>, ?val, 0) AS ?debitVal)"),
            "{b}"
        );
        assert!(b.contains("GROUP BY $this ?group"), "{b}");
        assert!(b.contains("FILTER(?sumDebits != ?sumCredits)"), "{b}");
    }

    #[test]
    fn term_lang_matches_and_has_lang_project_language_tag_filters() {
        // A hand-authored language-tag gate: a sparqlTarget-focused constraint whose body forbids a
        // gmeow:-namespaced tagged literal whose LANGUAGE TAG is not under the x-gmeow- prefix.
        let src = format!(
            "{SUGAR_PREFIXES}\
ex:ilt a logic:Constraint ;\n\
  logic:formalizes ex:LangShape ;\n\
  logic:severity \"Violation\" ;\n\
  logic:integrity ex:iltForall .\n\
ex:iltForall a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"this\" ] ; logic:forall ex:iltImpl .\n\
ex:iltImpl a logic:Formula ; logic:antecedent ex:iltTarget ; logic:consequent ex:iltOk .\n\
ex:iltTarget a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/sparqlTarget> ;\n\
  logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] , [ logic:termIndex 1 ; logic:termLiteral \"SELECT DISTINCT ?this WHERE {{ ?this ?p ?value . FILTER(isLiteral(?value)) }}\" ] .\n\
ex:iltOk a logic:Formula ; logic:not ex:iltBad .\n\
ex:iltBad a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"p\" ] , [ logic:termIndex 1 ; logic:termVariable \"value\" ] ; logic:exists ex:iltBody .\n\
ex:iltBody a logic:Formula ; logic:and ex:iltLink , ex:iltLit , ex:iltHasLang , ex:iltNotOk .\n\
ex:iltLink a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/linkVia> ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"this\" ] , [ logic:termIndex 1 ; logic:termVariable \"p\" ] , [ logic:termIndex 2 ; logic:termVariable \"value\" ] .\n\
ex:iltLit a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/termIsLiteral> ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"value\" ] .\n\
ex:iltHasLang a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/termHasLang> ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"value\" ] .\n\
ex:iltNotOk a logic:Formula ; logic:not ex:iltMatch .\n\
ex:iltMatch a logic:Formula ; logic:relation <https://blackcatinformatics.ca/logic/termLangMatches> ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"value\" ] , [ logic:termIndex 1 ; logic:termLiteral \"^x-gmeow-[a-z0-9-]+$\" ] .\n"
        );
        let (program, diags) =
            crate::frontend::parse_logic_str(&src, None).expect("fixture must parse");
        assert!(
            !diags.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"),
            "unexpected diagnostics: {diags:?}"
        );
        assert_eq!(program.constraints.len(), 1);
        let b = project_procedural_constraint(&program.constraints[0]);
        assert!(!b.is_empty(), "must project a block: {b}");
        // The tagged-literal presence check and the case-insensitive negated language-tag regex.
        assert!(b.contains(r#"LANG(?value) != """#), "{b}");
        assert!(
            b.contains("!REGEX(LANG(?value), '^x-gmeow-[a-z0-9-]+$', 'i')"),
            "{b}"
        );
        // The raw sparqlTarget is carried verbatim as the sh:target select.
        assert!(b.contains("a sh:SPARQLTarget"), "{b}");
    }

    #[test]
    fn p6_aggregate_sum_literal_rhs_projects_group_by_having() {
        let b = project_sugar(
            "ex:agg2 a logic:AggregateConstraint ;\n\
             logic:onClass ex:Portfolio ;\n\
             logic:aggFunction \"SUM\" ;\n\
             logic:aggPath ex:weight ;\n\
             logic:aggComparator \"=\" ;\n\
             logic:aggCompareTo \"1\"^^xsd:integer ;\n\
             logic:formalizes ex:Portfolio .",
        );
        assert!(b.contains("SUM(?value)"), "{b}");
        assert!(b.contains("$this <https://ex/weight> ?value"), "{b}");
        assert!(b.contains("GROUP BY $this"), "{b}");
        // The literal RHS keeps its datatype in the HAVING comparison.
        assert!(
            b.contains("HAVING ( SUM(?value) != '1'^^<http://www.w3.org/2001/XMLSchema#integer> )"),
            "{b}"
        );
    }

    #[test]
    fn p6_aggregate_count_distinct_validates_against_a_graph() {
        // End-to-end: the generated GROUP BY/HAVING SELECT must be valid SPARQL and flag the right
        // focus nodes. `good` has 2 distinct axes and dimensionCount 2 (conforms); `bad` has 2
        // distinct axes and dimensionCount 3 (violates COUNT(DISTINCT) = dimensionCount).
        use purrdf::shapes::engine::validate_graphs;
        let src = format!(
            "{SUGAR_PREFIXES}\
ex:aggv a logic:AggregateConstraint ;\n\
  logic:onClass ex:Dimensional ;\n\
  logic:aggFunction \"COUNT\" ;\n\
  logic:aggDistinct true ;\n\
  logic:aggPath ex:hasAxis ;\n\
  logic:aggComparator \"=\" ;\n\
  logic:aggCompareTo ex:dimensionCount ;\n\
  logic:formalizes ex:Dimensional ."
        );
        let (program, _) = crate::frontend::parse_logic_str(&src, None).expect("parse");
        let shapes_ttl = project_procedural_constraints(&program);
        let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
        let data = format!(
            "<https://ex/good> {ty} <https://ex/Dimensional> .\n\
<https://ex/good> <https://ex/hasAxis> <https://ex/x> .\n\
<https://ex/good> <https://ex/hasAxis> <https://ex/y> .\n\
<https://ex/good> <https://ex/dimensionCount> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/bad> {ty} <https://ex/Dimensional> .\n\
<https://ex/bad> <https://ex/hasAxis> <https://ex/x> .\n\
<https://ex/bad> <https://ex/hasAxis> <https://ex/y> .\n\
<https://ex/bad> <https://ex/dimensionCount> \"3\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n"
        );
        let report = validate_graphs(&data, &shapes_ttl, None).expect("validate");
        let flagged: Vec<String> = report
            .results
            .iter()
            .map(|r| r.focus_node.to_string())
            .collect();
        assert!(
            flagged.iter().any(|f| f.contains("bad")),
            "the count-mismatch node must be flagged; flagged: {flagged:?}"
        );
        assert!(
            !flagged.iter().any(|f| f.contains("good")),
            "the conforming node must NOT be flagged; flagged: {flagged:?}"
        );
    }

    /// The two-leg boundary-square-zero (∂²=0) join-aggregate demonstrator: a coface hops via an
    /// incidence record to an intermediate cell, then via a second incidence record to a far face,
    /// and the SUM of the incidence-sign PRODUCT over the intermediate cells must be 0 per
    /// (coface, far-face) group. Reused by the projection and end-to-end tests.
    const JOIN_AGG_SUGAR: &str = "ex:boundarySquareZero a logic:JoinAggregateConstraint ;\n\
         logic:onClass ex:TopCell ;\n\
         logic:aggFunction \"SUM\" ;\n\
         logic:aggComparator \"=\" ;\n\
         logic:aggThreshold 0 ;\n\
         logic:joinPath (\n\
           [ logic:legRecordType ex:Incidence ; logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
           [ logic:legRecordType ex:Incidence ; logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
         ) ;\n\
         logic:formalizes ex:BoundaryOperator .";

    #[test]
    fn join_aggregate_multi_hop_projects_deterministic_group_by_having() {
        let b = project_sugar(JOIN_AGG_SUGAR);
        // The focus is the coface (the top cell); the join is anchored on $this.
        assert!(b.contains("sh:targetClass <https://ex/TopCell>"), "{b}");
        // Leg 1 anchors on the bound focus $this via the source predicate, then binds the
        // intermediate endpoint ?j1 and the leaf value ?v1 (index-friendly ordering).
        assert!(
            b.contains("?r1 <https://ex/incidenceCoface> $this . ?r1 <https://ex/incidenceFace> ?j1 . ?r1 <https://ex/incidenceSign> ?v1 . ?r1 a <https://ex/Incidence> ."),
            "leg 1 must anchor on $this and bind ?j1/?v1: {b}"
        );
        // Leg 2 re-binds the SHARED join variable ?j1 as its source (the multi-hop join), binds the
        // far endpoint ?j2, and the second leaf value ?v2.
        assert!(
            b.contains("?r2 <https://ex/incidenceCoface> ?j1 . ?r2 <https://ex/incidenceFace> ?j2 . ?r2 <https://ex/incidenceSign> ?v2 . ?r2 a <https://ex/Incidence> ."),
            "leg 2 must re-bind ?j1 and bind ?j2/?v2: {b}"
        );
        // The group key is (focus, far endpoint); the aggregate is the SUM of the sign PRODUCT.
        assert!(b.contains("SELECT $this ?j2 WHERE"), "{b}");
        assert!(b.contains("GROUP BY $this ?j2"), "{b}");
        // Invariant is `=` 0, so the violation-selecting HAVING negates it to `!=`.
        assert!(
            b.contains(
                "HAVING ( SUM(?v1 * ?v2) != '0'^^<http://www.w3.org/2001/XMLSchema#integer> )"
            ),
            "{b}"
        );
        // No cartesian product over cells: every triple pattern is a record-anchored join, so no
        // FILTER-cross or bare cell×cell pattern is emitted.
        assert!(!b.contains("NOT EXISTS"), "{b}");
    }

    #[test]
    fn join_aggregate_projection_is_byte_deterministic() {
        // Two independent parses of the same source produce byte-identical SPARQL (stable variable
        // names + clause order), so regeneration is stable.
        let a = project_sugar(JOIN_AGG_SUGAR);
        let b = project_sugar(JOIN_AGG_SUGAR);
        assert_eq!(a, b, "join-aggregate projection must be byte-deterministic");
    }

    #[test]
    fn join_aggregate_boundary_square_zero_validates_against_a_graph() {
        // End-to-end: the generated multi-hop-join GROUP BY/HAVING SELECT must be valid SPARQL and
        // flag exactly the top cell whose ∂² ≠ 0. `good` is a triangle whose signed incidences make
        // every (coface, far-vertex) group sum to 0; `bad` has one flipped sign so a group sums to
        // -2 ≠ 0.
        use purrdf::shapes::engine::validate_graphs;
        let src = format!("{SUGAR_PREFIXES}{JOIN_AGG_SUGAR}");
        let (program, _) = crate::frontend::parse_logic_str(&src, None).expect("parse");
        let shapes_ttl = project_procedural_constraints(&program);
        let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
        let int = "<http://www.w3.org/2001/XMLSchema#integer>";
        // A signed incidence record `coface → face` with sign `s`, as four N-Triples over a fresh
        // labeled blank node (the data graph is parsed as N-Triples, so no `[]`/`;` sugar).
        let mut rec = 0u32;
        let mut inc = |coface: &str, face: &str, s: i32| {
            rec += 1;
            let r = format!("_:rec{rec}");
            format!(
                "{r} {ty} <https://ex/Incidence> .\n\
                 {r} <https://ex/incidenceCoface> <{coface}> .\n\
                 {r} <https://ex/incidenceFace> <{face}> .\n\
                 {r} <https://ex/incidenceSign> \"{s}\"^^{int} .\n"
            )
        };
        let mut data = String::new();
        // GOOD triangle: T over edges a,b,c over vertices p,q,r; ∂² = 0 in every group.
        data.push_str(&format!("<https://ex/T> {ty} <https://ex/TopCell> .\n"));
        data.push_str(&inc("https://ex/T", "https://ex/a", 1));
        data.push_str(&inc("https://ex/T", "https://ex/b", 1));
        data.push_str(&inc("https://ex/T", "https://ex/c", 1));
        data.push_str(&inc("https://ex/a", "https://ex/p", -1));
        data.push_str(&inc("https://ex/a", "https://ex/q", 1));
        data.push_str(&inc("https://ex/b", "https://ex/q", -1));
        data.push_str(&inc("https://ex/b", "https://ex/r", 1));
        data.push_str(&inc("https://ex/c", "https://ex/r", -1));
        data.push_str(&inc("https://ex/c", "https://ex/p", 1));
        // BAD triangle: same shape but the c→p sign is flipped, so group (Tb, pb) sums to -2 ≠ 0.
        data.push_str(&format!("<https://ex/Tb> {ty} <https://ex/TopCell> .\n"));
        data.push_str(&inc("https://ex/Tb", "https://ex/ab", 1));
        data.push_str(&inc("https://ex/Tb", "https://ex/bb", 1));
        data.push_str(&inc("https://ex/Tb", "https://ex/cb", 1));
        data.push_str(&inc("https://ex/ab", "https://ex/pb", -1));
        data.push_str(&inc("https://ex/ab", "https://ex/qb", 1));
        data.push_str(&inc("https://ex/bb", "https://ex/qb", -1));
        data.push_str(&inc("https://ex/bb", "https://ex/rb", 1));
        data.push_str(&inc("https://ex/cb", "https://ex/rb", -1));
        data.push_str(&inc("https://ex/cb", "https://ex/pb", -1)); // flipped: should be +1
        let report = validate_graphs(&data, &shapes_ttl, None).expect("validate");
        let flagged: Vec<String> = report
            .results
            .iter()
            .map(|r| r.focus_node.to_string())
            .collect();
        assert!(
            flagged.iter().any(|f| f.contains("/Tb")),
            "the ∂²≠0 top cell must be flagged; flagged: {flagged:?}"
        );
        assert!(
            !flagged
                .iter()
                .any(|f| f.contains("/T>") || f.ends_with("/T")),
            "the ∂²=0 top cell must NOT be flagged; flagged: {flagged:?}"
        );
    }

    #[test]
    fn comparison_constraint_lowers_to_an_ordering_filter() {
        let b = project_sugar(
            "ex:cmp a logic:ComparisonConstraint ;\n\
             logic:onClass ex:ScoreScale ;\n\
             logic:leftPath ex:scaleMin ;\n\
             logic:rightPath ex:scaleMax ;\n\
             logic:compareOp \">=\" ;\n\
             logic:formalizes ex:ScoreScale .",
        );
        assert!(b.contains("sh:targetClass <https://ex/ScoreScale>"), "{b}");
        assert!(b.contains("$this <https://ex/scaleMin> ?l ."), "{b}");
        assert!(b.contains("$this <https://ex/scaleMax> ?r ."), "{b}");
        // The FORBIDDEN relation (min >= max) is the violation-selecting FILTER.
        assert!(b.contains("FILTER ( ?l >= ?r )"), "{b}");
        assert!(!b.contains("NOT EXISTS"), "{b}");
    }

    #[test]
    fn path_node_kind_iri_lowers_to_a_negated_isiri_filter() {
        // With no onClass the value-path is the range restriction → sh:targetSubjectsOf.
        let b = project_sugar(
            "ex:nk a logic:PathNodeKindConstraint ;\n\
             logic:valuePath ex:hasAboutness ;\n\
             logic:nodeKind \"IRI\" ;\n\
             logic:formalizes ex:AboutnessTargetShape .",
        );
        assert!(
            b.contains("sh:targetSubjectsOf <https://ex/hasAboutness>"),
            "{b}"
        );
        assert!(b.contains("$this <https://ex/hasAboutness> ?v ."), "{b}");
        // The violation is a value that is NOT an IRI.
        assert!(b.contains("FILTER ( !isIRI(?v) )"), "{b}");
    }

    #[test]
    fn path_node_kind_blank_or_iri_on_a_class_target() {
        let b = project_sugar(
            "ex:nk2 a logic:PathNodeKindConstraint ;\n\
             logic:onClass ex:SetBuilderExpression ;\n\
             logic:valuePath ex:memberCondition ;\n\
             logic:nodeKind \"BlankNodeOrIRI\" ;\n\
             logic:formalizes ex:SetBuilderExpression .",
        );
        assert!(
            b.contains("sh:targetClass <https://ex/SetBuilderExpression>"),
            "{b}"
        );
        assert!(
            b.contains("FILTER ( !( isIRI(?v) || isBlank(?v) ) )"),
            "{b}"
        );
    }

    #[test]
    fn self_join_uniqueness_lowers_to_a_shared_value_self_join() {
        let b = project_sugar(
            "ex:sj a logic:SelfJoinUniquenessConstraint ;\n\
             logic:siblingPredicate ex:argumentSlot ;\n\
             logic:sharedPredicate ex:slotIndex ;\n\
             logic:formalizes ex:SlotIndexUniquenessShape .",
        );
        assert!(
            b.contains("sh:targetSubjectsOf <https://ex/argumentSlot>"),
            "{b}"
        );
        assert!(b.contains("$this <https://ex/argumentSlot> ?s1 ."), "{b}");
        assert!(b.contains("$this <https://ex/argumentSlot> ?s2 ."), "{b}");
        assert!(b.contains("?s1 <https://ex/slotIndex> ?i ."), "{b}");
        assert!(b.contains("?s2 <https://ex/slotIndex> ?i ."), "{b}");
        assert!(b.contains("FILTER ( ?s1 != ?s2 )"), "{b}");
    }

    #[test]
    fn inverse_existence_lowers_to_a_typed_inverse_not_exists() {
        let b = project_sugar(
            "ex:inv a logic:InverseExistenceConstraint ;\n\
             logic:onClass ex:FeatureValue ;\n\
             logic:inversePredicate ex:denotationTarget ;\n\
             logic:subjectClass ex:Denotation ;\n\
             logic:formalizes ex:FeatureValue .",
        );
        assert!(
            b.contains("sh:targetClass <https://ex/FeatureValue>"),
            "{b}"
        );
        // Violation = no typed Denotation points back at $this.
        assert!(b.contains("FILTER NOT EXISTS"), "{b}");
        assert!(
            b.contains(
                "?s a/<http://www.w3.org/2000/01/rdf-schema#subClassOf>* <https://ex/Denotation> ."
            ),
            "{b}"
        );
        assert!(
            b.contains("?s <https://ex/denotationTarget> $this ."),
            "{b}"
        );
    }

    #[test]
    fn transitive_reachability_lowers_to_a_subclass_property_path() {
        let b = project_sugar(
            "ex:tr a logic:TransitiveReachabilityConstraint ;\n\
             logic:onClass ex:FlagshipScenario ;\n\
             logic:viaPredicate ex:enforcesFailureClass ;\n\
             logic:pathPredicate ex:subClassOf ;\n\
             logic:reachTarget ex:ConformanceFailure ;\n\
             logic:formalizes ex:FlagshipScenario .",
        );
        assert!(
            b.contains("sh:targetClass <https://ex/FlagshipScenario>"),
            "{b}"
        );
        assert!(
            b.contains("$this <https://ex/enforcesFailureClass> ?v ."),
            "{b}"
        );
        // Violation = a failure class that does NOT transitively subclass the root.
        assert!(
            b.contains(
                "FILTER NOT EXISTS { ?v <https://ex/subClassOf>+ <https://ex/ConformanceFailure> . }"
            ),
            "{b}"
        );
    }

    #[test]
    fn acyclic_constraint_lowers_to_a_self_reaching_property_path() {
        let b = project_sugar(
            "ex:ac a logic:AcyclicConstraint ;\n\
             logic:onClass ex:FormSlot ;\n\
             logic:pathPredicate ex:dependsOn ;\n\
             logic:formalizes ex:FormSlot .",
        );
        assert!(b.contains("sh:targetClass <https://ex/FormSlot>"), "{b}");
        // Violation = the focus reaches itself along one-or-more dependsOn hops.
        assert!(b.contains("$this <https://ex/dependsOn>+ $this ."), "{b}");
        assert!(!b.contains("NOT EXISTS"), "{b}");
    }

    #[test]
    fn comparison_and_node_kind_constraints_validate_against_a_graph() {
        use purrdf::shapes::engine::validate_graphs;
        let src = format!(
            "{SUGAR_PREFIXES}\
ex:scaleCmp a logic:ComparisonConstraint ;\n\
  logic:onClass ex:ScoreScale ;\n\
  logic:leftPath ex:scaleMin ;\n\
  logic:rightPath ex:scaleMax ;\n\
  logic:compareOp \">=\" ;\n\
  logic:formalizes ex:ScoreScale ."
        );
        let (program, _) = crate::frontend::parse_logic_str(&src, None).expect("parse");
        let shapes_ttl = project_procedural_constraints(&program);
        let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
        let dec = "^^<http://www.w3.org/2001/XMLSchema#decimal>";
        let data = format!(
            "<https://ex/good> {ty} <https://ex/ScoreScale> .\n\
<https://ex/good> <https://ex/scaleMin> \"0.0\"{dec} .\n\
<https://ex/good> <https://ex/scaleMax> \"1.0\"{dec} .\n\
<https://ex/bad> {ty} <https://ex/ScoreScale> .\n\
<https://ex/bad> <https://ex/scaleMin> \"1.0\"{dec} .\n\
<https://ex/bad> <https://ex/scaleMax> \"1.0\"{dec} .\n"
        );
        let report = validate_graphs(&data, &shapes_ttl, None).expect("validate");
        let flagged: Vec<String> = report
            .results
            .iter()
            .map(|r| r.focus_node.to_string())
            .collect();
        assert!(
            flagged.iter().any(|f| f.contains("bad")),
            "the min>=max scale must be flagged; flagged: {flagged:?}"
        );
        assert!(
            !flagged.iter().any(|f| f.contains("good")),
            "the well-formed scale must NOT be flagged; flagged: {flagged:?}"
        );
    }

    // ── procedural-constraint capabilities (arithmetic BIND, variable predicate,
    //    direct-instance target, filter disjunction) ─────────────────────────────────

    /// Collect the focus nodes a projected constraint document flags over `data` (N-Triples).
    fn flagged_over(shapes_ttl: &str, data: &str) -> Vec<String> {
        use purrdf::shapes::engine::validate_graphs;
        let report = validate_graphs(data, shapes_ttl, None).expect("validate");
        report
            .results
            .iter()
            .map(|r| r.focus_node.to_string())
            .collect()
    }

    #[test]
    fn arithmetic_sum_bind_lowers_and_flags_a_dimension_mismatch() {
        // ∀this:Widget. ¬∃(p,q,d,s). sigPos(this,p) ∧ sigNeg(this,q) ∧ dim(this,d) ∧
        //   termSum(s,p,q) ∧ termDistinct(s,d)  — the p+q ≠ dimensionCount invariant.
        let sum =
            Formula::atom(tiri(LOGIC_TERM_SUM), vec![tvar("s"), tvar("p"), tvar("q")]).unwrap();
        let distinct =
            Formula::atom(tiri(LOGIC_TERM_DISTINCT), vec![tvar("s"), tvar("d")]).unwrap();
        let body = Formula::And(vec![
            atom("https://ex/sigPos", tvar("this"), tvar("p")),
            atom("https://ex/sigNeg", tvar("this"), tvar("q")),
            atom("https://ex/dim", tvar("this"), tvar("d")),
            sum,
            distinct,
        ]);
        let c = guarded("https://ex/cSum", Formula::Not(Box::new(exists("p", body))));
        let b = block(&c);
        assert!(b.contains("BIND ( ( ?p + ?q ) AS ?s )"), "{b}");
        // The BIND must precede the FILTER that reads ?s.
        assert!(
            b.find("BIND").unwrap() < b.find("FILTER ( ?s != ?d )").unwrap(),
            "BIND must precede its FILTER: {b}"
        );
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        let doc = project_procedural_constraints(&prog);
        let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
        let data = format!(
            "<https://ex/bad> {ty} <https://ex/Widget> .\n\
<https://ex/bad> <https://ex/sigPos> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/bad> <https://ex/sigNeg> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/bad> <https://ex/dim> \"5\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/good> {ty} <https://ex/Widget> .\n\
<https://ex/good> <https://ex/sigPos> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/good> <https://ex/sigNeg> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
<https://ex/good> <https://ex/dim> \"3\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n"
        );
        let flagged = flagged_over(&doc, &data);
        assert!(flagged.iter().any(|f| f.contains("bad")), "{flagged:?}");
        assert!(!flagged.iter().any(|f| f.contains("good")), "{flagged:?}");
    }

    #[test]
    fn variable_predicate_link_lowers_and_flags_any_edge_to_a_typed_object() {
        // ∀this:Widget. ¬∃(link,c). linkVia(this,link,c) ∧ type(c, ex:Bad) — any predicate
        // linking the focus to a Bad-typed object is forbidden.
        let link = Formula::atom(
            tiri(LOGIC_LINK_VIA),
            vec![tvar("this"), tvar("link"), tvar("c")],
        )
        .unwrap();
        let body = Formula::And(vec![
            link,
            atom(RDF_TYPE, tvar("c"), tiri("https://ex/Bad")),
        ]);
        let c = guarded(
            "https://ex/cLink",
            Formula::Not(Box::new(exists("c", body))),
        );
        let b = block(&c);
        assert!(b.contains("$this ?link ?c ."), "{b}");
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        let doc = project_procedural_constraints(&prog);
        let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
        let data = format!(
            "<https://ex/bad> {ty} <https://ex/Widget> .\n\
<https://ex/bad> <https://ex/anyEdge> <https://ex/x> .\n\
<https://ex/x> {ty} <https://ex/Bad> .\n\
<https://ex/good> {ty} <https://ex/Widget> .\n\
<https://ex/good> <https://ex/anyEdge> <https://ex/y> .\n"
        );
        let flagged = flagged_over(&doc, &data);
        assert!(flagged.iter().any(|f| f.contains("bad")), "{flagged:?}");
        assert!(!flagged.iter().any(|f| f.contains("good")), "{flagged:?}");
    }

    #[test]
    fn direct_instance_target_excludes_subclass_typed_nodes() {
        // ∀this. directType(this, ex:Base) → ¬∃v. required(this, v).
        let integrity = Formula::Forall {
            vars: vec!["this".to_owned()],
            body: Box::new(Formula::Implies(
                Box::new(
                    Formula::atom(
                        tiri(LOGIC_DIRECT_TYPE),
                        vec![tvar("this"), tiri("https://ex/Base")],
                    )
                    .unwrap(),
                ),
                Box::new(exists(
                    "v",
                    atom("https://ex/required", tvar("this"), tvar("v")),
                )),
            )),
        };
        let c = ConstraintIr::new(
            "https://ex/cDirect",
            integrity,
            ShaclSeverity::Violation,
            None,
        )
        .unwrap();
        assert!(matches!(&c.target, ShapeTarget::DirectClass(x) if x == "https://ex/Base"));
        let b = block(&c);
        assert!(b.contains("a sh:SPARQLTarget"), "{b}");
        assert!(b.contains("rdf-schema#subClassOf>+"), "{b}");
        // The directType marker must NOT leak into the WHERE body as a triple.
        assert!(!b.contains("directType"), "{b}");
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        let doc = project_procedural_constraints(&prog);
        let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
        let data = format!(
            "<https://ex/Sub> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://ex/Base> .\n\
<https://ex/directBad> {ty} <https://ex/Base> .\n\
<https://ex/subExcluded> {ty} <https://ex/Base> .\n\
<https://ex/subExcluded> {ty} <https://ex/Sub> .\n"
        );
        let flagged = flagged_over(&doc, &data);
        // directBad is a bare Base instance missing `required` → flagged.
        assert!(
            flagged.iter().any(|f| f.contains("directBad")),
            "{flagged:?}"
        );
        // subExcluded is ALSO a Sub instance → excluded by the direct-instance target.
        assert!(
            !flagged.iter().any(|f| f.contains("subExcluded")),
            "a subclass-typed node must be excluded: {flagged:?}"
        );
    }

    #[test]
    fn filter_disjunction_lowers_to_one_combined_filter_not_a_union() {
        // ∀this:Widget. ¬∃(b,d). band(this,b) ∧ dec(this,d) ∧
        //   ( (b = ex:certain ∧ d < 0.9) ∨ (b = ex:unspecified) )
        let dec_lit = |n: &str| Term::Literal {
            lexical: n.to_owned(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".to_owned()),
        };
        let arm1 = Formula::And(vec![
            Formula::atom(
                tiri(LOGIC_TERM_EQUAL),
                vec![tvar("b"), tiri("https://ex/certain")],
            )
            .unwrap(),
            Formula::atom(tiri(LOGIC_TERM_LESS), vec![tvar("d"), dec_lit("0.9")]).unwrap(),
        ]);
        let arm2 = Formula::atom(
            tiri(LOGIC_TERM_EQUAL),
            vec![tvar("b"), tiri("https://ex/unspecified")],
        )
        .unwrap();
        let bad = Formula::Or(vec![arm1, arm2]);
        let body = Formula::And(vec![
            atom("https://ex/band", tvar("this"), tvar("b")),
            atom("https://ex/dec", tvar("this"), tvar("d")),
            bad,
        ]);
        let c = guarded(
            "https://ex/cBand",
            Formula::Not(Box::new(exists("b", body))),
        );
        let b = block(&c);
        assert!(b.contains("FILTER ("), "{b}");
        assert!(
            b.contains(" || "),
            "the disjunction must be one FILTER, not a UNION: {b}"
        );
        assert!(!b.contains("UNION"), "{b}");
        let prog = LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c]);
        let doc = project_procedural_constraints(&prog);
        let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
        let data = format!(
            "<https://ex/bad> {ty} <https://ex/Widget> .\n\
<https://ex/bad> <https://ex/band> <https://ex/certain> .\n\
<https://ex/bad> <https://ex/dec> \"0.3\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n\
<https://ex/good> {ty} <https://ex/Widget> .\n\
<https://ex/good> <https://ex/band> <https://ex/certain> .\n\
<https://ex/good> <https://ex/dec> \"0.95\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n"
        );
        let flagged = flagged_over(&doc, &data);
        assert!(flagged.iter().any(|f| f.contains("bad")), "{flagged:?}");
        assert!(!flagged.iter().any(|f| f.contains("good")), "{flagged:?}");
    }

    #[test]
    fn consent_exactly_one_and_at_least_one_lowers_without_aggregates() {
        // The RightsStatement consent invariant, encoded in first-order form (no COUNT): a
        // consent-governing RightsStatement (a permission whose ruleAction is
        // processPersonalData) must have EXACTLY ONE data subject and AT LEAST ONE data
        // controller. "Exactly one subject" is the nested-negation form ∃s. subj(s) ∧
        // ¬∃s2.(subj(s2) ∧ s2≠s); SHACL pre-binds $this, so the UNION-of-negations arms correlate.
        let rs = "https://ex/RightsStatement";
        let action = "https://ex/processPersonalData";
        let one_subject = exists(
            "s",
            Formula::And(vec![
                atom("https://ex/hasDataSubject", tvar("this"), tvar("s")),
                Formula::Not(Box::new(exists(
                    "s2",
                    Formula::And(vec![
                        atom("https://ex/hasDataSubject", tvar("this"), tvar("s2")),
                        Formula::atom(tiri(LOGIC_TERM_DISTINCT), vec![tvar("s2"), tvar("s")])
                            .unwrap(),
                    ]),
                ))),
            ]),
        );
        let at_least_one_controller = exists(
            "c",
            atom("https://ex/hasDataController", tvar("this"), tvar("c")),
        );
        let guard = || {
            Formula::And(vec![
                atom(RDF_TYPE, tvar("this"), tiri(rs)),
                atom("https://ex/hasPermission", tvar("this"), tvar("perm")),
                atom("https://ex/ruleAction", tvar("perm"), tiri(action)),
            ])
        };
        // Two peer constraints (both formalizing the same RightsStatement source-term): one guards
        // the exactly-one-subject condition, one the at-least-one-controller condition. Splitting a
        // compound `∧` invariant into single-`FILTER NOT EXISTS` peers keeps each violation body a
        // guarded conjunction (no top-level UNION-of-filters whose arms bind no focus).
        let mk = |iri: &str, phi: Formula| {
            let integrity = Formula::Forall {
                vars: vec!["this".to_owned()],
                body: Box::new(Formula::Implies(Box::new(guard()), Box::new(phi))),
            };
            ConstraintIr::new(iri, integrity, ShaclSeverity::Warning, None)
                .unwrap()
                .with_formalizes("https://ex/ConsentWellformednessShape")
                .unwrap()
        };
        let c_subj = mk("https://ex/cConsentSubject", one_subject);
        let c_ctrl = mk("https://ex/cConsentController", at_least_one_controller);
        assert!(matches!(&c_subj.target, ShapeTarget::Class(x) if x == rs));
        let prog =
            LogicProgram::new(vec![], vec![], vec![], None).with_constraints(vec![c_subj, c_ctrl]);
        let doc = project_procedural_constraints(&prog);
        let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
        // rs2: 1 subject, 0 controller (violates); rs3: 0 subject, 1 controller (violates);
        // rs4: 2 subjects, 1 controller (violates); rsGood: 1 subject, 1 controller (ok);
        // rsPlain: NOT consent-governing (perm has no processPersonalData action) — must not fire.
        let data = format!(
            "<https://ex/rs2> {ty} <{rs}> .\n\
<https://ex/rs2> <https://ex/hasDataSubject> <https://ex/alice> .\n\
<https://ex/rs2> <https://ex/hasPermission> <https://ex/perm2> .\n\
<https://ex/perm2> <https://ex/ruleAction> <{action}> .\n\
<https://ex/rs3> {ty} <{rs}> .\n\
<https://ex/rs3> <https://ex/hasDataController> <https://ex/alice> .\n\
<https://ex/rs3> <https://ex/hasPermission> <https://ex/perm3> .\n\
<https://ex/perm3> <https://ex/ruleAction> <{action}> .\n\
<https://ex/rs4> {ty} <{rs}> .\n\
<https://ex/rs4> <https://ex/hasDataSubject> <https://ex/alice> .\n\
<https://ex/rs4> <https://ex/hasDataSubject> <https://ex/bob> .\n\
<https://ex/rs4> <https://ex/hasDataController> <https://ex/alice> .\n\
<https://ex/rs4> <https://ex/hasPermission> <https://ex/perm4> .\n\
<https://ex/perm4> <https://ex/ruleAction> <{action}> .\n\
<https://ex/rsGood> {ty} <{rs}> .\n\
<https://ex/rsGood> <https://ex/hasDataSubject> <https://ex/alice> .\n\
<https://ex/rsGood> <https://ex/hasDataController> <https://ex/acme> .\n\
<https://ex/rsGood> <https://ex/hasPermission> <https://ex/permG> .\n\
<https://ex/permG> <https://ex/ruleAction> <{action}> .\n\
<https://ex/rsPlain> {ty} <{rs}> .\n\
<https://ex/rsPlain> <https://ex/hasDataSubject> <https://ex/alice> .\n\
<https://ex/rsPlain> <https://ex/hasDataSubject> <https://ex/bob> .\n\
<https://ex/rsPlain> <https://ex/hasPermission> <https://ex/permP> .\n"
        );
        let flagged = flagged_over(&doc, &data);
        for bad in ["rs2", "rs3", "rs4"] {
            assert!(
                flagged.iter().any(|f| f.contains(bad)),
                "{bad} must be flagged: {flagged:?}"
            );
        }
        assert!(
            !flagged.iter().any(|f| f.contains("rsGood")),
            "the well-formed consent statement must NOT fire: {flagged:?}"
        );
        assert!(
            !flagged.iter().any(|f| f.contains("rsPlain")),
            "a non-consent statement must NOT fire: {flagged:?}"
        );
    }
}
