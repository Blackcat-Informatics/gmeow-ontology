// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Documentation-coverage status — the SINGLE source of the per-term and
//! per-slice coverage predicates.
//!
//! Both the lint gate (`gmeow_docs::lint`, which emits a `docs/missing-*` warning per
//! absent dimension) and the rendered docs site ([`crate::slug`], which surfaces
//! coverage on each term page and the documentation-health page) read coverage
//! from here, and the self-hosting RDF projection ([`crate::rdf`]) emits the
//! `gmeow:docCoversDimension` / `gmeow:docMissesDimension` incidence and the
//! FCA-derived `gmeow:docEarnedMaturity` from the SAME computation. Keeping the
//! predicates in one place means the gate count, the published page, and the
//! reasoned graph can never silently disagree about what a term is missing.
//!
//! Every dimension detector is a DETERMINISTIC structural predicate — a pure
//! function of the [`DocsModel`], a present/absent fact, never a corpus-tuned
//! threshold, no reasoner. This is what keeps the maturity axes built over the
//! incidence ([`crate::maturity`]) objective. The dimension keys are machine-keyed
//! to [`crate::maturity::Dimension`] (via [`CoverageDimension::dimension`]), which
//! in turn matches the `gmeow:dim*` individuals in
//! `slices/core/documentation/module.ttl` — the three-way agreement guarded by
//! `crates/docs/tests/coverage_dimensions.rs`.
//!
//! # One truthmaker with the evidence DAG
//!
//! The proof-carrying `gmeow:DocEvidence` DAG ([`crate::rdf`]'s `term_evidence`)
//! and the coverage incidence read the SAME model fields: `dimFixturePair` /
//! `dimLossLedgerRow` / `dimCompetencyRationale` are projections of the fixture /
//! loss / competency joins the evidence nodes are grounded in; `dimWorkedInstance`
//! reads the same `model.examples` the renderer resolves worked scenes through
//! (single-sourced, though examples are not one of the five `DocEvidence` kinds);
//! and `dimTranslationCoverage` reads the same [`crate::i18n::Translations`] index
//! the renderer resolves labels through — never a second, divergent detection.

use std::collections::{HashMap, HashSet};

use gmeow_logic_compile::ir::PreservationKind;

use crate::i18n::{ENGLISH, Translations};
use crate::maturity::{DimSet, Dimension};
use crate::model::{DocFixtureKind, DocTerm, DocsModel};

/// Full IRI of `rdfs:label` — the translatable carrier predicate.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// Full IRI of `skos:definition` — the translatable carrier predicate.
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";

/// True when a `logic:PreservationKind` local name (as the loss rows carry it) is
/// SOUND-OR-STRONGER — at or below `logic:SoundUnderApproximation`'s rung in the
/// canonical [`PreservationKind::ALL`] ordering (BOTTOM = `Exact`, most-preserving).
/// Uses the existing preservation ordering, never a hand-picked whitelist, so the
/// `dimLossJudgmentSound` axis stays objective. An unrecognized kind is NOT provably
/// sound → `false` (conservative and honest).
fn is_sound_or_stronger(kind: &str) -> bool {
    let sound_rung = PreservationKind::ALL
        .iter()
        .position(|k| *k == PreservationKind::SoundUnder)
        .expect("SoundUnder is in PreservationKind::ALL");
    PreservationKind::ALL
        .iter()
        .position(|k| k.as_str() == kind)
        .is_some_and(|i| i <= sound_rung)
}

/// The local name of an IRI: the tail after the last `/` or `#`.
fn local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

/// The set of term IRIs that are the subject of at least one external alignment
/// (term equivalence), built once from a model's linkages.
///
/// Membership is the only operation needed, so a [`HashSet`] gives O(1) per-term
/// checks rather than re-scanning the linkages for every term; iteration order is
/// never observed, so determinism is unaffected.
pub fn alignment_subjects(model: &DocsModel) -> HashSet<&str> {
    model.linkages.iter().map(|l| l.subject.as_str()).collect()
}

// ── Deterministic prose heuristics (structural, ratchet-safe) ───────────────────
//
// The boundary/worked-triple PREDICATES themselves live once, in [`crate::prose`],
// and are shared verbatim with the slice-quality kernel
// (`crates/slice-quality/src/axes.rs`). What legitimately differs between the two
// crates is the INPUT, not the predicate: slice-quality reads the raw RDF dataset +
// filesystem, this module reads the typed [`DocsModel`]. That input separation is by
// design and stays; the predicate is one definition so the two scores can never
// disagree about whether the same string states a boundary. Each remaining detector
// here is a present/absent structural fact, never a tuned score.

