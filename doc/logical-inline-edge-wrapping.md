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

An inline-start edge after a Unicode collapsible space must snapshot the break
before that space's advance. The space stays in the source range, but it does
not become visible trailing geometry because the edge projects the same source
boundary. Caller-resolved retained-space boundaries stay at their exact byte;
their visible space has separate ownership.

An inline-end edge can project a Unicode boundary across its immediately
following collapsible space. The consumer supplies a closed following-space
authority. A selected `CollapsedAfterProjectedBreak` edge keeps the traversed
space in the next line's source range with zero advance. A selected
`RetainedAdvance` edge keeps its shaped advance. This distinguishes owners
whose inline geometry retains the following source space from owners whose
block-axis-only geometry must not create visible leading space.

The selected participation remains line-local through final glyph
positioning. A collapsed source space cannot regain its natural advance when
positioned runs are materialised, while a later relayout still starts from the
untouched shaped cluster.

CSS white-space processing can remove the run that owned an opportunity before
Parley receives the collapsed text. Caller-resolved source opportunities must
therefore remain distinct from Unicode-derived opportunities, and retain one
of two closed projection authorities:

```text
resolved source opportunity
├── collapsed-space  may project from an owner edge across the removed space
└── retained-space   remains at its exact source byte boundary
```

Both have already applied wrapping styles; a Unicode-derived opportunity still
requires Parley to resolve the adjoining styles. A logical inline-end edge can
project a collapsed-space boundary only across immediately following
collapsible space. It must not project a retained-space boundary before its
visible space or search past visible content.

The consumer must retain that source decision when collapse removes its owning
run. Logical edges and the surviving text cannot reconstruct it. For example,
these two source sequences have the same shaped text, styles, and edge topology:

```text
normal parent space:  <nowrap>X </nowrap> <nowrap>X</nowrap>
nowrap-owned space:   <nowrap>X </nowrap><nowrap>X</nowrap>
shaped input:         X X
```

Only the first sequence has the parent-owned opportunity. The consumer must use
`LineBreakOverride::resolved_collapsed_source_opportunity` for it. A retained
wrapping space uses `resolved_retained_source_opportunity` instead. Treating an
owner edge as the missing decision would also make the second sequence wrap.

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
- [x] Border and padding start geometry retain the atomic reference topology.
- [x] Margin and padding end geometry retain the following source boundary.
- [x] Caller-resolved collapsed-space opportunities survive `nowrap` owner
      boundaries.
- [x] A retained opportunity after an exact-fit non-wrapping participant keeps
      that participant on the preceding line.
- [x] A retained source space breaks at its exact byte and starts the following
      line at zero rather than projecting before the visible space.
- [x] First-line and mixed white-space style topology retain source boundaries.
- [x] Word-internal owner edges remain unavailable in all source modes.
- [x] Border and padding integration topology retains the following collapsed
      space on the next line.
- [x] A consumer-resolved parent space wraps between `nowrap` inline owners.
- [x] The same shaped topology without that source decision stays unbreakable.
- [x] A Unicode space projected at inline start retains the text range and
      removes its trailing advance from the line snapshot.
- [x] A collapsing inline-end projection keeps its traversed Unicode space in
      the next source range with zero advance.
- [x] Final glyph positioning retains the selected zero-advance projection
      without mutating the shaped cluster used by relayout.
- [x] Border and padding consumer geometry retain the projected source-space
      advance.
- [x] The same projected space keeps its normal advance when the break is not
      taken.
- [x] Parley formatting, tests, changed-code Clippy audit, duplication, and
      diff checks pass.
