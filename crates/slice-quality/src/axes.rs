// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The closed set of measurement primitives, keyed by `gmeow:axisProducer`.
//!
//! The rubric names a producer per axis; [`resolve`] maps that name onto a Rust
//! primitive. An unknown producer is a hard fail — never a silent skip — because a
//! rubric that names a primitive the kernel does not implement is a real drift the
//! axis-to-producer binding gate must catch, not paper over.
//!
//! Each primitive reads only what its axis's `ContextScope` licenses and advises
//! solely about the target slice.

use std::sync::LazyLock;

use purrdf::{DatasetView, GraphMatch, TermRef};
use regex::Regex;

use crate::graph::{self, g, id, one_lit};
use crate::score::{AxisScore, ScoreContext, advisory};

/// A measurement primitive: score the slice and surface advisories.
pub type Primitive = fn(&ScoreContext) -> AxisScore;

/// Resolve a `gmeow:axisProducer` key to its primitive, or `None` if the kernel
/// implements no such primitive (a hard-fail condition for the caller).
#[must_use]
pub fn resolve(producer: &str) -> Option<Primitive> {
    match producer {
        "grounding_axis" => Some(grounding_axis),
        "information_axis" => Some(information_axis),
        "prose_axis" => Some(prose_axis),
        "provenance_honesty" => Some(provenance_honesty),
        _ => None,
    }
}

/// Every producer key the kernel implements — the closed set the completeness and
/// binding gates enumerate.
pub const IMPLEMENTED: &[&str] = &[
    "grounding_axis",
    "information_axis",
    "prose_axis",
    "provenance_honesty",
];

// ── Axis 1: Maximal grounding ─────────────────────────────────────────────

