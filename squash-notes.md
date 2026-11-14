Standpoint: add a per-ranker strict order between standpoints, as dated attributed claims (Principles 9, 10, 17)

Adds gmeow:moreEntrenchedThan: one party ranks one standpoint as more settled than another. The order exists
only as dated claims attributed to the ranker, with no global transitivity or asymmetry, so two rankers'
claims never chain into an unattributed edge. Three logic:Constraint records enforce it over the claims'
reifiers: an edge needs its ranker and its date; one ranker may not hold a pair in both directions; and one
ranker's live claims are closed under transitivity, which with the other two rejects a cycle of any length
among live claims. A claim that is succeeded, corrected or retracted is not live. Irreflexivity is recorded.
Usage guidance and the slice guide state one rule for a displayed and a historical order.

The characteristic-shape stage now reads a property's target class from logic:domain, with rdfs:domain read
where a slice still carries it, and refuses a property that declares two different ones.

Verified: independent review PASS at 98bf53bad and at the merged head d402e3b72; rapper, msgfmt, the
issue-reference lint and the producer recipe exit 0; format and strict clippy on the pipeline crate exit 0;
the crate's failing tests are the known fixture refusals. Hosted helpers, lint, producer-build and
rust-prebuild green; the three producer jobs and the aggregate red as on main. No producer run was made.

Closes #2685.
