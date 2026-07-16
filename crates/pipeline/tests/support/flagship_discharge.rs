// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SLICE-GENERIC flagship execution-discharge core.
//!
//! A grounding slice discharges its acceptance bar by EXECUTION: it declares a set of
//! `gmeow:FlagshipScenario` individuals in an acceptance manifest, each binding one scenario
//! to (a worked example, a counter-example, an enforcing failure class, a native producer).
//! The generic runner [`run_flagship_discharge`]:
//!
//! 1. **Parses the manifest** ([`parse_manifest`]) over the shared `gmeow:` predicates
//!    (`gmeow:demonstratedByExample`, `gmeow:guardedByCounterExample`,
//!    `gmeow:enforcesFailureClass`, `gmeow:demonstratedByProducer`) into a deterministically
//!    sorted `Vec<Flagship>` and asserts the declared count.
//! 2. **Runs the guard** — loads each counter-example MERGED with the slice's `module.ttl`,
//!    pushes it through BOTH the native structural lint ([`structural_lint_dataset`]) and the
//!    native SHACL engine ([`shacl_validate_dataset`]), and asserts the UNION of triggered
//!    failure classes IN THE SLICE'S NAMESPACE equals EXACTLY the one named by
//!    `gmeow:enforcesFailureClass` (set equality, not membership).
//! 3. **Checks the worked example** — the same two channels over the positive fixture, asserting
//!    NO slice-namespace failure fires.
//! 4. **Runs the producer** — invokes a per-slice `producer_assert` callback once per scenario,
//!    so each slice keeps its own producer-output assertions while sharing the discharge spine.
//! 5. **Checks counter-example depth** — parses the closed discharge marker and requires the
//!    per-slice negative callback to return matching structural or native execution evidence;
//!    native evidence is set-equal to the one declared failure class.
//!
//! The failure-class scanner and SHACL shape→class resolution are parameterized by the slice's
//! base IRI and short prefix, so `lang:`, `math:`, and `logic:` all drive the SAME core.

#![allow(dead_code)]

use std::collections::{BTreeSet, HashMap, HashSet, hash_map::Entry};
use std::path::{Path, PathBuf};

use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};
use gmeow_validate::lint::{LintConfig, structural_lint_dataset};
use gmeow_validate::store::{dataset_from_paths, parse_file_dataset, shacl_validate_dataset};
use purrdf::shapes::engine::parse_shapes;
use purrdf::{RdfDataset, RdfTerm};

define_diag_kind! {
    /// A flagship fixture or native negative execution failed before it could produce the
    /// semantic failure-class evidence the acceptance harness requires.
    pub struct FlagshipExecution { detail: String }
    code = "pipeline.test.flagship-execution";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "flagship execution failed: {}", detail;
}

/// Lift test-harness input/infrastructure failures onto the repository-wide diagnostic substrate.
pub fn flagship_error(detail: impl Into<String>) -> Diag {
    Diag::of_kind(FlagshipExecution {
        detail: detail.into(),
    })
}

/// The shared `gmeow:` namespace. The flagship-manifest PREDICATES (the acceptance-bar
/// wiring) and the shape→failure-class annotation predicate live here; the failure-class
/// VALUES they point at stay slice-namespaced (`lang:` / `math:` / `logic:`).
pub const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The identity of one grounding slice's discharge run.
///
/// A slice supplies its base IRI (the namespace its failure classes live in), its short
/// prefix (the `<prefix>:<Class>:` token the native lint emits), its on-disk slice root, and
/// the acceptance-manifest path relative to that root. `math:` and `logic:` construct their
/// own `SliceSpec` and call [`run_flagship_discharge`] with it.
pub struct SliceSpec {
    /// The slice base IRI, e.g. `https://blackcatinformatics.ca/lang/`. SHACL failure classes
    /// whose IRI starts with this are the slice-namespace failures the guard counts.
    pub slice_ns: &'static str,
    /// The slice short prefix, e.g. `lang`. The native lint emits a failure as the token
    /// `<prefix>:<CamelCaseClass>:`, so the scanner keys off this.
    pub slice_prefix: &'static str,
    /// The slice root directory (absolute), e.g. `<repo>/slices/grounding/lang`. Relative
    /// fixture paths in the manifest resolve against this, and its `module.ttl` / `shapes.ttl`
    /// are loaded here.
    pub slice_root: PathBuf,
    /// The acceptance manifest path, RELATIVE to `slice_root`,
    /// e.g. `examples/flagship-acceptance.ttl`.
    pub manifest_rel: &'static str,
}

