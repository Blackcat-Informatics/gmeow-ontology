// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Consumer documentation, cards and search contracts.

use super::*;

/// `counter_examples` over the live bundle: a term documenting fixtures yields
/// the real, split fixture bodies; a resolvable term documenting none yields the
/// honest empty-but-ok shape; an unknown term is a hard error envelope.
///
/// `gmeow:Activity` documents BOTH a well-formed exemplar and a counter-example
/// in the shipped `gmeow:graph/documentation` graph (verified by the projection
/// query in the shipped surface); `gmeow:AboutnessMode` is a documented term that authors no
/// fixtures.
#[gmeow_test_batch_macros::batch_mcp_test]
fn tool_counter_examples_surface() {
    let server = consumer_server();

    // A term with fixtures → real, non-empty split bodies.
    let hit = text_payload(
        server.call_tool_result("counter_examples", &json!({"term": "gmeow:Activity"})),
    );
    assert_eq!(hit["ok"], true);
    assert_eq!(hit["term"], "gmeow:Activity");
    let counter = hit["counter_examples"]
        .as_array()
        .expect("counter_examples is an array");
    let wellformed = hit["wellformed"]
        .as_array()
        .expect("wellformed is an array");
    assert!(
        !counter.is_empty(),
        "gmeow:Activity documents at least one counter-example: {hit}"
    );
    assert!(
        !wellformed.is_empty(),
        "gmeow:Activity documents at least one well-formed exemplar: {hit}"
    );
    // The counter-example carries a real Turtle body AND a violation code.
    let ce = &counter[0];
    assert!(
        ce["text"].as_str().is_some_and(|t| t.contains(':')),
        "counter-example carries a real Turtle body: {ce}"
    );
    assert!(
        ce["violation_code"].as_str().is_some_and(|c| !c.is_empty()),
        "counter-example carries an authored violation code: {ce}"
    );
    // The well-formed exemplar has a real body and NO violation code.
    let wf = &wellformed[0];
    assert!(
        wf["text"].as_str().is_some_and(|t| t.contains(':')),
        "well-formed exemplar carries a real Turtle body: {wf}"
    );
    assert!(
        wf["violation_code"].is_null(),
        "well-formed exemplar has no violation code: {wf}"
    );
    // Deterministic: a second call is byte-identical.
    let again = text_payload(
        server.call_tool_result("counter_examples", &json!({"term": "gmeow:Activity"})),
    );
    assert_eq!(hit, again, "counter_examples output is deterministic");

    // A resolvable term with NO fixtures → empty-but-ok (NOT an error).
    let empty = text_payload(
        server.call_tool_result("counter_examples", &json!({"term": "gmeow:AboutnessMode"})),
    );
    assert_eq!(
        empty["ok"], true,
        "no-fixture term is empty-but-ok: {empty}"
    );
    assert_eq!(empty["wellformed"], json!([]));
    assert_eq!(empty["counter_examples"], json!([]));

    // An unknown term → hard error envelope.
    let unknown = text_payload(server.call_tool_result(
        "counter_examples",
        &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
    ));
    assert_eq!(
        unknown["ok"], false,
        "unknown term is a hard error: {unknown}"
    );
    assert!(
        unknown["error"]
            .as_str()
            .is_some_and(|e| e.contains("unknown term")),
        "unknown-term error names the failure: {unknown}"
    );
}

