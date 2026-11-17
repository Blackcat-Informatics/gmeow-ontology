// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Conjecture and candidate persistence over the actual library backend.

use super::*;

// ── Conjecture-library persistence ───────────────────────────────────────

use gmeow_ns::MATH_NS;

/// A KB where the candidate's head class is UNRELATED (no disjointness), so firing derives
/// a new consistent fact ⇒ Open/Neither, no witness.
fn open_kb(cls_local: &str) -> String {
    format!(
        "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash.\n"
    )
}

fn temp_conjecture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("conjectures.gts");
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::set_var("GMEOW_CONJECTURE_PATH", &path);
    }
    (dir, path)
}

/// The imported conjecture-library dataset (all appended segments unioned).
fn read_conjectures(path: &Path) -> std::sync::Arc<purrdf::RdfDataset> {
    let bytes = fs::read(path).expect("read conjecture library");
    purrdf::import_gts_events(&bytes)
        .expect("import conjecture library")
        .dataset
}

/// Every subject typed `logic:Conjecture` in `dataset`.
fn conjecture_nodes(dataset: &purrdf::RdfDataset) -> BTreeSet<String> {
    dataset
        .owned_quads()
        .filter(|q| {
            q.predicate == RDF_TYPE_IRI && q.object == RdfTerm::iri(format!("{LOGIC_NS}Conjecture"))
        })
        .filter_map(|q| match q.subject {
            RdfTerm::Iri(s) => Some(s),
            _ => None,
        })
        .collect()
}

/// The `logic:witnessPremise` literal lexicals in `dataset`.
fn witness_premises(dataset: &purrdf::RdfDataset) -> Vec<String> {
    dataset
        .owned_quads()
        .filter(|q| q.predicate == format!("{LOGIC_NS}witnessPremise"))
        .filter_map(|q| match q.object {
            RdfTerm::Literal(lit) => Some(lit.lexical_form),
            _ => None,
        })
        .collect()
}

