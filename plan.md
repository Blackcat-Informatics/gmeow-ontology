# Restore ontology-linked model exports

Principles 4, 7 and 17: keep the shared compiled schema as the only structural
source and use PurRDF's routed renderer, with GMEOW-owned slice and term metadata.
The existing flat emitter drops navigation already supported upstream. Recover
slice modules, class definition digests, documentation coats and links, package
README, and total routing of synthesized/shared schema definitions. Remove stale
flat-layout assumptions and obsolete renderer snapshots within this scope.

## Task 1: Source and render the metadata

Use the existing documentation discovery path with this run's term manifest and
constraint catalog products, never stale generated files. Declare both edges in
the stage, executable spec and ontology DAG. Include documentation source inputs
and bump the stage implementation version. Route actual ontology classes via the
shared schema namespace codec; assign generated schema helpers an explicit shared
module without fabricating ontology identities. Missing class ownership or digest
must fail. Emit the package README from the same selected topology.

## Task 2: Verify the actual renderer and consumers

Run synthetic renderer controls for multiple slices, shared/helper definitions,
metadata, documentation, cross-module references and missing metadata refusal.
Update authenticated package-consumer assertions for routed output. Remove the
three orphan snapshots from the retired renderer once their contract is covered
by live tests. Run focused Make validation and normal signed hooks. Do not run
local optimized producer, fixture production, sync or full gate.

## Task 3: Publish, integrate and clean

Publish and merge sequentially through ghprsq under the standing CI waiver.
Preserve the parent issue: actual generated corpus acceptance and the live Python
import/schema lane are not established by Rust structural checks. The issue's
comments explicitly reserve new Python test/orchestration surface for separate
authorization. Do not create that surface or misrepresent parsing as import.
Document the current SHACL-derived developer-schema boundary rather than silently
inventing closed-world constraints for OWL/RBox-only properties. Preserve the
existing JSON-LD/CSVW contracts; no unrelated upstream emitter reimplementation.

No independent reviewer tool is exposed; no independent review is claimed.

Implementation: routed topology is wired through both production entries, with
fresh product reads in the pipeline and materialized reads only in the explicit
post-pipeline entry. Source identity includes docs and slice manifests; both
affected stage versions advance. Namespace admission uses the shared registered
namespace authority for both term enumeration and prior-release validation.

Five new controls passed: routed models and cyclic cross-slice references,
generated enum/envelope ownership, missing metadata refusal, route collision
refusal, executable/RDF DAG agreement and four-namespace manifest coverage. The
initial build caught an invalid public term import and one removed constant's
test reference; both were repaired. The first runnable fixtures mistakenly used
logic:Type instead of logic:Class; corrected fixtures pass with assertions intact.
All seven existing manifest history controls and strict producer-independent
Clippy passed. Corpus-purity checks, live producer-recipe source admission and
normal signed commit hooks passed. No source changes followed qualification.

The checked-in generated Python package remains unrefreshed because local corpus
production and synchronization are outside current execution authority. Source
implementation and synthetic renderer checks do not establish a shipped wheel,
whole-corpus generation or live Python runtime acceptance. The parent stays open.

Final integration review: no concurrent base delta and a clean merge-tree prediction. Source is committed and pushed. No inline findings or submitted reviews at inspection. CI is running and automatic review is pending; both are waived for integration by the explicit sequential-merge direction. No independent audit or whole-corpus acceptance is claimed.
