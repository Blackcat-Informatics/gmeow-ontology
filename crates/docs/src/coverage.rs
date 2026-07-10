// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Documentation-coverage status — the SINGLE source of the per-term and
//! per-slice coverage predicates.
//!
//! Both the lint gate ([`crate::lint()`], which emits a `docs/missing-*` warning per
//! absent dimension) and the rendered docs site ([`crate::render`], which surfaces
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
// These mirror the CONSERVATIVE, word-boundary-checked heuristics the
// slice-quality kernel uses (`crates/slice-quality/src/axes.rs`), reimplemented
// here as pure functions over the doc model's own strings — the two crates read
// DIFFERENT inputs (slice-quality reads the raw RDF dataset + filesystem; this
// reads the typed [`DocsModel`]), so this is not a duplicated producer but the
// same deterministic idea applied to a different carrier. Each is a present/absent
// structural fact, never a tuned score.

/// True if `word` occurs in `corpus` at identifier/word boundaries — the char on
/// each side is neither an ASCII alphanumeric nor `_`/`-`. Keeps an INCIDENTAL
/// substring (`"whenever"` containing `"never"`, `"NOTE"` containing `"not"`) from
/// counting, which would silently inflate a ratchet-gated dimension.
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

/// True if a definition states a boundary ("what it is NOT") via a negation cue,
/// matched at word boundaries on the lowercased text.
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
    CUES.iter().any(|cue| word_at_boundary(&d, cue))
}

/// True if `s` carries a turtle CURIE token (`prefix:local`): a `:` with a name
/// char before it and an alphanumeric/`_` after. Rejects a bare prose colon and a
/// full-IRI scheme (`http://`).
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

/// A worked triple names a term via a CURIE and carries turtle statement structure
/// (the `a` type keyword or a `; , .` terminator).
fn is_worked_triple(example: &str) -> bool {
    has_curie(example)
        && (word_at_boundary(example, "a")
            || example.contains(" ;")
            || example.contains(" .")
            || example.contains(" ,")
            || example.ends_with('.')
            || example.ends_with(';'))
}

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
        let label_needed = term.label.as_deref().is_some_and(|s| !s.trim().is_empty());
        let def_needed = term
            .definition
            .as_deref()
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
/// slice, not here). A dimension is "present" exactly when the corresponding
/// `docs/missing-*` lint would NOT fire.
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

    /// The presence flag for each per-term dimension, in [`DIMENSIONS`] order.
    pub fn flags(&self) -> [bool; Self::TOTAL] {
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

    /// How many of the [`TOTAL`](Self::TOTAL) per-term dimensions the term carries.
    pub fn present_count(&self) -> usize {
        self.flags().iter().filter(|present| **present).count()
    }

    /// The machine keys of the per-term dimensions the term is MISSING, in display
    /// order — the search-index facet for filtering under-documented terms.
    pub fn missing_keys(&self) -> Vec<&'static str> {
        DIMENSIONS
            .iter()
            .zip(self.flags())
            .filter(|(_, present)| !*present)
            .map(|(dim, _)| dim.key)
            .collect()
    }

    /// The set of PER-TERM [`Dimension`]s the term covers — its concept intent
    /// over the per-term attributes, fed into the FCA maturity closure.
    pub fn covered_dims(&self) -> DimSet {
        DIMENSIONS
            .iter()
            .zip(self.flags())
            .filter(|(_, present)| *present)
            .map(|(dim, _)| dim.dimension)
            .collect()
    }
}

/// Compute a term's per-term coverage against a precomputed [`CoverageContext`].
pub fn term_coverage(term: &DocTerm, ctx: &CoverageContext) -> TermCoverage {
    let use_when = !term.use_when.is_empty();
    let avoid_when = !term.avoid_when.is_empty();
    let how_to_use = !term.how_to_use.is_empty();

    let definition = term.definition.as_deref().unwrap_or("");
    let usage_joined = [
        term.use_when.join(" "),
        term.avoid_when.join(" "),
        term.how_to_use.join(" "),
    ]
    .join(" ");
    let usage_distinct =
        !usage_joined.trim().is_empty() && usage_joined.trim() != definition.trim();

    let rationales = ctx.rationales(term);
    let provenance_honesty = !rationales.iter().any(|r| names_test_artifact(r));
    let rationale_distinct = rationales
        .iter()
        .any(|r| !r.trim().is_empty() && Some(r.trim()) != term.label.as_deref().map(str::trim));
    let prose_quality = states_boundary(definition)
        && term.examples.iter().any(|e| is_worked_triple(e))
        && usage_distinct
        && rationale_distinct;

    TermCoverage {
        has_definition: !definition.trim().is_empty(),
        has_label: !term.label.as_deref().unwrap_or("").trim().is_empty(),
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
    }
}

