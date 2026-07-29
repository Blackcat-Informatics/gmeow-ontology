// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN-dialect MODEL-FACING surface, declared once as data.
//!
//! The medium axis is a change to how the bundle's bytes are CODED, and its central
//! claim is that coding is not meaning: a zstd-primed frame carries the same claim the
//! unprimed one does. The observable consequence is that nothing a model reads may move.
//! This module is the single declaration of *what a model reads*, in the two shapes the
//! gates need it:
//!
//! * [`is_gmn_dialect_path`] — ONE predicate over committed `generated/` paths, split
//!   into named [`clauses`] so a clause that resolves to nothing is EXCLUDED WITH A
//!   REASON rather than silently absent (the two such clauses are
//!   [`ClauseId::LangAbnf`] and [`ClauseId::Gmn1Primer`]);
//! * [`PINNED_GMN_DIALECT_PRODUCERS`] — the shrink-only census of source trees that
//!   PRODUCE those paths, which this branch's diff must not intersect.
//!
//! # The scope ruling this module encodes
//!
//! It gates the GMN DIALECT surfaces, not the docs term index. `gmeow:docsConcern`
//! feeds `gmeow_docs::model`, so ANY ontology addition necessarily grows rendered docs;
//! gating that would make "zero model-facing change" unsatisfiable by construction for
//! any ontology work at all. The `llms.txt`-family SHAPE is frozen separately (its
//! skeleton, section headers, ordering and the MCP resource list), because that surface
//! is materialized into bundle blobs rather than onto a `generated/` path and so is
//! unreachable from the path predicate here.
//!
//! # Why the path set is DERIVED, never hardcoded
//!
//! No GMN-dialect path is an authored `gmeow:extractsPath`: they are opaque-family
//! members whose `gmeow:FanoutExtraction` rows are EMITTED by the carrier into
//! `graph/fanout-opaque-manifest`. A hardcoded list would therefore be a second source
//! of truth that a dropped artifact could not falsify — it would simply stop being
//! compared. The gate reads the emitted bundle instead and filters it through the
//! predicate below, so a member that disappears fails the clause-coverage check.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Every model-facing invariance problem the checks in this module and in
/// [`crate::llms_shape`] found, accumulated rather than short-circuited.
///
/// A COLLECTOR rather than a `Result`, mirroring `gmeow_validate::repo_static`'s
/// `RepoStaticReport` — the peer gate of exactly this kind. Two reasons, and neither is
/// stylistic: a gate that stopped at the first problem would report one moved artifact
/// out of twenty and send the reader back for another full run per fix, and
/// `gmeow_errors::Diag` is the substrate for a FAILURE CLASS a producer raises, not for a
/// census a gate compiles.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModelFacingReport {
    problems: Vec<String>,
}

impl ModelFacingReport {
    /// Record one problem.
    pub fn problem(&mut self, message: impl Into<String>) {
        self.problems.push(message.into());
    }

    /// Whether nothing moved.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.problems.is_empty()
    }

    /// Every recorded problem, in the order it was found.
    #[must_use]
    pub fn problems(&self) -> &[String] {
        &self.problems
    }
}

impl fmt::Display for ModelFacingReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.problems.is_empty() {
            return f.write_str("no model-facing drift");
        }
        write!(
            f,
            "{} model-facing invariance problem(s):\n{}",
            self.problems.len(),
            self.problems.join("\n")
        )
    }
}

/// The committed prefix every `lang:` projection deliverable reconstructs under.
pub const LANG_PROJECTION_PREFIX: &str = "generated/projections/lang/";

/// The prefix the GMN-1 teachability primer is materialized under — a bundle BLOB lane
/// (`dist/llms.txt` / `dist/llms-full.txt`), never a `generated/` path.
pub const LLMS_DIST_PREFIX: &str = "dist/llms";

