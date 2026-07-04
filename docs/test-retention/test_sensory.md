# Retention: `tests/test_sensory.py`

**Category:** Python tool algorithm

## What it tests

Mapping tests for the Sensory module -- SOSA / AFO alignments only.

Retained dynamic tests:

- `test_sensor_mapped_to_sosa_sensor` — Sensor is aligned to sosa:Sensor.
- `test_sensor_platform_mapped_to_sosa_platform` — SensorPlatform is aligned to sosa:Platform.
- `test_observable_property_mapped_to_sosa` — ObservableProperty is aligned to sosa:ObservableProperty.
- `test_sensory_quantity_mapped_to_sosa_result` — SensoryQuantity is aligned to sosa:Result.
- `test_sensory_property_mapped_to_sosa_observed_property` — sensoryProperty is aligned to sosa:observedProperty.
- `test_platform_location_mapped_to_geo_location` — platformLocation is aligned to geo:location.
- `test_sensory_afo_mappings_exist` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.
