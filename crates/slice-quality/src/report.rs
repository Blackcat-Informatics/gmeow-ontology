// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end slice scoring: assemble the slice graph, run every rubric axis, and
//! build the assessment + the advisory report on the diagnostics substrate.

use std::path::{Path, PathBuf};

use gmeow_errors::{Finding, Report, Rule, Severity, Standpoint, seed_codes};
use gmeow_validate::rule_catalog::help_uri_for;

use crate::graph::{self, instances_of};
use crate::model::{Axis, AxisGrade, MeasurementStandard, SliceAssessment};
use crate::score::{ScoreContext, ScoringEnv, advisory};
use crate::{axes, lattice};

/// Advice-ranking KIND: an axis-level advice template (the rubric's
/// `gmeow:axisAdviceTemplate`, surfaced once per deficient axis) ranks strictly
/// AHEAD of that same axis's per-term instantiated findings. This is an EXPLICIT
/// ordering key — the order must never ride on code-string spelling (a `.template`
/// code sorts AFTER a `.no-stereotype` code because 'n' < 't', which would land the
/// template BELOW its per-term findings and violate "ahead of").
const ADVICE_KIND_TEMPLATE: u8 = 0;
/// The per-term instantiated advisory a primitive surfaced (ranks after its axis's
/// template item — see [`ADVICE_KIND_TEMPLATE`]).
const ADVICE_KIND_INSTANCE: u8 = 1;

/// Every diagnostic code a slice-quality report can emit — the two structural
/// codes minted by [`SliceReport::to_report`] (`grade`/`rollup`) plus every axis
/// and reasoner advisory code the primitives surface. This is the enumeration
/// authority the command seeds into the process-wide code registry so that every
/// emitted finding carries a *registered* code (never a bare, unregistered
/// string), and the set a help URI is attached to. Kept in one place so a new
/// advisory code is registered the moment it is listed here (a drift test in this
/// module pins the axes/reasoner emitters to this list).
pub const FINDING_CODES: &[&str] = &[
    // Structural report codes (report.rs).
    "slice-quality.grade",
    "slice-quality.rollup",
    // Reasoner-derived axis codes (reasoner.rs).
    "slice-quality.reasoner.no-closure",
    "slice-quality.reasoner.closure-redundant",
    "slice-quality.reasoner.no-obligations",
    "slice-quality.reasoner.counterexample-no-clash",
    // Per-axis advisory codes (axes.rs).
    "slice-quality.grounding.no-stereotype",
    "slice-quality.information.incomplete-coat",
    "slice-quality.prose.definition-no-boundary",
    "slice-quality.prose.example-not-triple",
    "slice-quality.prose.test-rationale",
    "slice-quality.linkage.no-correspondence-surface",
    "slice-quality.linkage.no-calculus-eligible-correspondence",
    "slice-quality.linkage.uncalculated-correspondence",
    "slice-quality.gmn-glyph-optimality.audit-graph-unavailable",
    "slice-quality.gmn-glyph-optimality.unaudited-executable-target",
    "slice-quality.projection.hand-authored-shapes",
    "slice-quality.projection.no-mappings",
    "slice-quality.testing.no-cells",
    "slice-quality.testing.untested-term",
    "slice-quality.documentation.thin-thesis",
    "slice-quality.documentation.no-docs",
    "slice-quality.translation.integrity-rejected",
    "slice-quality.translation.incomplete",
    "slice-quality.translation.uncovered-literal",
    "slice-quality.translation.uncovered-literal-truncated",
    "slice-quality.translation.notation-excluded",
    "slice-quality.translation.notation-excluded-truncated",
    "slice-quality.flagship.counterexample-structural-only",
    "slice-quality.gmn1-coverage.no-repo-root",
    "slice-quality.gmn1-coverage.no-dictionary",
    "slice-quality.gmn1-coverage.uncovered",
    "slice-quality.gmn-glyph-optimality.no-candidates",
    "slice-quality.gmn-glyph-optimality.incomplete",
    // Documentation-maturity axis codes (doc_maturity.rs).
    "slice-quality.doc-maturity.missing-dimension",
    "slice-quality.doc-maturity.model-unavailable",
    "slice-quality.doc-maturity.slice-untracked",
    // Advice-harvest-coverage axis codes (axes.rs advice_coverage_axis).
    "slice-quality.advice-coverage.unharvested",
    "slice-quality.advice-coverage.no-repo-root",
    "slice-quality.advice-coverage.no-constraint-source",
    // Axis-level advice-template item (report.rs) — the rubric's
    // `gmeow:axisAdviceTemplate` surfaced once per DEFICIENT axis, ranked ahead of
    // that axis's per-term findings, plus the latent-data-gap code minted when a
    // deficient axis carries no template (a rubric authoring gap, never swallowed).
    "slice-quality.axis-advice",
    "slice-quality.axis-advice.missing-template",
    // The lint-gate synthetic finding (`crate::lint::lint_report`) — minted only
    // when the measured roll-up fails to dominate the effective tier bar. Not a
    // scoring code (no axis, no rollup); registered here so it still carries a
    // registered code + help URI through the same json/sarif/html projections
    // every other slice-quality finding does.
    "slice-quality.lint.below-min-tier",
];

