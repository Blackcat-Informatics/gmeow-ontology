// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Coherence certificate publication and fail-closed readback contracts.

use super::*;

// ── coherence_certificate (R6) ─────────────────────────────────────────────────

/// A fixed canonical-profile marker for certificate fixtures.
fn canon_profile_fixture() -> gmeow_logic::certificate::CanonProfileMarker {
    gmeow_logic::certificate::CanonProfileMarker {
        profile_id: "fixture-profile".to_owned(),
        profile_version: 1,
        presentation_id: "fixture-presentation".to_owned(),
        presentation_version: 1,
    }
}

/// A consistent native [`ReasoningResult`], with the given evaluation/completeness
/// axes and optional certified fragment — the minimum a coherence outcome reads.
fn coherence_result(
    evaluation: EvaluationStatus,
    completeness: CompletenessStatus,
    fragment: Option<&str>,
) -> ReasoningResult {
    use gmeow_logic::result::{
        InformationState, InputStatus, PreservationClaim, ResultPayload, ResultProvenance,
    };
    let mut provenance = ResultProvenance::native("contract:abc", "world:default");
    provenance.certified_fragment = fragment.map(str::to_owned);
    ReasoningResult::new(
        InputStatus::Valid,
        evaluation,
        completeness,
        PreservationClaim::exact(),
        InformationState::Supported,
        provenance,
        ResultPayload::Empty,
    )
}

/// Build the lightest `McpView` that can drive the REAL production entry point
/// ([`McpView::coherence_certificate_json`], the exact method
/// `tool_coherence_certificate` calls) over hand-crafted `graph/attestations`
/// N-Quads — without paying for a full `emit_gts`/`SnapshotBuilder` round trip
/// (reserved for the `_heavy_offgate` whole-bundle test). `McpView::from_dataset`
/// only needs the ontology header for `export::fold_meta`; the raw `gts` bytes it
/// also stores are unused by `coherence_certificate_json`, so an empty `Arc<[u8]>`
/// is honest (never a fabricated placeholder read by the surface under test).
fn view_of(nquads: &str) -> McpView {
    let header = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n";
    let doc = format!("{header}{nquads}");
    McpView::from_dataset(dataset_of(&doc), Arc::from(Vec::<u8>::new()))
        .expect("construct McpView over the crafted certificate fixture")
}

/// Simulate a producer that failed to write exactly ONE required predicate: strip
/// every N-Quads line mentioning the bracket-delimited `logic:<local>` predicate
/// IRI, leaving every other quad (including the subject's `rdf:type`) intact. The
/// bracket delimiters make the match exact — no risk of one predicate's local name
/// being a substring of another's.
fn strip_predicate(nquads: &str, local: &str) -> String {
    let token = format!("<{LOGIC_NAMESPACE}{local}>");
    let mut out = nquads
        .lines()
        .filter(|line| !line.contains(&token))
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

/// The carrier folds a CONCLUSIVE, fragment-scoped, violation-free closure as a real
/// `logic:CoherenceCertificate`; the read tool surfaces `issues_certificate:true`, the
/// certificate class, and the pinned bundle_hash / per-graph axiom_hashes VERBATIM
/// — proving the digests are exactly `per_graph_axiom_hashes` and not
/// fabricated.
#[gmeow_test_batch_macros::batch_mcp_test]
fn coherence_certificate_surfaces_a_certificate_with_the_pinned_digests() {
    use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy, per_graph_axiom_hashes};
    use purrdf::gts::writer::digest_string;

    // A small axiom-bearing dataset whose per-graph digests the certificate pins.
    let axioms = dataset_of(
        "<https://e/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://e/B> <https://e/w> .\n",
    );
    let axiom_hashes = per_graph_axiom_hashes(axioms.as_ref(), digest_string);
    assert!(!axiom_hashes.is_empty(), "the fixture pins ≥1 axiom digest");
    let bundle_hash = digest_string(b"bundle-identity-bytes");

    let result = coherence_result(
        EvaluationStatus::Completed,
        CompletenessStatus::Unknown,
        Some("fragment:test"),
    );
    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        bundle_hash.clone(),
        axiom_hashes.clone(),
        ContradictionPolicy::ForbidGapAndGlut,
        "1970-01-01T00:00:00Z",
        std::collections::BTreeSet::new(),
        canon_profile_fixture(),
    )
    .unwrap();
    assert!(outcome.issues_certificate(), "the fixture must certify");

    let dataset = dataset_of(&outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS));
    let env = coherence_certificate_envelope(dataset.as_ref()).expect("certificate present");

    assert_eq!(env["ok"], true);
    assert_eq!(env["issues_certificate"], true);
    assert_eq!(env["is_refused"], false);
    assert_eq!(env["class_local_name"], "CoherenceCertificate");
    // The surfaced bundle_hash / axiom_hashes are the pipeline-pinned digests, VERBATIM.
    assert_eq!(env["bundle_hash"], bundle_hash);
    assert!(
        env["bundle_hash"].as_str().is_some_and(|s| !s.is_empty()),
        "bundle_hash is non-empty: {env}"
    );
    let surfaced: Vec<String> = env["axiom_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        surfaced,
        axiom_hashes.into_iter().collect::<Vec<_>>(),
        "axiom_hashes are the per_graph_axiom_hashes digests, not fabricated: {env}"
    );
    assert!(
        surfaced.iter().all(|h| h.contains(':') && h.len() > 8),
        "axiom digests are non-trivial content addresses: {surfaced:?}"
    );
    // The two completeness-gate axes round-trip off the linked result node.
    assert_eq!(env["evaluation"], "completed");
    assert_eq!(env["completeness"], "unknown");
    assert_eq!(env["contract_hash"], "contract:abc");
}

