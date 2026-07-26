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
    fn gmn_projection_uses_carrier_glyphs_and_graph_derived_grammar() {
        let catalog = repo_catalog();
        let input = collect_input(Some(&catalog)).expect("collect projection input");
        let dictionary = input
            .gmn_dictionary
            .as_ref()
            .expect("grounding/lang supplies the one carrier GMN dictionary");
        let expected_production = dictionary.glyph_registry().render_glyph_token_production();
        let corpus = build_corpus(Some(&catalog)).expect("build projection corpus");

        let grammar = corpus
            .artifacts
            .iter()
            .find(|(path, _)| path == "generated/projections/lang/ebnf/gmn.ebnf")
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .expect("the generated GMN EBNF artifact exists");
        assert!(
            grammar.lines().any(|line| line == expected_production),
            "generated grammar does not carry the graph-derived closed production:\n{grammar}"
        );

        // The GMN artifacts are keyed under the graph-resolved dialect major (gmn1/v<major>/…).
        let major = input
            .gmn_dialect_major
            .as_deref()
            .expect("grounding/lang supplies the graph-resolved GMN dialect major");
        let surface_path =
            format!("generated/projections/lang/gmn1/v{major}/gmn-grounding-glyphs.gmn");
        let surface = corpus
            .artifacts
            .iter()
            .find(|(path, _)| *path == surface_path)
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .expect("the grounding-glyph projection witness exists");
        for glyph in ["+", "π", "¬", "*"] {
            assert!(surface.contains(glyph), "missing {glyph:?} in:\n{surface}");
        }
        for fallback in [
            "math__Addition",
            "math__pi",
            "logic__not",
            "lang__Ungrammaticality",
        ] {
            assert!(
                !surface.contains(fallback),
                "writer leaked prefix fallback {fallback:?}:\n{surface}"
            );
        }
    }

    /// The subject IRI of the `lang:ProjectionEmission` record carrying
    /// `lang:projectionTargetName "<target>"` in the sorted N-Triples corpus.
    fn emission_subject_for(nt: &str, target: &str) -> String {
        let needle = format!(" <{}> \"{target}\" .", iri(LANG_NS, "projectionTargetName"));
        let line = nt
            .lines()
            .find(|l| l.ends_with(&needle))
            .unwrap_or_else(|| panic!("no projection emission for target {target:?}"));
        line[..line.find(' ').unwrap()]
            .trim_matches(|c| c == '<' || c == '>')
            .to_owned()
    }

    /// The GBNF/Lark projection targets lower the SAME graph-derived `gmn` glyph-grammar bytes the
    /// EBNF target lifts. The real `grammars/gmn.ebnf` carries a `#xD? #xA` hex terminal in its
    /// `EOL` production — a hex codepoint terminal both surfaces render as a fixed-width char-class
    /// escape (`[\x0D]` / `/\x0D/`) — so the WHOLE grammar is GBNF- and Lark-expressible and ships
    /// a REAL constrained-decode artifact under `gmn1/v<major>/<surface>/gmn.<surface>`. This is
    /// the flagship req-#12 deliverable: a genuine, usable constrained-decode grammar, never a
    /// SoundUnder placeholder.
    #[test]
    fn gbnf_and_lark_project_the_graph_derived_gmn_grammar_as_real_constrained_decode_artifacts() {
        use gmeow_lang_bridge::{Formalism, distinguished_rule, parse_grammar};

        let catalog = repo_catalog();
        let input = collect_input(Some(&catalog)).expect("collect projection input");
        let dictionary = input
            .gmn_dictionary
            .as_ref()
            .expect("grounding/lang supplies the one carrier GMN dictionary");
        let expected_production = dictionary.glyph_registry().render_glyph_token_production();
        let major = input
            .gmn_dialect_major
            .as_deref()
            .expect("grounding/lang supplies the graph-resolved GMN dialect major");
        let corpus = build_corpus(Some(&catalog)).expect("build projection corpus");
        let nt = String::from_utf8(corpus.ntriples.clone()).expect("utf8");

        // Positive control: the graph-derived closed glyph production DOES flow into the projected
        // gmn grammar (the EBNF artifact carries it verbatim) — so the GBNF/Lark artifacts below
        // are of the SAME graph-derived grammar, never a fallback-leaking one.
        let ebnf = corpus
            .artifacts
            .iter()
            .find(|(path, _)| path == "generated/projections/lang/ebnf/gmn.ebnf")
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .expect("the generated GMN EBNF artifact exists");
        assert!(
            ebnf.lines().any(|line| line == expected_production),
            "the EBNF projection must carry the graph-derived glyph production:\n{ebnf}"
        );
        assert!(
            !ebnf.contains("math__Addition"),
            "no prefix-fallback leakage in the graph-derived production:\n{ebnf}"
        );

        // The distinguished start symbol the constrained-decode consumer resolves — derived from
        // the SAME canonical tree the projection serializes (led-first in the artifact).
        let source_grammar = gmeow_lang_bridge::EbnfBridge
            .to_grammar(ebnf.as_bytes())
            .expect("gmn EBNF re-parses");
        let entry = distinguished_rule(&source_grammar.canonicalize());

        for (surface, ext, formalism) in [
            ("gbnf", "gbnf", Formalism::Gbnf),
            ("lark", "lark", Formalism::Lark),
        ] {
            // The REAL deliverable: an ACTUAL artifact under the version-keyed subtree.
            let versioned = format!("generated/projections/lang/gmn1/v{major}/{surface}/gmn.{ext}");
            let artifact = corpus
                .artifacts
                .iter()
                .find(|(p, _)| *p == versioned)
                .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
                .unwrap_or_else(|| {
                    panic!("the {surface} constrained-decode artifact must ship at {versioned}")
                });

            // It carries the graph-derived glyph production (every glyph present) and NO
            // prefix-fallback leakage.
            assert!(
                artifact
                    .lines()
                    .any(|l| l.trim_start().starts_with("glyphToken")),
                "the {surface} artifact must carry the glyphToken production:\n{artifact}"
            );
            for glyph in ["+", "π", "¬", "*"] {
                assert!(
                    artifact.contains(glyph),
                    "the {surface} artifact is missing graph-derived glyph {glyph:?}:\n{artifact}"
                );
            }
            assert!(
                !artifact.contains("math__Addition"),
                "no prefix-fallback leakage in the {surface} artifact:\n{artifact}"
            );

            // It is a valid constrained-decode grammar: the distinguished start/root rule leads
            // the file (the entry every GBNF `root` / Lark `start` consumer resolves from).
            let first_rule_line = artifact
                .lines()
                .find(|l| !l.trim().is_empty())
                .expect("a rule line");
            assert!(
                first_rule_line.starts_with(&entry),
                "the {surface} artifact must lead with the distinguished start rule {entry:?}, \
                 got: {first_rule_line}"
            );

            // It genuinely re-parses to the SAME canonical tree as the graph-derived source — a
            // measured round-trip, not a declared one.
            let reparsed = parse_grammar(artifact.as_bytes(), formalism)
                .unwrap_or_else(|d| panic!("the {surface} artifact must re-parse: {d:?}"));
            assert_eq!(
                reparsed.canonicalize().rules,
                source_grammar.canonicalize().rules,
                "the {surface} artifact must re-parse to the graph-derived canonical tree"
            );

            // The ledger records an EXACT emission with a measured-true round-trip (never a
            // SoundUnder placeholder for the flagship deliverable).
            let subject = emission_subject_for(&nt, surface);
            assert!(
                nt.contains(&triple(
                    &subject,
                    &iri(LOGIC_NS, "preservationKind"),
                    &PreservationKind::Exact.iri()
                )),
                "the {surface} emission must record Exact preservation"
            );
            assert!(
                nt.contains(&triple_typed(
                    &subject,
                    &iri(LANG_NS, "roundTripHolds"),
                    "true",
                    XSD_BOOLEAN
                )),
                "the {surface} emission's round-trip is measured true"
            );
        }
    }

    #[test]
    fn ontolex_forward_projects_the_lexeme_inventory() {
        let catalog = repo_catalog();
        let corpus = build_corpus(Some(&catalog)).expect("build corpus");
        // OntoLex-Lemon is source-driven: the example model's lang:Lexeme inventory lowers to a
        // real ontolex-lemon/*.ttl artifact carrying ontolex:LexicalEntry structure.
        assert!(
            corpus
                .artifacts
                .iter()
                .any(|(p, _)| p.starts_with("generated/projections/lang/ontolex-lemon/")),
            "the lang: lexeme inventory must drive an OntoLex-Lemon projection artifact"
        );
        let ontolex = corpus
            .artifacts
            .iter()
            .find(|(p, _)| p.starts_with("generated/projections/lang/ontolex-lemon/"))
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default();
        assert!(
            ontolex.contains("http://www.w3.org/ns/lemon/ontolex#LexicalEntry"),
            "the OntoLex projection must emit ontolex:LexicalEntry individuals: {ontolex}"
        );
        assert!(
            ontolex.contains("http://www.w3.org/ns/lemon/ontolex#LexicalSense"),
            "the OntoLex projection must emit ontolex:LexicalSense individuals: {ontolex}"
        );
        // A source-driven ledger row (SoundUnder, never the no-source placeholder).
        assert!(
            corpus
                .ledger
                .iter()
                .any(|r| r.target.starts_with("ontolex-lemon:")),
            "OntoLex-Lemon must fold a source-driven ledger row, not a no-source placeholder"
        );
    }

    #[test]
    fn every_ledger_row_satisfies_the_overclaim_floor() {
        let catalog = repo_catalog();
        let corpus = build_corpus(Some(&catalog)).expect("build corpus");
        for row in &corpus.ledger {
            // The residue is read back from the corpus loss store (the single source of truth),
            // never the now-drop-less `ProjectionResult`.
            let residue_owned = corpus.loss.projection_drops_for(&row.target);
            let residue: Vec<&str> = residue_owned.iter().map(String::as_str).collect();
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

    /// Loss-ledger bijection totality (the completeness claim, made a test): over the real
    /// composed model, every registered target is covered by a ledger row, every emission
    /// corresponds to exactly one `lang:ProjectionEmission` record and vice versa, every
    /// generated artifact is carried, and no record names a target outside the registry.
    /// Surjective both ways — the loss ledger accounts for every lang surface and nothing it
    /// does not emit.
    #[test]
    fn loss_ledger_is_bijective_and_total_over_the_registry() {
        use std::collections::BTreeSet;

        let catalog = repo_catalog();
        let input = collect_input(Some(&catalog)).expect("collect input");

        // Drive each target directly to count the emissions + artifacts the driver folds,
        // and record which targets are source-driven (produce ≥1 emission) vs no-source.
        let registry_names: BTreeSet<String> =
            registry().iter().map(|t| t.name().to_owned()).collect();
        let mut total_emissions = 0usize;
        let mut total_artifacts = 0usize;
        let mut source_driven: BTreeSet<String> = BTreeSet::new();
        for target in registry() {
            let emissions = target.emit(&input).expect("emit");
            if !emissions.is_empty() {
                source_driven.insert(target.name().to_owned());
            }
            for emission in &emissions {
                // Every emission carries a non-empty ledger row (a target never emits an
                // artifact or record with no accounting).
                assert!(
                    !emission.ledger.is_empty(),
                    "target '{}' emitted with an empty ledger",
                    target.name()
                );
                total_emissions += 1;
                total_artifacts += emission.artifacts.len();
            }
        }

        let corpus = build_corpus(Some(&catalog)).expect("corpus");
        let nt = String::from_utf8(corpus.ntriples).expect("utf8");

        // emission ↔ record: exactly one `lang:ProjectionEmission` per driven emission.
        let record_count = nt
            .matches(&format!("<{}> .", iri(LANG_NS, "ProjectionEmission")))
            .count();
        assert_eq!(
            record_count, total_emissions,
            "every emission must fold exactly one lang:ProjectionEmission record"
        );

        // artifact ↔ carried: every generated artifact rides the corpus, keyed under the
        // projection dir, and the count matches what the targets produced (nothing dropped,
        // nothing conjured).
        assert_eq!(
            corpus.artifacts.len(),
            total_artifacts,
            "every emitted artifact must be carried by the corpus"
        );
        for (path, bytes) in &corpus.artifacts {
            assert!(
                path.starts_with(LANG_PROJECTION_DIR),
                "artifact {path} is outside the projection dir"
            );
            assert!(!bytes.is_empty(), "artifact {path} carries no bytes");
        }

        // Target coverage (surjective): every registered target is covered by either a
        // source-driven ProjectionEmission record or an honest no-source ledger row.
        let no_source: BTreeSet<String> = corpus
            .ledger
            .iter()
            .filter_map(|r| r.target.strip_prefix("lang-projection:").map(str::to_owned))
            .collect();
        for name in &registry_names {
            assert!(
                source_driven.contains(name) || no_source.contains(name),
                "registry target '{name}' is covered by neither a projection record nor a \
                 no-source ledger row (loss-ledger totality gap)"
            );
        }

        // No orphan record: every emitted projectionTargetName is a real registry target.
        let target_marker = format!("<{}> ", iri(LANG_NS, "projectionTargetName"));
        for line in nt.lines() {
            if let Some(idx) = line.find(&target_marker) {
                let obj = &line[idx + target_marker.len()..];
                // object literal: "name" .  — strip quotes + trailing " ."
                let name = obj.trim_end_matches(" .").trim_matches('"');
                assert!(
                    registry_names.contains(name),
                    "orphan lang:ProjectionEmission names non-registry target {name:?}"
                );
            }
        }

        // The ledger accounts for at least one row per registered target.
        assert!(
            corpus.ledger.len() >= registry_names.len(),
            "the loss ledger must carry at least one row per registered target"
        );
    }

    /// A composed form scoped to two co-resident `lang:Analysis` readings must emit two
    /// CoNLL-U artifacts — never a single silently-chosen winner (the saw-her-duck discipline
    /// at the projection seam), driven from the `lang:` model through the registered target.
    #[test]
    fn conllu_two_readings_emit_two_artifacts_never_one() {
        // "saw duck" scoped to TWO analyses (duck-as-bird / duck-as-crouch), each its own tree.
        let doc = "\
@prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix ex:   <http://example.org/lang/> .\n\
ex:wSaw a lang:WordForm ; rdfs:label \"saw\" .\n\
ex:wDuck a lang:WordForm ; rdfs:label \"duck\" .\n\
ex:sent a lang:ComposedForm ; rdfs:label \"saw duck\" ;\n\
    lang:inAnalysis ex:aBird , ex:aCrouch ; lang:formSlot ex:b0 , ex:b1 , ex:c0 , ex:c1 .\n\
ex:aBird a lang:Analysis .\n\
ex:aCrouch a lang:Analysis .\n\
ex:b0 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .\n\
ex:b1 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:objectRole ; lang:dependsOn ex:b0 .\n\
ex:c0 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .\n\
ex:c1 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:complementRole ; lang:dependsOn ex:c0 .\n";
        let input = LangProjectionInput {
            lang_models: vec![NamedSource {
                name: "saw-duck".to_owned(),
                bytes: doc.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let conllu = registry()
            .into_iter()
            .find(|t| t.name() == "conllu")
            .expect("conllu target registered");
        let emissions = conllu.emit(&input).expect("emit");
        assert_eq!(
            emissions.len(),
            1,
            "one emission per analyzed composed form"
        );
        let e = &emissions[0];
        assert_eq!(e.emitted_reading_count, Some(2));
        assert_eq!(
            e.artifacts.len(),
            2,
            "two co-resident analyses must emit two CoNLL-U artifacts, never one"
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

    #[test]
    fn conllu_forward_projects_the_composed_form() {
        let catalog = repo_catalog();
        let corpus = build_corpus(Some(&catalog)).expect("build corpus");
        // CoNLL-U is source-driven: the example model's analyzed composed form lowers to a real
        // conllu/*.reading-0.conllu artifact carrying the UD tree.
        let conllu = corpus
            .artifacts
            .iter()
            .find(|(p, _)| p.starts_with("generated/projections/lang/conllu/"))
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned());
        let text = conllu.expect("the analyzed composed form must drive a CoNLL-U artifact");
        assert!(text.contains("\tchase\tchase\tVERB\t"), "{text}");
        assert!(text.contains("\troot\t"), "{text}");
        assert!(
            corpus
                .ledger
                .iter()
                .any(|r| r.target.starts_with("conllu:")),
            "CoNLL-U must fold a source-driven ledger row, not a no-source placeholder"
        );
    }

    /// The BCP-47 target GENERATES the bare tags `en`/`fr`/`zh` for the three carrier varieties
    /// (`lang:gmeowEnglish`/`gmeowFrench`/`gmeowMandarin`) in the scanned lang module surface,
    /// suppressing the redundant default-orthography script, and FOLDS those `<variety>
    /// gmeow:bcp47Tag "tag"` triples into the reasoned corpus graph (via `source_rdf`) so the
    /// SPARQL projection consumers resolve a language's tag by joining through `lang:varietyOf`.
    #[test]
    fn bcp47_forward_projects_the_carrier_variety_tags_into_the_corpus() {
        let catalog = repo_catalog();
        let corpus = build_corpus(Some(&catalog)).expect("build corpus");
        let nt = String::from_utf8(corpus.ntriples.clone()).expect("utf8");

        const BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";
        const LANG: &str = "https://blackcatinformatics.ca/lang/";
        // The three carrier varieties derive their bare tags, folded into the bundle graph on the
        // VARIETY IRI (not the language) — so a consumer joins `?v lang:varietyOf ?lang`.
        for (variety, tag) in [
            ("gmeowEnglish", "en"),
            ("gmeowFrench", "fr"),
            ("gmeowMandarin", "zh"),
        ] {
            let triple = format!("<{LANG}{variety}> <{BCP47_TAG}> \"{tag}\" .");
            assert!(
                nt.contains(&triple),
                "the bcp47 tag triple must be folded into the corpus graph: {triple}\n{nt}"
            );
        }
        // The committed artifact carries the same tags.
        let ttl = corpus
            .artifacts
            .iter()
            .find(|(p, _)| p == "generated/projections/lang/bcp47-tags.ttl")
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
            .expect("the carrier varieties must drive a bcp47-tags.ttl artifact");
        assert!(ttl.contains("\"en\""), "{ttl}");
        assert!(ttl.contains("\"fr\""), "{ttl}");
        assert!(ttl.contains("\"zh\""), "{ttl}");
    }

    /// The bundle-level GMN⇄controlled-NL verbalizer resolves REAL operator forms from the
    /// carrier registry + the grounding modules' `rdfs:label`s, and ships a versioned
    /// `verbalizations.ttl` of `lang:TranslationUnit` crossings whose N-Triples ride the corpus
    /// graph. The resolution is genuinely graph-driven (a non-empty operator inventory with real
    /// controlled-NL labels), injective, and byte-reproducible.
    #[test]
    fn verbalizer_projects_real_operator_pairs_into_the_corpus() {
        let catalog = repo_catalog();
        let input = collect_input(Some(&catalog)).expect("collect projection input");
        // The real carrier resolves a non-empty operator inventory, each with a controlled-NL label.
        assert!(
            !input.gmn_operator_forms.is_empty(),
            "the carrier registry must resolve verbalizable GMN operator forms"
        );
        assert!(
            input
                .gmn_operator_forms
                .iter()
                .all(|f| !f.term_label.is_empty() && !f.gmn_glyph.is_empty()),
            "every resolved operator form carries a glyph surface and a controlled-NL label"
        );

        let major = input
            .gmn_dialect_major
            .as_deref()
            .expect("grounding/lang supplies the graph-resolved GMN dialect major");
        let corpus = build_corpus(Some(&catalog)).expect("build projection corpus");
        let nt = String::from_utf8(corpus.ntriples.clone()).expect("utf8");

        // The versioned verbalizations artifact ships, is valid Turtle, and carries the crossings.
        let path = format!("generated/projections/lang/gmn1/v{major}/verbalizations.ttl");
        let ttl = corpus
            .artifacts
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_else(|| panic!("the verbalizations artifact must ship at {path}"));
        assert!(
            ttl.contains("<https://blackcatinformatics.ca/lang/translationCorrespondence>"),
            "verbalizations carry lang:translationCorrespondence:\n{ttl}"
        );
        // The crossings ride into the reasoned corpus graph (queryable, not only on disk).
        assert!(
            nt.contains("<https://blackcatinformatics.ca/gmeow/gmnVerbalizationsCurrent>"),
            "the verbalization corpus subject rides the projection corpus graph"
        );
        // A source-driven ledger row (the verbalizer emission was folded).
        assert!(
            corpus
                .ledger
                .iter()
                .any(|r| r.target == "gmn1:verbalizations"),
            "the verbalizer folds a ledger row"
        );
    }

    #[test]
    fn semaf_forward_projects_the_denotation() {
        let catalog = repo_catalog();
        let corpus = build_corpus(Some(&catalog)).expect("build corpus");
        // SemAF is source-driven: the example model's logic-formula lang:Denotation lowers to a
        // real semaf/*.amr meaning-graph artifact (dogfooding the meaning-annotation surface).
        let amr = corpus
            .artifacts
            .iter()
            .find(|(p, _)| p.starts_with("generated/projections/lang/semaf/"))
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned());
        let text = amr.expect("the logic-formula denotation must drive a SemAF/AMR artifact");
        assert!(text.contains("::snt cats chase mice"), "{text}");
        assert!(
            corpus.ledger.iter().any(|r| r.target.starts_with("semaf:")),
            "SemAF must fold a source-driven ledger row, not a no-source placeholder"
        );
    }
}
