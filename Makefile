# GMEOW ontology toolchain - canonical task runner.
# Make is the task-oriented plan. Core logic lives in `gmeow-dev`, Rust crates,
# or focused scripts; this file names the workflows and their dependencies.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Maintainer extraction target. Override: make maint-extract TARGET=foaf
TARGET ?= foaf

# Override: make commit MESSAGE="feat: add foaf alignment"
MESSAGE ?= chore: synchronize checked-in artifacts
GMEOW_DEV ?= cargo run -q -p gmeow-dev-cli --
SYNC_MODE ?=
SYNC_OUTPUTS ?= all
SYNC_VERBOSE ?=
CARGO_TARGET_DIR ?= target
SIGN_KEY ?=
PUBLIC_KEY ?= keys/gmeow-release-key.asc
GTS_OUT ?= dist/gmeow.gts
# Where `make console-assemble` writes the standalone <gmeow-console> tree. A SCRATCH
# base by design: `gmeow-dev console-assemble` REFUSES an --out equal to or inside
# `ontology-docs/` or `dist/gmeow-docs/`, because those have exactly one writer —
# `make regen SYNC_OUTPUTS=docs` — which reconciles them as whole trees.
CONSOLE_OUT ?= dist/console-smoke
PERF_DIR ?= dist/perf
# Injected release timestamp for the signed evidence fold (§18 determinism): the
# HEAD commit's strict-ISO committer date — deterministic per release commit, and
# overridable (e.g. RELEASE_ISSUED_AT=2026-06-25T00:00:00Z) for reproducible rebuilds.
RELEASE_ISSUED_AT ?= $(shell git show -s --format=%cI HEAD)
# release-publish knobs (§18 step 7). RELEASE_TAG names the GitHub release;
# CROSSREF_USER/CROSSREF_PASS are the depositor's Crossref member credentials
# (supplied by the maintainer at publish time — never stored). Publishing and
# DOI submission are USER-driven steps; this repo never holds signing keys.
RELEASE_TAG ?=
CROSSREF_USER ?=
CROSSREF_PASS ?=
CROSSREF_DEPOSIT_URL ?= https://doi.crossref.org/servlet/deposit

# Optional cargo-nextest partition for sharded CI runs (e.g., count:1/2)
NEXTEST_PARTITION ?=
NEXTEST_PARTITION_ARG := $(if $(NEXTEST_PARTITION),--partition $(NEXTEST_PARTITION) --no-tests pass,)
# The Rust workspace is tested in full; every crate is a normal Rust test binary
# (no CPython extension cdylib remains).
RUST_TEST_WORKSPACE_ARGS := --workspace

# The wasm32 cross-build must be hermetic w.r.t. an ambient host RUSTFLAGS. Cargo gives
# the RUSTFLAGS env var precedence over `[target.wasm32-unknown-unknown].rustflags`, so a
# host-set hint (e.g. a local pyo3 `-L .../lib -lpython3.13 -Ctarget-cpu=native` link flag)
# leaks into the wasm target and breaks the link (`unable to find library -lpython3.13`,
# `'x86-64-v3' is not a recognized processor`). Strip it for every wasm cargo invocation so
# these targets build regardless of the caller's environment.
WASM_CARGO := env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo

# The committed .cargo/config.toml defaults LOCAL Rust/C builds to host-tuned
# codegen for synchronization/reasoning throughput. CI and release workflows append the
# portable x86-64-v3 Rust target-cpu and override the C/C++ flags explicitly.

# The enforced corpus-aggregate recall floor lives in Rust
# (scoreboards::ACCEPTANCE_MIN_RECALL_PCT — the single source of truth). Leave this
# EMPTY to enforce that native floor; set it only to OVERRIDE for a dev measurement
# (e.g. `make acceptance ACCEPTANCE_MIN_RECALL=0` to measure without a floor).
ACCEPTANCE_MIN_RECALL ?=
FUZZ_TARGETS = nquads gts shacl sssom statements logic query clif cgif xcl
FUZZ_TIME ?= 30
MUTANTS_ARGS ?=
CHECK_PROFILE ?= impact
CHECK_ARGS ?=
CHECK_SYNC_MODE ?= check

# Real Make artifacts for expensive native build preparation. These replace
# environment sentinels: source timestamps decide when rebuilds are needed.
RUST_READY_STAMP := $(CARGO_TARGET_DIR)/.gmeow-rust-ready.stamp
RUST_INPUTS := Cargo.toml Cargo.lock .cargo/config.toml $(shell find crates -type f \( -name Cargo.toml -o -name '*.rs' -o -name build.rs \) 2>/dev/null)

.PHONY: help \
	install producer-build fmt lint check-lint lint-issue-refs i18n-lint \
	validate gts-frame-profile-gate reason verify reason-verify rust-build rust-test rust-docs check check-full check-sync \
	regen fanout commit normalize build project release release-sign-gts full-release verify-release release-publish clean \
	mappings wikidata coverage acceptance crossref audit \
	constitution-check crate-check lint-alignment doc-lint rust-gate coherence-gate-teeth clippy carrier-purity wasm \
	wasm-parity validate-wasm-pkg validate-wasm-pkg-test reason-wasm-pkg reason-wasm-pkg-test gmn-wasm-pkg gmn-wasm-pkg-test \
	mcp-wasm-pkg mcp-wasm-pkg-test mcp-core-wasm-pkg mcp-core-wasm-pkg-test \
	console-test console-assemble npm-publish-dry npm-consumable \
	lsp-build lsp-release lsp-sarif diagnostics-rust-sarif \
	slicetest conformance conformance-report insta-review slice-quality slice-quality-gate \
	fuzz-smoke bench bench-compare bench-golden-gate bench-soak rust-coverage mutants compliance-report perf-gate \
	maint-extract maint-refresh-target-axioms maint-refresh-mcp-asset maint-refresh-mcp-core-asset \
	maint-wikidata-live \
	maint-wikidata-coverage maint-wikidata-audit \
	maint-quality maint-evals-score \
	maint-compliance-report-full maint-bench-baseline maint-bench-instructions \
	maint-bench-engines maint-bench-cost-baseline maint-rust-heavy \
	maint-external-corpora maint-tptp-corpus maint-lang-selfhost \
	maint-chasebench-corpus maint-gmn-cost-matrix

##@ Core Workflows

help: ## Show the task plan.
	@awk 'BEGIN {FS = ":.*## "; print "GMEOW task plan"} \
		/^##@ / {printf "\n%s\n", substr($$0, 5); next} \
		/^[A-Za-z0-9_.-]+:.*## / {printf "  \033[36m%-28s\033[0m %s\n", $$1, $$2}' \
		$(MAKEFILE_LIST)

install: ## Bootstrap a clean clone source-first: build ONLY the producer, materialize generated/ via sync, then build the consumer CLIs that embed the bundle.
	$(MAKE) producer-build
	$(MAKE) regen
	$(MAKE) cli-build
	$(MAKE) lsp-release

fmt: ## Rewrite Rust formatting with cargo fmt.
	cargo fmt

lint-issue-refs: ## Reject issue/PR number references in Rust comments, Markdown docs, and TOML config.
	./scripts/lint-issue-refs.sh

lint: ## Run issue-ref lint and the full pre-commit hygiene suite (Rust fmt/clippy, spelling, YAML, actions, secrets).
	pre-commit run --all-files --show-diff-on-failure

# `rust-gate` owns the aggregate gate's one full clippy invocation. Keep the
# standalone `lint` target complete, but skip only that duplicate hook when `check`
# composes the same pre-commit suite with `rust-gate`. `lint-issue-refs` remains an
# always-run hook and therefore still executes exactly once in both entry points.
check-lint:
	SKIP=cargo-clippy pre-commit run --all-files --show-diff-on-failure

validate: ## Validate syntax, term annotations, SHACL, and DSL SHACL.
	$(GMEOW_DEV) validate

reason: ## Run the native Docker-free EL/DL reasoning authority.
	$(GMEOW_DEV) reason --mode native

verify: ## Run native reasoned-graph negative tests.
	$(GMEOW_DEV) verify --mode native

reason-verify: ## Run native reasoning + reasoned-graph verify with one closure.
	$(GMEOW_DEV) reason-verify

rust-build: $(RUST_READY_STAMP) ## Compile Rust workspace test binaries without running them.

rust-test: rust-build ## Run the Rust workspace tests and doctests.
	cargo run -q --package gmeow-docs --example prime-docs-fixture
	cargo nextest run --profile ci $(RUST_TEST_WORKSPACE_ARGS) $(NEXTEST_PARTITION_ARG)
	cargo test --doc $(RUST_TEST_WORKSPACE_ARGS)

gts-frame-profile-gate: ## Enforce zstd-rsyncable level 12 on every materialized GTS payload frame.
	$(GMEOW_DEV) gts-frame-profile generated/dist/gmeow.gts

rust-docs: ## Build Rust API docs and fail on broken or redundant public rustdoc links.
	RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links -A rustdoc::private_intra_doc_links" cargo doc --workspace --no-deps

lsp-build: lsp-release ## Build the gmeow-lsp binary.

lsp-release: $(RUST_READY_STAMP) ## Build the gmeow-lsp release binary and stage it into dist/bin/.
	cargo build -p gmeow-lsp --release
	mkdir -p dist/bin
	cp $(CARGO_TARGET_DIR)/release/gmeow-lsp dist/bin/gmeow-lsp
	@echo "gmeow-lsp release binary staged at dist/bin/gmeow-lsp"

producer-build: ## Build ONLY the gmeow-dev producer release binary and stage it into dist/bin/ (the clean-clone bootstrap entry: no generated/ bundle exists yet, so the consumer CLIs cannot compile — build ONLY the producer here, deliberately bypassing $(RUST_READY_STAMP), which would pull the whole workspace including the consumers into the build).
	cargo build -p gmeow-dev-cli --release
	mkdir -p dist/bin
	cp $(CARGO_TARGET_DIR)/release/gmeow-dev dist/bin/gmeow-dev
	@echo "gmeow-dev producer release binary staged at dist/bin/gmeow-dev"

cli-build: $(RUST_READY_STAMP) ## Build the gmeow + gmeow-dev release binaries and stage them into dist/bin/ (requires generated/dist/gmeow.gts to already be materialized — run 'make regen' first on a clean clone).
	cargo build -p gmeow-cli -p gmeow-dev-cli --release
	mkdir -p dist/bin
	cp $(CARGO_TARGET_DIR)/release/gmeow dist/bin/gmeow
	cp $(CARGO_TARGET_DIR)/release/gmeow-dev dist/bin/gmeow-dev
	@echo "gmeow + gmeow-dev release binaries staged at dist/bin/"

lsp-sarif: lsp-release ## Emit SARIF from all .ttl files in the workspace root (report-only).
	$(CARGO_TARGET_DIR)/release/gmeow-lsp sarif --out $(CARGO_TARGET_DIR)/lsp-sarif --category rust $$(find . -maxdepth 5 -name '*.ttl' -not -path './target/*' -not -path './.venv/*' | head -20) || true
	@echo "SARIF written to $(CARGO_TARGET_DIR)/lsp-sarif/gmeow-feedback.sarif"

