// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The real inputs the browser lane drives the shipped tool surface with.
//
// Every fixture below is a document, not a stub: each one is transcoded, validated,
// reasoned over or encoded by the SHIPPED engine, and every invented individual lives under
// `example.org` while every predicate and class is a term the bundle already defines. No
// vocabulary is minted here.
//
// `TOOL_INPUTS` is the argument table for the READ surface. It is keyed by tool name, and a
// spec asserts its key set EQUALS the pane set the shipped `action_policy` derives — in both
// directions. That equality is what keeps the coverage claim honest: growing the ontology's
// read surface fails this lane until the new tool is given a real input, and it can never be
// satisfied by a lane that quietly skipped one.

/** The RDF-1.2 record every star assertion turns on: a statement annotated by a quoted triple. */
export const ANNOTATED_RECORD = `@prefix ex:   <https://example.org/gmeow/console/> .
@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:answer rdfs:label "the console answered" .
ex:statement rdf:reifies <<( ex:answer rdfs:label "the console answered" )>> .
`;

/** A one-subsumption knowledge base whose closure the reasoner must derive a type from. */
export const SUBSUMPTION = `@prefix ex:   <https://example.org/gmeow/console/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:Recorded rdfs:subClassOf ex:Audited .
ex:call a ex:Recorded .
`;

/** The entailment the chase must produce over [`SUBSUMPTION`], as canonical N-Quads. */
export const SUBSUMPTION_ENTAILMENT =
  "<https://example.org/gmeow/console/call> " +
  "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> " +
  "<https://example.org/gmeow/console/Audited> .";

/**
 * A document built ONLY from codebook-covered terms.
 *
 * GMN-1 encodes an out-of-codebook IRI as a by-reference token, so a fixture carrying
 * `example.org` individuals cannot round-trip through `gmn_expand` — the fixed-point claim
 * needs a record every leg can actually carry.
 */
export const CODEBOOK_COVERED = `@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:ToolCall rdfs:subClassOf gmeow:Activity .
`;

/** A universally-quantified Horn candidate whose head fires `rdf:type(x, ex:B)`. */
export const CONJECTURE_FORMULA = `@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/gmeow/console/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
ex:cand a logic:Formula ;
    logic:forall ex:body ;
    logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "x" ] .
ex:body a logic:Formula ;
    logic:antecedent ex:ant ;
    logic:consequent ex:con .
ex:ant a logic:Formula ;
    logic:relation ex:trigger ;
    logic:argument [ logic:termIndex 0 ; logic:termVariable "x" ] ;
    logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .
ex:con a logic:Formula ;
    logic:relation rdf:type ;
    logic:argument [ logic:termIndex 0 ; logic:termVariable "x" ] ;
    logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .
`;

/** A KB whose head class is DISJOINT with the witness's asserted type, so firing clashes. */
export const CONJECTURE_KB = `@prefix ex:  <https://example.org/gmeow/console/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
ex:a ex:trigger ex:mark .
ex:a rdf:type ex:A .
ex:A owl:disjointWith ex:B .
`;

/** The standpoint the conjecture is evaluated in. */
export const CONJECTURE_STANDPOINT = "https://example.org/gmeow/console/standpoint";

/** A single-term synthetic slice, handed to the quality scorer as bytes. */
export const SLICE_FILES = {
  "manifest.ttl": `@prefix gmeow:   <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:    <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:    <http://www.w3.org/2004/02/skos/core#> .
@prefix dcterms: <http://purl.org/dc/terms/> .
<https://blackcatinformatics.ca/gmeow/slices/consolesmoke>
    a gmeow:Slice ;
    rdfs:label "consolesmoke"@x-gmeow-english ;
    skos:definition "A synthetic single-term slice handed to the shipped quality scorer as bytes by the browser smoke lane."@x-gmeow-english ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/consolesmoke> ;
    dcterms:title "Console smoke slice"@x-gmeow-english ;
    dcterms:creator "Blackcat Informatics® Inc." ;
    gmeow:sliceTier gmeow:tierCore ;
    gmeow:sliceConsumer "The standalone console's browser smoke lane."@x-gmeow-english .
`,
  "module.ttl": `@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
gmeow:ConsoleSmokeProbe a owl:Class ;
    rdfs:label "Console Smoke Probe"@x-gmeow-english ;
    skos:definition "A probe class scored against the shipped rubric."@x-gmeow-english .
`,
};

/** A SPARQL query the bundle answers, used wherever a real query is needed. */
export const BUNDLE_QUERY = `PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?term ?label WHERE { ?term rdfs:label ?label } ORDER BY ?term LIMIT 3`;

/** A term the shipped bundle defines — the anchor every term-shaped tool is driven with. */
export const KNOWN_TERM = "gmeow:ToolCall";

const GMEOW = "https://blackcatinformatics.ca/gmeow/";
const RDFS = "http://www.w3.org/2000/01/rdf-schema#";

/**
 * The argument table for the derived READ surface.
 *
 * Each entry is `async (ctx) => args`, where `ctx.call(tool, args)` dispatches through the
 * ASSEMBLED worker. Entries that need an address the bundle mints — a diagnostics anchor,
 * an entailed quad — DERIVE it by asking the engine, rather than pinning a digest that
 * would rot on the next regeneration.
 */
