Logic frontend: lower a scope carrier that has several values of one coordinate (Principles 4, 9, 10)

A reifier, statement node or rule node that carries several values of exactly one scope coordinate now
lowers to one scoped claim or rule per value, in an order that does not depend on storage. Several values of
two or more coordinates are refused whole (AMBIGUOUS_SCOPE_COORDINATES); several values on a rule atom are
refused (AMBIGUOUS_RULE_ATOM_SCOPE). Values are counted by what they denote: confidence and modality by parsed
value, the text coordinates by text. Every lowered claim or rule keeps the authored node as its source; rules
lowered from one node with one disposition are one admission unit with an unchanged identifier. A diagnostic
meta-rule node that lowers to several rules is refused instead of one being chosen silently. The diagnostic
UNSUPPORTED_SCOPE_MULTIPLICITY is removed.

Verified: independent review PASS at 46902b851; the merge of main into the branch changes none of the files
of this change. Hosted helpers, lint, producer-build and rust-prebuild green. The three hosted producer jobs
had not finished at merge and end red on main itself for lack of runner memory. No producer run was made on
this change.

Closes #2777. Part of #2748.