diagnostics-rust-sarif: ## Emit the user-facing rust diagnostics SARIF via gmeow-lsp.
	$(MAKE) lsp-release
	$(CARGO_TARGET_DIR)/release/gmeow-lsp sarif --out dist/diagnostics/rust --category rust ontology/gmeow.ttl $(shell find conformance -name '*.logic')

check: ## Synchronize generated outputs, then run the receipt-backed impact gate.
	CHECK_SYNC_MODE=update cargo xtask check --profile $(CHECK_PROFILE) $(CHECK_ARGS)

check-full: ## Synchronize generated outputs, then physically run every local gate task.
	CHECK_SYNC_MODE=update cargo xtask check --profile full $(CHECK_ARGS)

# The `check` DAG's root task (crates/xtask/src/main.rs CHECK_DAG). generated/
# is now a gitignored local product, not a committed tree, so this is an
# INVERTED gate: it does not diff a fresh render against committed bytes, it
# PROJECTS the bundle fresh from source (an absent generated/ is a cache MISS,
# not a not-found error — see dev_sync.rs's manifest-hit check) and, in Check
# mode, proves that projection reproduces byte-identically on a second pass.
# `check`/`check-full` force CHECK_SYNC_MODE=update so this materializes the
# bundle+fanout from a clean clone before any downstream consumer-build task
# in the DAG runs; only a standalone `make check-sync` (mode=check by
# default) treats a missing bundle as a hard-fail drift finding.
check-sync:
	$(GMEOW_DEV) sync --mode $(CHECK_SYNC_MODE) --outputs generated

i18n-lint: ## Reject malformed or mechanically corrupted committed translations.
	$(GMEOW_DEV) i18n lint

##@ Generated Artifacts And Outputs

regen: ## Regenerate generated/ + the bundle ONLY (no gates). Usually unnecessary — `make check` already syncs then gates. Scope via SYNC_OUTPUTS={generated,docs,all}, mode via SYNC_MODE. The standalone regenerate lane for build/release/commit/CI; the gate is `make check`.
	@# Steering banner on DIRECT invocation only (MAKELEVEL=0). `make check`'s own
	@# regeneration runs the `check-sync` target via xtask, NOT this `regen` target, so
	@# this banner never fires inside `make check`; and the sub-make `regen` calls from
	@# install/build/docs/recursion run at MAKELEVEL>=1, so they stay quiet too.
	@if [ "$(MAKELEVEL)" = "0" ]; then \
		printf '\033[1;33m%s\033[0m\n' \
		  "──────────────────────────────────────────────────────────────────────" \
		  "NOTE: 'make regen' only REGENERATES generated/ + the bundle — it does NOT" \
		  "run any gate. You almost never need it directly: 'make check' ALREADY" \
		  "syncs (CHECK_SYNC_MODE=update) and THEN runs the full gate, so 'make regen'" \
		  "before 'make check' just regenerates twice. Run 'make regen' alone ONLY for" \
		  "a clean-clone bootstrap or a regen-without-gate. To verify work: make check" \
		  "──────────────────────────────────────────────────────────────────────" >&2; \
		  exit 1 \
	fi
	@# The docs-only fanout (`sync_docs`) REFERENCES the single `make build` output
	@# (dist/gmeow.jsonld / dist/gmeow.yamlld) instead of re-serializing it, so on a
	@# cold checkout that build output must exist before this pipeline's docs fanout
	@# runs. `SYNC_OUTPUTS=all` already orders this correctly in ONE pass (the pipeline
	@# run writes dist/gmeow.jsonld/.yamlld before sync_docs reads them); only the
	@# narrower `SYNC_OUTPUTS=docs` selection skips that pipeline-level dist/ write, so
	@# it alone needs an explicit materialize-then-build prerequisite here.
	@if [ "$(SYNC_OUTPUTS)" = "docs" ]; then \
		$(MAKE) regen SYNC_MODE=$(SYNC_MODE) SYNC_OUTPUTS=generated SYNC_VERBOSE=$(SYNC_VERBOSE); \
		$(MAKE) build; \
	fi
	$(GMEOW_DEV) sync $(if $(strip $(SYNC_MODE)),--mode $(SYNC_MODE),) --outputs $(SYNC_OUTPUTS) $(if $(filter 1 true yes on,$(strip $(SYNC_VERBOSE))),--verbose,)

fanout: ## Project the flat consumer tree back out of gmeow.gts (PIPELINE_SPINE §6).
	$(GMEOW_DEV) fanout

commit: regen ## Synchronize artifacts, stage generator-owned outputs, and commit.
	@REGENERATED_PATHS=$$(GMEOW_CONSOLE=silent $(GMEOW_DEV) sync --list-paths); \
	for p in $${REGENERATED_PATHS}; do \
	  if [ -e "$$p" ]; then git add "$$p"; fi; \
	done; \
	if git diff --cached --quiet; then \
		echo "Nothing to commit."; exit 1; \
	else \
		git commit -m "$(MESSAGE)"; \
	fi
	@git diff --quiet || echo "Warning: unstaged changes remain. Stage them separately if needed."

normalize: ## Rewrite authored ontology sources into canonical serialization.
	$(GMEOW_DEV) normalize

build: cli-build ## Build the Rust CLIs plus serializations and JSON-LD context into dist/.
	$(GMEOW_DEV) build

project: ## Project GMEOW data to schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profiles.
	$(GMEOW_DEV) project

release: ## Materialize from source (update mode), native-reason, build, report, docs, and emit CrossRef deposit.
	# A release BUILDS the product from canonical sources — it must materialize, never
	# read-only check. generated/ is a git-ignored product (#1600), so a fresh release
	# checkout has no pre-materialized tree; force SYNC_MODE=update (mirroring how `check`
	# forces CHECK_SYNC_MODE=update) instead of inheriting gmeow-dev's CI default of Check,
	# which would hard-fail the superset gate ("no carrier representative") on the absent tree.
	$(MAKE) regen SYNC_MODE=update
	$(GMEOW_DEV) reason --mode native --merge
	$(MAKE) build
	$(MAKE) lsp-release
	$(MAKE) maint-compliance-report-full
	$(MAKE) crossref

release-sign-gts: ## Sign the regenerated GTS bundle for release packaging.
	@if [ -z "$(SIGN_KEY)" ]; then \
		echo "SIGN_KEY=/path/to/secret.asc is required"; exit 1; \
	fi
	$(GMEOW_DEV) compile-gts --sign-key "$(SIGN_KEY)" --public-key "$(PUBLIC_KEY)" --out "$(GTS_OUT)"

full-release: ## Signed release-as-evidence: gate + oracle lane + conformance + perf, folded + signed + DOI (§18).
	@if [ -z "$(SIGN_KEY)" ]; then \
		echo "SIGN_KEY=/path/to/secret.asc is required"; exit 1; \
	fi
	$(MAKE) check
	$(MAKE) conformance
	$(MAKE) conformance-report
	$(MAKE) bench-compare
	$(MAKE) maint-compliance-report-full
	$(GMEOW_DEV) release-bundle \
		--sign-key "$(SIGN_KEY)" --public-key "$(PUBLIC_KEY)" \
		--out "$(GTS_OUT)" --source generated/dist/gmeow.gts \
		--issued-at "$(RELEASE_ISSUED_AT)" \
		--evidence "generated/diagnostics/shacl.sarif:application/sarif+json:attestationTypeQualityReport:shacl:SHACL diagnostics SARIF" \
		--evidence "dist/compliance-report.ttl:text/turtle:attestationTypeQualityReport:compliance:Compliance report" \
		--evidence "generated/conformance/verdicts.json:application/json:attestationTypeConformanceVerdict:conformance:Logic conformance suite verdicts" \
		--evidence "bench/baseline.json:application/json:attestationTypeQualityReport:perf:Perf baseline"
	$(MAKE) verify-release
	$(MAKE) crossref
	@echo "full-release: signed evidence bundle written to $(GTS_OUT)"

verify-release: ## Consumer verification of a signed release bundle: signature + trust policy + attestation frames (§18).
	@if [ ! -f "$(GTS_OUT)" ]; then \
		echo "no signed release bundle at $(GTS_OUT); run 'make full-release SIGN_KEY=...' first"; exit 1; \
	fi
	$(GMEOW_DEV) verify-release-bundle --bundle "$(GTS_OUT)" $(if $(PUBLIC_KEY),--public-key "$(PUBLIC_KEY)",)
	@echo "verify-release: signature + trust policy + attestation frames verified over $(GTS_OUT)"

release-publish: ## USER-driven publish of a verified signed bundle: content-addressed GitHub release + Crossref DOI deposit (§18 step 7).
	@if [ ! -f "$(GTS_OUT)" ]; then \
		echo "no signed release bundle at $(GTS_OUT); run 'make full-release SIGN_KEY=...' first"; exit 1; \
	fi
	@if [ -z "$(RELEASE_TAG)" ]; then \
		echo "RELEASE_TAG=vX.Y.Z is required (names the GitHub release)"; exit 1; \
	fi
	$(MAKE) verify-release
	$(MAKE) crossref
	$(MAKE) regen SYNC_MODE=update SYNC_OUTPUTS=docs
	$(GMEOW_DEV) docs-package --out dist/gmeow-docs.tar
	sha256sum "$(GTS_OUT)" > "$(GTS_OUT).sha256"
	@echo "release bundle native content heads (BLAKE3):"
	gts heads "$(GTS_OUT)"
	gh release create "$(RELEASE_TAG)" \
		"$(GTS_OUT)" "$(GTS_OUT).sha256" dist/crossref-deposit.xml \
		dist/gmeow-docs.tar dist/gmeow-docs.tar.blake3 \
		dist/gmeow-docs/manifest/docs-manifest.ttl dist/gmeow-docs.manifest.ttl.blake3 \
		--title "GMEOW $(RELEASE_TAG) — signed release-as-evidence bundle" \
		--notes "Signed, content-addressed release bundle (§18). Verify with \`make verify-release\` or \`gts verify gmeow.gts\`; download integrity via the .sha256 sidecar; native content address via \`gts heads\`. The attached Crossref deposit is over the always-latest concept DOI (version-agnostic by design). The attached dist/gmeow-docs.tar is the external documentation distribution (site/mdbook/pdf/snippets/pydantic/okf/jsonld/yamlld), content-addressed by its .blake3 sidecar; dist/gmeow-docs/manifest/docs-manifest.ttl is the DCAT release manifest for those distributions, content-addressed by its own dist/gmeow-docs.manifest.ttl.blake3 sidecar (kept OUTSIDE the archived tree so re-packaging stays idempotent)."
	@if [ -n "$(CROSSREF_USER)" ] && [ -n "$(CROSSREF_PASS)" ]; then \
		echo "submitting Crossref deposit as $(CROSSREF_USER) ..."; \
		curl -fsS -F 'operation=doMDUpload' -F 'login_id=$(CROSSREF_USER)' \
			-F 'login_passwd=$(CROSSREF_PASS)' -F 'fname=@dist/crossref-deposit.xml' \
			"$(CROSSREF_DEPOSIT_URL)"; \
		echo "Crossref deposit submitted."; \
	else \
		echo "DOI registration PENDING: set CROSSREF_USER + CROSSREF_PASS to submit, or run:"; \
		echo "  curl -F operation=doMDUpload -F login_id=\$$CROSSREF_USER -F login_passwd=\$$CROSSREF_PASS -F fname=@dist/crossref-deposit.xml $(CROSSREF_DEPOSIT_URL)"; \
	fi
	@echo "release-publish: published $(RELEASE_TAG) ($(GTS_OUT) + .sha256 + crossref deposit + docs distribution tar/manifest + .blake3 sidecars)."

