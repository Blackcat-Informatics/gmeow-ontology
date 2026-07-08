# Retention: `tests/test_sensory.py`

**Category:** Python tool algorithm — **migrated to native Rust**

## What it tests

Mapping tests for the Sensory module -- SOSA / AFO alignments only.

Migrated to native conformance twins (`crates/validate/tests/conformance_sensory.rs`), each
asserting against the generated SSSOM projection (`generated/mappings/*.sssom.tsv`) rather than the
authored `equivalences.ttl` cells:

- `test_sensor_mapped_to_sosa_sensor` → `conformance_sensory.rs::sensor_mapped_to_sosa_sensor`
- `test_sensor_platform_mapped_to_sosa_platform` → `conformance_sensory.rs::sensor_platform_mapped_to_sosa_platform`
- `test_observable_property_mapped_to_sosa` → `conformance_sensory.rs::observable_property_mapped_to_sosa`
- `test_sensory_quantity_mapped_to_sosa_result` → `conformance_sensory.rs::sensory_quantity_mapped_to_sosa_result`
- `test_sensory_property_mapped_to_sosa_observed_property` → `conformance_sensory.rs::sensory_property_mapped_to_sosa_observed_property`
- `test_platform_location_mapped_to_geo_location` → `conformance_sensory.rs::platform_location_mapped_to_geo_location`
- `test_sensory_afo_mappings_exist` → `conformance_sensory.rs::sensory_afo_mappings_exist`

## Migration note

The six SOSA twins scan the whole `generated/mappings/` corpus because the sensory classes' precise
SOSA alignments compile into `gmeow-observations.sssom.tsv` (matching `load_mappings()`'s cross-slice
aggregation); the AFO twin reads the sensory set's audio-feature rows directly. The reconciliation
record lives in `crates/validate/tests/conformance_support/graph_migration_manifest.rs`.