/// True if a rationale string names a TEST ARTIFACT rather than an ontological
/// reason — a Rust test fn (`test_…`), a `.rs::`/`.py` source reference, or the
/// "Mirrors …" test-cross-reference phrasing. A deterministic name-membership
/// test (the doc-model twin of slice-quality's `TEST_ARTIFACT` regex, using plain
/// string membership so the docs crate needs no regex dependency).
fn names_test_artifact(s: &str) -> bool {
    if s.contains(".rs::") || s.contains(".py") || s.contains("Mirrors ") {
        return true;
    }
    // `test_<ident>`: the marker followed by an identifier char.
    let bytes = s.as_bytes();
    let mut from = 0;
    while let Some(rel) = s[from..].find("test_") {
        let at = from + rel;
        let after = bytes.get(at + 5).copied();
        if after.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            return true;
        }
        from = at + 5;
    }
    false
}

// ── Per-term coverage ───────────────────────────────────────────────────────────

/// Precomputed lookup sets for the whole model, so each per-term detector is O(1).
///
/// Built once per model via [`CoverageContext::new`]; the borrowed `&str` keys
/// reference the model's own strings (iteration order is never observed, so
/// determinism is unaffected). Every set/map here is the truthmaker a coverage
/// dimension reads — shared with the evidence DAG by construction.
pub struct CoverageContext<'a> {
    /// Term IRIs that are the subject of ≥1 external alignment (`dimAlignment`).
    aligned: HashSet<&'a str>,
    /// Term IRIs that are the subject of ≥1 mapping-set-backed linkage
    /// (`dimLinkageCoverage`).
    linkage_covered: HashSet<&'a str>,
    /// Term CURIEs referenced by ≥1 well-formed conformance fixture.
    fixture_wellformed: HashSet<&'a str>,
    /// Term CURIEs referenced by ≥1 counter-example fixture.
    fixture_counter: HashSet<&'a str>,
    /// Term CURIEs referenced by any fixture (`dimTestReach`).
    fixture_any: HashSet<&'a str>,
    /// Term CURIEs referenced by ≥1 worked example (`dimWorkedInstance`).
    example_terms: HashSet<&'a str>,
    /// Term IRIs exercised by ≥1 competency question (`dimTestReach`).
    competency_reached: HashSet<&'a str>,
    /// Term IRIs exercised by ≥1 competency question carrying a non-blank
    /// rationale (`dimCompetencyRationale`).
    competency_rationale: HashSet<&'a str>,
    /// Term IRI → the non-blank competency rationale strings exercising it (for
    /// `dimProvenanceHonesty` / `dimProseQuality`).
    term_rationales: HashMap<&'a str, Vec<&'a str>>,
    /// Term IRIs carrying ≥1 dynamic projection-loss ledger row.
    loss_iris: HashSet<&'a str>,
    /// Local names of authored static projection-loss targets.
    loss_target_locals: HashSet<&'a str>,
    /// Term IRI → the preservation-kind local names of its dynamic loss rows, and
    /// term IRI-local → the kinds of its static loss targets — the `docJudgment`
    /// values `dimLossJudgmentSound` reads (the same kinds the evidence DAG emits).
    loss_kinds_by_iri: HashMap<&'a str, Vec<&'a str>>,
    /// Loss-target local name → the preservation-kind local names it declares.
    loss_kinds_by_local: HashMap<&'a str, Vec<&'a str>>,
    /// The configured non-English translation languages (empty ⇒ vacuous).
    non_english_langs: Vec<&'a str>,
    /// The translation index (`dimTranslationCoverage`).
    translations: &'a Translations,
}

impl<'a> CoverageContext<'a> {
    /// Precompute the model-wide lookup sets shared by every per-term detector.
    pub fn new(model: &'a DocsModel) -> Self {
        let aligned = alignment_subjects(model);

        let linkage_covered = model
            .linkages
            .iter()
            .filter(|l| l.mapping_set.is_some())
            .map(|l| l.subject.as_str())
            .collect();

        let mut fixture_wellformed = HashSet::new();
        let mut fixture_counter = HashSet::new();
        let mut fixture_any = HashSet::new();
        for fixture in &model.fixtures {
            for curie in &fixture.terms_referenced {
                fixture_any.insert(curie.as_str());
                match fixture.kind {
                    DocFixtureKind::Wellformed => fixture_wellformed.insert(curie.as_str()),
                    DocFixtureKind::CounterExample => fixture_counter.insert(curie.as_str()),
                };
            }
        }

        let example_terms = model
            .examples
            .iter()
            .flat_map(|e| e.terms_referenced.iter().map(String::as_str))
            .collect();

        let mut competency_reached = HashSet::new();
        let mut competency_rationale = HashSet::new();
        let mut term_rationales: HashMap<&str, Vec<&str>> = HashMap::new();
        for cq in &model.competencies {
            let rationale = cq.rationale.as_deref().filter(|r| !r.trim().is_empty());
            for iri in &cq.exercises {
                competency_reached.insert(iri.as_str());
                if let Some(r) = rationale {
                    competency_rationale.insert(iri.as_str());
                    term_rationales.entry(iri.as_str()).or_default().push(r);
                }
            }
        }

        let mut loss_iris = HashSet::new();
        let mut loss_kinds_by_iri: HashMap<&str, Vec<&str>> = HashMap::new();
        if let Some(digest) = &model.term_loss {
            for (iri, rows) in &digest.by_term {
                if !rows.is_empty() {
                    loss_iris.insert(iri.as_str());
                    loss_kinds_by_iri
                        .entry(iri.as_str())
                        .or_default()
                        .extend(rows.iter().map(|r| r.preservation_kind.as_str()));
                }
            }
        }
        let mut loss_target_locals = HashSet::new();
        let mut loss_kinds_by_local: HashMap<&str, Vec<&str>> = HashMap::new();
        for lt in &model.loss_targets {
            loss_target_locals.insert(lt.target.as_str());
            loss_kinds_by_local
                .entry(lt.target.as_str())
                .or_default()
                .push(lt.preservation_kind.as_str());
        }

        let non_english_langs = model
            .translations
            .languages()
            .iter()
            .map(String::as_str)
            .filter(|l| *l != ENGLISH && !l.eq_ignore_ascii_case("en"))
            .collect();

        Self {
            aligned,
            linkage_covered,
            fixture_wellformed,
            fixture_counter,
            fixture_any,
            example_terms,
            competency_reached,
            competency_rationale,
            term_rationales,
            loss_iris,
            loss_target_locals,
            loss_kinds_by_iri,
            loss_kinds_by_local,
            non_english_langs,
            translations: &model.translations,
        }
    }

