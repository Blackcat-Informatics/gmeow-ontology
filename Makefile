# GMEOW ontology toolchain - canonical task runner.
# Make is the task-oriented plan. Core logic lives in `gmeow-dev`, Rust crates,
# or focused scripts; this file names the workflows and their dependencies.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Maintainer extraction target. Override: make maint-extract TARGET=foaf
TARGET ?= foaf

# Override: make commit MESSAGE="feat: add foaf alignment"
MESSAGE ?= "chore: regenerate checked-in artifacts"
GMEOW_DEV ?= uv run --package gmeow-dev gmeow-dev
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

# The committed .cargo/config.toml defaults LOCAL Rust/C builds to host-tuned
# codegen for regenerate/reasoning throughput. CI and release workflows append the
# portable x86-64-v3 Rust target-cpu and override the C/C++ flags explicitly.

ACCEPTANCE_MIN_RECALL ?= 60
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
	crate-check audit wikidata coverage acceptance reason verify mappings \
	lint-alignment doc-lint sparql-conformance gts-codec-hygiene

.PHONY: help \
	install fmt lint \
	native-py native-py-wheel native-py-install validate validate-gts reason verify test test-fast rust-build rust-test rust-docs check \
	regenerate check-generated commit docs normalize build project release release-sign-gts full-release verify-release release-publish clean \
	mappings wikidata coverage acceptance crossref audit \
	constitution-check crate-check lint-alignment doc-lint rust-gate clippy rdf-core-hygiene carrier-purity gts-codec-hygiene sparql-conformance wasm wasm-pkg wasm-pkg-test \
	capi-build capi-header capi-check capi-install \
	lsp-build lsp-release lsp-sarif diagnostics-rust-sarif \
	slicetest conformance conformance-report insta-review \
	fuzz-smoke bench bench-compare rust-coverage mutants compliance-report perf-gate \
	maint-classic-cross-check maint-reason-hermit maint-explain maint-697-oracle-gold maint-verify-docker \
	maint-reasoning-cases maint-statements-docker-check maint-crosscheck \
	maint-extract maint-refresh-target-axioms maint-wikidata-live \
	maint-wikidata-coverage maint-wikidata-audit maint-test-heavy \
	maint-test-network maint-test-network-rust maint-pull-images maint-quality maint-evals-score \
	maint-compliance-report-full maint-bench-baseline maint-rust-heavy \
	maint-external-corpora maint-capture-sparql-goldens

##@ Core Workflows

help: ## Show the task plan.
	@awk 'BEGIN {FS = ":.*## "; print "GMEOW task plan"} \
		/^##@ / {printf "\n%s\n", substr($$0, 5); next} \
		/^[A-Za-z0-9_.-]+:.*## / {printf "  \033[36m%-28s\033[0m %s\n", $$1, $$2}' \
		$(MAKEFILE_LIST)

install: ## Sync the uv environment and configure repo-local Git merge drivers.
	uv sync --all-packages
	bash scripts/bootstrap-git-merge-drivers.sh

fmt: ## Rewrite Python formatting with ruff.
	uv run ruff format .

lint: ## Run ruff, mypy, and the full pre-commit hygiene suite.
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

test: native-py ## Run the pytest suite, excluding maintainer and oracle lanes.
	uv run pytest -n auto --dist loadscope --durations=25 -m "not maintainer and not classic_cross_check"

test-fast: native-py ## Run the fast pytest suite, excluding maintainer, Docker, and oracle lanes.
	uv run pytest -n auto --dist loadscope --durations=25 -m "not maintainer and not docker and not classic_cross_check"

rust-build: $(RUST_READY_STAMP) ## Compile Rust workspace test binaries without running them.

rust-test: rust-build ## Run the Rust workspace tests and doctests.
	cargo run -q --package gmeow-docs --example prime-docs-fixture
	cargo nextest run --profile ci $(NEXTEST_PARTITION_ARG)
	cargo run -q -p gmeow-test-budget -- target/nextest/ci/junit.xml
	cargo test --doc

