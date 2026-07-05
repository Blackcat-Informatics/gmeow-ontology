// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The Lane-B `lang:` **projection** corpus producer: lower the canonical `lang:` model
//! out to the external linguistic ecosystems (OntoLex-Lemon, CoNLL-U, EBNF, ABNF) through
//! the correspondence-carrying [`registry`], and fold every emission's honest preservation
//! judgment into BOTH the loss ledger and a queryable `lang:ProjectionEmission` corpus.
//!
//! # The projection contract, enforced per emission
//!
//! Each target CARRIES a `logic:Correspondence`; the driver never asks a target for its
//! preservation. It DERIVES the kind from the carried correspondence
//! ([`is_exact_correspondence`] → `Exact`, else the emission's declared lossy kind) and
//! MEASURES the round-trip (the target re-parses / byte-round-trips; the driver
//! cross-checks the structural [`exact_round_trip_holds`] over the carried leg pair). Five
//! structural invariants turn a dishonest emission into a red build, each message naming
//! the Task-1 failure class it enforces:
//!
//! 1. **Round-trip measured for every target** — an `Exact`-deriving emission whose measured
//!    round-trip is false is a hard fail citing `lang:ExactPreservationViolated`.
//! 2. **Overclaim floor** — every folded [`ProjectionResult`] routes through
//!    [`assert_no_overclaim`]; an Exact overclaim or an Unsupported empty-residue is red.
//! 3. **CoNLL-U no-silent-winner** — a per-reading emission MUST emit one artifact per
//!    co-resident reading; fewer is `lang:ProjectionSilentDisambiguation`.
//! 4. **Registry completeness (functor totality)** — every emission-worthy `lang:` class
//!    maps to ≥1 registered target ([`assert_registry_covers`]); a gap is red.
//! 5. **Exact-negative teeth** — the bridges' own tests perturb an object and prove
//!    exactness FAILS, so "Exact" is falsifiable (see `crates/lang-bridge/tests`).
//!
//! Every emission is emitted BOTH as RDF (a `lang:ProjectionEmission` record plus the
//! lifted `lang:Grammar`/… source structure, into the carrier's
//! `graph/lang-projection-corpus` named graph) AND as `ProjectionResult` rows folded into
//! the loss ledger. All identities are content-addressed and the N-Triples are sorted +
//! deduped, so the corpus is byte-reproducible (no clock, no randomness).
//!
//! # Sources
//!
//! The authored `slices/grounding/lang/grammars/*.ebnf` are the grammar SOURCE surface
//! (the `.po` analogue for grammars); each is lifted and re-emitted to EBNF and (where the
//! canonical grammar is ABNF-expressible) ABNF. The OntoLex-Lemon and CoNLL-U targets are
//! registered but drive from OntoLex-Lemon lexicon / CoNLL-U treebank source surfaces the
//! composed non-test model does not carry, so each folds ONE honest no-source ledger row —
//! exactly as `lang_form` was empty until data appeared. The bridges themselves are fully
//! exercised (round-trip + teeth) by fixtures in `crates/lang-bridge/tests`.

use purrdf::slice::SliceCatalog;

use gmeow_lang_bridge::registry::{
    assert_registry_covers, registry, ConlluSource, LangEmission, LangProjectionInput, NamedSource,
    EMISSION_WORTHY_CLASSES,
};
use gmeow_lang_bridge::{exact_round_trip_holds, is_exact_correspondence, ntriples_sorted};
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::projections::{assert_no_overclaim, ProjectionResult};

use crate::error::PipelineError;

const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
/// The example-instance base every minted `lang:ProjectionEmission` IRI lives under, the
/// same base every other `lang:` producer content-addresses its individuals under.
const EXAMPLE_BASE: &str = "http://example.org/lang/";
/// The logical-path prefix under which every generated external projection artifact lives.
pub const LANG_PROJECTION_DIR: &str = "generated/projections/lang";