    /// Whether EVERY projection-loss preservation judgment the term carries is
    /// sound-or-stronger (`dimLossJudgmentSound`, the Principle-17 MAXIMAL
    /// refinement). Reads the same preservation-kind values the loss `DocEvidence`
    /// node emits, over the canonical [`PreservationKind`] ordering — a
    /// deterministic field comparison, never a whitelist. Vacuously true when the
    /// term carries no loss rows (the empty universal, consistent with the other
    /// all-present vacuities).
    fn loss_judgment_sound(&self, term: &DocTerm) -> bool {
        let dynamic = self.loss_kinds_by_iri.get(term.iri.as_str());
        let static_ = self.loss_kinds_by_local.get(local_name(&term.iri));
        dynamic
            .into_iter()
            .chain(static_)
            .flat_map(|v| v.iter())
            .all(|kind| is_sound_or_stronger(kind))
    }

    /// Whether the term's translatable carrier strings (label, definition) are
    /// present in every configured non-English language. Vacuously true when no
    /// non-English language is configured (a bare unit-test model) — the empty
    /// universal, matching the FCA vacuous-coverage convention.
    fn translation_covered(&self, term: &DocTerm) -> bool {
        if self.non_english_langs.is_empty() {
            return true;
        }
        // The authored (canonical) carrier is what needs translating — read it, not
        // the possibly already-localized display field (see `DocTerm::coverage_label`).
        let label_needed = term.coverage_label().is_some_and(|s| !s.trim().is_empty());
        let def_needed = term
            .coverage_definition()
            .is_some_and(|s| !s.trim().is_empty());
        self.non_english_langs.iter().all(|lang| {
            (!label_needed
                || self
                    .translations
                    .lookup(&term.iri, RDFS_LABEL, lang)
                    .is_some())
                && (!def_needed
                    || self
                        .translations
                        .lookup(&term.iri, SKOS_DEFINITION, lang)
                        .is_some())
        })
    }

    /// The term's competency rationales (empty when it carries none).
    fn rationales(&self, term: &DocTerm) -> &[&'a str] {
        self.term_rationales
            .get(term.iri.as_str())
            .map_or(&[][..], Vec::as_slice)
    }
}

