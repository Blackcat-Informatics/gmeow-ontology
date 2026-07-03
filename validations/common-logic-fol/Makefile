# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# Standalone Common Logic / first-order validator-zoo lane (NOT wired into the
# repo's make check / maint-*). Builds an image carrying the E theorem prover,
# translates the gmeow CLIF export to TPTP-FOF via the lane-local `clif2tptp`,
# and cross-checks native's verdicts against E. See README.md.

DOCKER ?= docker
IMAGE := cl-fol-eprover

.PHONY: help validate build-image probe clean

help: ## Show the lane targets.
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
	  awk 'BEGIN{FS=":.*?## "}{printf "  %-12s %s\n", $$1, $$2}'

validate: build-image probe ## Full probe: build the eprover image, then run probe.sh. Prints PASS or BOUNDARY.

build-image: ## Build the eprover Docker image.
	$(DOCKER) build -t $(IMAGE) .

probe: ## Run the probe against the built image (or a PATH-local eprover). Writes result.txt.
	bash probe.sh

clean: ## Remove build artifacts and the probe result.
	-rm -rf clif2tptp/target result.txt
