# Independent cost/soak JSON source review

Reviewed the three-file inventory in `validation.md`, old/current cost reader
diff, both actual report functions, and current benchmark producer branches.
No implementation, formatting, tests, checks or producer activity by this reviewer.

## Admission gap: container nonvacuity does not establish ledger evidence

`CostArtifact::validate` checks nonempty case/ledger containers, exact raw
case_count and nonempty unique case identities. It still admits an empty ledger
corpus key, an empty `finding_graph_blake3`, and empty engine pin strings. A
nonempty ledger map can also contain a zero-comparison tally. `render_soak`
then checks only nonzero gap counts and folds whatever corpus/digest strings
were supplied before claiming gap-zero HELD. For example, replacing the authored
SIMPLE ledger with zero tally counts and an empty graph digest passes the current
source checks despite carrying no comparison or finding-graph identity evidence.

This permissiveness predates the native migration; it is not a newly introduced
codec regression. It does limit the new nonvacuity/identity admission claim at the
touched shared boundary. The producer (`bench-engines/main.rs:285–329`) groups
actual case comparisons and emits their actual graph digest. A narrow follow-up
should distinguish missing identities/empty comparison evidence from legitimate
values, with authored refusal controls. It must **not** equate comparison tallies
to raw case counts, invent graph digest recomputation, or force new globally
mandatory optional panels. No digest-format requirement is inferred here.

## Reviewed behavior without an additional finding

- All 13 records retain their declared types through native Record admission:
  known duplicates refuse, unknown fields remain ignored, and optionals preserve
  missing/null behavior. Both readers share parsing/admission; no live engine or
  alternative Serde decoder remains in this family.
- Unsigned native admission explicitly uses `as_u64`, with checked u32 bounds.
  Signed overheads use `as_i128`, retaining integer extrema and signed-zero
  semantics. Four-slot vector tuples preserve order and compose native field/
  item/slot pointers; fractional/exponent/overflow/coercion paths refuse.
- Actual producer branches match the selected count policy: backward answer
  count, and derived count for forward, existential and both incremental forms.
  Incremental producer branches supply scratch, joined rows and both parity
  observations; grounding supplies the additional scratch probe/rule fields.
  Those requirements and optional-present selected panel checks do not invent
  absent zero/false measurements. Legitimate nullable proof height renders an
  em dash, separate from measured zero.
- Existing four-worker structural validation is preserved. Cost ledger still
  displays divergence evidence, while soak still refuses nonzero gap tallies.
  Sorted case/map order, integer rendering/arithmetic and exact soak digest
  framing (`corpus + US + stored graph digest + LF`) are unchanged.
- The 13 authored controls are source-eligible: plain JSON and temporary files
  reach actual report functions, never a production stage, committed baseline,
  benchmark, engine or corpus loader. Full literal reports and the independently
  supplied tiny-byte soak hash make useful assertions; numeric/tuple/profile/
  duplicate/refusal controls exercise the touched implementation. Existing real
  baseline tests remain outside this authored-input qualification.

No executable failure or pass is claimed by this source review. Root owns
compilation/focused qualification. Actual committed baseline, source-stage byte
parity, selected production acceptance and broader gates remain unrun under the
recorded exception. This review grants no additional waiver or broader acceptance.

## Producer proof for the bounded admission repair

Confirmed every successful fragment outcome contributes comparisons, unchanged
when returned: forward has one (`main.rs:587`, returned at 802), existential one
(1410/1447), backward two (1511/1573), incremental four (898/1031), and
incremental-grounding five (1170/1344). `run_case:463` dispatches exactly these
five branches. Each emitted record names the same `case.corpus` used by its
grouping key.

The producer loop at 275–287 pushes each record and unconditionally extends the
comparison group keyed by that same corpus. The loop at 291–329 emits exactly
one ledger for every grouping key, without filtering or extra entries.
Therefore the ledger corpus-key set equals the case-record corpus set, and
each ledger contains at least one comparison. The canonical
`conformance/divergence.rs:290–317` tally sets `cases = comparisons.len()`
without deduplication/filtering, directly justifying a strictly positive tally.

Missing ledger keys omit selected case evidence from the soak scope; extra keys
claim evidence outside the selected inventory. Exact corpus-set equality is
thus a source-proven invariant. Raw ledger-count equality with case count is
not: fragment multiplicities are one, one, two, four and five. No new producer,
measurement or runtime check was invoked to establish this source proof.

## Narrow repair disposition

Re-reviewed the frozen shared validation repair, two authored controls and updated
contract. The blocking admission gap is addressed in source: both engine pins,
every ledger key and stored finding identity must be nonempty, each comparison
tally must be positive, and the ledger corpus-key set must exactly equal the
case corpus set. Both actual readers reach this validation before projecting any
claim. The expected/found scope diagnostic is actionable.

The implementation adds no raw-case/tally equality, agree-count requirement,
digest format rule or recomputation. Legitimate zero agreement/divergence
evidence remains expressible; opaque nonempty graph identities remain accepted.
The original parallel validation stays mandatory. The new controls exercise
both readers for empty/vacuous inputs and directly exercise renamed, extra and
missing ledger scope. Existing authored expectations remain unchanged.

No further concrete finding in this bounded repaired source. Root reported the
original 13 controls passed; the expanded 15-control rerun was pending at this
disposition. This reviewer ran no checks and makes no production acceptance claim.
