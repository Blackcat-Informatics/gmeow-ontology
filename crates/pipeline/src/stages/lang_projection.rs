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
//! Every projection here is FORWARD — from the canonical `lang:` model out to an external
//! ecosystem (the backward ingestion leg lives in the runtime bridges). The authored
//! `slices/grounding/lang/grammars/*.ebnf` are the grammar SOURCE surface, lifted and
//! re-emitted to EBNF and (where ABNF-expressible) ABNF. The OntoLex-Lemon, CoNLL-U, TEI,
//! NIF, and SemAF targets lower the `lang:` A-box carried by the shipped `examples/*.ttl`:
//! `lang:Lexeme`/`lang:Sense` to OntoLex, analyzed `lang:ComposedForm` to CoNLL-U, and so on.
//! A target whose individuals the composed model does not carry folds ONE honest no-source
//! ledger row (as `lang_form` was empty until data appeared).

use purrdf::slice::SliceCatalog;

use gmeow_lang_bridge::registry::{
    EMISSION_WORTHY_CLASSES, LangEmission, LangProjectionInput, NamedSource,
    assert_registry_covers, registry,
};
use gmeow_lang_bridge::{
    CurrentCodebook, GmnDictionary, exact_round_trip_holds, is_exact_correspondence,
    ntriples_sorted, resolve_current_codebook, resolve_dialect_acceptance, resolve_operator_forms,
};
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::{ProjectionResult, assert_no_overclaim};

use gmeow_ns::LANG_NS;
use gmeow_ns::LOGIC_NS;
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
    /// target with no source in the composed model). The rows carry only identity/judgment;
    /// their drops live in [`loss`](Self::loss).
    pub ledger: Vec<ProjectionResult>,
    /// The loss store every emission's drops are unioned into (keyed by target focus). The
    /// mappings stage unions it into the single report loss store.
    pub loss: LossLedger,
    /// The generated external projection files, keyed by their committed logical path.
    pub artifacts: Vec<(String, Vec<u8>)>,
}

