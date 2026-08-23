// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The console's worked vignettes.
//
// Each one is a real invocation of a real pane over data drawn from the SHIPPED ontology's
// own surfaces — `gmeow:ToolCall` / `gmeow:derivedBy` / `gmeow:wasDerivedFrom` from the
// agentic slice, `logic:ActionSchema` / `logic:instantiatesSchema` /
// `logic:properPartOf` from the logic core. No vocabulary is minted here: every predicate
// and class below is a term the bundle already defines, and every INVENTED individual
// lives under `example.org`, never under the `gmeow:` or `logic:` namespaces.
//
// Every vignette exercises RDF-1.2 quoted triples (`<<( s p o )>>`). That is deliberate,
// not decorative: the annotated-statement shape is how a console session keeps an answer
// attributable to the call that produced it, so a gallery that never showed one would be
// demonstrating a different product than the one that ships.
//
// DOM-free: this module exports data, and the element renders it.

const EX = "https://example.org/gmeow/console/";

/** The shared prologue every vignette's Turtle opens with. */
const PROLOGUE = [
  "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .",
  "@prefix logic: <https://blackcatinformatics.ca/logic/> .",
  "@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .",
  "@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .",
  "@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .",
  `@prefix ex:    <${EX}> .`,
  "",
].join("\n");

/**
 * One recorded, annotated call — the canonical shape the whole gallery builds on.
 *
 * The call itself is a `gmeow:ToolCall` bound to a `logic:ActionSchema`; the ANSWER it
 * produced is asserted as an ordinary triple and then annotated through an RDF-1.2 triple
 * term, whose reifier carries `gmeow:derivedBy` (the call) and one `gmeow:wasDerivedFrom`
 * per antecedent. Reading the reifier back recovers the whole provenance chain.
 */
const ANNOTATED_CALL = `${PROLOGUE}ex:turn a gmeow:Activity ;
    rdfs:label "console vignette turn"@x-gmeow-english ;
    logic:transitionFromState ex:startState .

ex:startState a logic:Situation .

ex:lookupCall a gmeow:ToolCall ;
    logic:instantiatesSchema ex:lookupTermSchema ;
    logic:properPartOf ex:turn ;
    gmeow:atTime "2026-01-01T00:00:00Z"^^xsd:dateTime ;
    gmeow:eventTemporalFrame gmeow:temporalFrameUtc ;
    gmeow:usedTool ex:lookupTermTool ;
    gmeow:sessionStoreSegment "0000" .

ex:lookupTermTool a gmeow:SoftwareAgent .
ex:lookupTermSchema a logic:ActionSchema ;
    logic:mcpToolName "lookup_term" .

# The ANSWER, asserted plainly …
ex:answer rdfs:label "gmeow:ToolCall is an event class"@x-gmeow-english .

# … and annotated as a STATEMENT through an RDF-1.2 triple term.
ex:answerStatement rdf:reifies
        <<( ex:answer rdfs:label "gmeow:ToolCall is an event class"@x-gmeow-english )>> ;
    gmeow:derivedBy ex:lookupCall ;
    gmeow:wasDerivedFrom gmeow:ToolCall ;
    gmeow:wasDerivedFrom gmeow:derivedBy .
`;

/** A two-step trajectory whose steps are annotated statements — the auditor's own shape. */
const TRAJECTORY = `${PROLOGUE}ex:auditTurn a gmeow:Activity ;
    logic:transitionFromState ex:auditStart ;
    logic:planGoal ex:auditGoal .

ex:auditStart a logic:Situation .

ex:callOne a gmeow:ToolCall ;
    logic:instantiatesSchema ex:validateSchema ;
    logic:properPartOf ex:auditTurn ;
    gmeow:atTime "2026-01-01T00:00:01Z"^^xsd:dateTime ;
    gmeow:eventTemporalFrame gmeow:temporalFrameUtc ;
    gmeow:sessionStoreSegment "0000" .

ex:callTwo a gmeow:ToolCall ;
    logic:instantiatesSchema ex:reasonSchema ;
    logic:properPartOf ex:auditTurn ;
    gmeow:atTime "2026-01-01T00:00:02Z"^^xsd:dateTime ;
    gmeow:eventTemporalFrame gmeow:temporalFrameUtc ;
    gmeow:sessionStoreSegment "0001" .

ex:validateSchema a logic:ActionSchema ; logic:mcpToolName "validate_local" .
ex:reasonSchema   a logic:ActionSchema ; logic:mcpToolName "reason_graph" .

# The second step's conclusion, annotated with the step it came out of AND its antecedent.
ex:closureStatement rdf:reifies <<( ex:subject rdf:type ex:Derived )>> ;
    gmeow:derivedBy ex:callTwo ;
    gmeow:wasDerivedFrom ex:callOne .

ex:subject rdf:type ex:Derived .
`;

