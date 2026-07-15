// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN-1 round-trip gate: the executed byte witness behind
//! `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic true` declaration, mirroring
//! [`crate::stages::superset`]'s byte-reconstruction discipline over the SAME
//! authority — [`purrdf::canonicalize`] — rather than a bespoke comparator.
//!
//! # Scope
//!
//! Per the F1 user decision (`gmeow:gmnCorrNormalToGmn`'s carrier declaration), the
//! codec + this gate are TOTAL over the **grounding slices' GMN-0 NOW**
//! (`slices/grounding/{logic,lang,math}`, authored `module.ttl` PLUS `examples/*.ttl` —
//! the SAME domain the `axisGmn1Coverage` slice-quality axis's own definition scopes
//! coverage to). Coverage of every other slice is a separate, floor-gated quality axis
//! (Task 7), not this gate's job — this gate never reads a non-grounding slice, so it
//! can never red on a non-grounding gap.
//!
//! # What the gate proves
//!
//! For every grounding source file: parse it to a [`purrdf::RdfDataset`], build a
//! [`gmeow_lang_bridge::Gmn0Model`], run `gmn1_read(gmn1_write(model))`, and assert
//! canonical equality via [`gmeow_lang_bridge::gmn0_canonically_equal`] (which itself
//! calls `purrdf::canonicalize` — the same canonical-comparison primitive the
//! GTS/N-Quads byte-teeth gates use). A write-side uncovered construct, a read-side
//! parse defect, or a canonical mismatch is a hard failure — no skips, no optional
//! coverage, a single non-round-tripping fixture reds the gate.
//!
//! # The construct-coverage-completeness audit
//!
//! [`check_gmn1_roundtrip`] proves every quad IN the grounding corpus round-trips
//! byte-exact. It does NOT prove the corpus actually EXERCISES every branch of the
//! codec's own write-side dispatch (a category with zero real occurrences could carry a
//! latent bug that no amount of round-tripping the SAME corpus would ever surface).
//! [`check_gmn1_construct_coverage`] closes that gap: it classifies every quad via
//! [`gmeow_lang_bridge::classify_quad`] (the SAME dispatch [`gmn1_write`] calls, so the
//! classification can never drift from what the codec actually does) and hard-fails if
//! any [`gmeow_lang_bridge::Gmn1ConstructCategory`] has zero occurrences across the real
//! grounding sources. Both audits run in `run.rs`'s reconcile phase; both are total,
//! hard-fail gates over the same source domain.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_lang_bridge::registry::NamedSource;
use gmeow_lang_bridge::{
    ConstructCoverageTally, Gmn0Model, Gmn1ConstructCategory, Gmn1Error, GmnDictionary,
    gmn0_canonically_equal, gmn1_read, gmn1_write, round_trip_check,
};
use purrdf::parse_dataset;
use purrdf::slice::SliceCatalog;

/// The grounding slice directories this gate is total over (mirrors the
/// `axisGmn1Coverage` axis's own `slices/grounding/` scope, minus `kernel`: the kernel
/// module carries no independent GMN-0 content beyond what `logic`/`lang`/`math`
/// already exercise structurally, and is folded into the `lang`/`logic` round-trips via
/// their cross-references — this gate's own corpus is the three content-bearing
/// grounding modules named in the carrier declaration and Task 6's own text).
const GROUNDING_SLICES: [&str; 3] = ["logic", "lang", "math"];

/// One grounding-slice source file's round-trip outcome, carrying the ONE typed
/// [`gmeow_lang_bridge::Gmn1Error`] the codec's canonical classifier produced — the gate
/// makes no second classification of its own (the deleted `Gmn1FailureKind` duplicate is
/// gone). `run.rs` routes the ledger split off [`Self::failure_class`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gmn1RoundTripFailure {
    /// The repo-relative source path (`slices/grounding/lang/module.ttl`, an
    /// `examples/*.ttl` fixture, …).
    pub path: String,
    /// The typed codec failure — the single source of both the human detail
    /// ([`Gmn1Error`]'s `Display`) and the canonical `lang:` class
    /// ([`Gmn1Error::failure_class`]).
    pub error: Gmn1Error,
}

