// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 bindings for the pipeline — the `gmeow_native.pipeline` submodule (#861).
//!
//! Only this module imports `pyo3`, gated by the `python` feature, so the engine
//! core stays PyO3-free. `run_pipeline` is the single Python surface that
//! replaces the Python build orchestrator: it runs the WHOLE dogfooded DAG
//! single-pass (`crate::run::run_full`) and either WRITES every committed
//! artifact (regenerate) or COMPARES each against the committed bytes and reports
//! drift (check).

use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyModule};

use gmeow_diagnostics::py::PyReport;

use crate::run::{run_full, RunMode};
use crate::scoreboards;
use crate::transform::{self, CellInput, DerivedRowNative, TransformReportNative};
use crate::up_projection::{self, LiftMap, UpProjectionReport};

/// Run the full dogfooded build single-pass.
///
/// * `root` — the repository root.
/// * `jobs` — per-level parallelism budget (clamped to `>= 1` internally).
/// * `check` — when `true`, COMPARE each produced artifact against the committed
///   bytes and report drift (no writes); when `false`, WRITE every produced
///   artifact to disk (regenerate).
///
/// Returns a summary `dict`:
///
/// ```text
/// {
///   "mode":       "check" | "regenerate",
///   "produced":   int,        # committed-artifact paths the run produced
///   "reproduced": int,        # reproduced byte/iso-for-byte (check) / reconciled
///   "written":    int,        # regenerate only: paths whose bytes changed
///   "skipped_writes": int,    # regenerate only: paths already up to date
///   "drifted":    list[str],  # drifted committed paths (check); empty on regen
///   "timings":    list[{phase, elapsed_ms, metadata}],
///   "findings":   list[{severity, code, message}],  # drift / write findings
///   "clean":      bool,       # True ⇒ zero drift, full parity
/// }
/// ```
///
/// In CHECK mode the caller fails the gate when `drifted` is non-empty (or
/// `clean` is `False`). A [`crate::error::PipelineError`] (a hard build failure —
/// a malformed DAG, an unknown stage impl, an I/O error) maps to `ValueError`.
#[pyfunction]
#[pyo3(signature = (root, jobs, check))]
fn run_pipeline(py: Python<'_>, root: String, jobs: usize, check: bool) -> PyResult<Py<PyAny>> {
    let mode = if check {
        RunMode::Check
    } else {
        RunMode::Regenerate
    };
    let report = run_full(Path::new(&root), jobs, mode)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let out = PyDict::new(py);
    out.set_item(
        "mode",
        match report.mode {
            RunMode::Check => "check",
            RunMode::Regenerate => "regenerate",
        },
    )?;
    out.set_item("produced", report.produced)?;
    out.set_item("reproduced", report.reproduced)?;
    out.set_item("written", report.written)?;
    out.set_item("skipped_writes", report.skipped_writes)?;
    out.set_item("clean", report.is_clean())?;

    let drifted = PyList::empty(py);
    for path in &report.drifted {
        drifted.append(path)?;
    }
    out.set_item("drifted", drifted)?;

    let findings = PyList::empty(py);
    for finding in &report.findings {
        let f = PyDict::new(py);
        f.set_item("severity", finding.severity.as_str())?;
        f.set_item("code", &finding.code)?;
        f.set_item("message", &finding.message)?;
        findings.append(f)?;
    }
    out.set_item("findings", findings)?;

    let timings = PyList::empty(py);
    for timing in &report.timings {
        let t = PyDict::new(py);
        t.set_item("phase", &timing.phase)?;
        t.set_item("elapsed_ms", timing.elapsed_ms)?;
        t.set_item("metadata", timing.metadata.as_deref())?;
        timings.append(t)?;
    }
    out.set_item("timings", timings)?;

    Ok(out.into_any().unbind())
}

/// Compile only the statement layer through the native Rust statements stage.
///
/// This is an interface hook for developer feedback and oracle checks. The
/// compiler authority remains [`crate::stages::statements::compile_statements`];
/// Python receives the already-rendered OWL downcast and RDF 1.2 lead strings.
#[pyfunction]
#[pyo3(signature = (root))]
fn compile_statements(py: Python<'_>, root: String) -> PyResult<Py<PyAny>> {
    let (owl_ttl, rdf12_ttl) = crate::stages::statements::compile_statements(Path::new(&root))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let out = PyDict::new(py);
    out.set_item("owl_ttl", owl_ttl)?;
    out.set_item("rdf12_ttl", rdf12_ttl)?;
    Ok(out.into_any().unbind())
}