rust-docs: ## Build Rust API docs and fail on broken or redundant public rustdoc links.
	RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links -A rustdoc::private_intra_doc_links" cargo doc --workspace --no-deps

lsp-build: lsp-release ## Build the gmeow-lsp binary.

lsp-release: $(RUST_READY_STAMP) ## Build the gmeow-lsp release binary and stage it into dist/bin/.
	cargo build -p gmeow-lsp --release
	mkdir -p dist/bin
	cp $(CARGO_TARGET_DIR)/release/gmeow-lsp dist/bin/gmeow-lsp
	@echo "gmeow-lsp release binary staged at dist/bin/gmeow-lsp"

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

check-generated: native-py ## Drift + orphan check for all registered generators.
	$(GMEOW_DEV) check-generated -j $(CHECK_GENERATED_JOBS)

commit: regenerate ## Regenerate artifacts, stage generator-owned outputs, and commit.
	@REGENERATED_PATHS=$$(uv run python -c "from gmeow_tools.load_generators import load_all; load_all(); from gmeow_tools.generator import all_regenerated_paths; print(' '.join(all_regenerated_paths()))"); \
	git add $${REGENERATED_PATHS}; \
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

build: ## Build serializations and JSON-LD context into dist/.
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
	$(MAKE) maint-classic-cross-check
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
		--evidence "dist/gmeow-classic-cross-check.sarif:application/sarif+json:attestationTypeCrossCheckAgreement:crosscheck:Classic cross-check agreement matrix" \
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
		--notes "Signed, content-addressed release bundle (§18). Verify with \`make verify-release\` or \`gts verify gmeow.gts\`; download integrity via the .sha256 sidecar; native content address via \`gts heads\`. The attached Crossref deposit is over the always-latest concept DOI (version-agnostic by design, #44)."
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
	$(GMEOW_DEV) acceptance --min-recall $(ACCEPTANCE_MIN_RECALL)

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
	cargo nextest run --profile ci $(NEXTEST_PARTITION_ARG)
	cargo run -q -p gmeow-test-budget -- target/nextest/ci/junit.xml
	cargo test --doc

sparql-conformance: rust-build ## Run the OXIGRAPH-FREE native frozen-golden SPARQL corpus conformance gate (EPIC #906 Task 4).
	@# Replays every captured query through NativeSparqlEngine over the merged ontology,
	@# loaded oxigraph-free + flattened identically to the capture, and asserts equality
	@# vs the frozen oxigraph goldens. Default nextest profile excludes the off-gate-heavy
	@# sweep (run on `make maint-rust-heavy`). MUST NOT require the oxigraph feature.
	@# Hygiene: the conformance crate must carry NO oxigraph normal dependency, so it
	@# survives oxigraph removal in Task 8.
	@tree=$$(cargo tree -p gmeow-sparql-conformance --edges normal -f "{p}") || { echo "FAIL: cargo tree errored"; exit 1; }; \
	if echo "$$tree" | grep -q 'oxigraph v'; then \
		echo "FAIL: oxigraph is a NORMAL dependency of gmeow-sparql-conformance"; \
		echo "$$tree" | grep 'oxigraph v'; exit 1; \
	else \
		echo "OK: gmeow-sparql-conformance has no oxigraph normal dependency"; \
	fi
	cargo nextest run -p gmeow-sparql-conformance

clippy: rust-build ## Run cargo clippy on all Rust targets with warnings as errors.
	cargo clippy --all-targets -- -D warnings

carrier-purity: rust-build ## Prove the pipeline inter-stage carrier/transport path uses no oxigraph Store accumulation (#1132 C11).
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

