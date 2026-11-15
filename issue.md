# Issue #2299: Identify the E8 group-action actor explicitly in canonical data

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, quality, logic, complexity-hard, terms

## Body

The existing math:GroupAction definition states that an action names its acting group and target, but the authored E8 action record does not explicitly link to the resource identifying its acting group. Keep that mathematical identification obligation intact; its omission is not an accepted optional field. Own the defect under #1656 without reopening the closed original algebra work. Principles 4, 7 and 17.

Source evidence at 2a5ad3608331ff0592eb169459b682696fd0c3cd:

- slices/grounding/math/module.ttl describes an action of a math:Group on a carrier, with compatibility and identity laws, and says it names the group.
- In examples/e8-symmetry.ttl, exa:e8WeylAction has math:GroupAction type, math:actsOn exa:e8Roots, math:preservedOperation exa:rootReflection, and preservationLaw/satisfiesAxiom links to exa:groupActionLaw. It has no explicit link to the acting-group resource.
- The root system and Weyl group are separately named. The action's target or label does not by itself identify a unique acting group: an arbitrary subgroup action must not be conflated with the full group, and E8's specific Weyl/full-automorphism equality is not a general inference rule.
- The existing compatibility formula is realized syntax with a bounded witness. It does not supply the missing resource link or prove every identity/completeness/action claim. No fresh runtime or validation result has been observed for this omission.

Acceptance for the owning repair:

- Choose the actual canonical representation for the acting-group identity after inspecting existing vocabulary and consumers; wire the real authored record and production callers through that owner. Do not invent an ungoverned predicate, substitute the target for the actor, infer the actor from a label, or falsely retype the action as a group.
- Preserve actual action laws, witness data and unrelated Group/RootSystem/representation semantics. Retain the full-group/subgroup distinction and E8-only equality without a new universal superclass or identity rule.
- Make the mathematical obligation and actual declaration/validation behavior explicit. A documentation-only description of the omission does not resolve this defect, and no group-identification requirement may be waived to make existing data appear complete.
- Verify actual actor identity and relevant missing/wrong-actor boundaries through permitted controls and authenticated inputs. Existing source-only execution/vocabulary/test-inventory limits remain in force; no authority for new fields, helpers, tests, producer/corpus/sync/reasoning/full gates or lifecycle changes is granted by this report.

This remains unresolved until the representation, real record/caller wiring and required acceptance are delivered. It does not close #1656 or the separate E8 order defect #2294, and does not change existing publication/upstream holds.

## Source delivery and remaining qualification

#2549/#2550 delivers the explicit acting-group source representation at c441bfce55033cf9c9ab66e92299ef74de20350d. Canonical math:actingGroup links the actual E8 descriptor to exa:e8Weyl. Class-local required Group/maximum-one resource obligations derive validation through math:MalformedGroupActionActor. Actor, descriptor and target roles remain separate without imposing different resource identities; regular and conjugation actions may use the acting group itself as target.

Complete new/affected English/French/Chinese coats, owner algebra/conformance guidance, the actual mathematical DOI/section citation and four declarative raw actor-boundary cells ship together. The existing native CQ requires exact actor/target/operation/both-law links and preserves its original derived equality witness, result column and row. Existing E8 Formula/witness/type/order data and unrelated Group/RootSystem/Lie/representation semantics remain intact.

Independent design/governance and full source/language/fixture/CQ/citation review, exact fifteen-file transfer, current catalog lint, prose/byte checks and normal signed hooks passed. Terms CI was explicitly waived. No RDF/native/generated/query/shape/corpus/build/test/producer/sync/reasoner qualification was run manually.

This original issue remains open for actual actor-boundary and exact-E8/law-link controls over permitted authenticated inputs. Generic raw reader validation is distinct from native open-world type/equality inference. The reasoningLogic CQ harness has no replacement/deletion overlay; asserted-lane UNION overlays cannot supply those controls. No false successful control, native wrong-kind/count rejection or complete constructed action is claimed. Parent #1656 and separate #2294 remain open.


## Comments (2)

### review bot — 2026-11-02T23:40:03Z

Automated issue-planner boilerplate with no requirement or finding. Omitted from this copy; the original comment remains on the forge.

### paudley — 2026-11-14T23:30:26Z

Blocks: #2795 (Release 0.4.0).