export const TOOL_INPUTS = {
  action_policy: async () => ({}),
  advise: async () => ({ data: SUBSUMPTION, format: "turtle" }),
  coherence_certificate: async () => ({}),
  competency_questions: async () => ({ term: KNOWN_TERM }),
  conjecture_test: async () => ({
    formula: CONJECTURE_FORMULA,
    kb: CONJECTURE_KB,
    standpoint: CONJECTURE_STANDPOINT,
  }),
  convert: async () => ({ data: ANNOTATED_RECORD, from: "turtle", to: "nquads" }),
  counter_examples: async () => ({ term: KNOWN_TERM }),
  distribution_matrix: async () => ({}),
  doc_card: async () => ({ term: KNOWN_TERM, format: "markdown" }),
  docs_search: async () => ({ query: "tool call", limit: 3 }),
  encode_gmn1: async () => ({ data: CODEBOOK_COVERED, format: "turtle" }),
  entailments: async () => ({ term: KNOWN_TERM }),
  explain_finding: async (ctx) => ({ target_iri: await diagnosticsAnchor(ctx) }),
  explain_quad: async (ctx) => {
    // DERIVED, not pinned: the target is a conclusion the engine's own `entailments` tool
    // reported for a shipped term, expanded back to absolute IRIs.
    const { entailments } = await ctx.call("entailments", { term: KNOWN_TERM });
    const first = entailments[0];
    if (first === undefined) {
      throw new Error(`the bundle entails nothing for ${KNOWN_TERM} — explain_quad has no target`);
    }
    const [subject, , object] = first.conclusion.split(/\s+/);
    return {
      subject: expandCurie(subject),
      predicate: `${RDFS}subClassOf`,
      object_value: expandCurie(object),
      object_kind: "iri",
      graph: "",
      max_steps: 64,
    };
  },
  gmn_expand: async (ctx) => {
    const { gmn1 } = await ctx.call("encode_gmn1", { data: CODEBOOK_COVERED, format: "turtle" });
    return { gmn: gmn1 };
  },
  gmn_explain: async (ctx) => {
    const { legend } = await ctx.call("gmn_glyph_legend", {});
    return { glyph: legend[0].glyph };
  },
  gmn_glyph_legend: async () => ({}),
  gmn_validate: async (ctx) => {
    const { gmn1 } = await ctx.call("encode_gmn1", { data: CODEBOOK_COVERED, format: "turtle" });
    return { gmn: gmn1 };
  },
  list_candidates: async () => ({}),
  llms_full: async () => ({}),
  llms_txt: async () => ({}),
  lookup_term: async () => ({ term: KNOWN_TERM }),
  okf_index: async () => ({}),
  query_docs: async () => ({ query: BUNDLE_QUERY }),
  query_local: async () => ({
    data: ANNOTATED_RECORD,
    format: "turtle",
    scope: "input",
    query: "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 5",
  }),
  reason_graph: async () => ({ data: SUBSUMPTION, format: "turtle" }),
  recall: async () => ({ query: "launch window" }),
  slice_brief: async (ctx) => {
    // The slice name is read off the engine's own documentation card for a shipped term,
    // so the brief is asked about a slice the bundle actually declares.
    const { card } = await ctx.call("doc_card", { term: KNOWN_TERM, format: "markdown" });
    const slice = /^- slice: (\S+)$/m.exec(card ?? "");
    if (slice === null) {
      throw new Error(`the doc card for ${KNOWN_TERM} names no slice — slice_brief has no subject`);
    }
    return { slice: slice[1] };
  },
  slice_quality: async () => ({ files: SLICE_FILES }),
  store_segment: async () => ({}),
  validate_local: async () => ({ data: ANNOTATED_RECORD, format: "turtle" }),
  verify_graph: async () => ({ data: SUBSUMPTION, format: "turtle" }),
};

/** Expand the two prefixes the engine's own `entailments` rendering uses. */
function expandCurie(curie) {
  if (curie.startsWith("gmeow:")) return `${GMEOW}${curie.slice("gmeow:".length)}`;
  if (curie.startsWith("logic:")) return `https://blackcatinformatics.ca/logic/${curie.slice("logic:".length)}`;
  if (curie.startsWith("rdfs:")) return `${RDFS}${curie.slice("rdfs:".length)}`;
  return curie;
}

/** One `gmeow:NonTrivialAnchor` IRI, read out of the shipped bundle's diagnostics graph. */
async function diagnosticsAnchor(ctx) {
  const answer = await ctx.call("query_local", {
    data: "",
    format: "turtle",
    scope: "bundle",
    query: `PREFIX gmeow: <${GMEOW}>
SELECT ?anchor WHERE {
  GRAPH <${GMEOW}graph/diagnostics> { ?anchor a gmeow:NonTrivialAnchor }
} ORDER BY ?anchor LIMIT 1`,
  });
  const binding = answer.results?.bindings?.[0]?.anchor?.value;
  if (typeof binding !== "string") {
    throw new Error("the shipped bundle's diagnostics graph declares no anchor — explain_finding has no target");
  }
  return binding;
}
