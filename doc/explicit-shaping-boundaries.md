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

Line selection retains the feature policy for character spacing.
A new width or `BreakLines::revert` can change the selected glyphs.
Each line retains the glyphs used for its advance.

`safe_concat_boundaries` limits the source ranges that require new glyphs.
`PhysicalShaper` stores these ranges for reuse. Other ranges retain their glyphs.

`calculate_content_widths` uses the same glyph selection for minimum and maximum
widths. The source layout does not change. Emergency breaks from `break-word`
do not change the minimum width. Breaks from `anywhere` can decrease it.

The consumer integration requires more work.