/// The assembled projection corpus: the sorted, byte-stable N-Triples graph
/// (`graph/lang-projection-corpus`), the per-emission loss-ledger rows, and the generated
/// external artifacts keyed by their `generated/projections/lang/<target>/<name>` path.
pub struct LangProjectionCorpus {
    /// The deterministic, sorted N-Triples graph of `lang:ProjectionEmission` records plus
    /// the lifted source `lang:` structure they project.
    pub ntriples: Vec<u8>,
    /// One or more `ProjectionResult` rows per emission (plus one honest no-source row per
    /// target with no source in the composed model).
    pub ledger: Vec<ProjectionResult>,
    /// The generated external projection files, keyed by their committed logical path.
    pub artifacts: Vec<(String, Vec<u8>)>,
}

/// Build the projection corpus by driving every registered [`LangProjectionTarget`] over
/// the sources the shared in-memory [`SliceCatalog`] carries. `None` (no `slices/` tree)
/// yields the empty-input corpus (every target folds its honest no-source row).
pub fn build_corpus(catalog: Option<&SliceCatalog>) -> Result<LangProjectionCorpus, PipelineError> {
    // Functor totality (Invariant 4): every emission-worthy class must map to a registered
    // target BEFORE any emission runs — a gap is a hard fail, not a silent omission.
    for (class, _) in EMISSION_WORTHY_CLASSES {
        assert_registry_covers(class).map_err(stage_err)?;
    }

    let input = collect_input(catalog)?;

    let mut lines: Vec<String> = Vec::new();
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    let mut artifacts: Vec<(String, Vec<u8>)> = Vec::new();

    for target in registry() {
        let name = target.name();
        let emissions = target.emit(&input).map_err(|d| {
            stage_err(format!(
                "lang-projection target '{name}' hard-failed ({}): {}",
                d.failure_class.as_str(),
                d.construct
            ))
        })?;

        if emissions.is_empty() {
            // Honest no-source row: the target is registered but the composed model carries
            // no source it lowers FROM — vacuously exact (nothing projected, nothing
            // dropped), exactly like the initially-empty prose-lift corpus.
            ledger.push(no_source_row(name));
            continue;
        }

        for emission in emissions {
            let derived = derived_kind(&emission);
            enforce_invariants(name, &emission, derived)?;

            // Overclaim floor (Invariant 2): route every folded ledger row through the
            // shared gate before it enters the ledger.
            for row in &emission.ledger {
                let residue: Vec<&str> = row
                    .lossy_drops
                    .iter()
                    .chain(row.actual_drops.iter())
                    .map(String::as_str)
                    .collect();
                assert_no_overclaim(&row.target, row.preservation, &residue)
                    .map_err(|e| stage_err(e.to_string()))?;
                ledger.push(row.clone());
            }

            // The lifted source `lang:` structure (grammar RDF, …) rides into the corpus.
            for line in String::from_utf8_lossy(&emission.source_rdf).lines() {
                if !line.trim().is_empty() {
                    lines.push(line.to_owned());
                }
            }

            // The honest `lang:ProjectionEmission` record — the queryable loss judgment.
            emit_projection_record(&mut lines, name, &emission, derived);

            for artifact in &emission.artifacts {
                artifacts.push((
                    format!("{LANG_PROJECTION_DIR}/{}", artifact.path_suffix),
                    artifact.bytes.clone(),
                ));
            }
        }
    }

    artifacts.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(LangProjectionCorpus {
        ntriples: ntriples_sorted(lines),
        ledger,
        artifacts,
    })
}

/// The preservation kind DERIVED from the carried correspondence (never declared by the
/// target): an exact correspondence yields [`PreservationKind::Exact`], else the emission's
/// declared lossy kind.
fn derived_kind(emission: &LangEmission) -> PreservationKind {
    if is_exact_correspondence(&emission.correspondence) {
        PreservationKind::Exact
    } else {
        emission.lossy_kind
    }
}