/// R6 regression: a bounded/incomplete closure yields the strictly-weaker
/// `logic:CoherenceCheckAttestation`; the read tool must report
/// `issues_certificate:false` and `class_local_name:CoherenceCheckAttestation` — a
/// regression must NEVER silently upgrade an attestation to a certificate.
#[gmeow_test_batch_macros::batch_mcp_test]
fn coherence_certificate_maps_an_attestation_never_a_certificate() {
    use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

    let result = coherence_result(
        EvaluationStatus::BudgetExhausted,
        CompletenessStatus::Incomplete,
        None,
    );
    assert!(!result.is_conclusive());
    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        "blake3:bundle".to_owned(),
        ["blake3:axioms".to_owned()],
        ContradictionPolicy::ForbidGapAndGlut,
        "1970-01-01T00:00:00Z",
        std::collections::BTreeSet::new(),
        canon_profile_fixture(),
    )
    .unwrap();
    assert!(!outcome.issues_certificate());

    let dataset = dataset_of(&outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS));
    let env = coherence_certificate_envelope(dataset.as_ref()).expect("attestation present");
    assert_eq!(env["ok"], true);
    assert_eq!(
        env["issues_certificate"], false,
        "an attestation is NOT a certificate: {env}"
    );
    assert_eq!(env["class_local_name"], "CoherenceCheckAttestation");
    assert_eq!(env["evaluation"], "budget-exhausted");
    assert_eq!(env["completeness"], "incomplete");
}

/// HARD-FAIL: a bundle carrying no coherence artifact in `graph/attestations` is an
/// error — there is NO silent recompute fallback.
#[gmeow_test_batch_macros::batch_mcp_test]
fn coherence_certificate_hard_fails_on_a_bundle_without_a_certificate() {
    let stripped = dataset_of("<https://e/s> <https://e/p> <https://e/o> <https://e/g> .\n");
    let err = coherence_certificate_envelope(stripped.as_ref())
        .expect_err("a bundle with no coherence artifact must hard-fail");
    assert!(
        err.to_string()
            .contains("no coherence certificate or attestation"),
        "the hard fail names the missing artifact: {err}"
    );
}

/// A bundle carrying more than one distinct coherence subject is ambiguous and a hard
/// failure (no silent first-wins).
#[gmeow_test_batch_macros::batch_mcp_test]
fn coherence_certificate_hard_fails_on_an_ambiguous_bundle() {
    use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

    let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
    let a = CoherenceOutcome::from_reasoning_result(
        &coherence_result(
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            Some("frag:a"),
        ),
        "blake3:bundle-a".to_owned(),
        ["blake3:axioms-a".to_owned()],
        ContradictionPolicy::ForbidGapAndGlut,
        "1970-01-01T00:00:00Z",
        std::collections::BTreeSet::new(),
        canon_profile_fixture(),
    )
    .unwrap();
    let b = CoherenceOutcome::from_reasoning_result(
        &coherence_result(
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            Some("frag:b"),
        ),
        "blake3:bundle-b".to_owned(),
        ["blake3:axioms-b".to_owned()],
        ContradictionPolicy::ForbidGapAndGlut,
        "1970-01-01T00:00:00Z",
        std::collections::BTreeSet::new(),
        canon_profile_fixture(),
    )
    .unwrap();
    let both = format!("{}{}", a.to_nquads(graph), b.to_nquads(graph));
    let err = coherence_certificate_envelope(dataset_of(&both).as_ref())
        .expect_err("two coherence subjects must hard-fail");
    assert!(
        err.to_string()
            .contains("more than one distinct coherence subject"),
        "the hard fail names the ambiguity: {err}"
    );
}