/// Compile statements and return the structured feedback diagnostics report.
///
/// Python supplies `ontology_nt` because it already owns the merged-ontology
/// loading surface; the compiler, invariant checks, lossless check, and
/// `statement-compile.dsl-error` mapping remain Rust-owned.
#[pyfunction]
#[pyo3(signature = (root, ontology_nt))]
fn compile_statements_report(
    py: Python<'_>,
    root: String,
    ontology_nt: String,
) -> PyResult<Py<PyAny>> {
    let report =
        crate::stages::statements::compile_diagnostics_report(Path::new(&root), &ontology_nt);
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Compile mappings and return the structured feedback diagnostics report.
///
/// The compiler, SSSOM validation, and projection linting remain Rust-owned.
/// Python receives only the canonical report object for CLI/SARIF/HTML folding.
#[pyfunction]
#[pyo3(signature = (root))]
fn compile_mappings_report(py: Python<'_>, root: String) -> PyResult<Py<PyAny>> {
    let report = crate::stages::mappings::compile_diagnostics_report(Path::new(&root));
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Serialize N-Quads-star bytes to RDF-1.2-star JSON-LD or YAML-LD-star.
///
/// * `nquads_bytes` — a UTF-8 N-Quads-star document (plain N-Quads is accepted).
/// * `format` — `"jsonld"` for JSON-LD-star, `"yamlld"` for YAML-LD-star.
///
/// Returns the serialized bytes. This is the Python surface for the serializer
/// used by the `stage-export-yaml-ld` leaf (#699).
#[pyfunction]
#[pyo3(signature = (nquads_bytes, format = "jsonld"))]
fn serialize_yaml_ld(py: Python<'_>, nquads_bytes: &[u8], format: &str) -> PyResult<Py<PyAny>> {
    let dataset =
        gmeow_rdf::dataset_from_bytes(nquads_bytes, gmeow_rdf::NativeRdfFormat::NQuads)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("parse N-Quads: {e}")))?;
    let text = match format {
        "jsonld" => crate::stages::yaml_ld::serialize_graph(&dataset)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        "yamlld" => crate::stages::yaml_ld::serialize_graph_yaml(&dataset, None)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown format {format:?}; expected 'jsonld' or 'yamlld'"
            )))
        }
    };
    Ok(PyBytes::new(py, text.as_bytes()).into_any().unbind())
}

/// Universal RDF-1.2 transcode: convert `data` from one codec to another,
/// recording loss (#671).
///
/// * `data` — the source document bytes.
/// * `from_` / `to` — codec names (see `crate::transcode::Codec::from_cli_str`):
///   `turtle`, `ntriples`, `nquads`, `trig`, `jsonld`, `jsonld-star`,
///   `yaml-ld-star`, `rdfxml`, `gts`, `owl-rdf12`, and the projection targets
///   `owl-dl`, `owl-el`, `datalog`, `n3`, `nemo`, `gufo`, `canonical-rdf12`.
/// * `base_iri` — optional base IRI for relative-IRI resolution.
///
/// Returns `(output_bytes, realized_loss_json)`. Hard-fails (`ValueError`) on an
/// unknown codec, a non-invertible projection source, or an undecodable input
/// codec (JSON-LD-star / YAML-LD-star are output-only).
#[pyfunction]
#[pyo3(signature = (data, from_, to, base_iri = None))]
fn transcode(
    py: Python<'_>,
    data: &[u8],
    from_: &str,
    to: &str,
    base_iri: Option<String>,
) -> PyResult<(Py<PyBytes>, String)> {
    use crate::transcode::{realized_loss_json, transcode as run_transcode, Codec};
    let from = Codec::from_cli_str(from_)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let to = Codec::from_cli_str(to)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let output = run_transcode(data, from, to, base_iri.as_deref())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let loss = realized_loss_json(&output.realized);
    Ok((PyBytes::new(py, &output.bytes).unbind(), loss))
}

/// Parse JSON-LD-star bytes and downcast RDF 1.2 quoted triples to GMEOW
/// statement-metadata N-Quads.
///
/// The GMEOW JSON-LD-star emitter represents statement metadata with the
/// `@annotation` idiom, which parses to `?r rdf:reifies <<( ?s ?p ?o )>>`
/// plus annotation triples on `?r`. Those quoted triples cannot be carried
/// through the rdflib-compat up-projection lane, so this function re-expresses
/// each annotation as a native GMEOW statement-metadata cell:
///
/// ```turtle
/// ?r a gmeow:StatementMetadata ;
///    gmeow:qSubject ?s ;
///    gmeow:qPredicate ?p ;
///    gmeow:qObject ?o | gmeow:qObjectLiteral ?o ;
///    <annotation-pred> <annotation-value> .
/// ```
///
/// Returns UTF-8 N-Quads bytes with no quoted triple terms. Hard-fails on
/// unsupported JSON-LD features.
#[pyfunction]
#[pyo3(signature = (json_bytes))]
fn parse_jsonld_star_to_gmeow_statement_metadata_nquads(
    py: Python<'_>,
    json_bytes: &[u8],
) -> PyResult<Py<PyAny>> {
    let nquads = crate::stages::yaml_ld::jsonld_star_to_gmeow_statement_metadata_nquads(json_bytes)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, nquads.as_bytes()).into_any().unbind())
}