/// `entailments` over the live bundle: a term with derivations yields every
/// entailment's rule/conclusion with its premises preserved; an unknown term is a
/// hard error. `gmeow:Entity` grounds >1000 entailment records in the shipped
/// documentation graph.
#[gmeow_test_batch_macros::batch_mcp_test]
fn tool_entailments_surface() {
    let server = consumer_server();

    let hit =
        text_payload(server.call_tool_result("entailments", &json!({"term": "gmeow:Entity"})));
    assert_eq!(hit["ok"], true);
    assert_eq!(hit["term"], "gmeow:Entity");
    let entailments = hit["entailments"]
        .as_array()
        .expect("entailments is an array");
    assert!(
        !entailments.is_empty(),
        "gmeow:Entity grounds at least one entailment: {}",
        &hit.to_string()[..hit.to_string().len().min(400)]
    );
    // Every record carries a non-empty rule and conclusion.
    for e in entailments {
        assert!(
            e["rule"].as_str().is_some_and(|r| !r.is_empty()),
            "entailment carries a rule: {e}"
        );
        assert!(
            e["conclusion"].as_str().is_some_and(|c| !c.is_empty()),
            "entailment carries a conclusion: {e}"
        );
        assert!(e["premises"].is_array(), "premises is an array: {e}");
    }
    // Premises are preserved: at least one derivation carries premises.
    let total_premises: usize = entailments
        .iter()
        .map(|e| e["premises"].as_array().map_or(0, Vec::len))
        .sum();
    assert!(
        total_premises > 0,
        "at least one gmeow:Entity entailment preserves its premises"
    );

    // Unknown term → hard error envelope.
    let unknown = text_payload(server.call_tool_result(
        "entailments",
        &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
    ));
    assert_eq!(
        unknown["ok"], false,
        "unknown term is a hard error: {unknown}"
    );
}

/// Select the RICHEST-surface term in the shipped bundle for the tier tests:
/// the term (a) grounding at least one reasoner entailment AND (b) documenting at
/// least one conformance fixture, maximizing the total panel count (entailments +
/// fixtures), tie-broken by IRI so the choice is deterministic. Panics (a real
/// blocker, never a soft skip) if the bundle documents no such term.
///
/// Uses exactly TWO bulk queries (the entailment map + a single fixture-by-term
/// scan) and intersects them — never a per-term query per candidate.
fn richest_card_term(server: &McpServer) -> String {
    let entailments = server
        .view
        .entailment_map()
        .expect("entailment map from the shipped documentation graph");
    // One bulk scan: fixture count per documented term.
    let fixtures_query = format!(
        "PREFIX gm: <{GMEOW_NS}>\nSELECT ?term ?f WHERE {{ ?f a gm:DocFixture ; \
             gm:documents ?term . }}"
    );
    let mut fixtures_per_term: BTreeMap<String, usize> = BTreeMap::new();
    for row in server
        .view
        .docs_select_rows(&fixtures_query)
        .expect("fixture-by-term scan over graph/documentation")
    {
        if let Some(term) = row.get("term") {
            *fixtures_per_term.entry(term.clone()).or_default() += 1;
        }
    }
    let mut candidates: Vec<(usize, String)> = entailments
        .iter()
        .filter_map(|(iri, ents)| {
            fixtures_per_term
                .get(iri)
                .map(|&fx| (ents.len() + fx, iri.clone()))
        })
        .collect();
    // Most panels first, then lexicographically-first IRI (deterministic).
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    candidates
        .into_iter()
        .next()
        .map(|(_, iri)| iri)
        .expect("the shipped bundle documents a term with entailments AND fixtures")
}

/// A documented term that still carries full-tier panels (≥1 entailment AND ≥1
/// fixture) but the FEWEST of them — the cheapest term whose `full` card exercises
/// every rich-panel path. Tier/determinism assertions that render the full card
/// several times use this instead of [`richest_card_term`] (whose ~1000-entailment
/// surface is only needed for the byte-ceiling proof), so they render fast.
fn modest_panel_card_term(server: &McpServer) -> String {
    let entailments = server
        .view
        .entailment_map()
        .expect("entailment map from the shipped documentation graph");
    let fixtures_query = format!(
        "PREFIX gm: <{GMEOW_NS}>\nSELECT ?term ?f WHERE {{ ?f a gm:DocFixture ; \
             gm:documents ?term . }}"
    );
    let mut fixtures_per_term: BTreeMap<String, usize> = BTreeMap::new();
    for row in server
        .view
        .docs_select_rows(&fixtures_query)
        .expect("fixture-by-term scan over graph/documentation")
    {
        if let Some(term) = row.get("term") {
            *fixtures_per_term.entry(term.clone()).or_default() += 1;
        }
    }
    let mut candidates: Vec<(usize, String)> = entailments
        .iter()
        .filter_map(|(iri, ents)| {
            fixtures_per_term
                .get(iri)
                .map(|&fx| (ents.len() + fx, iri.clone()))
        })
        .collect();
    // FEWEST panels first, then lexicographically-first IRI (deterministic).
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    candidates
        .into_iter()
        .next()
        .map(|(_, iri)| iri)
        .expect("the shipped bundle documents a term with entailments AND fixtures")
}

