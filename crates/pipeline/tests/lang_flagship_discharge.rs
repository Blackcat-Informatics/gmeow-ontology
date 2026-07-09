// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `lang:FlagshipScenario` execution-discharge harness.
//!
//! The math flagship precedent (`math:FlagshipScenario`) discharges its acceptance bar by
//! EXISTENCE — a manifest names the artifacts and three surfaces check they are present and
//! fully linked. This harness goes one rung further: it discharges the language grounding
//! layer's five flagships by EXECUTION. It reads the acceptance manifest
//! `slices/grounding/lang/examples/flagship-acceptance.ttl`, and for each of the five
//! `lang:FlagshipScenario` individuals it:
//!
//! 1. **Runs the guard.** Loads the `lang:guardedByCounterExample` fixture and pushes it
//!    through BOTH native validation channels — the structural lint
//!    ([`structural_lint_dataset`]) and the native SHACL engine
//!    ([`shacl_validate_dataset`]) — and asserts the UNION of triggered `lang:` failure
//!    classes equals EXACTLY the one named by `lang:enforcesFailureClass`. The gate must
//!    bite, and bite for precisely the declared reason.
//! 2. **Checks the worked example.** Loads the `lang:demonstratedByExample` fixture, runs the
//!    SAME two channels, and asserts NO `lang:` failure class fires — the positive is
//!    well-formed.
//! 3. **Runs the producer.** Dispatches the `lang:demonstratedByProducer` identifier to the
//!    named native entrypoint, RUNS it, and asserts its output carries the structure that
//!    flagship claims (a compositional lowering with per-stage exact preservation, a
//!    prose-lift corpus with prose-hashes and exact round-trips, a translation corpus with
//!    per-unit preservation judgments, a grammar-projection corpus with exact round-trips
//!    and per-reading routing).
//!
//! The five (counter-example, example, failure-class, producer) tuples are READ from the
//! manifest, never hard-coded — so a manifest edit that unwires a flagship is caught here.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use gmeow_lang_bridge::lower::{flagship_svo_sentence, lower_svo};
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_validate::lint::{LintConfig, structural_lint_dataset};
use gmeow_validate::store::{dataset_from_paths, parse_file_dataset, shacl_validate_dataset};
use purrdf::shapes::engine::parse_shapes;
use purrdf::{RdfDataset, RdfTerm};

/// The `lang:` grounding namespace (byte-identical to every `lang:` producer). Used for the
/// SCANNED failure classes (`lang:<Class>`), which stay slice-namespaced.
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";

/// The shared `gmeow:` namespace. The flagship-manifest PREDICATES (the acceptance-bar
/// wiring) are hoisted here, so the manifest reads and the shape→failure-class annotation
/// reads resolve `gmeow:demonstratedBy*` / `gmeow:guardedByCounterExample` /
/// `gmeow:enforcesFailureClass`, while the failure-class VALUES they point at stay `lang:`.
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The repo root: `crates/pipeline/..` twice up, mirroring the in-crate stage tests so the
/// harness drives off the SAME slice tree the production path discovers.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root canonicalizes")
}

/// The language slice root — the base the manifest's relative fixture paths resolve against.
fn lang_slice_root() -> PathBuf {
    repo_root().join("slices").join("grounding").join("lang")
}

/// The shared in-memory source catalog, discovered ONCE exactly as the mappings stage
/// discovers it, so the FS2/FS4/FS5 producers ingest the real composed-source universe.
fn repo_catalog() -> purrdf::slice::SliceCatalog {
    purrdf::slice::SliceCatalog::discover(
        &repo_root().join("slices"),
        gmeow_pipeline::gmeow_slice_vocab(),
    )
    .expect("discover slice catalog")
}

/// The minimal lint config — the same shape [`gmeow_validate`]'s own integration tests build.
/// It carries no selector tokens or core-slice grading; the `lang:` structural checks it
/// runs are namespace-independent, so a bare config exercises them fully.
fn minimal_lint_config() -> LintConfig {
    LintConfig {
        namespace: "https://blackcatinformatics.ca/gmeow/".into(),
        ontology_iri: "https://blackcatinformatics.ca/gmeow".into(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: HashSet::new(),
    }
}

/// The local name of a `lang:` IRI (`…/lang/UnhashableSurface` → `UnhashableSurface`).
fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned()
}