/// The `logic:` foundation-stereotype types a grounded class may carry.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// Fraction of class terms carrying a `logic:` foundation stereotype. A class with
/// no stereotype is a bare domain term — the anti-pattern grounding measures.
fn grounding_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    let Some(type_p) = id(ds, graph::RDF_TYPE) else {
        return AxisScore::clean(0.0);
    };
    let owl_class = id(ds, "http://www.w3.org/2002/07/owl#Class");

    let classes: Vec<&String> = ctx
        .terms
        .iter()
        .filter(|iri| {
            id(ds, iri).is_some_and(|s| owl_class.is_some_and(|c| graph::has(ds, s, type_p, c)))
        })
        .collect();
    if classes.is_empty() {
        return AxisScore::clean(1.0); // no classes to ground → vacuously grounded
    }

    let mut grounded = 0usize;
    let mut findings = Vec::new();
    for class in &classes {
        let sid = id(ds, class).unwrap();
        let has_stereotype = ds
            .quads_for_pattern(Some(sid), Some(type_p), None, GraphMatch::Any)
            .any(|q| matches!(ds.resolve(q.o), TermRef::Iri(t) if t.starts_with(LOGIC_NS)));
        if has_stereotype {
            grounded += 1;
        } else {
            findings.push(advisory(
                "slice-quality.grounding.no-stereotype",
                format!(
                    "{class} is a bare class with no logic: foundation stereotype — assign one (SLICE_GUIDE §4; Principle 19)."
                ),
            ));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = grounded as f64 / classes.len() as f64;
    AxisScore { score, findings }
}

// ── Axis 3: Maximal information (annotation coat) ──────────────────────────

/// The annotation-coat predicates every TBox term must carry.
const COAT_TBOX: &[(&str, &str)] = &[
    ("rdfs:label", "http://www.w3.org/2000/01/rdf-schema#label"),
    (
        "skos:definition",
        "http://www.w3.org/2004/02/skos/core#definition",
    ),
    (
        "rdfs:isDefinedBy",
        "http://www.w3.org/2000/01/rdf-schema#isDefinedBy",
    ),
    (
        "skos:example",
        "http://www.w3.org/2004/02/skos/core#example",
    ),
];

/// The three-part usage coat + box role, required on TBox terms only.
const COAT_USAGE: &[&str] = &["useWhen", "avoidWhen", "howToUse", "graphBoxRole"];

/// The lighter coat expected of an A-Box value-vocabulary individual.
const COAT_INDIVIDUAL: &[(&str, &str)] = &[
    ("rdfs:label", "http://www.w3.org/2000/01/rdf-schema#label"),
    (
        "skos:definition",
        "http://www.w3.org/2004/02/skos/core#definition",
    ),
    (
        "rdfs:isDefinedBy",
        "http://www.w3.org/2000/01/rdf-schema#isDefinedBy",
    ),
];

fn is_tbox_term(ctx: &ScoreContext, iri: &str) -> bool {
    let ds = ctx.graph;
    let Some(type_p) = id(ds, graph::RDF_TYPE) else {
        return false;
    };
    let Some(sid) = id(ds, iri) else { return false };
    ds.quads_for_pattern(Some(sid), Some(type_p), None, GraphMatch::Any)
        .any(|q| match ds.resolve(q.o) {
            TermRef::Iri(t) => {
                t == "http://www.w3.org/2002/07/owl#Class"
                    || t.starts_with("http://www.w3.org/2002/07/owl#") && t.ends_with("Property")
            }
            _ => false,
        })
}

/// Average annotation-coat completeness across the slice's terms.
fn information_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    if ctx.terms.is_empty() {
        return AxisScore::clean(0.0);
    }
    let mut total_present = 0usize;
    let mut total_expected = 0usize;
    let mut findings = Vec::new();

    for iri in &ctx.terms {
        let Some(sid) = id(ds, iri) else { continue };
        let tbox = is_tbox_term(ctx, iri);
        let mut required: Vec<(&str, String)> = if tbox {
            let mut v: Vec<(&str, String)> = COAT_TBOX
                .iter()
                .map(|(label, p)| (*label, (*p).to_owned()))
                .collect();
            for local in COAT_USAGE {
                v.push((*local, g(local)));
            }
            v
        } else {
            COAT_INDIVIDUAL
                .iter()
                .map(|(label, p)| (*label, (*p).to_owned()))
                .collect()
        };
        // graphBoxRole is namespaced gmeow: — the g() form above already handles
        // it for TBox; individuals also want a box role.
        if !tbox {
            required.push(("graphBoxRole", g("graphBoxRole")));
        }

        let mut missing = Vec::new();
        for (label, pred_iri) in &required {
            total_expected += 1;
            let present = id(ds, pred_iri).is_some_and(|p| graph::has_any(ds, sid, p));
            if present {
                total_present += 1;
            } else {
                missing.push(*label);
            }
        }
        if !missing.is_empty() {
            findings.push(advisory(
                "slice-quality.information.incomplete-coat",
                format!(
                    "{iri} is missing annotation coat: {} (SLICE_GUIDE §6).",
                    missing.join(", ")
                ),
            ));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = if total_expected == 0 {
        0.0
    } else {
        total_present as f64 / total_expected as f64
    };
    AxisScore { score, findings }
}

// ── Axis 8: Prose quality ──────────────────────────────────────────────────

/// Negation cues that signal a boundary-stating ("what it is NOT") definition.
fn states_boundary(def: &str) -> bool {
    let d = def.to_lowercase();
    d.contains(" not ")
        || d.contains("never")
        || d.contains("rather than")
        || d.contains("as opposed to")
        || d.contains("instead of")
        || d.contains(" nor ")
        || def.contains("NOT")
}

/// A worked triple mentions a subject, a predicate, and an object — heuristically,
/// it contains a prefixed name and a statement separator.
fn is_worked_triple(example: &str) -> bool {
    (example.contains(':'))
        && (example.contains(" a ") || example.contains(';') || example.contains('.'))
}

/// Average prose-quality across the slice's terms with a definition.
fn prose_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    let def_p = id(ds, "http://www.w3.org/2004/02/skos/core#definition");
    let ex_p = id(ds, "http://www.w3.org/2004/02/skos/core#example");

    let mut checks = 0usize;
    let mut passed = 0usize;
    let mut findings = Vec::new();

    for iri in &ctx.terms {
        let Some(sid) = id(ds, iri) else { continue };
        if let Some(def) = def_p.and_then(|p| one_lit(ds, sid, p)) {
            checks += 1;
            if states_boundary(&def) {
                passed += 1;
            } else {
                findings.push(advisory(
                    "slice-quality.prose.definition-no-boundary",
                    format!("{iri} definition does not state a boundary (what it is NOT) (SLICE_GUIDE §6.2)."),
                ));
            }
        }
        if let Some(ex) = ex_p.and_then(|p| one_lit(ds, sid, p)) {
            checks += 1;
            if is_worked_triple(&ex) {
                passed += 1;
            } else {
                findings.push(advisory(
                    "slice-quality.prose.example-not-triple",
                    format!("{iri} skos:example is not a worked triple (SLICE_GUIDE §6.6)."),
                ));
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = if checks == 0 {
        0.0
    } else {
        passed as f64 / checks as f64
    };
    AxisScore { score, findings }
}

// ── Axis 8 sub-metric: Provenance honesty ("a test is not a rationale") ────

/// The test-artifact patterns a rationale must not name.
static TEST_ARTIFACT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\btest_[A-Za-z0-9_]+)|(\.rs::)|(\.py\b)|(Mirrors\s)").expect("valid regex")
});

/// Rationale-class predicates whose literals must state the reason, not the test.
fn rationale_predicates(ctx: &ScoreContext) -> Vec<purrdf::TermId> {
    ["saRationale", "cqRationale"]
        .iter()
        .filter_map(|local| id(ctx.graph, &g(local)))
        .chain(id(
            ctx.graph,
            "http://www.w3.org/2000/01/rdf-schema#comment",
        ))
        .collect()
}

/// Fraction of rationale literals that name no test artifact; flags each that does.
fn provenance_honesty(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    let preds = rationale_predicates(ctx);
    let mut total = 0usize;
    let mut clean = 0usize;
    let mut findings = Vec::new();

    for p in preds {
        for q in ds.quads_for_pattern(None, Some(p), None, GraphMatch::Any) {
            if let TermRef::Literal { lexical, .. } = ds.resolve(q.o) {
                total += 1;
                if TEST_ARTIFACT.is_match(lexical) {
                    let subj = match ds.resolve(q.s) {
                        TermRef::Iri(s) => s.to_owned(),
                        _ => "<anonymous>".to_owned(),
                    };
                    findings.push(advisory(
                        "slice-quality.prose.test-rationale",
                        format!(
                            "{subj} rationale names a test artifact — a test is evidence, not a reason. Strip the test-naming clause and state the ontological reason (SLICE_GUIDE §6.6)."
                        ),
                    ));
                } else {
                    clean += 1;
                }
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = if total == 0 {
        1.0
    } else {
        clean as f64 / total as f64
    };
    AxisScore { score, findings }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_maps_group_a_producers() {
        for key in IMPLEMENTED {
            assert!(resolve(key).is_some(), "{key} resolves to a primitive");
        }
        assert!(
            resolve("no_such_producer").is_none(),
            "unknown producer → None (hard fail upstream)"
        );
    }

    #[test]
    fn boundary_detection() {
        assert!(states_boundary("A widget. It is NOT a gadget."));
        assert!(states_boundary("A relator, never a mere pair."));
        assert!(!states_boundary("A widget of the system."));
    }

    #[test]
    fn worked_triple_detection() {
        assert!(is_worked_triple("ex:x a gmeow:Foo ."));
        assert!(!is_worked_triple("a plain sentence with no triple"));
    }
}