/// Seed every slice-quality finding code into the process-wide code registry
/// (idempotent). Called at the start of report construction so the codes are
/// registered before any finding is interned or rendered.
pub fn seed_finding_codes() {
    seed_codes(FINDING_CODES);
}

/// The full result of scoring one slice.
pub struct SliceReport {
    /// The measurement standard the slice was scored against (the floor-free
    /// projection of the rubric — scoring never touches a governance floor).
    pub standard: MeasurementStandard,
    /// The per-axis grade vector + roll-up tier.
    pub assessment: SliceAssessment,
    /// Every advisory finding the axes surfaced, ranked (heaviest axis first).
    pub advisories: Vec<Finding>,
    /// The axis IRI that PRODUCED `advisories[i]` — a stored back-reference, kept
    /// index-parallel through the rank sort.
    ///
    /// Without it the only way back from an advisory to its axis is
    /// `crate::lint::attribute_axis`'s best-effort textual join on the finding CODE,
    /// which returns `None` whenever the code's domain token matches no axis or more
    /// than one. The gate needs the exact set for ONE axis (the one below its floor),
    /// so it reads this instead of guessing.
    advisory_axes: Vec<String>,
    /// The axis IRI paired with each advisory, for weight-ranking and grouping.
    axis_weight: std::collections::HashMap<String, f64>,
}

/// Discover a slice's ontology IRI from its `manifest.ttl` (`a gmeow:Slice`).
///
/// `pub(crate)` so [`crate::measure_repo_residues`] (the projection-ceiling seed's
/// and gate's shared residue-measurement helper) resolves the same slice IRI this
/// module's own scoring path does — one resolution authority, never a second
/// re-implementation that could silently diverge on IRI choice.
pub(crate) fn slice_iri_of(slice_dir: &Path) -> gmeow_errors::Result<String> {
    let manifest = slice_dir.join("manifest.ttl");
    let ds = crate::dataset_from_paths(&[&manifest])?;
    instances_of(&ds, &graph::g("Slice"))
        .into_iter()
        .next()
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Report {
                detail: format!("{} declares no gmeow:Slice", manifest.display()),
            })
        })
}

/// Every `.ttl` under `slice_dir`'s `module.ttl`, `examples/`, and `tests/` —
/// deterministic (sorted), existing files only. The SINGLE authority for assembling
/// a slice's own graph: the sweep, the pipeline carrier producer, and the scoring
/// tests all collect a slice's Turtle through this one helper (no re-implemented,
/// possibly-divergent path walk).
pub fn slice_ttl_paths(slice_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![slice_dir.join("module.ttl")];
    for sub in ["examples", "tests"] {
        collect_ttl(&slice_dir.join(sub), &mut paths);
    }
    paths.retain(|p| p.is_file());
    paths.sort();
    paths
}

fn collect_ttl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_ttl(&p, out);
        } else if p.extension().is_some_and(|x| x == "ttl") {
            out.push(p);
        }
    }
}