/// A SINGLE coherence subject typed BOTH
/// `logic:CoherenceCertificate` and `logic:CoherenceCheckAttestation` is an
/// ambiguous, malformed artifact and must hard-fail — REGRESSION GUARD for the
/// exact gap this test closes: the ambiguity check used to compare only the
/// SUBJECT IRI (`existing != subject`), so two `rdf:type` triples on the SAME
/// subject fell through to the silent `Some(_) => {}` no-op arm and first-wins
/// picked whichever `rdf:type` the dataset's quad iteration order surfaced first.
/// Drives the REAL production entry point (`McpView::coherence_certificate_json`).
#[gmeow_test_batch_macros::batch_mcp_test]
fn coherence_certificate_json_hard_fails_on_a_dual_typed_subject() {
    use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

    let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
    let outcome = CoherenceOutcome::from_reasoning_result(
        &coherence_result(
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            Some("fragment:dual"),
        ),
        "blake3:bundle-dual".to_owned(),
        ["blake3:axioms-dual".to_owned()],
        ContradictionPolicy::ForbidGapAndGlut,
        "1970-01-01T00:00:00Z",
        std::collections::BTreeSet::new(),
        canon_profile_fixture(),
    )
    .unwrap();
    let nquads = outcome.to_nquads(graph);

    // Splice in a SECOND `rdf:type` triple on the SAME subject, typing it the
    // strictly-weaker Attestation class too — the malformed dual-typed artifact.
    let subject_line = nquads
        .lines()
        .find(|l| l.contains("22-rdf-syntax-ns#type") && l.contains("CoherenceCertificate"))
        .expect("the fixture carries the certificate's rdf:type triple");
    let subject = subject_line
        .split_whitespace()
        .next()
        .expect("rdf:type triple has a subject");
    let dual_typed = format!(
        "{nquads}{subject} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
             <{LOGIC_NAMESPACE}CoherenceCheckAttestation> <{graph}> .\n"
    );

    let view = view_of(&dual_typed);
    let out: Value = serde_json::from_str(&view.coherence_certificate_json())
        .expect("coherence_certificate_json returns valid JSON");
    assert_eq!(
        out["ok"], false,
        "a dual-typed subject must hard-fail, never first-win a class: {out}"
    );
    let error = out["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("typed BOTH"),
        "the hard fail names the ambiguous dual typing: {error}"
    );
}

/// Every producer-REQUIRED field the tool extracts must
/// be present with the producer's exact cardinality — a bundle missing ANY of them
/// is a CORRUPT artifact and must hard-fail (`ok:false`) naming the missing
/// predicate, never `ok:true` with a null/empty field silently laundering the gap
/// (no-silent-degradation, `.goals`). Drives the REAL production entry point
/// (`McpView::coherence_certificate_json`, the exact method
/// `tool_coherence_certificate` calls) over a battery of certificates each missing
/// exactly one predicate [`CoherenceOutcome::to_nquads`] otherwise always writes.
#[gmeow_test_batch_macros::batch_mcp_test]
fn coherence_certificate_json_hard_fails_on_each_missing_required_field() {
    use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

    let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
    let outcome = CoherenceOutcome::from_reasoning_result(
        &coherence_result(
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            Some("fragment:required"),
        ),
        "blake3:bundle-required".to_owned(),
        ["blake3:axioms-required".to_owned()],
        ContradictionPolicy::ForbidGapAndGlut,
        "1970-01-01T00:00:00Z",
        std::collections::BTreeSet::new(),
        canon_profile_fixture(),
    )
    .unwrap();
    assert!(
        outcome.issues_certificate(),
        "the fixture must be a real certificate (so certifiedFragment is exercised too)"
    );
    let full = outcome.to_nquads(graph);

    // Every predicate the producer writes UNCONDITIONALLY on a CoherenceCertificate
    // subject (bundleHash/axiomHash/contractHash/engine/contradictionPolicy/
    // summarizesResult/certifiedFragment) plus the two axes carried on the linked
    // result node (resultCompleteness/resultEvaluation) — one case per field.
    let required_predicates = [
        "bundleHash",
        "axiomHash",
        "contractHash",
        "engine",
        "contradictionPolicy",
        "summarizesResult",
        "canonProfileId",
        "canonProfileVersion",
        "canonPresentationId",
        "canonPresentationVersion",
        "certifiedFragment",
        "resultCompleteness",
        "resultEvaluation",
    ];
    for predicate in required_predicates {
        let stripped = strip_predicate(&full, predicate);
        let view = view_of(&stripped);
        let out: Value = serde_json::from_str(&view.coherence_certificate_json())
            .expect("coherence_certificate_json returns valid JSON");
        assert_eq!(
            out["ok"], false,
            "stripping logic:{predicate} must hard-fail, not ok:true with a null field: {out}"
        );
        let error = out["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(predicate),
            "the hard fail for a missing logic:{predicate} must name it: {error}"
        );
    }
}

