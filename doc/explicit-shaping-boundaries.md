# Explicit shaping boundaries

`RangedBuilder::push_shaping_boundary` ends shaping and joining context at a
source byte offset. The offset must be a UTF-8 boundary in the input text.
Offsets can be repeated or supplied in any order.

The boundary does not insert text, add a layout item, or add a line-break
opportunity. It splits optional ligatures and limits the context supplied
to joining scripts. Deferred line shaping keeps the same context limits.

The layout context clears these offsets before each builder. A later layout
does not inherit the previous layout's boundaries.