/// `doc_card` tiers: `summary` is the leanest surface (title + definition only,
/// under a pinned byte ceiling, none of the advisory / panel sections); `full`
/// is strictly larger and carries the rich oracle panels (Entailments / Do /
/// Don't headers).
#[gmeow_test_batch_macros::batch_mcp_test]
fn tool_doc_card_tier_byte_ceiling() {
    const SUMMARY_CEILING: usize = 1500;
    let server = consumer_server();
    let term = richest_card_term(&server);

    let summary = text_payload(
        server.call_tool_result("doc_card", &json!({"term": term, "detail": "summary"})),
    );
    let full =
        text_payload(server.call_tool_result("doc_card", &json!({"term": term, "detail": "full"})));
    let s_card = summary["card"].as_str().expect("summary markdown card");
    let f_card = full["card"].as_str().expect("full markdown card");

    // Summary is title + definition ONLY, under the ceiling.
    assert!(
        s_card.len() < SUMMARY_CEILING,
        "summary card ({} bytes) must be under {SUMMARY_CEILING}: {s_card}",
        s_card.len()
    );
    assert!(
        s_card.starts_with("# "),
        "summary carries the H1 title: {s_card}"
    );
    let summary_body = s_card
        .strip_prefix("# ")
        .and_then(|s| s.split_once("\n\n"))
        .map_or("", |(_, rest)| rest);
    assert!(
        !summary_body.trim().is_empty(),
        "summary carries a definition after the title: {s_card}"
    );
    // NONE of the advisory / metadata / panel surface at the summary tier.
    assert!(
        !s_card.contains("- category:"),
        "no metadata header: {s_card}"
    );
    assert!(
        !s_card.contains("**Use when:**"),
        "no advisory fields: {s_card}"
    );
    assert!(
        !s_card.contains("## Entailments"),
        "no rich panels: {s_card}"
    );

    // Full is strictly larger and carries the panel section headers.
    assert!(
        f_card.len() > s_card.len(),
        "full ({}) must exceed summary ({})",
        f_card.len(),
        s_card.len()
    );
    assert!(
        f_card.contains("## Entailments"),
        "full carries Entailments: {f_card}"
    );
    assert!(
        f_card.contains("## Do") || f_card.contains("## Don't"),
        "full carries a Do / Don't fixture panel: {f_card}"
    );
}

/// Single-renderer authority: `doc_card` at `detail=standard` is BYTE-IDENTICAL
/// to rendering the shared compact `Card` through `render_card` at `Standard` —
/// the tier gating never perturbs the docs-site card the standard tier mirrors.
#[gmeow_test_batch_macros::batch_mcp_test]
fn tool_doc_card_standard_is_byte_identical_to_compact_render() {
    let server = consumer_server();
    let term = "gmeow:EntityExistence";

    let envelope = text_payload(
        server.call_tool_result("doc_card", &json!({"term": term, "detail": "standard"})),
    );
    let card_md = envelope["card"].as_str().expect("standard markdown card");

    // Independent expected: the SAME shared builder + renderer at Standard.
    let requested = server.startup_requested.clone();
    let modeled_defs = server.view.modeled_defs();
    let expected = server.view.with_terms(requested, |terms| {
        let (title, card) = export::doc_card_build(terms, term, &modeled_defs)
            .resolved()
            .expect("known term resolves");
        gmeow_docs_model::card::render_card(
            &title,
            &card,
            gmeow_docs_model::card::CardDetail::Standard,
        )
    });
    assert_eq!(
        card_md, expected,
        "standard tier must be byte-identical to the compact single-renderer output"
    );
    // Default (no `detail`) is Standard.
    let defaulted = text_payload(server.call_tool_result("doc_card", &json!({"term": term})));
    assert_eq!(defaulted["detail"], "standard");
    assert_eq!(defaulted["card"].as_str().expect("card"), expected);
}