gts-codec-hygiene: rust-build ## Lock the native RDF codec seam: no gmeow_gts codec entrypoint + no oxigraph in production source.
	@# STRUCTURAL boundary-lock gate. The whole RDF codec seam is native:
	@# crates/rdf/src/native_codecs/ parses/serializes RDF on the first-party IR with
	@# NO gmeow_gts codec and NO oxigraph in the middle. The test scans crates/*/src for
	@# three rules: (1) native_codecs/ is 100% gmeow_gts- AND oxigraph-free (prod OR test);
	@# (2) the gmeow_gts RDF-codec ENTRYPOINTS (nquads/trig/yamlld/rdf_xml/rdf_codecs/rdf::)
	@# are banned in PRODUCTION — gmeow.gts CONTAINER symbols (reader/writer/model/verify/
	@# policy/codec::/ulid/…) STAY and are explicitly allowed; (3) no oxigraph-family crate
	@# reference in PRODUCTION (complements the crate-dep `rdf-core-hygiene` lock). The
	@# bundled negative-arm unit tests prove the detector flags each forbidden token and
	@# does NOT flag an allowed container symbol, so the gate can never silently pass.
	cargo nextest run -p gmeow-rdf --test gts_codec_hygiene
	@echo "OK: RDF codec seam is native — no gmeow_gts codec entrypoint, no oxigraph in production source"