/// The stable identity of one clause of the GMN-dialect path predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClauseId {
    /// `generated/projections/lang/**/ebnf/**` — the EBNF grammar renderings, the
    /// formalism the GMN dialect's own grammar is authored and published in.
    LangEbnf,
    /// `generated/projections/lang/**/gbnf/**` — the GBNF constrained-decoding grammar,
    /// the surface a model is literally decoded against.
    LangGbnf,
    /// `generated/projections/lang/**/lark/**` — the Lark grammar, the parser surface a
    /// consumer validates GMN against.
    LangLark,
    /// `generated/projections/lang/**/abnf/**` — RESOLVES TO ZERO on the live tree; see
    /// [`Clause::zero_reason`].
    LangAbnf,
    /// `generated/projections/lang/gmn1/**` — the whole versioned GMN-1 pack: every
    /// `.gmn` document, the conformance pack, the verbalizations.
    Gmn1Pack,
    /// The `token-metrics.ttl` measurement surface, wherever under the `lang:` projection
    /// tree it is keyed.
    Gmn1TokenMetrics,
    /// The operator/script GLYPH-TABLE surfaces — the sigil↔glyph inventory a model is
    /// taught to emit against.
    GmnGlyphTable,
    /// The GMN-1 teachability primer — RESOLVES TO ZERO here; see
    /// [`Clause::zero_reason`].
    Gmn1Primer,
}

/// One named clause of the predicate: what it matches, and — when it legitimately
/// matches nothing — WHY.
#[derive(Debug, Clone, Copy)]
pub struct Clause {
    /// The clause's stable identity.
    pub id: ClauseId,
    /// The pattern, spelled for a human reading a failure message.
    pub pattern: &'static str,
    /// `Some(reason)` when the clause is DECLARED to resolve to zero paths on the live
    /// tree. A declared-zero clause that starts matching is a HARD FAIL, not a quiet
    /// widening: a new model-facing artifact appeared and the declared reason expired.
    /// `None` means the clause MUST match at least one path, or the predicate has gone
    /// vacuous on that limb and the gate would pass by matching nothing.
    pub zero_reason: Option<&'static str>,
}

impl Clause {
    /// Whether `path` (a committed repo-relative path, forward slashes) falls under this
    /// clause. Segment-based rather than prefix-based: the GBNF and Lark surfaces are
    /// keyed UNDER the versioned GMN-1 pack (`gmn1/v1/gbnf/gmn.gbnf`), so a top-level
    /// `gbnf/` prefix test would silently resolve to zero and take the clause's
    /// non-vacuity guarantee with it.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        match self.id {
            ClauseId::LangEbnf => has_lang_segment(path, "ebnf"),
            ClauseId::LangGbnf => has_lang_segment(path, "gbnf"),
            ClauseId::LangLark => has_lang_segment(path, "lark"),
            ClauseId::LangAbnf => has_lang_segment(path, "abnf"),
            ClauseId::Gmn1Pack => has_lang_segment(path, "gmn1"),
            ClauseId::Gmn1TokenMetrics => {
                under_lang(path) && base_name(path) == "token-metrics.ttl"
            }
            ClauseId::GmnGlyphTable => under_lang(path) && base_name(path).contains("glyph"),
            ClauseId::Gmn1Primer => path.starts_with(LLMS_DIST_PREFIX),
        }
    }
}

/// Whether `path` sits anywhere under the `lang:` projection tree.
fn under_lang(path: &str) -> bool {
    path.starts_with(LANG_PROJECTION_PREFIX)
}

/// The final `/`-delimited component of `path`.
fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Whether `path` is under the `lang:` projection tree AND carries `segment` as one of
/// its DIRECTORY components (never as the file name — `gmn.ebnf` is not an `ebnf/`
/// directory).
fn has_lang_segment(path: &str, segment: &str) -> bool {
    let Some(rest) = path.strip_prefix(LANG_PROJECTION_PREFIX) else {
        return false;
    };
    let mut components: Vec<&str> = rest.split('/').collect();
    // Drop the file name: only DIRECTORY components name a target family.
    components.pop();
    components.contains(&segment)
}