/// Score `slice_dir` against an already-loaded measurement standard (the floor-free
/// projection of the rubric; the sweep path reuses one) in the given scoring
/// environment.
///
/// Every rubric axis binds a measurement primitive; an axis whose producer the
/// kernel does not implement is a hard error (never a silent skip). `env` decides
/// where the two repo-anchored axes (`gmn1_coverage`, `DocMaturity`) source their
/// wide-scope inputs: [`ScoringEnv::Repo`] reads the surrounding checkout (the
/// in-repo sweep/CLI/MCP path), [`ScoringEnv::Bundle`] carries them in an embedded
/// wheel (the consumer path — no repo around the slice).
///
/// # Errors
/// Returns a message if the standard or the slice graph cannot be loaded, or if the
/// rubric names a producer with no implemented primitive.
pub fn score_slice_with_standard(
    slice_dir: &Path,
    standard: &MeasurementStandard,
    env: ScoringEnv,
) -> gmeow_errors::Result<SliceReport> {
    let slice_iri = slice_iri_of(slice_dir)?;
    let paths = slice_ttl_paths(slice_dir);
    let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    let ds = crate::dataset_from_paths(&path_refs)?;
    // The scoring environment decides where the two repo-anchored axes source their
    // wide-scope inputs; every in-repo caller passes `ScoringEnv::Repo` (byte-identical
    // to the pre-seam behaviour), the consumer wheel passes `ScoringEnv::Bundle`.
    let ctx = ScoreContext::new(slice_iri.clone(), slice_dir.to_path_buf(), &ds, env);

    let mut scores: Vec<(&Axis, f64)> = Vec::with_capacity(standard.axes.len());
    // Each entry is (axis_iri, axis_weight, advice_kind, finding). `advice_kind`
    // ranks an axis-level template item ahead of that axis's per-term findings.
    let mut advisories: Vec<(String, f64, u8, Finding)> = Vec::new();
    let mut axis_weight = std::collections::HashMap::new();
    for axis in &standard.axes {
        axis_weight.insert(axis.iri.clone(), axis.weight);
        let primitive = axes::resolve(&axis.producer).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Report {
                detail: format!(
                    "rubric axis {} names producer '{}' with no implemented primitive (hard fail)",
                    axis.iri, axis.producer
                ),
            })
        })?;
        let result = primitive(&ctx);
        for f in result.findings {
            advisories.push((axis.iri.clone(), axis.weight, ADVICE_KIND_INSTANCE, f));
        }
        scores.push((axis, result.score.clamp(0.0, 1.0)));
    }

    // Surface the rubric's per-axis uplift advice (`gmeow:axisAdviceTemplate`) as an
    // axis-level advice item for every DEFICIENT axis. "Deficient" = an axis that
    // surfaced >= 1 per-term advisory (NOT "below target tier":
    // axisFlagshipCounterExampleDepth sits at Maximal with all-0.0 floors yet still
    // advises). The template item is ranked strictly ahead of that axis's per-term
    // findings via `ADVICE_KIND_TEMPLATE`; the loader already read the template into
    // `Axis.advice`, but until now nothing emitted it (dead rubric information).
    let deficient: std::collections::BTreeSet<&str> = advisories
        .iter()
        .map(|(axis_iri, _, _, _)| axis_iri.as_str())
        .collect();
    let mut templates: Vec<(String, f64, u8, Finding)> = Vec::new();
    for axis in &standard.axes {
        if !deficient.contains(axis.iri.as_str()) {
            continue;
        }
        let local = axis.iri.rsplit(['/', '#']).next().unwrap_or(&axis.iri);
        let finding = if axis.advice.trim().is_empty() {
            // A deficient axis with an empty template is a latent rubric data gap.
            // Do not silently swallow it (hard-fail discipline): surface it as its
            // own visible advisory so the missing remediation text is caught and
            // authored, rather than emitting a blank advice line.
            advisory(
                "slice-quality.axis-advice.missing-template",
                format!(
                    "{local}: axis is deficient but carries no gmeow:axisAdviceTemplate — a latent rubric data gap; author its uplift advice."
                ),
            )
        } else {
            advisory(
                "slice-quality.axis-advice",
                format!("{local}: {}", axis.advice),
            )
        };
        templates.push((axis.iri.clone(), axis.weight, ADVICE_KIND_TEMPLATE, finding));
    }
    advisories.extend(templates);

    let assessment = lattice::assess(&slice_iri, &scores, standard);

    // Rank advice: heaviest axis first, then group all advisories for the same axis
    // together (axis IRI tiebreak — otherwise two same-weight axes interleave and a
    // template can land ahead of a *different* axis's per-term findings), then within
    // an axis the axis-level template item ahead of that axis's per-term findings
    // (`advice_kind`), then finding code, then message — a deterministic total order
    // (no derived Ord over the float; explicit key).
    advisories.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.code.cmp(&b.3.code))
            .then_with(|| a.3.message.cmp(&b.3.message))
    });
    // Split the ranked pairs into the two index-parallel vectors AFTER the sort, so
    // `advisory_axes[i]` is the producing axis of `advisories[i]` by construction.
    let (advisory_axes, advisories): (Vec<String>, Vec<Finding>) = advisories
        .into_iter()
        .map(|(axis_iri, _, _, f)| (axis_iri, f))
        .unzip();

    Ok(SliceReport {
        standard: standard.clone(),
        assessment,
        advisories,
        advisory_axes,
        axis_weight,
    })
}

