# Project PR consolidation

Live GitHub inventory: 13 open PRs, comprising five Dependabot updates and eight
project PRs. No new PR was created in this continuation. The integration target
is PR 1802. Closing PRs is not evidence of merging their requirements.

## Incorporated source heads

The following exact remote PR heads are ancestors of the current PR 1802 head
`b797a315e09cf6a118e0ca60975007a65f43a138`:

| PR | Head | Published base |
| --- | --- | --- |
| 1793 | 51b7abd40cbdccf79c525086a853a2ec132ffc1d | main |
| 1798 | 12e5c4f8a395971c49126dbcdb6d822eb04f635f | main |
| 1799 | a465c997012fd77020dd692d7ea87f9c5d0c8e87 | issue-1796-datalog-first |
| 1800 | 4e7c7f2a7982004d21310002eb2cb20162a611e6 | issue-1757-snapshot-critical |
| 1801 | 92068d6292b81aefe620564ffbbfbb59061b78b1 | main |
| 1804 | a794c70b3cba86870c41a302815fe92c7a4e09c7 | main |

After the combined change qualifies and lands through ghprsq, explicitly account
for these PRs and their issue/review requirements before closing any remaining
superseded PR. Preserve the associated evidence and distinguish consolidation
from an independently qualified merge.

The complete captured review surface and dispositions for all six incorporated
heads are now indexed in incorporated-pr-review-debt.md. It retains two required
performance findings and all skipped/limited-review caveats. Ancestry does not
clear that review debt.

## Distinct demonstrator work

PR 1794 head `325c7b25843b55ff54667ca262a0bb43b72c480f` is not an ancestor and
`git cherry` identifies a distinct patch. Its complete five-file patch is retained
in raw/pr1794-distinct-source.patch. All four nonconflicting files were applied
after a successful scoped apply check. The program publication changes were
adapted to the current boxed diagnostic closure without discarding that layout
repair. The exact ObjectProperty/rdf:type witness fixture repair remains intact.

The CodeRabbit review and inline comments are captured in
raw/pr1794-reviews.json and raw/pr1794-review-comments.json. The one actionable
comment, discussion_r4176356480, correctly identifies ambiguous ChaseCertificate
scope documentation. The candidate now states the standalone authored-rule
scope, exclusions, and joint-admission alternative explicitly. The reasoning
stage implementation version is advanced for the changed certificate outputs.

This integration is currently uncommitted and unpushed. The original 45 focused
consumers passed; they were refreshed after the runtime review repairs and again
pass 45/45, raw/review-optimizations-certificate-consumers.log, nextest
dff7ae54-1c8b-4b71-bcf5-5fc081315a3c. Full production qualification, current-head
CodeRabbit review and merge remain required.