/// Parse YAML-LD-star bytes and downcast RDF 1.2 quoted triples to GMEOW
/// statement-metadata N-Quads.
///
/// Routes the YAML-LD-star document through the Rust native JSON-LD-star
/// downcast (anchors/aliases hard-fail), so the rdflib-compat up-projection lane
/// receives quoted-triple-free N-Quads (#699). The Python YAML codec is retired
/// in favor of this single Rust authority.
#[pyfunction]
#[pyo3(signature = (yaml_bytes))]
fn parse_yaml_ld_star_to_gmeow_statement_metadata_nquads(
    py: Python<'_>,
    yaml_bytes: &[u8],
) -> PyResult<Py<PyAny>> {
    let nquads =
        crate::stages::yaml_ld::yaml_ld_star_to_gmeow_statement_metadata_nquads(yaml_bytes)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, nquads.as_bytes()).into_any().unbind())
}

/// Verify a serialized RDF-1.2-star document round-trips isomorphic to its
/// source N-Quads-star.
///
/// * `nquads_bytes` — the original UTF-8 N-Quads-star document.
/// * `star_bytes` — the serialized RDF-1.2-star bytes to verify.
/// * `format` — `"jsonld"` for JSON-LD-star, `"yamlld"` for YAML-LD-star.
///
/// Returns `True` iff the re-parsed dataset is RDFC-1.0 canonical-equal to the
/// original. This is the Rust authority for the build-time serialization
/// isomorphism gate (#699), replacing the Python `_round_trip_star`.
#[pyfunction]
#[pyo3(signature = (nquads_bytes, star_bytes, format))]
fn roundtrip_isomorphic(nquads_bytes: &[u8], star_bytes: &[u8], format: &str) -> PyResult<bool> {
    crate::stages::yaml_ld::roundtrip_isomorphic(nquads_bytes, star_bytes, format)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Fold release evidence into a SIGNED `gmeow.gts` bundle (#673, §18).
///
/// This is the thin marshalling surface for the standalone release-as-evidence
/// fold ([`crate::stages::release::fold_release_bundle`]). ALL fold / sign /
/// attestation logic is Rust; Python only reads files, the signing key, and the
/// armor, then calls here.
///
/// * `snapshot_bytes` — the committed *unsigned* `gmeow.gts` (NEVER mutated).
/// * `evidence` — a list of `(data, media_type, attestation_type_iri, rep,
///   subject_label)` rows; the caller read each artifact file (a missing file is
///   a hard failure at the CLI before this is called).
/// * `attester_iri` — the release-lane agent IRI.
/// * `issued_at` — the INJECTED ISO-8601 release timestamp (determinism, §18).
/// * `release_subject_iri` — the IRI naming the signed release bundle.
/// * `signer_secret_armor` — the ASCII-armored unencrypted Ed25519 OpenPGP
///   SECRET key (the SIGN_KEY material); parsed to a `SigningKey` + kid HERE.
/// * `public_key_armor` — the ASCII-armored Ed25519 OpenPGP PUBLIC certificate
///   carried in the bundle's transport-key meta frame.
///
/// Returns the signed bundle bytes; the caller writes them to `--out`.
#[pyfunction]
#[pyo3(signature = (
    snapshot_bytes,
    evidence,
    attester_iri,
    issued_at,
    release_subject_iri,
    signer_secret_armor,
    public_key_armor,
))]
#[allow(clippy::too_many_arguments)]
fn fold_release_bundle_native(
    py: Python<'_>,
    snapshot_bytes: &[u8],
    evidence: Vec<(Vec<u8>, String, String, String, String)>,
    attester_iri: String,
    issued_at: String,
    release_subject_iri: String,
    signer_secret_armor: String,
    public_key_armor: String,
) -> PyResult<Py<PyAny>> {
    use crate::stages::release::{build_coherence_evidence, fold_release_bundle, EvidenceInput};

    // Load the Ed25519 signing material from the armored secret key in Rust
    // (no key handling in Python beyond reading the file bytes).
    let signer =
        gmeow_gts::openpgp::parse_secret_signing_key(&signer_secret_armor, None).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("parsing signing secret key: {e}"))
        })?;
    let (signing_key, kid) = signer.into_parts();
    let secret = signing_key.to_bytes();

    let inputs: Vec<EvidenceInput> = evidence
        .into_iter()
        .map(
            |(data, media_type, attestation_type_iri, rep, subject_label)| EvidenceInput {
                data,
                media_type,
                attestation_type_iri,
                rep,
                subject_label,
            },
        )
        .collect();

    // Own the snapshot bytes so the heavy fold runs off the GIL.
    let snapshot = snapshot_bytes.to_vec();
    let bytes = py
        .detach(move || {
            // Auto-include the scoped coherence certificate as one more signed
            // evidence artifact over the SAME snapshot being folded + signed, so it
            // rides the existing Ed25519 bundle signature (no new signing step).
            let mut inputs = inputs;
            inputs.push(build_coherence_evidence(&snapshot, &issued_at)?);
            fold_release_bundle(
                &snapshot,
                inputs,
                &attester_iri,
                &issued_at,
                &release_subject_iri,
                secret,
                &kid,
                &public_key_armor,
            )
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, &bytes).into_any().unbind())
}

