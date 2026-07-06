# GMEOW ontology toolchain - canonical task runner.
# Make is the task-oriented plan. Core logic lives in `gmeow-dev`, Rust crates,
# or focused scripts; this file names the workflows and their dependencies.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Maintainer extraction target. Override: make maint-extract TARGET=foaf
TARGET ?= foaf

# Override: make commit MESSAGE="feat: add foaf alignment"
MESSAGE ?= chore: regenerate checked-in artifacts
GMEOW_DEV ?= cargo run -q -p gmeow-dev-cli --
NPROC ?= $(shell nproc 2>/dev/null || echo 4)
# check-generated reproduces every committed artifact through the gmeow-pipeline DAG;
# its stages mix CPU work with artifact IO, so oversubscribing jobs past the core
# count overlaps the IO and measurably cuts wall-time (≈25% on a 2-core runner),
# keeping the ontology-generated CI lane under the 5-minute target. Bounded in
# practice by the DAG width per level.
CHECK_GENERATED_JOBS ?= $(shell echo $$(( $(NPROC) * 2 )))
CARGO_TARGET_DIR ?= target
SIGN_KEY ?=
PUBLIC_KEY ?= keys/gmeow-release-key.asc
GTS_OUT ?= dist/gmeow.gts
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
# `gmeow-native` is the single CPython extension cdylib and is covered by
# native-py/native-py-wheel. Selecting it in `cargo test` enables
# pyo3/extension-module for the shared pyo3 dependency, which is correct for a
# wheel but wrong for normal Rust test binaries.
RUST_TEST_WORKSPACE_ARGS := --workspace --exclude gmeow-native

# The wasm32 cross-build must be hermetic w.r.t. an ambient host RUSTFLAGS. Cargo gives
# the RUSTFLAGS env var precedence over `[target.wasm32-unknown-unknown].rustflags`, so a
# host-set hint (e.g. a local pyo3 `-L .../lib -lpython3.13 -Ctarget-cpu=native` link flag)
# leaks into the wasm target and breaks the link (`unable to find library -lpython3.13`,
# `'x86-64-v3' is not a recognized processor`). Strip it for every wasm cargo invocation so
# these targets build regardless of the caller's environment.
WASM_CARGO := env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo

# The committed .cargo/config.toml defaults LOCAL Rust/C builds to host-tuned
# codegen for regenerate/reasoning throughput. CI and release workflows append the
# portable x86-64-v3 Rust target-cpu and override the C/C++ flags explicitly.

# The enforced corpus-aggregate recall floor lives in Rust
# (scoreboards::ACCEPTANCE_MIN_RECALL_PCT — the single source of truth). Leave this
# EMPTY to enforce that native floor; set it only to OVERRIDE for a dev measurement
# (e.g. `make acceptance ACCEPTANCE_MIN_RECALL=0` to measure without a floor).
ACCEPTANCE_MIN_RECALL ?=
FUZZ_TARGETS = nquads gts shacl sssom statements
FUZZ_TIME ?= 30
MUTANTS_ARGS ?=

# Real Make artifacts for expensive native build preparation. These replace
# environment sentinels: source timestamps decide when rebuilds are needed.
NATIVE_PY_STAMP := .venv/.gmeow-native.stamp
RUST_READY_STAMP := $(CARGO_TARGET_DIR)/.gmeow-rust-ready.stamp
RUST_INPUTS := Cargo.toml Cargo.lock .cargo/config.toml $(shell find crates -type f \( -name Cargo.toml -o -name '*.rs' -o -name build.rs \) 2>/dev/null)
NATIVE_PY_INPUTS := pyproject.toml $(RUST_INPUTS)

CHECK_TARGETS := lint rust-gate validate check-generated constitution-check \
	crate-check audit wikidata coverage acceptance reason-verify reason-crosscheck \
	mappings lint-alignment doc-lint coherence-gate-teeth

