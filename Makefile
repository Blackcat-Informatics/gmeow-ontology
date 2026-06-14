# GMEOW ontology toolchain — canonical task runner.
# Every target shells into the `gmeow` CLI (src/gmeow_tools) or the dev tools;
# no logic lives in this file. Run `make help` for the target list.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Alignment target for `make extract` (license-checked). Override: make extract TARGET=foaf
TARGET ?= foaf

# Override: make commit MESSAGE="feat: add foaf alignment"
MESSAGE ?= "chore: regenerate checked-in artifacts"

.PHONY: help install fmt lint validate crosscheck reason reason-hermit explain verify reasoning-cases statements-docker-check extract \
        mappings wikidata wikidata-live wikidata-coverage wikidata-audit \
        lint-alignment refresh-target-axioms docs docs-full ontology-docs ontology-docs-full quality \
        normalize build project test test-fast test-docker check check-docker check-generated release regenerate commit clean clean-docs pull-images \
        coverage crossref constitution-check compliance-report audit evals-score

help: ## Show this help.
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

install: ## Sync the uv environment (runtime + dev deps).
	uv sync

fmt: ## Auto-format with ruff.
	uv run ruff format .

lint: ## Lint (ruff), type-check (mypy), and full repo-hygiene suite (pre-commit).
	uv run ruff check .
	uv run ruff format --check .
	uv run mypy
	# Symmetry with CI: CI's `lint` job runs the whole pre-commit suite
	# (markdownlint, end-of-file-fixer, codespell, yamllint, shellcheck, …),
	# so the local gate must too — otherwise those lanes only fail in CI.
	uv run pre-commit run --all-files --show-diff-on-failure

validate: ## Validate syntax, term annotations, and SHACL (pure Python).
	uv run gmeow validate

crosscheck: ## Prove rdflib and pyoxigraph answer every committed query alike (no Docker).
	uv run gmeow crosscheck-queries

reason: ## Merge, validate OWL 2 DL profile, and check ELK consistency (Docker).
	uv run gmeow reason --reasoner ELK --exclude-tautologies structural

reason-hermit: ## Sound + complete consistency check with HermiT (Docker).
	uv run gmeow reason --reasoner hermit

explain: ## Explain any unsatisfiable classes (HermiT, Docker).
	uv run gmeow explain

verify: reason ## Reasoned-graph negative tests (ROBOT verify over queries/verify/, Docker).
	uv run gmeow verify --reasoner ELK --reasoned-input dist/gmeow-reasoned-elk.ttl

reasoning-cases: ## HermiT/ELK inconsistency and fixture-coherence cases (Docker).
	uv run python scripts/reasoning_cases.py

statements-docker-check: ## Jena/ROBOT-backed statement artifact and reasoning checks (Docker).
	uv run python scripts/statements_docker_check.py

extract: ## Report import/extract policy for TARGET (refuses reference-only).
	uv run gmeow extract --target $(TARGET)


mappings-only: ## Build alignment axioms + VoID linksets (assumes SSSOM files present).
	uv run gmeow mappings

mappings: ## Build alignment axioms + VoID linksets from SSSOM; validate QID syntax.
	uv run gmeow mappings

lint-alignment: ## Lint SSSOM mappings for inverse / domain-range-mismatched targets (offline).
	uv run gmeow lint-alignment

refresh-target-axioms: ## Re-vendor minimal target-axiom snapshots (IMPORT_OK targets only).
	uv run gmeow refresh-target-axioms --target all

wikidata: ## Validate Wikidata QID/PID syntax in the mappings (offline).
	uv run gmeow wikidata

wikidata-live: ## Also verify Wikidata ids resolve (network).
	uv run gmeow wikidata --existence

wikidata-coverage: ## Report Wikidata mapping coverage by domain (offline).
	uv run gmeow wikidata-coverage

wikidata-audit: ## Audit fixtures and modules for Wikidata misuse (offline).
	uv run gmeow wikidata --fixtures

coverage: ## Report how much of the vendored entity slice GMEOW covers.
	uv run gmeow coverage --gaps

crossref: ## Generate the CrossRef DOI deposit XML.
	uv run gmeow crossref

docs: ontology-docs ## Alias for ontology-docs.

ontology-docs: ## Generate the unified ontology-docs site.
	uv run gmeow regenerate ontology-docs

