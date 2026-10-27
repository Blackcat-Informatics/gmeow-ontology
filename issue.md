# Issue #1989: Use native JSON for complete formula-lowering observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, logic, dependencies, rust, complexity-hard

## Body

Move the complete formula-lowering observation operation to PurRDF native JSON under Principles 4 and 7. Its actual publisher, cache bytes and authenticated reader must retain the whole observation and nested inspection family, not only an outer envelope.

Acceptance:

- Migrate FormulaObservation, FormulaLoweringInspection, RuleInspection, RcAtom and RcNumeric, plus all four AtomicTerm variants and all eleven NumericOperator variants. Reuse the existing native Formula, Diagnostic, PreservationClaim, recorded-outcome and checked four-component literal owners.
- Wire the actual formula fixture publisher, authenticated reader and authored readers. Preserve source selection, selected count, formulas, diagnostics, both recorded outcome levels, all Horn and existential rule fields, numeric operand and rule order, nullable numeric result, distinct pairs, literal metadata and complete causal diagnostics.
- Require named records and required fields, tolerate unknown fields, refuse duplicate known fields and positional records, and validate malformed shadowed map values before typed last-wins replacement. Preserve existing nullable defaults. Preserve explicitly tagged atomic kinds and strings without resource-token inference.
- Inspection records remain data-only diagnostics. Hydration must not rerun lowering, numeric validation scheduling, execution or preservation computation; it must not normalize a recorded rule into another rule.
- Remove obsolete JSON-only DTO Serde ownership after verifying callers. Retain shared AtomicTerm, RcAtom, RcNumeric and NumericOperator Serde for genuine RelationalCoreProgram binary persistence and typed hashing; their binary representation must remain unchanged.
- Reuse the four existing authored selection, shared-witness, literal/polarity and invalid-literal-slot controls. Add only focused complete transport, numeric metadata, causal-outcome and admission coverage. Compile the actual logic compiler, logic engine, pipeline and developer CLI callers and run normal hooks.

Remaining contracts: #1776, #1777, #1336 and #1962. Full prepared programs, complete workspace Serde retirement, actual corpus execution and release acceptance remain with their existing owners. The user retains the explicit restriction against local optimized producer, corpus production, synchronization and full gate execution, and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

