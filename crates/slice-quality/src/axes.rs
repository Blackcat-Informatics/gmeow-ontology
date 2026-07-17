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

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use gmeow_lang_bridge::{
    Gmn0Model, GmnDictionary, GmnGlyphRegistry, gmn_glyph_token_cost, measure_coverage,
};
use gmeow_logic_compile::projections::correspondence::extract_correspondences;
use gmeow_logic_compile::projections::correspondence_soundness::{Mapping, lint_dc_refinement};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef};
use regex::Regex;

use crate::counting;
use crate::graph::{self, all_iris, all_lits, g, id, instances_of, one_iri, one_lit};
use crate::score::{AxisScore, ScoreContext, ScoringEnv, advisory};

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
        "shape_migration_axis" => Some(shape_migration_axis),
        "testing_axis" => Some(testing_axis),
        "documentation_axis" => Some(documentation_axis),
        "translation_axis" => Some(translation_axis),
        "reasoner_axis" => Some(crate::reasoner::reasoner_axis),
        "flagship_counterexample_depth_axis" => Some(flagship_counterexample_depth_axis),
        "gmn1_coverage_axis" => Some(gmn1_coverage_axis),
        "gmn_glyph_optimality_axis" => Some(gmn_glyph_optimality_axis),
        "DocMaturity" => Some(crate::doc_maturity::DocMaturity::axis),
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
    "shape_migration_axis",
    "testing_axis",
    "documentation_axis",
    "translation_axis",
    "reasoner_axis",
    "flagship_counterexample_depth_axis",
    "gmn1_coverage_axis",
    "gmn_glyph_optimality_axis",
    "DocMaturity",
];

// ── Axis 1: Maximal grounding ─────────────────────────────────────────────

/// The `logic:` foundation-stereotype types a grounded class may carry.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
/// The one slice whose owned `logic:` classes constitute the foundation itself.
const LOGIC_SLICE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";

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
        let is_logic_foundation = ctx.slice_iri == LOGIC_SLICE_IRI && class.starts_with(LOGIC_NS);
        let has_stereotype = is_logic_foundation
            || ds
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

fn is_generic_usage_coat(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "when the modeled statement satisfies the scope and necessary conditions stated in this term's definition",
        "for a merely similar construct whose identity, truth conditions, or validation contract differs",
        "with its declared owl kind and preserve its domain, range, standpoint, and provenance constraints",
    ]
    .iter()
    .any(|template| value.contains(template))
}

fn coat_field_is_substantive(
    ds: &RdfDataset,
    subject: purrdf::TermId,
    label: &str,
    predicate: purrdf::TermId,
) -> bool {
    match label {
        "skos:example" => all_lits(ds, subject, predicate)
            .iter()
            .any(|value| is_worked_triple(value)),
        "useWhen" | "avoidWhen" | "howToUse" => all_lits(ds, subject, predicate)
            .iter()
            .any(|value| !value.trim().is_empty() && !is_generic_usage_coat(value)),
        _ => graph::has_any(ds, subject, predicate),
    }
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
            let present =
                id(ds, pred_iri).is_some_and(|p| coat_field_is_substantive(ds, sid, label, p));
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

// ── Word-boundary matching (shared by the ratchet-gated heuristics) ─────────

/// True if `word` occurs in `corpus` at identifier/word boundaries — the char on
/// each side of the match is neither an ASCII alphanumeric nor `_`/`-`. This is the
/// discriminator that keeps an INCIDENTAL substring (`"whenever"` containing
/// `"never"`, `"NOTE"` containing `"not"`, `FooBar` containing `Foo`) from counting
/// as a real occurrence — critical because every caller feeds a ratchet-gated score,
/// where a false positive silently inflates the tier. Phrase words (e.g.
/// `"rather than"`) match as a contiguous span, their outer ends boundary-checked.
fn word_at_boundary(corpus: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    corpus.match_indices(word).any(|(idx, _)| {
        let before = corpus[..idx].chars().next_back();
        let after = corpus[idx + word.len()..].chars().next();
        before.is_none_or(|c| !is_ident(c)) && after.is_none_or(|c| !is_ident(c))
    })
}

/// True if `s` carries a turtle CURIE token (`prefix:local`): a `:` with a name
/// char before it and an alphanumeric/`_` after. Deliberately conservative — it
/// rejects a bare prose colon (`"section 3: ..."`) and a full-IRI scheme
/// (`<http://…>`, whose `:` is followed by `/`), so a definition without a real
/// term reference is not mistaken for a worked triple.
fn has_curie(s: &str) -> bool {
    let bytes = s.as_bytes();
    let is_name = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    for (i, &c) in bytes.iter().enumerate() {
        if c != b':' {
            continue;
        }
        let before = i.checked_sub(1).map(|j| bytes[j]);
        let after = bytes.get(i + 1).copied();
        if before.is_some_and(is_name)
            && after.is_some_and(|a| a.is_ascii_alphanumeric() || a == b'_')
        {
            return true;
        }
    }
    false
}

// ── Axis 8: Prose quality ──────────────────────────────────────────────────

/// Negation cues that signal a boundary-stating ("what it is NOT") definition.
///
/// Cues are matched at word boundaries on the lowercased text ([`word_at_boundary`]),
/// so `"whenever"` no longer counts as `"never"` and `"NOTE"` no longer counts as
/// `"not"`. The heuristic is deliberately CONSERVATIVE (it prefers a false negative
/// to a false positive): the score it feeds is ratchet-gated, so wrongly passing a
/// non-boundary definition would silently inflate the tier, whereas missing a
/// boundary phrased with an unlisted cue only under-credits and stays advisory.
fn states_boundary(def: &str) -> bool {
    const CUES: &[&str] = &[
        "not",
        "never",
        "nor",
        "cannot",
        "rather than",
        "as opposed to",
        "instead of",
        "unlike",
        "distinct from",
    ];
    let d = def.to_lowercase();
    // A term-agnostic coat is not a semantic boundary. This exact family was
    // mechanically appended to hundreds of definitions and says nothing that
    // distinguishes one term from another, despite containing the cue "not".
    if d.contains(
        "not an interchangeable alias for a broader, narrower, or merely related construct",
    ) {
        return false;
    }
    CUES.iter().any(|cue| word_at_boundary(&d, cue))
}

/// A worked triple names a term via a CURIE (`prefix:local`) and carries turtle
/// statement structure (the `a` type keyword or a `; , .` terminator).
///
/// Conservative on both axes: a bare prose colon is not a CURIE ([`has_curie`]), and
/// prose punctuation alone does not pass without a CURIE present — so an ordinary
/// sentence (`"See section 3: important."`) is NOT scored as a worked triple. As with
/// [`states_boundary`], a false negative (under-crediting an oddly-formatted example)
/// is preferred to a false positive that would inflate this ratchet-gated score.
fn is_worked_triple(example: &str) -> bool {
    // `term rdfs:isDefinedBy slice` is ownership metadata, not an example of
    // the term in use. Counting it lets a generated provenance inventory pose
    // as hundreds of worked examples.
    !example.contains("rdfs:isDefinedBy")
        && has_curie(example)
        && (word_at_boundary(example, "a")
            || example.contains(" ;")
            || example.contains(" .")
            || example.contains(" ,")
            || example.ends_with('.')
            || example.ends_with(';'))
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
///
/// The pattern is a compile-time-constant literal; the `.expect` fires only if this
/// exact literal is malformed, which is a programming error caught in CI by
/// [`tests::test_artifact_regex_is_valid`] (which forces the `LazyLock` to compile),
/// never a data-dependent runtime panic on the library path.
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

/// The `gmeow:` super-vocabulary namespace (correspondence-cell vocabulary lives here).
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
/// The `dc:` DCMI-elements-1.1 namespace whose alignments the dumb-down calculus derives.
const DC_ELEMENTS_NS: &str = "http://purl.org/dc/elements/1.1/";
/// The `dcterms:` namespace (the refinement source the elements alignments dumb down from).
const DCTERMS_NS: &str = "http://purl.org/dc/terms/";

/// The identity-strength `gmeow:alignPredicate` values — the *only* alignment strengths a
/// correspondence lens can carry lawfully (an invertible `put ∘ get = id_S` rename). A
/// lossy `skos:closeMatch`/`relatedMatch`/`broadMatch`/`narrowMatch` is by construction
/// outside the calculus's remit, so it is never a migration target and never enters the
/// adoption population.
const IDENTITY_ALIGN_PREDICATES: &[&str] = &[
    "http://www.w3.org/2004/02/skos/core#exactMatch",
    "http://www.w3.org/2002/07/owl#equivalentClass",
    "http://www.w3.org/2002/07/owl#equivalentProperty",
];

/// Parse the slice's OWN authored correspondence surface — every `.ttl` under `mappings/`
/// plus `module.ttl` — into one dataset. This is where a slice authors its alignments:
/// `gmeow:ProjectionMapping` cells (the EDOAL/pattern form the correspondence-lowering
/// calculus consumes), `gmeow:TermEquivalence` rows (hand-authored SSSOM curation), and any
/// `logic:Correspondence` lens. Only THIS slice's directory is read, so every record found
/// is slice-owned (single-slice scope — the metric never advises on another slice's surface).
fn correspondence_surface(ctx: &ScoreContext) -> Option<std::sync::Arc<RdfDataset>> {
    let mut paths = Vec::new();
    let module = ctx.slice_dir.join("module.ttl");
    if module.is_file() {
        paths.push(module);
    }
    collect_mapping_ttl(&ctx.slice_dir.join("mappings"), &mut paths);
    if paths.is_empty() {
        return None;
    }
    paths.sort();
    let refs: Vec<&Path> = paths.iter().map(std::path::PathBuf::as_path).collect();
    crate::dataset_from_paths(&refs).ok()
}

/// Collect every `.ttl` under a mappings directory, recursively.
fn collect_mapping_ttl(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_mapping_ttl(&p, out);
        } else if p.extension().is_some_and(|x| x == "ttl") {
            out.push(p);
        }
    }
}