/// Which documentation-coverage dimensions a single term carries. One boolean per
/// PER-TERM [`crate::maturity::Dimension`], in [`DIMENSIONS`] order (the two
/// slice-scoped dimensions — thesis sentence, realized state — are computed on the
/// slice, not here).
///
/// # Present vs applicable vs covered
///
/// Each `has_*` field is a raw PRESENT fact (the detector fired). Fifteen of the
/// seventeen dimensions are UNCONDITIONALLY applicable — they hold against every
/// term — so for them a term is COVERED exactly when it is present. The four
/// external-correspondence / lossy-projection dimensions ([`Dimension::Alignment`],
/// [`Dimension::LinkageCoverage`], [`Dimension::LossLedgerRow`],
/// [`Dimension::LossJudgmentSound`]) are APPLICABILITY-CONDITIONED: GMEOW is a
/// SUPERSET ontology, so a term that maps to nothing external (a novel term) or is
/// a lossy projection of nothing is NOT required to carry an alignment, a linkage,
/// or a loss-ledger row. For those dimensions coverage is
/// `covered = !applicable ∨ present`: a not-applicable dimension is COVERED (never a
/// coverage defect), while a term that DECLARES an external correspondence or IS a
/// lossy-projection source but has no documented mapping / row is applicable ∧
/// ¬present → genuinely MISSING (a real defect still worth catching).
///
/// [`Self::flags`] returns the COVERED array (`!applicable ∨ present`), so the lint
/// gate, the emitted incidence, and the rendered page all read one covered/missing
/// truth; [`Self::present_count`] / [`Self::missing_keys`] follow the same
/// semantics (missing = applicable ∧ ¬present).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermCoverage {
    /// `skos:definition`/`rdfs:comment` is non-empty.
    pub has_definition: bool,
    /// `rdfs:label` is non-empty.
    pub has_label: bool,
    /// At least one of `gmeow:useWhen`/`avoidWhen`/`howToUse` is present.
    pub has_usage_advice: bool,
    /// At least one `skos:example`.
    pub has_example: bool,
    /// At least one `skos:scopeNote`.
    pub has_scope_note: bool,
    /// The term IRI is the subject of at least one external alignment.
    pub has_alignment: bool,
    /// Referenced by BOTH a well-formed and a counter-example fixture.
    pub has_fixture_pair: bool,
    /// Exercised by a competency question carrying a rationale.
    pub has_competency_rationale: bool,
    /// Demonstrated by a worked instance under `examples/`.
    pub has_worked_instance: bool,
    /// Carries a projection-loss ledger row.
    pub has_loss_ledger_row: bool,
    /// Subject of a mapping-set-backed linkage.
    pub has_linkage_coverage: bool,
    /// The full advice coat: useWhen ∧ avoidWhen ∧ howToUse ∧ graphBoxRole.
    pub has_annotation_coat: bool,
    /// Carrier strings present in every configured non-English language.
    pub has_translation_coverage: bool,
    /// Reached by at least one test (competency or fixture).
    pub has_test_reach: bool,
    /// The term's rationales name no test artifact.
    pub has_provenance_honesty: bool,
    /// The prose-quality structural conjunction.
    pub has_prose_quality: bool,
    /// Every projection-loss judgment is sound-or-stronger (the MAXIMAL-only
    /// Principle-17 refinement; vacuously true with no loss rows).
    pub has_loss_judgment_sound: bool,
    /// APPLICABILITY of the external-correspondence dimensions
    /// ([`Dimension::Alignment`], [`Dimension::LinkageCoverage`]): the term declares
    /// an external-correspondence intent — a non-empty `gmeow:adoptionTarget`, OR it
    /// is already the subject of an alignment / mapping-set-backed linkage. `false`
    /// for a superset-native term that maps to nothing external (then both
    /// dimensions are covered by non-applicability, never penalized).
    pub applicable_external: bool,
    /// APPLICABILITY of the lossy-projection dimensions ([`Dimension::LossLedgerRow`],
    /// [`Dimension::LossJudgmentSound`]): the term is a lossy-projection source — it
    /// appears in the projection-loss ledger (a dynamic loss row or an authored
    /// static loss target). `false` for a native, non-projected term (then both
    /// dimensions are covered by non-applicability, never penalized).
    pub applicable_lossy: bool,
}

/// One documentation-coverage dimension: the canonical [`Dimension`] key (the join
/// to `gmeow:dim*` and the maturity engine), a stable machine key (for the search
/// index and lint slug), a human display label, and the `docs/missing-*` lint code
/// it drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageDimension {
    /// The canonical maturity dimension — its `local_name()` is the `gmeow:dim*`
    /// individual and the three-way join key.
    pub dimension: Dimension,
    /// Stable machine key, never localized: e.g. `"usage_advice"`.
    pub key: &'static str,
    /// Human display label for the rendered page: e.g. `"Usage advice"`.
    pub label: &'static str,
    /// The `docs/missing-*` lint code this dimension's absence fires.
    pub lint_code: &'static str,
}

