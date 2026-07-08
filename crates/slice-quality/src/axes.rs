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
        "linkage_axis" => Some(linkage_axis),
        "projection_axis" => Some(projection_axis),
        "testing_axis" => Some(testing_axis),
        "documentation_axis" => Some(documentation_axis),
        "translation_axis" => Some(translation_axis),
        "reasoner_axis" => Some(crate::reasoner::reasoner_axis),
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
    "linkage_axis",
    "projection_axis",
    "testing_axis",
    "documentation_axis",
    "translation_axis",
    "reasoner_axis",
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
        // Boundary-stating definitions and worked examples are a TBox-term bar
        // (SLICE_GUIDE §6.2/§6.6); A-Box value-vocabulary individuals (tiers,
        // dimensions, thresholds) carry their lighter coat, not the boundary rule.
        if !is_tbox_term(ctx, iri) {
            continue;
        }
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

// ── Axis 4: Maximal linkage ────────────────────────────────────────────────

/// Namespaces that never require an external alignment (GMEOW-native + standard
/// value vocabularies): a term here is covered by authorship, not by a mapping.
const NATIVE_NS: &[&str] = &[
    // Every blackcatinformatics.ca IRI is GMEOW-native — the gmeow: super-vocabulary,
    // the logic:/lang:/math: grounding layers, and the ontology/slice root IRIs
    // (some of which have no trailing slash, e.g. the …/gmeow ontology root).
    "https://blackcatinformatics.ca/",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "http://www.w3.org/2000/01/rdf-schema#",
    "http://www.w3.org/2002/07/owl#",
    "http://www.w3.org/2001/XMLSchema#",
    "http://www.w3.org/2004/02/skos/core#",
    "http://purl.org/dc/terms/",
];

fn is_native(iri: &str) -> bool {
    NATIVE_NS.iter().any(|ns| iri.starts_with(ns))
}

/// Read the slice's mapping file text, if present.
fn mappings_text(ctx: &ScoreContext) -> Option<String> {
    std::fs::read_to_string(ctx.slice_dir.join("mappings/equivalences.ttl")).ok()
}

/// The external (non-native) IRIs the slice's OWN terms reference — the slice's
/// alignment surface. Example-fixture data (subjects that are not slice terms) is
/// illustrative, not an alignment obligation, so it is excluded.
fn external_alignment_surface(ctx: &ScoreContext) -> std::collections::BTreeSet<String> {
    let ds = ctx.graph;
    let terms: std::collections::BTreeSet<&str> = ctx.terms.iter().map(String::as_str).collect();
    let mut external = std::collections::BTreeSet::new();
    for iri in &ctx.terms {
        let Some(sid) = id(ds, iri) else { continue };
        for q in ds.quads_for_pattern(Some(sid), None, None, GraphMatch::Any) {
            for t in [ds.resolve(q.p), ds.resolve(q.o)] {
                if let TermRef::Iri(t_iri) = t
                    && !is_native(t_iri)
                    && !terms.contains(t_iri)
                {
                    external.insert(t_iri.to_owned());
                }
            }
        }
    }
    external
}

/// Linkage: of the external IRIs the slice's own vocabulary references, the
/// fraction aligned in its mapping file. A slice referencing no external terms is
/// vacuously fully linked.
fn linkage_axis(ctx: &ScoreContext) -> AxisScore {
    let external = external_alignment_surface(ctx);
    if external.is_empty() {
        return AxisScore::clean(1.0);
    }
    let map_text = mappings_text(ctx).unwrap_or_default();
    let mut covered = 0usize;
    let mut findings = Vec::new();
    for iri in &external {
        if map_text.contains(iri.as_str()) {
            covered += 1;
        } else {
            findings.push(advisory(
                "slice-quality.linkage.unmapped-external",
                format!("external term {iri} has no alignment in mappings/equivalences.ttl (Principle 17)."),
            ));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = covered as f64 / external.len() as f64;
    AxisScore { score, findings }
}

// ── Axis 5: Maximal projection ─────────────────────────────────────────────

/// Projection: does the slice provide its projectable source surfaces? SHACL
/// shapes when it has structural constraints, mappings when it links out.
fn projection_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    let mut expected = 0usize;
    let mut present = 0usize;
    let mut findings = Vec::new();

    // A slice with owl:Restriction / disjointness should source SHACL from logic:.
    let has_constraints = id(ds, "http://www.w3.org/2002/07/owl#Restriction")
        .is_some_and(|c| id(ds, graph::RDF_TYPE).is_some_and(|t| graph::has_any_object(ds, t, c)))
        || id(ds, "http://www.w3.org/2002/07/owl#disjointWith")
            .is_some_and(|p| graph::predicate_used(ds, p));
    if has_constraints {
        expected += 1;
        if ctx.slice_dir.join("shapes.ttl").is_file() {
            present += 1;
        } else {
            findings.push(advisory(
                "slice-quality.projection.no-shapes",
                "the slice declares structural constraints but ships no shapes.ttl projection source (projection purity).".to_owned(),
            ));
        }
    }
    // A slice whose own vocabulary links out should carry a mapping file.
    let links_out = !external_alignment_surface(ctx).is_empty();
    if links_out {
        expected += 1;
        if ctx.slice_dir.join("mappings/equivalences.ttl").is_file() {
            present += 1;
        } else {
            findings.push(advisory(
                "slice-quality.projection.no-mappings",
                "the slice links to external terms but ships no mappings/equivalences.ttl projection source (Principles 4/7/17).".to_owned(),
            ));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = if expected == 0 {
        1.0
    } else {
        present as f64 / expected as f64
    };
    AxisScore { score, findings }
}

// ── Axis 6: Optimal testing ────────────────────────────────────────────────

/// Concatenated text of every `.ttl`/`.rq` under `tests/` and `queries/`.
fn test_corpus(ctx: &ScoreContext) -> String {
    let mut buf = String::new();
    for sub in ["tests", "queries"] {
        collect_text(&ctx.slice_dir.join(sub), &mut buf);
    }
    buf
}

fn collect_text(dir: &std::path::Path, buf: &mut String) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_text(&p, buf);
        } else if p.extension().is_some_and(|x| x == "ttl" || x == "rq")
            && let Ok(t) = std::fs::read_to_string(&p)
        {
            buf.push_str(&t);
            buf.push('\n');
        }
    }
}