/// Every clause of the GMN-dialect path predicate, in a fixed total order.
///
/// The two declared-zero clauses are kept IN the predicate rather than deleted. Deleting
/// them would make the gate silently narrower over time — the exact failure mode a
/// derived path set exists to prevent — whereas keeping them turns "this surface
/// produces nothing today" into a claim the gate re-checks on every run.
#[must_use]
pub fn clauses() -> &'static [Clause] {
    &[
        Clause {
            id: ClauseId::LangEbnf,
            pattern: "generated/projections/lang/**/ebnf/**",
            zero_reason: None,
        },
        Clause {
            id: ClauseId::LangGbnf,
            pattern: "generated/projections/lang/**/gbnf/**",
            zero_reason: None,
        },
        Clause {
            id: ClauseId::LangLark,
            pattern: "generated/projections/lang/**/lark/**",
            zero_reason: None,
        },
        Clause {
            id: ClauseId::LangAbnf,
            pattern: "generated/projections/lang/**/abnf/**",
            zero_reason: Some(
                "the ABNF projection target emits an ARTIFACT only for a grammar whose \
                 canonical form is inside the ABNF-expressible context-free fragment \
                 (`abnf_blocking_constructs` empty, crates/lang-bridge/src/registry.rs). \
                 Every authored grammar the bundle projects carries EBNF-only constructs \
                 (negated/verbatim character classes, `A - B` difference), so each ABNF \
                 emission is an honest SoundUnder preservation record with NO artifact — \
                 never a fabricated best-effort ABNF. The clause is therefore live and \
                 empty rather than absent: the day a grammar becomes ABNF-expressible, a \
                 new model-facing artifact appears and this reason has expired",
            ),
        },
        Clause {
            id: ClauseId::Gmn1Pack,
            pattern: "generated/projections/lang/gmn1/**",
            zero_reason: None,
        },
        Clause {
            id: ClauseId::Gmn1TokenMetrics,
            pattern: "generated/projections/lang/**/token-metrics.ttl",
            zero_reason: None,
        },
        Clause {
            id: ClauseId::GmnGlyphTable,
            pattern: "generated/projections/lang/**/*glyph*",
            zero_reason: None,
        },
        Clause {
            id: ClauseId::Gmn1Primer,
            pattern: "dist/llms*",
            zero_reason: Some(
                "the GMN-1 primer is built in memory (crates/docs/src/gmn1_primer.rs) and \
                 materialized ONLY into the `dist/llms.txt` / `dist/llms-full.txt` bundle \
                 blobs, which are not `generated/` paths and so are absent from the \
                 bundle's committed-path projection. Its invariance is gated by the \
                 llms-shape freeze (the `PRIMER_HEADING` / section-ordering freeze), not \
                 by artifact-byte comparison, so this clause resolves to zero PATHS by \
                 construction here",
            ),
        },
    ]
}

/// The ONE GMN-dialect path predicate both legs filter with.
#[must_use]
pub fn is_gmn_dialect_path(path: &str) -> bool {
    clauses().iter().any(|clause| clause.matches(path))
}

/// The GMN-dialect subset of `paths`, in canonical order.
pub fn gmn_dialect_paths<'a, I>(paths: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = &'a String>,
{
    paths
        .into_iter()
        .filter(|path| is_gmn_dialect_path(path))
        .cloned()
        .collect()
}

/// Which paths each clause claims, for a failure message and for
/// [`check_clause_coverage`].
#[must_use]
pub fn clause_coverage(paths: &BTreeSet<String>) -> BTreeMap<ClauseId, BTreeSet<String>> {
    clauses()
        .iter()
        .map(|clause| {
            let matched: BTreeSet<String> = paths
                .iter()
                .filter(|path| clause.matches(path))
                .cloned()
                .collect();
            (clause.id, matched)
        })
        .collect()
}