.PHONY: help \
	install fmt lint lint-issue-refs \
	native-py native-py-wheel native-py-install validate validate-gts reason verify reason-verify reason-crosscheck test test-fast rust-build rust-test rust-docs check \
	regenerate fanout check-generated commit docs normalize build project release release-sign-gts full-release verify-release release-publish clean \
	mappings wikidata coverage acceptance crossref audit \
	constitution-check crate-check lint-alignment doc-lint rust-gate coherence-gate-teeth clippy carrier-purity wasm \
	lsp-build lsp-release lsp-sarif diagnostics-rust-sarif \
	slicetest conformance conformance-report insta-review \
	fuzz-smoke bench bench-compare rust-coverage mutants compliance-report perf-gate \
	maint-crosscheck \
	maint-extract maint-refresh-target-axioms maint-wikidata-live \
	maint-wikidata-coverage maint-wikidata-audit maint-test-heavy \
	maint-test-network maint-quality maint-evals-score \
	maint-compliance-report-full maint-bench-baseline maint-rust-heavy \
	maint-external-corpora maint-tptp-corpus maint-lang-selfhost

##@ Core Workflows

help: ## Show the task plan.
	@awk 'BEGIN {FS = ":.*## "; print "GMEOW task plan"} \
		/^##@ / {printf "\n%s\n", substr($$0, 5); next} \
		/^[A-Za-z0-9_.-]+:.*## / {printf "  \033[36m%-28s\033[0m %s\n", $$1, $$2}' \
		$(MAKEFILE_LIST)

install: ## Sync the uv environment, build the Rust CLIs, and configure repo-local Git merge drivers.
	uv sync --all-packages
	$(MAKE) cli-build
	bash scripts/bootstrap-git-merge-drivers.sh

fmt: ## Rewrite Python formatting with ruff.
	uv run ruff format .

lint-issue-refs: ## Reject issue/PR number references in Rust comments and Markdown docs.
	./scripts/lint-issue-refs.sh

lint: lint-issue-refs ## Run ruff, mypy, issue-ref lint, and the full pre-commit hygiene suite.
	uv run ruff check .
	uv run ruff format --check .
	uv run mypy
	uv run pre-commit run --all-files --show-diff-on-failure

validate: native-py ## Validate syntax, term annotations, SHACL, and DSL SHACL.
	$(GMEOW_DEV) validate

validate-gts: native-py ## Validate the committed generated/dist/gmeow.gts bundle.
	$(GMEOW_DEV) validate --gts generated/dist/gmeow.gts

reason: native-py ## Run the native Docker-free EL/DL reasoning authority.
	$(GMEOW_DEV) reason --mode native

verify: native-py ## Run native reasoned-graph negative tests.
	$(GMEOW_DEV) verify --mode native

reason-verify: native-py ## Run native reasoning + reasoned-graph verify with one closure.
	$(GMEOW_DEV) reason-verify

reason-crosscheck: native-py ## Cross-check native subsumptions against the purrdf-entail OWL-RL oracle (native ⊇ oracle).
	$(GMEOW_DEV) reason-crosscheck

test: native-py ## Run the pytest suite, excluding maintainer lanes.
	uv run pytest -n auto --dist loadscope --durations=25 -m "not maintainer"

test-fast: native-py ## Run the fast pytest suite, excluding maintainer lanes.
	uv run pytest -n auto --dist loadscope --durations=25 -m "not maintainer"

rust-build: $(RUST_READY_STAMP) ## Compile Rust workspace test binaries without running them.

rust-test: rust-build ## Run the Rust workspace tests and doctests.
	cargo run -q --package gmeow-docs --example prime-docs-fixture
	cargo nextest run --profile ci $(RUST_TEST_WORKSPACE_ARGS) $(NEXTEST_PARTITION_ARG)
	cargo run -q -p gmeow-test-budget -- target/nextest/ci/junit.xml
	cargo test --doc $(RUST_TEST_WORKSPACE_ARGS)

rust-docs: ## Build Rust API docs and fail on broken or redundant public rustdoc links.
	RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links -A rustdoc::private_intra_doc_links" cargo doc --workspace --no-deps

lsp-build: lsp-release ## Build the gmeow-lsp binary.

