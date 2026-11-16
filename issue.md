# Issue #2847: Change the recorded version to 0.4.0

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, rust

## Body

## Why

Release 0.4.0 needs every place that records the workspace and ontology version to say 0.4.0 before the tag.

## What

Change the version from 0.3.0 to 0.4.0 at each site that records it: the workspace package version and the fuzz crate in their Cargo manifests, the lockfile entries of the workspace crates, the five wasm package manifests and the docs console package manifest, the citation file, and the ontology header's version info and version IRI. Read how the term release authority treats a release change before editing the ontology header, and do whatever that mechanism requires in the same change.

## Acceptance

- Every site above reads 0.4.0 and nothing else changed.
- The lockfile agrees with the manifests (checked without building).
- The pull request carries no unrelated change.

Part of the 0.4.0 release.


## Comments (0)