/// One flagship binding, read from the manifest.
#[derive(Debug)]
struct Flagship {
    /// The flagship individual IRI (for diagnostics).
    subject: String,
    /// The absolute path to the `lang:demonstratedByExample` fixture.
    example: PathBuf,
    /// The absolute path to the `lang:guardedByCounterExample` fixture.
    counter_example: PathBuf,
    /// The local name of the `lang:enforcesFailureClass` the guard must raise.
    failure_class: String,
    /// The `lang:demonstratedByProducer` identifier string.
    producer: String,
}

/// Parse the acceptance manifest into the five flagship bindings, resolving each relative
/// fixture path against the language slice root.
fn parse_manifest() -> Vec<Flagship> {
    let manifest = lang_slice_root()
        .join("examples")
        .join("flagship-acceptance.ttl");
    let ds = parse_file_dataset(&manifest).expect("manifest parses");
    let base = lang_slice_root();

    // Collect, per flagship subject, each of the four bound values.
    let mut example: HashMap<String, String> = HashMap::new();
    let mut counter: HashMap<String, String> = HashMap::new();
    let mut class: HashMap<String, String> = HashMap::new();
    let mut producer: HashMap<String, String> = HashMap::new();

    // The four predicate IRIs, built ONCE rather than reformatted per quad. Hoisted to the
    // shared gmeow: manifest vocabulary; only the enforcesFailureClass VALUE stays lang:.
    let pred_example = format!("{GMEOW_NS}demonstratedByExample");
    let pred_counter = format!("{GMEOW_NS}guardedByCounterExample");
    let pred_class = format!("{GMEOW_NS}enforcesFailureClass");
    let pred_producer = format!("{GMEOW_NS}demonstratedByProducer");

    for quad in ds.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        let obj_literal = match &quad.object {
            RdfTerm::Literal(lit) => Some(lit.lexical_form.clone()),
            _ => None,
        };
        let obj_iri = match &quad.object {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        };
        let pred = quad.predicate.as_str();
        if pred == pred_example {
            example.insert(
                subject.clone(),
                obj_literal.expect("example is a literal path"),
            );
        } else if pred == pred_counter {
            counter.insert(
                subject.clone(),
                obj_literal.expect("counter-example is a literal path"),
            );
        } else if pred == pred_class {
            class.insert(
                subject.clone(),
                local_name(&obj_iri.expect("failure class is an IRI")),
            );
        } else if pred == pred_producer {
            producer.insert(
                subject.clone(),
                obj_literal.expect("producer identifier is a literal"),
            );
        }
    }

    let mut flagships: Vec<Flagship> = producer
        .keys()
        .map(|subject| {
            let rel_example = example.get(subject).unwrap_or_else(|| {
                panic!("flagship {subject} missing gmeow:demonstratedByExample")
            });
            let rel_counter = counter.get(subject).unwrap_or_else(|| {
                panic!("flagship {subject} missing gmeow:guardedByCounterExample")
            });
            Flagship {
                subject: subject.clone(),
                example: base.join(rel_example),
                counter_example: base.join(rel_counter),
                failure_class: class
                    .get(subject)
                    .unwrap_or_else(|| {
                        panic!("flagship {subject} missing gmeow:enforcesFailureClass")
                    })
                    .clone(),
                producer: producer[subject].clone(),
            }
        })
        .collect();
    flagships.sort_by(|a, b| a.subject.cmp(&b.subject));
    assert_eq!(
        flagships.len(),
        5,
        "the acceptance manifest declares exactly five gmeow:FlagshipScenario producers"
    );
    flagships
}

/// The set of `lang:` failure-class local names a lint report raises. A `lang:` failure is
/// emitted as the substring token `lang:<ClassLocalName>:` (a CamelCase class immediately
/// followed by a colon, e.g. `lang:ExactPreservationViolated: …`). Non-class `lang:` mentions
/// in a message — a property CURIE, or a full `…/lang/…` IRI — never match, because they are
/// not `lang:<Uppercase…>:`.
fn native_lang_failures(errors: &[String]) -> HashSet<String> {
    let mut out = HashSet::new();
    for error in errors {
        // Walk every `lang:` occurrence by byte index. `match_indices` yields the start of
        // each match, so we always advance past the whole token — never into the middle of a
        // multibyte char (a `lang:中文` token reads an empty ascii prefix and is skipped, not
        // sliced at a non-char boundary).
        for (idx, _) in error.match_indices("lang:") {
            let after = &error[idx + "lang:".len()..];
            // The ascii-alphanumeric run is pure ASCII, so its byte length is always a char
            // boundary; the char immediately after it decides whether this is `lang:<Class>:`.
            let local: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            // A failure class is a CamelCase (Uppercase-initial) local name immediately
            // followed by `:` — the emitted token shape `lang:<Class>:`.
            if local.starts_with(|c: char| c.is_ascii_uppercase())
                && after[local.len()..].starts_with(':')
            {
                out.insert(local);
            }
        }
    }
    out
}

