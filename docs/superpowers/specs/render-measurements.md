# Render Backend Measurements

Measurements collected with `scripts/measure.ps1` against the release build (`target/release/bar.exe`) on Windows.

## Direct2D

- **Duration:** 10s (10 samples @ 1000ms)
- **Working Set (steady / avg / max):** 62.88 MB / 67.60 MB / 74.68 MB
- **Private Bytes (steady / avg / max):** 47.59 MB / 52.35 MB / 59.48 MB
- **Startup CPU time:** 156.25 ms
- **Idle CPU delta over 10s:** 0 ms

| Sample | Working Set (MB) | Private Bytes (MB) | Total CPU (ms) |
|---:|---:|---:|---:|
| 1 | 74.68 | 59.48 | 156.25 |
| 2 | 74.68 | 59.48 | 156.25 |
| 3 | 74.68 | 59.48 | 156.25 |
| 4 | 74.68 | 59.48 | 156.25 |
| 5 | 62.88 | 47.59 | 156.25 |
| 6 | 62.88 | 47.59 | 156.25 |
| 7 | 62.88 | 47.59 | 156.25 |
| 8 | 62.88 | 47.59 | 156.25 |
| 9 | 62.88 | 47.59 | 156.25 |
| 10 | 62.88 | 47.59 | 156.25 |
