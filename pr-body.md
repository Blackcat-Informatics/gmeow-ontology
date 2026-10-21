Add a manual hosted workflow for the existing reviewed-translation golden generator. It checks out the dispatch source revision explicitly, runs the canonical Make target, and uploads the produced JSON under that source revision for review. Read-only permissions and disabled checkout credentials keep it from committing the result; a failed refresh prevents publication of the old tracked golden.

`make lint` passed, including YAML, Actions and security checks; independent source review is clear. No Rust source changed and no local catalog refresh or corpus run was performed. Hosted output still needs to run against the settled canonical sources and be reviewed before its normal commit. Documentation content, measured coverage and floor requirements remain open.

Refs #1656, #1894 and #1717. Principles 4 and 7.
