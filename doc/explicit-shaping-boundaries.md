# Glyph context boundaries

`RangedBuilder::push_shaping_boundary` ends glyph context at a
source byte offset. The offset must be a UTF-8 boundary in the input text.
The order of the offsets is not important. You can repeat offsets.

The boundary does not insert text, add a layout item, or permit a line break.
It splits optional ligatures and limits the glyph context.
Subsequent glyph selection keeps the same context limits.

The layout context clears these offsets before each builder. The next layout
does not inherit the previous layout's boundaries.

`InlineBox::with_continuous_shaping` preserves glyph context at the box's
source position while retaining its advance and line-break rules.
Use it when visual box edges split text at a different source
boundary. Supply that boundary with `push_shaping_boundary`.

Continuous context is different from a transparent zero-width anchor.
Line breaks selected with widths or character counts keep the box advances.

## Owner inputs

`RangedBuilder::push_inline_owner_shaping` keeps an owner's text range,
inline-box identities and visual edges. Each edge selects the first line,
last line or each visual fragment. Include an edge when a margin, border or
padding component is not zero. The total advance can be zero.

`Line::physical_shaping_boundaries` selects source positions from the visual
owner fragments. First and last line membership uses source item identities.
Inline children at the same text offset keep different identities. This query does not
change glyphs or line geometry.

The builder keeps the selected font, features, source characters and context
limits for subsequent glyph selection. Optional-ligature expansion uses
the same source record with its own feature policy. No policy changes
the initial context limits.

`BreakLines` selects the visual boundaries before it checks the line advance.
Each candidate keeps its source endpoint and glyph shapes together.
Soft line boundaries split the shaped range but retain glyph context.
Box edges and `push_shaping_boundary` end that context.

The first candidate is the terminal line: the source up to the next forced
break. Its glyphs give a natural break at the line width. The candidates end
at the first regular break after that natural break, which is the end of the
first word that overflows. Longer lines are not candidates. To find that
break, `BreakLines` probes each source position after the natural break. A
probe can end a line only at its position, so it stops when it passes that
position.

Line selection retains the feature policy for character spacing.
A new width or `BreakLines::revert` can change the selected glyphs.
Each line retains the glyphs used for its advance.

A candidate gets new glyphs from its line start to the next
`safe_concat_boundaries` position or owner edge after its last boundary. Other
ranges retain their glyphs. No candidate reads the source before its line
start, and each line keeps the glyphs of its commit. Soft line boundaries and
box edges split a range into segments. Each segment gets its glyphs alone,
with the glyph context of its context limits. `PhysicalShaper` stores each
segment for reuse. A committed line removes the stored segments that start or
end at a soft line boundary. The other segments stay for later lines.

A candidate installs its segments on the glyphs of the previous lines and
removes them after its measurement. The candidate keeps its segments, and its
commit installs them again. The terminal candidate gets new glyphs only up to
the first boundary after twice its natural line. It measures again with a
wider limit when its first overflowing word ends after that boundary. The limit
grows from the source position where the line starts, because a line of inline
boxes only has no text range.

A shorter terminal line can give the same glyphs, natural break and bound.
`BreakLines` first uses the natural line at four times the line width as the
terminal line, and doubles that width until the result is the same as that of
the terminal line to the forced break. The result is the same when all these
conditions are true:

- All runs have one bidi level, which is their paragraph level. The items
  before the end of the shorter line then keep their visual order.
- Each owner across that end has text to that end and no last-line edge on
  the start side. Such an owner then changes only its fragment at that end.
- The terminal candidate gets new glyphs up to a boundary before that end.
- The natural break is before that end, and a regular break follows it at or
  before that end.

When a condition is false, `BreakLines` uses the next width. The last terminal
line goes to the forced break.

`calculate_content_widths` uses the same glyph selection for minimum and maximum
widths. The source layout does not change. Emergency breaks from `break-word`
do not change the minimum width. Breaks from `anywhere` can decrease it.

The consumer integration requires more work.