/** A subsumption the reasoner closes over, with the premise recorded as a quoted triple. */
const ENTAILMENT = `${PROLOGUE}ex:Recorded rdfs:subClassOf ex:Audited .
ex:call rdf:type ex:Recorded .

# The PREMISE, annotated so the closure the reasoner returns stays attributable.
ex:premiseStatement rdf:reifies <<( ex:Recorded rdfs:subClassOf ex:Audited )>> ;
    gmeow:derivedBy ex:seedCall ;
    gmeow:wasDerivedFrom rdfs:subClassOf .

ex:seedCall a gmeow:ToolCall ;
    logic:instantiatesSchema ex:seedSchema ;
    logic:properPartOf ex:seedTurn ;
    gmeow:atTime "2026-01-01T00:00:00Z"^^xsd:dateTime ;
    gmeow:eventTemporalFrame gmeow:temporalFrameUtc .

ex:seedTurn a gmeow:Activity ; logic:transitionFromState ex:seedStart .
ex:seedStart a logic:Situation .
ex:seedSchema a logic:ActionSchema ; logic:mcpToolName "store_claim" .
`;

/**
 * The gallery.
 *
 * `tool` names the pane the vignette runs in; `args` is exactly the argument object that
 * pane's JSON Schema declares, so a vignette is loaded into a form and RUN rather than
 * being a screenshot of one.
 */
export const VIGNETTES = [
  {
    id: "annotated-answer",
    title: "An answer that stays attributable",
    pane: "validate_local",
    args: { data: ANNOTATED_CALL, format: "turtle" },
    prose:
      "A recorded gmeow:ToolCall, the answer it produced, and the RDF-1.2 triple term that " +
      "binds the two: the reifier carries gmeow:derivedBy (the call) and one " +
      "gmeow:wasDerivedFrom per antecedent. Tier-1 validation runs over the whole record.",
  },
  {
    id: "trajectory",
    title: "A two-step trajectory the native auditor can read",
    pane: "validate_local",
    args: { data: TRAJECTORY, format: "turtle" },
    prose:
      "Two bound gmeow:ToolCalls under one logic:properPartOf anchor bearing a start state, " +
      "each carrying gmeow:atTime and the single shared gmeow:eventTemporalFrame the " +
      "shipped transaction auditor requires — plus the second step's conclusion annotated " +
      "as a quoted triple naming the step it came out of.",
  },
  {
    id: "entailment",
    title: "A closure over annotated premises",
    pane: "reason_graph",
    args: { data: ENTAILMENT, format: "turtle" },
    prose:
      "One subsumption and one typed individual — with the premise itself recorded as an " +
      "RDF-1.2 quoted triple — run through the native structured-DL chase. This is a " +
      "reasoning-segment tool, so the first run demand-loads the reasoner.",
  },
  {
    id: "star-loss",
    title: "Where a quoted triple cannot go",
    pane: "convert",
    args: { data: ANNOTATED_CALL, from: "turtle", to: "rdfxml" },
    prose:
      "The same annotated record transcoded to RDF/XML, which has no triple-term syntax. " +
      "The realized loss ledger names the drop (rdf12-star-unrepresentable) and counts it, " +
      "rather than returning a smaller graph that merely looks complete.",
  },
  {
    id: "query-annotations",
    title: "Querying the annotations back out",
    pane: "query_local",
    args: {
      data: ANNOTATED_CALL,
      format: "turtle",
      scope: "input",
      query: `PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
SELECT ?statement ?call ?antecedent WHERE {
  ?statement rdf:reifies ?quoted ;
             gmeow:derivedBy ?call ;
             gmeow:wasDerivedFrom ?antecedent .
}`,
    },
    prose:
      "The provenance chain, recovered by following one edge back: every annotated " +
      "statement, the call it came out of, and each antecedent it was derived from.",
  },
  {
    id: "gmn-annotated",
    title: "An annotated record over the token channel",
    pane: "encode_gmn1",
    args: { data: ANNOTATED_CALL, format: "turtle" },
    prose:
      "The annotated call record encoded into the token-compact GMN-1 surface. Reference " +
      "positions resolve through the shipped lang: codebook, so a codebook-covered record " +
      "— quoted triples included — reads back to the identical RDF.",
  },
];

/** A vignette by id, or `null`. */
export function vignette(id) {
  return VIGNETTES.find((v) => v.id === id) ?? null;
}

/** The panes the gallery exercises, sorted — the coverage claim, computed not asserted. */
export function galleryPanes() {
  return [...new Set(VIGNETTES.map((v) => v.pane))].sort();
}