impl Gmn1RoundTripFailure {
    /// The full `lang:` failure-class IRI, straight from the codec's one classifier.
    #[must_use]
    pub fn failure_class(&self) -> &'static str {
        self.error.failure_class()
    }
}

/// The gate's outcome: every grounding source that failed to round-trip losslessly.
/// Empty ⇒ the gate is clean — `gmeow:gmnCorrNormalToGmn`'s `mnemomorphic true` claim is
/// discharged for this run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gmn1RoundTripReport {
    /// Every failing source, in a stable (sorted-path) order.
    pub failures: Vec<Gmn1RoundTripFailure>,
}

impl Gmn1RoundTripReport {
    /// The gate passes when no grounding source failed to round-trip.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Load `gmeow:gmnDictV2` from the lang slice's authored `module.ttl` — the SAME
/// dictionary every grounding source is decoded/encoded against (one shipped
/// `gmeow:gmnDictV2` version, per the carrier's own version-pinning discipline). Shared
/// by [`check_gmn1_roundtrip`] and [`check_gmn1_construct_coverage`] so both audits load
/// the identical dictionary, never two independently-loaded copies.
fn load_lang_dictionary(root: &Path) -> Result<GmnDictionary, gmeow_errors::Diag> {
    let lang_module_path = root.join("slices/grounding/lang/module.ttl");
    let lang_bytes = std::fs::read(&lang_module_path)
        .map_err(|e| stage_err(&format!("read {}: {e}", lang_module_path.display())))?;
    let lang_ds = parse_dataset(&lang_bytes, "text/turtle", None)
        .map_err(|e| stage_err(&format!("parse {}: {e}", lang_module_path.display())))?;
    GmnDictionary::from_dataset(&lang_ds)
        .map_err(|e| stage_err(&format!("gmeow:gmnDictV2 failed to load: {}", e.0)))
}

/// Every grounding source path (`slices/grounding/<slice>/module.ttl` plus every
/// `examples/*.ttl` under it, repo-root-relative), sorted. Shared by
/// [`check_gmn1_roundtrip`] and [`check_gmn1_construct_coverage`] so the two audits can
/// never disagree about WHICH files are in the "total over grounding" domain.
fn collect_grounding_sources(root: &Path) -> Result<Vec<String>, gmeow_errors::Diag> {
    let mut sources: Vec<String> = Vec::new();
    for slice in GROUNDING_SLICES {
        let slice_dir = root.join("slices/grounding").join(slice);
        let module_path = slice_dir.join("module.ttl");
        if module_path.is_file() {
            sources.push(
                module_path
                    .strip_prefix(root)
                    .unwrap_or(&module_path)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        let examples_dir = slice_dir.join("examples");
        if examples_dir.is_dir() {
            let entries = std::fs::read_dir(&examples_dir)
                .map_err(|e| stage_err(&format!("read dir {}: {e}", examples_dir.display())))?;
            for entry in entries {
                let entry =
                    entry.map_err(|e| stage_err(&format!("dir entry in {slice}/examples: {e}")))?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
                    sources.push(
                        path.strip_prefix(root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
    }
    sources.sort();
    Ok(sources)
}

/// Run the GMN-1 round-trip gate over every grounding slice's `module.ttl` and
/// `examples/*.ttl` under `<root>/slices/grounding/`.
pub fn check_gmn1_roundtrip(root: &Path) -> Result<Gmn1RoundTripReport, gmeow_errors::Diag> {
    let dict = load_lang_dictionary(root)?;
    let sources = collect_grounding_sources(root)?;

    let mut failures = Vec::new();
    for source in &sources {
        let bytes = std::fs::read(root.join(source))
            .map_err(|e| stage_err(&format!("read {source}: {e}")))?;
        let ds = match parse_dataset(&bytes, "text/turtle", None) {
            Ok(ds) => ds,
            Err(e) => {
                // A grounding source that will not parse as Turtle cannot be lifted at all —
                // the residual `lang:GmnNonDecodableGrammar`.
                failures.push(Gmn1RoundTripFailure {
                    path: source.clone(),
                    error: Gmn1Error::NonDecodableGrammar {
                        detail: format!("failed to parse as Turtle: {e}"),
                    },
                });
                continue;
            }
        };
        let model = Gmn0Model::from_dataset(&ds);
        // The codec's OWN round-trip primitive is the single classifier: write, read,
        // canonically compare, and surface the ONE typed `Gmn1Error` (uncovered construct,
        // parse defect, or canonical mismatch) — the gate never re-derives a class of its own.
        if let Err(e) = round_trip_check(&model, &dict) {
            failures.push(Gmn1RoundTripFailure {
                path: source.clone(),
                error: e,
            });
        }
    }

    failures.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Gmn1RoundTripReport { failures })
}

/// A codec construct category the real grounding corpus never exercised — the
/// completeness gap [`check_gmn1_construct_coverage`] exists to catch.
///
/// [`check_gmn1_roundtrip`] proves every quad IN the grounding corpus round-trips
/// byte-exact through [`gmn1_write`]/[`gmn1_read`] — but that says nothing about whether
/// the corpus actually EXERCISES every branch of the codec's own write-side dispatch
/// ([`Gmn1ConstructCategory`]). A branch with zero real occurrences could carry a latent
/// encode/decode bug indefinitely: the round-trip gate would keep passing (nothing in
/// the corpus takes that branch, so nothing can expose a mismatch in it), while the
/// carrier's `logic:mnemomorphic true` claim implicitly asserts totality over
/// EVERYTHING the grounding slices actually emit — the very fragment this gate is
/// scoped to. This gate closes that gap mechanically, over the SAME source files and
/// the SAME dictionary [`check_gmn1_roundtrip`] uses.
pub fn check_gmn1_construct_coverage(
    root: &Path,
) -> Result<Gmn1ConstructCoverageReport, gmeow_errors::Diag> {
    let dict = load_lang_dictionary(root)?;
    let sources = collect_grounding_sources(root)?;

    let mut tally = ConstructCoverageTally::default();
    for source in &sources {
        let bytes = std::fs::read(root.join(source))
            .map_err(|e| stage_err(&format!("read {source}: {e}")))?;
        let ds = parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
            stage_err(&format!(
                "parse {source} for the GMN-1 construct-coverage audit: {e}"
            ))
        })?;
        let model = Gmn0Model::from_dataset(&ds);
        tally.absorb(&model, &dict);
    }

    Ok(Gmn1ConstructCoverageReport {
        unexercised: tally.unexercised_categories(),
        // A quad this tally found uncovered here is the SAME construct
        // `check_gmn1_roundtrip` would hard-fail on for the same source (both audits
        // call `gmn1_codec`'s own dispatch) — carried through so a caller can assert the
        // two audits agree, never as this gate's own primary failure surface.
        uncovered_quad_count: tally.uncovered.len(),
    })
}

/// [`check_gmn1_construct_coverage`]'s outcome: every codec construct category the real
/// grounding corpus never exercised. Empty ⇒ every category the codec's write-side
/// dispatch can produce ([`Gmn1ConstructCategory::ALL`]) is genuinely proven against
/// production content, not merely a corpus that happens to round-trip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gmn1ConstructCoverageReport {
    /// Every unexercised category, in [`Gmn1ConstructCategory::ALL`] order.
    pub unexercised: Vec<Gmn1ConstructCategory>,
    /// How many quads the SAME classification pass found uncovered — cross-checked
    /// against [`check_gmn1_roundtrip`]'s own failures by
    /// `construct_coverage_agrees_with_the_roundtrip_gate_on_uncovered_count`.
    pub uncovered_quad_count: usize,
}

impl Gmn1ConstructCoverageReport {
    /// The gate passes when every codec construct category was exercised at least once.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.unexercised.is_empty()
    }
}

// ── The shipped-projection lint: a production caller of the codec's classifier ─────────

/// One shipped GMN-1 projection artifact that failed to read back cleanly through the
/// production `gmn1_read` codec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gmn1ShippedFailure {
    /// The committed artifact path (`generated/projections/lang/gmn1/<name>.gmn`).
    pub path: String,
    /// The typed codec failure carrying the canonical `lang:` class
    /// ([`Gmn1Error::failure_class`]).
    pub error: Gmn1Error,
}

impl Gmn1ShippedFailure {
    /// The full `lang:` failure-class IRI, straight from the codec's one classifier.
    #[must_use]
    pub fn failure_class(&self) -> &'static str {
        self.error.failure_class()
    }
}

