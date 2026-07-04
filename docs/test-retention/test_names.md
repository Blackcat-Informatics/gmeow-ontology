# Retention: `tests/test_names.py`

**Category:** Merged-graph guard

## What it tests

Structural + DL-safety guards for the names building block.

Retained dynamic tests:

- `test_place_naming_is_defined_class` — PlaceNaming reuses the NameUsage relator as a DEFINED class (≡ NameUsage ⊓ ∃usageNamed.
- `test_seeded_pronoun_sets_have_five_forms` — Every declinable anchor is a PronounSet filling ALL five forms.
- `test_pronoun_name_only_value_exists` — An explicit no-pronouns / name-only value exists, distinct from any/ask and carrying no five forms by design.
- `test_contested_name_usage_coexists` — Two standpoint-indexed NameUsage claims on the same person load, SHACL-pass, and are BOTH retained — neither is the ground truth.
- `test_audience_and_standpoint_are_distinct` — usageAudience (social scope) is not bridged to accordingTo (standpoint frame).
- `test_appellation_umbrella_and_structural_subclasses` — Retained dynamic test.
- `test_has_title_subproperty_of_hasappellation` — hasTitle is the creative-work-scoped specialization of hasAppellation , giving CreativeWork multilingual Appellation-based titles.
- `test_has_software_name_subproperty_of_hasappellation` — hasSoftwareName is the software-scoped specialization of hasAppellation , domain-free so it can attach to both SoftwareProject and SoftwareProduct.

## Why it cannot be deleted or moved to Rust today

Traverses OWL intersection lists using rdflib `Collection`; the `equivalentClass` body is an anonymous blank node that cannot be faithfully expressed as a single ASK without re-encoding the blank-node restriction inline. Iterates over a 21-item `_DECLINABLE_PRONOUN_ANCHORS` tuple with per-anchor label + five-form checks; a dynamic numeric sweep over live ABox data. Checks label presence via `graph.value()` and five-form absence on `_NON_SPECIFYING_PRONOUNS`; ABox / numeric live-data checks. `run_shacl()` on a fixture file; `ExampleConformance`, not a structural TBox assertion. Checks `gmeow:accordingTo`, which is defined in a cross-slice module (not home-asserted in names); the merged-graph sweep ensures neither direction is accidentally wired.