/// Build the `<node-shape-IRI> -> failure-class-local-name` map from the lang shapes graph.
/// Each `lang:` node shape carries `<shape> gmeow:enforcesFailureClass <lang:class>` (the
/// annotation predicate is hoisted to the shared gmeow: vocabulary; the failure-class value
/// stays lang:); the native SHACL engine stamps a violation's `source_shape` with its parent
/// NODE shape IRI (property constraints inherit the node shape's id), so this IRI-keyed map
/// resolves every violation.
fn shape_class_map(shapes_ds: &RdfDataset) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let pred_class = format!("{GMEOW_NS}enforcesFailureClass");
    for quad in shapes_ds.owned_quads() {
        if quad.predicate == pred_class
            && let (RdfTerm::Iri(shape), RdfTerm::Iri(class)) = (&quad.subject, &quad.object)
        {
            map.insert(shape.clone(), local_name(class));
        }
    }
    assert!(
        !map.is_empty(),
        "the lang shapes graph must carry gmeow:enforcesFailureClass annotations"
    );
    map
}

/// The set of `lang:` failure-class local names the native SHACL engine raises over `ds`,
/// resolved from each violation's `source_shape` through the shape→class map.
fn shacl_lang_failures(
    ds: &RdfDataset,
    shapes: &purrdf::shapes::shapes::Shapes,
    shape_class: &HashMap<String, String>,
) -> HashSet<String> {
    let report = shacl_validate_dataset(ds, shapes);
    let mut out = HashSet::new();
    for result in &report.results {
        let rendered = result.source_shape.to_string();
        // Unwrap exactly one `<…>` pair (a rendered IRI term); leave a bare IRI untouched
        // rather than greedily stripping repeated angle brackets.
        let shape_iri = rendered
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(rendered.as_str());
        if let Some(class) = shape_class.get(shape_iri) {
            out.insert(class.clone());
        }
    }
    out
}

/// Run BOTH channels over a fixture and return the union of triggered `lang:` failure classes.
///
/// The fixture is validated MERGED with the slice's `module.ttl` — exactly the union the
/// slice's own conformance harness (`crates/slicetest run_conformance_cell`) validates. The
/// module graph supplies the vocabulary typing and subclass axioms a counter-example
/// legitimately relies on (e.g. `lang:denotesEntity a lang:DenotationKind`,
/// `lang:WordForm rdfs:subClassOf lang:Form`), so an `sh:class`/`sh:nodeKind` constraint is
/// discharged by the vocabulary and NOT spuriously counted against a fixture that isolates a
/// different violation. The counter-example fixtures live under `tests/counter-examples/`, so
/// `make validate` never loads them as data — they are exercised only through harnesses like
/// this one.
fn triggered_lang_failures(
    fixture: &Path,
    shapes: &purrdf::shapes::shapes::Shapes,
    shape_class: &HashMap<String, String>,
) -> HashSet<String> {
    let module = lang_slice_root().join("module.ttl");
    let ds = dataset_from_paths(&[module, fixture.to_path_buf()])
        .unwrap_or_else(|e| panic!("fixture {} + module parse: {e}", fixture.display()));
    let lint = structural_lint_dataset(&ds, &minimal_lint_config());
    let mut union = native_lang_failures(&lint.errors());
    union.extend(shacl_lang_failures(&ds, shapes, shape_class));
    union
}

