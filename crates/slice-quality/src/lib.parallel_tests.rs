// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use rayon::prelude::*;

#[test]
fn indexed_parallel_collection_preserves_input_and_error_order() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .expect("four-worker pool");
    let input: Vec<usize> = (0..128).collect();
    for _ in 0..8 {
        let output: Vec<Result<usize, usize>> = pool.install(|| {
            input
                .par_iter()
                .map(|value| {
                    if value % 17 == 0 {
                        Err(*value)
                    } else {
                        Ok(value * value)
                    }
                })
                .collect()
        });
        let serial: Vec<Result<usize, usize>> = input
            .iter()
            .map(|value| {
                if value % 17 == 0 {
                    Err(*value)
                } else {
                    Ok(value * value)
                }
            })
            .collect();
        assert_eq!(output, serial);
    }
}
