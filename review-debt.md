# Review debt: pull request 2755

Scope: every review comment on the pull request and every finding of the independent reviews, with what was
done. The review bot's text is not reproduced here; each item is summarized in my words and identified by
its comment id. Evidence: the pull request's comment threads; the review files in this directory.

## Review bot (ten inline comments, three reviews)

Review of `85cef27fe`:

| Comment | Summary | Resolution |
|---|---|---|
| 4570367510 `loader.rs` | readers of the span table counted by an exact match, missing a consumer that declares the whole carrier | addressed in `76ce31834` (counted by the coverage relation); reply 4571881190 |
| 4570367544 `reads.rs` | the contract of a whole-lane artifact declaration for internal artifacts was undecided | addressed in `76ce31834`: fail closed, committed artifacts only; one edge now names its six dictionary artifacts; reply 4571881442 |
| 4570367558 `scheduler.rs` | a dataset read on a released product panicked in a worker and could leave the scheduler waiting | addressed in `76ce31834`: the read is recorded before the assertion, a worker always delivers a result; reply 4571881631 |
| 4570367567 `module.ttl` | new declarations authored with `rdfs:` spellings | addressed in `76ce31834`: `logic:subClassOf`, `logic:domain`, `logic:range`; reply 4571881827 |

Review of `dbbd77181`:

| Comment | Summary | Resolution |
|---|---|---|
| 4571644711 `module.ttl` (major) | the new terms had no French or Chinese catalog entries; the translation floor at stake | addressed in `95060c030`: an entry in each catalog for all 262 added literals; floor unchanged; measured 0.6275 against 0.3957; reply 4573075795 |
| 4571644701 `node.rs` | under the recording policy an undeclared internal artifact read through a whole-lane accessor is hidden and not recorded | declined here (recorder only); issue 2758 item 28; reply 4572866263 |
| 4571644705 `scheduler.rs` | a stage failing under the recording policy does not name its undeclared reads | declined here (recorder only); issue 2758 item 29; reply 4572866349 |
| 4571644709 `archive_blobs.rs` | a helper records a whole-carrier read for one representation; no current edge affected | declined here; issue 2758 item 30; reply 4572866390 |

Review of `492245596`:

| Comment | Summary | Resolution |
|---|---|---|
| 4572680374 `node.rs` | `artifacts()` cloned every payload before filtering | addressed in `95060c030` (`bundle_artifacts_where`); reply 4573076040 |
| 4572680381 `scheduler.rs` | the recording policy with drop-after-last-consumer releases producers; no shipped path | declined here; issue 2770; reply 4572866470 |

The bot has not reviewed `95060c030`.

## Independent reviews

- First reviewer, by head: at `85cef27fe`, `dbbd77181`, `68272e7e4` and `70557e49a` BLOCKED, each time on
  findings fixed in the commits that followed (a sweep of every statement the change adds found 17 false
  ones after the third round); at `e447d5f70` PASS; at `492245596` BLOCKED on three statements the fix made
  false (fixed in `49b8cff1a`); at `49b8cff1a` BLOCKED on one sentence (fixed in `4dac0f9b8`); at
  `4dac0f9b8` PASS; at `95060c030` PASS, its readings covering `e447d5f70..95060c030`. Files: one per head,
  `review-2755-at-<head>.md`, and `truth-sweep-2755-at-68272e7e4.md`.
- A second reviewer at `e447d5f70`: BLOCKED on one required finding, the byte copies at dispatch (fixed in
  `492245596`); its eight follow-ups are on issues 2758 (items 22 to 26) and 2770.
- Follow-ups of the passing reviews, none required: on issues 2758 (item 27), 2770 and 2776.

## Open after this change (required work, not done here)

Issues 2753 (cached stages are not compared), 2758 (read-comparison gaps, items 1 to 30), 2770 (release
copies, retention and test gaps), 2773 (fixture-backed tests), 2776 (the slice's testing and harvest scores;
one French rendering; the catalog entries read only by their author).
