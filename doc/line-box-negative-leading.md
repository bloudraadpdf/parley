# Negative leading in line boxes

## Problem

CSS line height can be smaller than the font ascent and descent. Each run then
has negative half-leading. A line can therefore have a negative extent below
its baseline.

Initialising either per-line extent to zero discards that valid negative
value. A uniform `30px/10px` Ahem line then becomes 14px high instead of 10px.

## Model

The first metric contributor mints both line extents. Later contributors take
the maximum on each side of the shared baseline.

```text
first run:  above = ascent  + half-leading
            below = descent + half-leading

next run:   above = max(above, next above)
            below = max(below, next below)
```

Zero is not a metric contributor.

## Verification

- [x] A uniform negative-leading run keeps its authored line height.
- [x] Mixed runs keep the maximum extent on each baseline side.
- [x] Empty lines retain their existing zero-based fallback.
- [x] Quantised line metrics retain their existing rounding policy.