/// [`check_gmn1_shipped_projections`]'s outcome: every shipped GMN-1 projection that failed
/// to read cleanly. Empty ⇒ every shipped `generated/projections/lang/gmn1/*.gmn` reads back
/// through the production codec with `failure_class()` clean.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gmn1ShippedReport {
    /// Every failing shipped artifact, in a stable (sorted-path) order.
    pub failures: Vec<Gmn1ShippedFailure>,
    /// How many shipped artifacts were verified to read cleanly (the lint's positive count —
    /// a zero here over a non-empty projection dir is itself a wiring smell the caller checks).
    pub verified: usize,
}

impl Gmn1ShippedReport {
    /// The lint passes when every shipped GMN-1 projection read cleanly.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }
}

/// PRODUCTION consumer of the codec's canonical classifier over SHIPPED artifacts: read every
/// committed `generated/projections/lang/gmn1/*.gmn` back through the production
/// [`gmeow_lang_bridge::gmn1_read`] and assert `failure_class()` is clean.
///
/// The shipped `.gmn` text carries `r_<hash>` by-reference tokens whose reference table is the
/// codec's out-of-band resolution store (it does not live inside the `.gmn` bytes), so the
/// lint reconstructs each source's full [`gmeow_lang_bridge::Gmn1Document`] EXACTLY the way the projection stage
/// does (parse the lang-model source → [`Gmn0Model`] → [`gmn1_write`]), ASSERTS the shipped
/// bytes equal that document's text (so a stale artifact is a hard fail, tying the read to the
/// committed bytes), then reads the document back. A shipped projection that fails to read is a
/// hard fail naming the file and the `lang:` class — mirroring the source enumeration of
/// [`collect_grounding_sources`] but over the shipped projection directory.
pub fn check_gmn1_shipped_projections(
    root: &Path,
) -> Result<Gmn1ShippedReport, gmeow_errors::Diag> {
    let gmn1_dir = root
        .join(crate::stages::lang_projection::LANG_PROJECTION_DIR)
        .join("gmn1");
    if !gmn1_dir.is_dir() {
        // No shipped GMN-1 projections (e.g. a source checkout with no generated tree yet):
        // vacuously clean, nothing to lint.
        return Ok(Gmn1ShippedReport::default());
    }

    // The shipped artifacts, keyed by their `<name>` stem.
    let mut shipped: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for entry in std::fs::read_dir(&gmn1_dir)
        .map_err(|e| stage_err(&format!("read dir {}: {e}", gmn1_dir.display())))?
    {
        let entry = entry.map_err(|e| stage_err(&format!("dir entry in gmn1 projections: {e}")))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("gmn") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| stage_err(&format!("non-UTF-8 gmn artifact: {}", path.display())))?
                .to_owned();
            let bytes = std::fs::read(&path)
                .map_err(|e| stage_err(&format!("read {}: {e}", path.display())))?;
            shipped.insert(stem, bytes);
        }
    }

    // Rebuild the projection's lang-model sources EXACTLY as the projection stage does, so the
    // reconstructed documents (text + out-of-band refs) match the shipped artifacts byte-for-byte.
    let catalog =
        SliceCatalog::discover(&root.join("slices"), crate::gmeow_ns::gmeow_slice_vocab())
            .map_err(|e| stage_err(&format!("discover slice catalog: {e}")))?;
    let sources: Vec<NamedSource> =
        crate::stages::lang_projection::lang_model_sources(Some(&catalog))?;
    let dictionary = load_lang_dictionary(root)?;
    let by_name: BTreeMap<&str, &NamedSource> =
        sources.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut failures = Vec::new();
    let mut verified = 0usize;
    for (name, bytes) in &shipped {
        let path = format!(
            "{}/gmn1/{name}.gmn",
            crate::stages::lang_projection::LANG_PROJECTION_DIR
        );
        let source = by_name.get(name.as_str()).ok_or_else(|| {
            stage_err(&format!(
                "shipped GMN-1 projection {path} has no lang-model source — an orphan artifact"
            ))
        })?;
        let ds = parse_dataset(&source.bytes, "text/turtle", None)
            .map_err(|e| stage_err(&format!("parse lang-model source for {path}: {e}")))?;
        let model = Gmn0Model::from_dataset(&ds);
        // The projection stage and this lint both consume the one carrier-authored
        // dictionary/glyph registry. A source-local fallback would silently suppress every
        // executable glyph that is declared in grounding/lang rather than in the example.
        let doc = gmn1_write(&model, &dictionary).map_err(|e| {
            stage_err(&format!(
                "shipped GMN-1 projection {path} no longer writes from its source: {e}"
            ))
        })?;
        if doc.text.as_bytes() != bytes.as_slice() {
            return Err(stage_err(&format!(
                "shipped GMN-1 projection {path} is stale — its bytes differ from the current \
                 projection of its source (run `make sync`)"
            )));
        }
        // The production classifier over the shipped artifact (its full document, whose text
        // we just proved equals the committed bytes; the reference table is the codec's
        // out-of-band store): clean ⇒ Ok.
        match gmn1_read(&doc, &dictionary) {
            Ok(back) => {
                if gmn0_canonically_equal(&model, &back) {
                    verified += 1;
                } else {
                    failures.push(Gmn1ShippedFailure {
                        path,
                        error: Gmn1Error::NonDecodableGrammar {
                            detail: "shipped projection reads back to a different GMN-0 model \
                                     (round-trip canonical mismatch)"
                                .to_owned(),
                        },
                    });
                }
            }
            Err(e) => failures.push(Gmn1ShippedFailure { path, error: e }),
        }
    }

    failures.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Gmn1ShippedReport { failures, verified })
}