/// The covered-dimension set for a whole slice — its concept intent over ALL
/// nineteen dimensions, from which the FCA earned maturity and coverage fraction
/// are derived.
///
/// A slice COVERS a per-term dimension iff EVERY documented term it owns covers it
/// (the definitional `1.0 = all-present`, never a threshold; a slice with no
/// documented terms vacuously covers every per-term dimension — the empty
/// universal). The two slice-scoped dimensions (thesis sentence, realized state)
/// are read directly from the slice's `docs.md` facts.
pub fn slice_covered_dims(slice_iri: &str, model: &DocsModel, ctx: &CoverageContext) -> DimSet {
    let flags: Vec<[bool; TermCoverage::TOTAL]> = model
        .terms
        .iter()
        .filter(|t| t.owner_slice == slice_iri)
        .map(|t| term_coverage(t, ctx).flags())
        .collect();

    let mut covered = DimSet::new();
    for (i, dim) in DIMENSIONS.iter().enumerate() {
        if flags.iter().all(|f| f[i]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DocTermCategory;

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

    fn bare(local: &str) -> DocTerm {
        DocTerm {
            iri: format!("{GMEOW}{local}"),
            curie: format!("gmeow:{local}"),
            category: DocTermCategory::Class,
            owner_slice: format!("{GMEOW}slice/zoo"),
            ..Default::default()
        }
    }

    #[test]
    fn empty_model_bare_term_covers_only_vacuous_dimensions() {
        let model = DocsModel::default();
        let ctx = CoverageContext::new(&model);
        let term = bare("Cat");
        let cov = term_coverage(&term, &ctx);
        // A bare term with no annotations: definition/label/... all absent. The
        // three vacuously-true dimensions with no configured universe are translation
        // (no non-English langs), provenance honesty (no rationale to be dishonest),
        // and loss-judgment-sound (no loss rows to be unsound).
        assert!(!cov.has_definition);
        assert!(
            cov.has_translation_coverage,
            "vacuous with no non-English langs"
        );
        assert!(cov.has_provenance_honesty, "vacuous with no rationale");
        assert!(cov.has_loss_judgment_sound, "vacuous with no loss rows");
        assert!(!cov.has_prose_quality);
        assert_eq!(cov.flags().len(), TermCoverage::TOTAL);
    }

    #[test]
    fn loss_judgment_sound_reads_the_preservation_ordering() {
        assert!(is_sound_or_stronger("ExactPreservation"));
        assert!(is_sound_or_stronger("SoundUnderApproximation"));
        // Weaker-than-sound kinds fail; an unrecognized kind is not provably sound.
        assert!(!is_sound_or_stronger("ValidationOnly"));
        assert!(!is_sound_or_stronger("CompleteOverApproximation"));
        assert!(!is_sound_or_stronger("Unsupported"));
        assert!(!is_sound_or_stronger("NotAKind"));
    }

    #[test]
    fn dimensions_and_flags_agree_in_length_and_order() {
        // The three-way order contract lives in tests/coverage_dimensions.rs; here we
        // pin the local invariant that flags() and DIMENSIONS are the same length and
        // that each DIMENSIONS entry's dimension is the matching per-term variant.
        assert_eq!(DIMENSIONS.len(), TermCoverage::TOTAL);
        let term = bare("Cat");
        let model = DocsModel::default();
        let ctx = CoverageContext::new(&model);
        assert_eq!(term_coverage(&term, &ctx).flags().len(), DIMENSIONS.len());
    }

    #[test]
    fn prose_heuristics_are_conservative() {
        assert!(states_boundary("A relator, never a mere pair."));
        assert!(!states_boundary("Applies whenever a bearer exists."));
        assert!(is_worked_triple("ex:x a gmeow:Foo ."));
        assert!(!is_worked_triple("See section 3: important."));
        assert!(names_test_artifact("see test_foo_bar for evidence"));
        assert!(names_test_artifact("Mirrors the fixture behaviour"));
        assert!(!names_test_artifact("a genuine ontological rationale"));
    }
}