/// One flagship binding, read from the manifest.
#[derive(Debug)]
pub struct Flagship {
    /// The flagship individual IRI (for diagnostics and per-scenario producer dispatch).
    pub subject: String,
    /// The absolute path to the `gmeow:demonstratedByExample` fixture.
    pub example: PathBuf,
    /// The absolute path to the `gmeow:guardedByCounterExample` fixture.
    pub counter_example: PathBuf,
    /// The local name of the `gmeow:enforcesFailureClass` the guard must raise.
    pub failure_class: String,
    /// The `gmeow:demonstratedByProducer` identifier string.
    pub producer: String,
    /// The declared depth at which the guarding counter-example is executed.
    pub counter_example_discharge: CounterExampleDischarge,
}

/// The closed `gmeow:CounterExampleDischarge` marker set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterExampleDischarge {
    /// The malformed fixture is checked only through structural/SHACL projection surfaces.
    Structural,
    /// The malformed fixture is additionally run through the native reasoning producer.
    ReasonerDriven,
}

/// What the per-slice counter-example callback actually executed.
#[derive(Debug, PartialEq, Eq)]
pub enum CounterExampleExecution {
    /// No native negative execution; the structural/SHACL proxy is the whole guard.
    StructuralProxy,
    /// Native execution completed and observed these typed failure classes.
    ReasonerDriven(BTreeSet<String>),
}

/// Enforce agreement between the ontology marker and the execution evidence. Reasoner-driven
/// evidence is set-equal to the single declared failure class; structural and native results are
/// never interchangeable.
pub fn assert_counterexample_depth(flagship: &Flagship, execution: CounterExampleExecution) {
    match (flagship.counter_example_discharge, execution) {
        (CounterExampleDischarge::Structural, CounterExampleExecution::StructuralProxy) => {}
        (
            CounterExampleDischarge::ReasonerDriven,
            CounterExampleExecution::ReasonerDriven(observed),
        ) => {
            let expected: BTreeSet<String> =
                std::iter::once(flagship.failure_class.clone()).collect();
            assert_eq!(
                observed, expected,
                "flagship {}: native counter-example execution must observe EXACTLY {{{}}}",
                flagship.subject, flagship.failure_class
            );
        }
        (declared, actual) => panic!(
            "flagship {}: counter-example marker/execution mismatch: declared {declared:?}, \
             executed {actual:?}",
            flagship.subject
        ),
    }
}

/// Insert one manifest value without silently accepting duplicate predicates.
fn insert_unique(
    values: &mut HashMap<String, String>,
    subject: &str,
    predicate: &str,
    value: String,
) {
    match values.entry(subject.to_owned()) {
        Entry::Vacant(slot) => {
            slot.insert(value);
        }
        Entry::Occupied(_) => {
            panic!("flagship {subject} has duplicate gmeow:{predicate}")
        }
    }
}

/// The per-scenario execution context handed to a slice's `producer_assert` callback: the
/// real in-memory slice catalog (discovered ONCE, exactly as the mappings stage discovers it)
/// and the repo root, so producers that ingest the source universe or read on-disk artifacts
/// run against the same tree the production path drives.
pub struct FlagshipCtx<'a> {
    /// The shared in-memory source catalog.
    pub catalog: &'a purrdf::slice::SliceCatalog,
    /// The repo root (`crates/pipeline/../..`).
    pub repo_root: PathBuf,
}

/// The repo root: `crates/pipeline/..` twice up, mirroring the in-crate stage tests so the
/// harness drives off the SAME slice tree the production path discovers.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root canonicalizes")
}

/// The projected shape surface carrying the derived `gmeow:FlagshipScenario-shape` (the flagship
/// cardinality gate, `sh:targetClass gmeow:FlagshipScenario`, whose `gmeow:enforcesFailureClass` is
/// `gmeow:UnwiredFlagshipScenario`). The shared wiring gate is authored declaratively in the logic
/// grounding slice and projected here; the root `shapes/gmeow-shapes.ttl` no longer holds it.
pub fn shared_shapes_path() -> PathBuf {
    repo_root().join("generated/shapes/validation-shapes.ttl")
}

/// The shared in-memory source catalog, discovered ONCE exactly as the mappings stage
/// discovers it, so producers ingest the real composed-source universe.
pub fn repo_catalog() -> purrdf::slice::SliceCatalog {
    purrdf::slice::SliceCatalog::discover(
        &repo_root().join("slices"),
        gmeow_pipeline::gmeow_slice_vocab(),
    )
    .expect("discover slice catalog")
}

