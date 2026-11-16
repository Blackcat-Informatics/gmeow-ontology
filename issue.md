# Issue #2820: A cache size bound makes prove refuse a large terminating program as non-terminating, naming no rule

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, cross-cutting, logic, rust

## Body

## Problem

`gmeow prove` reports the native engine as refused with reason `NonTerminatingExistential` for a large program whose rules have no non-terminating cycle. The cause is a size bound, not the rules, and the report names neither.

The joint executor refuses a program that has no termination certificate and no step budget (`crates/logic/src/physical/seminaive/joint.rs`, the `!self.admission.admits_native() && governor.remaining().is_none()` branch). Two certificates exist. The template certificate is tried first. When it fails, the input-specific certificate would be tried next, but `crates/logic/src/physical/seminaive/joint/input.rs` returns `EvidenceGap::uncacheable` without attempting it whenever the template's metadata is larger than `CACHEABLE_BYTES` (1 MiB, `joint/input/admission.rs`). A limit meant to decide what may be cached decides instead whether a program may be certified.

Observed with the built binary:

- A program that certifies and runs is refused once enough unrelated, inert Horn rules are added to push its template metadata over the bound.
- On a large program, a set of sources just over the bound is refused; removing any one source lets the native leg complete with a certificate.
- The same large program run with an explicit step allowance completes in a few thousand steps with no clash.
- Programs with a real existential cycle (`Src(A) -> exists Y. Src(Y) and rel(A, Y)`, and the same through an n-ary argument) are refused, correctly.

Three further defects on the same path:

1. The refusal reasons do not name a rule. Each weak-acyclicity position edge is one reason with no rule identity; the members of a restricted-joint-acyclicity cycle are only logged; a compiler-produced rule is identified by a content hash that is not mapped back to the authored formula.
2. The bound itself is reported as one reason among hundreds, with the same code as the others, although it is the cause.
3. When the engine refuses, the report omits the lowering-residue drops (`UNSUPPORTED_CONSTRUCT`) that a completed run shows, so the refused report understates what the selection contains.

## Acceptance

- [ ] The input-specific certificate is attempted whatever the template's size; only caching is skipped above the bound. Every other analysis bound still applies.
- [ ] A program padded over the bound with inert rules certifies and runs; the genuine-cycle programs stay refused. Tests cover both, at the engine and through `prove`.
- [ ] A termination refusal names the rules of the offending cycle and their relations, with a compiler-produced rule mapped back to its authored formula where one exists.
- [ ] A refusal caused by an analysis bound has its own drop code and says which bound.
- [ ] A refused report carries the same lowering-residue drops a completed run of the same selection carries.

Principles 7, 17.


## Comments (0)