/// Drive the REAL `coherence_certificate` tool through `call_tool_result` over a bundle
/// that carries the certificate in `graph/attestations` — the same disk-free, reason-free
/// read path the shipped consumer surface uses.
///
/// Fast (a minimal synthetic snapshot — just the ontology header plus the certificate's
/// own quads, built via `SnapshotBuilder`/`emit_gts`, NOT the real committed `gmeow.gts` —
/// the same tiny-snapshot construction `verify_graph_inconsistent_but_conclusive_never_certifies`
/// uses): on-gate, not `_heavy_offgate`. Measured at ~0.01-0.03 s standalone and under full
/// contention with the genuinely-heavy `*_heavy_offgate` siblings because — unlike
/// those siblings — this test never touches the real
/// committed bundle.
#[gmeow_test_batch_macros::batch_mcp_test]
fn coherence_certificate_tool_reads_the_carried_bundle() {
    use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};
    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::remove_var("GMEOW_LANG");
    }

    let outcome = CoherenceOutcome::from_reasoning_result(
        &coherence_result(
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            Some("fragment:test"),
        ),
        "blake3:carried-bundle".to_owned(),
        ["blake3:axioms-carried".to_owned()],
        ContradictionPolicy::ForbidGapAndGlut,
        "1970-01-01T00:00:00Z",
        std::collections::BTreeSet::new(),
        canon_profile_fixture(),
    )
    .unwrap();

    // A minimal snapshot: the required ontology header (the importer hard-fails
    // without it) plus the certificate's graph/attestations named graph, emitted as a
    // real gmeow.gts bundle.
    let doc = format!(
        "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             {}",
        outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS)
    );
    let dataset = dataset_of(&doc);
    let mut builder = SnapshotBuilder::new();
    builder.add_dataset(dataset.as_ref()).expect("add_dataset");
    // gmeow-test-input: synthetic-only
    let gts = emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
        // Undicted at the mandated level: these fixtures build a tiny in-test
        // snapshot, so there is no shipped dictionary to prime with — but the frame
        // profile still applies, so the level is declared rather than defaulted.
        &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
    )
    .expect("emit tiny cert-carrying snapshot");

    let server = McpServer::from_snapshot(&gts).unwrap();
    let out = text_payload(server.call_tool_result("coherence_certificate", &json!({})));
    assert_eq!(
        out["ok"], true,
        "the tool reads the carried certificate: {out}"
    );
    assert_eq!(out["class_local_name"], "CoherenceCertificate");
    assert_eq!(out["issues_certificate"], true);
    assert_eq!(out["bundle_hash"], "blake3:carried-bundle");
    assert_eq!(
        out["axiom_hashes"],
        json!(["blake3:axioms-carried"]),
        "the per-graph axiom digests ride the read envelope: {out}"
    );
    // Deterministic: the read is a pure projection of the carried quads.
    let again = text_payload(server.call_tool_result("coherence_certificate", &json!({})));
    assert_eq!(out, again, "the certificate read is deterministic");
}

