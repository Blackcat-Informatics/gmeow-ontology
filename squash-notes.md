test(math): control the group-action actor boundary and the E8-only agreement (Principles 4, 7, 17, 19)

The actor of a group action is already declared on main through `math:actingGroup` with three class-local
obligations on `math:GroupAction`. This adds the controls around that declaration in the `math` grounding
slice. No term, module, manifest, mapping or catalog changes.

A worked example, `examples/a2-acting-groups.ttl`, records three actions on the six A2 roots: by the full
automorphism group (order 12), by the Weyl group (order 6) and by its rotation subgroup (order 3). One
target has three actors, and for A2 the Weyl group is a proper normal subgroup of the automorphism group, so
the two roles that coincide for E8 are held by different resources.

Structural assertions pin the three actor obligations together; ban a property characteristic, a global
domain or range, and any sub-property, inverse, equivalent, chain or rule that would derive
`math:actingGroup`; require the E8 record to name `exa:e8Weyl` and no other actor and to keep both law
links; and require every authored action to name exactly one actor that is a group and is not the descriptor
itself. Each ban names a fail-witness fixture. Two competency questions over the asserted examples return
the E8 action's actor, target and law, and the groups acting on the A2 roots with their roles. Five
counter-example fixtures and one conforming cell are added. The guide and design text state what the
generated validation view covers and what only the declared-data controls catch.

Verified: independent review PASS at ab97cb3ec; the whole diff read. At that head on a clean tree: `make
lint` exit 0, `rapper -c` on the nine changed Turtle files exit 0, `prose-check` on both guides exit 0, and
each of the thirteen new query patterns declares every prefix it uses. Pull request checks: helpers, lint,
producer-build and rust-prebuild green; the three producer jobs and the aggregate red as on main, with the
producer killed on the runner and no error before the kill.

Not shown: nothing has executed the new material. No producer pass, harness, gate or reasoning run, and no
SPARQL parser run on the patterns; the questions, assertions, fail-witness runs and conformance cells are
worked by hand. GMN-1 coverage over the new example is not measured. The slice's quality floors are unchanged
by arithmetic, the module and catalogs being byte-identical to main. Main moved after the review without
touching this slice; nothing was re-run on the merged tree.