lsp-release: $(RUST_READY_STAMP) ## Build the gmeow-lsp release binary and stage it into dist/bin/.
	cargo build -p gmeow-lsp --release
	mkdir -p dist/bin
	cp $(CARGO_TARGET_DIR)/release/gmeow-lsp dist/bin/gmeow-lsp
	@echo "gmeow-lsp release binary staged at dist/bin/gmeow-lsp"

cli-build: $(RUST_READY_STAMP) ## Build the gmeow + gmeow-dev release binaries and stage them into dist/bin/.
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

check: native-py ## Run the full Docker-free local quality gate.
	# check-generated is one of CHECK_TARGETS, so it already runs as one of the
	# -j$(NPROC) outer jobs here; cap its inner pipeline pool to the outer count
	# (a command-line assignment overrides the CHECK_GENERATED_JOBS ?= NPROC*2
	# default) so the nested pools don't oversubscribe a small box. The standalone
	# `make check-generated` CI lane keeps the wider NPROC*2 IO-overlap pool.
	$(MAKE) -j$(NPROC) CHECK_GENERATED_JOBS=$(NPROC) $(CHECK_TARGETS)
	$(MAKE) test-fast
	$(MAKE) compliance-report
	@echo "all checks passed (Docker-free, Java-free)"

##@ Generated Artifacts And Outputs

regenerate: native-py ## Rebuild all checked-in generated artifacts from canonical sources.
	$(GMEOW_DEV) regenerate -j $(NPROC)

fanout: native-py ## Project the flat consumer tree back out of gmeow.gts (PIPELINE_SPINE §6).
	$(GMEOW_DEV) fanout -j $(NPROC)

check-generated: native-py ## Drift + orphan check for all registered generators.
	$(GMEOW_DEV) check-generated -j $(CHECK_GENERATED_JOBS)

commit: regenerate ## Regenerate artifacts, stage generator-owned outputs, and commit.
	@REGENERATED_PATHS=$$(GMEOW_CONSOLE=silent $(GMEOW_DEV) regenerate --list-paths); \
	for p in $${REGENERATED_PATHS}; do \
	  if [ -e "$$p" ]; then git add "$$p"; fi; \
	done; \
	if git diff --cached --quiet; then \
		echo "Nothing to commit."; exit 1; \
	else \
		git commit -m "$(MESSAGE)"; \
	fi
	@git diff --quiet || echo "Warning: unstaged changes remain. Stage them separately if needed."

docs: regenerate ## Regenerate gmeow.gts docs and extract ontology-docs/.
	$(GMEOW_DEV) extract-docs --directory ontology-docs --force generated/dist/gmeow.gts

normalize: ## Rewrite authored ontology sources into canonical serialization.
	$(GMEOW_DEV) normalize

build: cli-build ## Build the Rust CLIs plus serializations and JSON-LD context into dist/.
	$(GMEOW_DEV) build

project: ## Project GMEOW data to schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profiles.
	$(GMEOW_DEV) project

release: docs ## Regenerate, native-reason, build, report, docs, and emit CrossRef deposit.
	$(GMEOW_DEV) reason --mode native --merge
	$(MAKE) build
	$(MAKE) lsp-release
	$(MAKE) maint-compliance-report-full
	$(MAKE) crossref

release-sign-gts: native-py ## Sign the regenerated GTS bundle for release packaging.
	@if [ -z "$(SIGN_KEY)" ]; then \
		echo "SIGN_KEY=/path/to/secret.asc is required"; exit 1; \
	fi
	$(GMEOW_DEV) compile-gts --sign-key "$(SIGN_KEY)" --public-key "$(PUBLIC_KEY)" --out "$(GTS_OUT)"

full-release: native-py ## Signed release-as-evidence: gate + oracle lane + conformance + perf, folded + signed + DOI (§18).
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
		--evidence "generated/logic/dl-el-crosscheck-report.ttl:text/turtle:attestationTypeCrossCheckAgreement:nativeoracle:Native gap-zero DL-EL agreement ledger" \
		--evidence "bench/baseline.json:application/json:attestationTypeQualityReport:perf:Perf baseline"
	$(MAKE) verify-release
	$(MAKE) crossref
	@echo "full-release: signed evidence bundle written to $(GTS_OUT)"

