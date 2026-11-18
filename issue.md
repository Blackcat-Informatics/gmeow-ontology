# Issue #2927: Bind native proof completeness to the executed source

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, enhancement, foundational, cross-cutting, quality, logic, dependencies, rust, complexity-hard

## Body

# Bind native proof completeness to the executed source

Part of #2748. This independently landable unit binds native proof completeness
to the exact selected source and execution. It preserves every other parent
requirement and the existing classical/module/context/correspondence owners.

## Problem and resulting behavior

The native `gmeow prove` verdict currently depends on classical FOF source
admission. An existing provenance-only case records two claims consumed by the
native engine and no clash, yet remains undecided solely because classical
admission cannot flatten its reified source. Native execution and classical
export require distinct admission evidence for the same exact source selection.

Bind the native verdict to a live receipt for the source actually consumed and
the execution actually completed. Keep classical admission as the independent
authority for selected FOF export. A native certificate must neither authorize
that export nor turn unread source into admitted source.

This child does not promise a decided Principia verdict. The frozen consumer
still reads zero of its 4,548 scoped claims and has module, context, owner,
accounting and lowering blockers. Those obligations remain in the parent and
their existing owners.

## Included implementation contract

- Reuse one compiler-owned immutable inventory derived from the existing
  `StructuralSourceGraph`, source traces and selected roots. Keep the complete
  selected-unit denominator and stable source/program/selection identities.
- Source-obligation identity is profile independent and binds authored anchors,
  document occurrences and exact lowering traces. Existing FOF row IDs include
  dispositions; they are not neutral source IDs. Preserve the classical
  observation and publication contract while extracting the shared inventory.
- Borrow/share the existing parsed and prepared theory. Add no second corpus,
  parser, compiler, family scanner, unconditional classical admission or full
  inventory copy. Compute classical dispositions only for selected export,
  against the same inventory; reuse streaming evidence/report machinery.
- Keep native authority in the native reasoning layer. The compiler must not
  depend on engine types. Reuse `ClaimIngress` and its actual input/program
  authorization, selected-domain authority and retained native execution
  evidence. Do not introduce another closure or proof-evidence owner.
- Before execution, validate inventory, selection, preparation and ingress as
  eligibility. Reject malformed source and identity mismatches. Incomplete
  source can still run its faithfully admitted fragment to seek a refutation.
- After that exact run, bind the final live native receipt to the actual input,
  consumed source rows and native execution evidence. A native `consistent`
  verdict requires complete source accounting, completed supported execution,
  and no remaining semantic, preservation, accounting or execution drops.
- Preserve a valid witnessed clash even when source completeness is blocked.
  Keep every unread or unsupported obligation visible in that report.
- Separate live authority from serialized observation. Altered required rows,
  profiles, selections, ingress or execution identities invalidate authority;
  decoding a report never authorizes a fresh execution or projection.
- Wire the shipped CLI verdict and JSON/text failure/report paths through this
  native receipt. Retain independent FOF admission for explicit export and its
  existing complete-source, tamper and stale-output protections. A selected
  export that cannot be produced must fail actionably; native success cannot
  disguise a missing required output as overall command success.

## Ownership and overlap

| Owner | Boundary retained by this child |
| --- | --- |
| 2748 | Native admission and whole actual-canon acceptance; assign a distinct child owner before editing. |
| 2780 | Classical evidence-only admission. Coordinate shared inventory, resolver and CLI/report changes. Preserve its retained 23-path WIP at `37f60cd5ca39e849a24db318add73d12825e9f70`; its known false-consistent contradiction makes it an unqualified implementation base. |
| 2781 | Explicit module/import selection. Keep the existing top-level selection in this child; do not add imports or silently compose modules. |
| 2782 | Typed context translation for formulas and rules. Keep its blockers; do not erase coordinates. |
| 2796 | Correspondence/metatheory execution. Do not classify owner definitions as inert or execute children independently to remove owner drops. |

Preserve classical `CanonAdmissionPublication`, its pipeline consumer and
decode-as-observation contract. No producer artifact or publication-schema
rewrite belongs to this child. Agree shared-file ownership before implementation;
the receipt implementation is frozen and the current terms build may finish.
This native admission unit owns the active prover implementation lane.

## Acceptance using existing behavioral seams

1. The existing provenance-only contradiction is inconsistent, consumes both
   claims, and retains its witness/provenance. It must never become consistent,
   including the known failure shape recorded by the retained 2780 work.
2. The existing no-clash provenance-only case becomes native-consistent only
   when its complete receipt, accounted semantics and actual closure support
   it. An explicitly selected FOF export keeps its separate admission result
   and required-output failure behavior.
3. Existing unread module/standpoint, unknown metadata, mentioned carrier,
   nested mention and named-graph boundaries remain undecided or rejected as
   appropriate. This child broadens none of their semantics.
4. Existing wrong-input, wrong-program, foreign-preparation, altered-row/status/
   profile/selection and decoded-observation cases cannot forge admission.
5. Existing incomplete-budget, unsupported-lowering and native-not-supported
   paths remain undecided even if source eligibility succeeds. A completed
   no-clash string with incomplete/undetermined evidence is insufficient.
6. Actual TPTP and producer-publication consumers retain their classical
   contract. Review actual call sites after inventory extraction; do not infer
   native authority from a classical or serialized observation.
7. Review allocation ownership to verify one immutable inventory, one parsed/
   prepared theory and no unconditional classical work on native-only proof.
   Reuse existing relevant resource evidence; add no fixed memory threshold,
   mechanical test, broad benchmark or permanent test matrix.

Extend the existing cases identified in `native-admission-design.md`. Perform
focused qualification of the changed library paths and shipped CLI, followed
by mandatory hooks. This unit does not schedule another whole producer run. Principles 4, 7, 17 and
19 govern source ownership, faithful evidence and the native authority boundary.

## Frozen consumer and qualification limits

Use `updated-consumer-inventory.json` and `updated-consumer-fixtures.json` for
source traces and unchanged-file selections. The full frozen Principia tree is
`96ed43c1a6777e412ccdac5b4f83243872974d79`; its 212 selected documents contain
67,044 semantic units. Small file selections are regression-design inputs,
not demonstrated standalone closed-theory tests or replacements for the full
coverage denominator.

The diagnostic binary at source `52466a33` plus two exact frozen dirty files
completed 3,289 native steps without a clash, but remains incomplete/undetermined,
with zero scoped claims read and 15,054 drops. Its 131 lowering residues and all
scope/owner/accounting blockers remain required work outside this bounded child.
The older embedding assets failed their content gate and are explicitly
diagnostic inputs, never current ontology corpus or release acceptance.

The full-canon module-composition question remains pending. This child does not
choose between joint theory composition and independent module obligations,
change canon, flatten contexts, infer import edges, qualify unrestricted FOL,
or authorize signing, production, deployment or lifecycle actions.


## Comments (1)

Planning controls and a link to the merged source-refusal repair are present. This notice contains no additional implementation requirement or actionable source finding. Original comment: https://github.com/Blackcat-Informatics/gmeow-ontology/issues/2927#issuecomment-6967472335

The original notice remains at its forge link; exact intake bytes are preserved locally. The issue body above is unchanged.