/// The minimal lint config — the same shape [`gmeow_validate`]'s own integration tests build.
/// It carries no selector tokens or core-slice grading; the slice's structural checks it runs
/// are namespace-independent, so a bare config exercises them fully.
pub fn minimal_lint_config() -> LintConfig {
    LintConfig {
        namespace: GMEOW_NS.into(),
        ontology_iri: "https://blackcatinformatics.ca/gmeow".into(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: HashSet::new(),
    }
}

/// The local name of an IRI (`…/lang/UnhashableSurface` → `UnhashableSurface`).
pub fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned()
}

/// Parse the acceptance manifest into the flagship bindings, resolving each relative fixture
/// path against `spec.slice_root`, sorted deterministically by subject, asserting exactly
/// `expected_count` scenarios are declared.
pub fn parse_manifest(spec: &SliceSpec, expected_count: usize) -> Vec<Flagship> {
    let manifest = spec.slice_root.join(spec.manifest_rel);
    let ds = parse_file_dataset(&manifest).expect("manifest parses");
    let base = &spec.slice_root;

    // Collect, per flagship subject, each bound value.
    let mut example: HashMap<String, String> = HashMap::new();
    let mut counter: HashMap<String, String> = HashMap::new();
    let mut class: HashMap<String, String> = HashMap::new();
    let mut producer: HashMap<String, String> = HashMap::new();
    let mut discharge: HashMap<String, String> = HashMap::new();
    let mut scenarios = BTreeSet::new();

    // The manifest predicate IRIs, built ONCE. Hoisted to the shared gmeow: vocabulary; only
    // the enforcesFailureClass VALUE stays slice-namespaced.
    let pred_example = format!("{GMEOW_NS}demonstratedByExample");
    let pred_counter = format!("{GMEOW_NS}guardedByCounterExample");
    let pred_class = format!("{GMEOW_NS}enforcesFailureClass");
    let pred_producer = format!("{GMEOW_NS}demonstratedByProducer");
    let pred_discharge = format!("{GMEOW_NS}counterExampleDischarge");
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let flagship_class = format!("{GMEOW_NS}FlagshipScenario");

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
        if pred == RDF_TYPE && obj_iri.as_deref() == Some(flagship_class.as_str()) {
            scenarios.insert(subject.clone());
        } else if pred == pred_example {
            insert_unique(
                &mut example,
                subject,
                "demonstratedByExample",
                obj_literal.expect("example is a literal path"),
            );
        } else if pred == pred_counter {
            insert_unique(
                &mut counter,
                subject,
                "guardedByCounterExample",
                obj_literal.expect("counter-example is a literal path"),
            );
        } else if pred == pred_class {
            insert_unique(
                &mut class,
                subject,
                "enforcesFailureClass",
                local_name(&obj_iri.expect("failure class is an IRI")),
            );
        } else if pred == pred_producer {
            insert_unique(
                &mut producer,
                subject,
                "demonstratedByProducer",
                obj_literal.expect("producer identifier is a literal"),
            );
        } else if pred == pred_discharge {
            insert_unique(
                &mut discharge,
                subject,
                "counterExampleDischarge",
                obj_iri.expect("counter-example discharge marker is an IRI"),
            );
        }
    }

    let mut flagships: Vec<Flagship> = scenarios
        .iter()
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
                counter_example_discharge: match discharge.get(subject).map(String::as_str) {
                    Some(marker) if marker == format!("{GMEOW_NS}structuralDischarge") => {
                        CounterExampleDischarge::Structural
                    }
                    Some(marker) if marker == format!("{GMEOW_NS}reasonerDrivenDischarge") => {
                        CounterExampleDischarge::ReasonerDriven
                    }
                    Some(marker) => panic!(
                        "flagship {subject} has unknown gmeow:counterExampleDischarge marker <{marker}>"
                    ),
                    None => panic!(
                        "flagship {subject} missing gmeow:counterExampleDischarge"
                    ),
                },
            }
        })
        .collect();
    flagships.sort_by(|a, b| a.subject.cmp(&b.subject));
    assert_eq!(
        flagships.len(),
        expected_count,
        "the acceptance manifest declares exactly {expected_count} gmeow:FlagshipScenario individuals"
    );
    flagships
}