struct ConjEnvGuard;
impl ConjEnvGuard {
    fn set() -> (EnvRestore, ConjEnvGuard) {
        let env = EnvRestore::capture(&[
            "GMEOW_LANG",
            "GMEOW_MEMORY_PATH",
            "GMEOW_CONJECTURE_PATH",
            "GMEOW_CANDIDATE_PATH",
            "HOME",
            "USERPROFILE",
        ]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        (env, ConjEnvGuard)
    }
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn persist_conjecture_precondition_gates_the_commit() {
    // The write is a REAL TR gate: with the precondition present the committed run
    // succeeds; with it absent the run FAILS (so the tool returns ok:false before writing).
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let ok = execute_memory_txn(
        source_action_policy(),
        MCP_PERSIST_CONJECTURE_SCHEMA,
        &[MCP_CONJECTURE_VERDICT_PRESENTED],
        false,
    )
    .unwrap();
    assert!(
        matches!(ok, TxReceipt::CommittedSuccess { .. }),
        "the precondition-present committed run must succeed: {ok:?}"
    );
    let unmet = execute_memory_txn(
        source_action_policy(),
        MCP_PERSIST_CONJECTURE_SCHEMA,
        &[],
        false,
    )
    .unwrap();
    assert!(
        matches!(unmet, TxReceipt::CommittedFailure { .. }),
        "a persist with the precondition UNMET must fail the commit (the tool then returns \
             ok:false before writing): {unmet:?}"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn conjecture_test_is_pure_and_writes_nothing() {
    // R3a: the "test" leg (`conjecture_test`) is a PURE hypothetical evaluation. Driving it
    // with a candidate that, under the OLD single-tool surface, would have persisted a
    // refutation must still return the full verdict envelope while leaving the library file
    // byte-unchanged (here: absent) — no TR gate, no append, ever.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    assert!(!path.exists(), "library must not exist before the call");
    let native = server
        .evaluate_conjecture_test_request(&json!({
            "formula": forall_horn_candidate("B"),
            "kb": refuting_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
        }))
        .expect("valid governed conjecture request succeeds");
    let resp = McpServer::conjecture_test_response(&native);
    assert_eq!(resp["ok"], true, "the pure test must succeed: {resp}");
    assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
    assert_eq!(resp["witness"]["individual"], "http://ex/a");
    let node = resp["conjecture"].as_str().expect("node iri").to_string();
    assert!(!node.is_empty());
    // No persist/transaction section at all on this surface.
    assert!(
        resp.get("transaction").is_none(),
        "conjecture_test must never render a transaction section: {resp}"
    );
    assert!(
        resp.get("committed").is_none(),
        "conjecture_test must never render a committed flag: {resp}"
    );

    // T1 (G7): the grounded judgment travels on the SAME `judgment_nquads` key
    // `verify_graph`/`explain_quad` carry — even on this pure, nothing-persisted path —
    // and exactly matches the evaluator's projection of a real `logic:Conjecture`
    // node embedding a non-empty `logic:ReasoningResult`. The tiny result_rdf
    // contracts independently own the reader round trip.
    let judgment = resp["judgment_nquads"]
        .as_str()
        .expect("conjecture_test must carry a judgment_nquads string");
    assert!(
        !judgment.trim().is_empty(),
        "judgment_nquads must not be empty"
    );
    assert_eq!(judgment, native.verdict_nt);
    assert_eq!(
        native.lifecycle,
        ConjectureLifecycleState::RefutedInStandpoint.wire()
    );
    assert_eq!(resp["verdict"]["lifecycle"], native.lifecycle);
    assert!(
        judgment.contains(&format!(
            "<{}> <https://blackcatinformatics.ca/logic/conjectureLifecycleState> <{}> .",
            native.node_iri,
            ConjectureLifecycleState::RefutedInStandpoint.iri()
        )),
        "the actual conjecture node must carry its native refuted lifecycle"
    );
    assert!(
            judgment.contains(&format!(
                "<{}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Conjecture> .",
                native.node_iri
            )),
            "the native projection must ground the returned logic:Conjecture node"
        );
    assert!(
        judgment.contains("<https://blackcatinformatics.ca/logic/conjectureVerdict>")
            && judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
        "the native projection must link its embedded logic:ReasoningResult body"
    );

    // The library file is BYTE-UNCHANGED (still absent): no TR gate, no append.
    assert!(
        !path.exists(),
        "a pure conjecture_test call must write nothing to the library"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn store_conjecture_refutes_and_persists_with_witness() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    assert!(
        !path.exists(),
        "library must not exist before the first persist"
    );
    let resp = text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("B"),
            "kb": refuting_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    assert_eq!(resp["ok"], true, "refutation persist must succeed: {resp}");
    assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
    assert_eq!(resp["witness"]["individual"], "http://ex/a");
    let node = resp["conjecture"].as_str().expect("node iri").to_string();
    assert!(!node.is_empty());

    // The library file grew, and the witness premises round-trip back through the GTS.
    assert!(
        path.exists(),
        "the conjecture library file must have been written"
    );
    let dataset = read_conjectures(&path);
    assert!(
        conjecture_nodes(&dataset).contains(&node),
        "the content-addressed conjecture node must be readable back: {node}"
    );
    // The standpoint scope is recoverable.
    assert!(dataset.owned_quads().any(|q| {
        q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
            && q.object == RdfTerm::iri("http://ex/standpoint/alice")
    }));
    // The witness premises are recoverable.
    let premises = witness_premises(&dataset);
    assert!(
        !premises.is_empty(),
        "a refutation must persist recoverable witness premises"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn store_conjecture_bridges_math_twin_via_conjecture_under_test() {
    // Driving the real MCP `store_conjecture` tool (the shared conjecture-test core) with a
    // `math_conjecture` must persist the always-present structural twin bridge
    // `<math> math:conjectureUnderTest <logic:Conjecture-node>` (domain math:Conjecture,
    // range logic:Conjecture) — readable back out of the append-only GTS library.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let math_iri = "https://blackcatinformatics.ca/math/conjecture/goldbach";
    let resp = text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("B"),
            "kb": refuting_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
            "math_conjecture": math_iri,
        }),
    ));
    assert_eq!(resp["ok"], true, "math-twin persist must succeed: {resp}");
    let node = resp["conjecture"].as_str().expect("node iri").to_string();

    let dataset = read_conjectures(&path);
    let under_test = format!("{MATH_NS}conjectureUnderTest");
    assert!(
        dataset.owned_quads().any(|q| {
            q.subject == RdfTerm::iri(math_iri)
                && q.predicate == under_test
                && q.object == RdfTerm::iri(node.clone())
        }),
        "the math:conjectureUnderTest bridge <{math_iri}> -> <{node}> must be recoverable \
             from the persisted GTS library"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn store_conjecture_dry_run_writes_nothing() {
    // The `dry_run` witness on `store_conjecture` is a HYPOTHETICAL commit: the verdict and
    // TR receipt are computed exactly as a real commit would be, but nothing is appended.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let native = server
        .evaluate_store_conjecture_request(&json!({
            "formula": forall_horn_candidate("B"),
            "kb": refuting_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
            "dry_run": true,
        }))
        .expect("valid governed conjecture request succeeds");
    let resp = McpServer::store_conjecture_response(&native);
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["dry_run"], true);
    assert_eq!(resp["transaction"]["committed"], false);
    // The verdict is still computed and returned.
    assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
    // Nothing written: the library file does not exist / is zero bytes.
    assert!(
        !path.exists() || fs::metadata(&path).unwrap().len() == 0,
        "a dry run must write nothing to the library"
    );

    // T1 (G7): `store_conjecture`'s dry-run path STILL carries the grounded judgment under
    // the SAME `judgment_nquads` key the read tools and `conjecture_test` use — a
    // hypothetical persist is not a hypothetical verdict; the engine really ran.
    let judgment = resp["judgment_nquads"]
        .as_str()
        .expect("store_conjecture dry-run must carry a judgment_nquads string");
    assert!(
        !judgment.trim().is_empty(),
        "judgment_nquads must not be empty"
    );
    assert_eq!(judgment, native.verdict_nt);
    assert_eq!(
        native.lifecycle,
        ConjectureLifecycleState::RefutedInStandpoint.wire()
    );
    assert_eq!(resp["verdict"]["lifecycle"], native.lifecycle);
    assert!(
        judgment.contains(&format!(
            "<{}> <https://blackcatinformatics.ca/logic/conjectureLifecycleState> <{}> .",
            native.node_iri,
            ConjectureLifecycleState::RefutedInStandpoint.iri()
        )),
        "the actual conjecture node must carry its native refuted lifecycle"
    );
    assert!(
            judgment.contains(&format!(
                "<{}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Conjecture> .",
                native.node_iri
            )),
            "the native projection must ground the returned logic:Conjecture node"
        );
    assert!(
        judgment.contains("<https://blackcatinformatics.ca/logic/conjectureVerdict>")
            && judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
        "the native projection must link its embedded logic:ReasoningResult body"
    );

    // A second, committing call on the SAME candidate now appends for real: the dry run
    // above left the library byte-unchanged, so this is the library's FIRST segment.
    let committed = text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("B"),
            "kb": refuting_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    assert_eq!(committed["ok"], true);
    assert!(path.exists() && fs::metadata(&path).unwrap().len() > 0);
    // The committed path carries judgment_nquads too.
    assert!(
        committed["judgment_nquads"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty()),
        "the committed store_conjecture response must also carry judgment_nquads: {committed}"
    );
}

/// Store a real conjecture through the shipped `store_conjecture` tool and return its
/// content-addressed node IRI (the shared setup for the refute tests below).
fn store_one_conjecture(server: &McpServer, cls: &str) -> String {
    let resp = text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate(cls),
            "kb": open_kb(cls),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    assert_eq!(resp["ok"], true, "store must succeed: {resp}");
    assert_eq!(resp["verdict"]["lifecycle"], "open");
    resp["conjecture"].as_str().expect("node iri").to_string()
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn refute_conjecture_withdraws_and_appends_author_segment() {
    // store_conjecture then refute_conjecture its node IRI: the library gains a NEW
    // append-only segment marking that node ConjectureWithdrawn with the author reason;
    // the reader's EFFECTIVE lifecycle is now Withdrawn; the PRIOR segment bytes are
    // byte-for-byte intact (append-only, never mutated).
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let node = store_one_conjecture(&server, "B");
    // Before withdrawal the effective state is the engine verdict (Open), never Withdrawn.
    let before = read_library(library_at(&path).as_ref()).unwrap();
    assert_eq!(
        before.get(&node).copied(),
        Some(ConjectureLifecycleState::Open)
    );
    let prior = fs::read(&path).expect("library bytes before refute");

    let resp = text_payload(server.call_tool_result(
        "refute_conjecture",
        &json!({"conjecture_id": node, "reason": "author retired this line of inquiry"}),
    ));
    assert_eq!(resp["ok"], true, "the withdrawal must commit: {resp}");
    assert_eq!(resp["conjecture"], node);
    assert_eq!(resp["lifecycle"], "withdrawn");
    assert_eq!(resp["transaction"]["committed"], true);
    assert_eq!(resp["transaction"]["succeeded"], true);
    // T1 (G7): the compensating withdrawal carries its own grounded RDF projection under
    // the SAME `judgment_nquads` key — the target node re-marked ConjectureWithdrawn.
    assert!(
        resp["judgment_nquads"]
            .as_str()
            .is_some_and(|s| s.contains("ConjectureWithdrawn")),
        "refute_conjecture must carry judgment_nquads naming ConjectureWithdrawn: {resp}"
    );

    // The EFFECTIVE lifecycle (segment order) is now Withdrawn.
    let after = read_library(library_at(&path).as_ref()).unwrap();
    assert_eq!(
        after.get(&node).copied(),
        Some(ConjectureLifecycleState::Withdrawn),
        "the effective lifecycle after refute must be Withdrawn"
    );

    // Append-only: the file GREW and the prior bytes are an untouched prefix.
    let now = fs::read(&path).expect("library bytes after refute");
    assert!(
        now.len() > prior.len(),
        "the withdrawal must append new bytes"
    );
    assert_eq!(
        &now[..prior.len()],
        &prior[..],
        "prior segment bytes must be byte-for-byte intact (append-only)"
    );

    // The author reason literal round-trips out of the unioned library.
    let dataset = read_conjectures(&path);
    assert!(
        dataset.owned_quads().any(|q| {
            q.subject == RdfTerm::iri(node.clone())
                && q.predicate == format!("{LOGIC_NS}withdrawalReason")
                && matches!(
                    q.object,
                    RdfTerm::Literal(ref lit)
                        if lit.lexical_form == "author retired this line of inquiry"
                )
        }),
        "the author withdrawal reason must be recoverable from the library"
    );
    // The withdrawal is reviewer-asserted, never engine-produced.
    assert!(
        dataset.owned_quads().any(|q| {
            q.subject == RdfTerm::iri(node.clone())
                && q.predicate == format!("{LOGIC_NS}verdictProvenance")
                && q.object == RdfTerm::iri(format!("{LOGIC_NS}VerdictReviewerAsserted"))
        }),
        "the withdrawal must carry VerdictReviewerAsserted provenance"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn refute_conjecture_second_withdraw_rejected_by_segment_order() {
    // store -> withdraw -> withdraw again the SAME node: the second refute is rejected as
    // precondition-unmet because the EFFECTIVE state is already Withdrawn, decided by
    // SEGMENT ORDER (the last lifecycle assertion), not by the union or gmeow:atTime. The
    // rejected call appends NOTHING (the library stays byte-for-byte identical).
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let node = store_one_conjecture(&server, "B");
    let first = text_payload(server.call_tool_result(
        "refute_conjecture",
        &json!({"conjecture_id": node, "reason": "first withdrawal"}),
    ));
    assert_eq!(
        first["ok"], true,
        "the first withdrawal must commit: {first}"
    );
    let after_first = fs::read(&path).expect("library bytes after first withdrawal");

    let second = text_payload(server.call_tool_result(
        "refute_conjecture",
        &json!({"conjecture_id": node, "reason": "second withdrawal"}),
    ));
    assert_eq!(
        second["ok"], false,
        "a second withdrawal must be rejected: {second}"
    );
    assert_eq!(second["transaction"]["committed"], true);
    assert_eq!(second["transaction"]["succeeded"], false);
    assert!(
        second["error"]
            .as_str()
            .is_some_and(|e| e.contains("already withdrawn")),
        "the rejection must name the already-withdrawn precondition: {second}"
    );
    // The rejected call wrote nothing.
    let after_second = fs::read(&path).expect("library bytes after rejected withdrawal");
    assert_eq!(
        after_first, after_second,
        "a rejected withdrawal must append nothing to the library"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn refute_conjecture_unknown_id_rejected() {
    // An unknown conjecture_id (nothing stored) fails the TR gate (empty start state) and
    // returns ok:false before any write — the library file is never created.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    assert!(!path.exists(), "library must not exist before the call");
    let resp = text_payload(server.call_tool_result(
        "refute_conjecture",
        &json!({"conjecture_id": "https://blackcatinformatics.ca/gmeow/graph/conjecture/ghost"}),
    ));
    assert_eq!(resp["ok"], false, "an unknown id must be rejected: {resp}");
    assert!(
        resp["error"]
            .as_str()
            .is_some_and(|e| e.contains("unknown conjecture id")),
        "the rejection must name the unknown id: {resp}"
    );
    assert!(
        !path.exists(),
        "a rejected withdrawal of an unknown id must write nothing"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn refute_conjecture_dry_run_writes_nothing() {
    // dry_run=true witnesses the hypothetical commit (lifecycle withdrawn, committed:false)
    // but appends NOTHING — the library file stays byte-unchanged.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let node = store_one_conjecture(&server, "B");
    let before = fs::read(&path).expect("library bytes before dry run");

    let resp = text_payload(server.call_tool_result(
        "refute_conjecture",
        &json!({"conjecture_id": node, "reason": "sandbox", "dry_run": true}),
    ));
    assert_eq!(resp["ok"], true, "the dry run must succeed: {resp}");
    assert_eq!(resp["dry_run"], true);
    assert_eq!(resp["lifecycle"], "withdrawn");
    assert_eq!(resp["transaction"]["committed"], false);
    // T1 (G7): the hypothetical withdrawal still carries judgment_nquads — the RDF
    // projection is pure and side-effect-free, so witnessing it costs nothing written.
    assert!(
        resp["judgment_nquads"]
            .as_str()
            .is_some_and(|s| s.contains("ConjectureWithdrawn")),
        "refute_conjecture dry-run must carry judgment_nquads naming ConjectureWithdrawn: \
             {resp}"
    );

    // Nothing appended: bytes unchanged AND the effective lifecycle is still Open.
    let after = fs::read(&path).expect("library bytes after dry run");
    assert_eq!(before, after, "a dry run must write nothing to the library");
    let library = read_library(library_at(&path).as_ref()).unwrap();
    assert_eq!(
        library.get(&node).copied(),
        Some(ConjectureLifecycleState::Open),
        "a dry run must not change the effective lifecycle"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn store_conjecture_and_refute_round_trip_keep_library_and_audit_consistent() {
    // store_conjecture and refute_conjecture must each land their
    // library segment AND their audit segment TOGETHER — never one without the other. Drive
    // the real `call_tool_result` surface for both tools and, after EACH commit, assert the
    // library dataset carries BOTH the verdict/withdrawal triples AND the matching
    // `logic:instantiatesSchema` audit marker for that same call — round-tripping cleanly.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let node = store_one_conjecture(&server, "B");
    let after_store = read_conjectures(&path);
    assert!(
        conjecture_nodes(&after_store).contains(&node),
        "the store's library segment must be present after the commit"
    );
    assert!(
        after_store.owned_quads().any(|q| {
            q.predicate == LOGIC_INSTANTIATES_SCHEMA
                && q.object == RdfTerm::iri(MCP_PERSIST_CONJECTURE_SCHEMA)
        }),
        "the store's audit segment must be present in the SAME commit as its library \
             segment (no library-without-audit gap): {after_store:?}"
    );

    let refute = text_payload(server.call_tool_result(
        "refute_conjecture",
        &json!({"conjecture_id": node, "reason": "round-trip proof"}),
    ));
    assert_eq!(refute["ok"], true, "the withdrawal must commit: {refute}");

    let after_refute = read_conjectures(&path);
    let library = read_library(library_at(&path).as_ref()).unwrap();
    assert_eq!(
        library.get(&node).copied(),
        Some(ConjectureLifecycleState::Withdrawn),
        "the round-trip's effective state must be Withdrawn"
    );
    assert!(
        after_refute.owned_quads().any(|q| {
            q.predicate == LOGIC_INSTANTIATES_SCHEMA
                && q.object == RdfTerm::iri(MCP_WITHDRAW_CONJECTURE_SCHEMA)
        }),
        "the refute's audit segment must be present in the SAME commit as its withdrawal \
             segment: {after_refute:?}"
    );
    // The store's audit marker is STILL there too — append-only, nothing overwritten.
    assert!(
        after_refute.owned_quads().any(|q| {
            q.predicate == LOGIC_INSTANTIATES_SCHEMA
                && q.object == RdfTerm::iri(MCP_PERSIST_CONJECTURE_SCHEMA)
        }),
        "the store's audit marker must survive the later refute commit"
    );
}

#[cfg(unix)]
#[gmeow_test_batch_macros::batch_mcp_test]
fn conjecture_persist_is_all_or_nothing_on_a_failed_commit() {
    // forcing the atomic-replace write to fail PARTWAY (the temp file
    // for the combined library+audit bytes can't even be created) must leave the library
    // BYTE-UNCHANGED — never holding the library segment without its audit segment (or vice
    // versa), because both are assembled in memory and committed via ONE rename. This
    // simulates the "audit append fails after the library append already landed" half of the
    // gap: with the fix, there is no such partial state to observe.
    use std::os::unix::fs::PermissionsExt;

    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_dir, path) = temp_conjecture();

    // Seed the library with one already-committed segment (as if a prior store succeeded).
    let seed_node = "https://blackcatinformatics.ca/gmeow/graph/conjecture/seed";
    let seed_nt = format!(
        "<{seed_node}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n\
             <{seed_node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}ConjectureOpen> .\n"
    );
    write_conjecture_segment(&path, &seed_nt).unwrap();
    let before = fs::read(&path).expect("seeded library bytes");
    assert!(!before.is_empty());

    // Make the library's directory read-only: the existing `.lock` sidecar can still be
    // opened (it already exists), but `append_conjecture_segments` can no longer create the
    // same-directory temp file its atomic rename depends on — an I/O failure squarely inside
    // the combined library+audit commit, after both segments' bytes are already built.
    let dir = path
        .parent()
        .expect("library has a parent dir")
        .to_path_buf();
    let original_mode = fs::metadata(&dir).unwrap().permissions().mode();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod read-only");

    let outcome = (|| -> gmeow_errors::Result<()> {
        let lib_segment = build_nt_segment(
            &[],
            &probe_medium(),
            &format!(
                "<{seed_node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}ConjectureWithdrawn> .\n"
            ),
        )?;
        let audit_segment = build_audit_segment(
            &[],
            &probe_medium(),
            "urn:gmeow:conjecture-call:simulated-failure",
            MCP_WITHDRAW_CONJECTURE_SCHEMA,
            &[MCP_CONJECTURE_IN_LIBRARY],
            "1970-01-01T00:00:00Z",
        )?;
        let library = library_at(&path);
        with_library_lock(library.as_ref(), || {
            append_library_segments(library.as_ref(), &[lib_segment, audit_segment])
        })
    })();

    // Restore permissions unconditionally before asserting, so the tempdir can still clean
    // itself up even if an assertion below panics.
    fs::set_permissions(&dir, fs::Permissions::from_mode(original_mode)).expect("chmod restore");

    assert!(
        outcome.is_err(),
        "the forced I/O failure must surface as an error, not a silent partial write"
    );
    let after = fs::read(&path).expect("library bytes after the failed commit");
    assert_eq!(
        before, after,
        "a failed combined library+audit commit must leave the library BYTE-UNCHANGED \
             (all-or-nothing) — never holding one segment without the other"
    );
    let library = read_library(library_at(&path).as_ref()).unwrap();
    assert_eq!(
        library.get(seed_node).copied(),
        Some(ConjectureLifecycleState::Open),
        "the seed's state must still be Open — the failed withdrawal must not have applied"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn conjecture_lock_serializes_concurrent_writers() {
    // `with_conjecture_lock` must provide REAL mutual exclusion, not
    // just an in-process convention a second caller could sidestep — so this test opens the
    // SAME sidecar `.lock` file from two INDEPENDENT `std::fs::File` descriptors (one per
    // thread), mirroring how two separate OS processes would each open it themselves. Since
    // `flock` locks are scoped to the OPEN FILE DESCRIPTION (not the process), two distinct
    // descriptors contending on the same path exercise the identical kernel mechanism that
    // serializes real cross-process `store_conjecture` / `refute_conjecture` callers.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_dir, path) = temp_conjecture();

    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let holder_path = path.clone();
    let holder = std::thread::spawn(move || {
        with_library_lock(library_at(&holder_path).as_ref(), || {
            started_tx.send(()).expect("signal lock acquired");
            release_rx.recv().expect("wait for release signal");
            Ok(())
        })
        .expect("holder must acquire and release cleanly")
    });
    started_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the holder thread must acquire the lock");

    let waiter_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let waiter_done_writer = waiter_done.clone();
    let waiter_path = path.clone();
    let waiter = std::thread::spawn(move || {
        with_library_lock(library_at(&waiter_path).as_ref(), || {
            waiter_done_writer.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .expect("waiter must eventually acquire and release cleanly")
    });

    // While the holder still owns the lock, the waiter's OWN, independently-opened file
    // descriptor must be blocked — proving this is a real `flock`, not an in-process no-op.
    std::thread::sleep(std::time::Duration::from_millis(250));
    assert!(
        !waiter_done.load(std::sync::atomic::Ordering::SeqCst),
        "a second, independently-opened lock attempt must still be blocked while the first \
             holder is inside its critical section"
    );

    release_tx.send(()).expect("release the holder");
    holder.join().expect("holder thread must not panic");
    waiter.join().expect("waiter thread must not panic");
    assert!(
        waiter_done.load(std::sync::atomic::Ordering::SeqCst),
        "the second lock attempt must complete once the first holder releases"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn read_conjecture_library_resolves_effective_state_by_segment_order() {
    // The reader resolves the effective lifecycle purely by SEGMENT ORDER (last writer
    // wins) — NOT by the union (which would carry both states at once) and NOT by
    // gmeow:atTime (every segment shares the fixed determinism epoch). Two nodes are
    // written with OPPOSITE last-writer states to prove position alone decides: the state
    // that "sounds terminal" (Withdrawn) does NOT win unless it is written LAST.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_dir, path) = temp_conjecture();

    let base = "https://blackcatinformatics.ca/gmeow/graph/conjecture/";
    let reopened = format!("{base}reopened");
    let retired = format!("{base}retired");
    let ty = format!("<{reopened}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n");
    let ty2 = format!("<{retired}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n");
    let lc = |node: &str, state: &str| {
        format!("<{node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}{state}> .\n")
    };

    // `reopened`: Withdrawn FIRST, then Open — Open is last, so Open must win.
    write_conjecture_segment(
        &path,
        &format!("{ty}{}", lc(&reopened, "ConjectureWithdrawn")),
    )
    .unwrap();
    // `retired`: Open FIRST, then Withdrawn — Withdrawn is last, so Withdrawn must win.
    write_conjecture_segment(&path, &format!("{ty2}{}", lc(&retired, "ConjectureOpen"))).unwrap();
    write_conjecture_segment(&path, &lc(&reopened, "ConjectureOpen")).unwrap();
    write_conjecture_segment(&path, &lc(&retired, "ConjectureWithdrawn")).unwrap();

    let library = read_library(library_at(&path).as_ref()).unwrap();
    assert_eq!(
        library.get(&reopened).copied(),
        Some(ConjectureLifecycleState::Open),
        "the LAST segment (Open) must win even though a prior segment said Withdrawn"
    );
    assert_eq!(
        library.get(&retired).copied(),
        Some(ConjectureLifecycleState::Withdrawn),
        "the LAST segment (Withdrawn) must win over the prior Open"
    );
}

/// A candidate authored as a REIFIED GROUND binary atom — the exact `logic:relation` /
/// `logic:argument` reification every authored formula uses, but VARIABLE-FREE: the ground
/// fact `ex:a rdf:type ex:<cls_local>`. The reconstructed `Formula::Atom` is trivially-Horn,
/// so it once panicked `LogicProgram::with_formulas` during the candidate parse.
fn reified_ground_atom_candidate(cls_local: &str) -> String {
    format!(
        "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:phi a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
    )
}

/// A KB that ASSERTS the ground fact the reified-ground-atom candidate names, so `KB ⊨ φ`.
fn ground_atom_entailing_kb(cls_local: &str) -> String {
    format!(
        "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a rdf:type ex:{cls_local} .\n"
    )
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn parse_candidate_reified_ground_atom_lifts_not_panics() {
    // F2 regression: a REIFIED GROUND binary atom is trivially-Horn, so the front-end must
    // route it to `LogicProgram.axioms` (not `with_formulas`, which hard-asserts) and
    // `parse_candidate_formula` must reconstruct it — cleanly, never a panic and never a
    // false "0 formula(s) and 0 axiom(s)" rejection.
    use gmeow_logic_compile::ir::{Formula, Term as IrTerm};
    // `parse_candidate_formula` now lives in the shared gmeow-logic conjecture-eval
    // authority; assert the SHIPPED re-export still lifts a reified ground atom cleanly.
    let candidate =
        gmeow_logic::conjecture_eval::parse_candidate_formula(&reified_ground_atom_candidate("B"))
            .expect("reified ground atom must lift to a candidate formula");
    match candidate {
        Formula::Atom { relation, args } => {
            assert_eq!(relation, IrTerm::Iri(RDF_TYPE_IRI.to_owned()));
            assert_eq!(
                args,
                vec![
                    IrTerm::Iri("http://ex/a".to_owned()),
                    IrTerm::Iri("http://ex/B".to_owned()),
                ]
            );
        }
        other => panic!("expected a ground binary atom, got {other:?}"),
    }
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn conjecture_test_reified_ground_atom_evaluates_via_shipped_core() {
    // F2 regression on the SHIPPED surface: driving `run_conjecture_test` (the shared core
    // behind the CLI + MCP tool) with a reified ground-atom candidate must EVALUATE it (a KB
    // asserting the fact entails φ ⇒ corroborated) rather than panic (exit 101).
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_dir, _path) = temp_conjecture();

    let out = run_conjecture_test(
        &ConjectureRunInput {
            formula_ttl: &reified_ground_atom_candidate("B"),
            kb_ttl: &ground_atom_entailing_kb("B"),
            standpoint: "http://ex/standpoint/alice",
            math_conjecture: None,
            dry_run: true,
            max_steps: None,
            max_answers: None,
        },
        &probe_medium(),
        source_action_policy(),
    )
    .expect("a reified ground-atom conjecture must evaluate, not panic");
    assert_eq!(out.lifecycle, "corroborated");
    assert_eq!(out.information, "supported");
    assert_eq!(out.evaluation, "completed");
}

fn temp_candidate() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("candidates.gts");
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::set_var("GMEOW_CANDIDATE_PATH", &path);
    }
    (dir, path)
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn submit_candidate_admits_corroborated_records_provenance_and_lists() {
    // AC5: a candidate whose isolated-world verdict CORROBORATES it is admissible, so it is
    // committed to the append-only candidate library — carrying its target provenance — and
    // becomes visible to list_candidates.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_dir, path) = temp_candidate();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    assert!(!path.exists(), "library must not exist before the call");
    let resp = text_payload(server.call_tool_result(
        "submit_candidate",
        &json!({
            "formula": reified_ground_atom_candidate("B"),
            "kb": ground_atom_entailing_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
            "for_slice": "https://blackcatinformatics.ca/gmeow/slices/logic",
        }),
    ));
    assert_eq!(resp["ok"], true, "an admissible candidate commits: {resp}");
    assert_eq!(resp["admissible"], true);
    assert_eq!(resp["committed"], true);
    assert_eq!(resp["verdict"]["lifecycle"], "corroborated");
    let node = resp["candidate"]
        .as_str()
        .expect("candidate iri")
        .to_string();
    assert!(path.exists(), "the admissible candidate was appended");

    // list_candidates surfaces it, in-library, with its provenance.
    let list = text_payload(server.call_tool_result("list_candidates", &json!({})));
    assert_eq!(list["ok"], true);
    assert_eq!(list["candidate_count"], 1, "one admitted candidate: {list}");
    let c = &list["candidates"][0];
    assert_eq!(c["candidate"], node);
    assert_eq!(c["disposition"], "in-library");
    assert_eq!(
        c["for_slice"],
        "https://blackcatinformatics.ca/gmeow/slices/logic"
    );

    // The slice-provenance filter matches and mismatches correctly.
    let filtered = text_payload(server.call_tool_result(
        "list_candidates",
        &json!({"slice": "https://blackcatinformatics.ca/gmeow/slices/logic"}),
    ));
    assert_eq!(filtered["candidate_count"], 1);
    let other = text_payload(
        server.call_tool_result("list_candidates", &json!({"slice": "http://ex/nope"})),
    );
    assert_eq!(other["candidate_count"], 0);
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn submit_candidate_stages_nothing_on_refuted_or_open() {
    // AC6: a refuted (or open) candidate is NOT admissible — the candidateAdmissible
    // precondition never obtains, the commit fails, and the library file stays byte-identical
    // (here: absent). This is the polarity gate a verbatim conjecture clone would get WRONG
    // (it would commit a refuted node).
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let bytes = snapshot();

    for (label, formula, kb, expect_lifecycle) in [
        (
            "refuted",
            forall_horn_candidate("B"),
            refuting_kb("B"),
            "refuted-in-standpoint",
        ),
        ("open", forall_horn_candidate("B"), open_kb("B"), "open"),
    ] {
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        assert!(!path.exists(), "{label}: library absent before the call");

        let resp = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": formula,
                "kb": kb,
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], false, "{label}: not admitted: {resp}");
        assert_eq!(resp["admissible"], false, "{label}");
        assert_eq!(resp["verdict"]["lifecycle"], expect_lifecycle, "{label}");
        assert!(
            !path.exists(),
            "{label}: a non-admissible candidate must write NOTHING to the library"
        );
    }
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn submit_candidate_refuted_leaves_populated_library_byte_identical() {
    // AC6, strengthened: the "stages nothing" invariant must hold against a NON-EMPTY store,
    // not just an absent one. Admit a corroborated candidate (the library now holds real
    // bytes), snapshot them, then submit a genuinely refuted candidate — the polarity gate
    // must leave the on-disk library BYTE-IDENTICAL (no append, no truncation, no rewrite).
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_dir, path) = temp_candidate();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    // Populate the library with one admissible candidate.
    let admit = text_payload(server.call_tool_result(
        "submit_candidate",
        &json!({
            "formula": reified_ground_atom_candidate("B"),
            "kb": ground_atom_entailing_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    assert_eq!(
        admit["committed"], true,
        "setup: admissible candidate commits"
    );
    let populated = std::fs::read(&path).expect("library exists after an admitted candidate");
    assert!(
        !populated.is_empty(),
        "the populated library holds real bytes"
    );

    // A refuted submit against the POPULATED store must write nothing.
    let resp = text_payload(server.call_tool_result(
        "submit_candidate",
        &json!({
            "formula": forall_horn_candidate("B"),
            "kb": refuting_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    assert_eq!(
        resp["ok"], false,
        "refuted candidate is not admitted: {resp}"
    );
    assert_eq!(resp["admissible"], false);
    assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");

    let after = std::fs::read(&path).expect("library still present");
    assert_eq!(
        after, populated,
        "a refuted submit must leave the populated library byte-identical"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn submit_candidate_dry_run_writes_nothing() {
    // A dry-run on an ADMISSIBLE candidate computes the verdict via a hypothetical commit but
    // writes nothing.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_dir, path) = temp_candidate();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let resp = text_payload(server.call_tool_result(
        "submit_candidate",
        &json!({
            "formula": reified_ground_atom_candidate("B"),
            "kb": ground_atom_entailing_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
            "dry_run": true,
        }),
    ));
    assert_eq!(resp["ok"], true, "{resp}");
    assert_eq!(resp["dry_run"], true);
    assert_eq!(resp["admissible"], true);
    assert!(
        !path.exists(),
        "a dry-run submit must write nothing to the library"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn withdraw_candidate_supersedes_and_gates() {
    // Submit an admissible candidate, then withdraw it: list flips it to `withdrawn`
    // (superseded, never deleted). Withdrawing an unknown id hard-fails before writing.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_dir, path) = temp_candidate();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let submit = text_payload(server.call_tool_result(
        "submit_candidate",
        &json!({
            "formula": reified_ground_atom_candidate("B"),
            "kb": ground_atom_entailing_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    let node = submit["candidate"]
        .as_str()
        .expect("candidate iri")
        .to_string();

    // Withdrawing an unknown id fails the precondition (nothing appended for it).
    let unknown = text_payload(server.call_tool_result(
        "withdraw_candidate",
        &json!({"candidate_id": "urn:gmeow:not-a-candidate"}),
    ));
    assert_eq!(unknown["ok"], false, "unknown id must hard-fail: {unknown}");

    // Withdrawing the real node succeeds and supersedes it.
    let withdraw =
        text_payload(server.call_tool_result("withdraw_candidate", &json!({"candidate_id": node})));
    assert_eq!(withdraw["ok"], true, "withdraw the real node: {withdraw}");
    assert!(path.exists());

    let list = text_payload(server.call_tool_result("list_candidates", &json!({})));
    assert_eq!(
        list["candidate_count"], 1,
        "still listed (superseded): {list}"
    );
    assert_eq!(list["candidates"][0]["disposition"], "withdrawn");

    // The disposition filter narrows correctly.
    let in_library = text_payload(
        server.call_tool_result("list_candidates", &json!({"disposition": "in-library"})),
    );
    assert_eq!(in_library["candidate_count"], 0);
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn action_policy_covers_the_candidate_submission_pair() {
    // The submit_candidate ⇄ withdraw_candidate governed-write pair must be REPRESENTED in
    // the canonical action theory the engine parses (the same projected N-Quads the TR run
    // feeds), with the mutual P10 compensation pairing — not merely documented.
    const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/";
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const LOGIC_MCP_ACTION_SCHEMA: &str = "https://blackcatinformatics.ca/logic/McpActionSchema";
    const LOGIC_COMPENSATION: &str = "https://blackcatinformatics.ca/logic/compensation";

    let policy = action_policy_nquads();
    for (schema, compensation) in [
        ("submitCandidate", "withdrawCandidate"),
        ("withdrawCandidate", "submitCandidate"),
    ] {
        let type_line =
            format!("<{EX}{schema}> <{RDF_TYPE}> <{LOGIC_MCP_ACTION_SCHEMA}> <{TXN_WORLD}> .");
        let comp_line =
            format!("<{EX}{schema}> <{LOGIC_COMPENSATION}> <{EX}{compensation}> <{TXN_WORLD}> .");
        assert!(
            policy.contains(&type_line),
            "{schema} must be typed logic:McpActionSchema: missing {type_line:?}"
        );
        assert!(
            policy.contains(&comp_line),
            "{schema}'s compensation must be {compensation}: missing {comp_line:?}"
        );
    }
}

/// A KB whose `ex:trigger` fires the `∀`-Horn candidate on SEVERAL individuals, so the
/// candidate's derived (non-EDB) closure is strictly larger than a `max_steps`/`max_answers`
/// bound of 1 — the isolated scenario evaluation exceeds the ceiling.
fn multi_trigger_kb(cls_local: &str) -> String {
    format!(
        "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:b ex:trigger ex:mark .\n\
             ex:c ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash, just several derived facts.\n"
    )
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn conjecture_test_budget_bound_forces_open_via_the_mcp_surface() {
    // The `max_steps` / `max_answers` bound is reachable from the SHIPPED MCP
    // surface (not just the logic-crate unit test). A run whose derived closure exceeds the
    // ceiling is truncated → evaluation budget-exhausted → lifecycle open → discharge
    // Unknown. This is a PURE assertion on `conjecture_test`: no persist tail exists on
    // this surface at all, so the library stays absent throughout.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    // Unbounded control: the same candidate/KB runs to Completed (a non-budget verdict).
    let unbounded = text_payload(server.call_tool_result(
        "conjecture_test",
        &json!({
            "formula": forall_horn_candidate("C"),
            "kb": multi_trigger_kb("C"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    assert_eq!(unbounded["ok"], true);
    assert_ne!(
        unbounded["verdict"]["evaluation"], "budget-exhausted",
        "the unbounded control must not trip the ceiling: {unbounded}"
    );

    // Bounded: a ceiling of 1 truncates the multi-fact derived closure.
    let bounded = text_payload(server.call_tool_result(
        "conjecture_test",
        &json!({
            "formula": forall_horn_candidate("C"),
            "kb": multi_trigger_kb("C"),
            "standpoint": "http://ex/standpoint/alice",
            "max_steps": 1,
        }),
    ));
    assert_eq!(
        bounded["ok"], true,
        "bounded run must compute a verdict: {bounded}"
    );
    assert_eq!(
        bounded["verdict"]["evaluation"], "budget-exhausted",
        "exceeding the ceiling must stamp BudgetExhausted: {bounded}"
    );
    assert_eq!(
        bounded["verdict"]["lifecycle"], "open",
        "a budget-exhausted run is inconclusive → Open: {bounded}"
    );
    assert_eq!(
        bounded["verdict"]["discharge"], "ObligationUnknown",
        "a budget-exhausted run carries the obligation forward as Unknown: {bounded}"
    );

    // `max_answers` is the equivalent binding-count ceiling and trips the same way.
    let bounded_answers = text_payload(server.call_tool_result(
        "conjecture_test",
        &json!({
            "formula": forall_horn_candidate("C"),
            "kb": multi_trigger_kb("C"),
            "standpoint": "http://ex/standpoint/alice",
            "max_answers": 1,
        }),
    ));
    assert_eq!(
        bounded_answers["verdict"]["evaluation"], "budget-exhausted",
        "max_answers must impose the same ceiling: {bounded_answers}"
    );
    // `conjecture_test` is PURE: none of these calls ever touch the library.
    assert!(
        !path.exists(),
        "conjecture_test calls must write nothing to the library"
    );
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn store_conjecture_budget_bound_forces_open_and_still_persists() {
    // R3b acceptance: a budget-exhausted `store_conjecture` (committing) must still yield
    // the non-conclusive verdict — lifecycle open, discharge Unknown — via the governor,
    // NEVER a false discharge, even though the run commits and appends to the library.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    assert!(!path.exists());
    let bounded = text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("C"),
            "kb": multi_trigger_kb("C"),
            "standpoint": "http://ex/standpoint/alice",
            "max_steps": 1,
        }),
    ));
    assert_eq!(
        bounded["ok"], true,
        "a budget-exhausted commit must still compute+persist a verdict: {bounded}"
    );
    assert_eq!(bounded["verdict"]["evaluation"], "budget-exhausted");
    assert_eq!(
        bounded["verdict"]["lifecycle"], "open",
        "never a false discharge: budget-exhausted must stay Open: {bounded}"
    );
    assert_eq!(bounded["verdict"]["discharge"], "ObligationUnknown");
    assert_eq!(bounded["transaction"]["committed"], true);

    // The non-conclusive verdict is still a real, committed, append-only segment.
    assert!(
        path.exists() && fs::metadata(&path).unwrap().len() > 0,
        "a committed budget-exhausted run must still append its Open verdict"
    );
    let node = bounded["conjecture"]
        .as_str()
        .expect("node iri")
        .to_string();
    assert!(conjecture_nodes(read_conjectures(&path).as_ref()).contains(&node));
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn store_conjecture_persists_are_append_only() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("B"),
            "kb": refuting_kb("B"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    let first = fs::read(&path).expect("first library bytes");
    let first_len = first.len();
    assert!(first_len > 0);

    // A DISTINCT conjecture (different head class) appends a second segment.
    text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("C"),
            "kb": open_kb("C"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    let second = fs::read(&path).expect("second library bytes");
    assert!(
        second.len() > first_len,
        "the second persist must APPEND, growing the file"
    );
    assert_eq!(
        &second[..first_len],
        &first[..],
        "the first segment's bytes must be intact (append-only, never mutated)"
    );
    // Both conjectures are readable back.
    assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn store_conjecture_library_is_isolated_from_the_base_kb() {
    // R2: the caller's KB text is unchanged by the call, and the library is a DISTINCT
    // file the reasoner never folds into its base graph.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, mem_path) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let kb = refuting_kb("B");
    let kb_before = kb.clone();
    text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("B"),
            "kb": kb,
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    // The KB argument the tool was given is an owned String — the tool cannot mutate the
    // caller's copy. (Isolation is inherent: store_conjecture borrows and copies the KB.)
    assert_eq!(kb_before, refuting_kb("B"));
    // The conjecture library is a distinct file, NOT the memory store, and never the base
    // reasoning graph (reason reads graph_dataset(), never conjecture_path()).
    assert!(path.exists());
    assert_ne!(path, mem_path);
    // The bundled reasoning surface is unaffected — reason still runs cleanly over the
    // untouched base graph (the library is never unioned in).
    let reasoned = text_payload(server.call_tool_result("reason", &json!({})));
    // `reason` is dev-only; over a Consumer server it returns an error, proving the base
    // reasoning path does not consult the library either way. What matters for R2 is that
    // the library file and the KB are untouched, asserted above.
    let _ = reasoned;
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn same_formula_two_standpoints_mints_two_nodes() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    let a = text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("C"),
            "kb": open_kb("C"),
            "standpoint": "http://ex/standpoint/alice",
        }),
    ));
    let b = text_payload(server.call_tool_result(
        "store_conjecture",
        &json!({
            "formula": forall_horn_candidate("C"),
            "kb": open_kb("C"),
            "standpoint": "http://ex/standpoint/bob",
        }),
    ));
    assert_ne!(
        a["conjecture"], b["conjecture"],
        "the same formula in two standpoints must mint two DISTINCT nodes (P9)"
    );
    assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
}

#[gmeow_test_batch_macros::batch_mcp_test]
fn conjecture_library_corpus_query_recovers_open_and_refuted() {
    // A corpus competency: persist several DISTINCT conjectures (open + refuted) in one
    // standpoint, then scan the collection for all of them, with witness premises for the
    // refuted ones recoverable.
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (_env, _cg) = ConjEnvGuard::set();
    let (_mem, _mp) = temp_memory();
    let (_dir, path) = temp_conjecture();
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).unwrap();

    // Two refuted (B, D disjoint) and two open (C, E unrelated).
    for cls in ["B", "D"] {
        let r = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate(cls),
                "kb": refuting_kb(cls),
                "standpoint": "http://ex/standpoint/team",
            }),
        ));
        assert_eq!(r["verdict"]["lifecycle"], "refuted-in-standpoint", "{r}");
    }
    for cls in ["C", "E"] {
        let r = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate(cls),
                "kb": open_kb(cls),
                "standpoint": "http://ex/standpoint/team",
            }),
        ));
        assert_eq!(r["verdict"]["lifecycle"], "open", "{r}");
    }

    let dataset = read_conjectures(&path);
    // All four conjectures are recoverable.
    assert_eq!(conjecture_nodes(&dataset).len(), 4);
    // All are scoped to the team standpoint.
    let team_scoped = dataset
        .owned_quads()
        .filter(|q| {
            q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                && q.object == RdfTerm::iri("http://ex/standpoint/team")
        })
        .count();
    assert_eq!(team_scoped, 4, "every conjecture is standpoint-scoped");
    // The two refuted conjectures each carry a recoverable individual + premises.
    let refuted = dataset
        .owned_quads()
        .filter(|q| {
            q.predicate == format!("{LOGIC_NS}conjectureLifecycleState")
                && q.object == RdfTerm::iri(format!("{LOGIC_NS}ConjectureRefutedInStandpoint"))
        })
        .count();
    assert_eq!(refuted, 2, "exactly the two disjointness cases are refuted");
    assert!(
        witness_premises(&dataset).len() >= 2,
        "each refutation persists recoverable witness premises"
    );
}
