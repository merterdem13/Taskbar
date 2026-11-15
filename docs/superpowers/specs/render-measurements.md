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

## tiny-skia

- **Duration:** 10s (10 samples @ 1000ms)
- **Working Set (steady / avg / max):** 10.81 MB / 10.81 MB / 10.81 MB
- **Private Bytes (steady / avg / max):** 3.55 MB / 3.55 MB / 3.55 MB
- **Startup CPU time:** 125.00 ms
- **Idle CPU delta over 10s:** 0 ms

| Sample | Working Set (MB) | Private Bytes (MB) | Total CPU (ms) |
|---:|---:|---:|---:|
| 1 | 10.81 | 3.55 | 125.00 |
| 2 | 10.81 | 3.55 | 125.00 |
| 3 | 10.81 | 3.55 | 125.00 |
| 4 | 10.81 | 3.55 | 125.00 |
| 5 | 10.81 | 3.55 | 125.00 |
| 6 | 10.81 | 3.55 | 125.00 |
| 7 | 10.81 | 3.55 | 125.00 |
| 8 | 10.81 | 3.55 | 125.00 |
| 9 | 10.81 | 3.55 | 125.00 |
| 10 | 10.81 | 3.55 | 125.00 |

## Comparison & Decision

| Backend | Working Set (steady) | Private Bytes (steady) | Startup CPU (ms) | Idle CPU / 10s (ms) | Meets Visual Baseline | Meets 10–20 MB Target |
|---|---:|---:|---:|---:|---|---|
| **Direct2D (`render-d2d`)** | 62.88 MB | 47.59 MB | 156.25 ms | 0 ms | Yes | No (> 30 MB) |
| **tiny-skia (`render-skia`)** | **10.81 MB** | **3.55 MB** | **125.00 ms** | **0 ms** | **Yes** | **Yes** |

**Decision:** Select **`render-skia` (`tiny-skia` + `fontdue` glyph cache + 32-bit premultiplied BGRA DIB present over DWM Acrylic/Mica)** as the default render backend (`default = ["render-skia"]`), reducing steady-state Working Set by **82.8%** (62.88 MB → 10.81 MB) and Private Bytes by **92.5%** (47.59 MB → 3.55 MB) while preserving the visual baseline and zero idle CPU.
