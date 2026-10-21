# Owned source-capability and helper repairs

Based on main after transcode PR1906. This reused finance issue lane
repairs introduced gate defects after the financial implementation landed.
Original financial corpus/generated/installed acceptance remains mandatory and
open. This repair is not evidence that those requirements pass.

## Concrete failure evidence

- Hosted finance run39565507443, helpers job119106691430, and transcode run39566072124,
  helpers job119108532651: isomorphic diagnostic constructors in finance/math and
  rdf-collection21 copies versus debt19. The parent reproduced both via helpers.
- Hosted transcode run39566072124, producer-build job119108532904: json_record!
  has no selected source-input capability owner in transcode/src/lib.rs.

## Coherent repairs

1. Records::list and owner import consume pinned purrdf_core::walk_rdf_list,
   the same strict owner used by DatasetView::rdf_list_strict. The callbacks
   retain visited cells while the owner determines traversal and membership.
   This is the actual pinned API; rdf_list_with in census guidance is stale.
   Strict malformed-list behavior and the finance named-record/member policy
   remain mandatory. Required selected anonymous/quoted edges still refuse.
2. Exact collection-census.log corrected the initial attribution: the second
   introduced copy is Records::admit_owner, not the synthetic owner builder.
   Owner import now expands list cells and member records through the shared
   strict walk instead of its generic first/rest closure traversal. Full cell
   records, positive proof premises, unsupported-field refusal and authenticated
   owner override checks remain mandatory. Effective merged lists are also
   strictly checked, including rdf:nil. The synthetic builder was modernized
   through purrdf_core::build_rdf_list but was not the remaining census finding.
   No token relocation, marker, ceiling or exception hides the owner traversal.
3. One error::profile_result constructor owns the diagnostic kind/format, with
   math and finance profile identities supplied as data by both real callers.
4. The JSON macro capability requires the actual selected external purrdf-lex
   package, declared dependency alias and matching full Git revision/lock object
   from the canonical substrate repository. Portable ownership proves the same
   source from unambiguous declarations and the lock. No local/path/floating/fake
   package, conflicting alias/target, replacement or malformed package grants it.
5. Only external-prelude ::alias::json_record! and ::alias::json_string_enum!
   receive that capability. Existing nested-input traversal, opaque/module-input
   refusal and extern-crate shadow controls remain active. Transcode uses the
   qualified path; no bare-name allowlist or suppression was added.

No ontology, ONNX fixture, manifest pin, gate flag, helper debt/ledger or ceiling
changes belong to this repair. Parent owns sequential qualification, integration
and delivery. This source lane runs no formatter, checks, builds or producer.

See validation.md for the exact unrun coverage and required qualification.
