<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — The Crosswalk

> The **spine** of the design set. Every term from the three subsumed sources is reconciled here
> exactly once, with a disposition and the canonical GMEOW term it maps to. The sibling specs
> ([`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md),
> [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md),
> [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md),
> [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md)) reference this table rather than
> restating a term's status; when they disagree with it, this table is authoritative and they are
> the defect. The canonical stereotypes are the [Net new vocabulary](#net-new-vocabulary) table at the
> end; the source-by-source rows below give each term's disposition and rationale.

## The four dispositions

Every source term receives exactly one disposition. The discipline is Principle 5 (one canonical
term per concept) and Principle 6 (greenfield — mint the right thing, not the convenient thing):

| Disposition | Meaning | Constitutional basis |
|---|---|---|
| **REUSE** | an existing `gmeow:` term already names this concept; use it unchanged | Principle 5 — never mint a second term for a concept GMEOW already holds |
| **EXTEND** | an existing term gains a property, a value, or a documented usage — but no new class | Principle 6 — grow the existing surface before adding to it |
| **MINT** | genuinely absent; a new term with its `logic:` stereotype | only what reuse cannot cover |
| **BRIDGE** | an external (source) term; aligned by reference (SSSOM / EDOAL), never copied in | Principle 5 — cover by reference, never by rewriting |

The headline arithmetic: of the roughly twenty-five distinct concepts across the three sources,
**five MINT** (`DigitalSubject`, `Inhabitation`, `Embodiment`, `Portal`, `inhabitationLocus`), with
one further conditional MINT (`AgentSession`) and one optional promotion (`MemoryView`). Everything
else is REUSE, EXTEND, or BRIDGE. The naïve reading of the verdict alone would have minted eleven.

## Source A — Cagle, "A Vocabulary for Inhabited Systems"

| Cagle term | Cagle's meaning | Disposition | GMEOW resolution |
|---|---|---|---|
| **Actor** | the durable, motivating entity behind an Avatar (human, persona, agent, or collective) | REUSE + MINT | A *human* Actor is `gmeow:Person`; a *collective* Actor is `gmeow:Organization` / `gmeow:Group`; a *digital* Actor is an `Agent` playing the minted `gmeow:DigitalSubject` role. **Clash:** Cagle's "Actor" is *not* `gmeow:Agent` — `gmeow:Agent` is the union that *acts*; an Actor is the durable identity-bearer. BRIDGE `cagle:Actor` → `gmeow:Agent ⊔ DigitalSubject` (relatedMatch, lossy). |
| **Avatar** | the representation/interface an Actor expresses through, in a shared environment | MINT | the `Embodiment` split (`EmbodimentCarrierRole` + `EmbodimentAssignment`, see the verdict rows) — broader than Avatar (API identity, terminal, robot, voice, channel). Cagle's Avatar is the visual subset: BRIDGE `cagle:Avatar` → the carrier (narrowMatch). See [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md) for the *avatāra* etymology. |
| **Persona** | an Agent *with* persistent memory, stable identity, anthropomorphic characteristics | MINT (as the role) + BRIDGE | The durable AI subject is the `gmeow:DigitalSubject` role. **Critical clash:** GMEOW already owns `gmeow:Persona` (norms slice) as an *expression-policy relator* (bearer × register × expressed-norms × activation). KEEP it unchanged. Cagle's "Persona" maps to `DigitalSubject`, **never** to `gmeow:Persona`. BRIDGE `cagle:Persona` → `gmeow:DigitalSubject`; record the homonymy loudly. |
| **Agent** | an ephemeral process orchestrating services; no persistent identity/memory beyond task scope | REUSE | `gmeow:SoftwareAgent`, scoped to a `RuntimeExecution` / `AgentSession`, bearing no `DigitalSubject` continuity. **Clash:** Cagle's "Agent" is the *ephemeral* process; `gmeow:Agent` is the *agentive union* (Person/Organization/SoftwareAgent). |
| **Role** | the interface specification between an Actor and the holon it inhabits; permitted actions, info access, presentation norms | REUSE | `logic:Role` / `gmeow:Role` (organization slice — relationally-acquired function-in-context). Info-access gating is carried by the `Inhabitation` relator that names the role-filler; presentation norms route to `gmeow:Persona` / `gmeow:Register`. |
| **Collective** | an Actor itself composed of multiple Actors, coordinating toward a shared purpose | REUSE | `gmeow:Organization` (structured) or `gmeow:Group` (informal) + a shared `gmeow:Goal` (teleology). This is exactly Taylor's "Organization as a Party derivative" — see Source B. A Collective that bears a `DigitalSubject` role is the egregore case (see [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md)). |
| **holon** | a whole-that-is-also-a-part; an inhabited system is a holon of Actors-in-Roles projecting Avatars | REUSE (foundation) + DEFER (domain) | The foundation already supplies the holon kernel in `logic:` (issue #704): `logic:Holarchy`, `logic:HolonicPosition` (the five-place entity × holarchy × context × interval × path relation), and `logic:Holon` (its lossy unary projection). A *domain* `gmeow:Holon` Kind is **deferred** (it would overtype what `logic:HolonicPosition` correctly models relationally). The inhabited-system reading is `gmeow:InhabitedSystem` aligned to `logic:HolonicPosition` (host = `positionEntity`). See [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md#the-holon-deferred-at-the-domain-layer-supplied-at-the-foundation-layer). |
| **portal** | a Role transition between holons | MINT | A migration as a lifecycle event: `eventTypeInhabitationTransition` + `portalFrom`/`portalTo` (the lifecycle value pattern, not an Event subclass), closing one tenure and opening the next. See [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md#transitions-migration-as-a-lifecycle-event). |
| **scene graph** | the runtime view mapping Avatars + Roles, not underlying Actors | BRIDGE (solver-side) | Not asserted data. The scene graph is a **computed projection** (Principle 12) over the `Embodiment` and `Role` fillers of the currently-active `Inhabitation`s — a view, never triples. |

## Source B — the organizational-modeling email thread

The thread's thesis — *"Role is heavily abused; we don't use it except informally"* (Beale) — is
**vindicated, not implemented**: GMEOW already factors the overload. Every row here is REUSE; the
contribution is the documentation that the overload is already solved.

| Thread term | The thread's meaning | Disposition | GMEOW resolution |
|---|---|---|---|
| **Role-as-capability** | "someone who can work as an MD due to training" (Beale) | REUSE | `gmeow:Occupation` (the profession, ESCO/SOC-aligned) + `gmeow:Skill` / `gmeow:SkillProficiency` / `gmeow:Credential` (expertise slice). The "stand-in for a set of capabilities, behavior, knowledge" (Hunter) is the capability holon: a bundle of proficiencies and credentials. |
| **Post / Position** | "positions (aka post) in an organisation" (Beale) | REUSE | `gmeow:Post` (organization slice) — the holder-independent seat; `postIn` an Organization, filled via `Membership.fillsPost`; vacancy and succession are queries. An exact match for the thread's distinction. |
| **Function-in-a-process** | "the function that an individual plays in a process" (Beale); "Person as Role performs Function" (Hunter) | REUSE | `gmeow:Role` borne by a `gmeow:Membership` relator, with the function-in-the-activity carried by `logic:Participation` / the teleology action layer. "Person as Role in context performs Function" = `Person` + `Membership.hasRole Role` + an `ActionSchema` performed within an `Activity`. |
| **Accountability** | "We call 'Agreement' Accountability" (Beale) | REUSE | `gmeow:Commitment` (teleology — the social relator binding a committed agent to a beneficiary) `gmeow:foundedOn` a `gmeow:Agreement` (agreements slice). The thread's "Accountability" is a founded Commitment. |
| **Organization as a Party derivative** | "model Organization as a Party derivative … deferring detail to Roles and Relationships" (Taylor) | REUSE | `gmeow:Organization ⊑ gmeow:Agent`, with all detail deferred to `Membership` / `Role` / `Post` relators — exactly Taylor's pattern. BRIDGE the Party-model lineage (Taylor's cybernetic Party derivative) by reference in [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md). |
| **Org_unit** | Beale's distinguishing element (organizational sub-unit) | REUSE | `gmeow:Organization` + `gmeow:subOrganizationOf` (the part/whole spine). No new term. |
| **"Role is a kind of Holon"** | Hunter: a role decomposes into skills/knowledge/behavior enabling a business capability | REUSE | The capability decomposition is the expertise bundle (above); the holon framing is the foundation's `logic:HolonicPosition` / `logic:Holon` kernel (#704), with the deferred *domain* `gmeow:Holon` (Source A). The two readings meet at: a Role is filled by an Agent whose `SkillProficiency` bundle enables the function — modeled, not collapsed. |

## Source C — the inhabited-systems analysis verdict

The verdict proposed eleven new terms. The crosswalk mints five (plus one conditional, one optional)
and reuses the rest — the de-conflation is *reify roles/relators/tenures over `SoftwareAgent`*, not
mint Kinds (see [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md)).

| Verdict term | Verdict's meaning | Disposition | GMEOW resolution |
|---|---|---|---|
| **DigitalSubject** | the enduring digital "who" that may span runtimes | MINT | `gmeow:DigitalSubject` — an **anti-rigid `logic:RoleMixin`** (it spans Person/SoftwareAgent) borne over a `DigitalSubjectTenure`; NOT a rigid Kind and NOT a single-Kind `Role`. Self-assertion supports the status, never entails it. See [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md#the-durable-subject-a-rolemixin-with-a-tenure). |
| **Inhabitant** | the agent while occupying/operating a system | MINT | `gmeow:Inhabitant` — a `logic:RoleMixin` (spans Kinds), grounded `⊑ gmeow:Agent`; the tenure classifies its filler into the role. |
| **Inhabitation** | the reified subject-host occupation over time | MINT | `gmeow:InhabitationTenure` — a `logic:Situation ⊑ gmeow:TimeScopedRelation` (NOT a relator subclassing a situation). The core construct, with `gmeow:InhabitationConfiguration` for the time-scoped facets. |
| **HostSystem / RuntimeEnvironment** | the environment capable of being inhabited | REUSE | `gmeow:PhysicalObject` / `gmeow:SoftwareAgent` located via places, composed via `gmeow:partOf`. **No new class** — "hosted on" is containment. (A thin `gmeow:RuntimeEnvironment` subkind may be minted in the AI profile only if it earns its keep — see [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md).) |
| **ModelArtifact** | the model weights / architecture / version | REUSE | `gmeow:SoftwareProduct` / `gmeow:Distribution` (software slice five-facet) + `gmeow:ModelCard` (ai slice). Mint a thin `gmeow:ModelArtifact` subkind only if model-specific facets demand it (Principle 6). |
| **ModelDeployment** | a served, callable realization of an artifact | MINT *(model-serving)* | `gmeow:ModelDeployment` — a `logic:Relator` binding artifact × service `SoftwareAgent` × host × endpoint × interval. Awareness mode is a *facet* (a tenure on the service agent), not the deployment itself. |
| **RuntimeExecution** | a particular running process | MINT *(model-serving)* | `gmeow:RuntimeExecution` — a `logic:Event ⊑ gmeow:Activity` (an occurrent), within which `ModelInvocation`s occur. Its own identity, not an awareness tenure. |
| **AgentSession** | a bounded interaction context | MINT *(model-serving)* | `gmeow:AgentSession` — a `logic:Event ⊑ gmeow:Activity` (an **event aggregate** via `subEventOf`), not a relator subclassing a situation. Ordering via `logic:Path` stays in the solver (Principle 12). |
| **AgentEpisode** | a goal-oriented sequence within a session | REUSE | a sub-aggregate (`eventTypeAgentEpisode`, `subEventOf` the session). Mint a class only if episodes need their own identity. |
| **Embodiment** | the interface/avatar/device/account/channel used | MINT *(core)* | **split:** `gmeow:EmbodimentCarrierRole` (`logic:RoleMixin` — the surface entity in role) + `gmeow:EmbodimentAssignment` (`logic:Situation ⊑ TimeScopedRelation` — subject × carrier × interval × capabilities, suppressible). Subsumes Cagle's Avatar. |
| **CallableCapability** | a general bearer for tool use (passive or active) | REUSE | **Do not mint.** The agentic slice already *explicitly refused* a `Tool` subclass: tool-ness is a role in the `ToolCall` event ("the Persona lesson"). A passive capability is a `gmeow:ActionSchema` (teleology); a delegated one is a `gmeow:ToolCall` to a `SoftwareAgent`. Do **not** widen `usedTool` — its range (`SoftwareAgent`) is already the most general acting thing. |
| **inhabitationLocus** *(not in the verdict; from the esoteric generalization)* | self vs external vessel, and shared vs exclusive | MINT | **split into two orthogonal axes:** `gmeow:inhabitationLocusKind` (value vocab `locusSelf` / `locusVessel` — invocation vs evocation) and **derived** tenancy cardinality (shared = overlapping tenures over one host, computed not asserted). See [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md). |
| **MemoryView** *(implied by the "which memory view was active?" CQ)* | the memory scope active during an inhabitation | DERIVE (+ optional MINT) | Computed over `gmeow:MemoryItem` provenance + the active `Inhabitation` interval (Principle 12). Promote to a first-class `gmeow:MemoryView` object **only** when a view must be signed/attested for the GTS `ai-package`. |

## The clash register

The crosswalk surfaces four genuine homonyms or near-collisions. Each is resolved here so no sibling
doc re-litigates it:

1. **Persona.** Cagle's "Persona" (durable subject) ≠ `gmeow:Persona` (expression-policy relator,
   norms slice). Resolution: `gmeow:Persona` is untouched; Cagle's notion maps to `DigitalSubject`
   for the durable-subject sense and **folds into** the existing `gmeow:IdentityFacet` / `NameUsage`
   family for the presented-mask sense. **No new "Persona" term is minted** — homonymy in the
   canonical namespace is itself a Principle 5 violation.
2. **Agent.** Cagle's ephemeral "Agent" ≠ `gmeow:Agent` (the agentive union). Resolution: Cagle's
   Agent is a session-scoped `gmeow:SoftwareAgent`.
3. **Actor.** Cagle's "Actor" (durable identity-bearer) ≠ `gmeow:Agent` (actor-that-acts).
   Resolution: an Actor is a `Person`/`Organization`/`Group`, or an `Agent` bearing the
   `DigitalSubject` role.
4. **Role.** The email thread's overloaded "Role" is split across `gmeow:Role` ⟂ `gmeow:Post` ⟂
   `gmeow:Occupation` ⟂ `gmeow:Membership` ⟂ `gmeow:Commitment` — already, in the organization and
   expertise slices. Cagle's "Role" (interface spec) is `gmeow:Role` plus the access-gating carried
   by `Inhabitation`.

## Net new vocabulary

The canonical stereotype inventory. The discipline behind it: a relator may not subclass
`TimeScopedRelation` (a `logic:Situation`); `DigitalSubject` spans Kinds (so `RoleMixin`, not the
single-Kind `Role`); and an `Observation` subclass is a `logic:SubKind`, not a situation. Terms whose
identity criteria differ are kept separate rather than collapsed.

All terms live in **core** unless tagged *(model-serving)*, the thin standalone extension. The single
`agent-runtime` profile mints nothing.

| New term | `logic:` stereotype | One-line role |
|---|---|---|
| `gmeow:DigitalSubject` | `logic:RoleMixin` (⊑ Agent) | the durable-subject status an agent bears (spans Person/SoftwareAgent) |
| `gmeow:DigitalSubjectTenure` | `logic:Situation ⊑ TimeScopedRelation` | when/according-to-whom the status is borne |
| `gmeow:InhabitationTenure` | `logic:Situation ⊑ TimeScopedRelation` | S inhabited H over T |
| `gmeow:InhabitationConfiguration` | `logic:Situation ⊑ TimeScopedRelation` | the time-scoped active facets (active-at-T) |
| `gmeow:InhabitationClaim` | `logic:SubKind ⊑ StandpointClaim` | the contested, *unasserted* inhabitation (neutrality) |
| `gmeow:InhabitationDescription` | `logic:SubKind ⊑ Proposition` | the quoted, range-open configuration a claim observes |
| `gmeow:Inhabitant` / `gmeow:InhabitedSystem` | `logic:RoleMixin` | agent-side / host-side role fillers |
| `gmeow:EmbodimentCarrierRole` | `logic:RoleMixin` | the surface entity in role (subsumes Avatar) |
| `gmeow:EmbodimentAssignment` | `logic:Situation ⊑ TimeScopedRelation` | subject × carrier × interval × capabilities |
| `gmeow:SubjectStage` / `gmeow:SubjectLineage` | `logic:Situation ⊑ TimeScopedRelation` / `logic:Kind ⊑ InformationObject` | epochs and the durable identity record |
| `gmeow:IdentityContinuityAssessment` | `logic:SubKind ⊑ Observation` | the contestable same/different/indeterminate verdict |
| `gmeow:ControlAssessment` | `logic:SubKind ⊑ Observation` | who controls a host/embodiment (≠ deception) |
| `gmeow:inhabitationLocusKind` | `logic:AbstractIndividualType` (values) | self / vessel (tenancy is *derived*, not a value) |
| `eventTypeInhabitationTransition` + `portalFrom`/`portalTo` | `gmeow:EventType` value | migration (lifecycle value, not an Event subclass) |
| `gmeow:TransferManifest` | `logic:Kind ⊑ InformationObject` | what crossed a transition (evidence, not coincidence) |
| `gmeow:ModelArtifact` / `gmeow:ModelDeployment` / `gmeow:RuntimeExecution` / `gmeow:AgentSession` *(model-serving)* | `Kind` / `Relator` / `Event ⊑ Activity` / `Event ⊑ Activity` | the standalone model-serving identities |
| `gmeow:MemoryView` *(optional)* | situation | promoted only when a view must be signed |

Most are thin specializations of `Observation`, `Activity`, or `TimeScopedRelation` — the idiomatic
GMEOW pattern. Terms whose identity criteria differ are kept separate rather than collapsed into a
single overloaded node. Plus per-branch role-filler properties — each a property, never a class, and
never `gufo:inheresIn`.