/// Enforce the round-trip (Invariant 1) and CoNLL-U no-silent-winner (Invariant 3)
/// structural invariants over one emission.
fn enforce_invariants(
    target: &str,
    emission: &LangEmission,
    derived: PreservationKind,
) -> Result<(), PipelineError> {
    // Invariant 1: an emission that DERIVES Exact must actually round-trip (measured), and
    // its carried leg pair must be the structural inverse.
    if derived == PreservationKind::Exact {
        if !emission.round_trip_holds {
            return Err(stage_err(format!(
                "lang:ExactPreservationViolated: projection target '{target}' derives \
                 logic:ExactPreservation from its carried correspondence, but the measured \
                 round-trip is false for source <{}>",
                emission.source_iri
            )));
        }
        match &emission.leg_pair {
            Some((get, put)) => {
                if !exact_round_trip_holds(get, put) {
                    return Err(stage_err(format!(
                        "lang:ExactPreservationViolated: projection target '{target}' claims \
                         Exact but its carried put leg is not the structural inverse of the get \
                         leg (put ∘ get ≠ id) for source <{}>",
                        emission.source_iri
                    )));
                }
            }
            None => {
                return Err(stage_err(format!(
                    "lang:ExactPreservationViolated: projection target '{target}' derives Exact \
                     but carries no leg pair to decide the round-trip for source <{}>",
                    emission.source_iri
                )));
            }
        }
    }

    // Invariant 3: a per-reading emission emits ONE artifact per co-resident reading — never
    // a single silently-chosen winner.
    if let Some(count) = emission.emitted_reading_count {
        if emission.artifacts.len() as u64 != count {
            return Err(stage_err(format!(
                "lang:ProjectionSilentDisambiguation: per-reading target '{target}' declares \
                 {count} co-resident reading(s) but emitted {} artifact(s) for source <{}>; a \
                 per-reading projection never collapses readings to a single winner",
                emission.artifacts.len(),
                emission.source_iri
            )));
        }
    }
    Ok(())
}

/// Serialize one honest `lang:ProjectionEmission` record — the queryable loss judgment the
/// Task-1 native gates run over. Carries the target name, the projected source, the DERIVED
/// `logic:preservationKind`, each enumerated unsupported construct, the reading count (for a
/// per-reading target), and the MEASURED round-trip verdict.
fn emit_projection_record(
    lines: &mut Vec<String>,
    target: &str,
    emission: &LangEmission,
    derived: PreservationKind,
) {
    let emission_iri = example(
        "projection-emission",
        &digest16(
            "lang-projection-emission",
            &format!("{target}\u{1f}{}", emission.source_iri),
        ),
    );
    lines.push(triple(
        &emission_iri,
        RDF_TYPE,
        &iri(LANG_NS, "ProjectionEmission"),
    ));
    lines.push(triple_lit(
        &emission_iri,
        &iri(LANG_NS, "projectionTargetName"),
        target,
    ));
    lines.push(triple(
        &emission_iri,
        &iri(LANG_NS, "projectsSource"),
        &emission.source_iri,
    ));
    lines.push(triple(
        &emission_iri,
        &iri(LOGIC_NS, "preservationKind"),
        &derived.iri(),
    ));
    for construct in &emission.unsupported {
        lines.push(triple_lit(
            &emission_iri,
            &iri(LANG_NS, "unsupportedConstruct"),
            construct,
        ));
    }
    if let Some(count) = emission.emitted_reading_count {
        lines.push(triple_typed(
            &emission_iri,
            &iri(LANG_NS, "emittedReadingCount"),
            &count.to_string(),
            XSD_NON_NEGATIVE_INTEGER,
        ));
    }
    lines.push(triple_typed(
        &emission_iri,
        &iri(LANG_NS, "roundTripHolds"),
        if emission.round_trip_holds {
            "true"
        } else {
            "false"
        },
        XSD_BOOLEAN,
    ));
}

/// The honest no-source ledger row for a registered target the composed model carries no
/// source for: vacuously exact (nothing projected, nothing dropped) so the overclaim gate
/// accepts it, with a descriptive content note.
fn no_source_row(target: &str) -> ProjectionResult {
    ProjectionResult {
        target: format!("lang-projection:{target}"),
        content: format!(
            "no {target} projection source in the composed model; target registered, nothing \
             projected"
        ),
        is_rdf: false,
        preservation: PreservationKind::Exact,
        complexity: "n/a".to_owned(),
        lossy_drops: Vec::new(),
        actual_drops: Vec::new(),
    }
}