clean: ## Remove ephemeral build artifacts.
	rm -rf dist docs/_generated .stamps $(RUST_READY_STAMP)
	@echo "cleaned ephemeral artifacts"

##@ Project Gates

mappings: ## Build alignment axioms and VoID linksets from SSSOM mappings.
	$(GMEOW_DEV) mappings

wikidata: ## Validate Wikidata QID/PID syntax in mappings, offline.
	$(GMEOW_DEV) wikidata

coverage: ## Gate vendored entity-slice class and predicate coverage.
	$(GMEOW_DEV) coverage --gaps --min-class 0.92 --min-predicate 0.85

slice-quality: ## Score one slice against the slice-quality rubric (advisory). Usage: make slice-quality SLICE=slices/core/tags
	$(GMEOW_DEV) slice-quality $(if $(strip $(SLICE)),$(SLICE),--all)

slice-quality-gate: ## Enforce the opt-in slice-quality tier ratchet.
	$(GMEOW_DEV) slice-quality-gate

slice-quality-seed-floors: ## Emit gmeow:AxisFloorCommitment TTL for live scores to seed a NEW axis's floors (one-shot). Usage: make slice-quality-seed-floors AXIS=axisShapeMigration (or ALL_AXES=1)
	$(GMEOW_DEV) slice-quality-seed-floors $(if $(strip $(AXIS)),--axis $(AXIS),)$(if $(strip $(ALL_AXES)),--all-axes,)

acceptance: ## Gate full transpile recall against external RDF snapshots.
	$(GMEOW_DEV) acceptance $(if $(strip $(ACCEPTANCE_MIN_RECALL)),--min-recall $(ACCEPTANCE_MIN_RECALL),)

crossref: ## Generate the CrossRef DOI deposit XML.
	$(GMEOW_DEV) crossref

audit: ## Run claim audit gates over the worked fixture.
	$(GMEOW_DEV) audit tests/fixtures/coverage/hallucination-kg.ttl

constitution-check: ## Verify every constitutional principle has live enforcement.
	$(GMEOW_DEV) constitution-check

crate-check: ## Verify Rust crate layering and acyclic crate DAGs.
	$(GMEOW_DEV) crate-check

lint-alignment: ## Lint SSSOM mappings for inverse and domain/range mismatches.
	$(GMEOW_DEV) lint-alignment

doc-lint: ## Lint ontology-docs for dangling links and coverage gaps.
	$(GMEOW_DEV) doc-lint

rust-gate: rust-build carrier-purity ## Warm Rust once, then run carrier purity, clippy, nextest, and doctests serially.
	cargo clippy --all-targets -- -D warnings
	cargo run -q --package gmeow-docs --example prime-docs-fixture
	cargo nextest run --profile ci $(RUST_TEST_WORKSPACE_ARGS) $(NEXTEST_PARTITION_ARG)
	cargo test --doc $(RUST_TEST_WORKSPACE_ARGS)

coherence-gate-teeth: rust-build ## Run the whole-ontology poisoned-witness and relator-mediation gate-teeth proofs.
	cargo nextest run $(RUST_TEST_WORKSPACE_ARGS) --ignore-default-filter -E 'package(gmeow-logic) & test(/whole_bundle_.*gate/)'

clippy: rust-build ## Run cargo clippy on all Rust targets with warnings as errors.
	cargo clippy --all-targets -- -D warnings

carrier-purity: rust-build ## Prove the pipeline inter-stage carrier/transport path uses no oxigraph Store accumulation (C11).
	@# STRUCTURAL gate: the composed value rides the native RdfDataset/PipelineBundle
	@# carrier (RdfDataset::union), and `snapshot`'s named-graph assembly + the SOLE
	@# `emit_gts` byte emitter create no oxigraph `Store` to accumulate/union/round-trip
	@# the carried RDF. The test scans the carrier modules' PRODUCTION source for a
	@# reintroduced `Store::new()` / `store_from_dataset` / `dataset_from_store` and FAILS
	@# if one returns. The carrier's typed-literal value-space canonicalization is now
	@# NATIVE (gmeow_xsd::parse_by_iri + XsdValue::canonical_lexical), so there is NO
	@# sanctioned-exception residual — the former `canonicalize_quad_literals` transient
	@# `Store` is gone. Excludes source-file parsing / the DAG loader (ingestion, not
	@# transport). The bundled negative-arm unit test proves the detector flags a
	@# reintroduced accumulation.
	cargo nextest run -p gmeow-pipeline --test carrier_purity
	@echo "OK: pipeline carrier/transport path is oxigraph-Store-free (native gmeow_xsd literal canon, no sanctioned residual)"

CAPI_HEADER := crates/rdf-capi/include/purrdf.h

