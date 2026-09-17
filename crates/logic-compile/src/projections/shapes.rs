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

#[cfg(test)]
mod qualified_tests;

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
        ShapeValue::Literal(literal) => purrdf::RdfTerm::literal(literal.clone()).to_string(),
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

/// Bounds over one structurally identical qualifying value set. The canonical
/// components remain intact; only their generated SHACL parameter layout changes.
struct QualifiedCounts<'a> {
    shape: &'a [ConstraintComponent],
    min: Option<u32>,
    max: Option<u32>,
}

/// SHACL allows one qualified value shape per property shape. Restrictions over
/// the same value set intersect their bounds; different value sets need separate
/// property shapes, each retaining this path and its diagnostic metadata.
/// https://www.w3.org/TR/shacl/#QualifiedValueShapeConstraintComponent
fn property_shape_blocks(p: &PropertyConstraintIr, failure_class: Option<&str>) -> Vec<String> {
    let mut qualified: Vec<QualifiedCounts<'_>> = Vec::new();
    for component in &p.components {
        if let ConstraintComponent::QualifiedValueShape { shape, min, max } = component {
            if let Some(counts) = qualified.iter_mut().find(|counts| counts.shape == shape) {
                // For the SAME value set of size n, (n >= a AND n >= b) is
                // n >= max(a,b), and the dual holds for upper bounds. This is
                // structural arithmetic, independent of any bounded corpus.
                counts.min = counts.min.into_iter().chain(*min).max();
                counts.max = counts.max.into_iter().chain(*max).min();
            } else {
                qualified.push(QualifiedCounts {
                    shape,
                    min: *min,
                    max: *max,
                });
            }
        }
    }
    let mut blocks = vec![property_shape_block(
        p,
        failure_class,
        qualified.first(),
        true,
    )];
    blocks.extend(
        qualified
            .iter()
            .skip(1)
            .map(|counts| property_shape_block(p, failure_class, Some(counts), false)),
    );
    blocks
}

/// Render one independent count domain. Ordinary value, total-count and reifier
/// constraints occur on the first block only; duplicating them would multiply
/// findings whose source-shape identities differ.
fn property_shape_block(
    p: &PropertyConstraintIr,
    failure_class: Option<&str>,
    qualified: Option<&QualifiedCounts<'_>>,
    include_common: bool,
) -> String {
    let mut lines = vec![path_term(p)];
    // Validation reports name the generated property shape, which belongs to
    // the same canonical law as the containing node shape.
    if let Some(failure_class) = failure_class {
        lines.push(format!(
            "<https://blackcatinformatics.ca/gmeow/enforcesFailureClass> {}",
            iri_term(failure_class)
        ));
    }
    if include_common {
        if let Some(n) = p.min_count {
            lines.push(format!("sh:minCount {n}"));
        }
        if let Some(n) = p.max_count {
            lines.push(format!("sh:maxCount {n}"));
        }
    }
    let mut qualified_emitted = false;
    for c in &p.components {
        if matches!(c, ConstraintComponent::QualifiedValueShape { .. }) {
            if let Some(counts) = qualified
                && !qualified_emitted
            {
                let inner = counts
                    .shape
                    .iter()
                    .flat_map(component_lines)
                    .collect::<Vec<_>>();
                lines.push(format!("sh:qualifiedValueShape [ {} ]", inner.join(" ; ")));
                if let Some(n) = counts.min {
                    lines.push(format!("sh:qualifiedMinCount {n}"));
                }
                if let Some(n) = counts.max {
                    lines.push(format!("sh:qualifiedMaxCount {n}"));
                }
                qualified_emitted = true;
            }
        } else if include_common {
            lines.extend(component_lines(c));
        }
    }
    // RDF-1.2 statement-layer extension: the reifier of each `focus`→`path`→`value` statement must
    // conform to `sh:reifierShape`, and `sh:reificationRequired true` demands ≥1 reifier. The
    // native engine reads these only from a single forward-predicate property shape, so they are
    // suppressed on an inverse path (which `PropertyConstraintIr::with_reifier` already rejects).
    if include_common && !p.inverse {
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
        pos.extend(
            property_shape_blocks(p, shape.failure_class.as_deref())
                .into_iter()
                .map(|block| format!("sh:property {block}")),
        );
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
        Term::Literal(literal) => {
            let token = sparql_literal(&literal.lexical_form);
            if let Some(language) = &literal.language {
                let suffix = literal
                    .direction
                    .map_or_else(String::new, |direction| format!("--{}", direction.as_str()));
                Ok(format!("{token}@{language}{suffix}"))
            } else {
                Ok(match &literal.datatype {
                    Some(datatype) => format!("{token}^^<{datatype}>"),
                    None => token,
                })
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
        let Term::Literal(purrdf::RdfLiteral {
            lexical_form: lexical,
            ..
        }) = &args[1]
        else {
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

#[path = "shapes.tests.rs"]
#[cfg(test)]
mod tests;

#[path = "shapes.shex_tests.rs"]
#[cfg(test)]
mod shex_tests;

#[path = "shapes.procedural_tests.rs"]
#[cfg(test)]
mod procedural_tests;