/// An INCONSISTENT-but-CONCLUSIVE closure must never
/// surface as `CoherenceCertificate` on the tool surface — `completeness_class`
/// used to return `CoherenceCertificate` for ANY `is_conclusive()` result, with no
/// check for a named certified fragment or for the absence of a forbidden
/// violation, and `run_explain_quad` (unlike `run_verify_graph`, which bolted on
/// its own ad-hoc `Refused` downgrade) had NO protection at all.
///
/// Drives the REAL `verify_graph` tool (`call_tool_result`, not internals) with a
/// tiny canon plus an overlay that is GENUINELY inconsistent — `A ⊑ B`, `A ⊑ C`,
/// `B disjointWith C`, `x : A` forces `x` into `owl:Nothing` (the exact fixture
/// `reason_all_single_chase_yields_inconsistent_and_nonempty_closure` proves
/// derives `InformationState::Both` in one completed chase, i.e. CONCLUSIVE, not
/// budget-cut) — and asserts the response's `class_local_name` is NEVER
/// `CoherenceCertificate`. `class_local_name` is `completeness_class`'s output
/// (folded through `CoherenceOutcome::class_local_name_for`, the SAME gate
/// `run_explain_quad` now reads through), so this proves the shared gate, not a
/// per-tool special case.
///
/// Fast (a tiny synthetic canon + a 4-quad overlay, not the real corpus): on-gate,
/// not `_heavy_offgate`.
#[gmeow_test_batch_macros::batch_mcp_test]
fn verify_graph_inconsistent_but_conclusive_never_certifies() {
    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::remove_var("GMEOW_LANG");
    }

    // A minimal canon: just the required ontology header (the importer hard-fails
    // without it) — the SAME pattern `coherence_certificate_tool_reads_the_carried_
    // bundle_heavy_offgate` uses.
    let doc = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n";
    let dataset = dataset_of(doc);
    let mut builder = SnapshotBuilder::new();
    builder.add_dataset(dataset.as_ref()).expect("add_dataset");
    // gmeow-test-input: synthetic-only
    let gts = emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
        // Undicted at the mandated level: these fixtures build a tiny in-test
        // snapshot, so there is no shipped dictionary to prime with — but the frame
        // profile still applies, so the level is declared rather than defaulted.
        &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
    )
    .expect("emit tiny header-only canon");

    let server = McpServer::from_snapshot(&gts).unwrap();
    select_native_verification_laws(&server);

    // The overlay: a genuine DL contradiction — A ⊑ B, A ⊑ C, B disjointWith C,
    // x : A forces x into owl:Nothing. Un-graphed triples reason under the single
    // default world, and the whole tiny canon+overlay union closes well under the
    // governed step ceiling — CONCLUSIVE, never budget-cut.
    let overlay_data = "<http://gmeowtest.example/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/B> .\n\
             <http://gmeowtest.example/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/C> .\n\
             <http://gmeowtest.example/B> <http://www.w3.org/2002/07/owl#disjointWith> <http://gmeowtest.example/C> .\n\
             <http://gmeowtest.example/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://gmeowtest.example/A> .\n";

    let out = text_payload(server.call_tool_result(
        "verify_graph",
        &json!({"data": overlay_data, "format": "turtle", "max_steps": 64}),
    ));
    assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");

    // The closure genuinely completed (conclusive), not a budget-cut — so the
    // downgrade below is caused by the witnessed glut, not by non-conclusiveness.
    assert_eq!(
        out["evaluation"], "completed",
        "the fixture's tiny closure must be CONCLUSIVE (not budget-cut) for this to be a \
             faithful proof: {out}"
    );

    // The falsifiable assertion: an inconsistent-but-conclusive closure must NEVER
    // render `CoherenceCertificate` — the shared `CoherenceOutcome` gate downgrades
    // it to the flat refusal instead.
    assert_ne!(
        out["class_local_name"], "CoherenceCertificate",
        "an inconsistent-but-conclusive closure must never be labeled a \
             CoherenceCertificate: {out}"
    );
    assert_eq!(
        out["class_local_name"], "Refused",
        "a witnessed forbidden violation in a CONCLUSIVE closure is a flat refusal, per \
             the SAME CoherenceOutcome gate the bundle-level coherence certifier uses: {out}"
    );
}

