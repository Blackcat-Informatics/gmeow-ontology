// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Consumer registration, protocol dispatch and tool handlers.

#[cfg(feature = "reasoning")]
use std::collections::BTreeSet;
#[cfg(feature = "reasoning")]
use std::sync::Arc;

use serde_json::{Value, json};

#[cfg(feature = "core")]
use gmeow_bundle_view::export::ConsumerResolution;
use gmeow_errors::ResultExt;
#[cfg(feature = "core")]
use gmeow_lang_bridge::{
    Gmn0Model, Gmn1Document, GmnDictionary, build_verbalization_pairs, gmn0_canonically_equal,
    gmn1_read, gmn1_write, resolve_operator_forms,
};
#[cfg(feature = "reasoning")]
use gmeow_logic::conjecture::ConjectureLifecycleState;
#[cfg(feature = "reasoning")]
use gmeow_logic::query_ir::Budget;
#[cfg(feature = "reasoning")]
use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
    reason_all_budgeted,
};
#[cfg(feature = "reasoning")]
use gmeow_logic::result_rdf::project_conjecture_withdrawal;
#[cfg(feature = "reasoning")]
use gmeow_logic::transaction::execute::TxReceipt;
#[cfg(feature = "core")]
use gmeow_validate::local_oracle;
#[cfg(feature = "reasoning")]
use purrdf::gts::examples::agent_memory::{RevisionOptions, StoreOptions};

use crate::extension::{Extension, ResourceHandler, ToolHandler, zip_resources, zip_tools};
#[cfg(feature = "core")]
use crate::libraries::TXN_WORLD;
#[cfg(feature = "reasoning")]
use crate::libraries::{
    MCP_CLAIM_IN_MEMORY, MCP_CONJECTURE_IN_LIBRARY, MCP_REVISE_BELIEF_SCHEMA,
    MCP_STORE_CLAIM_SCHEMA, MCP_TARGET_CLAIM_EXISTS, MCP_WELL_FORMED_CLAIM,
    MCP_WITHDRAW_CONJECTURE_SCHEMA, append_library_segments, build_audit_segment, build_nt_segment,
    conjecture_library, execute_memory_txn, read_library, sha256_hex, txn_json, with_library_lock,
};
#[cfg(feature = "reasoning")]
use crate::storage::{ClaimStore, StructuredPayload, ToolCallAudit, ToolCallOptions, storage};

use super::MAX_VERIFY_OVERLAY_BYTES;
#[cfg(feature = "core")]
use super::{
    ACTION_POLICY_MEDIA_TYPE, ADVISE_ORIGIN, CardFormat, LANG_GMN_UNCOVERED_TERM,
    MAX_VALIDATE_DATA_BYTES, MCP_NAMESPACE, QueryScope, SearchHit, VALIDATE_LOCAL_ORIGIN,
    ambiguous_term_err, base64_decode, base64_encode, canonical_rdf_format, expand_slice_iri,
    extract_authoring_packets, glyph_legend, gmn_signature_join, guard_gmn_size,
    harvest_dataset_labels, iri_local_name, optional_bool, parse_card_detail, search_documentation,
    unknown_term_err,
};
use super::{
    ACTION_POLICY_URI, CHASE_SEGMENT_TOOLS, CORE_SEGMENT, McpServer, READ_TOOL_COUNT,
    REASONING_SEGMENT_TOOLS, SegmentSet, WRITE_TOOL_COUNT, builtin_resource_descriptors,
    candidate_submission_descriptor, lang_from_query, optional_limit, optional_step_count,
    optional_str, rdf_media_type, required_str, resolve_lang, rpc_error, segment_not_loaded, tool,
    tool_text,
};
#[cfg(feature = "reasoning")]
use super::{
    CandidateSubmitInput, ConjecturePureOutput, ConjectureRunInput, ConjectureRunOutput,
    ConjectureRunPureInput, MAX_VERIFY_OVERLAY_QUADS, MEMORY_HOT_DICTIONARY, McpView, StoreMedium,
    TOOL_AGENT_NS, claim_json, governed_budget, object_term_n3, optional_bool_checked,
    optional_f64, optional_str_checked, recall_json, required_file_map, run_conjecture_test,
    run_conjecture_test_pure, run_list_candidates, run_submit_candidate, run_withdraw_candidate,
    store_medium, store_segment_json, tool_arguments,
};

