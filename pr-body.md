Mapping feedback reported a warning when its required native correspondence soundness pass failed. The feedback collector accepts warning-only reports, so that path could still report success. Emit an Error finding with an explicit failure code and the native error text; the existing collector then fails the selected operation while retaining the diagnostics. Correct the stale Python-interface description. Principles 4, 7 and 17.

Independent review traced the native pass, Report::ok and the developer feedback collector. Prose, whitespace and normal signed commit hooks passed; warnings-denied diagnostics for all developer CLI targets passed. No new test seam or corpus production is introduced for this diagnostic classification change.

Part of #1712. Its shipped correspondence graph and full corpus finding acceptance remain open. CI remains waived under the standing campaign instruction.
