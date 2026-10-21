# Independent source review: explicit grounding-owner sets

Disposition: no actionable finding in the frozen 16-file prerequisite. This is a source review, not qualification. No tests, Make targets, parsing tools, builds, formatters, host probes, or corpus/producer operations were run by this reviewer.

## Admission and public model

`GroundingOwners` owns a private `BTreeSet<String>` and exposes only checked construction, exact membership and ordered iteration. Construction refuses empty sets and every IRI outside the three exact Logic/Math/Lang slice IRIs. No unchecked/default/first-owner constructor was added. The rubric parser examines every raw owner object before construction: a literal or blank node fails even beside a valid owner. The public re-export and all current `ProjectionVocabulary` initializers/callers found in the crate closure have been migrated; no `.owner` caller remains in that closure.

## Counting and weakening protections

Actual surface measurements use exact set membership through `OwnerBoundary::Surface`. Native complete envelopes and validated node/binding target edges retain their existing validation checks; raw external axioms, template atoms and incomplete cells remain residue. Existing ordinary structural-alignment behavior is unchanged. Explicit `OwnerBoundary::Neutral` preserves owner-neutral APIs without selecting a preferred member; historical advisory counting remains independent of owner selection.

The meta-ratchet rejects each working owner absent from the base set, so expansion and replacement both fail. Removals are permitted strengthening; checked construction prevents removal of the last owner. Every count-kind change now fails, closing the previously identified alternative-recognizer gap. Existing deletion/prefix and namespace-removal checks remain intact: renaming an old guard cannot hide its deletion, and an added overlapping guard does not remove the old guard's independent measurement. This prerequisite does not change catalog stem/prefix matching or implement the pending schema.org split.

## Canonical carrier and policy

The old `vocabularyOwnerFunctionality` individual is removed in full, with its functional-carrier ledger entry and paired French/Chinese catalog entries. No `characterizes vocabularyOwner` functionality assertion remains in the reviewed owner files. The replacement is a canonical `logic:allValuesFrom` restriction over the three allowed owner individuals plus an explicit `ClosedWorldClosure` entry for the owner path. It imposes required presence and allowed membership without a singleton maximum. The property/class authoring guidance and paired changed catalog text describe explicit sets and reject arbitrary authoring exemptions. No new named semantic term requires a missing annotation coat.

The diff leaves current guard instances, namespace/count configurations, ceilings and exemption declarations unchanged. This therefore enables explicit shared ownership for a future new guard; it does not silently widen an existing owner boundary or repair the remaining schema.org authoring residues.

## Coverage and remaining acceptance

The eleven new `owner_set_` controls cover deterministic complete admission; missing, invalid and mixed invalid objects; independent tiny canonical required/allowed/unbounded shape derivation; complete native and node bridge exemptions at both declared owners; malformed/raw/non-owner residue; shared-owner versus foreign relocation through both dataset and text carriers; owner expansion/replacement and all recognizer changes; standalone standards admission; and installed debt-reader surface membership. Existing tests were mechanically migrated to exact canonical owner IRIs rather than a namespace-shaped placeholder.

All these are authored tiny datasets/models or temporary independent source files. The canonical-shape test validates an independently authored analogue, not production compilation of the full rubric. Actual execution of the eleven controls, existing regression selection, source-policy/helper checks, full canonical derivation/native acceptance and produced/installed acceptance remain unverified by this review. Root owns their qualification. Full schema.org grounding repair remains pending.

## Follow-up: missed installed CLI callers

The initial review missed two actual installed-consumer field accesses in `crates/gmeow-cli/src/commands.rs`, in the human and TSV branches of `slice_projection_ceilings`. They still referred to removed `v.owner`; the earlier caller-closure statement was scoped too narrowly to the slice-quality crate and did not establish workspace completeness. Root caught and repaired both locations.

Reviewed only this follow-up delta: both now render `v.owners.iter().collect::<Vec<_>>().join(",")`. The checked set's deterministic IRI iteration includes every member without choosing a preferred owner. A singleton produces the same string and existing padding/TSV columns remain unchanged; multiple owners form a sorted comma-separated field. No admission, measurement, ceiling or error behavior changes in this delta. No further finding in the repair.

The full embedded-consumer CLI compile remains unrun/excluded from the focused source qualification; this source inspection does not establish compilation or installed execution. No extra tests, checks or source edits were performed by this reviewer. Review refrozen after this correction.