/// The PER-TERM coverage dimensions in stable [`Dimension`] order. Each mirrors a
/// `docs/missing-*` lint code, and the order matches [`TermCoverage::flags`] and
/// the per-term subset of [`Dimension::ALL`]. The two slice-scoped dimensions
/// ([`SLICE_DIMENSIONS`]) are NOT here.
pub const DIMENSIONS: [CoverageDimension; TermCoverage::TOTAL] = [
    CoverageDimension {
        dimension: Dimension::Definition,
        key: "definition",
        label: "Definition",
        lint_code: "docs/missing-definition",
    },
    CoverageDimension {
        dimension: Dimension::Label,
        key: "label",
        label: "Label",
        lint_code: "docs/missing-label",
    },
    CoverageDimension {
        dimension: Dimension::UsageAdvice,
        key: "usage_advice",
        label: "Usage advice",
        lint_code: "docs/missing-usage-advice",
    },
    CoverageDimension {
        dimension: Dimension::Example,
        key: "example",
        label: "Example",
        lint_code: "docs/missing-example",
    },
    CoverageDimension {
        dimension: Dimension::ScopeNote,
        key: "scope_note",
        label: "Scope note",
        lint_code: "docs/missing-scope-note",
    },
    CoverageDimension {
        dimension: Dimension::Alignment,
        key: "alignment",
        label: "Alignment",
        lint_code: "docs/missing-alignment",
    },
    CoverageDimension {
        dimension: Dimension::FixturePair,
        key: "fixture_pair",
        label: "Fixture pair",
        lint_code: "docs/missing-fixture-pair",
    },
    CoverageDimension {
        dimension: Dimension::CompetencyRationale,
        key: "competency_rationale",
        label: "Competency rationale",
        lint_code: "docs/missing-competency-rationale",
    },
    CoverageDimension {
        dimension: Dimension::WorkedInstance,
        key: "worked_instance",
        label: "Worked instance",
        lint_code: "docs/missing-worked-instance",
    },
    CoverageDimension {
        dimension: Dimension::LossLedgerRow,
        key: "loss_ledger_row",
        label: "Loss ledger row",
        lint_code: "docs/missing-loss-ledger-row",
    },
    CoverageDimension {
        dimension: Dimension::LinkageCoverage,
        key: "linkage_coverage",
        label: "Linkage coverage",
        lint_code: "docs/missing-linkage-coverage",
    },
    CoverageDimension {
        dimension: Dimension::AnnotationCoat,
        key: "annotation_coat",
        label: "Annotation coat",
        lint_code: "docs/missing-annotation-coat",
    },
    CoverageDimension {
        dimension: Dimension::TranslationCoverage,
        key: "translation_coverage",
        label: "Translation coverage",
        lint_code: "docs/missing-translation-coverage",
    },
    CoverageDimension {
        dimension: Dimension::TestReach,
        key: "test_reach",
        label: "Test reach",
        lint_code: "docs/missing-test-reach",
    },
    CoverageDimension {
        dimension: Dimension::ProvenanceHonesty,
        key: "provenance_honesty",
        label: "Provenance honesty",
        lint_code: "docs/missing-provenance-honesty",
    },
    CoverageDimension {
        dimension: Dimension::ProseQuality,
        key: "prose_quality",
        label: "Prose quality",
        lint_code: "docs/missing-prose-quality",
    },
    CoverageDimension {
        dimension: Dimension::LossJudgmentSound,
        key: "loss_judgment_sound",
        label: "Loss judgment sound",
        lint_code: "docs/missing-loss-judgment-sound",
    },
];

/// The DEMONSTRATION dimensions — the slice-level testing / documentation PRACTICES
/// a slice covers by DEMONSTRATING them on AT LEAST ONE applicable term (∃), not the
/// per-term qualities every term must individually carry (∀).
///
/// A fixture pair ("a rule with no negative fixture is not enforced"), a competency
/// question with a rationale, and a worked-instance scene each document the SLICE's
/// vocabulary and testing discipline — the slice demonstrates the practice when one
/// applicable term exhibits it; it is not evidence that *every* term must ship its
/// own fixture / CQ / worked scene. So for these three the per-SLICE aggregation is
/// existential (`slice covers ⟺ ∃ applicable term present`, vacuously covered when
/// no term is applicable), while every other per-term dimension stays universal
/// (`∀ applicable terms present`). The per-TERM incidence
/// (`gmeow:docCoversDimension` / `gmeow:docMissesDimension`) is UNCHANGED — it still
/// records each term's individual status as the diagnostic; only the slice roll-up
/// of these three flips from ∀ to ∃. The [`Dimension`] / [`DIMENSIONS`] vocabulary
/// and the [`crate::maturity`] anchor intents are untouched.
pub const DEMONSTRATION_DIMENSIONS: [Dimension; 3] = [
    Dimension::FixturePair,
    Dimension::CompetencyRationale,
    Dimension::WorkedInstance,
];

/// The SLICE-SCOPED coverage dimensions — computed directly on a slice's `docs.md`
/// facts, not per term. Kept in [`Dimension`] order (thesis sentence sits before
/// realized state's rank only incidentally; both are Maximal-tier for thesis and
/// Full-tier for realized state per the anchor intents).
pub const SLICE_DIMENSIONS: [CoverageDimension; 2] = [
    CoverageDimension {
        dimension: Dimension::RealizedState,
        key: "realized_state",
        label: "Realized state",
        lint_code: "docs/missing-realized-state",
    },
    CoverageDimension {
        dimension: Dimension::ThesisSentence,
        key: "thesis_sentence",
        label: "Thesis sentence",
        lint_code: "docs/missing-thesis-sentence",
    },
];

impl TermCoverage {
    /// The number of PER-TERM coverage dimensions.
    pub const TOTAL: usize = 17;

    /// The RAW present fact for each per-term dimension, in [`DIMENSIONS`] order —
    /// the detector output BEFORE the applicability layer. Internal: coverage is
    /// read through [`Self::flags`] (the covered array); this is only zipped with
    /// [`Self::applicable_flags`] to derive it.
    fn present_flags(&self) -> [bool; Self::TOTAL] {
        [
            self.has_definition,
            self.has_label,
            self.has_usage_advice,
            self.has_example,
            self.has_scope_note,
            self.has_alignment,
            self.has_fixture_pair,
            self.has_competency_rationale,
            self.has_worked_instance,
            self.has_loss_ledger_row,
            self.has_linkage_coverage,
            self.has_annotation_coat,
            self.has_translation_coverage,
            self.has_test_reach,
            self.has_provenance_honesty,
            self.has_prose_quality,
            self.has_loss_judgment_sound,
        ]
    }

