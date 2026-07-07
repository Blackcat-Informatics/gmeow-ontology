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

/// The `lang:` grounding namespace (byte-identical to every `lang:` producer).
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";

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
        gmeow_pipeline::gmeow_ns::gmeow_slice_vocab(),
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

    // The four predicate IRIs, built ONCE rather than reformatted per quad.
    let pred_example = format!("{LANG_NS}demonstratedByExample");
    let pred_counter = format!("{LANG_NS}guardedByCounterExample");
    let pred_class = format!("{LANG_NS}enforcesFailureClass");
    let pred_producer = format!("{LANG_NS}demonstratedByProducer");

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
            let rel_example = example
                .get(subject)
                .unwrap_or_else(|| panic!("flagship {subject} missing lang:demonstratedByExample"));
            let rel_counter = counter.get(subject).unwrap_or_else(|| {
                panic!("flagship {subject} missing lang:guardedByCounterExample")
            });
            Flagship {
                subject: subject.clone(),
                example: base.join(rel_example),
                counter_example: base.join(rel_counter),
                failure_class: class
                    .get(subject)
                    .unwrap_or_else(|| {
                        panic!("flagship {subject} missing lang:enforcesFailureClass")
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
        "the acceptance manifest declares exactly five lang:FlagshipScenario producers"
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
/// Each `lang:` node shape carries `<shape> lang:enforcesFailureClass <class>`; the native
/// SHACL engine stamps a violation's `source_shape` with its parent NODE shape IRI (property
/// constraints inherit the node shape's id), so this IRI-keyed map resolves every violation.
fn shape_class_map(shapes_ds: &RdfDataset) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let pred_class = format!("{LANG_NS}enforcesFailureClass");
    for quad in shapes_ds.owned_quads() {
        if quad.predicate == pred_class
            && let (RdfTerm::Iri(shape), RdfTerm::Iri(class)) = (&quad.subject, &quad.object)
        {
            map.insert(shape.clone(), local_name(class));
        }
    }
    assert!(
        !map.is_empty(),
        "the lang shapes graph must carry lang:enforcesFailureClass annotations"
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

        // FS4 and FS5 both name the projection stage. FS4: the serialization grammars are
        // lang:Grammar objects with exact emit/parse round-trips. FS5: the stage enforces
        // per-reading Invariant 3 (hard-fails on silent disambiguation), so a successful
        // build is itself the FS5 discharge. Both properties are asserted on one build.
        "pipeline::stages::lang_projection::build_corpus" => {
            let corpus = gmeow_pipeline::stages::lang_projection::build_corpus(Some(catalog))
                .expect("FS4/FS5: the projection corpus builds over the real grammars");
            let nt = String::from_utf8(corpus.ntriples).expect("corpus N-Triples is UTF-8");
            for needle in [
                "Grammar",            // FS4: lang:Grammar objects
                "ProjectionEmission", // FS4/FS5: one emission per grammar / per reading
                "ExactPreservation",  // FS4: the lossless crossing judgment
                "roundTripHolds",     // FS4: the measured emit/parse round-trip
            ] {
                assert!(
                    nt.contains(needle),
                    "FS4/FS5: the projection corpus must carry {needle}"
                );
            }
            assert!(
                !nt.contains("roundTripHolds \"false\""),
                "FS4: no Exact grammar emission may record a failing round-trip"
            );
        }

        other => panic!(
            "flagship {}: unknown lang:demonstratedByProducer identifier {other:?}",
            flagship.subject
        ),
    }
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
