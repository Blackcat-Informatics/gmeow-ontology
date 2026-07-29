# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: CC-BY-4.0
#
# Fuel economy of the 1974 Motor Trend road tests: a small, ordinary OLS analysis of
# `mtcars`, written the way an R user actually writes one. It is the R bridge's flagship
# fixture, so every construct here is load-bearing for the lift map:
#
#   * a data frame held BY REFERENCE (`mtcars`, and a filtered `subset()` of it)
#   * a multi-term model formula with a transform and an interaction
#   * the broom tidy / glance / augment triple (coefficients, summary, residuals)
#   * a distribution call (the parametric bootstrap noise draw)
#   * arithmetic transforms
#   * control flow, which routes to `logic:` rather than into `math:`

library(stats)
set.seed(20260725)

# The data, by reference. `subset()` frames it; it never inlines a single cell.
cars <- subset(mtcars, cyl > 4)

# A derived predictor. Power-to-weight is an ordinary arithmetic transform.
ptw <- hp / wt

# The workhorse fit: displacement and weight, with weight interacting with the number of
# forward gears, plus a quadratic in horsepower.
fit <- lm(mpg ~ disp + wt * gear + I(hp^2), data = cars)

# The broom triple.
tidy_coefficients <- summary(fit)$coefficients
model_summary <- summary(fit)
augmented_residuals <- residuals(fit)

# A second, simpler model for comparison, written with the magrittr pipe.
simple_fit <- cars %>% lm(mpg ~ wt, data = .)
simple_coefficients <- coef(simple_fit)

# A parametric bootstrap draw around the fitted error scale.
bootstrap_noise <- rnorm(1000, mean = 0, sd = 2.5)

# Log-scale diagnostics. `log(wt)` deliberately appears twice: content-addressed interning
# must resolve both mentions to one node.
log_weight_index <- log(wt) * 100
log_weight_ratio <- log(wt) / 4

# General computation. The whole guard lowers into logic: as one proposition; it is not
# mathematics, and it is not silently dropped either.
if (nrow(cars) > 10) {
  message("enough observations for the interaction term")
} else {
  warning("the interaction term is underpowered")
}

for (predictor in c("disp", "wt", "gear")) {
  cat(predictor, "\n")
}
