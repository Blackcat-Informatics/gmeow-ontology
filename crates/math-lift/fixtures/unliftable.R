# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: CC-BY-4.0
#
# A real, syntactically valid R script with NO statistical content whatsoever: string
# handling, iteration, and I/O. It exists to prove the hard-fail gate.
#
# Every statement here routes to `logic:` as general computation, so the run's `math:`
# statistical codomain is EMPTY — and an ingest run that structures nothing is a
# `math:UnliftableIngest`, not a lift. The bridge therefore refuses with a typed
# `math.lift.r.unliftable` diagnostic rather than emitting a degraded graph of
# string-valued placeholders.

report_labels <- c("alpha", "beta", "gamma", "delta")
prefix <- "row: "
rendered <- character(0)

banner <- function(text, width) {
  padding <- strrep("-", width)
  paste0(padding, " ", text, " ", padding)
}

for (label in report_labels) {
  shouted <- toupper(label)
  if (nchar(shouted) > 4) {
    rendered <- c(rendered, paste0(prefix, shouted))
  } else {
    rendered <- c(rendered, paste0(prefix, "short"))
  }
}

remaining <- length(rendered)
while (remaining > 0) {
  writeLines(rendered[remaining])
  remaining <- remaining - 1L
}

writeLines(banner("done", 8))