/// `doc_card` `format=json`: byte-stable across calls; standard-tier JSON omits
/// the full-tier rich fields; full-tier JSON carries them.
#[gmeow_test_batch_macros::batch_mcp_test]
fn tool_doc_card_json_determinism_and_tier_fields() {
    let server = consumer_server();
    let docs_a = server.view.documentation().unwrap();
    let docs_b = server.view.documentation().unwrap();
    assert!(
        Arc::ptr_eq(docs_a, docs_b),
        "documentation queries must share one projected graph"
    );
    // Determinism + tier-field presence hold for ANY term that carries panels,
    // so use the cheapest such term (not the ~1000-entailment richest term) —
    // this test renders the full card three times, so a lean term keeps it fast.
    let term = modest_panel_card_term(&server);

    // Byte-identical raw tool text across two identical calls.
    let call = || {
        server.call_tool_result(
            "doc_card",
            &json!({"term": term, "detail": "full", "format": "json"}),
        )["content"][0]["text"]
            .as_str()
            .expect("json tool text")
            .to_string()
    };
    assert_eq!(call(), call(), "json card is byte-stable across calls");

    // Standard-tier JSON: a Card object WITHOUT the full-tier rich fields.
    let std_json = text_payload(server.call_tool_result(
        "doc_card",
        &json!({"term": term, "detail": "standard", "format": "json"}),
    ));
    assert_eq!(std_json["format"], "json");
    let std_card = &std_json["card"];
    assert!(std_card.is_object(), "json card is an object: {std_card}");
    assert!(
        std_card.get("entailments").is_none(),
        "no entailments at standard"
    );
    assert!(
        std_card.get("fixtures_do").is_none(),
        "no fixtures_do at standard"
    );
    assert!(
        std_card.get("fixtures_dont").is_none(),
        "no fixtures_dont at standard"
    );
    assert!(
        std_card.get("diagnostics").is_none(),
        "no diagnostics at standard"
    );
    assert!(std_card.get("loss").is_none(), "no loss at standard");

    // Full-tier JSON DOES carry the rich panels.
    let full_json = text_payload(server.call_tool_result(
        "doc_card",
        &json!({"term": term, "detail": "full", "format": "json"}),
    ));
    assert!(
        full_json["card"].get("entailments").is_some(),
        "full json carries entailments: {}",
        &full_json.to_string()[..full_json.to_string().len().min(400)]
    );
}

/// `doc_card` cost metadata: every envelope carries positive `bytes`/`tokens`,
/// monotonically non-decreasing across summary ≤ standard ≤ full for one term.
#[gmeow_test_batch_macros::batch_mcp_test]
fn tool_doc_card_cost_metadata_is_monotone() {
    let server = consumer_server();
    let term = richest_card_term(&server);

    let tier = |detail: &str| {
        text_payload(server.call_tool_result("doc_card", &json!({"term": term, "detail": detail})))
    };
    let summary = tier("summary");
    let standard = tier("standard");
    let full = tier("full");

    for env in [&summary, &standard, &full] {
        assert!(
            env["bytes"].as_u64().expect("bytes") > 0,
            "bytes > 0: {env}"
        );
        assert!(
            env["tokens"].as_u64().expect("tokens") > 0,
            "tokens > 0: {env}"
        );
    }
    let bytes = |e: &Value| e["bytes"].as_u64().unwrap();
    let tokens = |e: &Value| e["tokens"].as_u64().unwrap();
    assert!(
        bytes(&summary) <= bytes(&standard),
        "bytes summary ≤ standard"
    );
    assert!(bytes(&standard) <= bytes(&full), "bytes standard ≤ full");
    assert!(
        tokens(&summary) <= tokens(&standard),
        "tokens summary ≤ standard"
    );
    assert!(tokens(&standard) <= tokens(&full), "tokens standard ≤ full");

    // Unknown detail / format is a hard error listing the valid values.
    let bad_detail = text_payload(
        server.call_tool_result("doc_card", &json!({"term": term, "detail": "verbose"})),
    );
    assert_eq!(
        bad_detail["ok"], false,
        "unknown detail hard-fails: {bad_detail}"
    );
    let bad_format =
        text_payload(server.call_tool_result("doc_card", &json!({"term": term, "format": "yaml"})));
    assert_eq!(
        bad_format["ok"], false,
        "unknown format hard-fails: {bad_format}"
    );
}