rdf-core-hygiene: ## Prove gmeow-rdf-core has no oxigraph normal dependency.
	cargo build -p gmeow-rdf-core
	@tree=$$(cargo tree -p gmeow-rdf-core --edges normal -f "{p}") || { echo "FAIL: cargo tree errored"; exit 1; }; \
	if echo "$$tree" | grep -q 'oxigraph v'; then \
		echo "FAIL: oxigraph is a NORMAL dependency of gmeow-rdf-core"; \
		echo "$$tree" | grep 'oxigraph v'; exit 1; \
	else \
		echo "OK: gmeow-rdf-core has no oxigraph normal dependency"; \
	fi
	@# [purrdf S2/#908] The native gmeow-iri leaf REPLACES `oxiri`; assert it pulls
	@# NO oxigraph-family crate (umbrella + any ox*/spar* leaf) into its normal tree.
	@itree=$$(cargo tree -p gmeow-iri --edges normal -f "{p}") || { echo "FAIL: cargo tree errored for gmeow-iri"; exit 1; }; \
	if echo "$$itree" | grep -Eq '(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld) v'; then \
		echo "FAIL: gmeow-iri pulls an oxigraph-family crate — the S2 zero-dep replacement is BROKEN"; \
		echo "$$itree" | grep -E '(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld) v'; exit 1; \
	else \
		echo "OK: gmeow-iri has NO oxigraph-family crate in its normal dependency tree (the #908 oxiri replacement is clean)"; \
	fi
	@# [purrdf S5/#911] The native gmeow-sparql-algebra leaf REPLACES `spargebra`;
	@# assert it pulls NO oxigraph-family crate into its normal tree.
	@stree=$$(cargo tree -p gmeow-sparql-algebra --edges normal -f "{p}") || { echo "FAIL: cargo tree errored for gmeow-sparql-algebra"; exit 1; }; \
	if echo "$$stree" | grep -Eq '(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld) v'; then \
		echo "FAIL: gmeow-sparql-algebra pulls an oxigraph-family crate — the S5 spargebra replacement is BROKEN"; \
		echo "$$stree" | grep -E '(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld) v'; exit 1; \
	else \
		echo "OK: gmeow-sparql-algebra has NO oxigraph-family crate in its normal dependency tree (the #911 spargebra replacement is clean)"; \
	fi
	@# [purrdf S6/#912] The native gmeow-sparql-eval evaluator REPLACES `spareval`;
	@# assert it pulls NO oxigraph-family crate into its normal OR dev tree. The
	@# dev-tree check is stronger than the sibling leaves': this is the wasm query
	@# path and its differential parity goldens are checked-in DATA (the oxigraph
	@# baseline-capture lives only in `crates/rdf`), so sparql-eval must never gain
	@# an oxigraph-family edge — not even a dev-dependency.
	@etree=$$(cargo tree -p gmeow-sparql-eval --edges normal,dev -f "{p}") || { echo "FAIL: cargo tree errored for gmeow-sparql-eval"; exit 1; }; \
	if echo "$$etree" | grep -Eq '(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld) v'; then \
		echo "FAIL: gmeow-sparql-eval pulls an oxigraph-family crate — the S6 spareval replacement is BROKEN"; \
		echo "$$etree" | grep -E '(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld) v'; exit 1; \
	else \
		echo "OK: gmeow-sparql-eval has NO oxigraph-family crate in its normal/dev dependency tree (the #912 spareval replacement is clean)"; \
	fi
	@# [purrdf S9/#915] The native gmeow-sparql-results serializer REPLACES
	@# `sparesults`; assert it pulls NO oxigraph-family crate into its normal OR
	@# dev tree (it carries checked-in golden DATA and is on the wasm-first path).
	@rtree=$$(cargo tree -p gmeow-sparql-results --edges normal,dev -f "{p}") || { echo "FAIL: cargo tree errored for gmeow-sparql-results"; exit 1; }; \
	if echo "$$rtree" | grep -Eq '(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld) v'; then \
		echo "FAIL: gmeow-sparql-results pulls an oxigraph-family crate — the S9 sparesults replacement is BROKEN"; \
		echo "$$rtree" | grep -E '(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld) v'; exit 1; \
	else \
		echo "OK: gmeow-sparql-results has NO oxigraph-family crate in its normal/dev dependency tree (the #915 sparesults replacement is clean)"; \
	fi
	@# [purrdf S5/#911 + S6/#912] EPIC #906 is wasm-first: the SPARQL leaves MUST
	@# compile to wasm32. The target's absence is a SKIP locally but a hard FAIL in
	@# CI, so the wasm-clean criterion is never silently unverified on the gating path.
	@if rustc --print target-list | grep -qx wasm32-unknown-unknown && rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then \
		cargo build -p gmeow-sparql-algebra --target wasm32-unknown-unknown || { echo "FAIL: gmeow-sparql-algebra does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "OK: gmeow-sparql-algebra builds for wasm32-unknown-unknown (wasm-clean)"; \
		cargo build -p gmeow-sparql-eval --target wasm32-unknown-unknown || { echo "FAIL: gmeow-sparql-eval does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "OK: gmeow-sparql-eval builds for wasm32-unknown-unknown (wasm-clean)"; \
		cargo build -p gmeow-sparql-results --target wasm32-unknown-unknown || { echo "FAIL: gmeow-sparql-results does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "OK: gmeow-sparql-results builds for wasm32-unknown-unknown (wasm-clean)"; \
	elif [ -n "$${CI:-}" ]; then \
		echo "FAIL: wasm32-unknown-unknown target absent in CI — the wasm-first criterion (#906) cannot be verified; CI must install it"; exit 1; \
	else \
		echo "SKIP: wasm32-unknown-unknown target not installed (local only; CI hard-fails) — 'rustup target add wasm32-unknown-unknown' to enable the wasm-clean check"; \
	fi
	@# [EPIC #906] No FIRST-PARTY crate may name oxigraph or any oxigraph-family crate
	@# as a DIRECT dependency. The oxigraph umbrella is fully removed; the residual
	@# ox*-family sub-crates (oxrdf/oxttl/oxiri/spargebra/oxsdatatypes/…) survive ONLY
	@# transitively through two EXTERNAL crates — nemo (the Datalog chase engine) and
	@# gmeow-gts — which are outside this workspace and on their own retirement track,
	@# so a whole-workspace `cargo tree` ban is not achievable here; this grep guards
	@# against a first-party regression that re-introduces a direct edge.
	@hits=$$(grep -rnE '^\s*(oxigraph|oxrdf|oxsdatatypes|oxiri|spargebra|spareval|sparopt|sparesults|oxttl|oxrdfio|oxrdfxml|oxjsonld)\s*=|"oxigraph"|dep:oxigraph' crates/*/Cargo.toml 2>/dev/null || true); \
	if [ -n "$$hits" ]; then \
		echo "FAIL: a first-party crate names an oxigraph-family crate as a direct dependency:"; \
		echo "$$hits"; exit 1; \
	else \
		echo "OK: no first-party crate has a direct oxigraph-family dependency (umbrella removed; residual ox* is external nemo/gmeow-gts only)"; \
	fi

CAPI_HEADER := crates/rdf-capi/include/purrdf.h

capi-build: ## Build libpurrdf (cdylib + staticlib + header + pkg-config) via cargo-c.
	cargo capi build -p gmeow-rdf-capi

capi-header: ## Regenerate the committed purrdf.h ABI contract from the crate.
	@touch crates/rdf-capi/src/lib.rs  # cargo-c only re-runs cbindgen when the crate recompiles; force it so a cache-fresh build cannot serve a stale header
	cargo capi build -p gmeow-rdf-capi
	@hdr=$$(find $(CARGO_TARGET_DIR) -path '*/include/purrdf/purrdf.h' | head -1); \
	  test -n "$$hdr" || { echo "FAIL: cargo-c did not emit purrdf.h"; exit 1; }; \
	  cp "$$hdr" $(CAPI_HEADER); echo "regenerated $(CAPI_HEADER)"

capi-check: ## Verify the committed purrdf.h is current + the C smoke links and runs.
	@touch crates/rdf-capi/src/lib.rs  # force cbindgen to re-run; otherwise CI's rust-cache can restore a stale header and the drift diff is meaningless
	cargo capi build -p gmeow-rdf-capi
	@hdr=$$(find $(CARGO_TARGET_DIR) -path '*/include/purrdf/purrdf.h' | head -1); \
	  test -n "$$hdr" || { echo "FAIL: cargo-c did not emit purrdf.h"; exit 1; }; \
	  if ! diff -q "$$hdr" $(CAPI_HEADER) >/dev/null; then \
	    echo "FAIL: $(CAPI_HEADER) is STALE — run 'make capi-header' and commit the ABI header"; \
	    diff $(CAPI_HEADER) "$$hdr" | head -40; exit 1; \
	  fi; \
	  echo "OK: committed purrdf.h matches the libpurrdf ABI surface"
	cargo test -p gmeow-rdf-capi --test c_smoke  # the smoke driver self-builds the cdylib if absent (hermetic in every lane)

capi-install: ## Install libpurrdf + purrdf.pc + header to PREFIX (default /usr/local).
	cargo capi install -p gmeow-rdf-capi --prefix="$(if $(PREFIX),$(PREFIX),/usr/local)"

wasm: ## Build the purrdf wasm engine (P10, #846) for wasm32-unknown-unknown.
	@# EPIC #832 P10 is wasm-first: the in-memory RDF/JS engine MUST compile to
	@# wasm32. The target's absence is a SKIP locally but a hard FAIL in CI, so the
	@# wasm-clean criterion is never silently unverified on the gating path (same
	@# idiom as `rdf-core-hygiene`'s sparql-algebra wasm check).
	@if rustc --print target-list | grep -qx wasm32-unknown-unknown && rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then \
		echo "== engine proof: gmeow-rdf (gts, no oxigraph/python) builds for wasm32 =="; \
		cargo build -p gmeow-rdf --no-default-features --features gts --target wasm32-unknown-unknown || { echo "FAIL: gmeow-rdf does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== binding proof: the purrdf cdylib builds for wasm32 =="; \
		cargo build -p gmeow-rdf-wasm --target wasm32-unknown-unknown || { echo "FAIL: gmeow-rdf-wasm (purrdf) does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== compiler proof: gmeow-logic-compile (pure parse->IR->project) builds for wasm32 (#664/#732) =="; \
		cargo build -p gmeow-logic-compile --target wasm32-unknown-unknown || { echo "FAIL: gmeow-logic-compile does not build for wasm32-unknown-unknown"; exit 1; }; \
		echo "== purity gate: no reasoning-runtime crate may appear in the gmeow-logic-compile wasm dep tree =="; \
		for forbidden in oxigraph oxrocksdb nemo scryer-prolog tokio pyo3; do \
			if cargo tree -p gmeow-logic-compile -e no-dev --target wasm32-unknown-unknown -i $$forbidden >/dev/null 2>&1; then \
				echo "FAIL: gmeow-logic-compile leaked $$forbidden into its wasm dependency tree:"; \
				cargo tree -p gmeow-logic-compile -e no-dev --target wasm32-unknown-unknown -i $$forbidden; \
				exit 1; \
			fi; \
		done; \
		echo "OK: purrdf wasm engine + bindings + gmeow-logic-compile build for wasm32 (compiler dep tree is reasoning-runtime-free)"; \
	elif [ -n "$${CI:-}" ]; then \
		echo "FAIL: wasm32-unknown-unknown target absent in CI — the P10 wasm-first criterion (#846) cannot be verified; CI must install it"; exit 1; \
	else \
		echo "SKIP: wasm32-unknown-unknown target not installed (local only; CI hard-fails) — 'rustup target add wasm32-unknown-unknown' to enable the purrdf wasm build"; \
	fi

wasm-pkg: ## Build the purrdf npm/ESM package (release wasm + wasm-bindgen web bindings).
	@# Release-build the cdylib, then run `wasm-bindgen` (pinned =0.2.125, matching the
	@# crate) to emit the ESM `web`-target JS bindings + .d.ts + .wasm into js/pkg/.
	@# `~/.cargo/bin` carries the cli on both CI runners and local dev installs.
	cargo build -p gmeow-rdf-wasm --target wasm32-unknown-unknown --release
	PATH="$$HOME/.cargo/bin:$$PATH" wasm-bindgen \
		$(CARGO_TARGET_DIR)/wasm32-unknown-unknown/release/gmeow_rdf_wasm.wasm \
		--out-dir crates/rdf-wasm/js/pkg --target web
	@# Size optimization is best-effort: wasm-opt -Oz roughly halves the artifact, but
	@# the package is correct without it. Absence is a note, not a failure.
	@if command -v wasm-opt >/dev/null 2>&1; then \
		wasm-opt -Oz -o crates/rdf-wasm/js/pkg/gmeow_rdf_wasm_bg.wasm crates/rdf-wasm/js/pkg/gmeow_rdf_wasm_bg.wasm && \
		echo "OK: wasm-opt -Oz applied"; \
	else \
		echo "note: wasm-opt not found — shipping unoptimized wasm (size-opt is a follow-up)"; \
	fi
	@echo "OK: purrdf npm package built (crates/rdf-wasm/js/, pkg/ generated)"

wasm-pkg-test: wasm-pkg ## Build the purrdf package and run the Node real-execution round-trip lane.
	cd crates/rdf-wasm/js && node --test tests/*.test.mjs

maint-rust-heavy: rust-build ## Run the Rust suite INCLUDING the off-gate heavy tests (#1045 maint-heavy profile).
	cargo run -q --package gmeow-docs --example prime-docs-fixture
	cargo nextest run --profile maint-heavy $(NEXTEST_PARTITION_ARG)

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
	cargo bench -p gmeow-logic -p gmeow-rdf -p gmeow-shacl -p gmeow-validate -p gmeow-sparql-eval

bench-compare: ## Report-only perf scoreboard: live criterion run vs committed bench/baseline.json.
	@cargo run -q -p gmeow-pipeline --bin bench-compare

perf-gate: native-py ## Report-only timings for validate, generated drift, reason, and verify.
	mkdir -p $(PERF_DIR)
	$(GMEOW_DEV) validate --timings --timings-json $(PERF_DIR)/validate.json
	$(GMEOW_DEV) check-generated -j $(CHECK_GENERATED_JOBS) --timings-json $(PERF_DIR)/check-generated.json
	$(GMEOW_DEV) reason --mode native --timings-json $(PERF_DIR)/reason.json
	$(GMEOW_DEV) verify --mode native --timings-json $(PERF_DIR)/verify.json
	uv run python -c 'import json, pathlib; p=pathlib.Path("$(PERF_DIR)"); files=["validate.json","check-generated.json","reason.json","verify.json"]; out={"commands":[json.loads((p / f).read_text(encoding="utf-8")) for f in files]}; (p / "gate-timings.json").write_text(json.dumps(out, indent=2, sort_keys=True) + "\n", encoding="utf-8")'
	@echo "perf gate timings written to $(PERF_DIR)/gate-timings.json"

rust-coverage: ## Generate report-only Rust region coverage.
	cargo llvm-cov nextest --workspace --include-ffi --lcov --output-path lcov.info
	cargo llvm-cov report --html

mutants: ## Run report-only cargo-mutants over the configured scope.
	cargo mutants $(MUTANTS_ARGS)

compliance-report: ## Emit dist/compliance-report.ttl from already-passing gates.
	$(GMEOW_DEV) compliance-report --from-passing-check

##@ Maintainer Tasks

maint-classic-cross-check: maint-pull-images native-py ## Run the full non-required Docker/Java oracle lane.
	$(GMEOW_DEV) reason --mode docker --reasoner ELK --exclude-tautologies structural
	$(GMEOW_DEV) verify --mode docker --reasoner ELK --reasoned-input dist/gmeow-reasoned-elk.ttl
	$(GMEOW_DEV) reason --mode docker --reasoner hermit
	uv run python scripts/reasoning_cases.py
	uv run python scripts/statements_docker_check.py
	uv run python scripts/slme_cross_check.py
	$(GMEOW_DEV) crosscheck-queries
	$(GMEOW_DEV) classic-cross-check
	$(GMEOW_DEV) classic-cross-check-rl
	uv run pytest -n auto --dist loadscope -m "classic_cross_check" -q
	@echo "classic cross-check oracle lane passed"

maint-reason-hermit: maint-pull-images native-py ## Run HermiT complete consistency check.
	$(GMEOW_DEV) reason --mode docker --reasoner hermit

maint-explain: maint-pull-images native-py ## Explain unsatisfiable classes with HermiT.
	$(GMEOW_DEV) explain

maint-697-oracle-gold: maint-pull-images ## (Re)freeze #697 curated-DL oracle gold via HermiT/ELK (Docker).
	uv run --package gmeow-dev python scripts/gen_dl_oracle_gold.py

maint-verify-docker: maint-pull-images native-py ## Run ROBOT/ELK reasoned-graph verification.
	$(GMEOW_DEV) reason --mode docker --reasoner ELK --exclude-tautologies structural
	$(GMEOW_DEV) verify --mode docker --reasoner ELK --reasoned-input dist/gmeow-reasoned-elk.ttl

maint-reasoning-cases: maint-pull-images ## Run Docker-backed reasoning fixture cases.
	uv run python scripts/reasoning_cases.py

maint-statements-docker-check: maint-pull-images native-py ## Run Jena/ROBOT statement artifact oracle checks.
	uv run python scripts/statements_docker_check.py

maint-crosscheck: native-py ## Cross-check rdflib and native gmeow_rdf query answers.
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
	uv run pytest -n auto --dist loadscope -m "maintainer and not classic_cross_check"

maint-test-network: ## Run live network tests.
	GMEOW_RUN_NETWORK=1 uv run pytest -m network

maint-test-network-rust: ## Run the live SERVICE federation test against a public endpoint.
	GMEOW_RUN_NETWORK=1 cargo test -p gmeow-sparql-eval --test service_live -- --ignored

maint-pull-images: ## Pull or build pinned Docker oracle images.
	bash scripts/pull-images.sh

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

maint-capture-sparql-goldens: ## (maintainer) Freeze the native SPARQL engine as committed goldens (EPIC #906 Task 2/8; native-vs-native regression gate).
	cargo run -q -p gmeow-rdf --features gts --bin capture_sparql_goldens
	@echo "wrote goldens under crates/sparql-conformance/tests/goldens/ — verify byte-stable (re-run = no diff), then commit"

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
	for pkg in gmeow_diagnostics gmeow_docs gmeow_logic gmeow_rdf gmeow_shacl gmeow_slice gmeow_validate; do \
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
	cargo nextest run --no-run $(NEXTEST_PARTITION_ARG)
	@touch $@