/// The CONSUMER tool descriptors — what a bundle-only `gmeow mcp` advertises — in
/// advertised order.
///
/// This list is joined to [`builtin_tool_handlers`] index-by-index by
/// [`zip_tools`], which refuses to build unless the two agree name-for-name. Keep
/// them in the same order: a descriptor added here without its handler (or the
/// converse) is a hard construction failure, not a silently unusable tool.
fn builtin_tool_descriptors() -> Vec<Value> {
    vec![
        tool(
            "lookup_term",
            "Resolve a bundled GMEOW term.",
            &[("term", "string"), ("lang", "string")],
        ),
        tool(
            "llms_txt",
            "Return the standard bundled vocabulary index.",
            &[("lang", "string")],
        ),
        tool(
            "llms_full",
            "Return the complete inlined bundled vocabulary index.",
            &[("lang", "string")],
        ),
        tool(
            "doc_card",
            "Return a prompt-ready term card for a bundled term, wrapped in a \
                 cost-metadata envelope ({ok, detail, format, bytes, tokens, card}). \
                 `detail` selects a token-budgeted tier: `summary` (title + definition \
                 only), `standard` (the compact card — default), or `full` (the oracle \
                 card: compact card plus entailments, Do / Don't fixtures, diagnostics, \
                 and projection-loss panels, queried from gmeow:graph/documentation). \
                 `format` is `markdown` (default) or `json` (the neutral Card object). \
                 An unknown detail/format is a hard error.",
            &[
                ("term", "string"),
                ("detail", "string"),
                ("format", "string"),
                ("lang", "string"),
            ],
        ),
        tool(
            "okf_index",
            "Return the OKF manifest JSON envelope.",
            &[("lang", "string")],
        ),
        tool(
            "query_docs",
            "Run SELECT, ASK, CONSTRUCT or DESCRIBE over the bundled documentation graph \
                 (gmeow:graph/documentation). Return native RDF 1.2 SPARQL JSON bindings \
                 with direction, triple terms and scoped blanks, a boolean, or a N-Quads graph; \
                 the envelope declares its form.",
            &[("query", "string")],
        ),
        tool(
            "docs_search",
            "Full-text search the bundled documentation graph (gmeow:graph/documentation) \
                 for terms, slices, and concerns whose label, definition, or advisory prose \
                 match `query` (case-insensitive substring / token match). Returns ranked \
                 records — a label match outranks a definition match outranks an advice match, \
                 tie-broken by IRI — each with its kind, documented IRI (id), label, definition, \
                 site URL, and the same advice / alignments / missing_coverage facets the site \
                 search index carries. Pass an optional `limit` (default 20). A query that \
                 matches nothing returns an empty result list.",
            &[("query", "string"), ("limit", "integer")],
        ),
        tool(
            "query_local",
            "Run a SPARQL query — SELECT, ASK, CONSTRUCT or DESCRIBE — over a READ-ONLY \
                 overlay graph you paste inline. `data` is the RDF text and `format` DECLARES \
                 how to read it — one of turtle|ttl|text/turtle, ntriples|nt|n-triples, \
                 nquads|nq|n-quads, trig, rdfxml|rdf+xml|xml|rdf, jsonld|json-ld. `scope` \
                 DECLARES what the query runs against: `bundle` (default) reads the overlay \
                 UNIONED with the signed canon — what does this graph mean in GMEOW — while \
                 `input` reads the overlay ALONE, with no bundle triple in the answer. The \
                 overlay is loaded as an EXTERNAL, read-only annex under both scopes: its \
                 triples are isolable via GRAPH <urn:gmeow:mcp:overlay:external> but are NEVER \
                 merged into the signed gmeow: canon and NEVER written anywhere. The result \
                 FORM is declared in the envelope: `bindings` (head/results), `boolean`, or \
                 `graph` (graph_nquads + quad_count). Bindings retain RDF 1.2 language and \
                 direction, recursive triple terms and scoped blank-node identity. A missing or unrecognized `format` or \
                 `scope`, or an oversized payload, is a hard error — neither is ever guessed.",
            &[
                ("data", "string"),
                ("format", "string"),
                ("query", "string"),
                ("scope", "string"),
            ],
        ),
        tool(
            "encode_gmn1",
            "Encode authored RDF into the token-compact GMN-1 surface — the WRITE leg of \
                 the codec, and the exact inverse of gmn_expand. `data` is the RDF text and \
                 `format` DECLARES how to read it (the same codec set query_local accepts). \
                 Runs the SAME dictionary the bundle carries, so the emitted surface is the one \
                 the on-gate authority ships. Carries an internal round-trip witness (the \
                 emitted document is read back and canonical equality asserted), so it never \
                 returns a lossy encoding. Returns {ok, gmn1, canonical_nquads, round_trip}. An \
                 unrecognized format, an oversized payload, or an encoding that does not read \
                 back is a hard error.",
            &[("data", "string"), ("format", "string")],
        ),
        tool(
            "verify_graph",
            "Reason over the bundle UNIONED with a READ-ONLY overlay graph you paste \
                 inline, then run the native reasoned-graph verify (the bad-example negative \
                 tests + non-entailment obligations) and return a PROOF-CARRYING judgment. \
                 `data` is the RDF text and `format` DECLARES how to read it — one of \
                 turtle|ttl|text/turtle, ntriples|nt|n-triples, nquads|nq|n-quads, trig, \
                 rdfxml|rdf+xml|xml|rdf, jsonld|json-ld. The overlay is loaded as an EXTERNAL, \
                 read-only annex exactly like query_local: its triples join the reasoning \
                 default world (bundle \u{222a} overlay, also isolable via GRAPH \
                 <urn:gmeow:mcp:overlay:external>) but are NEVER merged into the signed gmeow: \
                 canon and NEVER written anywhere; the \
                 whole union is transient and discarded after the call. The forward closure \
                 runs under a mid-chase step governor: max_steps bounds the derivation budget \
                 and max_answers the answer cap. The response carries class_local_name \
                 (CoherenceCertificate for a conclusive coherent closure, \
                 CoherenceCheckAttestation for a budget-cut closure, Refused for a witnessed \
                 forbidden contradiction), the completeness/evaluation axes, the verify \
                 findings, the cited IRIs, and judgment_nquads (the grounded \
                 logic:ReasoningResult). An overlay exceeding the size ceiling, and a missing \
                 or unrecognized `format`, is a hard error — the format is never guessed.",
            &[
                ("data", "string"),
                ("format", "string"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "reason_graph",
            "Run the native GMEOW structured-DL forward chase over an RDF graph you paste \
                 inline and return the ENTAILED CLOSURE — what this document implies. `data` is \
                 the RDF text and `format` DECLARES how to read it (the same codec set \
                 query_local accepts). Scoped to YOUR graph alone, never unioned with the \
                 bundle: the question is what this document entails. Distinct from verify_graph, \
                 which returns a verdict about a graph rather than its entailments, and from \
                 entailments, which reads precomputed derivation rows for one BUNDLED term and \
                 cannot see your data. The chase runs under the mid-chase step governor \
                 (max_steps bounds the derivation budget, max_answers the answer cap), and the \
                 completeness/evaluation axes plus judgment_nquads ride the response so a \
                 budget-cut closure is legible as CUT rather than as small. Returns {ok, \
                 entailed_count, closure_nquads, completeness, evaluation, judgment_nquads}. An \
                 unrecognized format, or an input above the size/quad ceiling, is a hard error.",
            &[
                ("data", "string"),
                ("format", "string"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "explain_quad",
            "Reconstruct the FAITHFUL cited-IRI derivation skeleton for ONE arbitrary quad \
                 the bundle's reasoned closure entails: the target `(subject, predicate, \
                 object_value)` in named graph `graph`. Reasons over the bundle under the \
                 mid-chase step governor (max_steps bounds the derivation budget), locates the \
                 target by its content-addressed reifier WORLD-DISAMBIGUATED by `graph`, then \
                 reconstructs and re-verifies ONLY that quad's proof tree. Returns the DFS \
                 step_skeleton (target first, each step carrying its rule, derivation id, \
                 (S,P,O), world, asserted flag, depth, antecedent step ids, and cited term \
                 IRIs), the complete cited_iris set, a Markdown rendering, the world_iri, the \
                 completeness class (CoherenceCertificate for a conclusive closure, \
                 CoherenceCheckAttestation for a budget-cut one), and judgment_nquads (the \
                 grounded logic:ReasoningResult). `object_kind` is `iri` or `literal` (inferred \
                 from object_value when omitted); `object_datatype` types a literal object. This \
                 is DISTINCT from explain_finding: explain_finding addresses a PUBLISHED \
                 diagnostic by its fingerprint/anchor IRI over the graph/diagnostics projection, \
                 while explain_quad proves an arbitrary entailed quad from the reasoner's premise \
                 provenance. A quad the closure does not entail, or a reifier shared across \
                 worlds that `graph` does not resolve, is a hard error (never an empty-but-ok \
                 proof).",
            &[
                ("subject", "string"),
                ("predicate", "string"),
                ("object_value", "string"),
                ("object_kind", "string"),
                ("object_datatype", "string"),
                ("graph", "string"),
                ("max_steps", "integer"),
            ],
        ),
        tool(
            "coherence_certificate",
            "Read the bundle's SCOPED COHERENCE CERTIFICATE — a budget-free, \
                 proof-carrying coherence attestation computed ONCE at pipeline time over the \
                 whole assembled bundle and folded into graph/attestations. This tool reads it \
                 straight off the bundled dataset: it takes NO inputs, runs NO reasoning, and \
                 never recomputes. The response carries class_local_name (CoherenceCertificate \
                 for a conclusive, fragment-scoped, violation-free closure; the strictly-weaker \
                 CoherenceCheckAttestation otherwise — an attestation is NEVER reported as a \
                 certificate), issues_certificate, is_refused, the pinned bundle_hash and \
                 per-graph axiom_hashes (one content digest per axiom-bearing named graph, comparable only between identically labelled datasets), contract_hash, engine, \
                 certified_fragment, the completeness/evaluation axes, contradiction_policy, \
                 projection_losses, and forbidden_violations. A bundle carrying no coherence \
                 artifact is a hard error (never a silent recompute).",
            &[],
        ),
        tool(
            "validate_local",
            "Validate an inline RDF graph against the bundled GMEOW shapes and disciplines \
                 (the same core as `gmeow validate`), then CLOSE THE LOOP: every finding is \
                 returned with its rule catalog helpUri, the CORRESPONDING counter-example fixture \
                 (matched by violation code), a positive well-formed exemplar, and the entailments \
                 of the term it concerns. `data` is the RDF text; `format` is one of \
                 turtle|ttl|text/turtle, ntriples|nt|n-triples, nquads|nq|n-quads, trig, \
                 rdfxml|rdf+xml|xml, jsonld|json-ld. Set `deep` true to ALSO run the Tier-2 \
                 semantic pass (reasons over your data unioned with the whole bundle's axioms, \
                 like `gmeow validate --deep` — powerful but multiple minutes; default false). \
                 Nothing is written (the data is validated in-memory and discarded); an unknown \
                 format or an oversized payload is a hard error.",
            &[
                ("data", "string"),
                ("format", "string"),
                ("deep", "boolean"),
            ],
        ),
        tool(
            "gmn_validate",
            "Validate a GMN-1 document (the token-compact GMEOW Model Notation surface) \
                 against the shipped codebook dictionary + validator tier, checkout-free off the \
                 bundle. This is an external LLM's entry to the GMN `@err` repair loop: `gmn` is \
                 the GMN-1 document text. Returns {ok, conformant} — conformant:true for a \
                 well-formed document, or conformant:false with the TYPED lang:Gmn*Failure class \
                 (failure_class / failure_local_name) and message of the first defect, so the \
                 caller repairs against the named class. A defect is a valid result, never a tool \
                 error; an oversized payload is a hard error.",
            &[("gmn", "string")],
        ),
        tool(
            "gmn_expand",
            "Expand a GMN-1 document to its GMN-0 normal form (the alias/glyph → full-IRI \
                 direction) and return the canonical N-Quads, checkout-free off the bundle. `gmn` \
                 is the GMN-1 document text. The expansion carries an internal round-trip witness \
                 (the decoded model is re-encoded and re-read, and canonical equality asserted), \
                 so it never returns a lossy expansion. Returns {ok, expanded_nquads, \
                 reencoded_gmn, round_trip}. A non-conformant input, or a round-trip that does not \
                 hold, is a hard error.",
            &[("gmn", "string")],
        ),
        tool(
            "gmn_explain",
            "Explain a GMN operator glyph: resolve `glyph` to its lang:Denotation and its \
                 graph-authored gmeow:gmnFixity / gmeow:gmnPrecedence / gmeow:gmnArity signature, \
                 plus its controlled-NL gloss (the deterministic GMN⇄NL verbalizer rendering), \
                 checkout-free off the bundle. Returns {ok, found, glyph, denotation, \
                 denotation_target, label, fixity, fixity_local_name, precedence, arity, \
                 gmn_surface, gloss}. An input that is not a covered operator glyph returns an \
                 honest typed miss (found:false + lang:GmnUncoveredTerm), never a fabricated \
                 answer.",
            &[("glyph", "string")],
        ),
        tool(
            "advise",
            "Advise on an inline agent-authored RDF claim: return the non-gating \
                 RECOMMENDATIONS (never rejections) GMEOW harvests for it — the avoid-when \
                 prohibition, the how-to-use corrective directive, and the use-when permission \
                 prose of every advisory `advice.*` finding the claim trips. The companion of \
                 `validate_local`: where validate reports OBLIGATIONS (a claim can fail), advise \
                 reports RECOMMENDATIONS and ALWAYS returns `ok:true` — a clean claim returns an \
                 empty list. Routes through the SAME shipped validator core as `validate_local` \
                 (shallow Tier-1 pass only; advice is a fast structural concept), so it never \
                 diverges. `data` is the RDF text; `format` is one of turtle|ttl|text/turtle, \
                 ntriples|nt|n-triples, nquads|nq|n-quads, trig, rdfxml|rdf+xml|xml, \
                 jsonld|json-ld. Nothing is written; an unknown format or an oversized payload \
                 is a hard error.",
            &[("data", "string"), ("format", "string")],
        ),
        tool(
            "explain_finding",
            "Explain a diagnostic witness over the bundled graph/diagnostics projection, \
                 addressed by its fingerprint IRI (a finding) or its anchor IRI (a cluster): \
                 returns the provenance DAG, the aggregate ledger gate verdict, the minimal fatal \
                 cut (fingerprint IRIs + codes), and the anchor cluster. An unknown target is a \
                 hard error (never an empty-but-ok DAG).",
            &[("target_iri", "string")],
        ),
        tool(
            "store_claim",
            "Append one attributed memory claim, executed as a Transaction-Logic \
                 transaction (the executional-entailment verdict gates the commit). Pass \
                 dry_run=true for a non-committing sandbox run (verdict only, nothing written).",
            &[
                ("text", "string"),
                ("source", "string"),
                ("confidence", "number"),
                ("according_to", "string"),
                ("dry_run", "boolean"),
            ],
        ),
        tool(
            "conjecture_test",
            "Test — a PURE hypothetical evaluation of a candidate logic: formula against a \
                 KB: compute and return the engine verdict, but NEVER TR-commit and NEVER \
                 append to the conjecture library. formula is a Turtle logic: document naming \
                 one candidate; kb is a Turtle KB; standpoint is the required reified scope \
                 (P9); math_conjecture optionally names the math:Conjecture twin. max_steps / \
                 max_answers optionally bound the isolated scenario evaluation (a \
                 derived-closure-size ceiling: exceeding it stamps BudgetExhausted → lifecycle \
                 open). For the committing counterpart, see store_conjecture.",
            &[
                ("formula", "string"),
                ("kb", "string"),
                ("standpoint", "string"),
                ("math_conjecture", "string"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "store_conjecture",
            "Store — evaluate a candidate logic: formula against a KB and, TR-gated on the \
                 persistConjecture schema (the executional-entailment verdict gates the \
                 commit), APPEND the engine verdict to the append-only conjecture library. \
                 formula is a Turtle logic: document naming one candidate; kb is a Turtle KB; \
                 standpoint is the required reified scope (P9); math_conjecture optionally \
                 names the math:Conjecture twin. max_steps / max_answers optionally bound the \
                 isolated scenario evaluation (a derived-closure-size ceiling: exceeding it \
                 stamps BudgetExhausted → lifecycle open). Pass dry_run=true for a \
                 non-committing sandbox run (verdict only, nothing written) — a \
                 hypothetical-commit witness.",
            &[
                ("formula", "string"),
                ("kb", "string"),
                ("standpoint", "string"),
                ("math_conjecture", "string"),
                ("dry_run", "boolean"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "refute_conjecture",
            "Author-withdraw a stored conjecture — the store_conjecture compensation (P10, \
                 as revise_belief compensates store_claim), executed as a Transaction-Logic \
                 transaction on the withdrawConjecture schema. It APPENDS a compensating \
                 author-withdrawn segment to the append-only conjecture library, flipping the \
                 node's effective logic:conjectureLifecycleState to logic:ConjectureWithdrawn \
                 (recorded, never deleted — prior segments stay intact). conjecture_id is the \
                 stored logic:Conjecture node IRI; reason is an optional author note. The TR \
                 precondition — the conjecture is still in the library and NOT already \
                 withdrawn — is decided from the live library state by segment order, so an \
                 unknown id or an already-withdrawn node is rejected before any write. Pass \
                 dry_run=true for a non-committing sandbox run (verdict only, nothing written).",
            &[
                ("conjecture_id", "string"),
                ("reason", "string"),
                ("dry_run", "boolean"),
            ],
        ),
        tool(
            "recall",
            "Recall stored memory claims.",
            &[
                ("query", "string"),
                ("min_confidence", "number"),
                ("limit", "integer"),
                ("include_suppressed", "boolean"),
            ],
        ),
        tool(
            "store_segment",
            "Serialize the grounded-memory store — every claim and every recorded tool \
                 call, in append order — as N-Quads in the shared session-store transport \
                 shape (gmeow:ClaimToken / gmeow:ToolCall, position-addressed under \
                 urn:gmeow:session:). Returns {ok, claim_count, tool_call_count, nquads}. \
                 A READ: it commits nothing and the segment is a projection of state that \
                 was already stored. Distinct from recall, which answers a QUERY with a \
                 ranked, truncated JSON view of matching claims and is therefore not a \
                 snapshot: this is what an exported session carries so the trajectory and \
                 the store it ran against travel together, and it is what re-seeds a store \
                 for a replay. An empty store returns an empty nquads and zero counts, \
                 which is an answer, not a failure.",
            &[],
        ),
        tool(
            "revise_belief",
            "Suppress a stored claim without deleting history (the store_claim \
                 compensation, P10), executed as a Transaction-Logic transaction whose \
                 precondition is that the target claim exists. Pass dry_run=true for a \
                 non-committing sandbox run (verdict only, nothing suppressed).",
            &[
                ("claim_id", "string"),
                ("reason", "string"),
                ("superseded_by", "string"),
                ("dry_run", "boolean"),
            ],
        ),
        tool(
            "counter_examples",
            "Return the conformance fixtures documenting a bundled term, split into \
                 well-formed exemplars and counter-examples, each with its full Turtle body and \
                 the authored expected outcome / violation code / conformance rationale (read from \
                 gmeow:graph/documentation). A term with no fixtures returns empty lists; an \
                 unknown term is a hard error.",
            &[("term", "string")],
        ),
        tool(
            "entailments",
            "Return the reasoner entailments documenting a bundled term — each derivation's \
                 rule, conclusion, and every premise (read from gmeow:graph/documentation). A term \
                 with no entailments returns an empty list; an unknown term is a hard error.",
            &[("term", "string")],
        ),
        tool(
            "competency_questions",
            "Return the runnable competency questions from gmeow:graph/documentation, each with \
                 its SPARQL query text, plural rationales (complete lexical_form/datatype/language/direction \
                 records), expected row count and exact-rows flag. With the OPTIONAL `term`, only that term's questions (an unknown term is a \
                 hard error); without `term`, the whole index.",
            &[("term", "string")],
        ),
        tool(
            "slice_quality",
            "Score a slice against the bundle-carried slice-quality rubric and return its \
                 per-axis grades and ranked uplift advice. `files` is a JSON OBJECT mapping each \
                 slice-relative path to that file\u{2019}s text \
                 ({\"manifest.ttl\": \"\u{2026}\", \"module.ttl\": \"\u{2026}\", \
                 \"examples/x.ttl\": \"\u{2026}\", \"i18n/fr.po\": \"\u{2026}\", \
                 \"docs.md\": \"\u{2026}\"}) \u{2014} the slice is scored from those bytes \
                 alone, so it needs no directory and no checkout (the rubric ships in \
                 gmeow.gts). A map with no `manifest.ttl` entry, or a malformed one, is a hard \
                 error naming what is missing.",
            &[("files", "object")],
        ),
        tool(
            "slice_brief",
            "Serve the pre-assembled authoring packet(s) for a slice straight from the bundle: \
                 the covered-term IRIs, their present fr/zh/external grounding cells, exemplars, \
                 and coverage margins, as structured JSON plus canonical turtle. `slice` is a \
                 slice short-name (e.g. `ai`) or a full slice IRI; the OPTIONAL `axis` (default \
                 `whole`) and `batch` narrow the result. A slice/axis/batch with no packet is a \
                 hard error. Resolve each covered-term IRI to its full definition/axioms via \
                 `lookup_term` / `doc_card`.",
            &[
                ("slice", "string"),
                ("axis", "string"),
                ("batch", "integer"),
            ],
        ),
        candidate_submission_descriptor(),
        tool(
            "withdraw_candidate",
            "The P10 compensating withdrawal of a persisted candidate: append a 'withdrawn' \
                 segment flipping the candidate's effective lifecycle (recorded, never deleted). \
                 An unknown or already-withdrawn id hard-fails before writing. `dry_run=true` \
                 witnesses the withdrawal but writes nothing.",
            &[
                ("candidate_id", "string"),
                ("reason", "string"),
                ("dry_run", "boolean"),
            ],
        ),
        tool(
            "list_candidates",
            "List every admitted candidate in the library with its effective disposition \
                 (in-library | withdrawn) and target provenance (for_slice / for_packet). \
                 Optional `slice` filters by target provenance and `disposition` by effective \
                 state. A missing library is an empty list.",
            &[("slice", "string"), ("disposition", "string")],
        ),
        tool(
            "convert",
            "Transcode an RDF-1.2 document from any source codec to any target codec and \
                 report the loss the conversion actually realized. `data` is the source \
                 document and `from`/`to` DECLARE the codecs — one of turtle|ttl, \
                 ntriples|nt, nquads|nq, trig, jsonld|json-ld, jsonld-star, yaml-ld-star, \
                 rdfxml|rdf-xml|xml, gts, owl-rdf12, owl-dl, owl-el, datalog|dl, n3, gufo, \
                 canonical-rdf12. Optional `base` supplies a base IRI. Optional `encoding` \
                 declares how to read `data` (`utf8`, the default, or `base64` for a binary \
                 source such as gts); the response's own `encoding` says how to read \
                 `output` and is `base64` whenever the target bytes are not valid UTF-8. \
                 Returns {ok, from, to, encoding, output, bytes, loss} where `loss` is the \
                 realized loss ledger — the rows this particular dataset actually lost on \
                 this edge, empty for a lossless pair. Quoted triples survive to every \
                 star-capable target; a target that cannot carry them says so in `loss`. An \
                 unknown codec, an undecodable source, or a failed serialization is a hard \
                 error. Conforming GTS-to-GTS conversion preserves the complete artifact \
                 byte for byte. Blob-free sources can be re-emitted under the mandated \
                 profile; nonconforming blob-bearing sources refuse rather than discard \
                 content. Text RDF outputs are projections, without complete proof carriage.",
            &[
                ("data", "string"),
                ("from", "string"),
                ("to", "string"),
                ("base", "string"),
                ("encoding", "string"),
            ],
        ),
        tool(
            "gmn_glyph_legend",
            "Return the GMN-1 glyph legend for the bundled codebook: every glyph the codec \
                 may emit, in canonical order, with its real cl100k_base LLM-token cost \
                 ({glyph, tokenCost}). The alphabet an agent needs before writing GMN-1 — \
                 the companion of gmn_validate / gmn_expand / gmn_explain, and the same \
                 legend the browser codec renders.",
            &[],
        ),
        tool(
            "distribution_matrix",
            "Return the shipped documentation-distribution catalog read out of the bundle's \
                 meta-level distribution-catalog graph: `distributions`, the per-format \
                 consumer-need matrix (slug, family, media_type, consumers, \
                 dropped_capabilities) — WHICH documentation surfaces exist, who each is \
                 for, and what capability each doc-render surface drops — and `concepts`, \
                 the formal-concept lattice over the surface x capability incidence \
                 (concept, extent, intent). An empty `concepts` list means the bundle \
                 declares no lattice, which is not an error. A bundle carrying no \
                 distribution catalog at all is a hard error.",
            &[],
        ),
        tool(
            "action_policy",
            // The two counts are the DERIVED constants, formatted in — an agent reads this
            // description at run time, so a hand-typed number here is a false statement
            // shipped to a caller the moment the surface grows.
            &format!(
                "Return the canonical action theory governing this engine's WHOLE tool surface, \
                 as N-Quads, in the transaction world the executor reasons in. It is TOTAL, \
                 not a sample: every tool advertised here has exactly one action schema and \
                 every schema names exactly one advertised tool, tied together by the \
                 logic:mcpToolName wire name (a schema's local name is an ontology name — \
                 ex:persistConjecture is the tool `store_conjecture` — so the correspondence \
                 is asserted, never guessed). The {WRITE_TOOL_COUNT} WRITE tools are typed \
                 logic:McpActionSchema and carry logic:precondition / logic:effect / \
                 logic:compensation (the rollback is supersession, never erasure); the \
                 {READ_TOOL_COUNT} READ tools are plain logic:ActionSchema carrying \
                 logic:capability + logic:precondition and NO effect and NO compensation, \
                 because a read changes no state. This is the exact projection the \
                 Transaction-Logic executor reads, so what you inspect is what the engine \
                 obeys. Also served as the gmeow://ontology/action-policy resource."
            ),
            &[],
        ),
    ]
}

/// Bind one reasoning-SEGMENT tool to a handler, given the deployment's `segments`.
///
/// Two expansions, and the cargo feature picks which one exists:
/// * with `reasoning` linked, the real method is named and — when the deployment serves
///   the segment — called; when it does not, the deferral signal is returned instead, so
///   a core deployment on a full build behaves exactly like the lean image;
/// * without it, the method does not exist to be named at all, and the only expansion is
///   the deferral signal.
///
/// A macro rather than a function because a function taking the real handler as an
/// argument would have to NAME `s.tool_verify_graph(a)` in a build where that method is
/// compiled out. This keeps ONE mechanism (the `segment_not_loaded` signal) and no
/// never-called stub bodies to drift from it.
macro_rules! reasoning_tool {
    ($segments:expr, $name:literal, $method:ident) => {{
        let name: &'static str = $name;
        debug_assert!(
            REASONING_SEGMENT_TOOLS.contains(&name) || CHASE_SEGMENT_TOOLS.contains(&name),
            "`{name}` is bound below core but is absent from BOTH non-core segment lists, so \
             `SegmentSet::serves` would route it as core"
        );
        #[cfg(feature = "reasoning")]
        // ONE routing predicate, shared with every host that asks the same question
        // ahead of time: `SegmentSet::serves` is the only place "does this tool run
        // here?" is decided, so a host's pre-flight answer and the engine's dispatch
        // cannot disagree.
        let entry: (&'static str, ToolHandler) = if $segments.serves(name) {
            (
                name,
                Box::new(|s: &McpServer, a: &Value| s.$method(a)) as ToolHandler,
            )
        } else {
            (
                name,
                Box::new(move |_: &McpServer, _: &Value| {
                    Err(segment_not_loaded(name, SegmentSet::segment_of(name)))
                }) as ToolHandler,
            )
        };
        #[cfg(not(feature = "reasoning"))]
        let entry: (&'static str, ToolHandler) = {
            let _ = &$segments;
            (
                name,
                Box::new(move |_: &McpServer, _: &Value| {
                    Err(segment_not_loaded(name, SegmentSet::segment_of(name)))
                }) as ToolHandler,
            )
        };
        entry
    }};
}

/// Bind one CORE-segment tool to a handler, given the deployment's `segments` — the exact
/// mirror of [`reasoning_tool!`], and it exists for the same reason.
///
/// The reasoning image is a DELTA, not a superset: it links the DL reasoner and the rubric
/// kernel and NOTHING of the core tool surface, so in that build `s.tool_convert(a)` is not
/// a method that exists to be named. Two expansions, and the cargo feature picks which:
/// * with `core` linked, the real method is named and — when the deployment serves the
///   segment — called; when it does not, the deferral signal routes the caller back to the
///   always-resident core image;
/// * without it, the method does not exist at all, and the only expansion is the signal.
///
/// The tool stays ADVERTISED in both, so `tools/list` is byte-identical across the tiers
/// and no caller can observe which half of the engine it is talking to except in latency.
macro_rules! core_tool {
    ($segments:expr, $name:literal, $method:ident) => {{
        let name: &'static str = $name;
        debug_assert!(
            !REASONING_SEGMENT_TOOLS.contains(&name),
            "`{name}` is bound as a core-segment tool but is present in \
             REASONING_SEGMENT_TOOLS, so `SegmentSet::serves` would route it as reasoning"
        );
        #[cfg(feature = "core")]
        // ONE routing predicate, shared with every host that asks the same question ahead
        // of time — see `reasoning_tool!`.
        let entry: (&'static str, ToolHandler) = if $segments.serves(name) {
            (
                name,
                Box::new(|s: &McpServer, a: &Value| s.$method(a)) as ToolHandler,
            )
        } else {
            (
                name,
                Box::new(move |_: &McpServer, _: &Value| {
                    Err(segment_not_loaded(name, CORE_SEGMENT))
                }) as ToolHandler,
            )
        };
        #[cfg(not(feature = "core"))]
        let entry: (&'static str, ToolHandler) = {
            let _ = &$segments;
            (
                name,
                Box::new(move |_: &McpServer, _: &Value| {
                    Err(segment_not_loaded(name, CORE_SEGMENT))
                }) as ToolHandler,
            )
        };
        entry
    }};
}

/// Bind one CORE-segment RESOURCE to a handler — the [`core_tool!`] shape, for the other
/// half of the surface.
///
/// All five builtin resources are core: each renders the bundle's documentation view
/// (`llms.txt`, the full inlined index, the GMN-1 primer, the OKF index) or the action
/// policy, and none of them reasons. Leaving them served in the reasoning image would keep
/// the whole term-card and `llms` renderer REACHABLE there — measured at ~3.7 MB of image —
/// which is exactly the superset the delta exists to avoid. They defer back to the
/// always-resident core image instead, with the same typed signal a deferred tool gets.
macro_rules! core_resource {
    ($segments:expr, $uri:expr, $handler:expr) => {{
        let uri: &'static str = $uri;
        #[cfg(feature = "core")]
        let entry: (&'static str, ResourceHandler) = if $segments.core {
            (uri, Box::new($handler) as ResourceHandler)
        } else {
            (
                uri,
                Box::new(move |_: &McpServer, _: &[String]| {
                    Err(segment_not_loaded(uri, CORE_SEGMENT))
                }) as ResourceHandler,
            )
        };
        #[cfg(not(feature = "core"))]
        let entry: (&'static str, ResourceHandler) = {
            let _ = &$segments;
            (
                uri,
                Box::new(move |_: &McpServer, _: &[String]| {
                    Err(segment_not_loaded(uri, CORE_SEGMENT))
                }) as ResourceHandler,
            )
        };
        entry
    }};
}

/// The CONSUMER `tools/call` handlers, in the SAME order as
/// [`builtin_tool_descriptors`]. Each entry restates the tool name so
/// [`zip_tools`] can prove the pairing rather than assume it.
///
/// All [`crate::TOOL_COUNT`] entries exist in EVERY deployment — the list does not shrink when a
/// segment is unloaded, because "advertised" and "dispatchable" are one fact here and a
/// lean core still advertises the whole surface. What changes is what the
/// [`REASONING_SEGMENT_TOOLS`] entries dispatch TO: their real implementation when the
/// segment is served, the [`segment_not_loaded`] routing signal when it is not.
fn builtin_tool_handlers(segments: SegmentSet) -> Vec<(&'static str, ToolHandler)> {
    vec![
        core_tool!(segments, "lookup_term", tool_lookup_term),
        core_tool!(segments, "llms_txt", tool_llms_txt),
        core_tool!(segments, "llms_full", tool_llms_full),
        core_tool!(segments, "doc_card", tool_doc_card),
        core_tool!(segments, "okf_index", tool_okf_index),
        core_tool!(segments, "query_docs", tool_query_docs),
        core_tool!(segments, "docs_search", tool_docs_search),
        core_tool!(segments, "query_local", tool_query_local),
        core_tool!(segments, "encode_gmn1", tool_encode_gmn1),
        reasoning_tool!(segments, "verify_graph", tool_verify_graph),
        reasoning_tool!(segments, "reason_graph", tool_reason_graph),
        reasoning_tool!(segments, "explain_quad", tool_explain_quad),
        reasoning_tool!(
            segments,
            "coherence_certificate",
            tool_coherence_certificate
        ),
        core_tool!(segments, "validate_local", tool_validate_local),
        core_tool!(segments, "gmn_validate", tool_gmn_validate),
        core_tool!(segments, "gmn_expand", tool_gmn_expand),
        core_tool!(segments, "gmn_explain", tool_gmn_explain),
        core_tool!(segments, "advise", tool_advise),
        core_tool!(segments, "explain_finding", tool_explain_finding),
        reasoning_tool!(segments, "store_claim", tool_store_claim),
        reasoning_tool!(segments, "conjecture_test", tool_conjecture_test),
        reasoning_tool!(segments, "store_conjecture", tool_store_conjecture),
        reasoning_tool!(segments, "refute_conjecture", tool_refute_conjecture),
        reasoning_tool!(segments, "recall", tool_recall),
        reasoning_tool!(segments, "store_segment", tool_store_segment),
        reasoning_tool!(segments, "revise_belief", tool_revise_belief),
        core_tool!(segments, "counter_examples", tool_counter_examples),
        core_tool!(segments, "entailments", tool_entailments),
        core_tool!(segments, "competency_questions", tool_competency_questions),
        reasoning_tool!(segments, "slice_quality", tool_slice_quality),
        core_tool!(segments, "slice_brief", tool_slice_brief),
        reasoning_tool!(segments, "submit_candidate", tool_submit_candidate),
        reasoning_tool!(segments, "withdraw_candidate", tool_withdraw_candidate),
        reasoning_tool!(segments, "list_candidates", tool_list_candidates),
        core_tool!(segments, "convert", tool_convert),
        core_tool!(segments, "gmn_glyph_legend", tool_gmn_glyph_legend),
        core_tool!(segments, "distribution_matrix", tool_distribution_matrix),
        core_tool!(segments, "action_policy", tool_action_policy),
    ]
}

/// The CONSUMER surface as an [`Extension`]: the builtin descriptors joined to the
/// builtin handlers, both for tools and for resources. Every [`McpServer`] starts
/// from exactly this, so a consumer server and a host-extended server share one
/// definition of "the consumer surface".
///
/// # Errors
///
/// [`InvalidRegistration`](crate::error::InvalidRegistration) if the descriptor and
/// handler lists have drifted out of bijection.
pub(super) fn builtin_extension(segments: SegmentSet) -> gmeow_errors::Result<Extension> {
    Ok(Extension::from_parts(
        zip_tools(builtin_tool_descriptors(), builtin_tool_handlers(segments))?,
        zip_resources(
            builtin_resource_descriptors(),
            builtin_resource_handlers(segments),
        )?,
    ))
}

impl McpServer {
    pub fn resources_result(&self) -> Value {
        json!({ "resources": self.surface.resource_descriptors() })
    }
}

/// The CONSUMER `resources/read` handlers, in the SAME order as
/// [`builtin_resource_descriptors`]. The media type is NOT restated here — it comes
/// from the descriptor, so advertised and served can never disagree.
fn builtin_resource_handlers(segments: SegmentSet) -> Vec<(&'static str, ResourceHandler)> {
    vec![
        core_resource!(
            segments,
            "gmeow://ontology/llms.txt",
            |s: &McpServer, requested: &[String]| { Ok(s.view.llms_txt_text(requested.to_vec())) }
        ),
        core_resource!(
            segments,
            "gmeow://ontology/llms-full.txt",
            |s: &McpServer, requested: &[String]| { s.view.llms_full_text(requested.to_vec()) }
        ),
        core_resource!(
            segments,
            "gmeow://ontology/gmn1-primer",
            |s: &McpServer, _requested: &[String]| {
                s.view.gmn1_primer().map(|p| p.resource_text())
            }
        ),
        core_resource!(
            segments,
            "gmeow://ontology/okf-index",
            |s: &McpServer, requested: &[String]| { Ok(s.view.okf_index_json(requested.to_vec())) }
        ),
        // The SAME `McpView::action_policy` the transaction executor reads and the
        // `action_policy` tool returns — one projection, three readers, no restatement.
        // Language-independent: the projection keeps only IRI→IRI structural quads, so
        // there is nothing here for a language selector to select.
        core_resource!(
            segments,
            ACTION_POLICY_URI,
            |s: &McpServer, _requested: &[String]| {
                Ok(s.view.action_policy()?.nquads().to_owned())
            }
        ),
    ]
}

impl McpServer {
    /// Submit through the registered candidate handler while retaining its native
    /// diagnostic. Host extensions use this operation for governed composition.
    ///
    /// # Errors
    /// Returns the same capability, argument, policy and execution failures as the
    /// MCP wire operation, before its terminal JSON error projection.
    pub fn submit_candidate(&self, args: &Value) -> gmeow_errors::Result<String> {
        self.surface.dispatch_tool(self, "submit_candidate", args)
    }

    pub fn call_tool_result(&self, name: &str, args: &Value) -> Value {
        if let Some(err) = args.get("__parse_error").and_then(Value::as_str) {
            return tool_text(json!({"ok": false, "error": err}).to_string(), true);
        }
        // TOTAL dispatch: the assembled surface is the ONLY router. A name it does
        // not carry raises `mcp.unknown-tool` naming that name — there is no
        // fallthrough arm and no mode guard, so "advertised" and "dispatchable" stay
        // the same fact for builtin and host-registered tools alike.
        let result = self.surface.dispatch_tool(self, name, args);
        match result {
            Ok(text) => tool_text(text, false),
            // A DEFERRAL is not a failure and must not read like one. It keeps
            // `isError: true` (the call produced no answer, so a client that only checks
            // that flag is still correct) but the payload carries the STRUCTURED routing
            // fields — the stable diagnostic code, the tool asked for, and the segment
            // that serves it — so a host can act on it mechanically instead of matching
            // prose. Unreachable on a deployment that serves every segment, which is why
            // the full engine's bytes are unchanged.
            Err(err) => match err.downcast_ref::<crate::error::SegmentNotLoaded>() {
                Some(deferred) => tool_text(
                    json!({
                        "ok": false,
                        "error": err.to_string(),
                        "code": crate::error::SegmentNotLoaded::CODE,
                        "tool": deferred.tool,
                        "segment": deferred.segment,
                        // The tools of the segment NAMED, not every tool this image defers:
                        // a host loads one module and needs to know what that module answers
                        // for. Two tiers sit below core, so the two lists differ.
                        "segment_tools": SegmentSet::tools_of(&deferred.segment),
                    })
                    .to_string(),
                    true,
                ),
                None => tool_text(
                    json!({"ok": false, "error": err.to_string()}).to_string(),
                    true,
                ),
            },
        }
    }

    pub fn read_resource_result(&self, uri: &str) -> Value {
        match self.read_resource_text(uri) {
            Ok((mime, text)) => json!({
                "contents": [{"uri": uri, "mimeType": mime, "text": text}],
            }),
            Err(err) => json!({
                "contents": [{"uri": uri, "mimeType": "application/json", "text": json!({"ok": false, "error": err.to_string()}).to_string()}],
                "isError": true,
            }),
        }
    }

    pub fn handle_message(&self, message: &str) -> String {
        let parsed: Value = match serde_json::from_str(message) {
            Ok(value) => value,
            Err(err) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {"code": -32700, "message": err.to_string()},
                })
                .to_string();
            }
        };
        let id = parsed.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = parsed.get("method").and_then(Value::as_str) else {
            return rpc_error(id, -32600, "missing method");
        };
        let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {"name": "gmeow", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"tools": {}, "resources": {}},
            }),
            "tools/list" => self.tools_result(),
            "resources/list" => self.resources_result(),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "tools/call requires params.name");
                };
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                self.call_tool_result(name, &args)
            }
            "resources/read" => {
                let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "resources/read requires params.uri");
                };
                self.read_resource_result(uri)
            }
            "shutdown" => json!({}),
            method if method.starts_with("notifications/") => return String::new(),
            _ => return rpc_error(id, -32601, &format!("unknown method: {method}")),
        };
        json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
    }

    /// Serve the stdio JSON-RPC 2.0 MCP loop: one request per line on stdin, one
    /// response per line on stdout, until EOF. Blocking; the native `gmeow mcp` /
    /// `gmeow-dev mcp` launchers call this directly.
    ///
    /// Native-only, and deliberately NOT routed through a transport seam. A browser has
    /// no stdin, no stdout, and no blocking read loop to give one: the wasm host drives
    /// the very same surface a frame at a time through [`Self::handle_message`], which
    /// is the whole protocol implementation and is compiled on every target. Abstracting
    /// "a blocking line loop over process stdio" would produce an interface with exactly
    /// one implementation and one caller.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_stdio(&self) -> gmeow_errors::Result<()> {
        use std::io::{BufRead as _, Write as _};

        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let response = self.handle_message(&line);
            if !response.is_empty() {
                writeln!(stdout, "{response}")?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    #[cfg(feature = "core")]
    fn tool_lookup_term(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let requested = self.requested_from_args(args)?;
        Ok(self.view.lookup_term_json(term, requested))
    }

    #[cfg(feature = "core")]
    fn tool_llms_txt(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.llms_txt_text(requested))
    }

    #[cfg(feature = "core")]
    fn tool_llms_full(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        self.view.llms_full_text(requested)
    }

    #[cfg(feature = "core")]
    fn tool_doc_card(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let detail = parse_card_detail(optional_str(args, "detail"))?;
        let format = CardFormat::parse(optional_str(args, "format"))?;
        let requested = self.requested_from_args(args)?;
        self.view.doc_card(term, detail, format, requested)
    }

    #[cfg(feature = "core")]
    fn tool_okf_index(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.okf_index_json(requested))
    }

    /// `counter_examples`: the conformance fixtures documenting a term, split into
    /// the well-formed exemplars and the counter-examples. An UNKNOWN term is a HARD
    /// FAIL (`Err` → error envelope); a KNOWN term that simply documents no fixtures
    /// is an honest empty-but-ok result (both lists empty).
    #[cfg(feature = "core")]
    fn tool_counter_examples(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let iri = self.resolve_term_or_err(term, args)?;
        let (wellformed, counter_examples) = self.view.term_fixtures(&iri)?;
        Ok(json!({
            "ok": true,
            "term": term,
            "wellformed": wellformed,
            "counter_examples": counter_examples,
        })
        .to_string())
    }

    /// `entailments`: the reasoner derivations documenting a term, each with its
    /// rule, conclusion, and every premise. Unknown term → hard error; a known term
    /// with no derivations → empty-but-ok.
    #[cfg(feature = "core")]
    fn tool_entailments(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let iri = self.resolve_term_or_err(term, args)?;
        let entailments = self.view.term_entailments(&iri)?;
        Ok(json!({
            "ok": true,
            "term": term,
            "entailments": entailments,
        })
        .to_string())
    }

    /// `competency_questions`: the runnable competency questions. With a `term`, only
    /// that term's questions (unknown term → hard error); without a `term`, the whole
    /// index of documented competency questions.
    #[cfg(feature = "core")]
    fn tool_competency_questions(&self, args: &Value) -> gmeow_errors::Result<String> {
        match optional_str(args, "term") {
            Some(term) => {
                let iri = self.resolve_term_or_err(term, args)?;
                let questions = self.view.competency_questions(Some(&iri))?;
                Ok(json!({"ok": true, "term": term, "questions": questions}).to_string())
            }
            None => {
                let questions = self.view.competency_questions(None)?;
                Ok(json!({"ok": true, "questions": questions}).to_string())
            }
        }
    }

    /// Resolve a term string to its canonical IRI, HARD-FAILING (`Err`) on an unknown
    /// term OR a cross-namespace bare-name collision — the shared resolution guard for
    /// the documentation-surface tools (`counter_examples`, `entailments`,
    /// `competency_questions`). Ambiguity carries the TYPED `McpAmbiguousTerm`
    /// diagnostic (sorted candidate CURIEs); an unknown term keeps the generic
    /// unknown-term diagnostic — never a silent pick.
    #[cfg(feature = "core")]
    fn resolve_term_or_err(&self, term: &str, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        match self.view.resolve_term_iri(term, requested) {
            ConsumerResolution::Resolved(iri) => Ok(iri),
            ConsumerResolution::Ambiguous { candidates } => {
                Err(ambiguous_term_err(term, &candidates))
            }
            ConsumerResolution::NotFound => Err(unknown_term_err(term)),
        }
    }

    #[cfg(feature = "core")]
    fn tool_query_docs(&self, args: &Value) -> gmeow_errors::Result<String> {
        let query = required_str(args, "query")?;
        Ok(self.view.query_docs_json(query))
    }

    /// `docs_search`: rank the documented terms / slices / concerns whose searchable
    /// facets match `query` over the `gmeow:graph/documentation` projection. An
    /// absent/empty documentation graph is a HARD FAIL; a query matching nothing is an
    /// honest empty-but-ok result.
    #[cfg(feature = "core")]
    fn tool_docs_search(&self, args: &Value) -> gmeow_errors::Result<String> {
        let query = required_str(args, "query")?;
        let limit = optional_limit(args, "limit")?.unwrap_or(20);
        let docs = self.view.documentation()?;
        let hits = search_documentation(docs, query, limit)?;
        let results: Vec<Value> = hits.iter().map(SearchHit::to_json).collect();
        Ok(json!({"ok": true, "query": query, "results": results}).to_string())
    }

    #[cfg(feature = "core")]
    fn tool_query_local(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let query = required_str(args, "query")?;
        let scope = QueryScope::parse(optional_str(args, "scope"))?;
        Ok(self.view.query_local_json(data, format, query, scope))
    }

    /// `encode_gmn1` — the WRITE leg of the GMN-1 codec: authored RDF in, the
    /// token-compact GMN-1 surface out.
    ///
    /// The other four GMN tools all CONSUME GMN (validate a document, expand it to GMN-0,
    /// explain a glyph, price the legend). Producing GMN-1 — the direction an agent needs to
    /// emit the compact notation at all — had no expression on the surface: `gmn1_write`
    /// existed only INSIDE `gmn_expand`, as an internal round-trip witness on a GMN-1 input.
    /// That made the codec's forward leg unreachable through the protocol even though the
    /// engine shipped it.
    ///
    /// Runs the SAME `Gmn0Model::from_dataset` → `gmn1_write` pair the codec's other
    /// consumers do, against the dictionary carried IN the bundle, so the emitted surface is
    /// the one the on-gate authority ships. Pairs with `gmn_expand` as an exact round trip:
    /// `encode_gmn1` then `gmn_expand` returns the input's canonical N-Quads.
    #[cfg(feature = "core")]
    fn tool_encode_gmn1(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let media = rdf_media_type("encode_gmn1", format)?;
        if data.len() > MAX_VERIFY_OVERLAY_BYTES as usize {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
                format!(
                    "encode_gmn1: data is {} bytes, exceeding the {MAX_VERIFY_OVERLAY_BYTES}-byte \
                     ceiling; split the document and encode the parts (no silent truncation)",
                    data.len()
                ),
            )));
        }
        let dataset = purrdf::parse_dataset(data.as_bytes(), media, None)
            .with_ctx(|| format!("parse encode_gmn1 input ({format})"))?;
        let dict = self.gmn_dictionary()?;
        let model = Gmn0Model::from_dataset(&dataset);
        let doc = gmn1_write(&model, dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "encode_gmn1: encoding the model to GMN-1 failed: {error}"
            )))
        })?;
        // Round-trip witness, the mirror of `gmn_expand`'s: read the emitted surface back and
        // assert canonical equality with the input model. An encoding that does not read back
        // is a HARD FAIL, never a returned answer — the same no-lossy-answer discipline.
        let back = gmn1_read(&doc, dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "encode_gmn1: the emitted GMN-1 does not read back (lang:{}): {error}",
                iri_local_name(error.failure_class())
            )))
        })?;
        let round_trip = gmn0_canonically_equal(&model, &back)?;
        if !round_trip {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
                "encode_gmn1: the emitted GMN-1 does not round-trip to the input model \
                          (a lossy encoding is a hard error, never an answer)"
                    .to_string(),
            )));
        }
        Ok(json!({
            "ok": true,
            "gmn1": doc.text,
            "canonical_nquads": back.canonical_nquads()?,
            "round_trip": round_trip,
        })
        .to_string())
    }

    /// `reason_graph` — R2's **reason** verb over caller-supplied RDF: run the native
    /// structured-DL forward chase and return the ENTAILED CLOSURE.
    ///
    /// Distinct from the two neighbours that sound like it, and neither could stand in:
    /// * `verify_graph` returns a proof-carrying VERDICT (completeness/evaluation axes,
    ///   findings, a coherence judgment) — it says whether the graph holds up, not what it
    ///   entails;
    /// * `entailments` reads PRECOMPUTED derivation rows out of the bundle's documentation
    ///   graph for one bundled term — it cannot see caller data at all.
    ///
    /// Scoped to the caller's graph ALONE (never `bundle ∪ data`): the question is what THIS
    /// document entails, and unioning the canon in would answer a different one. Budgeted
    /// through [`governed_budget`] like every other agent-facing reasoning call — R4 forbids
    /// exposing an unbudgeted forward chase to an agent loop — and the consumed/allowed steps
    /// ride the envelope so a truncated closure is legible as truncated rather than as small.
    #[cfg(feature = "reasoning")]
    fn tool_reason_graph(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let media = rdf_media_type("reason_graph", format)?;
        if data.len() > MAX_VERIFY_OVERLAY_BYTES as usize {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
                format!(
                    "reason_graph: data is {} bytes, exceeding the {MAX_VERIFY_OVERLAY_BYTES}-byte \
                     ceiling; split the graph and reason over the parts (no silent truncation)",
                    data.len()
                ),
            )));
        }
        let edb = purrdf::parse_dataset(data.as_bytes(), media, None)
            .with_ctx(|| format!("parse reason_graph input ({format})"))?;
        let edb_quads = edb.quad_count();
        if edb_quads > MAX_VERIFY_OVERLAY_QUADS {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
                format!(
                    "reason_graph: input is {} quads, exceeding the {MAX_VERIFY_OVERLAY_QUADS}-quad \
                     ceiling; the starting EDB of a governed chase is bounded",
                    edb_quads
                ),
            )));
        }
        let budget = governed_budget(
            optional_step_count(args, "max_steps")?,
            optional_limit(args, "max_answers")?,
        );
        let input = prepare_reasoning_input(&edb)?;
        // reason_graph explicitly reasons the submitted document alone: each
        // original context is a selected theory, including scoped blank worlds.
        let domains = SelectedDomains::new(
            input
                .source_contexts()
                .values()
                .map(|graph| {
                    SelectedLogicalWorld::new(
                        LogicalGraph::from_graph(graph.clone()),
                        DomainProfile::NonemptyObjectDomainV1,
                        "gmeow.mcp.reason-graph.v1".to_owned(),
                        *input.ingress_contract(),
                    )
                })
                .collect::<gmeow_errors::Result<Vec<_>>>()?,
        )?;
        let result = reason_all_budgeted(input, &domains, &budget)?;
        let artifact = gmeow_logic::result_rdf::ResultProjection::reasoning_with_closure(&result)?
            .into_gts()?
            .bytes;
        Ok(gmeow_logic::result_rdf::admit_reasoning_response(&artifact)?.to_string())
    }

    /// Reason-and-verify the bundle UNIONED with a READ-ONLY inline overlay graph,
    /// returning the proof-carrying judgment envelope. A thin wrapper over
    /// [`McpView::run_verify_graph`]: it reads the `data` + `format` overlay and the
    /// `max_steps` / `max_answers` budget off the args and delegates the whole
    /// overlay/union/govern/verify/judge discipline to the view core (one
    /// implementation).
    #[cfg(feature = "reasoning")]
    fn tool_verify_graph(&self, args: &Value) -> gmeow_errors::Result<String> {
        self.with_verify_graph_request(args, McpView::verify_graph_json)
    }

    /// Validate one verify request and stamp its finite governor before evaluation.
    /// Both response rendering and native-result contracts use this exact argument path.
    #[cfg(feature = "reasoning")]
    pub(super) fn with_verify_graph_request<T>(
        &self,
        args: &Value,
        execute: impl FnOnce(&McpView, &str, &str, &Budget) -> T,
    ) -> gmeow_errors::Result<T> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        let budget = governed_budget(max_steps, max_answers);
        Ok(execute(&self.view, data, format, &budget))
    }

    /// Reconstruct the FAITHFUL cited-IRI derivation skeleton for one quad in the
    /// bundle's governed closure, returning the proof-carrying envelope. A thin
    /// wrapper over [`McpView::run_explain_quad`]: it validates the required quad
    /// components, builds the canonical N3 object surface (Step B) from the structured
    /// `object_value` / `object_kind` / `object_datatype` args, reads the `max_steps`
    /// governor, and delegates the reason/locate/explain/faithfulness discipline to
    /// the view core (one implementation).
    ///
    /// This is the CONSUMER quad-explainer — distinct from `explain_finding`, which
    /// addresses a published diagnostic by its fingerprint/anchor IRI.
    #[cfg(feature = "reasoning")]
    fn tool_explain_quad(&self, args: &Value) -> gmeow_errors::Result<String> {
        self.with_explain_quad_request(args, McpView::explain_quad_json)
    }

    /// Validate the quad surface and stamp its finite governor before evaluation.
    /// Keeping this request path shared preserves object typing and omission semantics.
    #[cfg(feature = "reasoning")]
    pub(super) fn with_explain_quad_request<T>(
        &self,
        args: &Value,
        execute: impl FnOnce(&McpView, &str, &str, &str, &str, &Budget) -> T,
    ) -> gmeow_errors::Result<T> {
        let subject = required_str(args, "subject")?;
        let predicate = required_str(args, "predicate")?;
        let object_value = required_str(args, "object_value")?;
        let graph = required_str(args, "graph")?;
        // `object_kind` is optional: an explicit `iri`/`literal` overrides, else it is
        // inferred from the object surface. `object_datatype` types a literal only.
        let object_kind = optional_str(args, "object_kind");
        let object_datatype = optional_str(args, "object_datatype");
        let obj_n3 = object_term_n3(object_value, object_kind, object_datatype)?;

        let max_steps = optional_step_count(args, "max_steps")?;
        // R4: `explain_quad` never exposes `max_answers`, but
        // `governed_budget` still stamps a finite default there — no field of the
        // constructed `Budget` is ever `None` on an agent-facing path.
        let budget = governed_budget(max_steps, None);
        Ok(execute(
            &self.view, subject, predicate, &obj_n3, graph, &budget,
        ))
    }

    /// Surface the bundle's carried coherence certificate/attestation (R6). A thin,
    /// INPUT-FREE wrapper over [`McpView::coherence_certificate_json`]: the certificate is
    /// read straight off the bundled dataset (disk-free, reason-free), never recomputed.
    #[cfg(feature = "reasoning")]
    fn tool_coherence_certificate(&self, _args: &Value) -> gmeow_errors::Result<String> {
        Ok(self.view.coherence_certificate_json())
    }

    /// Validate an inline RDF `data` graph (in `format`) against the bundle's own
    /// shapes + disciplines through the DEEP (`--deep`) semantic pass — the SAME
    /// `gmeow_validate::data_validate::run` core the CLI `gmeow validate --deep`
    /// drives — then CLOSE THE LOOP by enriching each finding with the bundle's
    /// teaching surface: the rule's catalog help URI, the CORRESPONDING
    /// counter-example (matched by violation code), a positive exemplar, and the
    /// term's entailments.
    ///
    /// Transient discipline: nothing is written. The data arrives inline as a string
    /// arg (unlike `query_local`, which reads a file), is validated against the
    /// retained snapshot bytes, and is discarded. An unrecognized `format` or an
    /// oversized payload is a HARD FAIL, never a silent mis-parse or truncation.
    #[cfg(feature = "core")]
    fn tool_validate_local(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let canonical = canonical_rdf_format("validate_local", format)?;

        if data.len() > MAX_VALIDATE_DATA_BYTES {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
                format!(
                    "validate_local: data payload is {} bytes, exceeding the {} byte ceiling; \
                     split the graph and validate the parts (no silent truncation)",
                    data.len(),
                    MAX_VALIDATE_DATA_BYTES
                ),
            )));
        }

        // Tier-1 SHACL + disciplines always run (fast). The Tier-2 semantic
        // pass — reasoning over the user data unioned with the whole bundle's
        // axioms — is opt-in via `deep` (default false), exactly like
        // `gmeow validate --deep`: it is powerful but reasons over the entire
        // bundle (multiple minutes), so an interactive agent requests it only when
        // structural conformance is not enough. When enabled, a contract-invalid
        // input is emitted by the engine as a hard Error finding — surfaced here
        // verbatim, never post-processed away.
        //
        // This server is a RESIDENT bundle consumer: it already imported the
        // snapshot to the carrier dataset at startup and caches the parsed Tier-1
        // shape union on first use, so it drives the shared `run_with` composition
        // (the exact `gmeow validate` semantics) without re-decoding the whole
        // bundle on every call.
        let deep = optional_bool(args, "deep").unwrap_or(false);
        let report = gmeow_validate::data_validate::run_with(
            gmeow_validate::data_validate::BundleParts {
                #[cfg(not(target_arch = "wasm32"))]
                native_gates: deep.then(|| self.view.prepared_reasoned_gates()),
                shapes: self.view.tier1_shapes()?,
                dataset: self.view.dataset.as_ref(),
            },
            data.as_bytes(),
            canonical,
            MCP_NAMESPACE,
            VALIDATE_LOCAL_ORIGIN,
            deep,
        )?;

        // Loop closure: extract the correspondence maps from the documentation graph
        // and enrich each finding through the wasm-clean pure join.
        let (counter_examples_by_code, wellformed_by_code) = self.view.fixture_maps()?;
        let entailments_by_term = self.view.entailment_map()?;
        let enriched = local_oracle::enrich_report(
            &report,
            counter_examples_by_code,
            wellformed_by_code,
            entailments_by_term,
        );
        serde_json::to_string(&enriched).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "validate_local: serialize enriched report: {e}"
            )))
        })
    }

    /// Resolve the shipped GMN-1 dictionary (`gmeow:gmnDictV3` + its current codebook)
    /// straight off the bundled snapshot dataset — the SAME checkout-free source the
    /// `gmeow gmn` CLI folds from `BUNDLE_GTS`, so the MCP verifier tools an external LLM
    /// calls share ONE dictionary with the shipped CLI and gates. A load failure is a HARD
    /// FAIL, never a degraded default.
    #[cfg(feature = "core")]
    pub(super) fn gmn_dictionary(&self) -> gmeow_errors::Result<&GmnDictionary> {
        self.view.gmn_dictionary()
    }

    /// `gmn_validate` — the external LLM's entry to the GMN `@err` repair loop: read a
    /// GMN-1 document against the shipped dictionary + validator tier and report either
    /// conformance or the TYPED `lang:Gmn*Failure` class (+ message) of the first defect.
    /// A defect is a VALID result (`ok:true, conformant:false`), never a tool error — the
    /// caller repairs against the named class.
    #[cfg(feature = "core")]
    fn tool_gmn_validate(&self, args: &Value) -> gmeow_errors::Result<String> {
        let gmn = required_str(args, "gmn")?;
        guard_gmn_size(gmn, "gmn_validate")?;
        let dict = self.gmn_dictionary()?;
        let doc = Gmn1Document::from_text(gmn.to_owned());
        match gmn1_read(&doc, dict) {
            Ok(_model) => Ok(json!({ "ok": true, "conformant": true }).to_string()),
            Err(error) => {
                let class = error.failure_class();
                Ok(json!({
                    "ok": true,
                    "conformant": false,
                    "failure_class": class,
                    "failure_local_name": iri_local_name(class),
                    "message": error.to_string(),
                })
                .to_string())
            }
        }
    }

    /// `gmn_expand` — decode a GMN-1 document to its GMN-0 normal form (the "expand
    /// alias/glyph → full IRI" direction) and return the canonical N-Quads. The expansion
    /// carries an internal round-trip WITNESS: the decoded model is re-encoded and re-read,
    /// and canonical equality is asserted, so the tool never returns an unstable expansion.
    /// A non-conformant input, or a round-trip that does not hold, is a HARD FAIL.
    #[cfg(feature = "core")]
    fn tool_gmn_expand(&self, args: &Value) -> gmeow_errors::Result<String> {
        let gmn = required_str(args, "gmn")?;
        guard_gmn_size(gmn, "gmn_expand")?;
        let dict = self.gmn_dictionary()?;
        let doc = Gmn1Document::from_text(gmn.to_owned());
        let model = gmn1_read(&doc, dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "gmn_expand: input is not a conformant GMN-1 document (lang:{}): {error}",
                iri_local_name(error.failure_class())
            )))
        })?;
        // Round-trip witness: re-encode the expanded model and read it back; the expansion
        // is only sound if the reconstruction is canonically equal (no lossy expansion).
        let reencoded = gmn1_write(&model, dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "gmn_expand: re-encoding the expanded model failed: {error}"
            )))
        })?;
        let back = gmn1_read(&reencoded, dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "gmn_expand: re-reading the re-encoded model failed: {error}"
            )))
        })?;
        if !gmn0_canonically_equal(&model, &back)? {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
                "gmn_expand: round-trip witness failed — the expanded GMN-0 normal form \
                          is not canonically stable under re-encode/re-read (refusing a lossy \
                          expansion)"
                    .to_owned(),
            )));
        }
        Ok(json!({
            "ok": true,
            "expanded_nquads": model.canonical_nquads()?,
            "reencoded_gmn": reencoded.text,
            "round_trip": true,
        })
        .to_string())
    }

    /// `gmn_explain` — resolve a GMN operator glyph to its `lang:Denotation` and its
    /// graph-authored `gmeow:gmnFixity` / `gmeow:gmnPrecedence` / `gmeow:gmnArity`
    /// signature, plus its controlled-NL gloss (the same rendering produced by the
    /// verbalizer).
    /// An input that is not a covered operator glyph returns an HONEST typed miss
    /// (`found:false` + `lang:GmnUncoveredTerm`), never a fabricated answer.
    #[cfg(feature = "core")]
    fn tool_gmn_explain(&self, args: &Value) -> gmeow_errors::Result<String> {
        let glyph = required_str(args, "glyph")?;
        let dataset = self.view.dataset.as_ref();
        let dict = self.gmn_dictionary()?;
        let labels = harvest_dataset_labels(dataset);
        let forms = resolve_operator_forms(dict.glyph_registry(), &labels).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "gmn_explain: resolve GMN operator forms from the codebook: {error}"
            )))
        })?;
        let Some(form) = forms.iter().find(|f| f.gmn_glyph == glyph) else {
            return Ok(json!({
                "ok": true,
                "found": false,
                "glyph": glyph,
                "failure_class": LANG_GMN_UNCOVERED_TERM,
                "failure_local_name": iri_local_name(LANG_GMN_UNCOVERED_TERM),
                "message": format!(
                    "glyph {glyph:?} is not a covered GMN operator in the current codebook"
                ),
            })
            .to_string());
        };
        // The controlled-NL gloss is the verbalizer rendering VERBATIM: build the whole
        // (injective, disambiguated) corpus and read off this form's rendered pair, so the
        // gloss the LLM sees is the exact GMN⇄NL training-pair surface, not a re-derivation.
        let pairs = build_verbalization_pairs(&forms).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "gmn_explain: render the verbalizer gloss corpus: {error}"
            )))
        })?;
        let rendered = pairs.iter().find(|p| &p.form == form).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "gmn_explain: no verbalizer pair for glyph {glyph:?}"
            )))
        })?;
        // Precedence + the lang:Denotation IRI are NOT carried by the glyph registry (it keys
        // on fixity/arity only), so join them from the dataset by the (target, fixity, arity)
        // signature the operator's denoted Form authors.
        let (precedence, denotation) =
            gmn_signature_join(dataset, &form.term_iri, &form.fixity, form.arity);
        Ok(json!({
            "ok": true,
            "found": true,
            "glyph": form.gmn_glyph,
            "denotation": denotation,
            "denotation_target": form.term_iri,
            "label": form.term_label,
            "fixity": form.fixity,
            "fixity_local_name": iri_local_name(&form.fixity),
            "precedence": precedence,
            "arity": form.arity,
            "gmn_surface": rendered.gmn_surface,
            "gloss": rendered.nl,
        })
        .to_string())
    }

    /// `gmn_glyph_legend` — the GMN-1 glyph inventory joined to each glyph's real
    /// LLM-token cost, as a deterministic JSON array of `{glyph, tokenCost}`.
    ///
    /// The completion of the GMN triad on this surface: `gmn_validate` says whether a
    /// document conforms, `gmn_expand` gives its GMN-0 normal form, `gmn_explain` explains
    /// one operator — and this states the ALPHABET, which an agent needs before it can
    /// write GMN-1 at all.
    ///
    /// The legend is composed by the ONE implementation
    /// ([`gmeow_lang_bridge::glyph_legend_value`]) the browser codec shim's String exit also marshals, over
    /// the glyph registry of THIS bundle's shipped `gmeow:gmnDictV3` codebook — so the
    /// agent's legend and the docs widget's legend are the same rows in the same order, and
    /// there is no second table to drift.
    #[cfg(feature = "core")]
    fn tool_gmn_glyph_legend(&self, _args: &Value) -> gmeow_errors::Result<String> {
        let dict = self.gmn_dictionary()?;
        glyph_legend::response(dict.glyph_registry())
    }

    /// `convert` — the fourth verb of parse / reason / validate / SERIALIZE: transcode an
    /// RDF-1.2 document from any source codec to any target codec, and report the loss the
    /// conversion actually realized.
    ///
    /// This routes through [`gmeow_transcode`] — the SAME `Codec::from_cli_str` /
    /// `transcode` / `realized_loss_json` triple `gmeow convert` calls, not a second
    /// implementation — so the bytes an agent gets here and the bytes the CLI writes are
    /// the same function of the same input. `gmeow-transcode` is a leaf, which is what lets
    /// a bundle-only surface reach it without inheriting the build executor.
    ///
    /// RDF-1.2 is load-bearing: a quoted triple that survives to a star-capable target MUST
    /// still be there. The hub decides that, not this tool — and the `loss` ledger states
    /// what an RDF-1.1-shaped target dropped, rather than letting the caller assume nothing
    /// was lost.
    ///
    /// Bytes, not text: `encoding` declares how to read `data` (`utf8`, the default, or
    /// `base64` for a binary source such as `gts`), and the response's own `encoding` says
    /// how to read `output` — `base64` whenever the target's bytes are not valid UTF-8.
    /// Neither direction ever silently lossy-decodes.
    ///
    /// Gated on the `core` feature: `gmeow-transcode` is one of the two dependencies the
    /// core segment alone reaches, so the demand-loaded reasoning image links no transcode
    /// hub and this method does not exist there to be named. `core_tool!` binds the wire
    /// name to the deferral signal instead, and the tool stays advertised.
    #[cfg(feature = "core")]
    fn tool_convert(&self, args: &Value) -> gmeow_errors::Result<String> {
        use gmeow_transcode::{Codec, realized_loss_json, transcode as run_transcode};

        let data = required_str(args, "data")?;
        let from = required_str(args, "from")?;
        let to = required_str(args, "to")?;
        let base = optional_str(args, "base");
        let input = match optional_str(args, "encoding").unwrap_or("utf8") {
            "utf8" => data.as_bytes().to_vec(),
            "base64" => base64_decode("convert: `data`", data)?,
            other => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
                    format!("convert: unknown `encoding` {other:?} — expected `utf8` or `base64`"),
                )));
            }
        };

        let from_codec = Codec::from_cli_str(from)?;
        let to_codec = Codec::from_cli_str(to)?;
        let output = run_transcode(&input, from_codec, to_codec, base)?;

        // The realized ledger is rendered by the hub's own `realized_loss_json` (which
        // interns through the single substrate loss store) and re-read here only to nest it
        // as structured JSON in the envelope — never re-derived off the `Vec<RealizedLoss>`.
        let loss: Value =
            serde_json::from_str(&realized_loss_json(&output.realized)).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                    "convert: the realized-loss ledger is not valid JSON: {e}"
                )))
            })?;

        let ingestion_receipt = output
            .gts_ingestion
            .as_ref()
            .map(|report| {
                gmeow_gts_profile::ingestion_receipt(&output.bytes, report)
                    .map(|bytes| json!({"encoding": "base64", "cbor": base64_encode(&bytes)}))
            })
            .transpose()?;
        let byte_len = output.bytes.len();
        let (encoding, text) = match String::from_utf8(output.bytes) {
            Ok(utf8) => ("utf8", utf8),
            Err(err) => ("base64", base64_encode(err.as_bytes())),
        };
        Ok(json!({
            "ok": true,
            "from": from_codec.name(),
            "to": to_codec.name(),
            "encoding": encoding,
            "output": text,
            "bytes": byte_len,
            "loss": loss,
            "gts_ingestion_receipt": ingestion_receipt,
        })
        .to_string())
    }

    /// `distribution_matrix` — the shipped documentation-distribution catalog, read back
    /// out of THIS bundle's meta-level `graph/distribution-catalog` named graph.
    ///
    /// Two row sets, because they are two shapes over one graph:
    ///
    /// * `distributions` — the per-format consumer-need matrix (`slug`, `family`,
    ///   `media_type`, `consumers`, `dropped_capabilities`): WHICH documentation surfaces
    ///   exist, who each is for, and what capability each doc-render surface drops. This is
    ///   the same reader, over the same graph, that `gmeow docs matrix` prints.
    /// * `concepts` — the formal-concept lattice over the surface × capability incidence
    ///   (`concept`, `extent`, `intent`). A concept is not a distribution — it has an
    ///   extent and an intent and no media type — so it has its own reader and its own row
    ///   type. An empty list is the honest reading of a catalog that declares no concepts,
    ///   not a failure.
    ///
    /// Both come from [`gmeow_docs_catalog`], the wasm-clean catalog leaf, so this stays a
    /// bundle-only tool: no checkout, no build executor.
    ///
    /// Gated on the `core` feature, exactly as `tool_convert` is and for the same reason:
    /// `gmeow-docs-catalog` is the other dependency only this segment reaches.
    #[cfg(feature = "core")]
    fn tool_distribution_matrix(&self, _args: &Value) -> gmeow_errors::Result<String> {
        let gts = self.view.gts_bytes();
        let distributions = gmeow_docs_catalog::read_distribution_matrix(gts)?;
        let concepts = gmeow_docs_catalog::read_concept_lattice(gts)?;
        Ok(json!({
            "ok": true,
            "distributions": distributions
                .iter()
                .map(|row| json!({
                    "slug": row.slug,
                    "family": row.family,
                    "media_type": row.media_type,
                    "consumers": row.consumers,
                    "dropped_capabilities": row.dropped_capabilities,
                }))
                .collect::<Vec<Value>>(),
            "concepts": concepts
                .iter()
                .map(|row| json!({
                    "concept": row.concept,
                    "extent": row.extent,
                    "intent": row.intent,
                }))
                .collect::<Vec<Value>>(),
        })
        .to_string())
    }

    /// `action_policy` — the canonical action theory covering this engine's ENTIRE consumer
    /// tool surface, served as N-Quads.
    ///
    /// [`McpView::action_policy`] is the projection the Transaction-Logic executor reads: the
    /// `logic:precondition` / `logic:effect` / `logic:compensation` structure of every write
    /// action and the `logic:capability` / `logic:precondition` structure of every read, plus
    /// the `logic:mcpToolName` wire name that ties each schema to the tool it governs, in
    /// [`TXN_WORLD`]. The theory is TOTAL over the surface — 6 governed writes
    /// (`logic:McpActionSchema`) and 32 reads (plain `logic:ActionSchema`), bijective with the
    /// 38 advertised tools, enforced by
    /// `the_action_theory_is_bijective_with_the_consumer_tool_surface`. The tool returns THAT
    /// function's output verbatim — never a re-derivation off the embedded Turtle and never a
    /// second filter — so what an agent inspects is exactly what the engine obeys.
    ///
    /// No existing surface can serve it: `tools/list` returns names and JSON Schemas only,
    /// and `query_docs` is scoped to `gmeow:graph/documentation` while the policy is
    /// authored in the agentic slice's examples graph. The mirroring
    /// `gmeow://ontology/action-policy` resource serves the identical bytes as `text` for a
    /// client that reads resources rather than calling tools.
    #[cfg(feature = "core")]
    fn tool_action_policy(&self, _args: &Value) -> gmeow_errors::Result<String> {
        Ok(json!({
            "ok": true,
            "graph": TXN_WORLD,
            "media_type": ACTION_POLICY_MEDIA_TYPE,
            "nquads": self.view.action_policy()?.nquads(),
        })
        .to_string())
    }

    /// The `advise` tool: return the non-gating RECOMMENDATIONS (never rejections) a
    /// submitted claim trips — the companion of `validate_local`. It runs the SAME
    /// shipped validator core (shallow Tier-1 only — the advisory `advice.*` Note tier
    /// is a fast structural pass; deep reasoning buys no advisory yield for its
    /// multi-minute cost), keeps only the advisory tier, and serializes each as a
    /// contrary-to-duty-shaped recommendation. ALWAYS `ok:true`: advice is a
    /// recommendation, so a clean claim returns an empty list, never a failure.
    #[cfg(feature = "core")]
    fn tool_advise(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let canonical = canonical_rdf_format("advise", format)?;

        if data.len() > MAX_VALIDATE_DATA_BYTES {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp::new(
                format!(
                    "advise: data payload is {} bytes, exceeding the {} byte ceiling; \
                     split the graph and advise on the parts (no silent truncation)",
                    data.len(),
                    MAX_VALIDATE_DATA_BYTES
                ),
            )));
        }

        // Shallow Tier-1 only (deep = false): advisory Notes come from the fast SHACL
        // pass. Same bundle parts and core as validate_local, so advice never diverges
        // from the shipped validator.
        let report = gmeow_validate::data_validate::run_with(
            gmeow_validate::data_validate::BundleParts {
                #[cfg(not(target_arch = "wasm32"))]
                native_gates: None,
                shapes: self.view.tier1_shapes()?,
                dataset: self.view.dataset.as_ref(),
            },
            data.as_bytes(),
            canonical,
            MCP_NAMESPACE,
            ADVISE_ORIGIN,
            false,
        )?;

        // Keep only the advisory tier (the advice.* Note family) and shape each as a
        // contrary-to-duty recommendation: the violated prohibition (avoid_when = the
        // finding message), the sub-ideal repair (how_to_use), and the permission gate
        // (use_when). The bridge builds `suggestions` as [howToUse, "Use when: <prose>"],
        // so the shared `gmeow_validate::advisory::ADVICE_USE_WHEN_PREFIX` marker cleanly
        // separates the permission leg.
        let recommendations: Vec<Value> = report
            .findings
            .iter()
            .filter(|f| f.code.starts_with(gmeow_validate::codes::ADVICE_FAMILY))
            .map(|f| {
                let mut use_when: Vec<String> = Vec::new();
                let mut how_to_use: Vec<String> = Vec::new();
                for suggestion in &f.suggestions {
                    match suggestion.strip_prefix(gmeow_validate::advisory::ADVICE_USE_WHEN_PREFIX)
                    {
                        Some(rest) => use_when.push(rest.to_string()),
                        None => how_to_use.push(suggestion.clone()),
                    }
                }
                // The governed term the advice formalizes rides as a `formalizes:<term>`
                // tag; the tripped subject is the first location's logical IRI.
                let formalizes = f
                    .tags
                    .iter()
                    .find_map(|t| t.strip_prefix("formalizes:").map(str::to_string));
                let subject = f.locations.iter().find_map(|l| l.logical.clone());
                json!({
                    "code": f.code,
                    "subject": subject,
                    "formalizes": formalizes,
                    "avoid_when": f.message,
                    "how_to_use": how_to_use,
                    "use_when": use_when,
                    "help_uri": gmeow_validate::rule_catalog::catalog_anchor_uri(&f.code),
                })
            })
            .collect();

        let response = json!({
            "ok": true,
            "tool": "advise",
            "recommendations": recommendations,
        });
        serde_json::to_string(&response).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!(
                "advise: serialize recommendations: {e}"
            )))
        })
    }

    #[cfg(feature = "core")]
    fn tool_explain_finding(&self, args: &Value) -> gmeow_errors::Result<String> {
        let target = required_str(args, "target_iri")?;
        self.view.explain_finding_json(target)
    }

    #[cfg(feature = "reasoning")]
    fn tool_store_claim(&self, args: &Value) -> gmeow_errors::Result<String> {
        let text = required_str(args, "text")?;
        let confidence = optional_f64(args, "confidence")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);

        // store_claim's precondition — a well-formed claim is presented — obtains once the input
        // validates (required text + in-range confidence, enforced above). Run the action as a
        // TR transaction; the engine's executional entailment over THIS start state is the gate
        // (no synthetic boolean — an absent precondition would fail the run).
        let obtains = [MCP_WELL_FORMED_CLAIM];
        let receipt = execute_memory_txn(
            self.view.action_policy()?,
            MCP_STORE_CLAIM_SCHEMA,
            &obtains,
            dry_run,
        )?;
        match &receipt {
            TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("store_claim precondition unmet: {reason}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            // Sandbox run: the verdict is observed, nothing is written or recorded.
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        let memory = self.claim_store()?;
        let claim = memory.store_claim(
            text,
            StoreOptions {
                source: optional_str(args, "source"),
                confidence,
                according_to: optional_str(args, "according_to"),
            },
        )?;
        let response =
            json!({"ok": true, "claim": claim_json(&claim), "transaction": txn_json(&receipt)});
        let generated = [claim.id.as_str()];
        let arguments = tool_arguments(
            args,
            &["text", "source", "confidence", "according_to", "dry_run"],
        );
        let payload = StructuredPayload::from_json(Some(&arguments), Some(&response))?;
        memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}store_claim"),
            ToolCallOptions {
                payload: Some(&payload),
                generated: &generated,
                audit: Some(ToolCallAudit {
                    schema: MCP_STORE_CLAIM_SCHEMA,
                    obtains: &obtains,
                }),
                ..ToolCallOptions::default()
            },
        )?;
        Ok(response.to_string())
    }

    /// `recall` — the READ half of the grounded-memory triad.
    ///
    /// A reasoning-SEGMENT tool despite linking no reasoner: it must answer from the same
    /// claim package `store_claim` wrote to, and in the browser a segment is an image and
    /// an image is a store. See [`REASONING_SEGMENT_TOOLS`] for why the triad is
    /// indivisible.
    #[cfg(feature = "reasoning")]
    fn tool_recall(&self, args: &Value) -> gmeow_errors::Result<String> {
        recall_json(self.claim_store()?.as_ref(), args)
    }

    /// `store_segment` — the SERIALIZATION of the grounded-memory claim package.
    ///
    /// Segmented with the rest of the triad for the same reason `recall` is: an export of
    /// a store held in another module's memory would export an empty store.
    #[cfg(feature = "reasoning")]
    fn tool_store_segment(&self, _args: &Value) -> gmeow_errors::Result<String> {
        store_segment_json(self.claim_store()?.as_ref())
    }

    #[cfg(feature = "reasoning")]
    fn tool_revise_belief(&self, args: &Value) -> gmeow_errors::Result<String> {
        let claim_id = required_str(args, "claim_id")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let memory = self.claim_store()?;
        let claims = memory.claims()?;
        let known: BTreeSet<&str> = claims.iter().map(|claim| claim.id.as_str()).collect();
        let active: BTreeSet<&str> = claims
            .iter()
            .filter(|claim| !claim.suppressed)
            .map(|claim| claim.id.as_str())
            .collect();
        if let Some(successor) = optional_str(args, "superseded_by")
            && !known.contains(successor)
        {
            return Ok(json!({
                "ok": false,
                "error": format!("unknown superseded_by id: {successor}"),
            })
            .to_string());
        }

        // revise_belief's precondition — the target claim exists — obtains iff claim_id is a known
        // claim (the existing pre-flight check, now expressed AS the TR precondition). The del
        // effect retires the active claim, so claimInMemory obtains iff it is not already
        // suppressed. The engine's executional entailment is the gate; an unknown id fails the run.
        let mut obtains: Vec<&str> = Vec::new();
        if known.contains(claim_id) {
            obtains.push(MCP_TARGET_CLAIM_EXISTS);
        }
        if active.contains(claim_id) {
            obtains.push(MCP_CLAIM_IN_MEMORY);
        }
        let receipt = execute_memory_txn(
            self.view.action_policy()?,
            MCP_REVISE_BELIEF_SCHEMA,
            &obtains,
            dry_run,
        )?;
        match &receipt {
            TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("unknown claim id: {claim_id}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            // Sandbox run: the verdict is observed, nothing is suppressed or recorded.
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "suppressed": claim_id,
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        memory.revise_claim(
            claim_id,
            RevisionOptions {
                reason: optional_str(args, "reason"),
                superseded_by: optional_str(args, "superseded_by"),
            },
        )?;
        let response = json!({
            "ok": true,
            "suppressed": claim_id,
            "superseded_by": optional_str(args, "superseded_by"),
            "transaction": txn_json(&receipt),
        });
        let arguments = tool_arguments(args, &["claim_id", "reason", "superseded_by", "dry_run"]);
        let payload = StructuredPayload::from_json(Some(&arguments), Some(&response))?;
        memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}revise_belief"),
            ToolCallOptions {
                payload: Some(&payload),
                generated: &[],
                audit: Some(ToolCallAudit {
                    schema: MCP_REVISE_BELIEF_SCHEMA,
                    obtains: &obtains,
                }),
                ..ToolCallOptions::default()
            },
        )?;
        Ok(response.to_string())
    }

    /// `conjecture_test` — a hypothetical evaluation of a
    /// candidate `logic:` formula against a KB. Computes and returns the projected engine
    /// verdict; NEVER TR-commits and NEVER appends to the conjecture library. For the
    /// committing counterpart ("store"), see [`Self::tool_store_conjecture`].
    ///
    /// `formula` is a Turtle `logic:` document naming the candidate (exactly one top-level
    /// `logic:Formula`, or exactly one ground `logic:` axiom lifted to a binary atom); `kb`
    /// is a Turtle KB the candidate is tested against; `standpoint` is the REQUIRED reified
    /// scope (Principle 9); `math_conjecture` optionally names the `math:Conjecture` twin so
    /// the statement is bridged to the runtime `logic:Conjecture` node via
    /// `math:conjectureUnderTest` (on every verdict) and a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    #[cfg(feature = "reasoning")]
    fn tool_conjecture_test(&self, args: &Value) -> gmeow_errors::Result<String> {
        let out = self.evaluate_conjecture_test_request(args)?;
        Ok(Self::conjecture_test_response(&out).to_string())
    }

    /// Evaluate the actual parsed, governed tool request exactly once.
    #[cfg(feature = "reasoning")]
    pub(super) fn evaluate_conjecture_test_request(
        &self,
        args: &Value,
    ) -> gmeow_errors::Result<ConjecturePureOutput> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str(args, "math_conjecture");
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        // The CLI's `gmeow conjecture test` shares `run_conjecture_test_pure` but calls it
        // directly with its own (possibly unbounded) args — this governance is scoped to
        // the agent-facing MCP wrapper.
        let budget = governed_budget(max_steps, max_answers);

        // The parse → test → project path is the SHARED evaluation core (also behind
        // `store_conjecture` and the CLI). This tool never TR-gates and never appends — it is
        // a thin wrapper rendering the pure verdict as the JSON response.
        run_conjecture_test_pure(&ConjectureRunPureInput {
            formula_ttl: formula_src,
            kb_ttl: kb_src,
            standpoint,
            math_conjecture,
            max_steps: budget.max_steps,
            max_answers: budget.max_answers,
        })
    }

    /// Render the evaluator's existing verdict bytes without parsing or reevaluation.
    #[cfg(feature = "reasoning")]
    pub(super) fn conjecture_test_response(out: &ConjecturePureOutput) -> Value {
        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        json!({
            "ok": true,
            "verdict": verdict_json,
            "witness": witness_json,
            "conjecture": out.node_iri,
            // T1: the same grounded-judgment key/shape `verify_graph`/`explain_quad` carry —
            // here the embedded `logic:ReasoningResult` the engine's verdict was read from,
            // wrapped in the content-addressed `logic:Conjecture` node.
            "judgment_nquads": out.verdict_nt,
            "artifact": gmeow_logic::result_rdf::artifact_envelope(&out.artifact),
        })
    }

    /// `store_conjecture` — test a candidate `logic:` formula against
    /// a KB, project the engine verdict, and — TR-gated on the `persistConjecture` schema —
    /// APPEND it to the append-only conjecture library. Runs the SAME evaluation as
    /// `conjecture_test`; the difference is entirely in the tail (TR-gate + persist).
    ///
    /// `formula` / `kb` / `standpoint` / `math_conjecture` are as `conjecture_test`;
    /// `dry_run=true` computes and returns the verdict (via a hypothetical TR commit) but
    /// WRITES NOTHING.
    #[cfg(feature = "reasoning")]
    fn tool_store_conjecture(&self, args: &Value) -> gmeow_errors::Result<String> {
        let out = self.evaluate_store_conjecture_request(args)?;
        Ok(Self::store_conjecture_response(&out).to_string())
    }

    /// Evaluate and TR-gate the actual request once, retaining its captured policy.
    #[cfg(feature = "reasoning")]
    pub(super) fn evaluate_store_conjecture_request(
        &self,
        args: &Value,
    ) -> gmeow_errors::Result<ConjectureRunOutput> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str(args, "math_conjecture");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        // The CLI's `gmeow conjecture test` shares `run_conjecture_test` but calls it
        // directly with its own (possibly unbounded) args — this governance is scoped to
        // the agent-facing MCP wrapper.
        let budget = governed_budget(max_steps, max_answers);

        // The parse → test → project → TR-gate → persist path is the SHARED persisting core
        // (also behind the CLI `gmeow conjecture test`); the tool is a thin wrapper rendering
        // its outcome as the JSON response.
        run_conjecture_test(
            &ConjectureRunInput {
                formula_ttl: formula_src,
                kb_ttl: kb_src,
                standpoint,
                math_conjecture,
                dry_run,
                max_steps: budget.max_steps,
                max_answers: budget.max_answers,
            },
            &self.store_medium()?,
            self.view.action_policy()?,
        )
    }

    /// Render every persistence outcome from the already-evaluated typed result.
    #[cfg(feature = "reasoning")]
    pub(super) fn store_conjecture_response(out: &ConjectureRunOutput) -> Value {
        // The five-field verdict summary + witness, rendered for every response path.
        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        // T1: the verdict — and its grounded `judgment_nquads` projection — was ALREADY
        // computed by `evaluate_conjecture` before the TR gate ran, so it is available (and
        // carried) on every path below, including precondition-unmet: the engine's judgment
        // about the candidate is real regardless of whether the persist itself succeeded.
        if let Some(reason) = &out.precondition_unmet {
            return json!({
                "ok": false,
                "error": format!("persistConjecture precondition unmet: {reason}"),
                "verdict": verdict_json,
                "witness": witness_json,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            "artifact": gmeow_logic::result_rdf::artifact_envelope(&out.artifact),
            });
        }
        if out.dry_run {
            return json!({
                "ok": true,
                "dry_run": true,
                "verdict": verdict_json,
                "witness": witness_json,
                "conjecture": out.node_iri,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            "artifact": gmeow_logic::result_rdf::artifact_envelope(&out.artifact),
            });
        }

        json!({
            "ok": true,
            "verdict": verdict_json,
            "witness": witness_json,
            "conjecture": out.node_iri,
            "transaction": txn_json(&out.receipt),
            "judgment_nquads": out.verdict_nt,
            "artifact": gmeow_logic::result_rdf::artifact_envelope(&out.artifact),
        })
    }

    /// `refute_conjecture` — the compensating author-WITHDRAWAL counterpart of
    /// `store_conjecture` (P10, exactly as `revise_belief` compensates `store_claim`). It
    /// appends one compensating "author-withdrawn" segment to the append-only conjecture
    /// library, flipping the target node's EFFECTIVE `logic:conjectureLifecycleState` to
    /// `logic:ConjectureWithdrawn` — recorded, never deleted; the prior segments stay intact.
    ///
    /// The write is a REAL TR gate on the `withdrawConjecture` schema, whose precondition —
    /// the conjecture is still in the library and NOT already withdrawn — is DERIVED from the
    /// live library state read back by SEGMENT ORDER ([`read_library`], R3): the
    /// `conjectureInLibrary` situation obtains iff the node exists and its effective state is
    /// not yet `Withdrawn`. An unknown id or an already-withdrawn node yields an empty start
    /// state, so the executional-entailment run FAILS the commit and the tool returns
    /// `ok:false` before writing. `dry_run=true` witnesses the hypothetical commit and appends
    /// nothing.
    #[cfg(feature = "reasoning")]
    fn tool_refute_conjecture(&self, args: &Value) -> gmeow_errors::Result<String> {
        let conjecture_id = required_str(args, "conjecture_id")?;
        let reason = optional_str(args, "reason").unwrap_or("");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let library_handle = conjecture_library()?;
        let library_ref = library_handle.as_ref();

        // The read (library's EFFECTIVE state, by segment order) → precondition-check → (on a
        // real commit) library-append → audit-append sequence runs ENTIRELY inside ONE held
        // exclusive lock (`with_library_lock`): without it, two concurrent
        // `refute_conjecture` calls against the same id could both read "not yet withdrawn",
        // both pass the precondition, and both commit a withdrawal segment (lost-update). The
        // lock forces the second caller to observe the FIRST caller's already-committed
        // `ConjectureWithdrawn` state before it decides anything.
        with_library_lock(library_ref, || {
            // Read the library's EFFECTIVE state by segment order (last-writer-wins). The node
            // is withdrawable iff it is a stored conjecture whose effective state is not already
            // Withdrawn — the `del(conjectureInLibrary)` of a prior withdrawal retired it.
            let library = read_library(library_ref)?;
            let effective = library.get(conjecture_id).copied();
            let exists = effective.is_some();
            let in_library = matches!(
                effective,
                Some(state) if state != ConjectureLifecycleState::Withdrawn
            );

            // The precondition `conjectureInLibrary` obtains iff the node is present and not yet
            // withdrawn — the engine's executional entailment over THIS derived start state is
            // the commit gate (an absent situation fails the run; no synthetic boolean).
            let mut obtains: Vec<&str> = Vec::new();
            if in_library {
                obtains.push(MCP_CONJECTURE_IN_LIBRARY);
            }
            let receipt = execute_memory_txn(
                self.view.action_policy()?,
                MCP_WITHDRAW_CONJECTURE_SCHEMA,
                &obtains,
                dry_run,
            )?;
            // T1: the compensating withdrawal's OWN grounded RDF projection — content-addressed
            // on (conjecture_id, reason), pure and side-effect-free — computed once here so BOTH
            // the hypothetical (dry-run) and committed success paths can carry it under the SAME
            // `judgment_nquads` key the read tools and the other two conjecture tools use. The
            // precondition-unmet path below deliberately does NOT carry it: no withdrawal was
            // validly grounded (unknown id, or already withdrawn), so emitting this body there
            // would fabricate a judgment about a state change that never obtained.
            let nt_body = project_conjecture_withdrawal(conjecture_id, reason);
            match &receipt {
                TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                    let detail = if !exists {
                        format!("unknown conjecture id: {conjecture_id}")
                    } else {
                        format!("conjecture already withdrawn: {conjecture_id}")
                    };
                    return Ok(json!({
                        "ok": false,
                        "error": format!("withdrawConjecture precondition unmet: {detail}"),
                        "transaction": txn_json(&receipt),
                    })
                    .to_string());
                }
                // Sandbox run: the compensating verdict is witnessed, nothing is appended.
                TxReceipt::HypotheticalSuccess { .. } => {
                    return Ok(json!({
                        "ok": true,
                        "dry_run": true,
                        "conjecture": conjecture_id,
                        "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
                        "transaction": txn_json(&receipt),
                        "judgment_nquads": nt_body,
                    })
                    .to_string());
                }
                TxReceipt::CommittedSuccess { .. } => {}
            }

            // Committed: APPEND the compensating author-withdrawal segment (the target node
            // re-marked `ConjectureWithdrawn`, author reason, reviewer-asserted provenance)
            // TOGETHER with its cold-auditable trajectory segment (keyed to a content-addressed
            // call id) as ONE atomic file replace — so a failure building or committing either
            // segment can never leave the withdrawal applied without its audit record, or vice
            // versa. The library is still append-only overall: no PRIOR segment's bytes are
            // touched, only new bytes are added.
            let medium = self.store_medium()?;
            let existing = library_ref.read_bytes()?;
            let withdrawal_segment = build_nt_segment(&existing, &medium, &nt_body)?;
            // The audit continues the BODY, so it is authored against the bytes the body
            // leaves behind — not against the pre-commit library, which the body already moved past.
            let mut body_image = existing.clone();
            body_image.extend_from_slice(&withdrawal_segment);
            let call_id = format!(
                "urn:gmeow:conjecture-call:{}",
                sha256_hex(format!("withdraw\u{1}{conjecture_id}\u{1}{reason}").as_bytes())
            );
            let audit_segment = build_audit_segment(
                &body_image,
                &medium,
                &call_id,
                MCP_WITHDRAW_CONJECTURE_SCHEMA,
                &[MCP_CONJECTURE_IN_LIBRARY],
                "1970-01-01T00:00:00Z",
            )?;
            append_library_segments(library_ref, &[withdrawal_segment, audit_segment])?;
            Ok(json!({
                "ok": true,
                "conjecture": conjecture_id,
                "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
                "transaction": txn_json(&receipt),
                "judgment_nquads": nt_body,
            })
            .to_string())
        })
    }

    /// Score ONE slice on demand and return its grades + advice as JSON. This is a
    /// read-only advisory surface: it computes a fresh assessment for the caller and
    /// folds nothing. The whole-repo `gmeow:QualityAssessment` graph is instead attached
    /// to the carrier by the regeneration pipeline (`stage-source-load` via
    /// [`gmeow_slice_quality::assessment_nquads`]) so it ships inside `gmeow.gts`; this
    /// tool never mutates the bundle.
    ///
    /// The rubric standard is sourced from the embedded bundle bytes
    /// ([`McpView::gts_bytes`]) via [`gmeow_slice_quality::score_external_slice_files`]
    /// — the wheel-shippable `ScoringEnv::Bundle` path the `gmeow slice quality` CLI
    /// uses — so the tool is checkout-free and available on the Consumer surface.
    ///
    /// The slice arrives as an IN-MEMORY FILE MAP (`files`: slice-relative path -> file
    /// text), not a directory. That is what makes the tool servable by an engine with no
    /// filesystem at all, and it is also the honest surface for an agent that HAS the
    /// slice as text (a paste, an upload, a git blob) and no place to put it. A map with
    /// no `manifest.ttl` entry is a hard error naming it.
    #[cfg(feature = "reasoning")]
    fn tool_slice_quality(&self, args: &Value) -> gmeow_errors::Result<String> {
        let files = required_file_map("slice_quality", args, "files")?;
        let report = gmeow_slice_quality::score_external_slice_from_files(
            self.view.slice_quality_standards()?,
            &files,
        )
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp::new(format!("slice_quality: {e}")))
        })?;
        let grades: Vec<Value> = report
            .assessment
            .grades
            .iter()
            .map(|g| {
                let axis = g.axis_iri.rsplit(['/', '#']).next().unwrap_or(&g.axis_iri);
                json!({ "axis": axis, "tier": g.tier.label, "score": g.score })
            })
            .collect();
        let advice: Vec<Value> = report
            .advisories
            .iter()
            .map(|f| json!({ "code": f.code, "message": f.message }))
            .collect();
        Ok(json!({
            "slice": report.assessment.slice,
            "rollup_tier": report.assessment.rollup.label,
            "grades": grades,
            "advice": advice,
        })
        .to_string())
    }

    /// Serve the pre-assembled `gmeow:AuthoringPacket`(s) for one slice straight from
    /// the embedded bundle graph — a checkout-free consumer surface. `slice` is a slice
    /// short-name (`ai`) or full slice IRI; the optional `axis` (default `whole`, the
    /// only axis the pipeline currently partitions along) and `batch` narrow the result.
    /// A slice/axis/batch with no packet in the bundle is a hard error (never a vacuous
    /// empty pass). The covered-term IRIs the packet lists resolve to their full
    /// definition/axiom content through the existing `lookup_term` / `doc_card` tools.
    #[cfg(feature = "core")]
    fn tool_slice_brief(&self, args: &Value) -> gmeow_errors::Result<String> {
        let slice = required_str(args, "slice")?;
        let axis = optional_str(args, "axis");
        let batch = optional_step_count(args, "batch")?;
        let slice_iri = expand_slice_iri(slice);
        let out =
            extract_authoring_packets(self.view.authoring_briefs()?, &slice_iri, axis, batch)?;
        Ok(out.to_string())
    }

    /// `submit_candidate` — the neurosymbolic propose/verify seam's WRITE leg: test a candidate
    /// `logic:` formula against a KB, and — POLARITY-gated on the `submitCandidate` schema
    /// (admissible iff corroborated) — APPEND it to the append-only candidate library. Runs the
    /// SAME evaluation as `conjecture_test`; a refuted or open candidate is NOT admitted and
    /// stages nothing. `formula`/`kb`/`standpoint`/`math_conjecture` are as `conjecture_test`;
    /// optional `for_slice`/`for_packet` record target provenance; `dry_run=true` computes the
    /// verdict but WRITES NOTHING.
    #[cfg(feature = "reasoning")]
    fn tool_submit_candidate(&self, args: &Value) -> gmeow_errors::Result<String> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let kb_format = optional_str_checked(args, "kb_format")?.unwrap_or("text/turtle");
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str_checked(args, "math_conjecture")?;
        let for_slice = optional_str_checked(args, "for_slice")?;
        let for_packet = optional_str_checked(args, "for_packet")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        let budget = governed_budget(max_steps, max_answers);

        let out = run_submit_candidate(
            &CandidateSubmitInput {
                formula_ttl: formula_src,
                kb_ttl: kb_src,
                kb_format,
                standpoint,
                math_conjecture,
                for_slice,
                for_packet,
                dry_run,
                max_steps: budget.max_steps,
                max_answers: budget.max_answers,
            },
            &self.store_medium()?,
            self.view.action_policy()?,
        )?;

        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        // NOT admissible (refuted / open) OR a dry sandbox on a non-admissible candidate: the
        // precondition was unmet, so NOTHING was appended (AC6). The verdict is still carried.
        if let Some(reason) = &out.precondition_unmet {
            return Ok(json!({
                "ok": false,
                "error": format!("submitCandidate precondition unmet (candidate not admissible): {reason}"),
                "admissible": out.admissible,
                "verdict": verdict_json,
                "witness": witness_json,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            "artifact": gmeow_logic::result_rdf::artifact_envelope(&out.artifact),
            })
            .to_string());
        }
        if out.dry_run {
            return Ok(json!({
                "ok": true,
                "dry_run": true,
                "admissible": out.admissible,
                "verdict": verdict_json,
                "witness": witness_json,
                "candidate": out.node_iri,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            "artifact": gmeow_logic::result_rdf::artifact_envelope(&out.artifact),
            })
            .to_string());
        }

        // Admissible + committed: the candidate segment was appended (AC5).
        Ok(json!({
            "ok": true,
            "admissible": out.admissible,
            "committed": out.committed,
            "verdict": verdict_json,
            "witness": witness_json,
            "candidate": out.node_iri,
            "transaction": txn_json(&out.receipt),
            "judgment_nquads": out.verdict_nt,
            "artifact": gmeow_logic::result_rdf::artifact_envelope(&out.artifact),
        })
        .to_string())
    }

    /// `withdraw_candidate` — the compensating author-WITHDRAWAL counterpart of
    /// `submit_candidate` (P10, exactly as `refute_conjecture` compensates `store_conjecture`).
    /// It appends one compensating "withdrawn" segment to the append-only candidate library,
    /// flipping the target node's EFFECTIVE lifecycle to `logic:ConjectureWithdrawn` — recorded,
    /// never deleted. The write is a REAL TR gate on the `withdrawCandidate` schema, whose
    /// precondition — the candidate is still in the library and not already withdrawn — is
    /// DERIVED from the live library state read back by SEGMENT ORDER. An unknown id or an
    /// already-withdrawn node fails the commit and returns `ok:false` before writing.
    #[cfg(feature = "reasoning")]
    fn tool_withdraw_candidate(&self, args: &Value) -> gmeow_errors::Result<String> {
        let candidate_id = required_str(args, "candidate_id")?;
        let reason = optional_str(args, "reason").unwrap_or("");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        run_withdraw_candidate(
            candidate_id,
            reason,
            dry_run,
            &self.store_medium()?,
            self.view.action_policy()?,
        )
    }

    #[cfg(feature = "reasoning")]
    fn tool_list_candidates(&self, args: &Value) -> gmeow_errors::Result<String> {
        run_list_candidates(
            optional_str(args, "slice"),
            optional_str(args, "disposition"),
        )
    }

    /// Resolve `uri` against the assembled surface and run its handler, returning
    /// `(mimeType, body)`.
    ///
    /// The `?lang=` query is stripped and resolved here (it is a property of the
    /// REQUEST, shared by every resource) and handed to the handler; the media type
    /// comes from the advertised descriptor. Like tool dispatch this is TOTAL: a URI
    /// the surface does not carry raises `mcp.unknown-resource` naming it.
    ///
    /// # Errors
    ///
    /// An unresolvable `?lang=`, an unregistered URI, or whatever the handler raises.
    fn read_resource_text(&self, uri: &str) -> gmeow_errors::Result<(String, String)> {
        let (base, query) = uri.split_once('?').unwrap_or((uri, ""));
        let requested = lang_from_query(query)
            .map(|raw| resolve_lang(Some(raw), &self.tag_map, &self.available))
            .transpose()?
            .unwrap_or_else(|| self.startup_requested.clone());
        self.surface.read_resource(self, base, &requested)
    }

    /// The grounded-memory claim package this server's memory triad reads and writes.
    ///
    /// Resolved through the [`storage`] seam, so a native host gets its real
    /// `memory.gts` (at `GMEOW_MEMORY_PATH`, else `~/.gmeow/memory.gts`) and a browser
    /// host gets the in-process store — the tools themselves never learn which.
    ///
    /// Gated on the `reasoning` feature because the whole memory surface
    /// ([`CLAIM_STORE_TOOLS`]) is served by that segment: the writes are pinned there by
    /// their Transaction-Logic commit gate, and the reads must answer from the same store,
    /// which in a wasm deployment means the same image. A core-only build reaches the claim
    /// package from nowhere, and saying so in the type system is what keeps that true.
    #[cfg(feature = "reasoning")]
    pub(super) fn claim_store(&self) -> gmeow_errors::Result<Arc<dyn ClaimStore>> {
        storage().claim_store(self.view.gts_bytes())
    }

    /// The medium a runtime claim store is PRIMED with: the [`MEMORY_HOT_DICTIONARY`] bytes the
    /// loaded bundle pins.
    ///
    /// Priming is not an optimisation. A GTS segment declares exactly ONE codec catalog, so the
    /// claim package and the engine-minted trajectory-audit segment appended beside it have to
    /// name the SAME one; an unprimed store lets the two diverge, and the file then stops
    /// reconstructing — `Memory::graph()` yields `None`, so `claims()` reports an EMPTY store and
    /// `revise` rejects a claim that was stored moments earlier as unknown.
    ///
    /// # Errors
    ///
    /// The loaded bundle pins no [`MEMORY_HOT_DICTIONARY`] — a store cannot be primed with an id
    /// the bundle does not carry, and there is no unprimed fallback.
    #[cfg(feature = "reasoning")]
    fn store_medium(&self) -> gmeow_errors::Result<StoreMedium> {
        store_medium(self.view.gts_bytes(), MEMORY_HOT_DICTIONARY)
    }
}