/// `doc_card` full tier populates the rich panels FROM the documentation graph:
/// the full markdown inlines an actual entailment conclusion and a fixture title
/// the sibling `entailments` / `counter_examples` tools report for the term.
#[gmeow_test_batch_macros::batch_mcp_test]
fn tool_doc_card_full_inlines_graph_panels() {
    let server = consumer_server();
    let term = richest_card_term(&server);

    let full =
        text_payload(server.call_tool_result("doc_card", &json!({"term": term, "detail": "full"})));
    let f_card = full["card"].as_str().expect("full markdown card");

    // An entailment's conclusion (from the SAME graph the `entailments` tool reads).
    let ents = text_payload(server.call_tool_result("entailments", &json!({"term": term})));
    let conclusion = ents["entailments"][0]["conclusion"]
        .as_str()
        .expect("the richest term grounds an entailment with a conclusion");
    assert!(
        f_card.contains(conclusion),
        "full card inlines the entailment conclusion {conclusion:?}"
    );

    // A fixture title (from the SAME graph the `counter_examples` tool reads).
    let fixtures =
        text_payload(server.call_tool_result("counter_examples", &json!({"term": term})));
    let title = fixtures["counter_examples"]
        .get(0)
        .and_then(|f| f["title"].as_str())
        .or_else(|| {
            fixtures["wellformed"]
                .get(0)
                .and_then(|f| f["title"].as_str())
        })
        .expect("the richest term documents a fixture with a title");
    assert!(
        f_card.contains(title),
        "full card inlines the fixture title {title:?}"
    );
}

/// `competency_questions` over the live bundle: the index form (no `term`) returns
/// every runnable question, each carrying a `query_text` that PARSES as a valid
/// SPARQL SELECT; the per-term form returns that term's subset. `gmeow:Agent`
/// documents competency questions in the shipped documentation graph.
#[gmeow_test_batch_macros::batch_mcp_test]
fn tool_competency_questions_surface() {
    let server = consumer_server();

    // Index form: no `term`.
    let index = text_payload(server.call_tool_result("competency_questions", &json!({})));
    assert_eq!(index["ok"], true);
    assert!(
        index.get("term").is_none(),
        "index form carries no term key: {}",
        &index.to_string()[..index.to_string().len().min(200)]
    );
    let questions = index["questions"]
        .as_array()
        .expect("questions is an array");
    assert!(!questions.is_empty(), "the competency index is non-empty");
    for q in questions {
        assert!(
            q["query_text"].as_str().is_some_and(|t| !t.is_empty()),
            "every competency question carries a runnable query_text: {q}"
        );
    }

    // The first question's query_text round-trips through the native SPARQL
    // parser as an executable SELECT (over an empty dataset — parse+plan only).
    let first_query = questions[0]["query_text"].as_str().expect("query_text");
    let empty = std::sync::Arc::new(
        purrdf::RdfDatasetBuilder::new()
            .freeze()
            .expect("empty dataset"),
    );
    let parsed = gmeow_bundle_view::native_query::query(&empty, first_query)
        .expect("competency query_text is a valid SPARQL query");
    assert!(
        matches!(parsed, purrdf::SparqlResult::Solutions { .. }),
        "competency query_text is a SPARQL SELECT: {first_query}"
    );

    // Deterministic index.
    let index_again = text_payload(server.call_tool_result("competency_questions", &json!({})));
    assert_eq!(index, index_again, "competency index is deterministic");

    // Per-term form: gmeow:Agent's subset — non-empty, each with a query_text.
    let per_term = text_payload(
        server.call_tool_result("competency_questions", &json!({"term": "gmeow:Agent"})),
    );
    assert_eq!(per_term["ok"], true);
    assert_eq!(per_term["term"], "gmeow:Agent");
    let agent_questions = per_term["questions"]
        .as_array()
        .expect("questions is an array");
    assert!(
        !agent_questions.is_empty(),
        "gmeow:Agent documents at least one competency question: {per_term}"
    );
    for q in agent_questions {
        assert!(
            q["query_text"].as_str().is_some_and(|t| !t.is_empty()),
            "per-term competency question carries a query_text: {q}"
        );
    }
    assert!(
        agent_questions.len() <= questions.len(),
        "a term's competency subset is no larger than the whole index"
    );

    // Unknown term (per-term form) → hard error envelope.
    let unknown = text_payload(server.call_tool_result(
        "competency_questions",
        &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
    ));
    assert_eq!(
        unknown["ok"], false,
        "unknown term is a hard error: {unknown}"
    );
}