    /// The APPLICABILITY of each per-term dimension, in [`DIMENSIONS`] order. Fifteen
    /// dimensions are unconditionally applicable (`true`); the four
    /// external-correspondence / lossy-projection dimensions are gated on
    /// [`Self::applicable_external`] / [`Self::applicable_lossy`]. Order matches
    /// [`DIMENSIONS`]: index 5 = alignment, 9 = loss ledger row, 10 = linkage
    /// coverage, 16 = loss judgment sound.
    fn applicable_flags(&self) -> [bool; Self::TOTAL] {
        let ext = self.applicable_external;
        let lossy = self.applicable_lossy;
        [
            true,  // definition
            true,  // label
            true,  // usage_advice
            true,  // example
            true,  // scope_note
            ext,   // alignment
            true,  // fixture_pair
            true,  // competency_rationale
            true,  // worked_instance
            lossy, // loss_ledger_row
            ext,   // linkage_coverage
            true,  // annotation_coat
            true,  // translation_coverage
            true,  // test_reach
            true,  // provenance_honesty
            true,  // prose_quality
            lossy, // loss_judgment_sound
        ]
    }

    /// The COVERED flag for each per-term dimension, in [`DIMENSIONS`] order —
    /// `covered = !applicable ∨ present`. A not-applicable dimension reports as
    /// COVERED (a superset-native term is never penalized for having no external
    /// correspondence or no lossy projection); every unconditional dimension reports
    /// its raw presence. This is THE coverage truth the lint gate, the emitted
    /// incidence, and the rendered page all read.
    pub fn flags(&self) -> [bool; Self::TOTAL] {
        let present = self.present_flags();
        let applicable = self.applicable_flags();
        std::array::from_fn(|i| !applicable[i] || present[i])
    }

    /// How many of the [`TOTAL`](Self::TOTAL) per-term dimensions the term COVERS
    /// (present, or not applicable). The renderer's "N of TOTAL dimensions" headline.
    pub fn present_count(&self) -> usize {
        self.flags().iter().filter(|covered| **covered).count()
    }

    /// The machine keys of the per-term dimensions the term is MISSING (applicable ∧
    /// ¬present), in display order — the search-index facet for filtering
    /// under-documented terms. A not-applicable dimension is NEVER listed missing.
    pub fn missing_keys(&self) -> Vec<&'static str> {
        DIMENSIONS
            .iter()
            .zip(self.flags())
            .filter(|(_, covered)| !*covered)
            .map(|(dim, _)| dim.key)
            .collect()
    }

    /// The set of PER-TERM [`Dimension`]s the term covers (present, or not
    /// applicable) — its concept intent over the per-term attributes, fed into the
    /// FCA maturity closure.
    pub fn covered_dims(&self) -> DimSet {
        DIMENSIONS
            .iter()
            .zip(self.flags())
            .filter(|(_, covered)| *covered)
            .map(|(dim, _)| dim.dimension)
            .collect()
    }
}

/// The FOUR conjuncts of `dimProseQuality`, reported individually.
///
/// `dimProseQuality` is covered iff all four hold. Collapsing them to one bool at
/// the point of computation threw away exactly the information an author needs to
/// fix the term, so the conjunction is computed here and reduced by
/// [`ProseQualityDetail::covered`] only at the point of use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProseQualityDetail {
    /// The authored definition states a boundary — what the term is NOT
    /// ([`crate::prose::states_boundary`]).
    pub boundary: bool,
    /// At least one authored example is a worked triple
    /// ([`crate::prose::is_worked_triple`]).
    pub worked_example: bool,
    /// The usage coat (useWhen/avoidWhen/howToUse) is non-blank AND says something
    /// other than a verbatim restatement of the definition.
    pub usage_distinct: bool,
    /// At least one competency rationale is non-blank AND is not a verbatim
    /// restatement of the term's own label.
    pub rationale_distinct: bool,
}

impl ProseQualityDetail {
    /// The single bool `dimProseQuality` reads: every conjunct holds.
    #[must_use]
    pub fn covered(&self) -> bool {
        self.boundary && self.worked_example && self.usage_distinct && self.rationale_distinct
    }

    /// The names of the conjuncts that FAILED, in declaration order — the
    /// actionable per-term detail a diagnostic prints. Empty iff [`Self::covered`].
    #[must_use]
    pub fn unmet(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.boundary {
            out.push("definition states no boundary (what it is NOT)");
        }
        if !self.worked_example {
            out.push("no example is a worked triple");
        }
        if !self.usage_distinct {
            out.push("usage coat is blank or restates the definition");
        }
        if !self.rationale_distinct {
            out.push("no competency rationale distinct from the label");
        }
        out
    }
}

