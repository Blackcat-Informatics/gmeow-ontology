The borrowed equality API is merged and its acceptance checks have passed.

The public API agrees with the actual query operator on the representative native values. The [hosted native test](https://github.com/Blackcat-Informatics/purrdf/actions/runs/39059858222/job/117475400484) and the [hosted WASM execution](https://github.com/Blackcat-Informatics/purrdf/actions/runs/39059858222/job/117475400367) both passed that comparison test. Local checks also verified that an allocation refusal preserves its original owner and releases charges. Strict affected Clippy and normal commit hooks passed before integration through ghprsq.

The separate [imported-closure proof replay failure](https://github.com/Blackcat-Informatics/purrdf/issues/591) remains open. Its exact failure recurred in the [integration job](https://github.com/Blackcat-Informatics/purrdf/actions/runs/39059858222/job/117475400405); this is not a claim that the whole workflow passed.

The downstream integrity implementation remains in progress. CodeRabbit has not submitted a completed review of this API change; any later finding remains actionable.