/// Build the projection corpus by driving every registered
/// [`gmeow_lang_bridge::LangProjectionTarget`] over
/// the sources the shared in-memory [`SliceCatalog`] carries. `None` (no `slices/` tree)
/// yields the empty-input corpus (every target folds its honest no-source row).
pub fn build_corpus(
    catalog: Option<&SliceCatalog>,
) -> Result<LangProjectionCorpus, gmeow_errors::Diag> {
    // Functor totality (Invariant 4): every emission-worthy class must map to a registered
    // target BEFORE any emission runs — a gap is a hard fail, not a silent omission.
    for (class, _) in EMISSION_WORTHY_CLASSES {
        assert_registry_covers(class)?;
    }

    let input = collect_input(catalog)?;

    let mut lines: Vec<String> = Vec::new();
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    let mut loss = LossLedger::new();
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
            ledger.push(no_source_row(name, &mut loss));
            continue;
        }

        for emission in emissions {
            let derived = derived_kind(&emission);
            enforce_invariants(name, &emission, derived)?;

            // Fold this emission's loss store into the corpus store, then route every folded
            // ledger row through the shared overclaim gate (Invariant 2) reading its residue
            // back from the SAME store — never the (now drop-less) `ProjectionResult`.
            loss.union(&emission.loss);
            for row in &emission.ledger {
                let residue_owned = emission.loss.projection_drops_for(&row.target);
                let residue: Vec<&str> = residue_owned.iter().map(String::as_str).collect();
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
        loss,
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
) -> Result<(), gmeow_errors::Diag> {
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
    if let Some(count) = emission.emitted_reading_count
        && emission.artifacts.len() as u64 != count
    {
        return Err(stage_err(format!(
            "lang:ProjectionSilentDisambiguation: per-reading target '{target}' declares \
             {count} co-resident reading(s) but emitted {} artifact(s) for source <{}>; a \
             per-reading projection never collapses readings to a single winner",
            emission.artifacts.len(),
            emission.source_iri
        )));
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
fn no_source_row(target: &str, loss: &mut LossLedger) -> ProjectionResult {
    let row_target = format!("lang-projection:{target}");
    // Vacuously exact: nothing projected, nothing dropped (interns no drops).
    loss.record_projection_drops(&row_target, PreservationKind::Exact, &[], &[]);
    ProjectionResult {
        target: row_target,
        content: format!(
            "no {target} projection source in the composed model; target registered, nothing \
             projected"
        ),
        is_rdf: false,
        preservation: PreservationKind::Exact,
        complexity: "n/a".to_owned(),
    }
}

/// The lang-model sources the GMN-1 (and OntoLex/CoNLL-U/…) targets lower FROM — the exact
/// `lang_models` set [`collect_input`] feeds the registry. Exposed so the on-gate
/// shipped-projection lint (`crates/pipeline/src/stages/gmn1_gate.rs`) reconstructs each
/// shipped `gmn1/*.gmn` document from the SAME sources the projection stage projected, never a
/// second, drift-prone source enumeration.
pub(crate) fn lang_model_sources(
    catalog: Option<&SliceCatalog>,
) -> Result<Vec<NamedSource>, gmeow_errors::Diag> {
    Ok(collect_input(catalog)?.lang_models)
}

/// Collect the projection input aBox from the shared source catalog: every authored
/// `*.ebnf` grammar surface (the grammar SOURCE surface), and every lang-bearing
/// `examples/*.ttl` (the `lang:` A-box the OntoLex/CoNLL-U/TEI/NIF/SemAF targets lower FROM,
/// and — with the lang module surface — the BCP-47 target's variety scan).
fn collect_input(
    catalog: Option<&SliceCatalog>,
) -> Result<LangProjectionInput, gmeow_errors::Diag> {
    let mut grammars: Vec<NamedSource> = Vec::new();
    let mut lang_models: Vec<NamedSource> = Vec::new();
    let mut varieties: Vec<NamedSource> = Vec::new();
    let mut gmn_dictionary: Option<GmnDictionary> = None;
    let mut gmn_codebook: Option<CurrentCodebook> = None;
    let mut gmn_dialect_major: Option<String> = None;
    // The grounding-slice module surfaces (lang/logic/math) whose `rdfs:label`s name the GMN
    // denotation targets the verbalizer renders — captured here and parsed once after the loop
    // into the shared label index, so the bundle-level verbalizer resolves a term's controlled-NL
    // nucleus from the graph rather than from any local-name convention.
    let mut grounding_modules: Vec<Vec<u8>> = Vec::new();
    if let Some(catalog) = catalog {
        for record in catalog.records() {
            let record_is_grounding = is_grounding_slice(&record.slice_dir);
            for artifact in &record.artifacts {
                if record_is_grounding && artifact.logical_path == "module.ttl" {
                    grounding_modules.push(artifact.content.clone());
                }
                if artifact.logical_path.ends_with(".ebnf") {
                    grammars.push(NamedSource {
                        name: grammar_stem(&artifact.logical_path),
                        bytes: artifact.content.clone(),
                    });
                    continue;
                }
                // The `lang:` RDF surfaces the document/surface/meaning targets (TEI, NIF, SemAF)
                // lower FROM: the shipped `examples/*.ttl` that reference the lang: namespace. The
                // namespace check scopes the scan to lang-bearing sources (a non-lang example is
                // not fed to a lang bridge, so it never hard-fails the projection). A lang-bearing
                // example ALSO feeds the BCP-47 target's variety scan.
                if artifact.logical_path.starts_with("examples/")
                    && artifact.logical_path.ends_with(".ttl")
                    && contains_lang_namespace(&artifact.content)
                {
                    let source = NamedSource {
                        name: lang_model_stem(&artifact.logical_path),
                        bytes: artifact.content.clone(),
                    };
                    lang_models.push(source.clone());
                    varieties.push(source);
                    continue;
                }
                // The lang module's own vocabulary surface carries the framework's
                // `lang:LanguageVariety` individuals (the GMEOW English carrier). It feeds ONLY the
                // BCP-47 variety scan — never the document/surface/meaning bridges, whose targets
                // it carries no instances of. The namespace check scopes this to the lang module.
                if artifact.logical_path == "module.ttl"
                    && contains_lang_namespace(&artifact.content)
                {
                    varieties.push(NamedSource {
                        name: variety_module_stem(&record.slice_dir),
                        bytes: artifact.content.clone(),
                    });
                    if record.slice_dir.file_name().and_then(|name| name.to_str()) == Some("lang") {
                        let dataset = purrdf::parse_dataset(&artifact.content, "text/turtle", None)
                            .map_err(|error| {
                                stage_err(format!(
                                    "parse grounding/lang module for the GMN codebook: {error}"
                                ))
                            })?;
                        gmn_dictionary =
                            Some(GmnDictionary::from_dataset(&dataset).map_err(|error| {
                                stage_err(format!("load grounding/lang GMN codebook: {}", error.0))
                            })?);
                        // The resolved codebook is the second carrier of codebook identity the
                        // conformance pack's digest folds over (alongside the dictionary) — read
                        // from the SAME dataset so the emitted digest equals the gate/CLI recompute.
                        gmn_codebook =
                            Some(resolve_current_codebook(&dataset).map_err(|error| {
                                stage_err(format!(
                                    "resolve grounding/lang current GMN codebook: {}",
                                    error.0
                                ))
                            })?);
                        // The dialect major that keys every emitted GMN artifact path is
                        // RESOLVED FROM THE GRAPH (the gmeow:gmnDialectVersions lineage's
                        // roleLatest member) — read from the SAME dataset as the codebook, so
                        // the projection subtree and the codec's header pin one lineage. The
                        // shipped lang module always carries the lineage; its absence is a hard
                        // fail (no-optionality), never a constant default.
                        gmn_dialect_major = Some(
                            resolve_dialect_acceptance(&dataset)
                                .map_err(|error| {
                                    stage_err(format!(
                                        "resolve grounding/lang GMN dialect version lineage: {}",
                                        error.0
                                    ))
                                })?
                                .ok_or_else(|| {
                                    stage_err(
                                        "grounding/lang module carries no gmeow:gmnDialectVersions \
                                         lineage; the version-keyed GMN projection cannot default"
                                            .to_owned(),
                                    )
                                })?
                                .latest_major_key(),
                        );
                    }
                }
            }
        }
    }
    // Capture the AUTHORED GMN grammar (`grammars/gmn.ebnf`, pre-render) — the conformance
    // pack pins the authored template as its grammar leaf, NOT the graph-rendered derivative
    // the render loop below substitutes into `grammars`.
    let gmn_grammar_source = grammars
        .iter()
        .find(|grammar| grammar.name == "gmn")
        .map(|grammar| grammar.bytes.clone());
    if let Some(dictionary) = &gmn_dictionary {
        for grammar in &mut grammars {
            if grammar.name == "gmn" {
                grammar.bytes = dictionary
                    .glyph_registry()
                    .render_grammar(&grammar.bytes)
                    .map_err(|error| {
                        stage_err(format!(
                            "render GMN glyph grammar from the carrier registry: {}",
                            error.0
                        ))
                    })?;
            }
        }
    }
    // Resolve the verbalizable GMN operator inventory: the carrier glyph registry's operator
    // bindings joined to their denotation targets' `rdfs:label`s (harvested from the grounding
    // modules). A non-injective inventory hard-fails downstream in the bundle emission; here a
    // missing label for a selected operator is a HARD FAIL (no-optionality), never a silent skip.
    let gmn_operator_forms = if let Some(dictionary) = &gmn_dictionary {
        let labels = harvest_labels(&grounding_modules)?;
        resolve_operator_forms(dictionary.glyph_registry(), &labels).map_err(|error| {
            stage_err(format!(
                "resolve GMN verbalizable operator forms from the carrier registry: {error}"
            ))
        })?
    } else {
        Vec::new()
    };

    // Deterministic source order (independent of catalog discovery order).
    grammars.sort_by(|a, b| a.name.cmp(&b.name));
    lang_models.sort_by(|a, b| a.name.cmp(&b.name));
    varieties.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(LangProjectionInput {
        grammars,
        lang_models,
        varieties,
        gmn_dictionary,
        gmn_codebook,
        gmn_grammar_source,
        gmn_dialect_major,
        gmn_operator_forms,
    })
}

/// Whether a slice directory lives under the `grounding/` tree (lang/logic/math) — the source
/// of the GMN denotation targets' `rdfs:label`s the verbalizer renders.
fn is_grounding_slice(slice_dir: &std::path::Path) -> bool {
    slice_dir.components().any(|c| c.as_os_str() == "grounding")
}

/// Harvest the `rdfs:label` index (`IRI → label`) from the grounding module surfaces. When an
/// IRI carries several labels, the GMEOW-English one (`@x-gmeow-english`) wins; ties break to
/// the lexicographically smallest lexical form, so the index is deterministic across runs.
fn harvest_labels(
    modules: &[Vec<u8>],
) -> Result<std::collections::BTreeMap<String, String>, gmeow_errors::Diag> {
    use purrdf::RdfTerm;
    const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
    const GMEOW_ENGLISH: &str = "x-gmeow-english";
    // (is_gmeow_english, lexical_form) per IRI — preference order for the deterministic pick.
    let mut best: std::collections::BTreeMap<String, (bool, String)> =
        std::collections::BTreeMap::new();
    for module in modules {
        let dataset = purrdf::parse_dataset(module, "text/turtle", None).map_err(|error| {
            stage_err(format!(
                "parse grounding module for verbalizer labels: {error}"
            ))
        })?;
        for quad in dataset.owned_quads() {
            if quad.predicate != RDFS_LABEL {
                continue;
            }
            let RdfTerm::Iri(subject) = &quad.subject else {
                continue;
            };
            let RdfTerm::Literal(literal) = &quad.object else {
                continue;
            };
            let is_english = literal.language.as_deref() == Some(GMEOW_ENGLISH);
            let candidate = (is_english, literal.lexical_form.clone());
            match best.get(subject) {
                Some((cur_english, cur_lex)) => {
                    // Prefer the GMEOW-English label; among equals, the smallest lexical form.
                    let better = (candidate.0, std::cmp::Reverse(candidate.1.clone()))
                        > (*cur_english, std::cmp::Reverse(cur_lex.clone()));
                    if better {
                        best.insert(subject.clone(), candidate);
                    }
                }
                None => {
                    best.insert(subject.clone(), candidate);
                }
            }
        }
    }
    Ok(best.into_iter().map(|(k, (_, v))| (k, v)).collect())
}

/// Whether a source references the `lang:` namespace — the cheap scope filter that keeps the
/// TEI/NIF/SemAF scan to lang-bearing examples and never feeds an unrelated example to a lang
/// bridge (which would hard-fail the projection).
fn contains_lang_namespace(content: &[u8]) -> bool {
    const LANG_NS_BYTES: &[u8] = b"blackcatinformatics.ca/lang/";
    content
        .windows(LANG_NS_BYTES.len())
        .any(|w| w == LANG_NS_BYTES)
}

/// The `lang:` model source stem (the file basename without its `.ttl` extension).
fn lang_model_stem(logical_path: &str) -> String {
    logical_path
        .rsplit('/')
        .next()
        .unwrap_or(logical_path)
        .strip_suffix(".ttl")
        .unwrap_or(logical_path)
        .to_owned()
}

/// The variety module source name: the owning slice's directory name suffixed `-module`, so a
/// module surface is named stably by its slice (e.g. `lang-module`) without a substring scan.
fn variety_module_stem(slice_dir: &std::path::Path) -> String {
    let slice = slice_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("slice");
    format!("{slice}-module")
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

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-mappings".to_string(),
        message: message.into(),
    })
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
