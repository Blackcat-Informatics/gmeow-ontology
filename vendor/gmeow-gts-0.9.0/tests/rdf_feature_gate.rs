// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(not(any(feature = "rdf", feature = "duckdb", feature = "sophia-adapter")))]
#[test]
fn optional_adapters_are_not_enabled_by_default() {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata emits JSON");
    let package = metadata["packages"]
        .as_array()
        .expect("metadata packages are an array")
        .iter()
        .find(|package| package["name"] == "gmeow-gts")
        .expect("gmeow-gts package is present");

    assert_eq!(package["features"]["default"], serde_json::json!([]));
    assert_eq!(package["features"]["duckdb"], serde_json::json!([]));
    assert_eq!(package["features"]["rdf"], serde_json::json!(["dep:oxrdf"]));
    assert_eq!(
        package["features"]["oxigraph-adapter"],
        serde_json::json!(["rdf", "dep:oxigraph"])
    );
    assert_eq!(
        package["features"]["sophia-adapter"],
        serde_json::json!(["dep:sophia_api", "dep:sophia_inmem", "dep:sophia_turtle"])
    );
    assert_eq!(
        package["features"]["policy-config"],
        serde_json::json!(["dep:serde", "dep:serde_json"])
    );
    assert_eq!(
        package["features"]["policy-config-yaml"],
        serde_json::json!(["policy-config", "dep:serde_yaml"])
    );

    let oxrdf = package["dependencies"]
        .as_array()
        .expect("metadata dependencies are an array")
        .iter()
        .find(|dependency| dependency["name"] == "oxrdf")
        .expect("oxrdf dependency is present");
    assert_eq!(oxrdf["optional"], serde_json::json!(true));
    assert_eq!(oxrdf["uses_default_features"], serde_json::json!(false));
    assert_eq!(oxrdf["features"], serde_json::json!(["rdf-12"]));

    let oxigraph = package["dependencies"]
        .as_array()
        .expect("metadata dependencies are an array")
        .iter()
        .find(|dependency| dependency["name"] == "oxigraph")
        .expect("oxigraph dependency is present");
    assert_eq!(oxigraph["optional"], serde_json::json!(true));
    assert_eq!(oxigraph["uses_default_features"], serde_json::json!(false));
    assert_eq!(oxigraph["features"], serde_json::json!(["rdf-12"]));

    for name in ["serde", "serde_json", "serde_yaml"] {
        let dependency = package["dependencies"]
            .as_array()
            .expect("metadata dependencies are an array")
            .iter()
            .find(|dependency| dependency["name"] == name)
            .unwrap_or_else(|| panic!("{name} dependency is present"));
        assert_eq!(dependency["optional"], serde_json::json!(true));
    }

    for name in ["sophia_api", "sophia_inmem", "sophia_turtle"] {
        let dependency = package["dependencies"]
            .as_array()
            .expect("metadata dependencies are an array")
            .iter()
            .find(|dependency| dependency["name"] == name)
            .unwrap_or_else(|| panic!("{name} dependency is present"));
        assert_eq!(dependency["optional"], serde_json::json!(true));
    }

    assert!(
        !package["dependencies"]
            .as_array()
            .expect("metadata dependencies are an array")
            .iter()
            .any(|dependency| dependency["name"] == "duckdb"),
        "duckdb feature must stay a no-dependency runtime shell-out"
    );
}

#[cfg(feature = "rdf")]
#[test]
fn rdf_feature_enables_adapter_module() {
    let options = gmeow_gts::rdf::ExportOptions::default();
    assert!(!options.allow_rdf12_lossy);
}

#[cfg(feature = "oxigraph-adapter")]
#[test]
fn oxigraph_feature_enables_adapter_module() {
    let sidecar = gmeow_gts::oxigraph::GtsSidecar::default();
    assert!(sidecar.diagnostics.is_empty());
}

#[cfg(feature = "sophia-adapter")]
#[test]
fn sophia_feature_enables_adapter_module() {
    let _dataset = gmeow_gts::sophia::SophiaDataset::new();
}