/// The set of slice failure-class local names a lint report raises. A slice failure is emitted
/// as the substring token `<prefix>:<ClassLocalName>:` (a CamelCase class immediately followed
/// by a colon, e.g. `lang:ExactPreservationViolated: …`). Non-class `<prefix>:` mentions in a
/// message — a property CURIE, or a full `…/<prefix>/…` IRI — never match, because they are not
/// `<prefix>:<Uppercase…>:`.
///
/// The scan is multibyte-safe: `match_indices` yields token STARTS, so a `<prefix>:<non-ascii>`
/// token reads an empty ascii prefix and is skipped rather than sliced at a non-char boundary.
pub fn native_failure_classes(errors: &[String], prefix: &str) -> HashSet<String> {
    let token = format!("{prefix}:");
    let mut out = HashSet::new();
    for error in errors {
        for (idx, _) in error.match_indices(&token) {
            let after = &error[idx + token.len()..];
            // The ascii-alphanumeric run is pure ASCII, so its byte length is a char boundary;
            // the char immediately after it decides whether this is `<prefix>:<Class>:`.
            let local: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if local.starts_with(|c: char| c.is_ascii_uppercase())
                && after
                    .strip_prefix(local.as_str())
                    .is_some_and(|rest| rest.starts_with(':'))
            {
                out.insert(local);
            }
        }
    }
    out
}

/// Build the `<node-shape-IRI> -> failure-class-FULL-IRI` map from the given shapes graphs.
///
/// Each node shape carries `<shape> gmeow:enforcesFailureClass <class>` (the annotation
/// predicate is hoisted to the shared gmeow: vocabulary); the native SHACL engine stamps a
/// violation's `source_shape` with its parent NODE shape IRI (property constraints inherit the
/// node shape's id), so this IRI-keyed map resolves every violation. Building it from both a
/// slice's `shapes.ttl` and the shared `gmeow-shapes.ttl` lets one map resolve slice failures
/// (e.g. `lang:UnhashableSurface`) AND the shared `gmeow:UnwiredFlagshipScenario`.
pub fn shape_class_map(shape_paths: &[PathBuf]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let pred_class = format!("{GMEOW_NS}enforcesFailureClass");
    for path in shape_paths {
        let ds = parse_file_dataset(path)
            .unwrap_or_else(|e| panic!("shapes graph {} parses: {e}", path.display()));
        for quad in ds.owned_quads() {
            if quad.predicate == pred_class
                && let (RdfTerm::Iri(shape), RdfTerm::Iri(class)) = (&quad.subject, &quad.object)
            {
                map.insert(shape.clone(), class.clone());
            }
        }
    }
    assert!(
        !map.is_empty(),
        "the shapes graphs must carry gmeow:enforcesFailureClass annotations"
    );
    map
}

/// The set of slice-namespace failure-class local names the native SHACL engine raises over
/// `ds`, resolved from each violation's `source_shape` through the shape→class map and FILTERED
/// to classes whose IRI starts with `slice_ns` — so a shared `gmeow:` class never counts as a
/// slice failure.
pub fn shacl_slice_failures(
    ds: &RdfDataset,
    shapes: &purrdf::shapes::shapes::Shapes,
    shape_class: &HashMap<String, String>,
    slice_ns: &str,
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
        if let Some(class_iri) = shape_class.get(shape_iri)
            && class_iri.starts_with(slice_ns)
        {
            out.insert(local_name(class_iri));
        }
    }
    out
}

/// Run BOTH channels over a fixture and return the union of triggered slice-namespace failure
/// classes.
///
/// The fixture is validated MERGED with the slice's `module.ttl` — exactly the union the slice's
/// own conformance harness validates. The module graph supplies the vocabulary typing and
/// subclass axioms a counter-example legitimately relies on, so an `sh:class`/`sh:nodeKind`
/// constraint is discharged by the vocabulary and NOT spuriously counted against a fixture that
/// isolates a different violation.
pub fn triggered_slice_failures(
    spec: &SliceSpec,
    fixture: &Path,
    shapes: &purrdf::shapes::shapes::Shapes,
    shape_class: &HashMap<String, String>,
) -> HashSet<String> {
    let module = spec.slice_root.join("module.ttl");
    let ds = dataset_from_paths(&[module, fixture.to_path_buf()])
        .unwrap_or_else(|e| panic!("fixture {} + module parse: {e}", fixture.display()));
    let lint = structural_lint_dataset(&ds, &minimal_lint_config());
    let mut union = native_failure_classes(&lint.errors(), spec.slice_prefix);
    union.extend(shacl_slice_failures(
        &ds,
        shapes,
        shape_class,
        spec.slice_ns,
    ));
    union
}

