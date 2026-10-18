All three CodeRabbit findings are repaired. The constant and documentation now distinguish the default floor from explicit development overrides, and name the actual CI acceptance matrix branch.

The outside-diff correctness finding was valid: an absent addressable denominator returned 100%. The aggregate gate now rejects an unmeasured corpus even with a zero floor, and its structured corpus verdict agrees. A regression drives the real per-file round-trip gate over an OWL-linkage-only dataset, then checks both aggregate verdicts at the default and zero floors.

Nine focused native contracts passed. Strict producer-independent all-target Clippy passed with warnings denied, as did all applicable normal commit hooks. Fixes are committed and pushed. Full hosted pipeline qualification is still running; no local corpus producer or current-corpus recall measurement was invoked. Principles 4, 7 and 18 apply.