/// Prove the predicate is NON-VACUOUS on `paths`: every clause without a declared
/// [`Clause::zero_reason`] matches at least one path, and every clause WITH one matches
/// none.
///
/// This is what stops the artifact-invariance leg from degenerating. A byte-comparison
/// over an empty set passes trivially, so "the paths are identical" is worth nothing
/// unless the set is known to cover the surface it claims to.
///
/// Records a problem for a live clause that matched nothing, or a declared-zero clause
/// that matched something.
pub fn check_clause_coverage(paths: &BTreeSet<String>, report: &mut ModelFacingReport) {
    let coverage = clause_coverage(paths);
    for clause in clauses() {
        let matched = coverage.get(&clause.id).cloned().unwrap_or_default();
        match (clause.zero_reason, matched.is_empty()) {
            (None, true) => report.problem(format!(
                "clause {:?} ({}) matched NO path — the predicate has gone vacuous on that \
                 limb, so the byte-invariance it is supposed to prove would pass by comparing \
                 nothing. Either the artifact family was dropped (a model-facing change) or \
                 the clause drifted from where the family is keyed",
                clause.id, clause.pattern
            )),
            (Some(reason), false) => report.problem(format!(
                "clause {:?} ({}) is DECLARED to resolve to zero paths, but matched {matched:?}. \
                 The declared reason has expired: {reason}",
                clause.id, clause.pattern
            )),
            _ => {}
        }
    }
}

/// Leg 1, as a PURE function over two bundle projections: every GMN-dialect path
/// reconstructs BYTE-IDENTICALLY from both.
///
/// `dist` and `baseline` are `path → reconstructed bytes` maps from two emissions of the
/// SAME carrier through two declared media. The comparison is over the DERIVED dialect
/// subset, and it proves non-vacuity first ([`check_clause_coverage`]) so an empty set
/// can never pass as agreement.
///
/// Pure so the leg's red arm is reachable: the gate perturbs one artifact of the real
/// dist projection and requires this to refuse.
///
/// Returns the compared path set, so the caller can report what was actually proved
/// rather than only that something was.
pub fn check_artifact_invariance(
    dist: &BTreeMap<String, Vec<u8>>,
    baseline: &BTreeMap<String, Vec<u8>>,
    report: &mut ModelFacingReport,
) -> BTreeSet<String> {
    let dist_paths = gmn_dialect_paths(dist.keys());
    let baseline_paths = gmn_dialect_paths(baseline.keys());
    check_clause_coverage(&dist_paths, report);
    if dist_paths != baseline_paths {
        let only_dist: Vec<&String> = dist_paths.difference(&baseline_paths).collect();
        let only_baseline: Vec<&String> = baseline_paths.difference(&dist_paths).collect();
        report.problem(format!(
            "the two emissions reconstruct DIFFERENT GMN-dialect path sets — only in the \
             dictionary-primed emission: {only_dist:?}; only in the declared-baseline \
             emission: {only_baseline:?}. The medium re-codes bytes; it may not decide which \
             artifacts exist"
        ));
    }

    let mut moved: Vec<String> = Vec::new();
    for path in dist_paths.intersection(&baseline_paths) {
        let a = dist.get(path).expect("key came from the dist map");
        let b = baseline.get(path).expect("key came from the baseline map");
        if a != b {
            moved.push(format!(
                "{path} ({} B primed vs {} B baseline, blake3 {} vs {})",
                a.len(),
                b.len(),
                blake3::hash(a).to_hex(),
                blake3::hash(b).to_hex()
            ));
        }
    }
    if !moved.is_empty() {
        report.problem(format!(
            "{} GMN-dialect artifact(s) reconstruct DIFFERENTLY under the two declared media: \
             {moved:?}. A zstd-primed claim is the same claim — if what a model reads depends \
             on how the bundle was coded, the medium axis is not a coding change",
            moved.len()
        ));
    }
    dist_paths
}

