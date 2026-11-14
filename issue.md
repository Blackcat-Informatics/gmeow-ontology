# Logic frontend: lower a scope node that carries several values of one coordinate

**REQUIRED.**

## What happens today

The logic frontend reads the scope of a claim or rule from the node that carries it: a statement's reifier,
a statement node or a rule node. `scope_value` in `crates/logic-compile/src/frontend.rs` takes the
default-graph statements `<node> logic:<field> ?value` for one scope field. When the node has more than one
value of that field, it reports

```
UNSUPPORTED_SCOPE_MULTIPLICITY: logic:<field> has multiple scope coordinates; this scope requires explicit lowering
```

and `scope_from_node` withholds the whole claim or rule. That is the right refusal for a compiler that has
no rule for the case: picking whichever value is read first would not be admission. A full pass of this
repository's own slices reports the diagnostic zero times, so no claim of ours is withheld by it.

The vocabulary does not forbid the shape. The scope note of `logic:provenance` says no functional
characteristic is asserted and that the single coordinate is a limit of the selected compiler scope, not of
the vocabulary. And the message names a step, "explicit lowering", that does not exist: there is no
construct an author can write to keep several values of one coordinate on one node, and no compiler step
that lowers them. An author's only course today is to restate the claim under one reifier per value.

## Wanted

The lowering the message names: a scope node that carries several values of one coordinate is lowered to
one scoped claim (or rule) per value.

- For every scope field alike: `standpoint`, `time`, `confidence`, `modality`, `provenance`, `inModule`.
- When several fields are multi-valued on one node, decide and state the rule (the product of the values,
  or a refusal that names the node); do not leave it to the order rows are read in.
- Every other refusal stays: an invalid confidence, an unknown modality or a proposition-valued coordinate
  on any of the values still withholds the whole claim or rule.
- The result does not depend on storage order: the same claims in the same order for the same source,
  whichever value the store returns first.
- Each lowered claim keeps the identity of the node it came from, so diagnostics, provenance and the
  emission evidence still name the authored node.
- The scope notes of the scope properties in the logic slice and the sentence in `docs/PIPELINE_SPINE.md`
  about "unlowered coordinate multiplicity" are made true in the same change, with their catalog entries.

## Acceptance

- The existing case in `crates/logic-compile/src/frontend/native_statement_tests.rs` that pins the refusal
  becomes a pair: a node with two values of one coordinate yields the two scoped claims, and the two
  orders of the same input yield the same result.
- A case for each scope field, and one for two multi-valued fields on one node.
- A full pass of this repository's slices produces the same findings as before the change.

