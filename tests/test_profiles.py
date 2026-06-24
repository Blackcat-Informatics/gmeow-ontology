"""Tests for the Profile meta-pattern (issue #75).

The TBox structural assertions (the profile class/meta-property/seed-profile checks)
have been migrated to slices/core/profiles/tests/structural.ttl as declarative
gmeow:StructuralAssertion cells (cells ex:saProfileClassExists through
ex:saTemporalProvenanceProfileExists). The run_shacl / ExampleConformance tests
have been migrated to crates/validate/tests/conformance_profiles.rs (#867).
"""

from __future__ import annotations
