# Issue #1812: [term] Observations: encounters, witnessing in a simulated environment, the attention-request lifecycle (consumer: memetics)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement

## Body

## Summary

Three observation patterns a companion agent records have no vocabulary:
- meeting someone, as an observation;
- witnessing something at a place and a state of a simulated
  environment, and how much of an outside answer was redacted;
- the lifecycle of an attention request: requested, acknowledged,
  resolved.

Checked against `main` at `a631f73` (2026-10-10).

## Consumer

**memetics**, the memory adapter for a companion agent. Her observations
store holds structured facts about what she witnessed, asked and was
told. It is kept apart from her own reflections, so a fact never passes
for a feeling.

## 1. `Encounter`: an observation that someone was met

**Today.** Meeting someone is expressible as a `gmeow:Event` with
participants. But there is no observation kind for "she met this
person": the thing recall asks for ("when did I first meet them?").

**Proposed terms.**
- `gmeow:Encounter`, a sub-kind of `gmeow:Observation`: "An observation
  that the observer met someone. The meeting is a `gmeow:Event` (the
  observed feature), with who was there, when and where."
- `gmeow:encounteredIn`: the conversation or chat in which it happened.
  This is better as an object property to the message or conversation
  node than the consumer's interim verbatim identifier.

**Open question.** Is an encounter better as an `EventType` on the event
itself, with the observation generic? The consumer wants a cheap query
for "first encounter with X"; either shape serves.

## 2. Witnessing in a simulated environment; redaction counts

**Today.** No vocabulary says "witnessed at this place, at this version
of a simulated environment's state". Nothing records how many items an
outside model's answer had redacted.

**Use case.**
- The agent lives in a simulated environment with named places and a
  versioned state, where each change advances the state's head.
- What she witnesses is pinned to a place and a head, so it can be
  replayed or checked.
- When she consults an outside model, the answer can arrive with items
  redacted, and how many is part of what she was told.

**Proposed terms.**
- `gmeow:witnessedLocus`: the place within the environment. An object
  property to a place node is better than the consumer's interim
  verbatim token.
- `gmeow:witnessedAtHead`: the environment state version witnessed. An
  identifier (`xsd:token`), or a digest per #1807 item 2.
- `gmeow:externalRedactionCount`: domain `gmeow:ModelInvocation`, range
  `xsd:nonNegativeInteger`.

**Open question.** Does GMEOW have, or want, a general "simulated
environment state" entity that the head belongs to? Then
`witnessedAtHead` becomes a link rather than a string.

## 3. The attention-request lifecycle

**Today.** Attention exists as a faculty (`attendsTo`,
`processAttention`), but there is no request lifecycle.

**Use case.** A dream can ask her waking self to pay attention to
something. The request is acknowledged, then resolved (addressed,
deferred or dismissed) with a rationale and evidence.

**Proposed terms.**
- `gmeow:WakingAttention`, a kind: an attention request with its
  message, the record that raised it (here, a dream report), and when.
  - A more general name may be better: `gmeow:AttentionRequest`, with
    the waking case as one use.
- `gmeow:AttentionAcknowledgement`, an event, plus `gmeow:acknowledges`.
- `gmeow:AttentionResolution`, an event, plus `gmeow:resolves`.
- `gmeow:ResolutionOutcome`, a value vocabulary (`resolutionAddressed`,
  `resolutionDeferred`, `resolutionDismissed`), plus
  `gmeow:resolutionOutcome`.

**Open question.** Is this a special case of a general request or ticket
lifecycle GMEOW already has, or intends to have (task states,
`logic:EvaluationStatus`)? If so, the consumer will map onto it.

## Local stand-ins

memetics' local observations and selfhood slices mint all of the above,
in GMEOW's namespace and layout. Upstreaming is a move; renames (such as
`AttentionRequest`) are welcome.


## Comments (2)

### paudley — 2026-10-10T20:21:10Z

An update to item 2, from the consumer's own modelling decision: **places in an agent's simulated world are `gmeow:Location`s**, imaginary ones included, as GMEOW's own dream example types an imagined place as a `gmeow:Place`.

So `gmeow:witnessedLocus` is no longer a verbatim token. It is an object property with range `gmeow:Location`, read as a specialisation of `gmeow:locatedAt` for the act of witnessing. A statement in the source vocabulary that something "is located in" a place of that world (for example, an expected statement in a goal verifier) now reads `gmeow:locatedAt`, and the place is a `gmeow:Location`.

Two questions:
- Is `witnessedLocus` better as `rdfs:subPropertyOf gmeow:locatedAt` with the observation as subject, or as `gmeow:eventLocation` on the witnessing event?
- Should a simulated world's places carry a frame (`gmeow:hasReferenceFrame`, perhaps a narrative or virtual frame) to keep them apart from geographic places in queries?


### paudley — 2026-10-11T03:03:48Z

One further gap, found while migrating memetics' observation records (verified against upstream `a631f73`):

**Keeping what was asked of a model, and what it answered, in their own words.** memetics migrates one consultation of an outside model: a question, the model alias asked for, the model the provider resolved it to, the answer, three cited sources and a redaction count. The `ai` slice almost holds it, with three gaps:

1. **A prompt's text.** `gmeow:Prompt` is content-addressed (`gmeow:contentDigest`) and role-tagged, but has no property for its words. When the asking is worth keeping as written (a question a person put to a model), there's nowhere to put it. Proposed: `gmeow:promptText` (domain `gmeow:Prompt`, range `rdf:langString`), or a documented pattern pairing the prompt with a content object.
2. **The model name asked for.** `gmeow:usedModel` names the model actually called. A provider alias such as a "latest" tag resolves to a concrete model, and the alias requested is provenance too. Proposed: `gmeow:modelAlias` (domain `gmeow:ModelInvocation`, range `xsd:token`), or a `gmeow:requestedModel` object property to a model agent.
3. **An answer whose words are not a triple.** The natural modelling is the answer as a claim according to the model, but free-text answers are not triples. memetics records the answer as an observation generated by the invocation, with its words under a local `gmeow:answerText`. Open question: should GMEOW give generated text a home of its own (a content object, or a `gmeow:Observation` result literal), so "what the model said" stays apart from any claim derived from it?

- Consumer: memetics. Local stand-ins: `gmeow:promptText`, `gmeow:modelAlias`, `gmeow:answerText`.


