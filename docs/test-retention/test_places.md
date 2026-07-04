# Retention: `tests/test_places.py`

**Category:** Merged-graph guard

## What it tests

Retained guards for the places slice — bnode/chain, ABox fixture, and label-content tests that cannot be expressed as module-scoped SPARQL ASK cells in slices/core/places/tests/structural.ttl.

Retained dynamic tests:

- `test_location_superset_core` — Verifies classes, value scaffolds, properties, and topology of Location Core.
- `test_no_unsafe_motion_property_chains` — Principle 12: interpolation and coordinate transforms stay in solver.
- `test_has_coordinate_matrix_includes_geocode` — bnode owl:unionOf list walk to verify Geocode membership.
- `test_has_coordinates_property_chain` — bnode property-chain list check (exact member order).
- `test_has_geometry_property_chain` — bnode property-chain list check (exact member order).
- `test_place_type_parcel_exists` — label string-content check ('parcel' not in label).
- `test_contested_sovereignty_coexists` — Two contradictory standpoint-indexed containedInPlace claims load, SHACL-pass, and are BOTH retained — neither is the ground truth.
- `test_contested_place_names_coexist` — Two co-equal toponyms (endonym vs exonym) are both retained.
- `test_superseded_historical_name_suppressed` — A superseded place name is retained with displayable false (P10).
- `test_contested_jurisdiction_tenures_coexist` — Two contradictory JurisdictionTenures on the same place load, SHACL-pass, and are BOTH retained.
- `test_containment_tenure_records_border_change` — A ContainmentTenure records a place's parent change over time.
- `test_geometry_has_type_and_geojson` — A geometry may carry both a GeometryType value and a GeoJSON serialization.
- `test_contested_regulatory_overlays_coexist` — Two contradictory RegulatoryOverlays on the same place load, SHACL-pass, and are BOTH retained.
- `test_regulatory_overlay_linked_to_rights_statement` — A RegulatoryOverlay may link to a RightsStatement for the deontic rules that govern activity within the overlay.
- `test_regulatory_overlay_3d_bounds` — A restricted-airspace overlay carries altitude bounds as ScalarQuantity with QUDT units and a reference frame.
- `test_contested_eez_coexistence` — Two contradictory EEZ RegulatoryOverlays on the same maritime place load, SHACL-pass, and are BOTH retained.
- `test_location_stream_to_trajectory_derivation` — A Trajectory derived from a Stream of LocationStates loads and passes SHACL.
- `test_stream_and_trajectory_coexist` — Multiple streams/trajectories on the same entity coexist.
- `test_contested_capacity_claims_coexist` — Two contradictory Capacity measurements on the same location load, SHACL-pass, and are BOTH retained.
- `test_superseded_capacity_suppressed` — A superseded capacity is retained with displayable false (P10).
- `test_occupancy_with_unit_asserted` — An Occupancy measurement carries a scalar quantity with a QUDT unit.
- `test_storage_capacity_in_bytes` — A StorageLocation can have a capacity in bytes (QUDT BYTE unit).
- `test_virtual_location_types_coexist` — A VirtualLocation may have multiple virtualLocationType values (P9).
- `test_network_addresses_in_different_frames_coexist` — A VirtualLocation may have NetworkAddresses in different frames (P9).
- `test_superseded_network_address_suppressed` — A superseded network address is retained with displayable false (P10).
- `test_contested_dns_names_coexist` — Two standpoint-indexed DNS names for the same virtual location coexist (P9).
- `test_biological_standpoint_coordinate_claims_coexist` — Two standpoint-indexed SequenceCoordinates on the same gene load, SHACL-pass, and are BOTH retained.
- `test_geocode_shape_invalid_no_code` — A Geocode without any code value fails SHACL.
- `test_geocode_shape_invalid_two_codes` — A Geocode with two code values fails SHACL.
- `test_coordinate_observations_coexist` — Multiple CoordinateObservations on the same place load, SHACL-pass, and are BOTH retained.
- `test_superseded_coordinate_observation_suppressed` — A superseded coordinate observation is retained with displayable false.
- `test_land_tenure_instance_structure` — A LandTenure instance binds place, party, type, and interval.
- `test_cadastral_reference_instance_structure` — A CadastralReference instance binds value, type, authority, and jurisdiction.
- `test_contested_land_tenures_coexist` — Two contradictory LandTenures on the same parcel load, SHACL-pass, and are BOTH retained.
- `test_lapsed_tenure_suppressed_not_deleted` — A lapsed easement tenure is retained with displayable false (P10).
- `test_cadastral_reference_multiple_types_coexist` — A CadastralReference may carry multiple co-equal type claims (P9).

## Why it cannot be deleted or moved to Rust today

Retained guards for the places slice — bnode/chain, ABox fixture, and label-content tests that cannot be expressed as module-scoped SPARQL ASK cells in slices/core/places/tests/structural.ttl.
