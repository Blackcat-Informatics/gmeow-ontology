# Plan: re-land the declared pipeline graph (pull request 2755; part of issues 2697 and 2724)

## Behavior
The build pipeline's stage graph is loaded from the pipeline slice, each dependency edge declares what its
consumer reads, and the scheduler compares recorded reads with declared reads under the enforcing policy.
The change merged as pull request 2752 and reverted by 2754 is submitted again, checked and read before it
merges, with what review found corrected.

## Dependencies
Pull request 2762 (the helpers repair) merged first. Base: `origin/main` `04aa28a7e`.

## Exclusions
- Cached stages are not compared with their declared reads (issue 2753).
- The producer's memory use is not this change's subject beyond removing what the change itself adds; the
  memory work follows on `main` (issues 2696, 2770, 2774).
- The content gate that fails a full pass on `main` (issue 2745).
- Declared writes, a plan phase, constraints on the description, reads in receipts.
- The read-comparison gaps and review follow-ups recorded on issues 2758 and 2770; the fixture-backed tests
  (issue 2773).

## Acceptance
- Better than `main`, and not worse than `main` in anything `main` does today.
- Every hosted check finished and each failure read; no check that passes on `main` fails here.
- An independent review passed at the exact merging head, its coverage in a file; the final reading of the
  merging diff by the primary session.
- One cold, full-scope, enforcing producer pass at the exact merging head, compared with a pass of `main`
  made the same way: every stage run, no read difference, the same final gate, the peak memory, the two sets
  of findings and the count of their difference. A finding `main` does not produce on unchanged input blocks.
- Every item moved from blocker to issue is filed as required work and listed in the squash notes: 2753,
  2758, 2770, 2773.
- The description states only what was run and shown.

## Risk
The scheduler hands products between concurrent stages; a wrong release or a wrong view fails late or holds
memory. A statement in a comment, a document or the slice that the code does not keep is itself a defect
here. No ceiling, baseline or recorded debt is raised.

## User rulings
- 2026-11-13 (session start): "You are forbidden from skipping skips, breaking trust, commiting fraud, lying
  about your work, hacking around quality gates - all things your predecessor did."
- 2026-11-13: "YOu won't be able to fix CI without redoing 2752."
- 2026-11-13: "I restored the remote branch: pipeline-declared-graph"
- 2026-11-13: "The previous agent also decided to break up the work into a seriese of commits - that might not
  be viable to restore CI.  I'm granting you the option of merging this work even with CI failing ONLY as the
  goal is to get the CI working.  Not as the previous agent put it \"Why did you not run CI? --  I skipped it
  because I didn't feel it was needed.\""
- 2026-11-13: "Yes. The graph work was literally to fix the CI, and it's a LARGE chunk of work.  I don't want
  that interpretted as a blanket \"no need to check anything\"."
- 2026-11-13: "I'm most worried that the agent ignored a lot of known issues on those forced merged branches.
  It's possible the best strategy is to remerge 2752, and then go back and address the issues with targetted
  fixes."
- 2026-11-13 about 23:00 local, the merge bar: "Just to be clear though - we should merge 2755 even if it's not
  the _complete_ solution as long as it is a BETTER solution than we have - we can always iterate on later
  issues." Then: "That's not a request to merge." "Just a note about the criteria we use to evaluate the
  merge." "I just won't want us to end up chasing the memory issue for instance in this branch - I would
  rather we get something working into main, then attack the performance and memory issues (of which there
  are MANY!)." "Peak-memory is the right issue - the CI cannot run if it OOMs."
- 2026-11-13: the audit terms listed under Acceptance (the cold pass on the exact merging head, both sets of
  findings compared, moved items filed as required work) were confirmed by the user as the standard.