/// True if the `gmeow:hasBinding` object `binding` is a lawful FACT-rename binding — its
/// `gmeow:relation` is `"="` or it is flagged `gmeow:mnemomorphic true`. Exactly the
/// bindings the mappings stage promotes to a discharged `logic:SectionLaw` (the numerator
/// condition `discharged_section_cells_from_triples` credits, read at the authoring surface).
fn binding_is_mnemomorphic(ds: &RdfDataset, binding: purrdf::TermId) -> bool {
    let relation_eq = id(ds, &g("relation"))
        .into_iter()
        .flat_map(|p| graph::all_lits(ds, binding, p))
        .any(|l| l == "=");
    let mnemomorphic = id(ds, &g("mnemomorphic"))
        .into_iter()
        .flat_map(|p| graph::all_lits(ds, binding, p))
        .any(|l| l == "true");
    relation_eq || mnemomorphic
}

/// The `gmeow:ProjectionMapping` cells whose binding is a lawful FACT rename (mnemomorphic
/// `=`) — the structured EDOAL cells the correspondence-lowering calculus lifts to a
/// discharged `logic:SectionLaw`. Lossy `<=` projection cells are NOT identity-strength and
/// never enter the population. Sorted (cell IRI order).
fn mnemomorphic_projection_cells(ds: &RdfDataset) -> BTreeSet<String> {
    let has_binding = id(ds, &g("hasBinding"));
    let marked_grounding: BTreeSet<String> =
        instances_of(ds, &format!("{LOGIC_NS}GroundingCorrespondence"))
            .into_iter()
            .collect();
    let valid_grounding = crate::grounding::validated_grounding_cells(ds);
    let mut out = BTreeSet::new();
    for cell in instances_of(ds, &g("ProjectionMapping")) {
        // A grounding marker strengthens the authoring contract. It cannot fall back to
        // the permissive ordinary ProjectionMapping path when its envelope is malformed.
        if marked_grounding.contains(&cell) && !valid_grounding.contains(&cell) {
            continue;
        }
        let Some(sid) = id(ds, &cell) else { continue };
        let routed = has_binding.is_some_and(|hb| {
            ds.quads_for_pattern(Some(sid), Some(hb), None, GraphMatch::Any)
                .any(|q| binding_is_mnemomorphic(ds, q.o))
        });
        if routed {
            out.insert(cell);
        }
    }
    out
}

/// One legacy (hand-authored, identity-strength) alignment record the slice owns: the
/// `gmeow:TermEquivalence` / mapping IRI plus the human detail naming its migration target.
struct LegacyRecord {
    record_iri: String,
    detail: String,
}

/// The slice's `gmeow:TermEquivalence` rows at identity strength. A complete
/// `logic:GroundingCorrespondence` envelope is removed from this legacy set after it is
/// credited to the calculus; an ordinary row remains a real migration target.
fn identity_hand_authored(ds: &RdfDataset) -> Vec<LegacyRecord> {
    let identity: BTreeSet<&str> = IDENTITY_ALIGN_PREDICATES.iter().copied().collect();
    let mut out = Vec::new();
    for record in instances_of(ds, &g("TermEquivalence")) {
        let Some(sid) = id(ds, &record) else { continue };
        let Some(pred) = id(ds, &g("alignPredicate")).and_then(|p| one_iri(ds, sid, p)) else {
            continue;
        };
        if !identity.contains(pred.as_str()) {
            continue;
        }
        let subj = id(ds, &g("alignSubject"))
            .and_then(|p| one_iri(ds, sid, p))
            .unwrap_or_default();
        let obj = id(ds, &g("alignObject"))
            .and_then(|p| one_iri(ds, sid, p))
            .unwrap_or_default();
        out.push(LegacyRecord {
            record_iri: record.clone(),
            detail: format!(
                "{record} is a hand-authored identity-strength alignment ({subj} → {obj} via {pred}) not routed through the correspondence calculus — either author a complete logic:GroundingCorrespondence envelope for a shipped grounding law or lift it to a gmeow:ProjectionMapping mnemomorphic \"=\" cell so the section-law discharge proves the rename lawful (Principle 17)."
            ),
        });
    }
    out.sort_by(|a, b| a.record_iri.cmp(&b.record_iri));
    out
}

/// Compress a `dc:`/`dcterms:` IRI to the CURIE form `lint_dc_refinement` matches on; any
/// other IRI is returned unchanged (only the `dc:` object drives the hand-authored check).
fn dc_curie(iri: &str) -> String {
    if let Some(local) = iri.strip_prefix(DC_ELEMENTS_NS) {
        format!("dc:{local}")
    } else if let Some(local) = iri.strip_prefix(DCTERMS_NS) {
        format!("dcterms:{local}")
    } else {
        iri.to_owned()
    }
}

/// The slice's `dc:` (DCMI-elements-1.1) hand-authored alignments, named via the real
/// `lint_dc_refinement` dumb-down lint (code `dc-hand-authored`): a `dc:` element alignment
/// the `dcterms:`→`dc:` sub-property derivation should have produced, authored by hand
/// instead. Builds `Mapping` rows from the slice's `gmeow:TermEquivalence` records, runs the
/// lint, and maps each flagged row back to its record IRI. Sorted by record IRI.
fn dc_hand_authored(ds: &RdfDataset) -> Vec<LegacyRecord> {
    // Build one Mapping row per TermEquivalence, keyed back to its record IRI.
    let mut rows: Vec<(Mapping, String)> = Vec::new();
    for record in instances_of(ds, &g("TermEquivalence")) {
        let Some(sid) = id(ds, &record) else { continue };
        let field = |local: &str| {
            id(ds, &g(local))
                .and_then(|p| one_iri(ds, sid, p))
                .map(|iri| dc_curie(&iri))
                .unwrap_or_default()
        };
        rows.push((
            Mapping {
                subject_id: field("alignSubject"),
                predicate_id: field("alignPredicate"),
                object_id: field("alignObject"),
                confidence: String::new(),
                mapping_justification: String::new(),
            },
            record,
        ));
    }
    let mappings: Vec<Mapping> = rows.iter().map(|(m, _)| m.clone()).collect();
    let mut out = Vec::new();
    for diag in lint_dc_refinement(&mappings) {
        if diag.code != "dc-hand-authored" {
            continue;
        }
        // Match the flagged row back to its TermEquivalence record by subject+object CURIE.
        let record_iri = rows
            .iter()
            .find(|(m, _)| {
                Some(&m.object_id) == diag.object_id.as_ref()
                    && diag.subject_id.as_ref().is_none_or(|s| s == &m.subject_id)
            })
            .map(|(_, iri)| iri.clone())
            .unwrap_or_default();
        out.push(LegacyRecord {
            record_iri: record_iri.clone(),
            detail: format!("{record_iri} is a hand-authored dc: alignment not routed through the correspondence calculus: {} Route it through the dcterms:→dc: dumb-down derivation rather than authoring the dc: alignment by hand (Principle 17).", diag.message),
        });
    }
    out.sort_by(|a, b| a.record_iri.cmp(&b.record_iri));
    out
}