/// Marshalled shape of a [`crate::stages::release::ReleaseVerifyReport`] for
/// Python: a `(signed, valid, kid, fingerprint, artifacts_verified)` tuple.
type ReleaseVerifyTuple = (usize, usize, Option<String>, Option<String>, usize);

/// Verify a signed release-evidence bundle (#673, §18) — the consumer half of
/// the fold and the body of `make verify-release`.
///
/// Thin marshalling surface for [`crate::stages::release::verify_release_bundle`]:
/// it does the COSE signature + trust-policy check AND walks the
/// `graph/attestations` frames, hard-failing if any attested artifact's bytes
/// are absent. Returns `(signed, valid, kid, fingerprint, artifacts_verified)`;
/// a verification failure raises `ValueError` (no silent pass).
///
/// * `bundle_bytes` — the signed bundle to verify.
/// * `expected_public_armor` — optional out-of-band trusted public key; when
///   present the signature is checked against it, not just the embedded key.
#[pyfunction]
#[pyo3(signature = (bundle_bytes, expected_public_armor=None))]
fn verify_release_bundle_native(
    py: Python<'_>,
    bundle_bytes: &[u8],
    expected_public_armor: Option<String>,
) -> PyResult<ReleaseVerifyTuple> {
    use crate::stages::release::verify_release_bundle;

    let bundle = bundle_bytes.to_vec();
    let report = py
        .detach(move || verify_release_bundle(&bundle, expected_public_armor.as_deref()))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok((
        report.signed,
        report.valid,
        report.kid,
        report.fingerprint,
        report.artifacts_verified,
    ))
}

/// Run the native correspondence-soundness pass (the five alignment checks + the two
/// FnO back-end checks, incl. the sole native enforcer of Constitution Principle 5) over
/// the committed `generated/` tree at `root`, returning one dict per problem.
///
/// Each dict carries the common `{severity, code, message, check, instance}` fields plus
/// optional `{subject_id, predicate_id, object_id}` CURIEs for alignment-row findings. An
/// empty list means the correspondence stack and SSSOM alignments are internally
/// consistent.
///
/// `allow_network` (default `false`) permits live fetching of missing target-axiom
/// snapshots.
///
/// # Errors
///
/// Raises `ValueError` on any missing/unparsable required source (a committed artifact,
/// the ontology, an SSSOM source) — no degraded fallback.
#[pyfunction]
#[pyo3(signature = (root, allow_network = false))]
fn lint_projection<'py>(
    py: Python<'py>,
    root: &str,
    allow_network: bool,
) -> PyResult<Vec<Bound<'py, PyDict>>> {
    let problems = crate::stages::correspondence_soundness::lint_correspondence_soundness(
        Path::new(root),
        allow_network,
    )
    .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("projection lint failed: {e}")))?;
    problems
        .into_iter()
        .map(|d| {
            let dict = PyDict::new(py);
            dict.set_item("severity", d.severity)?;
            dict.set_item("code", d.code)?;
            dict.set_item("message", d.message)?;
            dict.set_item("check", d.check)?;
            dict.set_item("instance", d.instance)?;
            dict.set_item("subject_id", d.subject_id)?;
            dict.set_item("predicate_id", d.predicate_id)?;
            dict.set_item("object_id", d.object_id)?;
            Ok(dict)
        })
        .collect()
}

/// Expose alignment policy constants from the Rust authority.
///
/// Python callers use this only to filter the combined `lint_projection` stream and to
/// keep saturation's strong-predicate gate in lockstep with the native pass. The predicate
/// sets are sourced from the correspondence-soundness module (the single authority).
#[pyfunction]
fn alignment_policy<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    use gmeow_logic_compile::projections::correspondence_soundness as soundness;
    let dict = PyDict::new(py);
    dict.set_item(
        "alignment_checks",
        vec![
            "inverse-direction",
            "domain-range",
            "property-character",
            "equivalence-collapse",
            "dc-refinement",
            "dc-hand-authored",
        ],
    )?;
    dict.set_item(
        "strong_class_predicates",
        soundness::STRONG_CLASS_PREDICATES.to_vec(),
    )?;
    dict.set_item(
        "strong_property_predicates",
        soundness::STRONG_PROPERTY_PREDICATES.to_vec(),
    )?;
    Ok(dict)
}