/// Collect the projection input aBox from the shared source catalog: every authored
/// `*.ebnf` grammar surface (the grammar SOURCE surface). OntoLex-Lemon lexicons and
/// CoNLL-U treebanks are not authored as source artifacts in the composed model, so those
/// input slices are empty (each target folds its honest no-source row).
fn collect_input(catalog: Option<&SliceCatalog>) -> Result<LangProjectionInput, PipelineError> {
    let mut grammars: Vec<NamedSource> = Vec::new();
    if let Some(catalog) = catalog {
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if !artifact.logical_path.ends_with(".ebnf") {
                    continue;
                }
                let name = grammar_stem(&artifact.logical_path);
                grammars.push(NamedSource {
                    name,
                    bytes: artifact.content.clone(),
                });
            }
        }
    }
    // Deterministic source order (independent of catalog discovery order).
    grammars.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(LangProjectionInput {
        grammars,
        lexicons: Vec::new(),
        treebanks: Vec::<ConlluSource>::new(),
    })
}

/// The grammar source stem (the file basename without its `.ebnf` extension), used as the
/// emitted artifact name.
fn grammar_stem(logical_path: &str) -> String {
    logical_path
        .rsplit('/')
        .next()
        .unwrap_or(logical_path)
        .strip_suffix(".ebnf")
        .unwrap_or(logical_path)
        .to_owned()
}

// ── N-Triples helpers (mirroring the sibling lang: producers) ──────────────────────

fn stage_err(message: impl Into<String>) -> PipelineError {
    PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: message.into(),
    }
}

fn iri(ns: &str, local: &str) -> String {
    format!("{ns}{local}")
}

fn example(segment: &str, id: &str) -> String {
    format!("{EXAMPLE_BASE}{segment}/{id}")
}

/// A stable 16-hex-char content address (byte-identical to the shared `lang:` digest).
fn digest16(domain: &str, key: &str) -> String {
    gmeow_lang_bridge::digest16(domain, key)
}

fn triple(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> <{object}> .")
}

fn triple_lit(subject: &str, predicate: &str, literal: &str) -> String {
    format!("<{subject}> <{predicate}> {} .", nt_literal(literal))
}

fn triple_typed(subject: &str, predicate: &str, literal: &str, datatype: &str) -> String {
    format!(
        "<{subject}> <{predicate}> {}^^<{datatype}> .",
        nt_literal(literal)
    )
}

