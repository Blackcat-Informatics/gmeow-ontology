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
CARGO_TARGET_DIR ?= target
SIGN_KEY ?=
PUBLIC_KEY ?= keys/gmeow-release-key.asc
GTS_OUT ?= dist/gmeow.gts

# Optional cargo-nextest partition for sharded CI runs (e.g., count:1/2)
NEXTEST_PARTITION ?=
NEXTEST_PARTITION_ARG := $(if $(NEXTEST_PARTITION),--partition $(NEXTEST_PARTITION) --no-tests pass,)

# The committed .cargo/config.toml pins Rust to the portable x86-64-v3 floor.
# Report-only benchmarks opt into host tuning and must restate the bundled-lld
# linker flags because the RUSTFLAGS env var replaces config rustflags.
NATIVE_RUSTFLAGS := -Zunstable-options -Clink-self-contained=+linker -Clinker-features=+lld -Ctarget-cpu=native

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
	lint-alignment doc-lint

.PHONY: help \
	install fmt lint \
	native-py validate validate-gts reason verify test test-fast rust-build rust-test check \
	regenerate check-generated commit docs normalize build project release release-sign-gts clean \
	mappings wikidata coverage acceptance crossref audit \
	constitution-check crate-check lint-alignment doc-lint rust-gate clippy rdf-core-hygiene \
	slicetest conformance insta-review \
	fuzz-smoke bench bench-compare rust-coverage mutants compliance-report \
	maint-classic-cross-check maint-reason-hermit maint-explain maint-697-oracle-gold maint-verify-docker \
	maint-reasoning-cases maint-statements-docker-check maint-crosscheck \
	maint-extract maint-refresh-target-axioms maint-wikidata-live \
	maint-wikidata-coverage maint-wikidata-audit maint-test-heavy \
	maint-test-network maint-pull-images maint-quality maint-evals-score \
	maint-compliance-report-full maint-bench-baseline

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
	cargo nextest run $(NEXTEST_PARTITION_ARG)
	cargo test --doc

check: native-py ## Run the full Docker-free local quality gate.
	$(MAKE) -j$(NPROC) $(CHECK_TARGETS)
	$(MAKE) test-fast
	$(MAKE) compliance-report
	@echo "all checks passed (Docker-free, Java-free)"

##@ Generated Artifacts And Outputs

regenerate: native-py ## Rebuild all checked-in generated artifacts from canonical sources.
	$(GMEOW_DEV) regenerate -j $(NPROC)

check-generated: native-py ## Drift + orphan check for all registered generators.
	$(GMEOW_DEV) check-generated -j $(NPROC)

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
	$(MAKE) maint-compliance-report-full
	$(MAKE) crossref

release-sign-gts: native-py ## Sign the regenerated GTS bundle for release packaging.
	@if [ -z "$(SIGN_KEY)" ]; then \
		echo "SIGN_KEY=/path/to/secret.asc is required"; exit 1; \
	fi
	$(GMEOW_DEV) compile-gts --sign-key "$(SIGN_KEY)" --public-key "$(PUBLIC_KEY)" --out "$(GTS_OUT)"

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

rust-gate: rust-build ## Warm Rust once, then run clippy, nextest, and doctests serially.
	cargo clippy --all-targets -- -D warnings
	cargo nextest run $(NEXTEST_PARTITION_ARG)
	cargo test --doc

clippy: rust-build ## Run cargo clippy on all Rust targets with warnings as errors.
	cargo clippy --all-targets -- -D warnings

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

slicetest: ## Run the slice-resident test-DSL harness in isolation.
	cargo nextest run -p gmeow-slicetest $(NEXTEST_PARTITION_ARG)
	cargo test --doc -p gmeow-slicetest

conformance: ## Run the native logic conformance harness in isolation.
	cargo nextest run -p gmeow-conformance $(NEXTEST_PARTITION_ARG)

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
	RUSTFLAGS="$(NATIVE_RUSTFLAGS)" cargo bench -p gmeow-logic -p gmeow-rdf -p gmeow-shacl -p gmeow-validate

bench-compare: ## Report-only perf scoreboard: live criterion run vs committed bench/baseline.json.
	@cargo run -q -p gmeow-pipeline --bin bench-compare

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

native-py: $(NATIVE_PY_STAMP)

$(NATIVE_PY_STAMP): $(NATIVE_PY_INPUTS)
	VIRTUAL_ENV="$(CURDIR)/.venv" uvx maturin develop --manifest-path crates/native/Cargo.toml
	@touch $@

$(RUST_READY_STAMP): $(RUST_INPUTS)
	@mkdir -p $(dir $@)
	cargo nextest run --no-run $(NEXTEST_PARTITION_ARG)
	@touch $@