/// Linkage — correspondence-calculus adoption. Of the identity-strength correspondences the
/// slice authors (the only alignments a lens can carry lawfully), the fraction routed through
/// the correspondence CALCULUS rather than hand-authored:
///
///   adoption = |calculus-routed| / |calculus-routed ∪ hand-authored identity records|
///
/// * **Calculus-routed** (numerator): the `logic:Correspondence` lens individuals
///   ([`extract_correspondences`]); complete identity-strength
///   `logic:GroundingCorrespondence` frontend cells admitted by the shared fail-closed
///   grounding validator; and the `gmeow:ProjectionMapping` cells whose binding is a lawful
///   FACT rename (mnemomorphic `=` — the cells the mappings stage lifts to a discharged
///   `logic:SectionLaw`).
/// * **Hand-authored** (the migration targets): identity-strength `gmeow:TermEquivalence` rows
///   and `dc:` alignments the dumb-down calculus should derive ([`lint_dc_refinement`]).
///
/// Only this slice's `mappings/` + `module.ttl` are read, so every record is slice-owned.
/// A slice with no identity-strength correspondence surface (none, or only lossy
/// `closeMatch`-class alignments the calculus cannot carry) has an empty population: the axis
/// is not applicable, so it takes the crate's neutral-for-the-meet vacuity score (1.0) but —
/// unlike a real full-adoption pass — carries an explicit advisory that the axis is vacuous,
/// so the 1.0 is never silently read as "fully linked".
fn linkage_axis(ctx: &ScoreContext) -> AxisScore {
    let Some(ds) = correspondence_surface(ctx) else {
        return AxisScore {
            score: 1.0,
            findings: vec![advisory(
                "slice-quality.linkage.no-correspondence-surface",
                "the slice authors no mapping/correspondence surface (no mappings/ or module.ttl) — the correspondence-calculus adoption axis is not applicable (vacuous 1.0).".to_owned(),
            )],
        };
    };
    let ds: &RdfDataset = &ds;

    // Numerator: the calculus-routed correspondences the slice owns.
    let (correspondences, _malformed) = extract_correspondences(ds);
    let mut calculus: BTreeSet<String> = correspondences
        .into_iter()
        .filter(|c| c.iri.starts_with(GMEOW_NS))
        .map(|c| c.iri)
        .collect();
    calculus.extend(mnemomorphic_projection_cells(ds));
    let term_cells: BTreeSet<String> = instances_of(ds, &g("TermEquivalence"))
        .into_iter()
        .collect();
    let identity: BTreeSet<&str> = IDENTITY_ALIGN_PREDICATES.iter().copied().collect();
    calculus.extend(
        crate::grounding::validated_grounding_cells(ds)
            .into_iter()
            .filter(|cell| term_cells.contains(cell))
            .filter(|cell| {
                id(ds, cell)
                    .and_then(|sid| id(ds, &g("alignPredicate")).and_then(|p| one_iri(ds, sid, p)))
                    .is_some_and(|predicate| identity.contains(predicate.as_str()))
            }),
    );

    // Legacy: the hand-authored identity-strength records — the migration targets.
    let mut legacy: BTreeMap<String, String> = BTreeMap::new();
    for rec in identity_hand_authored(ds) {
        legacy.entry(rec.record_iri).or_insert(rec.detail);
    }
    for rec in dc_hand_authored(ds) {
        legacy.entry(rec.record_iri).or_insert(rec.detail);
    }
    // A valid identity GroundingCorrespondence deliberately appears in both frontend sets;
    // calculus ownership wins, while an incomplete marker remains legacy debt.
    for iri in &calculus {
        legacy.remove(iri);
    }

    let denom = calculus.len() + legacy.len();
    if denom == 0 {
        return AxisScore {
            score: 1.0,
            findings: vec![advisory(
                "slice-quality.linkage.no-calculus-eligible-correspondence",
                "the slice authors no identity-strength correspondence (none, or only lossy closeMatch-class alignments the lens calculus cannot carry) — the correspondence-calculus adoption axis is not applicable (vacuous 1.0), not a full-adoption pass.".to_owned(),
            )],
        };
    }

    let findings: Vec<_> = legacy
        .into_values()
        .map(|detail| advisory("slice-quality.linkage.uncalculated-correspondence", detail))
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let score = calculus.len() as f64 / denom as f64;
    AxisScore { score, findings }
}

// ── Axis 5: Maximal projection ─────────────────────────────────────────────