docs-full: ontology-docs-full ## Alias for ontology-docs-full.

ontology-docs-full: ## Generate ontology-docs including optional Docker stages.
	uv run python -c "from gmeow_tools.ontology_docs import build_ontology_docs; from pathlib import Path; build_ontology_docs(Path('ontology-docs'))"

quality: ## Run OOPS! pitfall scan (network, best-effort).
	uv run gmeow quality

normalize: ## Canonicalize the authored ontology sources (rewrites files).
	uv run gmeow normalize

check-generated: ## Drift + orphan check for all registered generators.
	uv run gmeow check-generated -j $$(nproc 2>/dev/null || echo 4)

constitution-check: ## Every constitutional principle must have live enforcement (#280).
	uv run gmeow constitution-check

compliance-report: ## Run in-process gates, emit dist/compliance-report.ttl (#285).
	uv run gmeow compliance-report

audit: ## Claim audit gates over the worked fixture (#55): ungrounded/contradicted/stale.
	uv run gmeow audit tests/fixtures/coverage/hallucination-kg.ttl

evals-score: ## Score committed model emissions against the published contract (offline, #298).
	uv run gmeow evals score

build: ## Build serializations and JSON-LD context into dist/.
	uv run gmeow build

project: ## Project GMEOW data to pure schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profiles (FnO/EDOAL).
	uv run gmeow project

test: ## Run the full test suite (incl. heavy ci_only export tests).
	uv run pytest -n auto

test-fast: ## Run the fast test suite (excludes ci_only, docker, and CI-only pyoxigraph).
	uv run pytest -n auto -m "not ci_only and not docker and not pyoxigraph_ci"

test-docker: check-docker ## Compatibility alias for the Docker-backed Make lanes.

test-network: ## Run the network tests (LIVE endpoints) — MANUAL only, never in CI/check.
	GMEOW_RUN_NETWORK=1 uv run pytest -m network

check: ## Fast local gate: core ontology + transforms (ELK only; HermiT runs in its own CI job).
	$(MAKE) -j$$(nproc 2>/dev/null || echo 4) lint validate crosscheck check-generated constitution-check audit wikidata coverage reason verify mappings-only lint-alignment
	$(MAKE) test-fast
	$(MAKE) compliance-report
	@echo "✓ all checks passed"

check-docker: ## Optional local Docker gate: HermiT, reasoning cases, and Jena statements.
	$(MAKE) reason
	uv run gmeow verify --reasoner ELK --reasoned-input dist/gmeow-reasoned-elk.ttl
	$(MAKE) reason-hermit
	$(MAKE) reasoning-cases
	$(MAKE) statements-docker-check
	@echo "✓ all Docker checks passed"

release: ## RDF 1.2 + OWL downcast → reasoned closure (HermiT) + build + regenerate + CrossRef deposit.
	uv run gmeow regenerate
	uv run gmeow reason --reasoner hermit --full
	uv run gmeow build
	uv run gmeow compliance-report
	uv run gmeow crossref

regenerate: ## Rebuild all checked-in generated artifacts from canonical sources.
	uv run gmeow regenerate -j $$(nproc 2>/dev/null || echo 4)

commit: regenerate ## Regenerate artifacts, stage them, and commit.
	@REGENERATED_PATHS=$$(uv run python -c "from gmeow_tools.load_generators import load_all; load_all(); from gmeow_tools.generator import all_regenerated_paths; print(' '.join(all_regenerated_paths()))"); \
	git add $${REGENERATED_PATHS}; \
	if git diff --cached --quiet; then \
		echo "Nothing to commit."; exit 1; \
	else \
		git commit -m "$(MESSAGE)"; \
	fi
	@git diff --quiet || echo "Warning: unstaged changes remain. Stage them with 'git add' and commit separately if needed."

pull-images: ## Pre-pull the pinned Docker images (ROBOT, Jena).
	bash scripts/pull-images.sh

clean: ## Remove ephemeral build artifacts (preserves committed ontology-docs/).
	rm -rf dist docs/_generated .stamps
	@echo "✓ cleaned"

clean-docs: ## Remove the committed ontology-docs/ tree (regenerate with make ontology-docs).
	rm -rf ontology-docs
	@echo "✓ cleaned ontology-docs/"