#[test]
fn every_flagship_is_discharged_by_execution() {
    let flagships = parse_manifest();

    // The lang SHACL shapes, parsed once for the SHACL channel, plus a plain dataset parse of
    // the same file so the shape→failure-class annotations (not a SHACL predicate) are read.
    let shapes_path = lang_slice_root().join("shapes.ttl");
    let shapes_text = std::fs::read_to_string(&shapes_path).expect("read lang shapes.ttl");
    let shapes = parse_shapes(&shapes_text).expect("lang shapes parse");
    let shapes_ds = parse_file_dataset(&shapes_path).expect("lang shapes.ttl parses as a dataset");
    let shape_class = shape_class_map(&shapes_ds);

    // The FS2/FS4/FS5 producers ingest the real slice catalog; discover it once.
    let catalog = repo_catalog();

    for flagship in &flagships {
        // ---- (1) Executed guard: the counter-example raises EXACTLY its failure class. ----
        let triggered = triggered_lang_failures(&flagship.counter_example, &shapes, &shape_class);
        let expected: HashSet<String> = std::iter::once(flagship.failure_class.clone()).collect();
        assert_eq!(
            triggered,
            expected,
            "flagship {}: the counter-example {} must raise EXACTLY {{{}}}, but raised {:?}",
            flagship.subject,
            flagship.counter_example.display(),
            flagship.failure_class,
            triggered
        );

        // ---- (2) Clean worked example: NO lang failure class fires. ----
        let clean = triggered_lang_failures(&flagship.example, &shapes, &shape_class);
        assert!(
            clean.is_empty(),
            "flagship {}: the worked example {} must be clean, but raised {:?}",
            flagship.subject,
            flagship.example.display(),
            clean
        );

        // ---- (3) Executed producer: RUN it and assert its output structure. ----
        run_producer(flagship, &catalog);
    }
}