#[cfg(test)]
impl SliceReport {
    /// Test-only constructor: assemble a [`SliceReport`] from already-computed
    /// parts, bypassing [`score_slice_with_standard`]'s scoring pass entirely.
    /// Used by `crate::lint`'s unit tests to build synthetic reports (a
    /// declared tier ratchet, a graded advisory, a degenerate empty-grade
    /// slice, …) without a real slice directory or rubric dataset.
    /// `advisory_axes` must be index-parallel to `advisories`; a caller that does
    /// not care about axis provenance passes an empty vector and gets none.
    pub(crate) fn for_test(
        standard: MeasurementStandard,
        assessment: SliceAssessment,
        advisories: Vec<Finding>,
        advisory_axes: Vec<String>,
        axis_weight: std::collections::HashMap<String, f64>,
    ) -> Self {
        assert!(
            advisory_axes.is_empty() || advisory_axes.len() == advisories.len(),
            "advisory_axes must be index-parallel to advisories (or empty)"
        );
        Self {
            standard,
            assessment,
            advisories,
            advisory_axes,
            axis_weight,
        }
    }
}

impl SliceReport {
    /// The advisories PRODUCED BY one axis, in report order.
    ///
    /// Reads the stored `advisory_axes` back-reference, so the answer is exact: it
    /// never guesses from the finding code the way the severity-grading decoration in
    /// `crate::lint` has to.
    #[must_use]
    pub fn advisories_for_axis(&self, axis_iri: &str) -> Vec<&Finding> {
        self.advisory_axes
            .iter()
            .zip(&self.advisories)
            .filter(|(produced_by, _)| produced_by.as_str() == axis_iri)
            .map(|(_, finding)| finding)
            .collect()
    }

    /// The roll-up tier label.
    #[must_use]
    pub fn rollup_label(&self) -> &str {
        &self.assessment.rollup.label
    }

