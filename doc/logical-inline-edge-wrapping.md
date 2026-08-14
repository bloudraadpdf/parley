# Logical inline edge wrapping

CSS Text 3 section 5 states that an inline element boundary does not introduce
a forced line break or a soft-wrap opportunity. A logical inline edge can add
margin, border, or padding advance, but that geometry does not make the edge an
atomic inline.

Parley therefore models three closed line-break participation states:

```text
inline participant
├── atomic              wrap opportunities before and after
├── logical owner edge  geometry only; source text owns wrapping
└── transparent anchor  neither geometry nor wrapping
```

Logical inline-start and inline-end edges retain their directional attachment
for bidirectional ordering. Their line-break participation is independent:
neither edge can create a break before or after itself. A real character
boundary, such as U+0020 SPACE, remains available through the text analyser.
When both adjoining text styles use `text-wrap-mode: nowrap`, that character
boundary remains unavailable.

The analyser remains the sole authority for a source boundary. An owner edge
can project that existing boundary to the correct side of its geometry, but it
cannot derive an opportunity from its own presence. The projection uses the
resolved wrapping styles on both sides of the boundary; a closing `nowrap`
owner must not suppress an opportunity in following wrapping content.

## Validation

- [x] A same-index inline-end and inline-start pair inside `XXXXXXXXXX` does
      not split the word.
- [x] The pair does not reduce the word's min-content width.
- [x] `text-wrap-mode: nowrap` cannot retain a break snapshot from a logical
      inline edge.
- [x] A U+0020 boundary across logical inline edges still wraps and places the
      end and start edges on their owning lines.
- [x] Atomic inline boxes retain their wrap opportunities.
- [x] Transparent anchors remain absent from line breaking and sizing.
- [x] Directional edge and bidirectional topology controls pass.
- [x] A Unicode opportunity survives a `nowrap` owner-to-wrapping-content
      boundary.
- [x] A Unicode opportunity survives a wrapping-to-`nowrap` boundary.
- [x] An unbreakable boundary with the same owner topology stays unbreakable.
- [x] Non-zero logical edge geometry can take, but cannot create, a source
      opportunity.
- [x] A positioned logical owner has the same glyph offsets as the equivalent
      atomic geometry and whitespace topology.
- [x] Parley formatting, tests, changed-code Clippy audit, duplication, and
      diff checks pass.