/// Dispatch and RUN a flagship's `lang:demonstratedByProducer`, asserting the executed
/// output carries the structure the flagship claims.
fn run_producer(flagship: &Flagship, catalog: &purrdf::slice::SliceCatalog) {
    match flagship.producer.as_str() {
        // FS1: the compositional-lowering corpus PRODUCER — the production wiring that folds the
        // SVO lowering into gmeow.gts. A sentence lowers, compositionally, to a full-FOL formula;
        // every stage is declared and Exact (asserted on the SAME `lower_svo` the producer runs),
        // and the FOLDED corpus N-Triples carry the compositional formula (the `chase` relation).
        "pipeline::stages::lang_lowering::build_corpus" => {
            // The stage-structure asserts, on the same lowering the producer runs.
            let lowering =
                lower_svo(&flagship_svo_sentence()).expect("FS1: the flagship SVO sentence lowers");
            lowering
                .assert_all_stages_declared()
                .expect("FS1: every lowering stage is declared");
            for stage in &lowering.stages {
                assert_eq!(
                    stage.preservation,
                    PreservationKind::Exact,
                    "FS1: the modeled fragment lowers exactly, stage {}",
                    stage.name
                );
            }

            // The FOLDED production corpus: the bundle projection the pipeline lands in gmeow.gts.
            let corpus = gmeow_pipeline::stages::lang_lowering::build_corpus()
                .expect("FS1: the compositional-lowering corpus builds");
            let nt = String::from_utf8(corpus.ntriples)
                .expect("FS1: corpus N-Triples emission is UTF-8");
            assert!(
                !nt.trim().is_empty(),
                "FS1: the folded lowering corpus is non-empty"
            );
            for needle in [
                "CompositionalLowering", // the lowering typing
                "LoweringStage",         // the per-stage preservation records
                "chase",                 // the compositional formula's `chase` relation
            ] {
                assert!(
                    nt.contains(needle),
                    "FS1: the folded lowering corpus must carry {needle}"
                );
            }
            // Every folded stage records an exact preservation (no lossy lowering shipped).
            let exact = PreservationKind::Exact.iri();
            let stage_exact = nt
                .lines()
                .filter(|l| l.contains("preservationKind") && l.contains(&exact))
                .count();
            assert_eq!(
                stage_exact,
                lowering.stages.len(),
                "FS1: every folded lowering stage records an exact preservation"
            );
        }

        // FS2: the prose-lift stage. Every @x-gmeow-english literal is lifted to a
        // content-addressed surface carrying its prose-hash and an exact surface round-trip.
        "pipeline::stages::lang_form::build_corpus" => {
            let corpus = gmeow_pipeline::stages::lang_form::build_corpus(Some(catalog))
                .expect("FS2: the prose-lift corpus builds over the real slice catalog");
            let nt = String::from_utf8(corpus.ntriples).expect("FS2: corpus N-Triples is UTF-8");
            assert!(
                !nt.trim().is_empty(),
                "FS2: the prose-lift corpus is non-empty"
            );
            for needle in [
                "candidateSourceHash",   // the prose-hash (logic:candidateSourceHash)
                "surfaceCorrespondence", // the surface round-trip Correspondence
                "surfaceText",           // the lifted surface literal
                "ExactPreservation",     // the folded exact-preservation ledger judgment
            ] {
                assert!(
                    nt.contains(needle),
                    "FS2: the prose-lift corpus must carry {needle}"
                );
            }

            // The TOTALITY the flagship advertises, discharged on the PRODUCTION corpus by
            // the contract artifact itself: every DISTINCT @x-gmeow-english literal in the
            // extraction universe is lifted to a reachable lang:SurfaceForm (inline
            // surfaceText, or a by-reference surfaceBlob digest for document-scale surfaces),
            // so `covered == universe` — the count-equality, not mere presence of a token.
            let coverage = gmeow_pipeline::stages::lang_form::prose_lift_coverage(Some(catalog))
                .expect("FS2: prose-lift coverage computes over the real slice catalog");
            assert!(
                coverage.universe > 0,
                "FS2: the source bundle must carry @x-gmeow-english prose to lift"
            );
            assert_eq!(
                coverage.covered,
                coverage.universe,
                "FS2: {} of {} distinct @x-gmeow-english literals are not lifted — the prose \
                 lift is not total",
                coverage.universe - coverage.covered,
                coverage.universe
            );
        }

        // FS3: the translation stage. The multilingual docs are lang:TranslationUnits, each
        // carrying a per-unit preservation judgment rather than a silent Exact default.
        "pipeline::stages::lang_translation::build_corpus" => {
            let root = repo_root();
            let corpus = gmeow_pipeline::stages::lang_translation::build_corpus(&root)
                .expect("FS3: the translation corpus builds over the real .po catalogs");
            let nt = String::from_utf8(corpus.ntriples).expect("FS3: corpus N-Triples is UTF-8");
            for needle in ["TranslationUnit", "preservationKind"] {
                assert!(
                    nt.contains(needle),
                    "FS3: the translation corpus must carry {needle}"
                );
            }
        }

        // FS4 and FS5 both name the projection stage, so the shared producer string cannot
        // tell them apart. Their DISTINCTIVE claims are discharged SEPARATELY, keyed on the
        // flagship node's local name, so each is INDEPENDENTLY falsifiable over the SAME real
        // projection corpus:
        //   FS4 (serializationsAsGrammars) — the serialization grammars are lang:Grammar
        //     objects whose emit/parse round-trip is exact (a lossless crossing).
        //   FS5 (ambiguityHeldHonestly) — a genuinely ambiguous authored form projects its
        //     readings as first-class CO-RESIDENT data: the corpus carries an emission with
        //     lang:emittedReadingCount >= 2 AND the MATCHING number of co-resident reading
        //     artifacts (one per reading), so the projection holds the ambiguity rather than
        //     silently collapsing it to a single winner. The projection stage's Invariant 3
        //     hard-fail (`lang:ProjectionSilentDisambiguation`) is the negative teeth; this is
        //     the POSITIVE discharge on the shipped surface.
        "pipeline::stages::lang_projection::build_corpus" => {
            let corpus = gmeow_pipeline::stages::lang_projection::build_corpus(Some(catalog))
                .expect("FS4/FS5: the projection corpus builds over the real grammars");
            let nt = String::from_utf8(corpus.ntriples).expect("corpus N-Triples is UTF-8");
            for needle in [
                "Grammar",            // lang:Grammar objects
                "ProjectionEmission", // one emission per grammar / per reading
                "ExactPreservation",  // the lossless crossing judgment
                "roundTripHolds",     // the measured emit/parse round-trip
            ] {
                assert!(
                    nt.contains(needle),
                    "FS4/FS5: the projection corpus must carry {needle}"
                );
            }
            assert!(
                !nt.contains("roundTripHolds \"false\""),
                "FS4/FS5: no Exact emission may record a failing round-trip"
            );

            match local_name(&flagship.subject).as_str() {
                // FS4: the grammar round-trip is exact — the authored *.ebnf grammars drive
                // lang:Grammar objects and EBNF projection artifacts whose crossing is Exact.
                "serializationsAsGrammars" => {
                    assert!(
                        corpus
                            .artifacts
                            .iter()
                            .any(|(p, _)| p.starts_with("generated/projections/lang/ebnf/")),
                        "FS4: the serialization grammars must drive EBNF projection artifacts"
                    );
                }
                // FS5: the ambiguity is HELD — a co-resident emission with >= 2 readings and
                // exactly that many co-resident reading artifacts on the shipped surface.
                "ambiguityHeldHonestly" => assert_ambiguity_held(&nt, &corpus.artifacts),
                other => panic!(
                    "flagship {}: a lang_projection producer is bound to an unexpected flagship \
                     node {other:?}; FS4/FS5 are the only projection-stage flagships",
                    flagship.subject
                ),
            }
        }

        other => panic!(
            "flagship {}: unknown gmeow:demonstratedByProducer identifier {other:?}",
            flagship.subject
        ),
    }
}