// ── The producer census (leg 2) ──────────────────────────────────────────────

/// The PINNED, shrink-only census of source trees that produce a GMN-dialect path or
/// drive the GMN dialect's compute surface.
///
/// Shrink-only in the same sense as the peer `PINNED_HAND_AUTHORED_SHAPES_TTL` ratchet in
/// `gmeow_validate::repo_static`: an entry that matches no live file does NOT red (a
/// retirement that deletes a producer and forgets to trim this list must still pass),
/// while a GMN-dialect path producer that is NOT covered here reds through
/// [`check_producer_census_is_complete`] — a ratchet over an incomplete set would freeze
/// the incompleteness.
///
/// Each entry is a repo-relative PREFIX (a trailing `/` makes it a tree; anything else
/// matches the path itself and any path extending it, which is how
/// `crates/lang-bridge/src/gmn` covers both the `gmn_*` and the `gmn1_*` modules in one
/// row rather than enumerating eight files that would drift).
pub const PINNED_GMN_DIALECT_PRODUCERS: &[&str] = &[
    // The GMN cost matrix: the measured operator-cost surface the glyph table is
    // optimized against.
    "crates/gmn-cost-matrix/",
    // The browser-side GMN engine — the dialect surface a model-facing consumer runs.
    "crates/gmn-wasm/",
    // The dialect CLI lane (`gmeow gmn …`).
    "crates/gmeow-cli/src/gmn.rs",
    // The GMN grammar lifting/serialization core: EBNF / ABNF / GBNF / Lark all render
    // the SAME `RuleExpr` tree from here.
    "crates/lang-bridge/src/grammar.rs",
    // Both `gmn_*` (consume / metrics / migrate / symbology / verbalize) and `gmn1_*`
    // (codec / digest / witness) modules.
    "crates/lang-bridge/src/gmn",
    // The projection registry: the actual emitter of the grammar targets, the versioned
    // GMN-1 pack, `token-metrics.ttl` and the verbalizations.
    "crates/lang-bridge/src/registry.rs",
    // The build-time GMN-1 round-trip gate over the shipped `.gmn` pack.
    "crates/pipeline/src/stages/gmn1_gate.rs",
];

/// Whether `path` (repo-relative, forward slashes) lies inside the pinned producer
/// census.
#[must_use]
pub fn is_gmn_dialect_producer(path: &str) -> bool {
    PINNED_GMN_DIALECT_PRODUCERS
        .iter()
        .any(|pin| path == *pin || path.starts_with(pin))
}

/// Leg 2, as a PURE function over a changed-path list: no change on this branch may
/// touch a GMN-dialect producer.
///
/// Pure so it is falsifiable — the red fixture feeds it a synthetic diff containing a
/// `crates/gmn-wasm/` path and requires it to refuse. A gate whose failure arm cannot be
/// reached is not a gate.
pub fn check_producer_non_interference<'a, I>(changed: I, report: &mut ModelFacingReport)
where
    I: IntoIterator<Item = &'a str>,
{
    let touched: BTreeSet<&str> = changed
        .into_iter()
        .filter(|path| is_gmn_dialect_producer(path))
        .collect();
    if touched.is_empty() {
        return;
    }
    report.problem(format!(
        "this change touches {} GMN-dialect producer file(s): {touched:?}. The medium axis \
         re-CODES the bundle's bytes and must leave what a model reads untouched, so a diff \
         against a producer of the GMN dialect surfaces is a model-facing change by \
         definition. Land it separately; do NOT trim PINNED_GMN_DIALECT_PRODUCERS to make \
         this pass — the census is shrink-only for RETIREMENTS, not for exemptions",
        touched.len()
    ));
}

/// One derived path-producing site: a source file and the committed path family it
/// mints.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProducedPath {
    /// The repo-relative source file that forms the path.
    pub source: String,
    /// The committed path family, with `{…}` interpolations collapsed to `*`.
    pub path: String,
}