wasm: ## Prove gmeow's wasm-clean crates (logic-compile + Tier-1 validator + reasoner + GMN codec + MCP engine) build for wasm32.
	@# gmeow's own wasm-first crates MUST compile to wasm32 with a reasoning-runtime-free
	@# dep tree. The RDF/wasm engine is the sibling `purrdf` package (gated by its own
	@# CI), so this target proves only gmeow's own crates. The target's absence is a SKIP
	@# locally but a hard FAIL in CI, so the wasm-clean criterion is never silently
	@# unverified on the gating path.
	@if rustc --print target-list | grep -qx wasm32-unknown-unknown && rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then \
		echo "== compiler proof: gmeow-logic-compile (pure parse->IR->project) builds for wasm32 =="; \
		$(WASM_CARGO) build -p gmeow-logic-compile --target wasm32-unknown-unknown || { echo "FAIL: gmeow-logic-compile does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== purity gate: no reasoning-runtime crate may appear in the gmeow-logic-compile wasm dep tree =="; \
		for forbidden in oxigraph oxrocksdb tokio pyo3; do \
			if $(WASM_CARGO) tree -p gmeow-logic-compile -e no-dev --target wasm32-unknown-unknown -i $$forbidden >/dev/null 2>&1; then \
				echo "FAIL: gmeow-logic-compile leaked $$forbidden into its wasm dependency tree:"; \
				$(WASM_CARGO) tree -p gmeow-logic-compile -e no-dev --target wasm32-unknown-unknown -i $$forbidden; \
				exit 1; \
			fi; \
		done; \
		echo "== validator proof: gmeow-validate (Tier-1 core) + gmeow-validate-wasm (Tier-1 SHACL + the GMN-1 codec validator) build for wasm32 =="; \
		$(WASM_CARGO) build -p gmeow-validate --target wasm32-unknown-unknown || { echo "FAIL: gmeow-validate does not build for wasm32-unknown-unknown"; exit 1; }; \
		: "gmeow-validate-wasm now also carries the GMN-1 validator (gmn_validate: gmn1_read against the embedded codebook). Its build pulls gmeow-lang-bridge's codec + dictionary; the codec path is reasoner-free and its tiktoken-rs glyph-cost analytics are cfg(not(wasm32))-gated off, so this same build proves the GMN path compiles wasm-clean."; \
		$(WASM_CARGO) build -p gmeow-validate-wasm --target wasm32-unknown-unknown || { echo "FAIL: gmeow-validate-wasm does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== purity gate: no reasoner / native-only crate may appear in the validator wasm dep tree (incl. the GMN-1 codec path) =="; \
		: "rayon is intentionally NOT forbidden — it cross-compiles to wasm32 and degrades to sequential when threads are unavailable (wasm-safe data-parallelism, not a reasoner/native-only crate); purrdf's RDF/SHACL core uses it and the wasm build links cleanly"; \
		: "gmeow-logic is the native DL reasoning runtime — it must NEVER reach the GMN-1 (or Tier-1) wasm validator; tiktoken-rs is the native-only glyph-cost BPE vocabulary that gmn_validate does not use. Both are forbidden here so a future leak into the GMN path (e.g. an un-gated gmn_symbology import) hard-fails."; \
		for vpkg in gmeow-validate gmeow-validate-wasm; do \
			for forbidden in gmeow-logic oxigraph oxrocksdb tokio pyo3 ureq duckdb ring tiktoken-rs; do \
				if $(WASM_CARGO) tree -p $$vpkg -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -qE "(^| )$$forbidden v[0-9]"; then \
					echo "FAIL: $$vpkg leaked $$forbidden into its wasm dependency tree:"; \
					$(WASM_CARGO) tree -p $$vpkg -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -E "(^| )$$forbidden v[0-9]"; \
					exit 1; \
				fi; \
			done; \
		done; \
		echo "== reasoner proof: gmeow-reason-wasm (the native structured-DL chase) builds for wasm32 =="; \
		$(WASM_CARGO) build -p gmeow-reason-wasm --target wasm32-unknown-unknown || { echo "FAIL: gmeow-reason-wasm does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== purity gate: no native-only crate may appear in the reasoner wasm dep tree (gmeow-logic + rayon ARE allowed — the reasoner runs serially on wasm) =="; \
		for forbidden in oxigraph oxrocksdb tokio pyo3 ureq duckdb ring nemo scryer; do \
			if $(WASM_CARGO) tree -p gmeow-reason-wasm -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -qE "(^| )$$forbidden v[0-9]"; then \
				echo "FAIL: gmeow-reason-wasm leaked $$forbidden into its wasm dependency tree:"; \
				$(WASM_CARGO) tree -p gmeow-reason-wasm -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -E "(^| )$$forbidden v[0-9]"; \
				exit 1; \
			fi; \
		done; \
		echo "== GMN codec proof: gmeow-gmn-wasm (GMN-0<->GMN-1 transcode) builds for wasm32 =="; \
		$(WASM_CARGO) build -p gmeow-gmn-wasm --target wasm32-unknown-unknown || { echo "FAIL: gmeow-gmn-wasm does not build for wasm32-unknown-unknown"; exit 1; }; \
		for forbidden in oxigraph oxrocksdb tokio pyo3 ureq duckdb ring; do \
			if $(WASM_CARGO) tree -p gmeow-gmn-wasm -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -qE "(^| )$$forbidden v[0-9]"; then \
				echo "FAIL: gmeow-gmn-wasm leaked $$forbidden into its wasm dependency tree:"; \
				$(WASM_CARGO) tree -p gmeow-gmn-wasm -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -E "(^| )$$forbidden v[0-9]"; \
				exit 1; \
			fi; \
		done; \
		echo "== MCP engine proof: gmeow-mcp-wasm (the demand-loaded REASONING segment) builds for wasm32 =="; \
		$(WASM_CARGO) build -p gmeow-mcp-wasm --target wasm32-unknown-unknown || { echo "FAIL: gmeow-mcp-wasm does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== purity gate: no native-only crate may appear in the MCP engine wasm dep tree (gmeow-logic + rayon ARE allowed — the engine reasons serially on wasm) =="; \
		: "tempfile is the NATIVE half of the mcp storage seam (atomic library appends). It does not cross-compile to wasm32, so it is cfg(not(target_arch = \"wasm32\"))-gated in crates/mcp; forbidding it here proves that gate holds and the browser backend is the one in play."; \
		: "gmeow-transcode / gmeow-docs-catalog are the two dependencies ONLY the core segment reaches, and tiktoken-rs is the ~1.7 MB cl100k vocabulary lang-bridge's glyph-cost feature carries. All three forbidden here is what makes this image a genuine DELTA rather than a superset of the core image — the exact regression that made the old heavy segment duplicate every core byte on disk."; \
		for forbidden in gmeow-transcode gmeow-docs-catalog tiktoken-rs oxigraph oxrocksdb tokio pyo3 ureq duckdb ring nemo scryer tempfile; do \
			if $(WASM_CARGO) tree -p gmeow-mcp-wasm -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -qE "(^| )$$forbidden v[0-9]"; then \
				echo "FAIL: gmeow-mcp-wasm leaked $$forbidden into its wasm dependency tree:"; \
				$(WASM_CARGO) tree -p gmeow-mcp-wasm -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -E "(^| )$$forbidden v[0-9]"; \
				exit 1; \
			fi; \
		done; \
		echo "== lean-core proof: gmeow-mcp-core-wasm (the SAME 38-tool surface, reasoning segment demand-loaded) builds for wasm32 =="; \
		$(WASM_CARGO) build -p gmeow-mcp-core-wasm --target wasm32-unknown-unknown || { echo "FAIL: gmeow-mcp-core-wasm does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== segment gate: the reasoning segment must be ABSENT from the lean core's wasm dep tree =="; \
		: "This is the whole claim of the crate, so it is a dep-tree assertion rather than a comment. gmeow-logic (the DL reasoner) and gmeow-slice-quality (the rubric kernel over it) are what the first-load image exists to NOT carry; if either reappears the byte win is gone and the tiering is theatre. tiktoken-rs is forbidden on BOTH sides: no image may carry the cl100k vocabulary. The native-only forbidden set is checked too, exactly as for the reasoning segment."; \
		for forbidden in gmeow-logic gmeow-slice-quality tiktoken-rs oxigraph oxrocksdb tokio pyo3 ureq duckdb ring nemo scryer tempfile; do \
			if $(WASM_CARGO) tree -p gmeow-mcp-core-wasm -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -qE "(^| )$$forbidden v[0-9]"; then \
				echo "FAIL: gmeow-mcp-core-wasm leaked $$forbidden into its wasm dependency tree:"; \
				$(WASM_CARGO) tree -p gmeow-mcp-core-wasm -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -E "(^| )$$forbidden v[0-9]"; \
				exit 1; \
			fi; \
		done; \
		echo "OK: gmeow-logic-compile + the wasm Tier-1 validator + the wasm reasoner + the wasm GMN codec + the wasm MCP reasoning segment + its lean core build for wasm32 (dep trees are native-runtime-free; the two segments are DISJOINT and neither carries the cl100k vocabulary)"; \
	elif [ -n "$${CI:-}" ]; then \
		echo "FAIL: wasm32-unknown-unknown target absent in CI — gmeow's wasm-clean criterion cannot be verified; CI must install it"; exit 1; \
	else \
		echo "SKIP: wasm32-unknown-unknown target not installed (local only; CI hard-fails) — 'rustup target add wasm32-unknown-unknown' to enable the wasm-clean check"; \
	fi

validate-wasm-pkg: ## Build the gmeow-validate-wasm npm/ESM package (release wasm + wasm-bindgen web bindings).
	@# Release-build the cdylib, then run `wasm-bindgen` (pinned, matching the crate) to
	@# emit the ESM `web`-target JS bindings + .d.ts + .wasm into js/pkg/.
	$(WASM_CARGO) build -p gmeow-validate-wasm --target wasm32-unknown-unknown --release
	PATH="$$HOME/.cargo/bin:$$PATH" wasm-bindgen \
		$(CARGO_TARGET_DIR)/wasm32-unknown-unknown/release/gmeow_validate_wasm.wasm \
		--out-dir crates/validate-wasm/js/pkg --target web
	@# wasm-opt -Oz is a REQUIRED build step (roughly halves the artifact). It is a
	@# hard dependency: a missing wasm-opt is a build failure, never a note.
	@command -v wasm-opt >/dev/null 2>&1 || { echo "ERROR: wasm-opt (binaryen) not found — it is a REQUIRED wasm build dependency; install binaryen"; exit 1; }
	wasm-opt -Oz --enable-bulk-memory --enable-bulk-memory-opt -o crates/validate-wasm/js/pkg/gmeow_validate_wasm_bg.wasm crates/validate-wasm/js/pkg/gmeow_validate_wasm_bg.wasm
	@echo "OK: wasm-opt -Oz applied"
	@echo "OK: gmeow-validate-wasm npm package built (crates/validate-wasm/js/, pkg/ generated)"

validate-wasm-pkg-test: validate-wasm-pkg ## Build the validator npm package and run its Node real-execution round-trip lane.
	@# The purrdf RDF/wasm engine ships + tests its own npm package on purrdf's CI; this
	@# lane proves ONLY gmeow's own deliverable — the Tier-1 validator wasm package — by
	@# validating a real dataset against the committed gmeow.gts through the wasm-bindgen
	@# bindings just built above.
	cd crates/validate-wasm/js && node --test tests/*.test.mjs
	@echo "OK: gmeow-validate-wasm Node round-trip lane passed"

reason-wasm-pkg: ## Build the gmeow-reason-wasm npm/ESM package (release wasm + wasm-bindgen web bindings).
	$(WASM_CARGO) build -p gmeow-reason-wasm --target wasm32-unknown-unknown --release
	PATH="$$HOME/.cargo/bin:$$PATH" wasm-bindgen \
		$(CARGO_TARGET_DIR)/wasm32-unknown-unknown/release/gmeow_reason_wasm.wasm \
		--out-dir crates/reason-wasm/js/pkg --target web
	@command -v wasm-opt >/dev/null 2>&1 || { echo "ERROR: wasm-opt (binaryen) not found — it is a REQUIRED wasm build dependency; install binaryen"; exit 1; }
	wasm-opt -Oz --enable-bulk-memory --enable-bulk-memory-opt -o crates/reason-wasm/js/pkg/gmeow_reason_wasm_bg.wasm crates/reason-wasm/js/pkg/gmeow_reason_wasm_bg.wasm
	@echo "OK: gmeow-reason-wasm npm package built (crates/reason-wasm/js/, pkg/ generated)"


reason-wasm-pkg-test: reason-wasm-pkg ## Build the reasoner npm package and run its Node native↔wasm parity witness lane.
	cd crates/reason-wasm/js && node --test tests/*.test.mjs
	@echo "OK: gmeow-reason-wasm Node native↔wasm parity witness lane passed"

gmn-wasm-pkg: ## Build the gmeow-gmn-wasm npm/ESM package (release wasm + wasm-bindgen web bindings).
	$(WASM_CARGO) build -p gmeow-gmn-wasm --target wasm32-unknown-unknown --release
	PATH="$$HOME/.cargo/bin:$$PATH" wasm-bindgen \
		$(CARGO_TARGET_DIR)/wasm32-unknown-unknown/release/gmeow_gmn_wasm.wasm \
		--out-dir crates/gmn-wasm/js/pkg --target web
	@command -v wasm-opt >/dev/null 2>&1 || { echo "ERROR: wasm-opt (binaryen) not found — it is a REQUIRED wasm build dependency; install binaryen"; exit 1; }
	wasm-opt -Oz --enable-bulk-memory --enable-bulk-memory-opt -o crates/gmn-wasm/js/pkg/gmeow_gmn_wasm_bg.wasm crates/gmn-wasm/js/pkg/gmeow_gmn_wasm_bg.wasm
	@echo "OK: gmeow-gmn-wasm npm package built (crates/gmn-wasm/js/, pkg/ generated)"


gmn-wasm-pkg-test: gmn-wasm-pkg ## Build the GMN codec npm package and run its Node native↔wasm parity witness lane.
	cd crates/gmn-wasm/js && node --test tests/*.test.mjs
	@echo "OK: gmeow-gmn-wasm Node native↔wasm parity witness lane passed"

mcp-wasm-pkg: ## Build the gmeow-mcp-wasm npm/ESM package (release wasm + wasm-bindgen web bindings).
	$(WASM_CARGO) build -p gmeow-mcp-wasm --target wasm32-unknown-unknown --release
	PATH="$$HOME/.cargo/bin:$$PATH" wasm-bindgen \
		$(CARGO_TARGET_DIR)/wasm32-unknown-unknown/release/gmeow_mcp_wasm.wasm \
		--out-dir crates/mcp-wasm/js/pkg --target web
	@command -v wasm-opt >/dev/null 2>&1 || { echo "ERROR: wasm-opt (binaryen) not found — it is a REQUIRED wasm build dependency; install binaryen"; exit 1; }
	@# `--enable-nontrapping-float-to-int` is REQUIRED here and absent from the three
	@# sibling lanes because this image genuinely uses that instruction family: the MCP
	@# engine links the transcode hub + the numeric-literal canonicalization paths, whose
	@# codegen emits `i64.trunc_sat_f64_{s,u}`. binaryen validates the input against
	@# exactly the feature set named on the command line, so without the flag wasm-opt
	@# HARD-FAILS ("all used features should be allowed") — the flag DECLARES what rustc
	@# emitted, it does not relax a check.
	wasm-opt -Oz --enable-bulk-memory --enable-bulk-memory-opt --enable-nontrapping-float-to-int -o crates/mcp-wasm/js/pkg/gmeow_mcp_wasm_bg.wasm crates/mcp-wasm/js/pkg/gmeow_mcp_wasm_bg.wasm
	@echo "OK: wasm-opt -Oz applied"
	@echo "OK: gmeow-mcp-wasm npm package built (crates/mcp-wasm/js/, pkg/ generated)"

maint-refresh-mcp-asset: mcp-wasm-pkg-test ## Re-vendor the gmeow-mcp-wasm reasoning segment into crates/docs/assets/mcp/ and re-pin its BLAKE3 manifest (only after the Node native<->wasm parity lane passes).
	@# The re-pin drives the SHARED vendored-wasm-asset harness through the `MCP_ASSET`
	@# descriptor (`gmeow_docs::vendored_asset`). The vendored set is a TREE, not a flat
	@# list: `index.mjs` imports `./pkg/<mod>.js`, so the wasm-bindgen output keeps its
	@# `pkg/` subpath and the emitted site layout is the package layout.
	@# WITNESS.mcp.json rides along because the attestation must describe THESE bytes —
	@# `attestation_status` refuses a witness whose digests no longer match the shipped blob.
	mkdir -p crates/docs/assets/mcp/pkg
	cp crates/mcp-wasm/js/index.mjs                        crates/docs/assets/mcp/index.mjs
	cp crates/mcp-wasm/js/pkg/gmeow_mcp_wasm.js            crates/docs/assets/mcp/pkg/gmeow_mcp_wasm.js
	cp crates/mcp-wasm/js/pkg/gmeow_mcp_wasm_bg.wasm       crates/docs/assets/mcp/pkg/gmeow_mcp_wasm_bg.wasm
	cp crates/mcp-wasm/js/pkg/gmeow_mcp_wasm.d.ts          crates/docs/assets/mcp/pkg/gmeow_mcp_wasm.d.ts
	cp crates/mcp-wasm/js/pkg/gmeow_mcp_wasm_bg.wasm.d.ts  crates/docs/assets/mcp/pkg/gmeow_mcp_wasm_bg.wasm.d.ts
	cp crates/mcp-wasm/tests/WITNESS.mcp.json              crates/docs/assets/mcp/WITNESS.mcp.json
	@# The re-pin is RUN ALONE, then the whole binary re-runs as the verification. Blessing
	@# inside the full parallel run raced: `both_segments_carry_a_present_and_current_native_wasm_attestation`
	@# reads DIGESTS.blake3 while `vendored_mcp_reasoning_segment_passes_the_anti_rot_gate`
	@# is rewriting it, so a refresh that genuinely moved bytes failed on a stale read and
	@# then passed on a re-run — a gate that has to be run twice is not a gate.
	GMEOW_MCP_BLESS=1 cargo test -p gmeow-docs --test mcp_asset -- --exact vendored_mcp_reasoning_segment_passes_the_anti_rot_gate
	cargo test -p gmeow-docs --test mcp_asset
	@echo "OK: re-vendored gmeow-mcp-wasm into crates/docs/assets/mcp/ (DIGESTS.blake3 re-pinned)"

maint-refresh-mcp-core-asset: mcp-core-wasm-pkg-test ## Re-vendor the gmeow-mcp-core-wasm first-load segment into crates/docs/assets/mcp-core/ and re-pin its BLAKE3 manifest (only after the Node parity + demand-load lane passes).
	@# The twin of maint-refresh-mcp-asset, through the `MCP_CORE_ASSET` descriptor. Its
	@# prerequisite lane is the STRONGER one: mcp-core-wasm-pkg-test builds BOTH segments and
	@# exercises the real demand loader across them, so these bytes are only re-pinned after
	@# the tiering has been proven end to end.
	mkdir -p crates/docs/assets/mcp-core/pkg
	cp crates/mcp-core-wasm/js/index.mjs                             crates/docs/assets/mcp-core/index.mjs
	cp crates/mcp-core-wasm/js/pkg/gmeow_mcp_core_wasm.js            crates/docs/assets/mcp-core/pkg/gmeow_mcp_core_wasm.js
	cp crates/mcp-core-wasm/js/pkg/gmeow_mcp_core_wasm_bg.wasm       crates/docs/assets/mcp-core/pkg/gmeow_mcp_core_wasm_bg.wasm
	cp crates/mcp-core-wasm/js/pkg/gmeow_mcp_core_wasm.d.ts          crates/docs/assets/mcp-core/pkg/gmeow_mcp_core_wasm.d.ts
	cp crates/mcp-core-wasm/js/pkg/gmeow_mcp_core_wasm_bg.wasm.d.ts  crates/docs/assets/mcp-core/pkg/gmeow_mcp_core_wasm_bg.wasm.d.ts
	cp crates/mcp-core-wasm/tests/WITNESS.core-deferral.json         crates/docs/assets/mcp-core/WITNESS.core-deferral.json
	@# Blessed alone then verified in full, for the reason spelled out on maint-refresh-mcp-asset.
	GMEOW_MCP_CORE_BLESS=1 cargo test -p gmeow-docs --test mcp_asset -- --exact vendored_mcp_core_segment_passes_the_anti_rot_gate
	cargo test -p gmeow-docs --test mcp_asset
	@echo "OK: re-vendored gmeow-mcp-core-wasm into crates/docs/assets/mcp-core/ (DIGESTS.blake3 re-pinned)"

mcp-wasm-pkg-test: mcp-wasm-pkg ## Build the MCP engine npm package and run its Node native↔wasm parity witness lane.
	cd crates/mcp-wasm/js && node --test tests/*.test.mjs
	@echo "OK: gmeow-mcp-wasm Node native↔wasm parity witness lane passed"

mcp-core-wasm-pkg: ## Build the gmeow-mcp-core-wasm npm/ESM package (the LEAN first-load engine: release wasm + wasm-bindgen web bindings).
	$(WASM_CARGO) build -p gmeow-mcp-core-wasm --target wasm32-unknown-unknown --release
	PATH="$$HOME/.cargo/bin:$$PATH" wasm-bindgen \
		$(CARGO_TARGET_DIR)/wasm32-unknown-unknown/release/gmeow_mcp_core_wasm.wasm \
		--out-dir crates/mcp-core-wasm/js/pkg --target web
	@command -v wasm-opt >/dev/null 2>&1 || { echo "ERROR: wasm-opt (binaryen) not found — it is a REQUIRED wasm build dependency; install binaryen"; exit 1; }
	@# The SAME feature declaration the full MCP image needs, and for the same reason: the
	@# core engine still links the transcode hub + the numeric-literal canonicalization
	@# paths, whose codegen emits `i64.trunc_sat_f64_{s,u}`. binaryen validates the input
	@# against exactly the feature set named on the command line, so without the flag
	@# wasm-opt HARD-FAILS — the flag DECLARES what rustc emitted, it does not relax a check.
	wasm-opt -Oz --enable-bulk-memory --enable-bulk-memory-opt --enable-nontrapping-float-to-int -o crates/mcp-core-wasm/js/pkg/gmeow_mcp_core_wasm_bg.wasm crates/mcp-core-wasm/js/pkg/gmeow_mcp_core_wasm_bg.wasm
	@echo "OK: wasm-opt -Oz applied"
	@echo "OK: gmeow-mcp-core-wasm npm package built (crates/mcp-core-wasm/js/, pkg/ generated)"

mcp-core-wasm-pkg-test: mcp-core-wasm-pkg mcp-wasm-pkg ## Build the lean core npm package and run its Node parity + demand-load lane.
	@# Depends on BOTH packages: the lane does not merely assert the deferral signal, it
	@# executes the demand loader against the REAL reasoning segment (`gmeow-mcp-wasm`) and
	@# asserts the replayed frame's answer is byte-identical to sending it to the full
	@# engine directly. A stubbed segment would prove nothing about re-dispatch.
	cd crates/mcp-core-wasm/js && node --test tests/*.test.mjs
	@echo "OK: gmeow-mcp-core-wasm Node parity + demand-load lane passed"

console-test: ## Run the standalone console's DOM-free acceptance lane against the shipped wasm engine.
	@# The seven named assertions. They drive the SHIPPED `crates/docs/assets/mcp-core/`
	@# image over the SHIPPED `generated/dist/gmeow.gts` — no browser, no mocks. Locally
	@# this SKIPs when node is absent; CI hard-fails, so the console's derived surface is
	@# never silently unverified on the gating path. (The shell-agreement assertion and the
	@# producer's byte-identity assertions are Rust tests and ride `rust-gate`.)
	@# `crates/docs/assets/tests/` rides the same lane: it proves the shipped client-side
	@# BLAKE3 (`assets/blake3.mjs`) against the published reference vectors AND against the
	@# Rust `blake3` crate's own output (the committed `DIGESTS.blake3` manifests). Every
	@# integrity check the browser performs is that function, so a JS/Rust disagreement
	@# would silently turn asset verification into asset rejection.
	@if command -v node >/dev/null 2>&1; then \
		( cd crates/docs/assets && node --test tests/*.test.mjs ); \
		( cd crates/docs/assets/console && node --test tests/*.test.mjs ); \
	elif [ -n "$${CI:-}" ]; then \
		echo "FAIL: node absent in CI — the console acceptance lane cannot run; CI must install it"; exit 1; \
	else \
		echo "SKIP: node not installed (local only; CI hard-fails) — install node to run the console acceptance lane"; \
	fi

npm-publish-dry: ## Network-safe `npm publish --dry-run` over every package this repo publishes.
	@# The package set is DISCOVERED from the shipped bytes (`scripts/npm-package-dirs.mjs`
	@# lists every non-private, non-Marketplace package.json), never restated here — the
	@# names authority is the manifests themselves.
	@#
	@# `--dry-run` is what makes this network-safe AND on-gate-able: npm resolves the
	@# tarball locally, prints the exact `files` set it would upload, and performs NO
	@# registry request and NO authentication. It is the same code path a real publish
	@# takes right up to the upload, so a manifest that would fail to pack fails HERE.
	@#
	@# The wasm engine packages declare their `pkg/…` bindings in `files`; those are the
	@# `*-wasm-pkg` build output, so run this after `make wasm-parity` (or any `*-pkg`
	@# lane) if you want the tarball listing to include them.
	@set -e; \
	node scripts/npm-package-dirs.mjs | while read -r dir; do \
		echo "== npm publish --dry-run $$dir =="; \
		( cd "$$dir" && npm publish --dry-run --access public ); \
	done
	@echo "OK: every published package packs cleanly (dry run; no registry contact)"

npm-consumable: ## Prove every published package is CONSUMABLE: pack -> install the tarball into a temp project -> run the witness against the INSTALLED package.
	@# Not a re-run of the in-tree lanes: the driver `npm pack`s the real tarball from the
	@# real `files` set, `npm install`s it into a throwaway project so the package resolves
	@# BY NAME through node_modules, and then instantiates the wasm out of the INSTALLED
	@# bytes to perform a real operation — several of them against the SAME committed
	@# attestations the parity lanes assert byte-identity to. A published package with no
	@# witness is a hard failure, never a skip.
	@#
	@# Requires the wasm engine packages to have been built (`make wasm-parity` or the
	@# individual `*-wasm-pkg` lanes): a tarball missing its `pkg/` bindings cannot be
	@# imported, which is exactly the consumability failure this lane exists to catch.
	node scripts/npm-consumability.mjs

console-assemble: ## Assemble the standalone <gmeow-console> tree into $(CONSOLE_OUT) for local preview.
	@# `gmeow-dev console-assemble` REFUSES an --out inside ontology-docs/ or
	@# dist/gmeow-docs/ (one writer: `make regen SYNC_OUTPUTS=docs`), which is why
	@# CONSOLE_OUT defaults to a scratch base.
	$(GMEOW_DEV) console-assemble --out $(CONSOLE_OUT)

wasm-parity: ## Prove "native≡wasm" on-gate: wasm32 build purity + the five Node lanes that RUN the shipped wasm and assert byte-identity to native.
	@# `wasm` proves the crates BUILD for wasm32 + dep purity; the five `*-pkg-test`
	@# lanes RUN the shipped wasm (validate/reason/gmn/mcp/mcp-core) and assert
	@# byte-identity to the native engine — the "every gmeow surface proven native≡wasm"
	@# contract. The mcp-core lane additionally proves the TIERED contract end to end: the
	@# deferral signal is byte-pinned, and the demand loader re-dispatches the identical
	@# frame to the real reasoning segment for an answer identical to the full engine's.
	@# On-gate so the vendored digest that gates the shipped bytes is exactly the one a
	@# parity run blessed (the `maint-refresh-*-asset` targets now depend on `*-pkg-test`,
	@# so re-vendoring cannot re-pin bytes that never passed parity). Locally this SKIPs
	@# when the wasm32 target or node is absent; CI hard-fails — the parity criterion is
	@# never silently unverified on the gating path.
	@if rustc --print target-list | grep -qx wasm32-unknown-unknown && rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown && command -v node >/dev/null 2>&1; then \
		$(MAKE) wasm validate-wasm-pkg-test reason-wasm-pkg-test gmn-wasm-pkg-test mcp-wasm-pkg-test mcp-core-wasm-pkg-test; \
	elif [ -n "$${CI:-}" ]; then \
		echo "FAIL: wasm32-unknown-unknown target or node absent in CI — the native≡wasm parity witnesses cannot run; CI must install both"; exit 1; \
	else \
		echo "SKIP: wasm32-unknown-unknown target or node not installed (local only; CI hard-fails) — 'rustup target add wasm32-unknown-unknown' + install node to run the native≡wasm parity lanes"; \
	fi

maint-rust-heavy: rust-build ## Run the Rust suite INCLUDING the off-gate heavy tests (maint-heavy profile).
	cargo run -q --package gmeow-docs --example prime-docs-fixture
	cargo nextest run --profile maint-heavy $(NEXTEST_PARTITION_ARG)
	$(MAKE) maint-dev-cli-heavy

maint-dev-cli-heavy: rust-build ## Run the off-gate gmeow-dev CLI heavy parity lane (whole-pipeline/gate commands: feedback, validate, logic compile --check, up-projection-audit).
	GMEOW_DEV_CLI_HEAVY=1 cargo nextest run -p gmeow-dev-cli --run-ignored ignored-only $(NEXTEST_PARTITION_ARG)

slicetest: ## Run the slice-resident test-DSL harness in isolation.
	cargo nextest run -p gmeow-slicetest $(NEXTEST_PARTITION_ARG)
	cargo test --doc -p gmeow-slicetest

conformance: ## Run the native logic conformance harness in isolation.
	cargo nextest run -p gmeow-conformance $(NEXTEST_PARTITION_ARG)

conformance-report: ## Materialize the logic conformance suite verdicts as a foldable release artifact (§18).
	cargo run -p gmeow-conformance --bin conformance-report -- --out generated/conformance/verdicts.json

insta-review: ## Regenerate intentional insta snapshot goldens, then verify determinism.
	INSTA_UPDATE=always cargo nextest run $(NEXTEST_PARTITION_ARG)
	INSTA_UPDATE=no cargo nextest run $(NEXTEST_PARTITION_ARG)

##@ CI And Report-Only Work

fuzz-smoke: ## Run bounded coverage-guided fuzz smoke tests for each format frontend.
	@for t in $(FUZZ_TARGETS); do \
	  echo "== fuzz $$t ($(FUZZ_TIME)s) =="; \
	  mkdir -p fuzz/corpus/$$t; \
	  cargo fuzz run $$t fuzz/corpus/$$t fuzz/seeds/$$t -- -max_total_time=$(FUZZ_TIME) || exit 1; \
	done

bench: ## Run criterion benchmarks with host-tuned codegen.
	cargo bench -p gmeow-logic -p gmeow-validate

bench-compare: ## Report-only perf scoreboard: live criterion run vs committed bench/baseline.json.
	@cargo run -q -p gmeow-pipeline --bin bench-compare

bench-golden-gate: ## On-gate native-vs-golden agreement gate: run the native engine over the committed mini corpora and hard-fail on any golden divergence (no live oracle — cheap).
	cargo run -q -p gmeow-bench-engines -- --check-golden

bench-soak: ## On-gate divergence-ledger soak window: run the deterministic native-vs-golden check 3× and require gap-zero with a byte-identical digest (no live oracle).
	cargo run -q -p gmeow-bench-engines -- --soak 3

perf-gate: ## Report-only timings for validate, generated drift, reason, and verify.
	mkdir -p $(PERF_DIR)
	$(GMEOW_DEV) validate --timings --timings-json $(PERF_DIR)/validate.json
	$(GMEOW_DEV) sync --mode check --outputs generated --timings-json $(PERF_DIR)/sync.json
	$(GMEOW_DEV) reason-verify --timings-json $(PERF_DIR)/reason-verify.json
	cargo run -q -p gmeow-pipeline --bin perf_gate_merge -- $(PERF_DIR)
	@echo "perf gate timings written to $(PERF_DIR)/gate-timings.json"

rust-coverage: ## Generate report-only Rust region coverage.
	cargo llvm-cov nextest --workspace --include-ffi --lcov --output-path lcov.info
	cargo llvm-cov report --html

mutants: ## Run report-only cargo-mutants over the configured scope.
	cargo mutants $(MUTANTS_ARGS)

compliance-report: ## Emit dist/compliance-report.ttl from already-passing gates.
	$(GMEOW_DEV) compliance-report --from-passing-check

##@ Maintainer Tasks

maint-extract: ## Run import/extract policy for TARGET.
	$(GMEOW_DEV) extract --target $(TARGET)

maint-refresh-target-axioms: ## Re-vendor minimal target-axiom snapshots.
	$(GMEOW_DEV) refresh-target-axioms --target all


maint-wikidata-live: ## Verify Wikidata identifiers resolve over the network.
	$(GMEOW_DEV) wikidata --existence

maint-wikidata-coverage: ## Report Wikidata mapping coverage by domain.
	$(GMEOW_DEV) wikidata-coverage

maint-wikidata-audit: ## Audit fixtures and modules for Wikidata misuse.
	$(GMEOW_DEV) wikidata --fixtures

maint-quality: ## Run OOPS! network pitfall scan.
	$(GMEOW_DEV) quality

maint-evals-score: ## Score committed model emissions against the eval contract.
	$(GMEOW_DEV) evals score

maint-compliance-report-full: ## Run in-process gates and emit dist/compliance-report.ttl.
	$(GMEOW_DEV) compliance-report

maint-bench-baseline: ## (maintainer) Refresh bench/baseline.json from a fresh criterion run.
	$(MAKE) bench
	cargo run -q -p gmeow-pipeline --bin bench-compare -- --emit-baseline > bench/baseline.json
	@echo "wrote bench/baseline.json ($$(wc -c < bench/baseline.json) bytes) — regenerate + commit"

maint-bench-instructions: ## (maintainer) Deterministic retired-instruction counts for the engines via iai-callgrind under Valgrind (off-gate corroboration; NOT wired into `make check`).
	@# iai-callgrind measures RETIRED INSTRUCTIONS under Valgrind Callgrind — a
	@# machine-independent, run-to-run stable metric that corroborates the on-gate
	@# steps+alloc+peak-live cost gate. It is off-gate because it needs Valgrind and
	@# the out-of-tree runner; per the measurement doctrine, instruction-count is
	@# CORROBORATION, not the gate. HARD FAIL (no silent skip) if either tool is
	@# absent — the version below MUST match the iai-callgrind dev-dep in
	@# crates/logic/Cargo.toml.
	@command -v valgrind >/dev/null 2>&1 || { \
	  echo "ERROR: valgrind not found — it is REQUIRED for maint-bench-instructions (iai-callgrind drives Valgrind Callgrind)."; \
	  echo "  Install it, e.g.: Arch: 'sudo pacman -S valgrind'; Debian/Ubuntu: 'sudo apt-get install valgrind'; Fedora: 'sudo dnf install valgrind'."; \
	  exit 1; }
	@command -v iai-callgrind-runner >/dev/null 2>&1 || { \
	  echo "ERROR: iai-callgrind-runner not found — it is REQUIRED for maint-bench-instructions and must match the pinned iai-callgrind dev-dep."; \
	  echo "  Install it with: cargo install iai-callgrind-runner --version 0.16.1"; \
	  exit 1; }
	@# Give Valgrind line tables to symbolize WITHOUT touching the committed
	@# no-debug-symbol profiles: override strip/debug for THIS invocation only via
	@# env, so DWARF is not persisted into the checked-in bench profile.
	@# Build PORTABLY via the .cargo/bench-portable.toml fragment (x86-64-v3 Rust
	@# target-cpu + C/C++ flags), overriding the committed host-tuned
	@# -Ctarget-cpu=native / -march=native for THESE invocations only:
	@# retired-instruction counts must be MACHINE-INDEPENDENT (a host-tuned build
	@# defeats the whole point), and a host AVX-512 build makes Valgrind SIGILL on
	@# newer CPUs. Same portable floor the CI/release workflows use.
	CARGO_PROFILE_BENCH_STRIP=none CARGO_PROFILE_BENCH_DEBUG=line-tables-only \
	  cargo --config .cargo/bench-portable.toml bench -p gmeow-logic --bench engines_iai
	@# The whole-ontology-union conformance cost-partition: setup S vs per-twin scan
	@# V, in retired instructions. Grounds the off-gate decision deterministically.
	CARGO_PROFILE_BENCH_STRIP=none CARGO_PROFILE_BENCH_DEBUG=line-tables-only \
	  cargo --config .cargo/bench-portable.toml bench -p gmeow-validate --bench conformance_union_cost_iai
	@# The allocation half of the same partition (bytes / alloc count / peak-live).
	@# Needs NO Valgrind — the counts are host-independent — so it always runs.
	cargo bench -p gmeow-validate --bench conformance_union_cost_alloc

maint-bench-engines: ## (maintainer) Native benchmark over the committed mini corpora: emit deterministic cost/agreement + report-only wall/RSS evidence.
	@# The `bench-engines` harness drives every committed mini bench case through the
	@# native engine and compares it to hand-derived forward/existential goldens or
	@# captured SLD answer digests for backward cases, in-process with a fresh EDB
	@# per case. Offline: no network, no Valgrind.
	@#
	@# It produces TWO strictly-separated outputs: (2a) a gate-eligible cost/agreement
	@# artifact (integer cost vectors, consumed_steps, derived_count, deterministic
	@# peak-live bytes, verdict tokens, and the band-gated total-allocation scalars;
	@# NO wall-clock / peak-RSS), and (2b) a REPORT-ONLY advisory table on stderr
	@# carrying non-deterministic wall/RSS.
	@#
	@# R1 pool-quiesce: `main` pins the process-GLOBAL Rayon pool to a single thread
	@# (`ThreadPoolBuilder::new().num_threads(1).build_global()`) before any measured
	@# engine case — good hygiene that makes peak-live-bytes rock-solid. After every
	@# allocation-measured case, a dedicated local four-worker pool runs the permanent
	@# rule-parallel fixture and records scheduler-independent candidate-row work,
	@# merge-buffer bounds, full output/provenance parity, and budget-cut parity. It
	@# records no wall-time claim. The TOTAL allocation
	@# bytes/count still carry a small irreducible transient (rayon/allocator scratch,
	@# ~0.008% on the most-recursive case, proven by differing back-to-back in-process
	@# measures), so they use the documented one-sided tolerance bands; peak
	@# simultaneously-live bytes nets that scratch to zero and remains exact in (2a).
	@#
	@# Replay assertion: the second run uses the harness's OWN `--check-cost` contract.
	@# Every deterministic descriptor field (including peak-live) must match exactly;
	@# alloc_bytes must remain inside its one-sided 1% band; alloc_count uses the greater
	@# of 1% and the measured 42-allocation quantized floor. A raw whole-
	@# artifact diff would contradict the documented allocation-jitter contract.
	@set -e; \
	  tmpdir="$$(mktemp -d)"; \
	  trap 'rm -rf "$$tmpdir"' EXIT; \
	  echo "→ bench-engines run 1 (artifact + advisory table on stderr)"; \
	  cargo run -q -p gmeow-bench-engines --bin bench-engines -- --emit-cost "$$tmpdir/cost-1.json"; \
	  echo "→ bench-engines run 2 (exact-descriptor + allocation-band replay)"; \
	  if ! cargo run -q -p gmeow-bench-engines --bin bench-engines -- --check-cost "$$tmpdir/cost-1.json" >"$$tmpdir/replay.log" 2>&1; then \
	    cat "$$tmpdir/replay.log"; \
	    echo "ERROR: replay diverged in an exact descriptor or breached the allocation tolerance band."; \
	    exit 1; \
	  fi; \
	  echo "✓ deterministic descriptors are byte-identical and total allocations remain in band ($$(wc -c < "$$tmpdir/cost-1.json")-byte artifact)"

maint-bench-cost-baseline: ## (maintainer) Refresh bench/cost-baseline.json from a fresh native run (offline; the drift-gated cost-ledger source).
	@# The SINGLE producer of the committed deterministic cost/agreement baseline:
	@# `gmeow-bench-engines --emit-cost` over the committed mini corpora (offline; no
	@# `--corpus-dir`, so external corpora are NOT included). Mirrors
	@# `maint-bench-baseline`: a deliberate, hand-committed refresh — never auto-drift.
	@# The deterministic part of the artifact (integer cost vectors, consumed_steps,
	@# derived counts, peak-live bytes, verdict-agreement tokens, the four-worker
	@# structural evidence record, and the per-corpus divergence-ledger tally) is a pure
	@# function of engine version + corpus. The two
	@# TOTAL-allocation scalars (alloc_bytes / alloc_count) are NOT byte-reproducible
	@# (allocation counts move in a measured 14-allocation quantum across a 42-count
	@# span on small cases), so instead of a raw two-run byte-diff
	@# the fresh baseline is re-verified with the harness's OWN band-aware `--check-cost`:
	@# it hard-fails on ANY exact-descriptor divergence AND on an alloc total outside the
	@# one-sided tolerance band, so a passing self-check proves the deterministic part is
	@# byte-identical and the alloc totals are within band. `generated/bench/cost-ledger.md`
	@# is the drift-gated projection (the `stage-export-cost-ledger` leaf), regenerated +
	@# committed alongside.
	@set -e; \
	  replay_log="$$(mktemp)"; \
	  trap 'rm -f "$$replay_log"' EXIT; \
	  cargo run -q -p gmeow-bench-engines --bin bench-engines -- --emit-cost bench/cost-baseline.json; \
	  if ! cargo run -q -p gmeow-bench-engines --bin bench-engines -- --check-cost bench/cost-baseline.json >"$$replay_log" 2>&1; then \
	    cat "$$replay_log"; \
	    echo "ERROR: a fresh run diverged from the just-written bench/cost-baseline.json (exact descriptor drift, or an alloc total outside the tolerance band)."; \
	    exit 1; \
	  fi; \
	  echo "wrote bench/cost-baseline.json ($$(wc -c < bench/cost-baseline.json) bytes; deterministic part byte-identical + alloc totals within band on the self-check) — regenerate + commit generated/bench/cost-ledger.md"

# The bounded ORE subset cap: the ORE 2015 sample corpus is ~725 MB / 1920
# ontologies. Grading all of them is intractable for a maint lane, so we cap to
# the first ORE_GRADE_CAP ontologies of the EL-consistency task (the profile whose
# fragment the native DL consistency path is closest to deciding). Override on the
# command line (`make maint-external-corpora ORE_GRADE_CAP=50`) to widen the sweep.
ORE_GRADE_CAP ?= 12

maint-external-corpora: ## Grade the native reasoner against the full Lane-B external corpora (W3C OWL 2 suite + ORE 2015) over the network; record divergences as a gmeow:Finding graph.
	mkdir -p .tmp/w3c-owl2 generated/conformance
	curl -sSL https://www.w3.org/2009/11/owl-test/all.rdf -o .tmp/w3c-owl2/all.rdf
	cargo run -p gmeow-conformance --bin ingest-external -- --grade-suite .tmp/w3c-owl2/all.rdf w3c-owl2-full generated/conformance/divergence-w3c-owl2-full.nq
	# ── W3C OWL 2 EL profile suite (reproducible EL grade) ───────────────────
	# Grades only the EL-profile subset so the EL divergence numbers (DlGap /
	# CorpusOnly counts) have a committed, deterministic reproduction command.
	# The soundness gate hard-fails on any CorpusOnly row (wrong decided answer)
	# and on any DlGap outside the committed quarantine baseline.
	curl -sSL https://www.w3.org/2009/11/owl-test/profile-EL.rdf -o .tmp/w3c-owl2/profile-EL.rdf
	cargo run -p gmeow-conformance --bin ingest-external -- --grade-suite .tmp/w3c-owl2/profile-EL.rdf w3c-owl2-el-full generated/conformance/divergence-w3c-owl2-el-full.nq
	# ── ORE 2015 Reasoner Competition Corpus (Zenodo DOI 10.5281/zenodo.18578) ──
	# LICENSE: the corpus is licensed for reasoner-BENCHMARKING use ONLY — NOT
	# redistribution ("the open access extends only to this purpose; all individual
	# ontologies retain their own license restrictions; removal-on-request"). So we
	# DOWNLOAD it for benchmarking (license-compliant) and grade in a scratch temp
	# dir; NOTHING ORE is ever vendored/committed (.tmp + generated/conformance are
	# gitignored). The grade hard-fails on any fetch/extract/soundness error.
	bash -euo pipefail -c '\
	  cap=$(ORE_GRADE_CAP); \
	  zip=.tmp/ore2015/ore2015_sample.zip; \
	  sub=.tmp/ore2015/subset; \
	  mkdir -p .tmp/ore2015 "$$sub"; \
	  rm -f "$$sub"/*.owl; \
	  if [ ! -s "$$zip" ]; then \
	    curl -sSL -o "$$zip" "https://zenodo.org/api/records/18578/files/ore2015_sample.zip/content"; \
	  fi; \
	  unzip -o -q -j "$$zip" "pool_sample/el/consistency/fileorder.txt" -d .tmp/ore2015; \
	  files=$$(head -n "$$cap" .tmp/ore2015/fileorder.txt | tr -d "\r" | sed "s#^#pool_sample/files/#"); \
	  unzip -o -q -j "$$zip" $$files -d "$$sub"; \
	  test -n "$$(ls -A "$$sub"/*.owl 2>/dev/null)" || { echo "ORE extract produced no ontologies"; exit 1; }; \
	  cargo run -p gmeow-conformance --bin ingest-external -- --grade-ore "$$sub" ore2015-el-consistency generated/conformance/divergence-ore2015-el.nq; \
	'
	@echo "external-corpora grading complete; divergences in generated/conformance/divergence-w3c-owl2-full.nq + divergence-ore2015-el.nq"

# The scratch dir the Lane-B TPTP grade reads `*.p` problems from. Populate it from
# a local TPTP distribution checkout, or set TPTP_SUBSET_URL to a tarball of a
# decidable subset. Defaults to a gitignored scratch dir (never committed).
TPTP_PROBLEMS_DIR ?= .tmp/tptp
TPTP_SUBSET_URL ?=

maint-tptp-corpus: ## Grade the native FOL path against a live/local TPTP subset (Lane-B, per-problem licensed, NEVER vendored); record divergences as a gmeow:Finding graph.
	# `.tmp` holds the fetched subset tarball; create it explicitly so an overridden
	# TPTP_PROBLEMS_DIR (pointing outside `.tmp`) does not leave `curl -o .tmp/...` without a parent.
	mkdir -p .tmp $(TPTP_PROBLEMS_DIR) generated/conformance
	# TPTP problems carry PER-PROBLEM licenses and are NEVER vendored/committed
	# ($(TPTP_PROBLEMS_DIR) is a gitignored scratch dir). This lane parses the real
	# FOF/CNF bodies, applies the FOL-negation reduction, lowers the EL/DL fragment,
	# and grades the native decision against each problem's `% SZS status` / `% Status`
	# ground truth. A problem outside the native fragment is recorded as an honest
	# DlGap `gmeow:Finding` (never a silent pass). It is the documented path from the
	# tiny committed Lane-A `tptp-mini` corpus to the full distribution.
	bash -euo pipefail -c '\
	  dir="$(TPTP_PROBLEMS_DIR)"; url="$(TPTP_SUBSET_URL)"; \
	  if [ -n "$$url" ] && [ -z "$$(ls -A "$$dir"/*.p 2>/dev/null)" ]; then \
	    echo "fetching TPTP subset tarball $$url"; \
	    curl -sSL "$$url" -o .tmp/tptp-subset.tgz; \
	    tar -xzf .tmp/tptp-subset.tgz -C "$$dir" --strip-components=1 || tar -xzf .tmp/tptp-subset.tgz -C "$$dir"; \
	  fi; \
	  test -n "$$(find "$$dir" -name "*.p" -print -quit 2>/dev/null)" || { \
	    echo "no TPTP *.p problems under $$dir."; \
	    echo "populate it from a local TPTP checkout (cp \$$TPTP_HOME/Problems/SYN/*.p $$dir/),"; \
	    echo "or run: make maint-tptp-corpus TPTP_SUBSET_URL=<tarball-of-decidable-problems>"; \
	    exit 1; }; \
	  cargo run -p gmeow-conformance --bin ingest-external -- --grade-tptp "$$dir" tptp-live generated/conformance/divergence-tptp.nq; \
	'
	@echo "TPTP Lane-B grading complete; divergences in generated/conformance/divergence-tptp.nq"

maint-lang-selfhost: ## Gate-3 self-hosting differential: parse the repo's own slices/**/*.ttl with the native purrdf codec and check the lifted turtle.ebnf grammar structurally covers every construct the corpus exercises.
	# Off-gate corpus sweep (marked #[ignore]); runs with ZERO config against the
	# repo's own slices/ Turtle tree. Set GMEOW_TTL_CORPUS to point at a larger
	# external Turtle corpus. The lane hard-fails if the corpus is missing/empty or
	# if any repo document is not valid Turtle the sanctioned parser accepts.
	cargo test -p gmeow-lang-bridge --test grammar -- --ignored maint_grammar_selfhost_differential

# The scratch dir the Lane-B OntoUML grade reads catalog models from (ontology.ttl /
# model.ttl). Populate it from a local `ontouml-models` checkout, or set
# ONTOUML_SUBSET_URL to a tarball of a catalog subset. Defaults to a gitignored
# scratch dir (never committed).
ONTOUML_MODELS_DIR ?= .tmp/ontouml
ONTOUML_SUBSET_URL ?=

maint-ontouml-corpus: ## Grade the native foundation disciplines against a live/local FAIR OntoUML/UFO catalog subset (Lane-B, CC BY-SA — NEVER vendored); record divergences as a gmeow:Finding graph.
	# `.tmp` holds the fetched subset tarball; create it explicitly so an overridden
	# ONTOUML_MODELS_DIR (pointing outside `.tmp`) does not leave `curl -o .tmp/...` without a parent.
	mkdir -p .tmp $(ONTOUML_MODELS_DIR) generated/conformance
	# The FAIR OntoUML/UFO catalog is CC BY-SA 4.0 (ReferenceOnly under the native
	# license policy) and is NEVER vendored/committed ($(ONTOUML_MODELS_DIR) is a
	# gitignored scratch dir). This lane parses the real OntoUML metamodel models,
	# lowers the endurant-sortal + relator fragment onto the native foundation
	# disciplines, audits each model's own license from its metadata, and records every
	# fired discipline (a presumed-clean model that trips a discipline) or capability
	# gap as a gmeow:Finding. It is the documented path from the tiny committed Lane-A
	# `ontouml-mini` corpus to the full catalog.
	bash -euo pipefail -c '\
	  dir="$(ONTOUML_MODELS_DIR)"; url="$(ONTOUML_SUBSET_URL)"; \
	  if [ -n "$$url" ] && [ -z "$$(find "$$dir" \( -name "ontology.ttl" -o -name "model.ttl" \) -print -quit 2>/dev/null)" ]; then \
	    echo "fetching OntoUML catalog subset tarball $$url"; \
	    curl -sSL "$$url" -o .tmp/ontouml-subset.tgz; \
	    tar -xzf .tmp/ontouml-subset.tgz -C "$$dir" --strip-components=1 || tar -xzf .tmp/ontouml-subset.tgz -C "$$dir"; \
	  fi; \
	  test -n "$$(find "$$dir" \( -name "ontology.ttl" -o -name "model.ttl" \) -print -quit 2>/dev/null)" || { \
	    echo "no OntoUML models (ontology.ttl/model.ttl) under $$dir."; \
	    echo "populate it from a local ontouml-models checkout (git clone https://github.com/OntoUML/ontouml-models $$dir),"; \
	    echo "or run: make maint-ontouml-corpus ONTOUML_SUBSET_URL=<tarball-of-catalog-models>"; \
	    exit 1; }; \
	  cargo run -p gmeow-conformance --bin ingest-external -- --grade-ontouml "$$dir" ontouml-live generated/conformance/divergence-ontouml.nq; \
	'
	@echo "OntoUML Lane-B grading complete; divergences in generated/conformance/divergence-ontouml.nq"

# ── dbunibas/chasebench (NO LICENSE -> ReferenceOnly -> refused) ──────────────────
# The upstream ChaseBench distribution ships NO license file (verified: GitHub reports
# no license), so gmeow_license::policy_for_license classifies it ReferenceOnly: it may
# be REFERENCED but never vendored or converted. This lane FETCHES it, CHECKS the
# license, and HARD-FAILS. This is the honest no-optionality behavior: the lane
# exists, fetches, checks, and refuses.
CHASEBENCH_URL ?= https://github.com/dbunibas/chasebench/archive/refs/heads/master.tar.gz

maint-chasebench-corpus: ## (maintainer) Fetch dbunibas/chasebench, check its license, and HARD-FAIL (no license -> ReferenceOnly; NEVER in `make check`).
	mkdir -p .tmp/chasebench
	bash -euo pipefail -c '\
	  url="$(CHASEBENCH_URL)"; tgz=.tmp/chasebench/src.tar.gz; \
	  echo "-> fetching dbunibas/chasebench from $$url"; \
	  if curl -fsSL "$$url" -o "$$tgz"; then \
	    rm -rf .tmp/chasebench/chasebench-*; \
	    tar -xzf "$$tgz" -C .tmp/chasebench/ || true; \
	    root=$$(find .tmp/chasebench -maxdepth 1 -type d -name "chasebench-*" | head -1); \
	    if [ -n "$$root" ] && find "$$root" -maxdepth 1 \( -iname "license*" -o -iname "copying*" \) | grep -q .; then \
	      echo "UNEXPECTED: dbunibas/chasebench now ships a license file — re-audit it through"; \
	      echo "gmeow_license::policy_for_license before enabling conversion."; \
	      exit 1; \
	    fi; \
	    echo "-> fetched; confirmed NO license file is present in the upstream tree."; \
	  else \
	    echo "-> fetch failed (network unreachable or ref moved); the license status is unchanged."; \
	  fi; \
	  echo ""; \
	  echo "HARD FAIL: dbunibas/chasebench ships NO license. Under gmeow_license::policy_for_license"; \
	  echo "  an unlicensed corpus is ReferenceOnly — it CANNOT be vendored or converted into a"; \
	  echo "  runnable bench corpus."; \
	  echo "remediation: obtain an explicitly licensed upstream corpus before adding any runnable conversion."; \
	  exit 1; \
	'

# The token-cost-matrix fetch sources. o200k/cl100k are embedded (tiktoken-rs) and Qwen is
# vendored (Apache-2.0, blake3-pinned in-repo). Llama (Meta Llama 3 Community License) and
# Gemma (Gemma Terms of Use) are AGPL-INCOMPATIBLE restricted licenses, so — exactly like
# maint-tptp-corpus / maint-ontouml-corpus — their tokenizer.json assets are FETCHED here at
# maint-time into the git-ignored .tmp/, blake3-verified against the committed pins, used
# in-process, and NEVER committed. Defaults point at ungated faithful re-hosts of Meta's /
# Google's exact tokenizers (identical bytes => identical pin). To fetch from the canonical
# GATED repos (meta-llama / google) instead, override the URL and export HF_TOKEN (the fetched
# bytes must still match the pinned blake3, or the lane HARD-FAILS).
LLAMA_TOKENIZER_URL ?= https://huggingface.co/NousResearch/Meta-Llama-3-8B/resolve/main/tokenizer.json
GEMMA_TOKENIZER_URL ?= https://huggingface.co/unsloth/gemma-2-2b/resolve/main/tokenizer.json

maint-gmn-cost-matrix: ## (maintainer) Full five-family GMN token-cost matrix over the emitted GMN/Turtle/JSON-LD grounding serializations; flags byte-fragmenting glyphs per vocab. OFF-gate (INFORMS; never in `make check`).
	@# Runs the five mandated tokenizer families (o200k_base + cl100k_base embedded via
	@# tiktoken-rs; Qwen vendored + blake3-pinned; Llama + Gemma fetched-at-maint-time) over the
	@# SAME emitted GMN / Turtle / JSON-LD serializations of the grounding corpus the on-gate
	@# Task-7 byte-fallback estimator gates, and writes dist/bench/gmn-token-cost-matrix.md.
	@# NOT a `make check` (CHECK_DAG) target: it INFORMS the S2-S4 glyph/tokenizer co-design; the
	@# on-gate teeth remain `compute_token_metrics`. HARD-FAILS if ANY of the five families cannot
	@# be fetched/verified (no silent three-family degrade), and re-checks byte-identity across two
	@# runs like maint-bench-cost-baseline.
	@#
	@# Llama/Gemma license note: their licenses are non-free and AGPL-incompatible, so the assets
	@# are NEVER committed — fetched here, verified, and discarded, mirroring the repo's other
	@# restricted-license Lane-B corpora. See crates/gmn-cost-matrix/assets/vocab/PROVENANCE.md.
	bash -euo pipefail -c '\
	  dir=.tmp/gmn-cost-matrix; mkdir -p "$$dir"; \
	  if [ -n "$${HF_TOKEN:-}" ]; then AUTH=(-H "Authorization: Bearer $$HF_TOKEN"); else AUTH=(); fi; \
	  echo "-> fetching Llama tokenizer.json ($(LLAMA_TOKENIZER_URL))"; \
	  curl -fsSL $${AUTH[@]+"$${AUTH[@]}"} "$(LLAMA_TOKENIZER_URL)" -o "$$dir/llama.json" || { \
	    echo "ERROR: Llama tokenizer.json fetch failed. Override LLAMA_TOKENIZER_URL or (for the"; \
	    echo "  gated meta-llama repo) export HF_TOKEN. The five families are mandatory — no partial matrix."; \
	    exit 1; }; \
	  echo "-> fetching Gemma tokenizer.json ($(GEMMA_TOKENIZER_URL))"; \
	  curl -fsSL $${AUTH[@]+"$${AUTH[@]}"} "$(GEMMA_TOKENIZER_URL)" -o "$$dir/gemma.json" || { \
	    echo "ERROR: Gemma tokenizer.json fetch failed. Gemma is gated:manual on google/gemma-2-2b —"; \
	    echo "  accept its license and export HF_TOKEN, or override GEMMA_TOKENIZER_URL to an authorized"; \
	    echo "  source. The five families are mandatory — no partial matrix."; \
	    exit 1; }; \
	  echo "-> running the full five-family matrix (blake3-verifies each fetched asset)"; \
	  cargo run -q -p gmeow-gmn-cost-matrix --bin gmn-cost-matrix -- --llama "$$dir/llama.json" --gemma "$$dir/gemma.json"; \
	  echo "-> determinism re-check (second run must be byte-identical)"; \
	  cargo run -q -p gmeow-gmn-cost-matrix --bin gmn-cost-matrix -- --llama "$$dir/llama.json" --gemma "$$dir/gemma.json" --out "$$dir/recheck.md"; \
	  if ! diff -q dist/bench/gmn-token-cost-matrix.md "$$dir/recheck.md" >/dev/null; then \
	    echo "ERROR: the token-cost matrix drifted between two runs — non-deterministic render."; exit 1; fi; \
	  echo "✓ wrote dist/bench/gmn-token-cost-matrix.md (five families, byte-identical across two runs)"; \
	'

$(RUST_READY_STAMP): $(RUST_INPUTS)
	@mkdir -p $(dir $@)
	cargo nextest run --no-run $(RUST_TEST_WORKSPACE_ARGS) $(NEXTEST_PARTITION_ARG)
	@touch $@
