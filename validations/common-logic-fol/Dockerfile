# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# Minimal image providing the E theorem prover (`eprover`), for the
# common-logic-fol validator-zoo lane. Nothing GPL is vendored into this repo:
# the `eprover` binary lives only inside the built image, fetched from Debian's
# own package archive at build time.

FROM debian:stable-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends eprover \
    && rm -rf /var/lib/apt/lists/*

ENTRYPOINT ["eprover"]