/// The four-way `dimProseQuality` conjunction for one term, reported per conjunct.
///
/// # This is `dimProseQuality`, feeding `axisDocMaturity` — NOT `axisProseQuality`
///
/// This function is the truthmaker for the `dimProseQuality` coverage dimension,
/// which enters the FCA maturity closure ([`crate::maturity`]) and so feeds the
/// slice-quality **`axisDocMaturity`** axis.
///
/// **`axisProseQuality` is a DIFFERENT measure.** It is
/// `gmeow_slice_quality::axes::prose_axis`, and it scores a different input set: the
/// mean pass-rate of two independent per-term checks (`skos:definition` states a
/// boundary; `skos:example` is a worked triple) over the slice's TBox terms read
/// straight from the RDF dataset — no usage coat, no competency rationale, no
/// four-way conjunction, and no doc-model filtering. The two share the STRICT
/// [`crate::prose`] predicates and nothing else. Confusing them is not hypothetical:
/// a hand-rolled detector built against one and compared against the other's score
/// disagreed for exactly this reason.
#[must_use]
pub fn prose_quality_detail(term: &DocTerm, ctx: &CoverageContext) -> ProseQualityDetail {
    // Read the CANONICAL (authored English) definition/label, never the localized
    // display text — see `term_coverage`.
    let definition = term.coverage_definition().unwrap_or("");
    let usage_joined = [
        term.use_when.join(" "),
        term.avoid_when.join(" "),
        term.how_to_use.join(" "),
    ]
    .join(" ");
    let rationales = ctx.rationales(term);
    ProseQualityDetail {
        boundary: crate::prose::states_boundary(definition),
        worked_example: term
            .examples
            .iter()
            .any(|e| crate::prose::is_worked_triple(e)),
        usage_distinct: !usage_joined.trim().is_empty() && usage_joined.trim() != definition.trim(),
        rationale_distinct: rationales.iter().any(|r| {
            !r.trim().is_empty() && Some(r.trim()) != term.coverage_label().map(str::trim)
        }),
    }
}

/// Compute a term's per-term coverage against a precomputed [`CoverageContext`].
pub fn term_coverage(term: &DocTerm, ctx: &CoverageContext) -> TermCoverage {
    let use_when = !term.use_when.is_empty();
    let avoid_when = !term.avoid_when.is_empty();
    let how_to_use = !term.how_to_use.is_empty();

    // Read the CANONICAL (authored English) definition/label, never the localized
    // display text: documentation-completeness is a property of the authored source,
    // so viewing a term in French must not change its score (see
    // `DocTerm::coverage_label`). On an English/unlocalized model these fall back to
    // the display fields, so English scoring is unchanged.
    let definition = term.coverage_definition().unwrap_or("");

    let rationales = ctx.rationales(term);
    let provenance_honesty = !rationales.iter().any(|r| names_test_artifact(r));
    // The four-way conjunction is computed per conjunct and reduced here, so the
    // per-conjunct detail survives for diagnostics instead of collapsing at source.
    let prose_quality = prose_quality_detail(term, ctx).covered();

    // Applicability of the external-correspondence dimensions: the term declares an
    // external-correspondence intent (a non-empty `gmeow:adoptionTarget`) OR is
    // already the subject of an alignment / mapping-set-backed linkage. A
    // superset-native term with none is NOT applicable — its `dimAlignment` /
    // `dimLinkageCoverage` are covered by non-applicability, never a coverage defect.
    let applicable_external = !term.adoption_targets.is_empty()
        || ctx.aligned.contains(term.iri.as_str())
        || ctx.linkage_covered.contains(term.iri.as_str());
    // Applicability of the lossy-projection dimensions: the term is a lossy-projection
    // source — it carries a dynamic projection-loss ledger row or an authored static
    // loss target. A native, non-projected term is NOT applicable — its
    // `dimLossLedgerRow` / `dimLossJudgmentSound` are covered by non-applicability.
    let applicable_lossy = ctx.loss_iris.contains(term.iri.as_str())
        || ctx.loss_target_locals.contains(local_name(&term.iri));

    TermCoverage {
        has_definition: !definition.trim().is_empty(),
        has_label: !term.coverage_label().unwrap_or("").trim().is_empty(),
        has_usage_advice: use_when || avoid_when || how_to_use,
        has_example: !term.examples.is_empty(),
        has_scope_note: !term.scope_notes.is_empty(),
        has_alignment: ctx.aligned.contains(term.iri.as_str()),
        has_fixture_pair: ctx.fixture_wellformed.contains(term.curie.as_str())
            && ctx.fixture_counter.contains(term.curie.as_str()),
        has_competency_rationale: ctx.competency_rationale.contains(term.iri.as_str()),
        has_worked_instance: ctx.example_terms.contains(term.curie.as_str()),
        has_loss_ledger_row: ctx.loss_iris.contains(term.iri.as_str())
            || ctx.loss_target_locals.contains(local_name(&term.iri)),
        has_linkage_coverage: ctx.linkage_covered.contains(term.iri.as_str()),
        has_annotation_coat: use_when && avoid_when && how_to_use && term.box_role.is_some(),
        has_translation_coverage: ctx.translation_covered(term),
        has_test_reach: ctx.competency_reached.contains(term.iri.as_str())
            || ctx.fixture_any.contains(term.curie.as_str()),
        has_provenance_honesty: provenance_honesty,
        has_prose_quality: prose_quality,
        has_loss_judgment_sound: ctx.loss_judgment_sound(term),
        applicable_external,
        applicable_lossy,
    }
}