verify-release: native-py ## Consumer verification of a signed release bundle: signature + trust policy + attestation frames (§18).
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
	sha256sum "$(GTS_OUT)" > "$(GTS_OUT).sha256"
	@echo "release bundle native content heads (BLAKE3):"
	uv run gts heads "$(GTS_OUT)"
	gh release create "$(RELEASE_TAG)" \
		"$(GTS_OUT)" "$(GTS_OUT).sha256" dist/crossref-deposit.xml \
		--title "GMEOW $(RELEASE_TAG) — signed release-as-evidence bundle" \
		--notes "Signed, content-addressed release bundle (§18). Verify with \`make verify-release\` or \`gts verify gmeow.gts\`; download integrity via the .sha256 sidecar; native content address via \`gts heads\`. The attached Crossref deposit is over the always-latest concept DOI (version-agnostic by design)."
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
	@echo "release-publish: published $(RELEASE_TAG) ($(GTS_OUT) + .sha256 + crossref deposit)."

clean: ## Remove ephemeral build artifacts.
	rm -rf dist docs/_generated .stamps $(NATIVE_PY_STAMP) $(RUST_READY_STAMP)
	@echo "cleaned ephemeral artifacts"

##@ Project Gates

mappings: ## Build alignment axioms and VoID linksets from SSSOM mappings.
	$(GMEOW_DEV) mappings

wikidata: ## Validate Wikidata QID/PID syntax in mappings, offline.
	$(GMEOW_DEV) wikidata

coverage: ## Gate vendored entity-slice class and predicate coverage.
	$(GMEOW_DEV) coverage --gaps --min-class 0.92 --min-predicate 0.85

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

rust-gate: rust-build carrier-purity ## Warm Rust once, then run the carrier-purity gate, clippy, nextest, the 25s budget gate, and doctests serially.
	cargo clippy --all-targets -- -D warnings
	cargo run -q --package gmeow-docs --example prime-docs-fixture
	cargo nextest run --profile ci $(RUST_TEST_WORKSPACE_ARGS) $(NEXTEST_PARTITION_ARG)
	cargo run -q -p gmeow-test-budget -- target/nextest/ci/junit.xml
	cargo test --doc $(RUST_TEST_WORKSPACE_ARGS)

coherence-gate-teeth: rust-build ## Run the whole-ontology coherence + relator-mediation gate teeth proofs on-gate (budget-exempt, ~95s).
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

