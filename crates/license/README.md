# gmeow-license

The RUST-FIRST single source of truth for GMEOW's license-token policy classifier.

A pure, dependency-free classifier over SPDX-ish license identifiers. One algorithm,
two named consumers:

- **`gmeow-conformance`** — whether a third-party test corpus may be *vendored* into
  `cases/external/`.
- **`gmeow_tools.config.LinkPolicy`** (Python surface) — whether an external
  vocabulary's *axioms may be copied* into the CC-BY-published GMEOW ontology. The
  Python side is a thin marshalling shim over the PyO3 `license_policy_for` entrypoint
  (in `gmeow-validate`), which delegates to [`policy_for_license`].

The classifier is conservative and fails safe: a restrictive marker (NC/ND/SA/GPL/…)
anywhere in the token forces `ReferenceOnly`, and an unknown license defaults to
`ReferenceOnly`.
