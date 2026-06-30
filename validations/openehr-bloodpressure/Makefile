# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# Standalone openEHR validator-zoo lane (NOT wired into the repo's make check / maint-*).
# Stands up a real EHRbase CDR via Docker, then validates the source + augmented
# compositions under the vendored Blutdruck.opt. See README.md.

COMPOSE ?= docker compose

.PHONY: validate up probe down clean help

help: ## Show the lane targets.
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
	  awk 'BEGIN{FS=":.*?## "}{printf "  %-10s %s\n", $$1, $$2}'

validate: up probe down ## Full probe: up → POST OPT + both compositions → down. Prints PASS or BOUNDARY.

up: ## Start EHRbase + Postgres and wait for health.
	$(COMPOSE) up -d
	@echo "EHRbase starting; the probe waits for health."

probe: ## Run the validation probe against a running EHRbase (writes result.txt).
	bash probe.sh

down: ## Stop and remove the EHRbase stack and volumes.
	-$(COMPOSE) down -v

clean: down ## Alias for down; also removes the probe result.
	-rm -f result.txt