/// Projection: does the slice express its validation as a pure projection? For
/// structural constraints that means the `module.ttl` OWL/RDFS axioms alone
/// (projected to `generated/shapes/*`) with the hand-authored `shapes.ttl`
/// retired — a slice still shipping that second source scores a shortfall here;
/// plus mappings when it links out.
fn projection_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    let mut expected = 0usize;
    let mut present = 0usize;
    let mut findings = Vec::new();

    // Principle-17 migration debt (advisory): a hand-authored `shapes.ttl` is a SECOND source of
    // truth. Validation must be authored in `logic:` (a `logic:Constraint` or the OWL/RDFS axioms
    // the declarative shapes derive from) and PROJECTED to `generated/shapes/*`, never carried as a
    // parallel hand-authored SHACL surface. A slice still shipping a hand-authored `shapes.ttl`
    // carries per-slice migration debt for a future quality pass to discharge. This is advisory —
    // the shapes stay live/enforced until the slice's constraints are migrated (equivalence before
    // deletion); the finding is the tracked pressure, not a gate.
    if ctx.slice_dir.join("shapes.ttl").is_file() {
        findings.push(advisory(
            "slice-quality.projection.hand-authored-shapes",
            "this slice ships a hand-authored shapes.ttl (a second source of truth): migrate its \
             constraints to logic: (a logic:Constraint or the backing OWL/RDFS axioms in \
             module.ttl) so they PROJECT to generated/shapes/* (Principle 17), then retire the \
             hand-authored file. See docs/MIGRATING-SHAPES-TO-LOGIC.md."
                .to_owned(),
        ));
    }

    // A slice with owl:Restriction / disjointness carries structural constraints. Under Principle
    // 17 its validation is "maximally projected" only when those constraints live SOLELY as the
    // OWL/RDFS axioms in module.ttl (which derive generated/shapes/* via `derive_validation_shapes`)
    // with NO parallel hand-authored shapes.ttl. So this projectable-source obligation is MET only
    // once the hand-authored SHACL is retired: a constraint-bearing slice still shipping a
    // shapes.ttl has not projected its validation and scores a shortfall here (the debt finding
    // above names the file). This keeps the term falsifiable and aligns it with migration —
    // retiring shapes.ttl is the ONLY way to earn the credit, and can never lose it.
    let has_constraints = id(ds, "http://www.w3.org/2002/07/owl#Restriction")
        .is_some_and(|c| id(ds, graph::RDF_TYPE).is_some_and(|t| graph::has_any_object(ds, t, c)))
        || id(ds, "http://www.w3.org/2002/07/owl#disjointWith")
            .is_some_and(|p| graph::predicate_used(ds, p));
    if has_constraints {
        expected += 1;
        if !ctx.slice_dir.join("shapes.ttl").is_file() {
            present += 1;
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

// ── Axis: Shape migration (authored shapes → logic: projection) ─────────────

/// Shape migration: the fraction of a slice's hand-authored `shapes.ttl`
/// `sh:NodeShape` / `sh:PropertyShape` blocks that are GROUNDED — carry a
/// `logic:formalizes` back-reference (the same criterion the blanket
/// projection-purity gate enforces).
///
/// A hand-authored validation shape without `logic:formalizes` is a second source of truth: the
/// SHACL / ShEx surfaces are generated lossy projections of the `logic:` canon (Principle 17),
/// not a place to hand-author constraints. Each un-backed shape is named as a migration target —
/// author its cardinality / class / datatype obligation in the owning `module.ttl` (reasoner-safe:
/// `owl:FunctionalProperty` for at-most-one, `owl:someValuesFrom` for existence, NEVER
/// `owl:cardinality`, which reds `reason-verify`) so the projector reproduces it and the block is
/// deleted; a genuine ValidationOnly residue (exactly-N cardinality, node-level `sh:or`, a
/// cross-node `sh:sparql`) instead carries `logic:formalizes` naming its canonical `logic:` source
/// (`docs/SLICE_GUIDE.md` §grounding a shape).
///
/// This reads through the shared [`crate::counting`] enumerator at
/// [`crate::counting::CountMode::Historical`] — the SAME primitive the projection-vocabulary
/// ratchet gate calls at full-residue scope — so "what is a shape" and "what counts as
/// grounded" are decided in exactly one place. `Historical` mode pins every divergence
/// dimension to this axis's pre-existing behaviour (typed shapes only, presence-only
/// grounding, no bridge subtraction), so the measured score here is bit-identical to
/// before the refactor.
fn shape_migration_axis(ctx: &ScoreContext) -> AxisScore {
    let shapes_path = ctx.slice_dir.join("shapes.ttl");
    if !shapes_path.is_file() {
        return AxisScore::clean(1.0); // no authored shape surface → nothing to migrate
    }
    let Ok(bytes) = std::fs::read(&shapes_path) else {
        return AxisScore::clean(1.0);
    };
    let Ok(ds) = purrdf::parse_dataset(&bytes, "text/turtle", None) else {
        // A malformed shapes.ttl surfaces as a validation error on another gate, not here.
        return AxisScore::clean(1.0);
    };
    let constructs = counting::enumerate(
        &ds,
        &counting::shacl_vocab(),
        counting::CountMode::Historical,
        // Historical (advisory) scope does no bridge/owner subtraction, so the surface
        // IRI is unused here.
        "",
    );
    if constructs.is_empty() {
        return AxisScore::clean(1.0);
    }
    let mut findings = Vec::new();
    let mut grounded = 0usize;
    for shape in &constructs {
        if shape.grounded {
            grounded += 1;
        } else {
            let iri = &shape.key;
            findings.push(advisory(
                "slice-quality.projection.ungrounded-shape",
                format!(
                    "hand-authored validation shape <{iri}> carries no logic:formalizes: migrate \
                     its obligation into module.ttl (owl:FunctionalProperty / owl:someValuesFrom — \
                     never owl:cardinality) so the projector reproduces it and the block is deleted, \
                     or back a genuine ValidationOnly residue with logic:formalizes (SLICE_GUIDE.md)."
                ),
            ));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = grounded as f64 / constructs.len() as f64;
    AxisScore { score, findings }
}

// ── Axis 6: Optimal testing ────────────────────────────────────────────────

/// Concatenated semantic text of every `.ttl`/`.rq` under `tests/` and
/// `queries/`. Comment-only mentions and SPARQL `VALUES` inventories are erased:
/// neither executes an assertion about an individual term, and counting them
/// rewards exhaustive name lists rather than test behaviour.
fn test_corpus(ctx: &ScoreContext) -> String {
    let mut buf = String::new();
    for sub in ["tests", "queries"] {
        collect_text(&ctx.slice_dir.join(sub), &mut buf);
    }
    strip_non_executing_test_mentions(&buf)
}

fn strip_non_executing_test_mentions(corpus: &str) -> String {
    let mut bytes = corpus.as_bytes().to_vec();

    // Strip hash comments when `#` begins a token. A namespace fragment such as
    // `<https://example/#Foo>` is retained because its `#` is not preceded by
    // whitespace. Replacing with spaces preserves identifier boundaries.
    let mut line_start = 0usize;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| line_start + offset);
        let mut comment = None;
        for idx in line_start..line_end {
            if bytes[idx] == b'#' && (idx == line_start || bytes[idx - 1].is_ascii_whitespace()) {
                comment = Some(idx);
                break;
            }
        }
        if let Some(start) = comment {
            bytes[start..line_end].fill(b' ');
        }
        line_start = line_end.saturating_add(1);
    }

    // Erase complete SPARQL VALUES clauses with a small brace-aware scanner.
    // VALUES is ASCII by grammar; byte replacement therefore cannot split any
    // retained UTF-8 scalar.
    let lower = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    let lower = lower.as_bytes();
    let mut cursor = 0usize;
    while cursor + 6 <= lower.len() {
        let Some(offset) = lower[cursor..].windows(6).position(|w| w == b"values") else {
            break;
        };
        let start = cursor + offset;
        let before_ok =
            start == 0 || !(lower[start - 1].is_ascii_alphanumeric() || lower[start - 1] == b'_');
        let after = start + 6;
        let after_ok =
            after == lower.len() || !(lower[after].is_ascii_alphanumeric() || lower[after] == b'_');
        if !before_ok || !after_ok {
            cursor = after;
            continue;
        }
        let Some(open_offset) = lower[after..].iter().position(|byte| *byte == b'{') else {
            break;
        };
        let header = &lower[after..after + open_offset];
        if header.len() > 256 || !header.iter().any(|byte| matches!(byte, b'?' | b'$')) {
            cursor = after;
            continue;
        }
        let open = after + open_offset;
        let mut depth = 0usize;
        let mut close = None;
        for (offset, byte) in lower[open..].iter().enumerate() {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        close = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { break };
        bytes[start..=close].fill(b' ');
        cursor = close + 1;
    }

    String::from_utf8(bytes).expect("test corpus remains UTF-8 after ASCII-region redaction")
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
        // Word-boundary match, not a raw substring: an incidental hit (a term whose
        // local name is a prefix of another, e.g. `Foo` inside `FooBar`) must not
        // count as "reached" — that inflates this ratchet-gated testing score.
        if word_at_boundary(&corpus, local) {
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

/// The project languages beyond the authored English, as `(semantic tag, catalog
/// stem)`. Mandarin's objective tag is `cmn`; its physical catalog is authored under
/// the `zh.po` stem (the convention the slices — e.g. `logic` — already use), so the
/// axis reports `cmn` while reading `i18n/zh.po`.
const TRANSLATION_LANGS: &[(&str, &str)] = &[("fr", "fr"), ("cmn", "zh")];

/// Translation: per-language coverage of **every localizable literal** the slice
/// authors — each `(term, predicate)` where `predicate` is one of
/// [`gmeow_docs::i18n_compile::LOCALIZABLE_PREDICATES`] and the slice graph carries a
/// literal — audited via the `i18n/<stem>.po` catalogs. A literal counts as covered
/// for a language iff its catalog entry carries a real (non-empty) `msgstr`. Full
/// coverage requires English (authored, always full) plus French and Mandarin on
/// every localizable literal; the score is the mean of the three per-language
/// coverage fractions, bounded `[0,1]` and `1.0` iff every localizable literal is
/// fully translated into both fr and cmn. A non-empty value only counts after the
/// shared deterministic translation-integrity guard accepts it.
fn translation_axis(ctx: &ScoreContext) -> AxisScore {
    use std::collections::HashSet;

    use gmeow_docs::i18n_compile::{
        LOCALIZABLE_PREDICATES, counts_as_reviewed_coverage, expand_predicate, language_from_po,
        parse_po,
    };

    let ds = ctx.graph;

    // Denominator: every localizable literal the slice authors, as the set of
    // (term-iri, full-predicate-iri) pairs the graph carries a literal for.
    let mut literals: Vec<(String, String)> = Vec::new();
    for iri in &ctx.terms {
        let Some(sid) = id(ds, iri) else { continue };
        for pred in LOCALIZABLE_PREDICATES {
            if id(ds, pred).is_some_and(|p| graph::has_any(ds, sid, p)) {
                literals.push((iri.clone(), (*pred).to_string()));
            }
        }
    }
    let expected = literals.len();
    if expected == 0 {
        return AxisScore::clean(1.0);
    }

    // English is authored, so it is always fully covered.
    let mut lang_cov = vec![1.0_f64];
    let mut findings = Vec::new();
    for (tag, stem) in TRANSLATION_LANGS {
        let po = ctx.slice_dir.join(format!("i18n/{stem}.po"));
        // Covered (term, full-predicate) pairs: catalog entries that count as
        // REVIEWED coverage under the single shared policy
        // (`counts_as_reviewed_coverage`) — a real (non-empty) msgstr that is NOT
        // flagged `#, fuzzy` and passes the integrity guard — keyed by the same full
        // predicate IRI as the graph. Machine-seeded (`#, fuzzy`) entries are counted
        // separately as `seeded` (awaiting review, not yet coverage); non-empty
        // non-fuzzy entries that fail integrity are `rejected` (copied English).
        let mut rejected = 0usize;
        let mut seeded = 0usize;
        let covered: HashSet<(String, String)> = match std::fs::read_to_string(&po) {
            // A missing catalog legitimately means no coverage for this language.
            Err(_) => HashSet::new(),
            Ok(text) => {
                // A PRESENT catalog is required input: a malformed one is surfaced as a
                // finding with zero coverage for this language, never a silent skip that
                // would fake a score.
                //
                // The coverage/integrity language is the CONFIGURED TARGET (`tag`), never the
                // catalog's self-reported `Language:` header: a mislabeled `fr.po` claiming
                // `Language: en` must not be integrity-checked as English, which would credit
                // copied English as French coverage. The header is still parsed to VALIDATE the
                // catalog — a present header whose primary subtag matches neither the target
                // `tag` (e.g. `cmn`) nor the file stem (e.g. `zh`) is a real authoring bug,
                // surfaced as an advisory (never silently trusted or ignored) — and a malformed
                // catalog remains a hard parse-error with zero coverage for this language.
                let language = (*tag).to_string();
                match language_from_po(&text) {
                    Ok(Some(header)) => {
                        let primary = header
                            .split(['_', '-'])
                            .next()
                            .unwrap_or_default()
                            .to_ascii_lowercase();
                        if !primary.is_empty()
                            && !primary.eq_ignore_ascii_case(tag)
                            && !primary.eq_ignore_ascii_case(stem)
                        {
                            findings.push(advisory(
                                "slice-quality.translation.mislabeled-catalog",
                                format!(
                                    "{tag} catalog i18n/{stem}.po declares `Language: {header}`, which matches neither the target `{tag}` nor the file stem `{stem}`; coverage is evaluated against `{tag}` regardless."
                                ),
                            ));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        findings.push(advisory(
                            "slice-quality.translation.parse-error",
                            format!("{tag} catalog i18n/{stem}.po failed to parse: {e}"),
                        ));
                        lang_cov.push(0.0);
                        continue;
                    }
                }
                let entries = match parse_po(&text, false) {
                    Ok(entries) => entries,
                    Err(e) => {
                        findings.push(advisory(
                            "slice-quality.translation.parse-error",
                            format!("{tag} catalog i18n/{stem}.po failed to parse: {e}"),
                        ));
                        lang_cov.push(0.0);
                        continue;
                    }
                };
                let mut set = HashSet::new();
                for entry in &entries {
                    if entry.msgctxt.is_empty() {
                        continue;
                    }
                    if counts_as_reviewed_coverage(entry, &language) {
                        if let Some((term, pred)) = entry.msgctxt.split_once('|') {
                            set.insert((term.to_string(), expand_predicate(pred)));
                        }
                    } else if entry.fuzzy && !entry.msgstr.is_empty() {
                        seeded += 1;
                    } else if !entry.msgstr.is_empty() {
                        rejected += 1;
                    }
                }
                set
            }
        };
        if rejected > 0 {
            findings.push(advisory(
                "slice-quality.translation.integrity-rejected",
                format!(
                    "{tag} has {rejected} non-empty catalog value(s) rejected by the translation-integrity guard; copied or hybrid English does not count as coverage."
                ),
            ));
        }
        let hits = literals
            .iter()
            .filter(|pair| covered.contains(*pair))
            .count();
        #[allow(clippy::cast_precision_loss)]
        let cov = hits as f64 / expected as f64;
        if cov < 1.0 {
            let seeded_note = if seeded > 0 {
                format!(
                    " ({seeded} further catalog value(s) are machine-seeded (fuzzy) and awaiting human review — removing the `#, fuzzy` flag on a verified entry raises coverage)"
                )
            } else {
                String::new()
            };
            findings.push(advisory(
                "slice-quality.translation.incomplete",
                format!(
                    "{tag} covers {hits}/{expected} localizable literals{seeded_note}; the top tier requires 100% en+fr+cmn on every localizable literal."
                ),
            ));
        }
        lang_cov.push(cov);
    }
    #[allow(clippy::cast_precision_loss)]
    let score = lang_cov.iter().sum::<f64>() / lang_cov.len() as f64;
    AxisScore { score, findings }
}

// ── Axis 11: Flagship counter-example reasoner-depth ───────────────────────

/// The `gmeow:counterExampleDischarge` marker local name and its reasoner-driven
/// value local name — the honest per-scenario classification the axis reads.
const COUNTEREXAMPLE_DISCHARGE: &str = "counterExampleDischarge";
const REASONER_DRIVEN_DISCHARGE: &str = "reasonerDrivenDischarge";

/// Flagship counter-example reasoner-depth: the fraction of the slice's own
/// `gmeow:FlagshipScenario` individuals whose guarding counter-example is
/// reasoner-driven (drives the native solver to observe the missing entailment at
/// reasoning-runtime) rather than discharged by a structural/SHACL well-formedness
/// proxy.
///
/// The signal is read from the honest `gmeow:counterExampleDischarge` marker each
/// scenario carries (authored in `examples/flagship-acceptance.ttl`):
/// `gmeow:reasonerDrivenDischarge` counts toward the numerator;
/// `gmeow:structuralDischarge` — and any scenario with no marker — does not. The
/// measure is an intrinsically bounded fraction (`1.0` = definitionally maximal:
/// every flagship counter-example reasoner-driven), so there is nothing to
/// calibrate; the structural proxy is the floor and the reasoner-driven
/// counter-example is the depth target (see `LOGIC-CONFORMANCE.md`, Tests as
/// ontology data; Principle 17/18).
///
/// Only the slice's OWN acceptance manifest is measured: `gmeow:`-namespaced
/// `gmeow:FlagshipScenario` individuals (the manifest lives under
/// `gmeow:examples/<slice>/acceptance/`). The `ex:`-namespaced FlagshipScenario
/// FIXTURES under `tests/` exercise the wiring SHACL shape, not the acceptance bar,
/// so they are excluded. A slice with no flagship scenarios scores vacuously `1.0`.
fn flagship_counterexample_depth_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    let discharge_p = id(ds, &g(COUNTEREXAMPLE_DISCHARGE));
    let reasoner_driven = id(ds, &g(REASONER_DRIVEN_DISCHARGE));

    let scenarios: Vec<String> = instances_of(ds, &g("FlagshipScenario"))
        .into_iter()
        .filter(|iri| iri.starts_with(GMEOW_NS))
        .collect();
    if scenarios.is_empty() {
        // No flagship acceptance manifest → no counter-examples to deepen; the axis
        // is not applicable, so it takes the vacuous 1.0 (never silently "deep").
        return AxisScore::clean(1.0);
    }

    let mut reasoner_backed = 0usize;
    let mut findings = Vec::new();
    for scenario in &scenarios {
        let Some(sid) = id(ds, scenario) else {
            continue;
        };
        let is_reasoner_driven = matches!(
            (discharge_p, reasoner_driven),
            (Some(p), Some(rd)) if graph::has(ds, sid, p, rd)
        );
        if is_reasoner_driven {
            reasoner_backed += 1;
        } else {
            findings.push(advisory(
                "slice-quality.flagship.counterexample-structural-only",
                format!(
                    "flagship scenario {scenario} discharges its guarding counter-example with a structural/SHACL well-formedness proxy, not a reasoner-driven counter-example — raise it so the native solver observes the missing entailment at reasoning-runtime, marking it gmeow:reasonerDrivenDischarge (LOGIC-CONFORMANCE.md, Tests as ontology data; Principle 17/18)."
                ),
            ));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = reasoner_backed as f64 / scenarios.len() as f64;
    AxisScore { score, findings }
}

// ── Axis: GMN-1 coverage (Task 7 — the F1 mnemomorphic-domain convergence contract) ─

/// Walk `slice_dir`'s components to the repo root — the directory whose child is the
/// FIRST `slices` component. The same path-prefix discipline the plan's own grounding
/// detection uses (no `gmeow:tierGrounding` predicate exists to read; `slices/
/// grounding/` is organizational path-only per `slices/vocabulary.ttl`), applied here
/// to locate the shared `slices/grounding/lang/module.ttl` dictionary from any slice's
/// own directory.
pub(crate) fn repo_root_of(slice_dir: &Path) -> Option<std::path::PathBuf> {
    let mut root = std::path::PathBuf::new();
    for comp in slice_dir.components() {
        if comp.as_os_str() == "slices" {
            return Some(root);
        }
        root.push(comp);
    }
    None
}

/// Load the shared `gmeow:gmnDictV3` dictionary from the canonical
/// `slices/grounding/lang/module.ttl` — the SAME dictionary the Task-6 round-trip
/// gate (`crates/pipeline/src/stages/gmn1_gate.rs`) loads, so this axis's coverage
/// measurement never diverges against a second, locally-improvised dictionary.
fn gmn1_dictionary(root: &std::path::Path) -> Option<GmnDictionary> {
    let path = root.join("slices/grounding/lang/module.ttl");
    let bytes = std::fs::read(&path).ok()?;
    let ds = purrdf::parse_dataset(&bytes, "text/turtle", None).ok()?;
    GmnDictionary::from_dataset(&ds).ok()
}

/// The slice's own GMN-0 source surface: `module.ttl` plus every (non-recursive)
/// `examples/*.ttl` — module + examples ONLY, never `tests/`, mirroring both this
/// axis's own `skos:definition` and the Task-6 grounding gate's identical scope.
fn gmn1_coverage_source_paths(slice_dir: &Path) -> Vec<std::path::PathBuf> {
    let mut paths = vec![slice_dir.join("module.ttl")];
    if let Ok(rd) = std::fs::read_dir(slice_dir.join("examples")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "ttl") {
                paths.push(p);
            }
        }
    }
    paths.retain(|p| p.is_file());
    paths.sort();
    paths
}

/// GMN-1 coverage: the fraction of a slice's own GMN-0 normal-form vocabulary
/// (module + examples) the Task-6 codec can losslessly round-trip.
///
/// Reuses the codec's OWN term encoder via [`measure_coverage`] — never a duplicated,
/// possibly-divergent notion of "coverable" — so this axis and the grounding
/// round-trip gate (`gmn1_gate.rs`) can never silently disagree on what counts as
/// covered. A slice with no module/examples GMN-0 content, or whose repo root or
/// shared dictionary cannot be resolved (a malformed checkout — a condition other
/// structural gates catch independently), scores the crate's neutral vacuous 1.0
/// with an advisory naming the reason, never a silent false-positive "fully covered".
fn gmn1_coverage_axis(ctx: &ScoreContext) -> AxisScore {
    // The dictionary source branches on the scoring environment; the coverage
    // measurement TAIL below (source paths → Gmn0Model → measure_coverage → findings)
    // is shared by both arms. `repo_dict` is deferred-init: it holds the on-disk
    // dictionary the Repo arm reads, kept alive so the shared tail can borrow it.
    let repo_dict;
    let dict: &GmnDictionary = match &ctx.env {
        // Repo mode is the verbatim pre-seam behaviour: resolve the repo root, read
        // the shared on-disk dictionary, and carry the tolerant no-repo-root /
        // no-dictionary advisories (a malformed checkout other structural gates catch).
        ScoringEnv::Repo => {
            let Some(root) = repo_root_of(&ctx.slice_dir) else {
                return AxisScore {
                    score: 1.0,
                    findings: vec![advisory(
                        "slice-quality.gmn1-coverage.no-repo-root",
                        "the slice directory carries no resolvable slices/ path prefix — GMN-1 coverage cannot be measured (vacuous 1.0).".to_owned(),
                    )],
                };
            };
            let Some(dict) = gmn1_dictionary(&root) else {
                return AxisScore {
                    score: 1.0,
                    findings: vec![advisory(
                        "slice-quality.gmn1-coverage.no-dictionary",
                        "the shared gmeow:gmnDictV3 dictionary (slices/grounding/lang/module.ttl) failed to load — GMN-1 coverage cannot be measured (vacuous 1.0).".to_owned(),
                    )],
                };
            };
            repo_dict = dict;
            &repo_dict
        }
        // Bundle mode uses the embedded dictionary directly: it was already loaded and
        // validated at bundle-construction time (a corrupt wheel hard-failed there), so
        // this arm has no tolerant no-dictionary advisory — it always has a valid dict.
        ScoringEnv::Bundle(dict) => dict.as_ref(),
    };

    let paths = gmn1_coverage_source_paths(&ctx.slice_dir);
    if paths.is_empty() {
        return AxisScore::clean(1.0); // no GMN-0 source content → vacuously covered
    }
    let path_refs: Vec<&Path> = paths.iter().map(std::path::PathBuf::as_path).collect();
    let Ok(ds) = crate::dataset_from_paths(&path_refs) else {
        // A malformed source surfaces as a validation error on another gate, not here.
        return AxisScore::clean(1.0);
    };

    let model = Gmn0Model::from_dataset(&ds);
    let report = measure_coverage(&model, dict);
    let score = report.fraction();
    let findings = if report.covered < report.total {
        vec![advisory(
            "slice-quality.gmn1-coverage.uncovered",
            format!(
                "{}/{} of the slice's GMN-0 quads (module + examples) do not yet round-trip \
                 losslessly through the GMN-1 codec — extend crates/lang-bridge/src/gmn1_codec.rs \
                 to cover the construct, or file it as a named codec-coverage gap against \
                 LANG-GMN.md (never leave it silently unmeasured).",
                report.total - report.covered,
                report.total
            ),
        )]
    } else {
        Vec::new()
    };
    AxisScore { score, findings }
}

// ── Axis: GMN glyph disposition / optimality (independent of round-trip coverage) ─

const GROUNDING_SLICE_IRIS: &[&str] = &[
    "https://blackcatinformatics.ca/gmeow/slices/lang",
    "https://blackcatinformatics.ca/gmeow/slices/logic",
    "https://blackcatinformatics.ca/gmeow/slices/math",
];

/// Measure whether every explicitly audited symbol candidate owned by this slice has a
/// complete, evidence-backed disposition, whether every executable glyph target has such
/// a candidate, and whether adopted/named-key decisions agree with the executable registry
/// and pinned BPE cost. This axis is intentionally disjoint from [`gmn1_coverage_axis`]:
/// semantic quad round-trip can remain 1.0 while a symbol candidate is missing its
/// accessibility/fallback/evidence coat, chose a suboptimal rendering, or an executable
/// denotation was added without entering the audit population, and this axis will fall.
fn gmn_glyph_optimality_axis(ctx: &ScoreContext) -> AxisScore {
    // Candidate/Denotation authority is centralized in the lang grounding slice even
    // when the audited target belongs to logic: or math:. Honour this axis's declared
    // merged-closure scope in repo mode by composing that authority with the scored
    // slice graph. Without this join, logic/math would falsely report "no candidates"
    // despite their audited dispositions living exactly where the grounding contract
    // requires them to live. Repo scoring MUST fail closed when that authority cannot
    // be assembled: falling back to the slice-local graph would turn a missing/invalid
    // lang authority into a false "fully audited" score. Bundle scoring is different:
    // its supplied graph already carries the assembled audit closure.
    let repo_graph;
    let ds = match &ctx.env {
        ScoringEnv::Repo => {
            let Some(root) = repo_root_of(&ctx.slice_dir) else {
                return gmn_audit_graph_unavailable(
                    "the slice directory has no resolvable slices/ path prefix",
                );
            };
            let lang_module = root.join("slices/grounding/lang/module.ttl");
            if !lang_module.is_file() {
                return gmn_audit_graph_unavailable(format!(
                    "the shared symbol-audit authority {} is missing",
                    lang_module.display()
                ));
            }
            let mut paths = crate::report::slice_ttl_paths(&ctx.slice_dir);
            if !paths.contains(&lang_module) {
                paths.push(lang_module);
            }
            paths.sort();
            let refs: Vec<&Path> = paths.iter().map(std::path::PathBuf::as_path).collect();
            let Ok(graph) = crate::dataset_from_paths(&refs) else {
                return gmn_audit_graph_unavailable(
                    "the slice plus shared lang symbol-audit authority could not be parsed",
                );
            };
            repo_graph = graph;
            repo_graph.as_ref()
        }
        ScoringEnv::Bundle(_) => ctx.graph,
    };
    let candidates = instances_of(ds, &g("GmnSymbolCandidate"));
    let target_p = id(ds, &g("gmnCandidateTarget"));

    // A candidate belongs to the scored slice when its target is one of that slice's
    // owned terms. The candidate records themselves live in the lang-owned gmeow: plane,
    // so merged-closure scope is required and the target join is the ownership seam.
    let mut relevant = Vec::new();
    let is_lang_authority = ctx.slice_iri == "https://blackcatinformatics.ca/gmeow/slices/lang";
    for candidate in candidates {
        let Some(cid) = id(ds, &candidate) else {
            continue;
        };
        let targets = target_p.map_or_else(Vec::new, |p| all_iris(ds, cid, p));
        // A targetless row cannot be attributed through the ownership join. Retain
        // it while scoring the lang authority itself so the cardinality check below
        // reports the malformed row instead of filtering it into silence.
        if (targets.is_empty() && is_lang_authority)
            || targets.iter().any(|target| ctx.terms.contains(target))
        {
            relevant.push((candidate, targets));
        }
    }

    let disposition_p = id(ds, &g("gmnSymbolDisposition"));
    let basis_p = id(ds, &g("gmnDispositionBasis"));
    let glyph_p = id(ds, &g("gmnCandidateGlyph"));
    let fallback_p = id(ds, &g("gmnAsciiFallback"));
    let spoken_p = id(ds, &g("gmnSpokenLabel"));
    let rationale_p = id(ds, &g("gmnDispositionRationale"));
    let denotation_p = id(ds, &g("gmnCandidateDenotation"));
    let cites_p = id(ds, &g("cites"));
    let den_target_p = id(ds, "https://blackcatinformatics.ca/lang/denotationTarget");
    let den_grapheme_p = id(ds, &g("gmnDenotationGrapheme"));

    let adopted = g("gmnDispositionAdoptedGlyph");
    let named = g("gmnDispositionNamedKey");
    let structured = g("gmnDispositionStructuredConstructor");
    let rejected = g("gmnDispositionSemanticRejection");
    let token_basis = g("gmnBasisTokenCost");
    let ambiguity_basis = g("gmnBasisAmbiguity");
    let confusable_basis = g("gmnBasisConfusability");
    let mismatch_basis = g("gmnBasisSemanticMismatch");

    let registry = GmnGlyphRegistry::from_dataset(ds);
    let audited_targets: BTreeSet<&str> = relevant
        .iter()
        .flat_map(|(_, targets)| targets.iter().map(String::as_str))
        .collect();
    // The executable registry correctly refuses a Denotation -> Grapheme chain that lacks
    // an adopted candidate, so asking only the *successful* registry for its targets would
    // make that exact omission invisible. Derive the candidate obligations one step earlier:
    // every denotation that names a grapheme in the current gmnScript repertoire and a target
    // is intended executable inventory. This is a graph join, not a hand-listed glyph table.
    let intended_executable_targets: BTreeSet<String> = match (
        den_target_p,
        den_grapheme_p,
        id(ds, "https://blackcatinformatics.ca/lang/hasGrapheme"),
        id(ds, &g("gmnScript")),
    ) {
        (Some(target_p), Some(grapheme_p), Some(has_grapheme_p), Some(script)) => {
            let repertoire: BTreeSet<_> = ds
                .quads_for_pattern(Some(script), Some(has_grapheme_p), None, GraphMatch::Any)
                .map(|quad| quad.o)
                .collect();
            ds.quads_for_pattern(None, Some(grapheme_p), None, GraphMatch::Any)
                .filter(|quad| repertoire.contains(&quad.o))
                .flat_map(|quad| all_iris(ds, quad.s, target_p))
                .filter(|target| ctx.terms.contains(target))
                .collect()
        }
        _ => BTreeSet::new(),
    };
    let missing_executable_targets: Vec<&str> = intended_executable_targets
        .iter()
        .map(String::as_str)
        .filter(|target| !audited_targets.contains(target))
        .collect();
    if relevant.is_empty() && missing_executable_targets.is_empty() {
        return no_gmn_candidates(ctx);
    }

    let mut complete = 0usize;
    let mut findings = Vec::new();

    for target in &missing_executable_targets {
        findings.push(advisory(
            "slice-quality.gmn-glyph-optimality.unaudited-executable-target",
            format!(
                "{target} has an executable graph-derived GMN glyph binding but no \
                 gmeow:GmnSymbolCandidate — add the evidence-backed disposition row so the \
                 registry cannot grow outside the audited symbol inventory"
            ),
        ));
    }

    for (candidate, targets) in &relevant {
        let cid = id(ds, candidate).expect("candidate came from the same graph");
        let mut defects = Vec::<String>::new();
        if targets.len() != 1 {
            defects.push(format!(
                "expected exactly one target, found {}",
                targets.len()
            ));
        }
        let target = targets.first().map(String::as_str).unwrap_or("");

        let dispositions = disposition_p.map_or_else(Vec::new, |p| all_iris(ds, cid, p));
        if dispositions.len() != 1 {
            defects.push(format!(
                "expected exactly one symbol disposition, found {}",
                dispositions.len()
            ));
        }
        let disposition = dispositions.first().map(String::as_str).unwrap_or("");
        if ![
            adopted.as_str(),
            named.as_str(),
            structured.as_str(),
            rejected.as_str(),
        ]
        .contains(&disposition)
        {
            defects.push("disposition is outside the closed four-value vocabulary".to_owned());
        }

        let bases = basis_p.map_or_else(Vec::new, |p| all_iris(ds, cid, p));
        if bases.len() != 1 {
            defects.push(format!(
                "expected exactly one decision basis, found {}",
                bases.len()
            ));
        }
        let basis = bases.first().map(String::as_str).unwrap_or("");
        if ![
            token_basis.as_str(),
            ambiguity_basis.as_str(),
            confusable_basis.as_str(),
            mismatch_basis.as_str(),
        ]
        .contains(&basis)
        {
            defects.push("decision basis is outside the closed evidence vocabulary".to_owned());
        }

        let mut exact_literal = |pred: Option<purrdf::TermId>, name: &str| -> Option<String> {
            let values = pred.map_or_else(Vec::new, |p| all_lits(ds, cid, p));
            if values.len() != 1 || values[0].trim().is_empty() {
                defects.push(format!("{name} must be exactly one non-empty literal"));
                None
            } else {
                Some(values[0].clone())
            }
        };
        let glyph = exact_literal(glyph_p, "candidate glyph");
        let fallback = exact_literal(fallback_p, "ASCII fallback");
        let _spoken = exact_literal(spoken_p, "spoken label");
        let _rationale = exact_literal(rationale_p, "disposition rationale");
        if cites_p.is_none_or(|p| all_iris(ds, cid, p).is_empty()) {
            defects.push("candidate has no gmeow:cites evidence anchor".to_owned());
        }
        if fallback.as_deref().is_some_and(|value| !value.is_ascii()) {
            defects.push("ASCII fallback contains a non-ASCII codepoint".to_owned());
        }

        let denotations = denotation_p.map_or_else(Vec::new, |p| all_iris(ds, cid, p));
        if disposition == adopted || disposition == named {
            if denotations.len() != 1 {
                defects.push(format!(
                    "adopted/named sign must point at exactly one denotation, found {}",
                    denotations.len()
                ));
            } else if let Some(did) = id(ds, &denotations[0]) {
                let den_targets = den_target_p.map_or_else(Vec::new, |p| all_iris(ds, did, p));
                if den_targets.len() != 1 || den_targets[0] != target {
                    defects.push(
                        "candidate denotation does not resolve exactly to its target".to_owned(),
                    );
                }
                let graphemes = den_grapheme_p.map_or_else(Vec::new, |p| all_iris(ds, did, p));
                if disposition == adopted && graphemes.len() != 1 {
                    defects.push(
                        "adopted glyph denotation does not name exactly one grapheme".to_owned(),
                    );
                }
                if disposition == named && !graphemes.is_empty() {
                    defects.push(
                        "named-key disposition unexpectedly enters the glyph registry".to_owned(),
                    );
                }
            }
        } else if !denotations.is_empty() {
            defects.push(
                "structured/rejected candidate unexpectedly carries a sign denotation".to_owned(),
            );
        }

        match (disposition, basis, glyph.as_deref(), fallback.as_deref()) {
            (d, b, Some(glyph), Some(fallback)) if d == adopted => {
                if b != token_basis {
                    defects.push("adopted glyph is not backed by the token-cost basis".to_owned());
                }
                if gmn_glyph_token_cost(glyph) > gmn_glyph_token_cost(fallback) {
                    defects.push(format!(
                        "adopted glyph {glyph:?} costs more than fallback {fallback:?}"
                    ));
                }
                match &registry {
                    Ok(registry)
                        if registry
                            .bindings_for_term(target)
                            .iter()
                            .any(|(_, registered)| *registered == glyph) => {}
                    Ok(_) => defects.push(
                        "adopted candidate has no matching executable scoped registry binding"
                            .to_owned(),
                    ),
                    Err(error) => {
                        defects.push(format!("executable glyph registry is invalid: {}", error.0))
                    }
                }
            }
            (d, b, Some(glyph), Some(fallback)) if d == named => {
                let measured_win = b == token_basis
                    && gmn_glyph_token_cost(glyph) > gmn_glyph_token_cost(fallback);
                let safety_win = b == ambiguity_basis || b == confusable_basis;
                if !measured_win && !safety_win {
                    defects.push(
                        "named-key disposition is backed by neither a measured token win nor a safety basis"
                            .to_owned(),
                    );
                }
            }
            (d, b, _, _) if d == structured && b != ambiguity_basis => defects.push(
                "structured-constructor disposition must be backed by ambiguity/arity evidence"
                    .to_owned(),
            ),
            (d, b, _, _) if d == rejected && b != mismatch_basis => defects
                .push("semantic rejection must be backed by semantic-mismatch evidence".to_owned()),
            _ => {}
        }

        if defects.is_empty() {
            complete += 1;
        } else {
            findings.push(advisory(
                "slice-quality.gmn-glyph-optimality.incomplete",
                format!("{candidate}: {}", defects.join("; ")),
            ));
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let denominator = relevant.len() + missing_executable_targets.len();
    let score = complete as f64 / denominator as f64;
    AxisScore { score, findings }
}

fn gmn_audit_graph_unavailable(detail: impl std::fmt::Display) -> AxisScore {
    AxisScore {
        score: 0.0,
        findings: vec![advisory(
            "slice-quality.gmn-glyph-optimality.audit-graph-unavailable",
            format!(
                "GMN glyph optimality cannot be audited because {detail}; scoring fails closed at 0.0 until the canonical grounding authority is available"
            ),
        )],
    }
}

fn no_gmn_candidates(ctx: &ScoreContext) -> AxisScore {
    if GROUNDING_SLICE_IRIS.contains(&ctx.slice_iri.as_str()) {
        AxisScore {
            score: 0.0,
            findings: vec![advisory(
                "slice-quality.gmn-glyph-optimality.no-candidates",
                "grounding slice has no explicit gmeow:GmnSymbolCandidate audit population"
                    .to_owned(),
            )],
        }
    } else {
        AxisScore::clean(1.0)
    }
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
        assert!(states_boundary("A process, as opposed to an endurant."));
        assert!(states_boundary("A quality, distinct from its bearer."));
        assert!(!states_boundary("A widget of the system."));
        // Incidental substrings must NOT pass through the ratchet: "whenever" is not
        // "never", "denote"/"NOTE" are not "not", "cannon" is not "cannot".
        assert!(!states_boundary(
            "Applies whenever a bearer exists; denote it clearly."
        ));
        assert!(!states_boundary(
            "A note about the cannon on the annotation."
        ));
        assert!(!states_boundary(
            "A widget. It is not an interchangeable alias for a broader, narrower, or merely related construct."
        ));
    }

    #[test]
    fn worked_triple_detection() {
        assert!(is_worked_triple("ex:x a gmeow:Foo ."));
        assert!(is_worked_triple("ex:s ex:p ex:o ;"));
        assert!(!is_worked_triple("a plain sentence with no triple"));
        // A bare prose colon and a period is NOT a worked triple (old false positive).
        assert!(!is_worked_triple("See section 3: this is important."));
        // A full-IRI scheme colon is not a CURIE either.
        assert!(!is_worked_triple("visit http://example.org/ for details."));
        // Ownership metadata is not a worked use of the subject term.
        assert!(!is_worked_triple(
            "logic:Widget rdfs:isDefinedBy <https://example.org/slice> ."
        ));
    }

    #[test]
    fn testing_corpus_excludes_comments_and_values_inventories() {
        let corpus = r#"
# logic:CommentOnly
ASK {
    VALUES ?term { logic:InventoryOnly logic:AlsoInventoryOnly }
    logic:ActuallyExercised rdfs:subClassOf ?term .
}
"#;
        let semantic = strip_non_executing_test_mentions(corpus);
        assert!(!word_at_boundary(&semantic, "CommentOnly"));
        assert!(!word_at_boundary(&semantic, "InventoryOnly"));
        assert!(!word_at_boundary(&semantic, "AlsoInventoryOnly"));
        assert!(word_at_boundary(&semantic, "ActuallyExercised"));
    }

    #[test]
    fn generic_usage_coats_are_not_substantive_information() {
        assert!(is_generic_usage_coat(
            "Use logic:Widget when the modeled statement satisfies the scope and necessary conditions stated in this term's definition."
        ));
        assert!(is_generic_usage_coat(
            "Assert logic:Widget with its declared OWL kind and preserve its domain, range, standpoint, and provenance constraints."
        ));
        assert!(!is_generic_usage_coat(
            "Use logic:Widget for a rigid identity-bearing type whose instances remain Widgets in every accessible world."
        ));
    }

    #[test]
    fn test_artifact_regex_is_valid() {
        // Forces the LazyLock initializer to run: proves the regex literal compiles
        // (so the `.expect` can never fire at runtime) and pins its intended matches.
        assert!(TEST_ARTIFACT.is_match("see test_foo_bar for evidence"));
        assert!(TEST_ARTIFACT.is_match("crates/foo.rs::bar"));
        assert!(TEST_ARTIFACT.is_match("tests/thing.py behaviour"));
        assert!(TEST_ARTIFACT.is_match("Mirrors the fixture"));
        assert!(!TEST_ARTIFACT.is_match("a genuine ontological rationale"));
    }

    #[test]
    fn word_boundary_rejects_incidental_substrings() {
        assert!(word_at_boundary("ex:Foo a owl:Class .", "Foo"));
        assert!(!word_at_boundary("ex:FooBar a owl:Class .", "Foo"));
        assert!(!word_at_boundary("prefixFoo", "Foo"));
        // Phrase cue spans a word gap but is boundary-checked at its ends.
        assert!(word_at_boundary("a, rather than b", "rather than"));
    }
}
