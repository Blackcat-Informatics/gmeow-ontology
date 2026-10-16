The consolidation audit found that this per-world certificate patch is distinct
from the six other project PR heads already contained in PR #1802. It has now
been integrated into the combined worktree intended for #1802, preserving the
current native proof and ownership changes. This is local, uncommitted work;
the published head has not changed yet.

The valid CodeRabbit finding in discussion_r4176356480 is addressed in the
candidate: ChaseCertificate now explicitly documents when its admission covers
only the world's authored rules, excludes schema laws and foreign rules, and
when it instead records the joint execution admission. The reason stage version
is advanced for the changed certificate outputs. The complete review surface
contains one unresolved thread; it remains unresolved until the fix is published.

Current validation: source lint and producer-independent Clippy both passed
after this integration, with warnings denied. The new source-rule, minimum and
published-demonstrator consumers are still compiling; they have no runtime
verdict yet. The earlier three corrected native cases plus their minimum-witness
control passed 4/4. Four browser package builds also completed successfully, but
are candidate packages requiring freshness and parity qualification.

The normal optimized producer build is running for the authenticated bundle and
codebook bootstrap. Full corpus validation, shipped-bundle certificate goldens,
all browser parity checks, hosted CI, current-head review and ghprsq merge remain
required. This PR remains open while its distinct change is incorporated and
qualified; no merge or issue completion is claimed.
