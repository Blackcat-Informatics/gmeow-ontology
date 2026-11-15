feat(evidence): an assertion category on a claim, related to the existing evidence values (Principles 2, 4, 9, 10)

A consumer that publishes claims as statement reifiers needs to say, on the claim itself, what kind of
assertion it is. `gmeow:assertionCategory` is one non-functional object property with a range and no domain,
so it applies to a bare statement reifier and to a claim record alike; its values are the three individuals of
`gmeow:AssertionCategory`: self-description, published evidence and derived interpretation. A category
classifies a claim and never ranks, hides or supersedes it; a categorisation is itself a claim and can be
attributed to its standpoint. The property is not a rank, a trust level or a display flag, and is distinct
from `gmeow:hasEvidenceClass`, which grades one citation.

The values are related to the existing vocabulary by eight native alignment cells in the slice's mapping
source: self-description to `evidenceSELF` (close), `evidenceSelfControlledSite` (narrower) and
`sourceIndependenceSelfOrIssuerOriginated` (broader); published evidence to four evidence classes that name a
kind of published source (narrower); derived interpretation to `observationTypeDerived` (close). The
self-description value points at `gmeow:selfAsserted` with a see-also. The relations hold between values:
nothing is derived between a claim's category and the evidence classes of its citations, a structural
assertion forbids a property bridge, and the worked example has a self-description whose citation carries the
official-source class. Reuse of the existing properties on a reifier was tried first and rejected for stated
reasons. The slice now depends on `gender` and `observations`.

A worked example, competency questions (including one that overlays the mapping source and pins the exact
eight cells), structural assertions, guide text in both slices, and new French and Chinese catalogs for the
new literals.

Verified: independent review PASS at f807002e4 after four rounds, the last two answering a review finding
(links moved from the module to the mapping source) and a slice-quality floor the earlier rounds had missed.
The module, manifest, example, tests, guides and the cells were read; the catalogs were sampled and every
`msgid` compared by the reviewer. At that head `make lint`, the Turtle parses, the catalog comparison and the
guides' prose check pass. The slice's quality floors hold by arithmetic from copies of the scoring code
(prose 0.8333 against 0.8, translation 0.4082 against 0.3333, documentation 0.75 against 0.75). Pull request
checks: helpers, lint, producer-build and rust-prebuild green; the three producer jobs and the aggregate red
as on main.

Not shown: no producer pass, harness, gate, mapping compile or reasoning run. The competency questions and
structural assertions were worked by hand; that the mapping compiler accepts cells between two GMEOW
individuals is read from its source; the generated rows and correspondences have not been produced; the
ownership check on the two new dependencies, the translation lint on catalogs that cover only the new
literals, and GMN-1 coverage are unproven or unmeasured.

Closes #2800.
