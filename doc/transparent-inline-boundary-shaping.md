# Shaping across transparent inline boundaries

Text shaping and inline layout use different boundaries. An inline box can
retain source ownership and line-placement identity without inserting an
inline-axis advance between adjacent characters.

CSS Text section 7.3 requires shaping to continue across an inline boundary
when the boundary has no effective formatting change. Parley classifies each
inline box before shaping:

- an atomic box or a logical edge with a non-zero inline advance intervenes;
- a zero-width logical edge and a transparent anchor are transparent.

An intervening box ends the current shaping item. A transparent box retains its
exact source position in the layout item stream but does not end the shaping
item. The shaped clusters on both sides therefore keep one font context,
including kerning, ligatures, and contextual substitutions.

The classification is independent of line breaking. Relayout can change the
line that owns a transparent box, but it cannot change the shaped glyph data.
