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
> the defect.

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
| **Avatar** | the representation/interface an Actor expresses through, in a shared environment | MINT | `gmeow:Embodiment` — broader than Avatar (it covers API identity, terminal, robot, voice, channel, not only the human-facing visual surface). Cagle's Avatar is the visual subset: BRIDGE `cagle:Avatar` → `gmeow:Embodiment` (narrowMatch). See [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md) for the *avatāra* etymology. |
| **Persona** | an Agent *with* persistent memory, stable identity, anthropomorphic characteristics | MINT (as the role) + BRIDGE | The durable AI subject is the `gmeow:DigitalSubject` role. **Critical clash:** GMEOW already owns `gmeow:Persona` (norms slice) as an *expression-policy relator* (bearer × register × expressed-norms × activation). KEEP it unchanged. Cagle's "Persona" maps to `DigitalSubject`, **never** to `gmeow:Persona`. BRIDGE `cagle:Persona` → `gmeow:DigitalSubject`; record the homonymy loudly. |
| **Agent** | an ephemeral process orchestrating services; no persistent identity/memory beyond task scope | REUSE | `gmeow:SoftwareAgent`, scoped to a `RuntimeExecution` / `AgentSession`, bearing no `DigitalSubject` continuity. **Clash:** Cagle's "Agent" is the *ephemeral* process; `gmeow:Agent` is the *agentive union* (Person/Organization/SoftwareAgent). |
| **Role** | the interface specification between an Actor and the holon it inhabits; permitted actions, info access, presentation norms | REUSE | `logic:Role` / `gmeow:Role` (organization slice — relationally-acquired function-in-context). Info-access gating is carried by the `Inhabitation` relator that names the role-filler; presentation norms route to `gmeow:Persona` / `gmeow:Register`. |
| **Collective** | an Actor itself composed of multiple Actors, coordinating toward a shared purpose | REUSE | `gmeow:Organization` (structured) or `gmeow:Group` (informal) + a shared `gmeow:Goal` (teleology). This is exactly Taylor's "Organization as a Party derivative" — see Source B. A Collective that bears a `DigitalSubject` role is the egregore case (see [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md)). |
| **holon** | a whole-that-is-also-a-part; an inhabited system is a holon of Actors-in-Roles projecting Avatars | DEFER (MINT later) | First-class `gmeow:Holon` is **deferred** — it cross-cuts organization, connectivity, and procedures, and is expressible today as anything simultaneously `gmeow:partOf` and `gmeow:hasPart`. Interim: the inhabited-system reading is the `gmeow:InhabitedSystem` role + `logic:HolonicPosition` by reference. See [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md#the-deferred-holon). |
| **portal** | a Role transition between holons | MINT | `gmeow:Portal` (`logic:Event`) reifying the transition, paired with the supersession chain that closes one `Inhabitation` and opens the next. See [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md#transitions-the-portal). |
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
| **"Role is a kind of Holon"** | Hunter: a role decomposes into skills/knowledge/behavior enabling a business capability | REUSE + DEFER | The capability decomposition is the expertise bundle (above); the holon framing is the deferred `gmeow:Holon` (Source A). The two readings meet at: a Role is filled by an Agent whose `SkillProficiency` bundle enables the function — modeled, not collapsed. |

## Source C — the inhabited-systems analysis verdict

The verdict proposed eleven new terms. The crosswalk mints five (plus one conditional, one optional)
and reuses the rest — the de-conflation is *reify roles/relators/tenures over `SoftwareAgent`*, not
mint Kinds (see [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md)).

| Verdict term | Verdict's meaning | Disposition | GMEOW resolution |
|---|---|---|---|
| **DigitalSubject** | the enduring digital "who" that may span runtimes | MINT | `gmeow:DigitalSubject` — an **anti-rigid `logic:Role`** an `Agent` plays on self-assertion (NOT a rigid Kind; see [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md#the-highest-risk-decision)). The one genuinely-new identity construct. |
| **Inhabitant** | the agent while occupying/operating a system | MINT | `gmeow:Inhabitant` — a `logic:Role` played in an `Inhabitation`; contingent, anti-rigid. |
| **Inhabitation** | the reified subject-host occupation over time | MINT | `gmeow:Inhabitation` — a `logic:Relator ⊑ gmeow:TimeScopedRelation`. The core construct. A **lean spine**: subject + host + interval + locus directly, referencing persona/embodiment/deployment/memory-view. |
| **HostSystem / RuntimeEnvironment** | the environment capable of being inhabited | REUSE | `gmeow:PhysicalObject` / `gmeow:SoftwareAgent` located via places, composed via `gmeow:partOf`. **No new class** — "hosted on" is containment. (A thin `gmeow:RuntimeEnvironment` subkind may be minted in the AI profile only if it earns its keep — see [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md).) |
| **ModelArtifact** | the model weights / architecture / version | REUSE | `gmeow:SoftwareProduct` / `gmeow:Distribution` (software slice five-facet) + `gmeow:ModelCard` (ai slice). Mint a thin `gmeow:ModelArtifact` subkind only if model-specific facets demand it (Principle 6). |
| **ModelDeployment** | a served, callable realization of an artifact | REUSE (+ conditional MINT) | `gmeow:SoftwareAgent` + `gmeow:AwarenessTenure(modeOnlineInference)` over a serving window. Mint a distinct `gmeow:ModelDeployment` relator only if the deployment must carry its own facets the tenure cannot — see [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md). |
| **RuntimeExecution** | a particular running process | REUSE | `gmeow:AwarenessTenure` (the serving window) or `gmeow:ModelInvocation` (a single call), by granularity. **No new class** — the awareness machine-modes were built for this. |
| **AgentSession** | a bounded interaction context | REUSE (+ conditional MINT) | `gmeow:TimeScopedRelation` + ordering via `logic:Path` / `temporally-succeeds`. Mint at most **one** thin `gmeow:AgentSession ⊑ TimeScopedRelation` to unblock the agentic deferral, if the aggregate is needed by the named consumer. Ordering stays in the solver (Principle 12). |
| **AgentEpisode** | a goal-oriented sequence within a session | REUSE | `logic:State` within the session `logic:Path`; the `AwarenessTenure`-nesting idiom (an episode within a session as REM within sleep). Mint only if `AgentSession` is minted and episodes need their own identity. |
| **Embodiment** | the interface/avatar/device/account/channel used | MINT | `gmeow:Embodiment` — a `logic:Relator` (subject × channel × activation) ⊑ `TimeScopedRelation`; suppressible (Principle 10). Subsumes Cagle's Avatar. |
| **CallableCapability** | a general bearer for tool use (passive or active) | REUSE | **Do not mint.** The agentic slice already *explicitly refused* a `Tool` subclass: tool-ness is a role in the `ToolCall` event ("the Persona lesson"). A passive capability is a `gmeow:ActionSchema` (teleology); a delegated one is a `gmeow:ToolCall` to a `SoftwareAgent`. Do **not** widen `usedTool` — its range (`SoftwareAgent`) is already the most general acting thing. |
| **inhabitationLocus** *(not in the verdict; from the esoteric generalization)* | self-inhabitation vs external-vessel vs shared-substrate | MINT | `gmeow:inhabitationLocus` — an open value vocabulary (`locusSelf` / `locusVessel` / `locusSharedSubstrate`). Distinguishes invocation-into-self from evocation-into-a-vessel, and self-runtime from externally-served deployment. See [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md). |
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

The complete minted set across all three sources and the esoteric generalization:

| New term | `logic:` stereotype | One-line role |
|---|---|---|
| `gmeow:DigitalSubject` | `logic:Role` (anti-rigid) | the durable digital "who" an Agent plays on self-assertion |
| `gmeow:Inhabitation` | `logic:Relator ⊑ TimeScopedRelation` | the lean subject-host-interval-locus spine |
| `gmeow:Inhabitant` | `logic:Role` | the agent's role within an Inhabitation |
| `gmeow:InhabitedSystem` | `logic:Role` | the host's role within an Inhabitation (interim for the deferred Holon) |
| `gmeow:Embodiment` | `logic:Relator ⊑ TimeScopedRelation` | the projected, suppressible surface (subsumes Avatar) |
| `gmeow:Portal` | `logic:Event` | the reified transition between inhabitations |
| `gmeow:inhabitationLocus` | `logic:AbstractIndividualType` (value vocab) | self / vessel / shared-substrate |
| `gmeow:AgentSession` *(conditional)* | `logic:Relator ⊑ TimeScopedRelation` | the bounded interaction aggregate, if the consumer needs it |
| `gmeow:MemoryView` *(optional)* | `logic:Relator` | promoted only when a view must be signed |

Plus a small number of per-branch bearer and connector properties named in
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) and [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md)
— each a property, never a class, and never `gufo:inheresIn`.
