PASS at 173a4ff75

# Delta review: canonical profile pin, 07a001de0..173a4ff75 (branch paudley/2823-identities-are-minted-from-canonical)

## Scope and state

- `git rev-parse HEAD` = 173a4ff75ec3f27bfbe3b7a318c46e982e2b768d. `git status --porcelain` is empty. The head equals `origin/paudley/2823-...`: `git diff --stat` against it is empty.
- `git log 07a001de0..HEAD` holds one commit. `git diff --stat 07a001de0..173a4ff75` touches one file, `docs/GATE-AND-PIPELINE.md` (+6 -5).
- `git diff --word-diff 07a001de0..173a4ff75` shows that the only inserted words are "and quantified-variable", between "as argument" and "nodes". No word was removed. Every other change in the hunk is line wrapping within the same paragraph (lines 137-150).

## Coverage of the whole branch

The verdict covers the whole branch. It rests on my reviews at ed0989d8c, b235e25db, fb3e62334 and 07a001de0, plus this delta. This delta adds two words to one sentence and rewraps its paragraph. Nothing those reviews judged changed otherwise, so their judgments stand.

## What I read

- `git diff` and `git diff --word-diff` for `07a001de0..173a4ff75` (whole), and the commit message of 173a4ff75.
- `docs/GATE-AND-PIPELINE.md` 136-152 at head, with line lengths.
- From the earlier reviews, still current because no code changed: `crates/reason-wasm/tests/support/conjecture_witness.rs` (whole), `crates/logic/src/conjecture_eval.rs` 120-340 and `crates/logic-compile/src/ir.rs` 2450-2476.
- I ran `prose-check docs/GATE-AND-PIPELINE.md` (exit 0).

## The changed sentence

"The conjecture formulas do hold blank nodes, as argument and quantified-variable nodes, and those are read into a formula over IRIs and variables, whose terms have no blank-node form."

- This is exactly true of the two formula inputs.
  - `PROOF_FORMULA` holds two blank nodes, both `logic:argument [ … ]`.
  - `REFUTE_FORMULA` holds five: one `logic:quantifiedVariable [ … ]` and four `logic:argument [ … ]` (conjecture_witness.rs:12-43).
  - These seven are all the blank nodes in the two documents, and no other kind occurs.
- The rest of the sentence is unchanged and still true. `parse_candidate_formula` reads each document into an IR `Formula` (conjecture_eval.rs:128-182). The quantified-variable node becomes the variable `x`. IR `Term` has no blank variant (ir.rs:2450-2476).
- O1 from 07a001de0 is resolved.

## Rest of the paragraph

The word diff shows the other sentences unchanged word for word, so none changed meaning:

- the substrate sentence;
- "for two different reasons";
- the closure-input and knowledge-base sentence with its "not enforced" caveat;
- the producer and copy sentences.

The longest line is now 83 characters, matching the neighbouring lines, so O2 from 07a001de0 is resolved. The code spans are intact: `` `make maint-refresh-reason-witness` `` now opens line 146 whole.

## Findings

### REQUIRED

None.

### OPTIONAL

None new. These are carried from fb3e62334, outside this delta and unchanged:

- O1: doc 112-113 exception wording versus row 123;
- O2: `canon_profile.rs:24`, 60 and 65-66;
- O3: corpus message pointer form;
- O4: host cargo wrapper errors and the missing `--manifest-path`;
- O5: `pub` constants.

## Not shown

- No evidence log names 173a4ff75. `make lint` and the gts-profile tests were last recorded at fb3e62334. This delta is a prose-only hunk, and I re-ran `prose-check` on the file myself.
- No `make check` or producer run. Nobody has shown that the branch leaves `generated/` and `gmeow.gts` byte-identical.
- No witness refresh. Profile independence rests on reading the code.
- Whether PurRDF's Turtle parser assigns labels derived from the canonical profile.
- No compilation of the dependent crates beyond what earlier reviews recorded, and no integration with the current `origin/main`.