/// Every derived GMN-dialect path producer in `produced` must be a member of
/// [`PINNED_GMN_DIALECT_PRODUCERS`].
///
/// The companion to [`check_producer_non_interference`], and the reason that ratchet is
/// not merely a ratchet over whatever somebody happened to list. It filters through the
/// SAME [`is_gmn_dialect_path`] predicate leg 1 uses — deliberately NOT through "every
/// path leg 1's manifest carries", which would sweep in the non-GMN `lang:` projection
/// emitters (`tei.rs`, `nif.rs`, `conllu.rs`, `ontolex.rs`, `semaf.rs`, `bcp47.rs`) and
/// turn a dialect gate into a whole-slice freeze.
///
/// Records a problem for every source file that mints a GMN-dialect path family and is
/// outside the pinned census.
pub fn check_producer_census_is_complete(
    produced: &BTreeSet<ProducedPath>,
    report: &mut ModelFacingReport,
) {
    let mut missing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in produced {
        if is_gmn_dialect_path(&row.path) && !is_gmn_dialect_producer(&row.source) {
            missing
                .entry(row.source.clone())
                .or_default()
                .insert(row.path.clone());
        }
    }
    if missing.is_empty() {
        return;
    }
    report.problem(format!(
        "{} source file(s) mint a GMN-dialect path but are absent from \
         PINNED_GMN_DIALECT_PRODUCERS: {missing:?}. A non-interference ratchet over an \
         INCOMPLETE producer set freezes the incompleteness — the unlisted producer could be \
         edited freely while the gate reported green. Add it to the census (growth here is a \
         completeness fix, not a widening) in the same change that introduces it",
        missing.len()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| (*p).to_string()).collect()
    }

    /// The live shape of the shipped `lang:` projection tree, as the predicate sees it.
    fn live_like() -> BTreeSet<String> {
        set(&[
            "generated/projections/lang/ebnf/gmn.ebnf",
            "generated/projections/lang/ebnf/gts.ebnf",
            "generated/projections/lang/gmn1/v1/gbnf/gmn.gbnf",
            "generated/projections/lang/gmn1/v1/lark/gmn.lark",
            "generated/projections/lang/gmn1/v1/token-metrics.ttl",
            "generated/projections/lang/gmn1/v1/gmn-grounding-glyphs.gmn",
            // Non-GMN neighbours that must NOT be swept in.
            "generated/projections/lang/tei/forms-and-sign-systems.sentSawHerDuck.tei.xml",
            "generated/projections/lang/conllu/forms-and-sign-systems.x.conllu",
            "generated/projections/lang/bcp47-tags.ttl",
        ])
    }

    #[test]
    fn the_predicate_selects_the_dialect_surfaces_and_nothing_else() {
        let selected = gmn_dialect_paths(live_like().iter());
        assert!(selected.contains("generated/projections/lang/ebnf/gmn.ebnf"));
        assert!(selected.contains("generated/projections/lang/gmn1/v1/gbnf/gmn.gbnf"));
        assert!(selected.contains("generated/projections/lang/gmn1/v1/lark/gmn.lark"));
        assert!(selected.contains("generated/projections/lang/gmn1/v1/token-metrics.ttl"));
        // The non-GMN lang projections are the discrimination witness: a predicate that
        // took the whole `lang:` tree would be a slice freeze wearing a dialect gate's
        // name.
        assert!(!selected.contains(
            "generated/projections/lang/tei/forms-and-sign-systems.sentSawHerDuck.tei.xml"
        ));
        assert!(
            !selected.contains("generated/projections/lang/conllu/forms-and-sign-systems.x.conllu")
        );
        assert!(!selected.contains("generated/projections/lang/bcp47-tags.ttl"));
        // …and a path outside the projection tree entirely is never a dialect path.
        assert!(!is_gmn_dialect_path("generated/medium/gmeow-core-v1.zdict"));
        assert!(!is_gmn_dialect_path("crates/lang-bridge/src/grammar.rs"));
    }

    /// A file NAMED `*.ebnf` is not an `ebnf/` family member by itself: the clause keys
    /// on the directory, so a stray extension cannot satisfy it.
    #[test]
    fn the_family_clauses_key_on_the_directory_not_the_extension() {
        assert!(
            !Clause {
                id: ClauseId::LangEbnf,
                pattern: "",
                zero_reason: None,
            }
            .matches("generated/projections/lang/gmn.ebnf")
        );
        assert!(
            Clause {
                id: ClauseId::LangEbnf,
                pattern: "",
                zero_reason: None,
            }
            .matches("generated/projections/lang/ebnf/gmn.ebnf")
        );
    }

    /// Run one check and return its report.
    fn run(check: impl FnOnce(&mut ModelFacingReport)) -> ModelFacingReport {
        let mut report = ModelFacingReport::default();
        check(&mut report);
        report
    }

    #[test]
    fn clause_coverage_is_clean_on_a_live_like_tree() {
        let report = run(|r| check_clause_coverage(&gmn_dialect_paths(live_like().iter()), r));
        assert!(
            report.is_clean(),
            "every live clause must match and both declared-zero clauses stay empty: {report}"
        );
    }

    /// A live clause that matches nothing reds: the byte comparison would otherwise
    /// pass by comparing an empty set.
    #[test]
    fn a_live_clause_matching_nothing_is_a_hard_fail() {
        let mut shrunk = live_like();
        shrunk.retain(|p| !p.contains("/lark/"));
        let report = run(|r| check_clause_coverage(&gmn_dialect_paths(shrunk.iter()), r));
        assert!(!report.is_clean(), "a dropped Lark grammar must red");
        assert!(report.to_string().contains("LangLark"), "{report}");
        assert!(report.to_string().contains("vacuous"), "{report}");
    }

    /// A DECLARED-zero clause that starts matching reds: the reason has expired and a
    /// new model-facing artifact appeared.
    #[test]
    fn a_declared_zero_clause_that_starts_matching_is_a_hard_fail() {
        let mut grown = live_like();
        grown.insert("generated/projections/lang/abnf/gmn.abnf".to_string());
        let report = run(|r| check_clause_coverage(&gmn_dialect_paths(grown.iter()), r));
        assert!(!report.is_clean(), "a newly-emitted ABNF artifact must red");
        assert!(report.to_string().contains("LangAbnf"), "{report}");
        assert!(report.to_string().contains("expired"), "{report}");
    }

    fn projection(paths: &BTreeSet<String>) -> BTreeMap<String, Vec<u8>> {
        paths
            .iter()
            .map(|path| (path.clone(), path.as_bytes().to_vec()))
            .collect()
    }

    #[test]
    fn artifact_invariance_passes_when_both_emissions_reconstruct_the_same_bytes() {
        let files = projection(&live_like());
        let mut report = ModelFacingReport::default();
        let compared = check_artifact_invariance(&files, &files, &mut report);
        assert!(
            report.is_clean(),
            "identical projections must agree: {report}"
        );
        assert!(compared.contains("generated/projections/lang/gmn1/v1/gbnf/gmn.gbnf"));
        assert!(!compared.iter().any(|path| path.contains("/tei/")));
    }

    #[test]
    fn artifact_invariance_reds_when_one_artifact_moves() {
        let dist = projection(&live_like());
        let mut baseline = dist.clone();
        baseline.insert(
            "generated/projections/lang/gmn1/v1/gbnf/gmn.gbnf".to_string(),
            b"perturbed".to_vec(),
        );
        let report = run(|r| {
            check_artifact_invariance(&dist, &baseline, r);
        });
        assert!(!report.is_clean(), "a perturbed dialect artifact must red");
        assert!(report.to_string().contains("gmn.gbnf"), "{report}");
        assert!(report.to_string().contains("the same claim"), "{report}");
    }

    #[test]
    fn artifact_invariance_reds_when_one_emission_drops_an_artifact() {
        let dist = projection(&live_like());
        let mut baseline = dist.clone();
        baseline.remove("generated/projections/lang/gmn1/v1/lark/gmn.lark");
        let report = run(|r| {
            check_artifact_invariance(&dist, &baseline, r);
        });
        assert!(!report.is_clean(), "a dropped dialect artifact must red");
        assert!(report.to_string().contains("gmn.lark"), "{report}");
    }

    #[test]
    fn the_producer_ratchet_passes_on_a_diff_that_avoids_the_dialect() {
        let report = run(|r| {
            check_producer_non_interference(
                ["crates/pipeline/src/medium/registry.rs", "Makefile"],
                r,
            );
        });
        assert!(
            report.is_clean(),
            "a medium-only diff touches no dialect producer: {report}"
        );
    }

    /// The red fixture the acceptance criterion names: a `crates/gmn-wasm/` path in the
    /// diff.
    #[test]
    fn the_producer_ratchet_reds_on_a_gmn_wasm_edit() {
        let report = run(|r| {
            check_producer_non_interference(
                ["crates/pipeline/src/lib.rs", "crates/gmn-wasm/src/lib.rs"],
                r,
            );
        });
        assert!(!report.is_clean(), "a gmn-wasm edit must red");
        assert!(
            report.to_string().contains("crates/gmn-wasm/src/lib.rs"),
            "{report}"
        );
    }

    /// Both spellings under the ONE `crates/lang-bridge/src/gmn` row.
    #[test]
    fn the_pin_covers_both_gmn_and_gmn1_module_spellings() {
        assert!(is_gmn_dialect_producer(
            "crates/lang-bridge/src/gmn_symbology.rs"
        ));
        assert!(is_gmn_dialect_producer(
            "crates/lang-bridge/src/gmn1_codec.rs"
        ));
        // …and does NOT cover the non-GMN lang emitters.
        assert!(!is_gmn_dialect_producer("crates/lang-bridge/src/tei.rs"));
        assert!(!is_gmn_dialect_producer("crates/lang-bridge/src/nif.rs"));
        assert!(!is_gmn_dialect_producer("crates/lang-bridge/src/conllu.rs"));
    }

    #[test]
    fn the_census_completeness_check_accepts_the_pinned_producers() {
        let produced: BTreeSet<ProducedPath> = [
            ProducedPath {
                source: "crates/lang-bridge/src/registry.rs".to_string(),
                path: "generated/projections/lang/gmn1/v*/token-metrics.ttl".to_string(),
            },
            ProducedPath {
                source: "crates/lang-bridge/src/tei.rs".to_string(),
                path: "generated/projections/lang/tei/*.tei.xml".to_string(),
            },
        ]
        .into();
        let report = run(|r| check_producer_census_is_complete(&produced, r));
        assert!(
            report.is_clean(),
            "the pinned emitter and a non-GMN emitter both pass: {report}"
        );
    }

    /// The red fixture for the completeness companion: an unpinned file minting a
    /// dialect path.
    #[test]
    fn the_census_completeness_check_reds_on_an_unpinned_dialect_producer() {
        let produced: BTreeSet<ProducedPath> = [ProducedPath {
            source: "crates/lang-bridge/src/tei.rs".to_string(),
            path: "generated/projections/lang/gmn1/v*/smuggled.gmn".to_string(),
        }]
        .into();
        let report = run(|r| check_producer_census_is_complete(&produced, r));
        assert!(!report.is_clean(), "an unpinned dialect producer must red");
        assert!(report.to_string().contains("tei.rs"), "{report}");
        assert!(
            report.to_string().contains("freezes the incompleteness"),
            "{report}"
        );
    }
}