/// `verify_graph`'s `coherent` field MUST agree with
/// `class_local_name` — both MUST be derived from the SAME `CoherenceOutcome`
/// gate, never from two independent signals.
///
/// The overlay carries a genuine DL glut via plain PAIRWISE `owl:disjointWith`
/// (A ⊑ B, A ⊑ C, B disjointWith C, x : A forces x into owl:Nothing) —
/// DELIBERATELY not an `owl:AllDisjointClasses` set, so the ONE bad-example
/// verify query that could independently catch a disjoint-axis violation
/// (`class-in-two-disjoint-axes.rq`, which matches only `owl:AllDisjointClasses`
/// membership) does NOT fire: `report.ok()` is true. Earlier,
/// `coherent` was read straight from `report.ok()`, so this exact fixture would
/// render the self-contradictory `coherent:true` alongside
/// `class_local_name:"Refused"`. The fix routes `coherent` through the SAME
/// shared `completeness_refused` gate that decides `class_local_name`, so the
/// two fields can never disagree.
///
/// Fast (a tiny synthetic canon + a 4-quad overlay, not the real corpus): on-gate,
/// not `_heavy_offgate`.
#[gmeow_test_batch_macros::batch_mcp_test]
fn verify_graph_coherent_never_disagrees_with_refused_class() {
    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::remove_var("GMEOW_LANG");
    }

    // The header-only canon PLUS the `axis-not-disjoint.rq` bad-example query's
    // own required orthogonality matrix (an `owl:AllDisjointClasses` set naming
    // the seven fixed identity axes) — otherwise that unrelated bad-example
    // query fires on ANY header-only canon (it demands the matrix exist at all),
    // which would make `report.ok()` false for a reason having nothing to do
    // with this test's glut and defeat the falsifiability of the assertion below.
    let doc = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             _:axdisj <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#AllDisjointClasses> .\n\
             _:axdisj <http://www.w3.org/2002/07/owl#members> _:axlist0 .\n\
             _:axlist0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/GenderIdentity> .\n\
             _:axlist0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist1 .\n\
             _:axlist1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/GenderExpression> .\n\
             _:axlist1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist2 .\n\
             _:axlist2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/SexAssignedAtBirth> .\n\
             _:axlist2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist3 .\n\
             _:axlist3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/SexualOrientation> .\n\
             _:axlist3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist4 .\n\
             _:axlist4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/RomanticOrientation> .\n\
             _:axlist4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist5 .\n\
             _:axlist5 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/PronounSet> .\n\
             _:axlist5 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist6 .\n\
             _:axlist6 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/Honorific> .\n\
             _:axlist6 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .\n";
    let dataset = dataset_of(doc);
    let mut builder = SnapshotBuilder::new();
    builder.add_dataset(dataset.as_ref()).expect("add_dataset");
    // gmeow-test-input: synthetic-only
    let gts = emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
        // Undicted at the mandated level: these fixtures build a tiny in-test
        // snapshot, so there is no shipped dictionary to prime with — but the frame
        // profile still applies, so the level is declared rather than defaulted.
        &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
    )
    .expect("emit tiny header-only canon");

    let server = McpServer::from_snapshot(&gts).unwrap();
    select_native_verification_laws(&server);

    // The glut: same shape as `verify_graph_inconsistent_but_conclusive_never_
    // certifies`, but PAIRWISE `owl:disjointWith` on classes NOT named in the
    // orthogonality matrix above — so neither `axis-not-disjoint.rq` (satisfied
    // by the matrix) nor `class-in-two-disjoint-axes.rq` (requires
    // `owl:AllDisjointClasses` membership, which g4B/g4C never join) can match.
    let overlay_data = "<http://gmeowtest.example/g4A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/g4B> .\n\
             <http://gmeowtest.example/g4A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/g4C> .\n\
             <http://gmeowtest.example/g4B> <http://www.w3.org/2002/07/owl#disjointWith> <http://gmeowtest.example/g4C> .\n\
             <http://gmeowtest.example/g4x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://gmeowtest.example/g4A> .\n";

    let out = text_payload(server.call_tool_result(
        "verify_graph",
        &json!({"data": overlay_data, "format": "turtle", "max_steps": 64}),
    ));
    assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
    assert_eq!(
        out["evaluation"], "completed",
        "the fixture's tiny closure must be CONCLUSIVE (not budget-cut) for this to be a \
             faithful proof: {out}"
    );

    // The class label refutes coherence via the DL glut...
    assert_eq!(
        out["class_local_name"], "Refused",
        "a witnessed forbidden violation in a CONCLUSIVE closure is a flat refusal: {out}"
    );
    // ...and `coherent` MUST agree — never `coherent:true` alongside
    // `class_local_name:"Refused"`, even though no bad-example verify query
    // fired on this fixture's pairwise `owl:disjointWith` shape.
    assert_eq!(
        out["coherent"], false,
        "coherent must be false whenever class_local_name is Refused, regardless of \
             whether any bad-example verify query matched: {out}"
    );
}