/// The FS5 positive discharge: over the REAL projection corpus, a genuinely ambiguous
/// authored form keeps every reading as first-class co-resident data. Assert that some
/// `lang:ProjectionEmission` in the corpus declares `lang:emittedReadingCount` >= 2 AND that
/// the co-resident reading artifacts for that emission's source number EXACTLY that count
/// (>= 2, one artifact per reading) — the ambiguity is held on the shipped surface, never
/// silently collapsed to a single winner.
///
/// This is independently falsifiable from FS4: it fails if no authored form projects two
/// co-resident readings (the count drops to 1), or if the per-reading artifacts do not match
/// the declared reading count — neither of which FS4's grammar-round-trip assert can mask.
fn assert_ambiguity_held(nt: &str, artifacts: &[(String, Vec<u8>)]) {
    let pred_count = format!("{LANG_NS}emittedReadingCount");
    let pred_source = format!("{LANG_NS}projectsSource");

    // Per emission subject, the declared reading count and the projected source IRI.
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut sources: HashMap<String, String> = HashMap::new();
    for line in nt.lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(subj), Some(pred), Some(obj)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let subj = subj.trim_matches(['<', '>']).to_owned();
        let pred = pred.trim_matches(['<', '>']);
        if pred == pred_count {
            // obj: "N"^^<…nonNegativeInteger> .
            if let Some(n) = obj
                .split('"')
                .nth(1)
                .and_then(|lex| lex.parse::<u64>().ok())
            {
                counts.insert(subj, n);
            }
        } else if pred == pred_source {
            // obj: <source-iri> .
            let src = obj
                .trim_end_matches(" .")
                .trim_matches(['<', '>'])
                .to_owned();
            sources.insert(subj, src);
        }
    }

    // A co-resident emission: reading count >= 2 whose source resolves.
    let (emission, count) = counts
        .iter()
        .filter(|&(_, &n)| n >= 2)
        .max_by_key(|&(_, &n)| n)
        .map(|(s, &n)| (s.clone(), n))
        .expect(
            "FS5: the real projection corpus must carry a lang:ProjectionEmission with \
             lang:emittedReadingCount >= 2 — a genuinely ambiguous authored form whose readings \
             are held as co-resident data, not collapsed to a single winner",
        );
    let source = sources
        .get(&emission)
        .unwrap_or_else(|| panic!("FS5: co-resident emission {emission} names no projectsSource"));

    // The co-resident reading artifacts for that source form: the CoNLL-U target emits one
    // `…<form>.reading-<i>.conllu` artifact per reading. Their number must equal the declared
    // count (and hence be >= 2) — the ambiguity is materialized one-artifact-per-reading.
    let form_local = source.rsplit(['/', '#']).next().unwrap_or(source);
    let marker = format!(".{form_local}.reading-");
    let coresident = artifacts
        .iter()
        .filter(|(p, _)| p.contains("/conllu/") && p.contains(&marker))
        .count() as u64;
    assert!(
        coresident >= 2,
        "FS5: the ambiguous form <{source}> must materialize >= 2 co-resident reading \
         artifacts, found {coresident}"
    );
    assert_eq!(
        coresident, count,
        "FS5: the ambiguous form <{source}> declares {count} co-resident reading(s) but \
         materialized {coresident} artifact(s) — one artifact per reading, never a silent \
         collapse or a phantom reading"
    );
}

#[test]
fn native_lang_failures_handles_non_ascii_and_isolates_camelcase_class() {
    // A `lang:` token immediately followed by a NON-ASCII multibyte char must neither panic
    // (the scan must never slice at a non-char boundary) nor match. A CamelCase class token
    // `lang:<Class>:` in the SAME message must still be collected, and only that one.
    let errors = vec![
        "guard raised lang:中文 alongside lang:denotesEntity: and lang:ExactPreservationViolated: here"
            .to_string(),
    ];
    let got = native_lang_failures(&errors);
    let want: HashSet<String> = std::iter::once("ExactPreservationViolated".to_string()).collect();
    assert_eq!(
        got, want,
        "only the CamelCase-before-colon class matches; lowercase and non-ascii tokens do not, and the multibyte char does not panic"
    );

    // A `lang:` at the very end of a string and a trailing multibyte token also stay panic-free.
    let edge = vec!["dangling lang:".to_string(), "tail lang:漢字".to_string()];
    assert!(native_lang_failures(&edge).is_empty());
}