    /// The per-axis grades sorted by the weakest tier first (the uplift order).
    #[must_use]
    pub fn grades_weakest_first(&self) -> Vec<&AxisGrade> {
        let mut v: Vec<&AxisGrade> = self.assessment.grades.iter().collect();
        v.sort_by(|a, b| {
            a.tier
                .rank
                .cmp(&b.tier.rank)
                .then_with(|| {
                    let wa = self.axis_weight.get(&a.axis_iri).copied().unwrap_or(0.0);
                    let wb = self.axis_weight.get(&b.axis_iri).copied().unwrap_or(0.0);
                    wb.partial_cmp(&wa).unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.axis_iri.cmp(&b.axis_iri))
        });
        v
    }

    /// Build the advisory [`Report`] on the diagnostics substrate: every axis grade
    /// as an informational note, every uplift item as an `Advisory` warning.
    #[must_use]
    pub fn to_report(&self) -> Report {
        // Register every slice-quality code before any finding is built, so the
        // emitted report carries registered (never bare) diagnostic codes.
        seed_finding_codes();
        let mut report = Report::new("slice-quality");
        // Per-axis grade notes (never gating).
        for grade in self.grades_weakest_first() {
            let local = grade
                .axis_iri
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(&grade.axis_iri);
            report.add_finding(
                Finding::new(
                    Severity::Info,
                    "slice-quality.grade",
                    format!("{local}: {} (score {:.2})", grade.tier.label, grade.score),
                )
                .with_tool("slice-quality")
                .with_standpoint(Standpoint::Advisory),
            );
        }
        // Roll-up.
        report.add_finding(
            Finding::new(
                Severity::Info,
                "slice-quality.rollup",
                format!(
                    "roll-up tier: {} ({})",
                    self.assessment.rollup.label, self.assessment.slice
                ),
            )
            .with_tool("slice-quality")
            .with_standpoint(Standpoint::Advisory),
        );
        // Ranked uplift advisories.
        for f in &self.advisories {
            report.add_finding(f.clone());
        }

        // Attach a rule descriptor for every distinct emitted code, each carrying a
        // help URI into the generated constraint catalog (`help_uri_for` — the SAME
        // anchor transform the pipeline's constraint-catalog stage and `gmeow
        // validate` stamp onto rule help URIs, never a hand-rolled base). The
        // renderers join a finding to its rule by code, so every finding surfaces a
        // registered code + help URI in the json/sarif/html projections.
        let mut severities: std::collections::BTreeMap<String, Severity> =
            std::collections::BTreeMap::new();
        for finding in &report.findings {
            severities
                .entry(finding.code.clone())
                .or_insert(finding.severity);
        }
        for (code, severity) in severities {
            let mut rule = Rule::new(code.clone(), severity);
            rule.help_uri = Some(help_uri_for(&code));
            report.add_rule(rule);
        }
        report
    }

    /// A deterministic human-facing text rendering.
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "slice-quality: {}\n  roll-up tier: {}\n",
            self.assessment.slice, self.assessment.rollup.label
        ));
        out.push_str("  per-axis grades (weakest first):\n");
        for grade in self.grades_weakest_first() {
            let local = grade
                .axis_iri
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(&grade.axis_iri);
            out.push_str(&format!(
                "    {local}: {} ({:.2})\n",
                grade.tier.label, grade.score
            ));
        }
        if self.advisories.is_empty() {
            out.push_str("  no uplift advice — the slice meets every axis.\n");
        } else {
            out.push_str(&format!(
                "  ranked uplift advice ({}):\n",
                self.advisories.len()
            ));
            for (i, f) in self.advisories.iter().enumerate() {
                out.push_str(&format!("    {}. [{}] {}\n", i + 1, f.code, f.message));
            }
        }
        out
    }
}

// -----------------------------------------------------------------------------
// RDF projection of a slice assessment.
// -----------------------------------------------------------------------------

/// The named graph the slice-quality assessment projection lives in.
const SLICE_QUALITY_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/slice-quality";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// The computational-model observation method every advisor-produced grade carries
/// (the scorer is a deterministic Rust primitive, not expert judgement).
const METHOD_COMPUTATIONAL_MODEL: &str =
    "https://blackcatinformatics.ca/gmeow/methodComputationalModel";
