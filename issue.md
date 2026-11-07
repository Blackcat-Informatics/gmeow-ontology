# Issue #2598: Represent indexed Bessel families with explicit order and valid domains

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: documentation, enhancement, foundational, cross-cutting, quality, dependencies, complexity-hard

## Body

## Problem

besselJ and besselY are named with the indexed labels Jₙ and Yₙ but typed as unary SpecialFunction individuals, with no selected order. Both declare realNumbers as domain; the usual real-valued Yₙ branch is undefined at zero and uses positive arguments. The current declarations therefore hide the order-family role and conflict with total Function meaning.

## Complete indexed-family source repair

Preserve the two existing IRIs and labels while explicitly representing the source's integer-order, real-valued reading as curried family maps:

- besselJ: integers to the function space of realNumbers to realNumbers.
- besselY: integers to the function space of positive real arguments to realNumbers.

Retype the family resources as Function maps, separating them from unary SpecialFunction members. No arbitrary fixed order, ambient-domain reinterpretation, guessed pole value, unframed partial function or unordered Cartesian-product operand shortcut is acceptable.

Define the positive-real domain as a fully framed open interval (0, +infinity), and the two codomains as FunctionSpace plus SetBuilderExpression resources with realized member conditions and explicitly bound membership laws. Family/member-frame laws must retain the order selection and the selected member's actual domain/codomain through existing functionValueRel semantics. Those laws describe framing and selection; they must not pretend to uniquely characterize Bessel analytic values or implement their evaluation.

The anticipated inventory is nine public terms: the two existing family resources, the positive-real interval, two member-function spaces, their two membership-law roots and two family/member-frame law roots. Give every selected public term full distinct English guidance and French/Chinese translations. With this inventory, the source adds 45 English guidance fields and 63 new canonical pairs per locale. Census any necessary extra public resource honestly before freezing the final contract.

## Scope and acceptance

- Correct actual type/domain/codomain links, both definitions/coats, the necessary owning-design explanation and any mapping comment that inaccurately calls a family a unary member. Preserve mapping targets, strengths and provenance rather than inventing new equivalences.
- Reuse the canonical source contracts for Function, FunctionSpace, SetBuilderExpression, Interval and Formula binding/ownership. Internal typed AST nodes must form real trees or supported acyclic DAGs with contiguous argument/binder indices.
- No generic family class, order property, numeric-object bridge or tuple representation is added without a demonstrated need. No source label or declaration is treated as numerical evidence.
- Full independent source/English/French/Chinese/mathematical/caller/example/design/mapping review; exact reconstruction/preservation, prose/whitespace and current catalog lint; configured hooks/signing, fresh PR and ghprsq landing.
- Preserve all unrelated terms, the preceding scalar-function repair, constraints, fixtures, query bodies, runtime implementations/profiles and floors.

This is the indexed-family counterpart to the scalar special-function repair under #1656. The integer-order and positive-real Y branch are explicit in this contract; broader complex-order/branch behavior is not certified. Terms CI is waived by the user; other manual product execution remains outside current authorization. Native membership guards, analytic values, generated/installed/runtime behavior, measured coverage/floors and authenticated consumers remain unqualified. Every original parent requirement remains open until satisfied. Principles 4, 7, 17 and 19.


## Comments (0)

