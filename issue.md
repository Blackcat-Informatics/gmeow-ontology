# Issue #2727: displayable is not scoped to a standpoint

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, foundational, cross-cutting, terms

## Body

## Need

Display suppression needs one of two things: a form that is scoped to a standpoint, or an explicit statement that display preference belongs to the reader and is not data. Today `gmeow:displayable` is a single global switch, so a standpoint cannot suppress a claim for itself without suppressing it for every reader.

## Evidence

`gmeow:displayable` (`slices/core/names/module.ttl:305-317`) is a `logic:DatatypeProperty` with range `xsd:boolean` and no domain. Its definition says it is "The ONLY display control in the model ... It is intentionally domain-free ... consumers MUST honour false and never surface the string. NOT functional: in a multi-source merge, sources may disagree, and those claims must coexist rather than force a global inconsistency — the cautious value (false) wins at the projection layer."

Its `gmeow:howToUse` says: "Set gmeow:displayable false on the suppressed name or facet; consumers MUST honour false and never emit the string ... let the cautious value win when sources disagree." Its `gmeow:avoidWhen` says: "avoid forcing one value in a merge — competing claims coexist, the cautious false winning at projection."

It carries no standpoint. A value on a node applies to every reader, and where values conflict the `false` value wins. The names slice documentation (`slices/core/names/module.ttl:1606`) says "display SELECTION is locale-relative and symmetric; only display SUPPRESSION (gmeow:displayable false, e.g. a deadname) is modelled". The standpoint slice says display suppression for a corrected claim is "a projection policy, not a global truth judgment" (`slices/core/standpoint/module.ttl:1266`) and asks for suppression "in that standpoint" (line 1263), which `displayable` as defined cannot express.

Warning-level constraints ask for `displayable false`:

- `gmeow:SupersededEntitySuppressionConstraint`, `slices/core/lifecycle/module.ttl:172-197`: "A closed-world integrity constraint: a superseded gmeow:Entity (one carrying gmeow:supersededBy) should set gmeow:displayable false — suppressed from display, never deleted (Principle 10)." `logic:severity "Warning"`.
- `gmeow:SupersededObservationSuppressionConstraint`, `slices/core/observations/module.ttl:914-934`: "a gmeow:Observation carrying gmeow:validUntil (superseded or withdrawn) should set gmeow:displayable false". `logic:severity "Warning"`. By reading, any observation with an end of validity triggers it, whether or not anything superseded it.
- `gmeow:WithdrawnStandpointSuppressionConstraint`, `slices/core/standpoint/module.ttl:921-945`: "A withdrawn standpoint (a StandpointTenure whose interval has ended) should set gmeow:displayable false — suppressed from display, never deleted." `logic:severity "Warning"`.
- `gmeow:SupersededNameSuppressionConstraint`, `slices/core/names/module.ttl:1624-1636`: "A superseded name or deadname should set gmeow:displayable false." `logic:severity "Warning"`.

Guidance in `slices/core/epistemics/module.ttl` on claim tokens and evaluations (lines 44 and 58) also says to retract or supersede "by suppression (gmeow:displayable false)".

Display suppression of a person's superseded name is a safety-relevant case where a global, cautious `false` is the intended behaviour. The gap is the other case, where suppression is a stance of one standpoint about another's claim.

## Options

1. A standpoint-indexed display claim: a relation or annotation, indexed by `gmeow:accordingTo`, stating that a node should not be shown from that standpoint, with `gmeow:displayable` retained for the global safety case.
2. Document `gmeow:displayable` and suppression as a projection policy applied by the reader from its own stated standpoint, and state that nothing in the data may set it because another claim disputes a node.
3. Keep `gmeow:displayable` as is but constrain who may assert it (for example require an `accordingTo` and honour it only for the asserting standpoint's readers), and revisit the warning constraints above so they do not prompt suppression for non-superseded cases.

## Decision required before any term is authored

Whether display is data (indexed by standpoint) or reader policy, and how the existing global cases (deadnames) stay safe, must be decided first. The four constraints above must be examined under the chosen option. No term is proposed for authoring in this issue.

Related: #2725, #2726


## Comments (3)

### paudley — 2026-11-14T23:30:38Z

Blocks: #2795 (Release 0.4.0).

### paudley — 2026-11-15T00:32:15Z

Decision by the maintainer on the open question: **display preference is reader policy, not data.**

- `gmeow:displayable` stays the one display control and keeps its global safety meaning (a deadname or other superseded label is suppressed for every reader; the cautious false wins at projection).
- No standpoint-indexed display term is added. What a particular reader sees is decided at projection and recorded, where it is recorded at all, as a per-reader `gmeow:DisclosureDecision`.
- No data may set `gmeow:displayable false` because a claim is disputed, corrected or evaluated.

Acceptance:

1. The four warning constraints that prompt `displayable false` (superseded entity, superseded observation, withdrawn standpoint, superseded name) are each examined and narrowed to the cases that are global safety. None may fire because one claim disputes or corrects another, and the observation constraint no longer fires merely because a validity interval has ended.
2. The definition and usage notes of `gmeow:displayable`, and the guidance on the claim relations, say which cases are global safety and which are reader policy.
3. A counter-example shows a disputed claim with no display flag is not warned about, and an existing deadname case still is.
4. Translations and examples are revised with the definitions.


### paudley — 2026-11-15T02:37:53Z

Scope found while landing the standpoint part of this decision. These are the remaining places, by slice, that state or enforce the old reading (a correction or dispute sets `gmeow:displayable false`), and they belong to this issue:

- **events:** the module says corrections use `gmeow:displayable false` (about lines 1567 and 1615-1626, including the constraint that formalizes the corrected-event suppression shape, which prompts suppression of a corrected event), and `docs.md` line 22 says the same.
- **attestation:** `docs.md` about line 221 gives a corrected issuer name `displayable false`.
- **dreaming (extension):** the module about line 15 treats a corrected dream report as superseded.
- **epistemics:** the usage notes of `gmeow:ClaimEvaluation`, `gmeow:ClaimToken` and `gmeow:DoxasticTenure` tell a holder revising its OWN record to mint a new one and flag the old one `displayable false`. That is a self-revision, not a dispute; whether it remains a global safety case under this decision is to be judged with the four constraints.
- **gender, names, software (extension):** text saying a correction rejects the earlier claim only in the correcting standpoint. Not false, but it lacks the statement that a correction neither supersedes nor suppresses.
- **standpoint (after its current change lands):** `WithdrawnStandpointSuppressionConstraint` fires because a tenure's interval has ended, the same pattern this decision rules out for observations, and is still to be examined; a `gmeow:succeedsClaim` or `gmeow:retractsClaim` link made by ANOTHER party still ends a claim's liveness for a ranker, and because succession specializes `gmeow:supersedes` it also prompts suppression of the other party's claim.
- **A modelling point to settle here or separately:** `gmeow:correctsClaim` has domain and range `gmeow:Entity`, so claim tokens (activities) related by it are inferred to be entities.

One pull request per slice.


