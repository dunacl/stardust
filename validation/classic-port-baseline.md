# Classic port baseline

Summary statistics of the StarGen port (`sg-classic`, model 0.9.0), to compare
with the rewrite of ADR 0006. 1000 systems per preset, seeds 1–1000, from
`sg-cli batch --fidelity classic`. Only these statistics are kept. The rewrite
is not expected to reproduce them, since it follows Dole's published model and
not StarGen's tuning; a large difference here needs an explanation, not a fix.

Masses are in Earth masses. The range is the 5th to 95th percentile. Giants are
planets of at least 50% hydrogen and helium.

| Preset | Planets, mean | Range | Total mass, mean | Median | Largest, mean | Median | Giants, mean | Median `a`, AU | Median `e` | Spacing ratio, mean |
|---|---|---|---|---|---|---|---|---|---|---|
| sun-like | 11.86 | 9–15 | 906 | 865 | 667 | 602 | 3.86 | 3.00 | 0.054 | 1.60 |
| red-dwarf | 14.18 | 11–17 | 10 | 10 | 3 | 3 | 0.00 | 2.26 | 0.056 | 1.47 |
| massive-a-star | 10.07 | 8–13 | 4518 | 4559 | 3109 | 3165 | 5.18 | 3.54 | 0.053 | 1.76 |
| circumbinary | 10.38 | 8–13 | 289 | 265 | 217 | 180 | 2.48 | 5.41 | 0.055 | 1.57 |
| s-type-binary | 11.78 | 9–14 | 23 | 20 | 14 | 12 | 0.59 | 0.84 | 0.054 | 1.48 |
| hierarchical-triple | 15.39 | 12–18 | 1728 | 1721 | 1385 | 1332 | 3.02 | 1.59 | 0.054 | 1.56 |