/// The covered-dimension set for a whole slice — its concept intent over ALL
/// nineteen dimensions, from which the FCA earned maturity and coverage fraction
/// are derived.
///
/// Two aggregation modes, keyed on [`DEMONSTRATION_DIMENSIONS`]:
///
/// - **Per-term quality (∀ — the default).** A slice COVERS the dimension iff EVERY
///   documented term it owns COVERS it — where per-term coverage is
///   [`TermCoverage::flags`] (`!applicable ∨ present`). For the unconditional
///   dimensions that is "every term present"; for the four applicability-conditioned
///   dimensions ([`Dimension::Alignment`], [`Dimension::LinkageCoverage`],
///   [`Dimension::LossLedgerRow`], [`Dimension::LossJudgmentSound`]) a term to which
///   the dimension does NOT apply covers it vacuously, so the slice covers it iff
///   every term for which it IS applicable has it present — and a slice of all
///   non-applicable terms covers it vacuously, never penalized.
///
/// - **Slice demonstration (∃ — [`DEMONSTRATION_DIMENSIONS`]: fixture pair,
///   competency rationale, worked instance).** These are slice-level PRACTICES; the
///   slice covers one iff AT LEAST ONE applicable term demonstrates it
///   (`∃ applicable term present`), and it is covered vacuously when no term is
///   applicable. A slice does not have to make *every* term carry its own fixture /
///   CQ / worked scene to demonstrate the discipline. The per-TERM incidence is
///   unchanged (still missing-per-term as the diagnostic); only this roll-up flips.
///
/// A slice with no documented terms vacuously covers every per-term dimension (the
/// empty universal, in both modes). The two slice-scoped dimensions (thesis
/// sentence, realized state) are read directly from the slice's `docs.md` facts.
pub fn slice_covered_dims(slice_iri: &str, model: &DocsModel, ctx: &CoverageContext) -> DimSet {
    let covs: Vec<TermCoverage> = model
        .terms
        .iter()
        .filter(|t| t.owner_slice == slice_iri)
        .map(|t| term_coverage(t, ctx))
        .collect();

    // Hoist each term's flag arrays OUT of the per-dimension loop below: computed
    // ONCE per term here (`applicable_flags()`/`present_flags()`/`flags()` each
    // reconstruct a size-TOTAL array), then indexed by dimension inside the loop —
    // O(N × TOTAL) instead of O(N × TOTAL²), with identical values in the same order.
    type FlagArray = [bool; TermCoverage::TOTAL];
    let per_term: Vec<(FlagArray, FlagArray, FlagArray)> = covs
        .iter()
        .map(|cov| {
            let applicable = cov.applicable_flags();
            let present = cov.present_flags();
            // `covered = !applicable ∨ present`, matching `TermCoverage::flags`
            // exactly — computed here from the already-hoisted arrays rather than
            // calling `flags()` again, which would redundantly recompute both.
            let flags = std::array::from_fn(|i| !applicable[i] || present[i]);
            (applicable, present, flags)
        })
        .collect();

    let mut covered = DimSet::new();
    for (i, dim) in DIMENSIONS.iter().enumerate() {
        let dim_covered = if DEMONSTRATION_DIMENSIONS.contains(&dim.dimension) {
            // ∃: covered iff no applicable term (vacuous) or ≥1 applicable term
            // demonstrates the practice. `present ∧ applicable`, never the covered
            // flag — a not-applicable term must not vacuously "demonstrate".
            let mut any_applicable = false;
            let mut demonstrated = false;
            for (applicable, present, _) in &per_term {
                if applicable[i] {
                    any_applicable = true;
                    if present[i] {
                        demonstrated = true;
                        break;
                    }
                }
            }
            !any_applicable || demonstrated
        } else {
            // ∀: covered iff every term covers it (`!applicable ∨ present`).
            per_term.iter().all(|(_, _, flags)| flags[i])
        };
        if dim_covered {
            covered.insert(dim.dimension);
        }
    }
    if let Some(slice) = model.slices.iter().find(|s| s.iri == slice_iri) {
        if slice.realized_state_complete {
            covered.insert(Dimension::RealizedState);
        }
        if slice.has_thesis_sentence {
            covered.insert(Dimension::ThesisSentence);
        }
    }
    covered
}

#[path = "coverage.tests.rs"]
#[cfg(test)]
mod tests;
