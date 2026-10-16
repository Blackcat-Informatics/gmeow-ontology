# Reflective property-object FOF applications

The writer now emits an arity-indexed application relation whose first argument
is the actual property object. Equality between property objects therefore reaches
their applications through ordinary first-order congruence. Unary membership and
primitive schema predicates have explicit profile interpretation laws; source
admission must still succeed before those laws are added. Unsupported selected
semantics remain explicit refusals.

The independent finite-model checker regression uses the complete emitted problem
and rejects a model that tries to assign equal property objects different
extensions. This is a synthetic artifact-checker demonstration, not an external
solver run or complete-canon admission result.

Final source evidence before reconciliation:
- raw/fof-property-reflection-runtime-v2.log: terminal PASS, 103/103 selected
  TPTP/external-evidence controls, nextest 08c7c535-7db3-42bf-bb88-44023d21baf9.
- raw/fof-property-reflection-clippy-v3.log: terminal PASS, strict
  producer-independent workspace/all-target check; four corpus-dependent consumers
  are excluded and remain unqualified.
- raw/fof-property-reflection-lint-v3.log: terminal PASS.

The later correspondence/retraction reconciliation changes require their own
affected checks. Producer, complete canon, installed consumer, full gate, hosted
CI, final independent review, and merge acceptance remain unverified.