/// A tiny synthetic documentation dataset for the `search_documentation` unit
/// tests: two class terms whose `to_gmeow_rdf` projection carries the new
/// `docSearch*` facets, parsed back into an [`purrdf::RdfDataset`] exactly the way
/// the production carrier holds the bundle's documentation graph. This does NOT
/// depend on the committed bundle (which only gains the facets after regenerate) —
/// it exercises the projection + search end to end from the model.
fn synthetic_docs_dataset() -> Arc<purrdf::RdfDataset> {
    use gmeow_docs_model::model::{DocLinkage, DocTerm, DocTermCategory};
    let ns = "https://blackcatinformatics.ca/gmeow/";
    let model = gmeow_docs_model::model::DocsModel {
        terms: vec![
            DocTerm {
                iri: format!("{ns}Cat"),
                curie: "gmeow:Cat".to_string(),
                label: Some("Cat".to_string()),
                definition: Some("A small domesticated feline.".to_string()),
                category: DocTermCategory::Class,
                owner_slice: format!("{ns}slice/zoo"),
                scope_notes: vec!["Prefer for a domestic cat; avoid for a wildcat.".to_string()],
                ..Default::default()
            },
            DocTerm {
                iri: format!("{ns}Feline"),
                curie: "gmeow:Feline".to_string(),
                label: Some("Feline".to_string()),
                definition: Some("The cat family of mammals.".to_string()),
                category: DocTermCategory::Class,
                owner_slice: format!("{ns}slice/zoo"),
                ..Default::default()
            },
        ],
        // A crosswalk linkage on Cat → the alignment facet token `exactMatch:Q146`.
        linkages: vec![DocLinkage {
            mapping_set: None,
            subject: format!("{ns}Cat"),
            subject_curie: "gmeow:Cat".to_string(),
            predicate: "http://www.w3.org/2004/02/skos/core#exactMatch".to_string(),
            object: "http://www.wikidata.org/entity/Q146".to_string(),
            justification: None,
            confidence: None,
            owner_slice: format!("{ns}slice/zoo"),
        }],
        ..Default::default()
    };
    let nquads = gmeow_docs_model::rdf::to_gmeow_rdf(&model, &BTreeMap::new());
    purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
        .expect("to_gmeow_rdf emits valid N-Quads")
}