impl SliceReport {
    /// Project this slice assessment into the `gmeow:` RDF vocabulary as
    /// deterministic N-Quads, all in the `gmeow:graph/slice-quality` named graph.
    ///
    /// The assessment IS a bundle of quality observations (per the worked example
    /// `slices/core/slice-quality-rubric/examples/rubric-assessment.ttl`): each
    /// per-axis grade becomes a `gmeow:QualityAssessment` (a `gmeow:Observation`
    /// subclass) whose `gmeow:assessedEntity` is the slice IRI, whose
    /// `gmeow:qualityDimension` is the axis's emitted dimension, whose
    /// `gmeow:observationMethod` is `gmeow:methodComputationalModel`, and whose two
    /// coexisting `gmeow:observationResult`s are (a) a `math:Quantity` wrapping
    /// the normalized score (`math:quantityValue` + `math:dimensionless`) and (b)
    /// the categorical `gmeow:QualityTier` the score earned. The roll-up tier is one
    /// more top-level `gmeow:QualityAssessment` whose sole result is the meet tier —
    /// dimension-spanning, so it carries no `gmeow:qualityDimension`.
    ///
    /// This mirrors the discipline of `gmeow-docs`/`gmeow-errors` `to_gmeow_rdf`:
    /// N-Quads (no TriG/prefix handling), `nq_escape`d literals, IRIs (never blank
    /// nodes) minted deterministically from the slice + axis local names (never a
    /// counter), sorted iteration, every generated A-Box subject stamped with
    /// `rdfs:label` / `skos:definition` / `rdfs:isDefinedBy` / `gmeow:graphBoxRole
    /// gmeow:boxABox`, and a trailing newline.
    #[must_use]
    pub fn to_gmeow_rdf(&self) -> String {
        let graph = format!("<{SLICE_QUALITY_GRAPH}>");
        let mut lines: Vec<String> = Vec::new();

        let triple = |s: &str, p: &str, o: &str, lines: &mut Vec<String>| {
            lines.push(format!("{s} <{p}> {o} {graph} ."));
        };
        let literal = |value: &str| format!("\"{}\"", nq_escape(value));

        // Every projected subject is generated A-Box instance data: stamp it with a
        // human label, a definition-equivalent, its named-graph provenance anchor, and
        // the assertional `gmeow:boxABox` role so the folded bundle satisfies the
        // assertional-tier validation contract (mirrors docs/errors emitters).
        let role_object = format!("<{}boxABox>", crate::model::GMEOW);
        let isdefinedby_object = format!("<{SLICE_QUALITY_GRAPH}>");
        let annotate = |subject: &str, label: &str, definition: &str, lines: &mut Vec<String>| {
            triple(subject, RDFS_LABEL, &literal(label), lines);
            triple(subject, SKOS_DEFINITION, &literal(definition), lines);
            triple(subject, RDFS_IS_DEFINED_BY, &isdefinedby_object, lines);
            triple(
                subject,
                &format!("{}graphBoxRole", crate::model::GMEOW),
                &role_object,
                lines,
            );
        };

        let slice_iri = &self.assessment.slice;
        let slice_slug = local_slug(slice_iri);
        let assessed_object = format!("<{}>", nq_iri(slice_iri));

        // Map each axis IRI to its emitted quality dimension (the grade vector carries
        // only the axis; the dimension binding lives on the rubric axis).
        let dim_of: std::collections::HashMap<&str, &str> = self
            .standard
            .axes
            .iter()
            .map(|a| (a.iri.as_str(), a.dimension_iri.as_str()))
            .collect();

        // Per-axis grades, emitted in a deterministic axis-IRI order.
        let mut grades: Vec<&AxisGrade> = self.assessment.grades.iter().collect();
        grades.sort_by(|a, b| a.axis_iri.cmp(&b.axis_iri));
        for grade in grades {
            let axis_local = local_slug(&grade.axis_iri);
            let assessment_iri = format!(
                "{}slice-quality/assessment/{slice_slug}/{axis_local}",
                crate::model::GMEOW
            );
            let score_iri = format!(
                "{}slice-quality/score/{slice_slug}/{axis_local}",
                crate::model::GMEOW
            );
            let assessment_subject = format!("<{}>", nq_iri(&assessment_iri));
            let score_subject = format!("<{}>", nq_iri(&score_iri));

            triple(
                &assessment_subject,
                RDF_TYPE,
                &format!("<{}QualityAssessment>", crate::model::GMEOW),
                &mut lines,
            );
            triple(
                &assessment_subject,
                &format!("{}assessedEntity", crate::model::GMEOW),
                &assessed_object,
                &mut lines,
            );
            // The dimension the grade is emitted under — a hard requirement: an axis
            // with no bound dimension is a rubric authoring error, never silently
            // dropped (the projection then carries a visibly under-specified grade).
            let dimension = dim_of.get(grade.axis_iri.as_str()).copied().unwrap_or("");
            if !dimension.is_empty() {
                triple(
                    &assessment_subject,
                    &format!("{}qualityDimension", crate::model::GMEOW),
                    &format!("<{}>", nq_iri(dimension)),
                    &mut lines,
                );
            }
            triple(
                &assessment_subject,
                &format!("{}observationMethod", crate::model::GMEOW),
                &format!("<{METHOD_COMPUTATIONAL_MODEL}>"),
                &mut lines,
            );
            // Result 1: the normalized score, wrapped in a math:Quantity (the range of
            // observationResult is logic:Individual — a bare literal is forbidden here).
            triple(
                &assessment_subject,
                &format!("{}observationResult", crate::model::GMEOW),
                &score_subject,
                &mut lines,
            );
            // Result 2: the categorical tier the score earned (observationResult is
            // non-functional; a scalar reading and a categorical verdict coexist).
            triple(
                &assessment_subject,
                &format!("{}observationResult", crate::model::GMEOW),
                &format!("<{}>", nq_iri(&grade.tier.iri)),
                &mut lines,
            );
            annotate(
                &assessment_subject,
                &format!("Quality grade: {axis_local} on {slice_iri}"),
                &format!(
                    "Slice-quality assessment of {slice_iri} on axis {} — measured {} (score {}).",
                    grade.axis_iri,
                    grade.tier.label,
                    fmt_score(grade.score)
                ),
                &mut lines,
            );

            // The score math:Quantity: value + dimensionless dimension. A normalized
            // ratio has no unit witness and therefore needs no measurement frame.
            triple(
                &score_subject,
                RDF_TYPE,
                &format!("<{}Quantity>", crate::model::MATH),
                &mut lines,
            );
            triple(
                &score_subject,
                &format!("{}quantityValue", crate::model::MATH),
                &format!("\"{}\"^^<{XSD_DECIMAL}>", fmt_score(grade.score)),
                &mut lines,
            );
            triple(
                &score_subject,
                &format!("{}hasDimension", crate::model::MATH),
                &format!("<{}dimensionless>", crate::model::MATH),
                &mut lines,
            );
            annotate(
                &score_subject,
                &format!("Quality score: {axis_local} on {slice_iri}"),
                &format!(
                    "Normalized slice-quality score {} for axis {} of {slice_iri}.",
                    fmt_score(grade.score),
                    grade.axis_iri
                ),
                &mut lines,
            );
        }

        // The roll-up: one top-level QualityAssessment whose sole result is the meet
        // tier. It spans every dimension (the unweighted lattice meet of the axis
        // grades), so it carries NO gmeow:qualityDimension — it is not a value along
        // one axis but the combined verdict.
        let rollup_iri = format!(
            "{}slice-quality/assessment/{slice_slug}/rollup",
            crate::model::GMEOW
        );
        let rollup_subject = format!("<{}>", nq_iri(&rollup_iri));
        triple(
            &rollup_subject,
            RDF_TYPE,
            &format!("<{}QualityAssessment>", crate::model::GMEOW),
            &mut lines,
        );
        triple(
            &rollup_subject,
            &format!("{}assessedEntity", crate::model::GMEOW),
            &assessed_object,
            &mut lines,
        );
        triple(
            &rollup_subject,
            &format!("{}observationMethod", crate::model::GMEOW),
            &format!("<{METHOD_COMPUTATIONAL_MODEL}>"),
            &mut lines,
        );
        triple(
            &rollup_subject,
            &format!("{}observationResult", crate::model::GMEOW),
            &format!("<{}>", nq_iri(&self.assessment.rollup.iri)),
            &mut lines,
        );
        annotate(
            &rollup_subject,
            &format!("Roll-up quality tier on {slice_iri}"),
            &format!(
                "Roll-up slice-quality tier {} for {slice_iri} — the unweighted lattice meet of every per-axis grade.",
                self.assessment.rollup.label
            ),
            &mut lines,
        );

        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

/// Format a normalized `[0,1]` score as a deterministic plain-decimal lexical form
/// (fixed precision — never scientific notation, so it is a legal `xsd:decimal`).
fn fmt_score(score: f64) -> String {
    format!("{score:.6}")
}

/// The local name of an IRI (the tail after the last `/` or `#`), slugified to
/// `[a-z0-9-]` for use inside a minted subject IRI. Deterministic and stable.
fn local_slug(iri: &str) -> String {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    let name = &iri[cut..];
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Escape an IRI for an N-Quads IRIREF (the characters the grammar forbids raw:
/// controls, spaces, and the delimiter set `<>"{}|^`` ` and backslash).
fn nq_iri(iri: &str) -> String {
    let mut out = String::with_capacity(iri.len());
    for ch in iri.chars() {
        match ch {
            '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\' => {
                out.push_str(&format!("\\u{:04X}", ch as u32));
            }
            c if (c as u32) <= 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Escape a string literal for N-Triples/N-Quads (mirrors `gmeow_docs::rdf::nq_escape`).
fn nq_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
