# Issue #1966: Bound streaming digest scratch storage independently of worker stacks

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Hosted cold generation aborts with a stack overflow after successful producer compilation and input-audit verification. Generation B in run 40314531795 failed; generation A and fixture-prefix also failed and need their terminal failures inspected.

Static disassembly of the authenticated hosted executable identifies three functions with one-megabyte stack reservations: action-cache blob verification, pipeline raw-input hashing, and slice-test worker executable hashing. Inlining places the pipeline reservation above the Stage input-files callback. That callback can enter nested Rayon discovery, whose waiting worker may execute another stage and its cache verification. This establishes overlapping large-frame risk; it does not identify the crashing thread's exact call stack.

This bounded unit moves those fixed-size streaming scratch buffers to heap storage. Preserve the existing chunk size, SHA-256 byte framing, bounded-read refusal, executable identity and typed error paths. Reuse focused synthetic hashing and cache refusal controls, compile affected production consumers under strict diagnostics, and obtain independent source review. Do not increase worker stacks, alter scheduler concurrency or produce a local corpus.

The source unit completes when the three known large stack reservations are removed and their behavior is qualified. Exact hosted cold generations, parity, authenticated fixtures, downstream consumers and proof that the observed abort is resolved remain required by #1962. That parent stays open until its full contract passes. Governing Principles 4 and 7 apply.


## Comments (0)

