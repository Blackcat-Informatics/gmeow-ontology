# GMEOW ontology toolchain — canonical task runner.
# Every target shells into the `gmeow` CLI (src/gmeow_tools) or the dev tools;
# no logic lives in this file. Run `make help` for the target list.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Alignment target for `make extract` (license-checked). Override: make extract TARGET=foaf
TARGET ?= foaf

# Override: make commit MESSAGE="feat: add foaf alignment"
MESSAGE ?= "chore: regenerate checked-in artifacts"

.PHONY: help install fmt lint validate crosscheck reason reason-hermit explain verify extract \
        mappings wikidata wikidata-live wikidata-coverage wikidata-audit \
        lint-alignment refresh-target-axioms docs docs-full quality \
        normalize build project test test-fast check check-generated release regenerate commit clean pull-images \
        coverage crossref constitution-check

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
	uv run gmeow reason --reasoner ELK

reason-hermit: ## Sound + complete consistency check with HermiT (Docker).
	uv run gmeow reason --reasoner hermit

explain: ## Explain any unsatisfiable classes (HermiT, Docker).
	uv run gmeow explain

verify: ## Reasoned-graph negative tests (ROBOT verify over queries/verify/, Docker).
	uv run gmeow verify --reasoner ELK

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

docs: ## Generate pyLODE HTML documentation.
	uv run gmeow docs

docs-full: ## Generate pyLODE + WIDOCO documentation (Docker).
	uv run gmeow docs --widoco

quality: ## Run OOPS! pitfall scan (network, best-effort).
	uv run gmeow quality

normalize: ## Canonicalize the authored ontology sources (rewrites files).
	uv run gmeow normalize

check-generated: ## Drift + orphan check for all registered generators.
	uv run gmeow check-generated

constitution-check: ## Every constitutional principle must have live enforcement (#280).
	uv run gmeow constitution-check

build: ## Build serializations and JSON-LD context into dist/.
	uv run gmeow build

project: ## Project GMEOW data to pure schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profiles (FnO/EDOAL).
	uv run gmeow project

test: ## Run the full test suite (incl. heavy ci_only export tests).
	uv run pytest -n auto

test-fast: ## Run the fast test suite (excludes ci_only heavy/secondary export tests).
	uv run pytest -n auto -m "not ci_only"

check: ## Fast local gate: core ontology + transforms.
	$(MAKE) -j$$(nproc 2>/dev/null || echo 4) lint validate crosscheck check-generated constitution-check wikidata coverage reason reason-hermit verify mappings-only lint-alignment
	$(MAKE) test-fast
	@echo "✓ all checks passed"

release: ## RDF 1.2 + OWL downcast → reasoned closure (HermiT) + build + regenerate + CrossRef deposit.
	uv run gmeow regenerate
	uv run gmeow reason --reasoner hermit --full
	uv run gmeow build
	uv run gmeow crossref

regenerate: ## Rebuild all checked-in generated artifacts from canonical sources.
	uv run gmeow regenerate

commit: regenerate ## Regenerate artifacts, stage them, and commit.
	@REGENERATED_PATHS=$$(uv run python -c "from gmeow_tools.generator import all_regenerated_paths; print(' '.join(all_regenerated_paths()))"); \
	git add $${REGENERATED_PATHS}; \
	if git diff --cached --quiet; then \
		echo "Nothing to commit."; exit 1; \
	else \
		git commit -m "$(MESSAGE)"; \
	fi
	@git diff --quiet || echo "Warning: unstaged changes remain. Stage them with 'git add' and commit separately if needed."

pull-images: ## Pre-pull the pinned Docker images (ROBOT, WIDOCO, Jena).
	bash scripts/pull-images.sh

clean: ## Remove generated artifacts.
	rm -rf dist docs/_generated .stamps
	@echo "✓ cleaned"
