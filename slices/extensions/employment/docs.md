<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Employment — tenure, compensation, and career as reified membership

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/employment` · **tier: extension**
> The ResumeRDF / Europass / schema.org / ESCO / CTDL superset layer — one relator, many projections.

Employment is not a new primitive; it is a `Membership` — a reified `gufo:Relator`
connecting an agent to an organization, optionally founded on an Agreement (Principle 4:
reuse the spine, add only the facets). The structural skeleton (member, organization,
role, period) is inherited; this slice adds the employment-specific facets — type,
compensation, seniority, occupation. Flat-first, reify on demand: a flat `gmeow:memberOf`
covers the 80 % case; promote to `gmeow:Employment` when type, compensation, seniority,
or provenance matters. Career *events* (hiring, promotion, transfer, resignation,
termination) are value individuals in the universal `EventType` vocabulary, never
subclasses (Principle 9).

The standpoint doctrine applies with full force: disputed tenure, rival role
claims, and contested terminations are standpoint-indexed claims that **coexist**, none
privileged (Principle 9) — two `accordingTo`-annotated `memberOf` triples, or two reified
`Employment` relators each carrying `gmeow:accordingTo`. There is no employment-specific
dispute mechanism, no `primaryEmployment`, no `preferredJob` — only the cross-cutting
standpoint facility. A withdrawn employment record sets `gmeow:displayable false`, never
deletion (Principle 10). Its Principle-15 consumer, declared in the manifest:
**CV/employment claims over organization and agreements, and the mail corpus's
signatures** — a signature block ("Jane Doe, Senior Engineer, Acme") is exactly one
flat-or-reified employment claim from the vantage of the message.

## The relator

### gmeow:Employment

A `Membership` subkind: agent, organization, and role inherited; type, compensation,
seniority, and occupation added here. Relator mediation is axiomatized (EL
`someValuesFrom`: an Employment has a type, a member Agent, and an Organization) so the
doctrine is reasoner-visible; closed-world cardinality belongs to SHACL
(`gmeow-shapes.ttl`).

### gmeow:employmentInterval

The tenure: `Employment → TimeInterval`, matching `participationInterval` and
`membershipInterval` (relators carry their period this way; `duringInterval` is reserved
for `gufo:Situation`-based time-scoped relations). Open-ended — no end instant — when the
employment is current. Contested dates are coexisting standpoint-indexed intervals.

## The facet vocabularies (Principle 9 — values, never subclasses)

### gmeow:EmploymentType

The kind of employment: seeds `employmentTypeFullTime`, `…PartTime`, `…Contract`,
`…Intern`, `…Freelance`, `…Volunteer`, `…Apprentice`. Open: a kind not among the seeds is
a fresh individual with a label, never an `Employment` subclass.

### gmeow:employmentType

Functional pointer to the type — one canonical type at a time. A change of type is a new
employment record or a standpoint-indexed claim, never an in-place overwrite.

### gmeow:SeniorityLevel

The rank: seeds `seniorityEntry`, `seniorityMid`, `senioritySenior`, `seniorityLead`,
`seniorityExecutive`. Deliberately orthogonal to both employment type and role — a
part-time executive and a full-time intern are both expressible without a combinatorial
class explosion.

### gmeow:employmentSeniority

Functional: one canonical seniority at a time; a promotion is a new employment record or
a standpoint-indexed claim — which is also how the promotion *event* (an `EventType`
value) and the resulting record stay distinct.

## The cross-slice facets

### gmeow:employmentRole

The job title or function, specializing the universal `gmeow:hasRole`. Functional per
employment; rival role claims are rival relators, not multiple values.

### gmeow:employmentOccupation

The occupation drawn from an open external vocabulary (ESCO, SOC). Functional: one
canonical occupation per record; an agent with several occupations has several employment
records.

### gmeow:employmentCompensation

`Employment → MonetaryAmount` — compensation always carries its explicit currency
reference frame (Principle 11: an amount without its frame is meaningless).
Non-functional by design: raises, bonuses, and multi-currency arrangements are co-equal
standpoint-indexed claims over time, not a single overwritten salary field.

## Solver layer & alignment

Career-trajectory queries — tenure overlap, gap detection, seniority progression — are
solver-layer computations over `employmentInterval` and the temporal slice's clocks
(Principle 12); the OWL core asserts records, not timelines. Alignment to the superset
sources (ResumeRDF, Europass, schema.org `EmployeeRole`, ESCO, CTDL) is the projection
layer's concern: each is a lossy down-projection of the relator, deferred until its
consumer is live.

## Dependencies

Depends on `kernel`, `entities`, `observations`, `organization` (Membership, Role,
Agreement), and `temporal` (TimeInterval). Reuses MonetaryAmount, Occupation, Credential,
Skill, and LanguageProficiency from their home slices without redefinition.
