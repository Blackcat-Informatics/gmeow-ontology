# Issue #1792: Case-split class analysis is quadratic in restriction descriptors during materialization

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug

## Body

## Observation
During native materialization, the joint class producer's `PreparedClassAnalysis::evaluate` (`crates/logic/src/reason/refute/casesplit/execution.rs`, reached from `families.rs` `FamilyState::round`) dominates stratum time, and its cost grows roughly quadratically with the number of restriction descriptors in the input, even when none of the restrictions has an instance.

Measured in `gmeow-logic` with N qualified-minimum restrictions (each with its own `onProperty` and a shared `onClass`, `minQualifiedCardinality 1`) plus one instance-bearing restriction. These are dev-profile test timings of the class evaluate call in the first stratum-1 round:

| restrictions | class evaluate |
|---|---|
| 1,250 | 0.8 s |
| 2,500 | 5.1 s |
| 5,000 | 17.3 s |

For the 5,000 case, materialize took about 32–67 s in total, while every other gather phase (sources, ordinary rules, properties, existentials, obligation families) took milliseconds per round. Later rounds re-evaluate in about 4.5 s each.

The production corpus carries about 6,540 `logic:onProperty` and 5,450 `logic:onClass` rows (#1789), so this is expected to cost tens of seconds per reasoning run once production reasoning is admitted.

## Next
Profile `prepare` and `evaluate` (premise rebinding through `native_support`, `scan.ingest` and `prepare_schema`, and the case-split model work) and remove the per-restriction rescans. Results must stay exact.

## Comments (0)