/// Discharge a slice's flagship acceptance bar by EXECUTION.
///
/// For each of the `expected_count` `gmeow:FlagshipScenario` individuals in the slice manifest,
/// this asserts (1) the counter-example raises EXACTLY the declared slice failure class over the
/// union of the native lint and native SHACL channels, (2) the worked example raises NONE, then
/// (3) invokes `producer_assert` so the slice discharges its own producer-output claims, and (4)
/// requires the actual counter-example execution depth to agree with its closed marker.
///
/// `producer_assert` receives the parsed [`Flagship`] and a shared [`FlagshipCtx`] (the real
/// slice catalog and repo root, built once for the whole run).
pub fn run_flagship_discharge(
    spec: &SliceSpec,
    expected_count: usize,
    producer_assert: &dyn Fn(&Flagship, &FlagshipCtx<'_>),
) {
    run_flagship_discharge_with_counterexample(spec, expected_count, producer_assert, &|_, _| {
        Ok(CounterExampleExecution::StructuralProxy)
    });
}

/// Discharge a slice's flagship acceptance bar with an explicit counter-example execution
/// callback. The callback returns an error for parse, capability, budget, or infrastructure
/// failures; only a successful [`CounterExampleExecution::ReasonerDriven`] result can satisfy a
/// `gmeow:reasonerDrivenDischarge` marker.
pub fn run_flagship_discharge_with_counterexample(
    spec: &SliceSpec,
    expected_count: usize,
    producer_assert: &dyn Fn(&Flagship, &FlagshipCtx<'_>),
    counterexample_assert: &dyn Fn(
        &Flagship,
        &FlagshipCtx<'_>,
    ) -> gmeow_errors::Result<CounterExampleExecution>,
) {
    let flagships = parse_manifest(spec, expected_count);

    // Parse the enforcing SHACL surface once. A slice still being migrated reads its local
    // `shapes.ttl`; a fully migrated slice reads the canonical generated validation and
    // procedural projections instead (Principle 17). Deleting a proven-redundant local file must
    // not delete the negative-test channel that its projection now owns.
    let legacy_shapes = spec.slice_root.join("shapes.ttl");
    let shape_paths = if legacy_shapes.is_file() {
        vec![legacy_shapes]
    } else {
        vec![
            repo_root().join("generated/shapes/validation-shapes.ttl"),
            repo_root().join("generated/shapes/constraint-shapes.ttl"),
            repo_root().join("generated/shapes/procedural-constraints.ttl"),
        ]
    };
    let shapes_text = shape_paths
        .iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("read enforcing shapes {}: {e}", path.display()))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let shapes = parse_shapes(&shapes_text).expect("slice shapes parse");

    // The shape→failure-class map, resolved from the slice shapes AND the shared gmeow-shapes,
    // so both slice failures and the shared unwired class resolve through one map.
    let mut shape_class_paths = shape_paths;
    shape_class_paths.push(shared_shapes_path());
    let shape_class = shape_class_map(&shape_class_paths);

    // Producers ingest the real slice catalog and repo tree; discover them once.
    let catalog = repo_catalog();
    let ctx = FlagshipCtx {
        catalog: &catalog,
        repo_root: repo_root(),
    };

    for flagship in &flagships {
        // ---- (1) Executed guard: the counter-example raises EXACTLY its failure class. ----
        let triggered =
            triggered_slice_failures(spec, &flagship.counter_example, &shapes, &shape_class);
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

        // ---- (2) Clean worked example: NO slice failure class fires. ----
        let clean = triggered_slice_failures(spec, &flagship.example, &shapes, &shape_class);
        assert!(
            clean.is_empty(),
            "flagship {}: the worked example {} must be clean, but raised {:?}",
            flagship.subject,
            flagship.example.display(),
            clean
        );

        // ---- (3) Executed producer: the slice asserts its own output structure. ----
        producer_assert(flagship, &ctx);

        // ---- (4) Honest depth: marker and actual negative execution agree exactly. ----
        let execution = counterexample_assert(flagship, &ctx).unwrap_or_else(|detail| {
            panic!(
                "flagship {}: native counter-example execution failed as infrastructure/input, \
                 not as the declared failure class: {detail}",
                flagship.subject
            )
        });
        assert_counterexample_depth(flagship, execution);
    }
}