/// Classify one SSSOM row for the native up-projection audit.
#[pyfunction]
#[pyo3(signature = (subject_id, predicate_id, object_id))]
fn up_projection_classify_sssom(
    subject_id: String,
    predicate_id: String,
    object_id: String,
) -> PyResult<(String, String, String)> {
    let class = up_projection::classify_sssom(&subject_id, &predicate_id, &object_id);
    Ok((class.bucket, class.gmeow, class.target))
}

/// Compute the best combined up-projection class for one target term.
#[pyfunction]
#[pyo3(signature = (term, sssom, structural))]
fn up_projection_combined_class(
    term: String,
    sssom: std::collections::BTreeMap<String, String>,
    structural: std::collections::BTreeMap<String, String>,
) -> PyResult<String> {
    Ok(up_projection::combined_class(&term, &sssom, &structural))
}

/// Build the native lift map from serialized SSSOM, projection TTL, and ontology NT.
#[pyfunction]
#[pyo3(signature = (sssom_texts, projection_ttls, ontology_nt))]
fn up_projection_build_lift_map(
    py: Python<'_>,
    sssom_texts: Vec<String>,
    projection_ttls: Vec<String>,
    ontology_nt: String,
) -> PyResult<Py<PyAny>> {
    let lift = py
        .detach(move || up_projection::build_lift_map(&sssom_texts, &projection_ttls, &ontology_nt))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    lift_map_to_py(py, &lift)
}

/// Up-project N-Triples through the native Rust kernel.
#[pyfunction]
#[pyo3(signature = (source_nt, sssom_texts, projection_ttls, ontology_nt, descend = false))]
fn up_projection_project_nt(
    py: Python<'_>,
    source_nt: String,
    sssom_texts: Vec<String>,
    projection_ttls: Vec<String>,
    ontology_nt: String,
    descend: bool,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            up_projection::up_project_nt(
                &source_nt,
                &sssom_texts,
                &projection_ttls,
                &ontology_nt,
                descend,
            )
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    up_projection_report_to_py(py, &report)
}

/// Run the gate-derived up-projection invertibility audit over the corpus and render the
/// committed Markdown report. The corpus is supplied as Turtle text (`(name, ttl)`); the
/// TTL→N-Triples conversion happens natively (no rdflib), as does the whole gate evaluation.
/// Returns the rendered Markdown plus the gate-verdict ledger counts.
#[pyfunction]
#[pyo3(signature = (sssom_texts, projection_ttls, corpus_ttls))]
fn up_projection_gate_audit(
    py: Python<'_>,
    sssom_texts: Vec<String>,
    projection_ttls: Vec<String>,
    corpus_ttls: Vec<(String, String)>,
) -> PyResult<Py<PyAny>> {
    let (ledger, markdown) = py
        .detach(move || {
            let mut corpus_nts = Vec::with_capacity(corpus_ttls.len());
            for (name, ttl) in &corpus_ttls {
                let nt = up_projection::ttl_to_nt(ttl)
                    .map_err(|e| format!("corpus {name} ttl→nt: {e}"))?;
                corpus_nts.push((name.clone(), nt));
            }
            let ledger = crate::up_projection_gates::gate_derived_audit(
                &sssom_texts,
                &projection_ttls,
                &corpus_nts,
            )?;
            let markdown = crate::up_projection_report::render_audit_markdown(&ledger);
            Ok::<_, String>((ledger, markdown))
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    audit_ledger_to_py(py, &ledger, &markdown)
}

/// Resolve one context-aware up-projection candidate through the native descent index.
#[pyfunction]
#[pyo3(signature = (predicate, subject_types, sssom_texts, projection_ttls, ontology_nt))]
fn up_projection_resolve_context(
    py: Python<'_>,
    predicate: String,
    subject_types: Vec<String>,
    sssom_texts: Vec<String>,
    projection_ttls: Vec<String>,
    ontology_nt: String,
) -> PyResult<Py<PyAny>> {
    let subject_types: std::collections::BTreeSet<String> = subject_types.into_iter().collect();
    let resolved = py
        .detach(move || {
            up_projection::resolve_context_candidate(
                &predicate,
                &subject_types,
                &sssom_texts,
                &projection_ttls,
                &ontology_nt,
            )
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let Some((gmeow, context_type, relation, confidence)) = resolved else {
        return Ok(py.None());
    };
    let out = PyDict::new(py);
    out.set_item("gmeow", gmeow)?;
    out.set_item("context_type", context_type)?;
    out.set_item("relation", relation)?;
    out.set_item("confidence", confidence)?;
    Ok(out.into_any().unbind())
}

/// Run only the hand-authored/native reverse-projection minting layer.
#[pyfunction]
#[pyo3(signature = (source_nt))]
fn up_projection_reverse_nt(py: Python<'_>, source_nt: String) -> PyResult<Py<PyAny>> {
    let graph_nt = py
        .detach(move || up_projection::reverse_nt(&source_nt))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, graph_nt.as_bytes()).into_any().unbind())
}

/// Deterministically skolemize an N-Triples graph through the native transform core.
#[pyfunction]
#[pyo3(signature = (source_nt))]
fn transform_skolemize_nt(py: Python<'_>, source_nt: String) -> PyResult<Py<PyAny>> {
    let graph_nt = py
        .detach(move || transform::skolemize_nt(&source_nt))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, graph_nt.as_bytes()).into_any().unbind())
}