/// Testing: fraction of the slice's terms named by at least one test cell / query.
fn testing_axis(ctx: &ScoreContext) -> AxisScore {
    if ctx.terms.is_empty() {
        return AxisScore::clean(0.0);
    }
    let corpus = test_corpus(ctx);
    if corpus.is_empty() {
        return AxisScore {
            score: 0.0,
            findings: vec![advisory(
                "slice-quality.testing.no-cells",
                "the slice ships no test cells or competency queries (SLICE_QA).".to_owned(),
            )],
        };
    }
    let mut reached = 0usize;
    let mut findings = Vec::new();
    for iri in &ctx.terms {
        let local = iri.rsplit(['/', '#']).next().unwrap_or(iri);
        if corpus.contains(local) {
            reached += 1;
        } else {
            findings.push(advisory(
                "slice-quality.testing.untested-term",
                format!(
                    "{iri} is exercised by no competency/structural/example cell (SLICE_GUIDE §9)."
                ),
            ));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = reached as f64 / ctx.terms.len() as f64;
    AxisScore { score, findings }
}

// ── Axis 7: Documentation ──────────────────────────────────────────────────

/// A narrative thesis is a docs.md with substantive prose beyond its headings.
fn documentation_axis(ctx: &ScoreContext) -> AxisScore {
    let mut checks = 0usize;
    let mut passed = 0usize;
    let mut findings = Vec::new();

    checks += 1;
    match std::fs::read_to_string(ctx.slice_dir.join("docs.md")) {
        Ok(md) => {
            let prose: usize = md
                .lines()
                .filter(|l| !l.trim_start().starts_with('#') && !l.trim_start().starts_with("<!--"))
                .map(str::len)
                .sum();
            if prose > 200 {
                passed += 1;
            } else {
                findings.push(advisory(
                    "slice-quality.documentation.thin-thesis",
                    "docs.md carries no narrative thesis (SLICE_GUIDE documentation doctrine)."
                        .to_owned(),
                ));
            }
        }
        Err(_) => findings.push(advisory(
            "slice-quality.documentation.no-docs",
            "the slice ships no docs.md.".to_owned(),
        )),
    }
    #[allow(clippy::cast_precision_loss)]
    let score = if checks == 0 {
        0.0
    } else {
        passed as f64 / checks as f64
    };
    AxisScore { score, findings }
}

// ── Axis 9: Translation coverage ───────────────────────────────────────────

/// The project languages beyond the authored English.
const TRANSLATION_LANGS: &[&str] = &["fr", "zh"];

/// Translation: per-language coverage of the slice's label+definition literals,
/// audited via `.po` catalogs. Full coverage requires English (authored) plus
/// French and Mandarin.
fn translation_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    let label_p = id(ds, "http://www.w3.org/2000/01/rdf-schema#label");
    let def_p = id(ds, "http://www.w3.org/2004/02/skos/core#definition");
    let mut expected = 0usize;
    for iri in &ctx.terms {
        let Some(sid) = id(ds, iri) else { continue };
        if label_p.is_some_and(|p| graph::has_any(ds, sid, p)) {
            expected += 1;
        }
        if def_p.is_some_and(|p| graph::has_any(ds, sid, p)) {
            expected += 1;
        }
    }
    if expected == 0 {
        return AxisScore::clean(1.0);
    }

    // English is authored, so it is always fully covered.
    let mut lang_cov = vec![1.0_f64];
    let mut findings = Vec::new();
    for lang in TRANSLATION_LANGS {
        let po = ctx.slice_dir.join(format!("i18n/{lang}.po"));
        let entries = std::fs::read_to_string(&po)
            .map(|t| {
                t.lines()
                    .filter(|l| {
                        l.starts_with("msgctxt")
                            && (l.contains("|rdfs:label") || l.contains("|skos:definition"))
                    })
                    .count()
            })
            .unwrap_or(0);
        #[allow(clippy::cast_precision_loss)]
        let cov = (entries as f64 / expected as f64).min(1.0);
        if cov < 1.0 {
            findings.push(advisory(
                "slice-quality.translation.incomplete",
                format!("{lang} covers {entries}/{expected} label+definition literals; the top tier requires 100% en+fr+cmn."),
            ));
        }
        lang_cov.push(cov);
    }
    #[allow(clippy::cast_precision_loss)]
    let score = lang_cov.iter().sum::<f64>() / lang_cov.len() as f64;
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