/// `search_documentation` over the synthetic dataset: matches on label /
/// definition / advice, attaches the advice + alignment + missing-coverage facets,
/// ranks a label match above a definition match, returns empty for a non-match, and
/// is deterministic.
#[gmeow_test_batch_macros::batch_mcp_test]
fn search_documentation_matches_facets_ranks_and_is_deterministic() {
    let dataset_arc = synthetic_docs_dataset();
    let dataset = Arc::new(
        dataset_arc
            .project_graph(Some(&purrdf::RdfTerm::iri(
                gmeow_bundle_view::graph_iris::GRAPH_DOCUMENTATION,
            )))
            .unwrap(),
    );
    let cat_iri = "https://blackcatinformatics.ca/gmeow/Cat";
    let feline_iri = "https://blackcatinformatics.ca/gmeow/Feline";

    // "cat": Cat matches by LABEL (rank 0); Feline matches by DEFINITION ("the cat
    // family …", rank 1) — so Cat sorts first.
    let hits = search_documentation(&dataset, "cat", 20).expect("search ok");
    let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![cat_iri, feline_iri],
        "label match outranks definition match"
    );
    let cat = &hits[0];
    assert_eq!(cat.kind, "term");
    assert_eq!(cat.label, "Cat");
    assert_eq!(
        cat.definition.as_deref(),
        Some("A small domesticated feline.")
    );
    assert_eq!(
        cat.advice,
        vec!["Prefer for a domestic cat; avoid for a wildcat.".to_string()],
        "the advice facet is attached"
    );
    assert_eq!(
        cat.alignments,
        vec!["exactMatch:Q146".to_string()],
        "the alignment facet is attached"
    );
    assert!(
        !cat.missing_coverage.is_empty(),
        "an under-documented term carries missing-coverage dimensions: {:?}",
        cat.missing_coverage
    );

    // An advice-only match: "wildcat" appears only in Cat's advice prose.
    let advice_hits = search_documentation(&dataset, "wildcat", 20).expect("search ok");
    let advice_ids: Vec<&str> = advice_hits.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(advice_ids, vec![cat_iri], "advice prose is searchable");

    // A definition-only match: "mammals" appears only in Feline's definition.
    let def_hits = search_documentation(&dataset, "mammals", 20).expect("search ok");
    let def_ids: Vec<&str> = def_hits.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(def_ids, vec![feline_iri], "definition prose is searchable");

    // A non-matching query is empty-but-ok (never a hard fail on a populated graph).
    let none = search_documentation(&dataset, "xylophone", 20).expect("search ok");
    assert!(none.is_empty(), "a non-matching query returns no hits");

    // Determinism: the same query twice yields the same order.
    let a = search_documentation(&dataset, "cat", 20).expect("search ok");
    let b = search_documentation(&dataset, "cat", 20).expect("search ok");
    assert_eq!(
        a.iter().map(|h| &h.id).collect::<Vec<_>>(),
        b.iter().map(|h| &h.id).collect::<Vec<_>>(),
        "search order is reproducible"
    );

    // The limit is honored.
    let limited = search_documentation(&dataset, "cat", 1).expect("search ok");
    assert_eq!(limited.len(), 1, "limit caps the result count");
}

/// `search_documentation` HARD-FAILS when the documentation graph is absent/empty
/// — docs_search serves the documentation graph, so a missing graph is a defect,
/// never a silent empty result.
#[gmeow_test_batch_macros::batch_mcp_test]
fn search_documentation_hard_fails_on_absent_documentation_graph() {
    let empty = purrdf::RdfDatasetBuilder::new()
        .freeze()
        .expect("empty dataset");
    let err = search_documentation(&empty, "cat", 20)
        .expect_err("an absent documentation graph is a hard fail");
    assert!(
        err.to_string().contains("graph/documentation"),
        "the hard-fail error names the missing documentation graph: {err}"
    );
}

/// `docs_search` dispatches over the shipped bundle and returns an OK envelope with
/// a `results` array. (The committed bundle carries the documentation graph but not
/// yet the `docSearch*` facets — those land after regenerate — so the live match
/// set is validated by the synthetic unit test above; here we prove the tool wires
/// through, never hard-fails on the populated graph, and is deterministic.)
#[gmeow_test_batch_macros::batch_mcp_test]
fn docs_search_tool_dispatches_over_the_bundle() {
    let server = consumer_server();
    let hit = text_payload(server.call_tool_result("docs_search", &json!({"query": "entity"})));
    assert_eq!(
        hit["ok"], true,
        "docs_search returns ok over the bundle: {hit}"
    );
    assert_eq!(hit["query"], "entity");
    assert!(hit["results"].is_array(), "results is an array: {hit}");
    let again = text_payload(server.call_tool_result("docs_search", &json!({"query": "entity"})));
    assert_eq!(hit, again, "docs_search output is deterministic");
}