/// Escape a string as an N-Triples quoted literal (UTF-8 passes through verbatim).
fn nt_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn repo_catalog() -> SliceCatalog {
        SliceCatalog::discover(
            &repo_root().join("slices"),
            crate::gmeow_ns::gmeow_slice_vocab(),
        )
        .expect("discover slice catalog")
    }

    #[test]
    fn grammars_drive_real_ebnf_emissions_with_exact_round_trip() {
        let catalog = repo_catalog();
        let corpus = build_corpus(Some(&catalog)).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples.clone()).expect("utf8");

        // The authored .ebnf grammars lift and re-emit as generated EBNF projection files.
        assert!(
            corpus
                .artifacts
                .iter()
                .any(|(p, _)| p.starts_with("generated/projections/lang/ebnf/")),
            "the authored grammars must drive EBNF projection artifacts"
        );
        // Each grammar surfaces a lang:Grammar and a lang:ProjectionEmission carrying an
        // Exact preservation kind and a measured round-trip.
        assert!(nt.contains(&iri(LANG_NS, "Grammar")));
        assert!(nt.contains(&iri(LANG_NS, "ProjectionEmission")));
        assert!(nt.contains(&PreservationKind::Exact.iri()));
        assert!(nt.contains(&iri(LANG_NS, "roundTripHolds")));
        // Exact grammar emissions round-trip: no lang:roundTripHolds "false" on an EBNF
        // projection subject (an Exact-with-false record would have hard-failed the build).
        assert!(nt.contains("\"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>"));
    }

    #[test]
    fn ontolex_and_conllu_fold_honest_no_source_rows() {
        let catalog = repo_catalog();
        let corpus = build_corpus(Some(&catalog)).expect("build corpus");
        for target in ["ontolex-lemon", "conllu"] {
            assert!(
                corpus
                    .ledger
                    .iter()
                    .any(|r| r.target == format!("lang-projection:{target}")),
                "target '{target}' must fold an honest no-source ledger row"
            );
        }
    }

    #[test]
    fn every_ledger_row_satisfies_the_overclaim_floor() {
        let catalog = repo_catalog();
        let corpus = build_corpus(Some(&catalog)).expect("build corpus");
        for row in &corpus.ledger {
            let residue: Vec<&str> = row
                .lossy_drops
                .iter()
                .chain(row.actual_drops.iter())
                .map(String::as_str)
                .collect();
            assert_no_overclaim(&row.target, row.preservation, &residue)
                .unwrap_or_else(|e| panic!("overclaim floor violated: {e}"));
        }
    }

    #[test]
    fn corpus_is_byte_reproducible() {
        let catalog = repo_catalog();
        let a = build_corpus(Some(&catalog)).expect("a").ntriples;
        let b = build_corpus(Some(&catalog)).expect("b").ntriples;
        assert_eq!(a, b, "the projection corpus must be deterministic");
    }

    #[test]
    fn registry_completeness_covers_every_emission_worthy_class() {
        // Functor totality: every documented emission-worthy class resolves to a registered
        // target, and an unlisted class hard-fails.
        for (class, _) in EMISSION_WORTHY_CLASSES {
            assert_registry_covers(class).expect("registered class must be covered");
        }
        assert!(assert_registry_covers("NotAClass").is_err());
    }

    /// A synthetic co-resident CoNLL-U source with two readings must emit two artifacts —
    /// never a single silently-chosen winner (the saw-her-duck discipline at the projection
    /// seam).
    #[test]
    fn conllu_two_readings_emit_two_artifacts_never_one() {
        // "saw her duck" — reading A (bird / nominal head), reading B (crouch / verbal head).
        let reading_a = "1\tsaw\tsee\tVERB\t_\t_\t0\troot\t_\t_\n\
                         2\ther\ther\tPRON\t_\t_\t1\tnsubj\t_\t_\n\
                         3\tduck\tduck\tNOUN\t_\t_\t1\tobj\t_\t_\n\n";
        let reading_b = "1\tsaw\tsee\tVERB\t_\t_\t0\troot\t_\t_\n\
                         2\ther\ther\tPRON\t_\t_\t3\tnsubj\t_\t_\n\
                         3\tduck\tduck\tVERB\t_\t_\t1\txcomp\t_\t_\n\n";
        let input = LangProjectionInput {
            grammars: Vec::new(),
            lexicons: Vec::new(),
            treebanks: vec![ConlluSource {
                name: "saw-her-duck".to_owned(),
                readings: vec![reading_a.as_bytes().to_vec(), reading_b.as_bytes().to_vec()],
            }],
        };
        let conllu = registry()
            .into_iter()
            .find(|t| t.name() == "conllu")
            .expect("conllu target registered");
        let emissions = conllu.emit(&input).expect("emit");
        assert_eq!(emissions.len(), 1, "one emission per treebank source");
        let e = &emissions[0];
        assert_eq!(e.emitted_reading_count, Some(2));
        assert_eq!(
            e.artifacts.len(),
            2,
            "two co-resident readings must emit two CoNLL-U artifacts, never one"
        );
        // The driver invariant accepts the honest 2-of-2 emission…
        enforce_invariants("conllu", e, derived_kind(e)).expect("2==2 passes");
        // …and hard-fails a collapsed single-winner emission (silent disambiguation).
        let mut collapsed = e.clone();
        collapsed.artifacts.truncate(1);
        let err = enforce_invariants("conllu", &collapsed, derived_kind(&collapsed))
            .expect_err("1-of-2 must hard-fail");
        assert!(format!("{err}").contains("lang:ProjectionSilentDisambiguation"));
    }
}
