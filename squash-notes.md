test(docs): regenerate the reviewed-coverage golden from the current catalogs (Principles 4, 7)

The frozen reviewed-coverage golden was out of date against the translation catalogs, so the equality test could not pass. Regenerate it with its own maintenance target, which reads only the slice and DSL catalogs, the slice manifests and the two DSL vocabulary files. It now covers 190 catalogs and 64441 reviewed keys.