wasm: ## Prove gmeow's wasm-clean crates (logic-compile + Tier-1 validator) build for wasm32.
	@# gmeow's own wasm-first crates MUST compile to wasm32 with a reasoning-runtime-free
	@# dep tree. The RDF/wasm engine is the sibling `purrdf` package (gated by its own
	@# CI), so this target proves only gmeow's own crates. The target's absence is a SKIP
	@# locally but a hard FAIL in CI, so the wasm-clean criterion is never silently
	@# unverified on the gating path.
	@if rustc --print target-list | grep -qx wasm32-unknown-unknown && rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then \
		echo "== compiler proof: gmeow-logic-compile (pure parse->IR->project) builds for wasm32 =="; \
		$(WASM_CARGO) build -p gmeow-logic-compile --target wasm32-unknown-unknown || { echo "FAIL: gmeow-logic-compile does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== purity gate: no reasoning-runtime crate may appear in the gmeow-logic-compile wasm dep tree =="; \
		for forbidden in oxigraph oxrocksdb nemo scryer-prolog tokio pyo3; do \
			if $(WASM_CARGO) tree -p gmeow-logic-compile -e no-dev --target wasm32-unknown-unknown -i $$forbidden >/dev/null 2>&1; then \
				echo "FAIL: gmeow-logic-compile leaked $$forbidden into its wasm dependency tree:"; \
				$(WASM_CARGO) tree -p gmeow-logic-compile -e no-dev --target wasm32-unknown-unknown -i $$forbidden; \
				exit 1; \
			fi; \
		done; \
		echo "== validator proof: gmeow-validate (Tier-1 core) + gmeow-validate-wasm build for wasm32 =="; \
		$(WASM_CARGO) build -p gmeow-validate --target wasm32-unknown-unknown || { echo "FAIL: gmeow-validate does not build for wasm32-unknown-unknown"; exit 1; }; \
		$(WASM_CARGO) build -p gmeow-validate-wasm --target wasm32-unknown-unknown || { echo "FAIL: gmeow-validate-wasm does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== purity gate: no reasoner / native-only crate may appear in the validator wasm dep tree =="; \
		: "rayon is intentionally NOT forbidden — it cross-compiles to wasm32 and degrades to sequential when threads are unavailable (wasm-safe data-parallelism, not a reasoner/native-only crate); purrdf's RDF/SHACL core uses it and the wasm build links cleanly"; \
		for vpkg in gmeow-validate gmeow-validate-wasm; do \
			for forbidden in oxigraph oxrocksdb nemo scryer-prolog tokio pyo3 ureq duckdb ring; do \
				if $(WASM_CARGO) tree -p $$vpkg -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -qE "(^| )$$forbidden v[0-9]"; then \
					echo "FAIL: $$vpkg leaked $$forbidden into its wasm dependency tree:"; \
					$(WASM_CARGO) tree -p $$vpkg -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -E "(^| )$$forbidden v[0-9]"; \
					exit 1; \
				fi; \
			done; \
		done; \
		echo "OK: gmeow-logic-compile + the wasm Tier-1 validator build for wasm32 (dep trees are reasoning-runtime-free)"; \
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
	wasm-opt -Oz -o crates/validate-wasm/js/pkg/gmeow_validate_wasm_bg.wasm crates/validate-wasm/js/pkg/gmeow_validate_wasm_bg.wasm
	@echo "OK: wasm-opt -Oz applied"
	@echo "OK: gmeow-validate-wasm npm package built (crates/validate-wasm/js/, pkg/ generated)"

validate-wasm-pkg-test: validate-wasm-pkg ## Build the validator npm package and run its Node real-execution round-trip lane.
	@# The purrdf RDF/wasm engine ships + tests its own npm package on purrdf's CI; this
	@# lane proves ONLY gmeow's own deliverable — the Tier-1 validator wasm package — by
	@# validating a real dataset against the committed gmeow.gts through the wasm-bindgen
	@# bindings just built above.
	cd crates/validate-wasm/js && node --test tests/*.test.mjs
	@echo "OK: gmeow-validate-wasm Node round-trip lane passed"

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

perf-gate: native-py ## Report-only timings for validate, generated drift, reason, and verify.
	mkdir -p $(PERF_DIR)
	$(GMEOW_DEV) validate --timings --timings-json $(PERF_DIR)/validate.json
	$(GMEOW_DEV) check-generated -j $(CHECK_GENERATED_JOBS) --timings-json $(PERF_DIR)/check-generated.json
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

maint-crosscheck: native-py ## Prove every committed query answers on the native purrdf engine.
	$(GMEOW_DEV) crosscheck-queries

maint-extract: native-py ## Run import/extract policy for TARGET.
	$(GMEOW_DEV) extract --target $(TARGET)

maint-refresh-target-axioms: ## Re-vendor minimal target-axiom snapshots.
	$(GMEOW_DEV) refresh-target-axioms --target all

maint-wikidata-live: ## Verify Wikidata identifiers resolve over the network.
	$(GMEOW_DEV) wikidata --existence

maint-wikidata-coverage: ## Report Wikidata mapping coverage by domain.
	$(GMEOW_DEV) wikidata-coverage

maint-wikidata-audit: ## Audit fixtures and modules for Wikidata misuse.
	$(GMEOW_DEV) wikidata --fixtures

maint-test-heavy: native-py ## Run kept-Python-module maintainer tests.
	uv run pytest -n auto --dist loadscope -m "maintainer"

maint-test-network: ## Run live network tests.
	GMEOW_RUN_NETWORK=1 uv run pytest -m network

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

native-py: $(NATIVE_PY_STAMP)

$(NATIVE_PY_STAMP): $(NATIVE_PY_INPUTS)
	# Build --release: the native reasoner (Nemo chase + RDFC-1.0 canonicalization +
	# Turtle serialization) is pure CPU and dominates every gate that runs the
	# pipeline. MEASURED 2026-06-28: a release ext cuts `make regenerate` 353s → 81s
	# (4.4x) with byte-identical output. The `[profile.release]` overrides in
	# Cargo.toml already cap Nemo's build RAM (opt-2/256-units, measured 7.37 GB —
	# within the 16 GB wheel-runner budget), so the cutover is safe.
	VIRTUAL_ENV="$(CURDIR)/.venv" uvx maturin develop --release --manifest-path crates/native/Cargo.toml
	@touch $@

native-py-wheel: ## Build the unified gmeow_native wheel into dist/wheels (CI prebuild-once).
	rm -rf dist/wheels
	# Build from crates/native/ so maturin resolves `python-source = "python"`
	# (the legacy gmeow_* import shims) relative to that pyproject, not the repo root.
	# `--compatibility linux` skips auditwheel repair (no system-lib bundling): the
	# prebuild job and every consumer run on the same ubuntu-latest image, so a plain
	# linux wheel is correct and avoids the repair step. --release matches the
	# `maturin develop --release` in `native-py`: the optimized reasoner cuts every
	# pipeline gate (regenerate/check-generated/validate) ~4.4x; Nemo's release build
	# RAM is capped to 7.37 GB by the `[profile.release]` overrides in Cargo.toml.
	cd crates/native && VIRTUAL_ENV="$(CURDIR)/.venv" uvx maturin build --release --compatibility linux -o "$(CURDIR)/dist/wheels"

native-py-install: ## Install the prebuilt unified wheel from dist/wheels (CI consumers); hard-fail if absent/ambiguous.
	set -eu; \
	shopt -s nullglob; \
	wheels=(dist/wheels/*.whl); \
	if [ $${#wheels[@]} -ne 1 ]; then \
		echo "native-py-install: expected exactly one wheel in dist/wheels, found $${#wheels[@]} — no fallback to maturin develop" >&2; \
		exit 1; \
	fi; \
	VIRTUAL_ENV="$(CURDIR)/.venv" uv pip install --no-deps --force-reinstall "$${wheels[0]}"; \
	site="$$(VIRTUAL_ENV="$(CURDIR)/.venv" uv run python -c 'import sysconfig; print(sysconfig.get_path("purelib"))')"; \
	if [ -z "$$site" ]; then echo "native-py-install: could not resolve site-packages" >&2; exit 1; fi; \
	for pkg in gmeow_diagnostics gmeow_docs gmeow_logic gmeow_validate; do \
		rm -rf "$$site/$$pkg"; \
		cp -r "crates/native/python/$$pkg" "$$site/$$pkg"; \
	done
	# The wheel ships only the `gmeow_native` cdylib; the tiny pure-Python legacy
	# import shims (gmeow_logic → gmeow_native.logic, etc.) live in
	# crates/native/python/ and `maturin develop` exposes them editable. Here the repo
	# is checked out, so copy the sibling shim packages into site-packages alongside
	# the installed cdylib so `import gmeow_logic` (and friends) resolves.
	@mkdir -p $(dir $(NATIVE_PY_STAMP))
	@touch $(NATIVE_PY_STAMP)
	# Mark the native-ext stamp satisfied so downstream gates (`validate`,
	# `check-generated`, ...) that depend on `native-py` do NOT re-run `maturin
	# develop` — the unified extension is already installed from the prebuilt wheel.

$(RUST_READY_STAMP): $(RUST_INPUTS)
	@mkdir -p $(dir $@)
	cargo nextest run --no-run $(RUST_TEST_WORKSPACE_ARGS) $(NEXTEST_PARTITION_ARG)
	@touch $@
