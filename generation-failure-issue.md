## Observed failure

Both generation jobs on PR #1827 head ee21d2c324f9331f3a6bc8c40196cccdcb2475d3 failed after the authenticated optimized producer build passed.

Run: https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/38938288117

Jobs 117114464065 and 117114464074 report:

`sync pipeline failed: recovery evidence has no executable resolved leg pair`

The failure prevents generated-corpus qualification and skips downstream validation. Raw job logs are retained with the PR's selected Stage evidence.

## Required repair

Trace the recovery evidence and resolved correspondence legs through the actual production path. Repair the missing executable pair without weakening evidence requirements or silently omitting a recovery law. Add a focused regression and demonstrate both generation lanes and dependent validation pass on the repaired source. Principles 4, 7 and 18 apply.

The user explicitly requested immediate ghprsq integration and issue ownership for CI findings. No passing whole-pipeline result is claimed.
