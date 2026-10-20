# Issue #1796: Two authored existential cycles through inverse properties cannot be certified by a skolem-chase termination certifier

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: question

## Body

With the certifier artifacts of the companion issue removed, a class-level analysis of the slice ontologies (502 TTL files, 855 restrictions) leaves two genuine existential cycles. Both run through an inverse property pair:

- **Finance:** `JournalEntry ⊑ ≥2 journalEntryPostings.Posting` (`slices/…/finance/module.ttl` ~L150-157, ~L1169) and `Posting ⊑ ∃postingJournalEntry.JournalEntry` (~L238-243). The inverse is at ~L573/~L766; ranges at ~L569-571/~L762-764.
- **Music:** `PitchTrajectory ⊑ ≥2 trajectoryControlPoint.Thing` (music `module.ttl` ~L7218) and `PitchTrajectoryControlPoint ⊑ ∃controlPointOfTrajectory.PitchTrajectory` (~L568-573). The inverse is at ~L1218.

The engine's **restricted** chase terminates on these: the inverse edge back to the parent already satisfies the child's restriction. But every rung of the current termination ladder (weak, joint, super-weak and model-summarizing acyclicity) bounds the **skolem** chase, which does not terminate here. A scratch reproduction of the finance pair stays refused on the full ladder.

## Options
1. **A restricted-chase termination certifier** (RMFA/RMSA-style): a trigger counts as blocked when Datalog-first reasoning already satisfies it through the inverse edge. This admits these programs soundly, but it is new certifier capability.
2. **Change the modelling:** for example, state the child-to-parent link without an existential, or drop one direction's cardinality. This is an ontology decision.

Until one of these lands, production stage-reason cannot be admitted even after the artifact fixes. Caveat: the analysis covered slice TTL only, without property chains or nominals.

## Comments (1)

### paudley — 2026-10-04T05:59:50Z

## Decision and design: restricted-chase certification, done efficiently

Decision: admit these programs with a **restricted-chase termination certifier**, keeping the ontology as authored. It must be a general, efficient `logic:` capability, because restrictions paired with inverse properties are ordinary DL modelling.

### Precondition found: the engine is not Datalog-first
The joint materializer gathers **every** producer against one frozen round snapshot (`crates/logic/src/physical/seminaive/joint.rs`: *"Every producer reads the same cross-world frozen round"*). So, for finance:
1. Round *k* mints a `Posting` null `n`.
2. In round *k+1*, the inverse rule derives `postingJournalEntry(n, x)`. In that same round, `Posting ⊑ ∃postingJournalEntry.JournalEntry` is checked against the frozen snapshot, which lacks that edge, and mints a fresh `JournalEntry`.
3. That repeats forever.

Under today's strategy these programs **genuinely do not terminate**. A blocking-based certifier on top of it would be unsound.

### Plan
1. **Datalog-first restricted chase.** Within each stratum, witness-minting producers (existential and source existential rules, minimum and qualified-cardinality witness families, n-ary reifier heads) are gathered only in a round where the non-generative producers have reached a fixpoint. Every other round evaluates the non-generative producers alone.
   - It stays deterministic (sorted commit, unchanged) and semi-naive.
   - It mints a subset of today's witnesses, never more.
   - It is the standard strategy of production existential-rule engines, and the precondition for (2).
2. **Restricted joint acyclicity (RJA)** (Carral, Dragoste & Krötzsch 2017, already in `LOGIC-REFERENCES.md`) as a ladder rung after super-weak acyclicity:
   - It refines joint acyclicity's null-propagation edges with a blocking test. For an edge from rule ρ's existential to rule ρ′: instantiate ρ's body with fresh constants, add ρ's head with the null, and close that constant-size instance under the program's Datalog rules.
   - If every trigger of ρ′ on the null already has its head satisfied, the edge is blocked.
   - The cost is one small Datalog closure per rule pair, polynomial overall, self-hosted on the engine's fixpoint (as `LOGIC-PERFORMANCE.md` prescribes for the model-summarizing rung).
   - Sound under Datalog-first restricted-chase scheduling.
3. **Doctrine:** record Datalog-first scheduling and the RJA rung in the `LOGIC-PERFORMANCE.md` chase doctrine.
4. **Tests:**
   - finance-style and music-style inverse pairs certify under RJA and terminate under Datalog-first execution;
   - a genuinely cyclic program stays refused;
   - certify and execute agree on bounded runs;
   - mutation checks for each mechanism.

Generated outputs may change: Datalog-first mints fewer witnesses.

