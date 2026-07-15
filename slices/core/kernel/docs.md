<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Kernel — the logic-grounded core and the universal axes

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/kernel` · **tier: core**
> The foundation every slice stands on: the sortal spine, the part/whole and link spines, and the domain-free facet axes.

Every GMEOW class is grounded through the canonical `logic:` foundation, and the kernel
supplies the top of that spine plus everything that must exist *before* any domain slice can be honest:
universal mereology and connectivity, the domain-free epistemic facets (Principle 9), the
withhold-or-coarsen disclosure mechanism (Principle 10), and the compliance-by-construction
annotations that turn Constitution principles into generated lints. Kernel properties omit
domains and ranges where breadth is the point; heavy computation — reachability,
geomasking, k-anonymity — is solver work (Principle 12).

## The sortal spine and the universal relations

### gmeow:Entity · gmeow:Agent · gmeow:InformationObject

`Entity` is the universe of discourse: anything persisting in time that can bear
properties (`logic:Endurant`) — what makes the domain-free facets below attachable
anywhere. `Agent` acts, bears responsibility, and enters agreements. `InformationObject`
carries content — the shared parent of the document, software, account, and tag layers.

### gmeow:PhysicalObject · gmeow:SocialObject

The material sortal (disjoint with `Agent` and `InformationObject`) and the
convention-sustained one (myths, agreements, standpoints). `SocialObject` is deliberately
*not* disjoint with `InformationObject`: a myth is both, and asserting false disjointness
would turn benign overlap into inconsistency (Principle 9).

### gmeow:partOf · gmeow:hasPart · gmeow:connectsTo

The universal transitive part/whole spine and the traversable-link spine (connectivity spine).
Domain properties (geographic containment, sub-organizations, spatial links, kinship,
citation edges) keep their exact meanings and specialize these so generic consumers can
ask for parthood or connectivity without collapsing semantics. No domain/range is
asserted; `connectsTo` is neither symmetric nor transitive, and reachability is solver
work (P12). Unit linkage (`gmeow:unit`, QUDT by reference — Principle 5) is defined in the
observations slice, grounding onto the `logic:` measurement seam.

## The domain-free epistemic axes

Four orthogonal, domain-free, non-functional facets may attach to any value,
entity, claim, or carrier. None subsumes another; none bridges to confidence
or standpoint modality (Principle 9). Together with those two statement-layer
properties they form the six-way matrix every projection consults:

| Axis | Property | Question it answers | Kind |
|---|---|---|---|
| Granularity | `gmeow:hasGranularity` | At what resolution is this stated? | resolution |
| Determinacy | `gmeow:hasDeterminacy` | How inherently defined is the value itself? | ontic |
| Sensitivity | `gmeow:hasSensitivity` | What disclosure risk does it carry? | privacy |
| **Aboutness** | `gmeow:hasAboutness` | Does the carrier *describe* its subject or *enact* it? | rhetorical |
| (Confidence) | `gmeow:confidence` | How sure is the asserter? | epistemic |
| (Standpoint modality) | `gmeow:standpointModality` | What belief value does the frame assign? | doxastic |

**Aboutness** is the mention/use distinction made first-class: a chunk
*defining* a trust framework describes trust; a covenant *demanding* trust
enacts it. Text about deception is not text that deceives. A carrier may
describe one subject while enacting another, and vantages may disagree via
the statement layer. Fiction is the licensed case where enactment co-occurs
with non-assertion — see the deception module's
`veridicalityLicensedFalsehood` (documented bridge; deliberately no axiom
coupling, so enactment never entails assertion).

External alignment is near-empty by survey, not by omission (search trail
from the parked `wip-aboutness-349` branch, whose mapping set lands with the
compiler-arc work): the one settled Wikidata anchor is **Q2577553**
(*use–mention distinction*, analytic philosophy) — a loose `relatedMatch` for
the class, since the QID names the distinction, not a mode vocabulary. IAO's
*is about* (IAO_0000136) is aboutness-as-reference (what a carrier is about),
not aboutness-as-mode (what it does with it) — refused. schema.org, PROV-O,
CIDOC-CRM, DOLCE+DnS, and Web Annotation carry no mention/use mode property;
the seed individuals have no settled QIDs and stay unaligned rather than
force weak matches (Principle 5).

### gmeow:hasGranularity · gmeow:hasDeterminacy

The "no silent precision" axis and the ontic axis. `gmeow:GranularityLevel` values are an
open, *ordered* vocabulary of individuals (never per-level subclasses), ordered by the
transitive `gmeow:coarserThan` and `skos:exactMatch`-aligned to OWL-Time and ISO 19112.
`gmeow:Determinacy` records whether a value is inherently crisp, vague, fuzzy,
probabilistic, or disputed — held strictly apart from epistemic confidence. Both are NOT
functional: in a merge competing claims coexist (Principle 9); fuzzy math is solver (P12).

### gmeow:hasSensitivity · gmeow:hasAboutness

The privacy axis — an open, ordered `gmeow:SensitivityLevel` vocabulary (public ≺
internal ≺ confidential ≺ restricted ≺ sensitive personal) that drives disclosure control
under a consent guard — and the rhetorical axis (`gmeow:AboutnessMode`: describes /
enacts). `hasAboutness` is uniquely an `owl:AnnotationProperty`: aboutness is routinely
asserted *about statements*, and the annotation form keeps the generated OWL downcast in
OWL 2 DL (Principle 3); the subject the mode holds toward is carried by the surrounding
construct, never inferred.

## Disclosure control by projection (Principle 10)

### gmeow:coarsenTo · gmeow:generalizesVia

Suppression (`gmeow:displayable false` → withhold) and generalization (`coarsenTo` → emit
a coarser ancestor, e.g. the enclosing city instead of exact coordinates) are one
mechanism: withhold *or* coarsen at projection time, never by deletion. Coarsening walks
the property named by `generalizesVia` (default `gmeow:partOf`) up to the target
granularity level; when both marks apply, withhold wins. Geomask math is solver-side (P12).

### gmeow:eligibleForConsumer · gmeow:hasDisclosurePolicy

The *who* and *what* layers of disclosure control: the `gmeow:ProjectionContext`
targets a fact may reach (internal archive, agent memory, Wikidata, public site, …) and
its `gmeow:DisclosurePolicy` release posture. `policyPublicOnlyWithIndependentSource`
resolves against the evidence module's `sourceIndependence` in the solver layer (P12).
Both are domain-free and non-functional: competing claims coexist (Principle 9).

### gmeow:coequalFacet · gmeow:requiresFrame · gmeow:coarsenGuarded

Compliance by construction: annotations with no logical semantics that *declare
which invariants the toolchain must generate*. `coequalFacet` puts a property under the
Principle 9 orthogonality lint (own range, no bridges between axes, never functional,
jointly disjoint ranges). `requiresFrame` generates the Principle 11 frame-relativity
SHACL shape, tunable via `ruleSeverity` (binding vs advisory) and `frameCardinality`.
`coarsenGuarded` marks precision-bearing properties so the compiler injects coarsen
guards into every generated projection (precision guard); `gmeow:namingNote` records the
lint-visible justification for a legitimately primary-style value-vocabulary name.

## Mental moments and the proficiency value vocab

### gmeow:MentalMoment

The shared umbrella for an agent's intrinsic mental states — `gmeow:MentalMoment ⊑
gufo:IntrinsicMode`. A NAMED `gufo:Category` so a consumer can query *every* mental
moment of an agent uniformly (the agent-memory flagship, Principle 15) rather than
walking three unrelated branches. Its members live in their domain slices: cognition's
`gmeow:CognitiveState` (knowing), epistemics' doxastic states (believing — planned),
and teleology's `gmeow:IntentionalMode` (desiring/intending). Never instantiated
directly.

### gmeow:ProficiencyScale · gmeow:ProficiencyLevel · gmeow:ProficiencyModality

The domain-neutral value vocabulary for rating proficiency — relocated here from the
`expertise` slice to break a latent `expertise ↔ cognition` dependency cycle
(Principle 6/16): `expertise`, `languages`, and `cognition` all reuse these classes, so
they belong in the kernel every consumer already depends on. The framework individuals
(CEFR, Dreyfus, NIH, Bloom's, SOLO, …) stay in their domain slices and reference these
classes by IRI; only the class home moved.

## Documentation doctrine

### gmeow:pairsWith · gmeow:useWhen · gmeow:howToUse · gmeow:guideBlob

Docs ship *with* the ontology in three tiers (Principle 4): term docs are canonical in
the graph; narrative guides are canonical markdown whose `### gmeow:Term` anchors resolve
against the graph or the build fails, riding the GTS package as content-addressed blobs
linked by `guideBlob` in an external documentation projection (trustworthy from that
projection's hash chain — Principle 7); the logical GTS bundle excludes both guide
payloads and their references. `pairsWith`
makes the flat-first/reify-on-demand pairing machine-usable: flat shortcut → reified
relator (`gmeow:hasTag` ↔ `gmeow:Tagging`), rendered by `gmeow describe` from structure,
never a logical bridge (Principle 12). Documentation literals are CommonMark typed
`gmeow:markdown`; HTML/SGML subsets are rejected.

Advisory term metadata splits generic scope prose into machine-readable WHEN/HOW/WHERE
facets. `gmeow:useWhen` and `gmeow:avoidWhen` are narrower `skos:scopeNote`s;
`gmeow:howToUse` carries the short modeling recipe; `gmeow:useForConsumer` and
`gmeow:avoidForConsumer` point to declared `gmeow:ProjectionContext` individuals so
`describe`, generated docs, and projection tooling can say which consumers a construct is
meant for without turning that advice into logical entailment.

The kernel depends on nothing and is depended on by every slice; whatever computation it
names is solver work (Principle 12) — the kernel models the axes, tooling evaluates them.