fn stage_err(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-gmn1-gate".to_string(),
        message: message.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn gate_is_clean_over_the_real_grounding_slices() {
        let root = repo_root();
        let report = check_gmn1_roundtrip(&root).expect("gate runs without a hard I/O error");
        assert!(
            report.is_clean(),
            "GMN-1 round-trip gate is not clean over the grounding slices: {:#?}",
            report.failures
        );
    }

    /// The gate's own negative teeth, proven through the SAME `check_gmn1_roundtrip`
    /// entry point `run.rs` wires into `make check` — not merely the codec's own unit
    /// tests. Builds a throwaway `<tmp>/slices/grounding/{logic,lang,math}` tree with a
    /// deliberately uncovered construct (an IRI under no registered namespace) in the
    /// `math` module and asserts the gate reds on it, naming the offending path — proof
    /// the gate has teeth at the file-I/O entry point a permanent fixture can safely
    /// exercise (unlike temporarily corrupting a real committed slice file).
    #[test]
    fn gate_reds_on_a_deliberately_uncovered_construct() {
        let dir =
            std::env::temp_dir().join(format!("gmeow-gmn1-gate-negative-{}", std::process::id()));
        let lang_dir = dir.join("slices/grounding/lang");
        let logic_dir = dir.join("slices/grounding/logic");
        let math_dir = dir.join("slices/grounding/math");
        std::fs::create_dir_all(&lang_dir).unwrap();
        std::fs::create_dir_all(&logic_dir).unwrap();
        std::fs::create_dir_all(&math_dir).unwrap();

        // A minimal but real current codebook. Its dictionary is deliberately empty (legal for
        // this fixture), but the graph still pins the same typed dictionary/script references and
        // dialect versions that production loading requires. That keeps this test focused on the
        // deliberately uncovered math construct rather than failing at registry bootstrap.
        std::fs::write(
            lang_dir.join("module.ttl"),
            b"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
              gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;\n\
                gmeow:references gmeow:fixtureDictionary, lang:fixtureScript ;\n\
                gmeow:gmnDictionaryVersion \"2\" ;\n\
                gmeow:gmnGlyphTableVersion \"2\" .\n\
              gmeow:fixtureDictionary a gmeow:GmnDictionary ;\n\
                gmeow:gmnDictionaryVersion \"2\" .\n\
              lang:fixtureScript a lang:Script ;\n\
                lang:hasGrapheme lang:fixtureGrapheme .\n",
        )
        .unwrap();
        std::fs::write(
            logic_dir.join("module.ttl"),
            b"@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n",
        )
        .unwrap();
        // The deliberately uncovered construct: an IRI under a namespace this codec's
        // prefix table does not register.
        std::fs::write(
            math_dir.join("module.ttl"),
            b"@prefix unreg: <https://not-a-registered-namespace.example/> .\n\
              unreg:subject unreg:predicate unreg:object .\n",
        )
        .unwrap();

        let report = check_gmn1_roundtrip(&dir).expect("gate runs without a hard I/O error");
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            !report.is_clean(),
            "the gate must red on a deliberately uncovered construct, not pass vacuously"
        );
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.path == "slices/grounding/math/module.ttl"
                    && f.failure_class() == Gmn1Error::CLASS_UNCOVERED_TERM),
            "the failure must name the offending source path AND classify as \
             lang:GmnUncoveredTerm (so run.rs routes it through the dedicated DiagLedger \
             identity, not the generic round-trip-mismatch code): {:#?}",
            report.failures
        );
    }

    #[test]
    fn construct_coverage_is_complete_over_the_real_grounding_slices() {
        let root = repo_root();
        let report =
            check_gmn1_construct_coverage(&root).expect("audit runs without a hard I/O error");
        assert!(
            report.is_complete(),
            "GMN-1 construct-coverage audit found {} the real grounding corpus \
             never exercises — the 'total over grounding' claim is unproven for: {:#?}",
            if report.unexercised.len() == 1 {
                "a category"
            } else {
                "categories"
            },
            report.unexercised
        );
        assert_eq!(
            report.uncovered_quad_count, 0,
            "the construct-coverage audit's own classification pass must find zero \
             uncovered quads over the real grounding corpus"
        );
    }

    /// Cross-checks the two independently-computed GMN-1 audits agree: the round-trip
    /// gate's own `Uncovered`-kind failure count and the construct-coverage audit's
    /// `uncovered_quad_count` must both be zero over the real grounding corpus (both
    /// call the SAME `gmn1_codec` dispatch, so a disagreement would itself be a bug).
    #[test]
    fn construct_coverage_agrees_with_the_roundtrip_gate_on_uncovered_count() {
        let root = repo_root();
        let roundtrip = check_gmn1_roundtrip(&root).expect("round-trip gate runs");
        let coverage = check_gmn1_construct_coverage(&root).expect("coverage audit runs");
        let roundtrip_uncovered = roundtrip
            .failures
            .iter()
            .filter(|f| f.failure_class() == Gmn1Error::CLASS_UNCOVERED_TERM)
            .count();
        assert_eq!(
            roundtrip_uncovered, 0,
            "sanity: the real grounding corpus has zero Uncovered round-trip failures"
        );
        assert_eq!(
            coverage.uncovered_quad_count, 0,
            "the construct-coverage audit must agree with the round-trip gate: zero \
             uncovered quads over the real grounding corpus"
        );
    }

    /// The construct-coverage audit's own negative teeth (proving this assertion is
    /// falsifiable, not vacuously true). Starting from REAL grounding content — not
    /// a fabricated fixture — this filters
    /// OUT every quad that hits [`Gmn1ConstructCategory::LiteralDecimal`] (the real
    /// corpus's rarest category: exactly one occurrence across all three grounding
    /// slices' module.ttl + examples, per the audit's own tally) and proves the SAME
    /// tally machinery [`check_gmn1_construct_coverage`] runs in production then flags
    /// that category unexercised. This demonstrates the completeness assertion has real
    /// teeth: removing a real grounding construct's only occurrence genuinely fails the
    /// audit, exactly the failure mode Task 3 exists to catch (a construct present in
    /// production content but never proven against by any test).
    #[test]
    fn construct_coverage_audit_is_falsifiable_when_a_real_category_is_removed() {
        let root = repo_root();
        let dict = load_lang_dictionary(&root).expect("dictionary loads");
        let sources = collect_grounding_sources(&root).expect("collect sources");

        let mut full_tally = ConstructCoverageTally::default();
        let mut filtered_quads = Vec::new();
        for source in &sources {
            let bytes = std::fs::read(root.join(source)).expect("read source");
            let ds = parse_dataset(&bytes, "text/turtle", None).expect("parse source");
            let model = Gmn0Model::from_dataset(&ds);
            full_tally.absorb(&model, &dict);
            for q in &model.quads {
                let hits_decimal = matches!(
                    gmeow_lang_bridge::classify_quad(q, &dict),
                    gmeow_lang_bridge::QuadCoverage::Covered {
                        subject,
                        predicate,
                        object,
                    } if subject == Gmn1ConstructCategory::LiteralDecimal
                        || predicate == Gmn1ConstructCategory::LiteralDecimal
                        || object == Gmn1ConstructCategory::LiteralDecimal
                );
                if !hits_decimal {
                    filtered_quads.push(q.clone());
                }
            }
        }

        // Sanity: the real corpus DOES exercise LiteralDecimal — the negative control is
        // only meaningful if the category it removes was genuinely present beforehand.
        assert!(
            full_tally.count(Gmn1ConstructCategory::LiteralDecimal) > 0,
            "sanity: the real grounding corpus must exercise LiteralDecimal for this \
             negative control to prove anything; the corpus changed — pick a different \
             sparse category to filter"
        );
        assert!(
            !filtered_quads.is_empty(),
            "sanity: filtering must not remove the whole corpus"
        );

        let filtered_model = Gmn0Model {
            quads: filtered_quads,
        };
        let mut filtered_tally = ConstructCoverageTally::default();
        filtered_tally.absorb(&filtered_model, &dict);

        assert_eq!(
            filtered_tally.count(Gmn1ConstructCategory::LiteralDecimal),
            0,
            "filtering out every quad that hits LiteralDecimal must leave zero \
             occurrences in the filtered corpus"
        );
        assert!(
            filtered_tally
                .unexercised_categories()
                .contains(&Gmn1ConstructCategory::LiteralDecimal),
            "the completeness audit must flag LiteralDecimal as unexercised once its \
             only real occurrence is removed — proof the assertion is falsifiable, not a \
             vacuous pass: {:#?}",
            filtered_tally.unexercised_categories()
        );
    }

    #[test]
    fn report_is_clean_iff_no_failures() {
        let clean = Gmn1RoundTripReport::default();
        assert!(clean.is_clean());
        let dirty = Gmn1RoundTripReport {
            failures: vec![Gmn1RoundTripFailure {
                path: "x".to_owned(),
                error: Gmn1Error::NonDecodableGrammar {
                    detail: "y".to_owned(),
                },
            }],
        };
        assert!(!dirty.is_clean());
    }

    /// The production shipped-projection lint reads every committed
    /// `generated/projections/lang/gmn1/*.gmn` back through the production codec and asserts
    /// `failure_class()` is clean — a genuine production caller of the canonical classifier
    /// over shipped artifacts (not merely a test-only round-trip).
    #[test]
    fn shipped_gmn1_projections_all_read_clean() {
        let root = repo_root();
        let report = check_gmn1_shipped_projections(&root)
            .expect("shipped-projection lint runs without a hard I/O error");
        assert!(
            report.is_clean(),
            "every shipped GMN-1 projection must read back clean through the production codec, \
             but these failed: {:#?}",
            report.failures
        );
        assert!(
            report.verified > 0,
            "the lint must actually have exercised shipped projections (the repo ships \
             generated/projections/lang/gmn1/*.gmn), not vacuously pass on an empty set"
        );
    }
}
