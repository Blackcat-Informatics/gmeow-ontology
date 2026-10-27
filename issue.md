# Issue #2000: Use native goal-directed observations and authenticated corpus controls

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Move the complete goal-directed producer observation to PurRDF native JSON under Principles 4 and 7, and remove repository corpus execution from its test path.

Acceptance:

- Implement all fields of GoalDirectedAnswer, GoalDirectedVerdict and GoalDirectedEvaluation, plus one canonical five-field observation record shared by the pipeline publisher and authenticated typed consumer. Replace the pipeline JSON value and the corpus-local duplicate DTO together. Keep the existing dependency direction; no second schema owner or generic compatibility bridge.
- Wire the actual stage-goal-directed observation publisher and authenticated source-artifact reader through the native owner. Preserve compact output without a terminal newline. The current JSON value sorts every object recursively: retain outer order evaluations, program_projections, projection, repeated_program_projections and repeated_projection; retain evaluation order answers, clauses, description, goal, iri, name, status, verdict_probe_atoms and verdicts. Answer, verdict and binding-map keys retain their lexical order.
- Preserve all raw rendered atoms, goals, clauses, probe text, proof identifiers, authored IRIs, names, descriptions, statuses and verdict strings. Preserve false proof_checks and arbitrary string values as observation data. Retain every vector in supplied order and all five observation fields independently. Do not execute, proof-check, validate semantic tokens, sort evaluations or repair evidence during hydration.
- Require named records and every nonoptional field, including boolean and collection fields. Reject malformed or duplicate known fields while accepting unknown fields. Typed string maps must decode every value before last-key replacement and preserve their sorted-map semantics. Use the native default depth policy and existing PurRDF record APIs.
- Verify all actual consumers before retiring the three transport-only Serde derives. Preserve any genuine separate binary or JSON owner if one is established. Structural query, answer, verdict and program IRI framing and all reasoning, proof, sorting and projection algorithms remain unchanged.
- Remove compile_logic_upstream_with_reasoning_programs and the test path that reads and compiles the repository reasoning-program corpus. The existing positive pipeline graph control must consume the exact authenticated stage-goal-directed product read-only, retaining all Peano, member, well-founded true/false/undefined, subsort and zero-answer control assertions. Missing or stale selection must fail without invoking a producer.
- Preserve independent upstream-refusal checks with explicit tiny authored inputs. Tests must not rebuild any repository corpus directly or indirectly. Do not copy the shipped demonstrators into test constants or synthesize a selector. Keep corpus controls separate from the focused authored runtime selection.
- Extend existing authored Peano and content-addressed projection controls through native observation bytes without weakening their proof, result or identity assertions. Add bounded complete-wire and admission cases for the new native boundary. Compile actual logic, pipeline and developer CLI callers, run only safe authored controls locally and use normal hooks.

Remaining contracts: #1776, #1777, #1336 and #1962. Complete reasoning execution evidence, prepared programs, other observation families, workspace Serde retirement, installed consumer and actual corpus/release acceptance remain unfinished in their existing owners. The standing user exception prohibits local optimized producer, corpus/fixture production, synchronization and full-gate execution and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