/// Compute E(G) through the native transform core.
#[pyfunction]
#[pyo3(signature = (abox_nt, ontology_nt, cells, denied))]
fn transform_saturate_nt(
    py: Python<'_>,
    abox_nt: String,
    ontology_nt: String,
    cells: Vec<(String, String, String, String, String)>,
    denied: Vec<(String, String, String)>,
) -> PyResult<Py<PyAny>> {
    let cells = cell_inputs(cells);
    let rows = py
        .detach(move || transform::saturate_nt(&abox_nt, &ontology_nt, &cells, &denied))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    derived_rows_to_py(py, &rows)
}

/// Run MAXIMAL(G) through the native transform core.
#[pyfunction]
#[pyo3(signature = (raw_nt, ontology_nt, cells, denied, projection_queries))]
fn transform_project_nt(
    py: Python<'_>,
    raw_nt: String,
    ontology_nt: String,
    cells: Vec<(String, String, String, String, String)>,
    denied: Vec<(String, String, String)>,
    projection_queries: Vec<(String, String)>,
) -> PyResult<Py<PyAny>> {
    let cells = cell_inputs(cells);
    let report = py
        .detach(move || {
            transform::transform_nt(&raw_nt, &ontology_nt, &cells, &denied, &projection_queries)
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    transform_report_to_py(py, &report)
}

fn cell_inputs(cells: Vec<(String, String, String, String, String)>) -> Vec<CellInput> {
    cells
        .into_iter()
        .map(
            |(iri, subject, predicate_curie, object, confidence)| CellInput {
                iri,
                subject,
                predicate_curie,
                object,
                confidence,
            },
        )
        .collect()
}

fn derived_rows_to_py(py: Python<'_>, rows: &[DerivedRowNative]) -> PyResult<Py<PyAny>> {
    let out = PyList::empty(py);
    for row in rows {
        let item = PyDict::new(py);
        item.set_item("subject", &row.subject)?;
        item.set_item("predicate", &row.predicate)?;
        item.set_item("object", &row.object)?;
        item.set_item("reifier", &row.reifier)?;
        item.set_item("annotations", PyList::new(py, row.annotations.iter())?)?;
        out.append(item)?;
    }
    Ok(out.into_any().unbind())
}

fn transform_report_to_py(py: Python<'_>, report: &TransformReportNative) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("base_nt", &report.base_nt)?;
    out.set_item("base_plus_derived_nt", &report.base_plus_derived_nt)?;
    out.set_item("gts_bytes", PyBytes::new(py, &report.gts_bytes))?;
    out.set_item("asserted", report.asserted)?;
    out.set_item("saturated", report.saturated)?;
    out.set_item("projected", report.projected)?;
    out.set_item("suppressed_dropped", report.suppressed_dropped)?;
    Ok(out.into_any().unbind())
}

/// Run the native claim audit scoreboard over one or more Turtle files.
#[pyfunction]
#[pyo3(signature = (root, files))]
fn claim_audit(py: Python<'_>, root: String, files: Vec<String>) -> PyResult<Py<PyAny>> {
    let paths = files
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let report = py
        .detach(move || scoreboards::claim_audit(Path::new(&root), &paths))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let text = scoreboards::render_claim_audit_text(&report);
    let json = scoreboards::render_claim_audit_json(&report)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let diagnostics = scoreboards::claim_audit_diagnostics(&report);

    let out = PyDict::new(py);
    out.set_item("text", text)?;
    out.set_item("json", json)?;
    out.set_item("flagged", report.flagged())?;
    out.set_item("shacl_errors", PyList::new(py, report.shacl_errors.iter())?)?;
    out.set_item(
        "shacl_warnings",
        PyList::new(py, report.shacl_warnings.iter())?,
    )?;
    out.set_item("report", Py::new(py, PyReport::from_engine(diagnostics))?)?;
    Ok(out.into_any().unbind())
}

/// Run the native claim audit and return only the canonical diagnostics report.
#[pyfunction]
#[pyo3(signature = (root, files))]
fn claim_audit_diagnostics_report(
    py: Python<'_>,
    root: String,
    files: Vec<String>,
) -> PyResult<Py<PyAny>> {
    let paths = files
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let report = py
        .detach(move || scoreboards::claim_audit(Path::new(&root), &paths))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(Py::new(
        py,
        PyReport::from_engine(scoreboards::claim_audit_diagnostics(&report)),
    )?
    .into_any())
}

/// Run the native real-data acceptance scoreboard.
#[pyfunction]
#[pyo3(signature = (root, source = None, descend = true))]
fn acceptance(
    py: Python<'_>,
    root: String,
    source: Option<String>,
    descend: bool,
) -> PyResult<Py<PyAny>> {
    let source_path = source.map(std::path::PathBuf::from);
    let results = py
        .detach(move || {
            scoreboards::run_acceptance_corpus(Path::new(&root), source_path.as_deref(), descend)
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let markdown = scoreboards::render_acceptance_report(&results);
    let diagnostics = scoreboards::acceptance_diagnostics(&results);
    let aggregate_recall = scoreboards::corpus_recall_pct(&results);

    let out = PyDict::new(py);
    out.set_item("markdown", markdown)?;
    out.set_item(
        "passed",
        results.iter().all(scoreboards::FileAcceptance::passed),
    )?;
    out.set_item("aggregate_recall", aggregate_recall)?;
    out.set_item("recall_pct", aggregate_recall)?;
    out.set_item("results", file_acceptance_results_to_py(py, &results)?)?;
    out.set_item("report", Py::new(py, PyReport::from_engine(diagnostics))?)?;
    Ok(out.into_any().unbind())
}

/// Run the native acceptance scoreboard and return only diagnostics.
#[pyfunction]
#[pyo3(signature = (root, source = None, descend = true))]
fn acceptance_diagnostics_report(
    py: Python<'_>,
    root: String,
    source: Option<String>,
    descend: bool,
) -> PyResult<Py<PyAny>> {
    let source_path = source.map(std::path::PathBuf::from);
    let results = py
        .detach(move || {
            scoreboards::run_acceptance_corpus(Path::new(&root), source_path.as_deref(), descend)
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(Py::new(
        py,
        PyReport::from_engine(scoreboards::acceptance_diagnostics(&results)),
    )?
    .into_any())
}

fn file_acceptance_results_to_py(
    py: Python<'_>,
    results: &[scoreboards::FileAcceptance],
) -> PyResult<Py<PyAny>> {
    let out = PyList::empty(py);
    for result in results {
        let file = PyDict::new(py);
        file.set_item("source", &result.source)?;
        file.set_item("source_triples", result.source_triples)?;
        file.set_item("output_triples", result.output_triples)?;
        file.set_item("passed", result.passed())?;
        let gates = PyList::empty(py);
        for gate in &result.gates {
            let item = PyDict::new(py);
            item.set_item("name", &gate.name)?;
            item.set_item("passed", gate.passed)?;
            item.set_item("hard", gate.hard)?;
            item.set_item("summary", &gate.summary)?;
            let metrics = PyDict::new(py);
            for (key, value) in &gate.metrics {
                metrics.set_item(key, *value)?;
            }
            item.set_item("metrics", metrics)?;
            item.set_item("detail", PyList::new(py, gate.detail.iter())?)?;
            gates.append(item)?;
        }
        file.set_item("gates", gates)?;
        out.append(file)?;
    }
    Ok(out.into_any().unbind())
}

fn lift_map_to_py(py: Python<'_>, lift: &LiftMap) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("rules", string_map(py, &lift.rules)?)?;
    out.set_item("ambiguous", string_set_map(py, &lift.ambiguous)?)?;
    out.set_item("inverse_rules", string_map(py, &lift.inverse_rules)?)?;
    out.set_item("claim_rules", tuple_map(py, &lift.claim_rules)?)?;
    out.set_item(
        "object_properties",
        PyList::new(py, lift.object_properties.iter())?,
    )?;

    let value_rules = PyList::empty(py);
    for ((source_predicate, source_value), (gmeow_predicate, gmeow_value)) in &lift.value_rules {
        let row = PyDict::new(py);
        row.set_item("source_predicate", source_predicate)?;
        row.set_item("source_value", source_value)?;
        row.set_item("gmeow_predicate", gmeow_predicate)?;
        row.set_item("gmeow_value", gmeow_value)?;
        value_rules.append(row)?;
    }
    out.set_item("value_rules", value_rules)?;
    Ok(out.into_any().unbind())
}

fn up_projection_report_to_py(py: Python<'_>, report: &UpProjectionReport) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("graph_nt", &report.graph_nt)?;
    out.set_item("lifted", report.lifted)?;
    out.set_item("claimed", report.claimed)?;
    out.set_item("gap_terms", usize_map(py, &report.gap_terms)?)?;
    out.set_item("ambiguous_terms", usize_map(py, &report.ambiguous_terms)?)?;
    out.set_item("claim_terms", usize_map(py, &report.claim_terms)?)?;
    out.set_item("context_resolved", report.context_resolved)?;
    out.set_item("context_terms", usize_map(py, &report.context_terms)?)?;
    out.set_item("tag_resolved", report.tag_resolved)?;
    out.set_item(
        "tag_resolved_terms",
        usize_map(py, &report.tag_resolved_terms)?,
    )?;
    out.set_item("minted", report.minted)?;
    Ok(out.into_any().unbind())
}

fn audit_ledger_to_py(
    py: Python<'_>,
    ledger: &crate::up_projection_gates::AuditLedger,
    markdown: &str,
) -> PyResult<Py<PyAny>> {
    let tier_counts = |counts: &crate::up_projection_gates::TierCounts| -> PyResult<Py<PyAny>> {
        let d = PyDict::new(py);
        d.set_item("proved", counts.proved)?;
        d.set_item("claimed", counts.claimed)?;
        d.set_item("red_excluded", counts.red_excluded)?;
        d.set_item("unsupported", counts.unsupported)?;
        d.set_item("liftable", counts.liftable())?;
        d.set_item("total", counts.total())?;
        Ok(d.into_any().unbind())
    };
    let out = PyDict::new(py);
    out.set_item("markdown", markdown)?;
    out.set_item("totals", tier_counts(&ledger.totals)?)?;
    let per_vocab = PyDict::new(py);
    for (vocab, counts) in &ledger.per_vocab {
        per_vocab.set_item(vocab, tier_counts(counts)?)?;
    }
    out.set_item("per_vocab", per_vocab)?;
    out.set_item("gaps", PyList::new(py, ledger.gaps.iter())?)?;
    // Convenience top-level headline figures (proved + claimed over total).
    out.set_item("proved", ledger.totals.proved)?;
    out.set_item("claimed", ledger.totals.claimed)?;
    out.set_item("red_excluded", ledger.totals.red_excluded)?;
    out.set_item("unsupported", ledger.totals.unsupported)?;
    out.set_item("liftable", ledger.liftable())?;
    out.set_item("total", ledger.total())?;
    Ok(out.into_any().unbind())
}

fn string_map(
    py: Python<'_>,
    map: &std::collections::BTreeMap<String, String>,
) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (key, value) in map {
        out.set_item(key, value)?;
    }
    Ok(out.into_any().unbind())
}

fn tuple_map(
    py: Python<'_>,
    map: &std::collections::BTreeMap<String, (String, String)>,
) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (key, (left, right)) in map {
        out.set_item(key, (left, right))?;
    }
    Ok(out.into_any().unbind())
}

fn string_set_map(
    py: Python<'_>,
    map: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (key, values) in map {
        out.set_item(key, PyList::new(py, values.iter())?)?;
    }
    Ok(out.into_any().unbind())
}

fn usize_map(
    py: Python<'_>,
    map: &std::collections::BTreeMap<String, usize>,
) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (key, value) in map {
        out.set_item(key, *value)?;
    }
    Ok(out.into_any().unbind())
}

/// Register the `gmeow_native.pipeline` submodule. Called by the unified
/// `gmeow_native` cdylib (#630); exposes `run_pipeline`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(compile_statements, m)?)?;
    m.add_function(wrap_pyfunction!(compile_statements_report, m)?)?;
    m.add_function(wrap_pyfunction!(compile_mappings_report, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_yaml_ld, m)?)?;
    m.add_function(wrap_pyfunction!(transcode, m)?)?;
    m.add_function(wrap_pyfunction!(
        parse_jsonld_star_to_gmeow_statement_metadata_nquads,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(
        parse_yaml_ld_star_to_gmeow_statement_metadata_nquads,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(roundtrip_isomorphic, m)?)?;
    m.add_function(wrap_pyfunction!(lint_projection, m)?)?;
    m.add_function(wrap_pyfunction!(alignment_policy, m)?)?;
    m.add_function(wrap_pyfunction!(fold_release_bundle_native, m)?)?;
    m.add_function(wrap_pyfunction!(verify_release_bundle_native, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_classify_sssom, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_combined_class, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_build_lift_map, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_project_nt, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_gate_audit, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_resolve_context, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_reverse_nt, m)?)?;
    m.add_function(wrap_pyfunction!(transform_skolemize_nt, m)?)?;
    m.add_function(wrap_pyfunction!(transform_saturate_nt, m)?)?;
    m.add_function(wrap_pyfunction!(transform_project_nt, m)?)?;
    m.add_function(wrap_pyfunction!(claim_audit, m)?)?;
    m.add_function(wrap_pyfunction!(claim_audit_diagnostics_report, m)?)?;
    m.add_function(wrap_pyfunction!(acceptance, m)?)?;
    m.add_function(wrap_pyfunction!(acceptance_diagnostics_report, m)?)?;
    m.add_class::<crate::mcp::McpView>()?;
    m.add_class::<crate::mcp::McpServer>()?;
    m.add_function(wrap_pyfunction!(crate::mcp::run_consumer_mcp, m)?)?;
    m.add_function(wrap_pyfunction!(crate::mcp::run_dev_mcp, m)?)?;
    Ok(())
}
