# Formatting-control line metrics

Unicode default-ignorable formatting controls participate in bidi resolution,
but they do not create a typographic character unit. A run containing only
such controls therefore cannot enlarge the line box through its font metrics.

Line breaking classifies each text item with a closed metric participation:

```text
visible or spacing content        -> contributes run metrics
default-ignorable controls only   -> preserves topology without metrics
```

A mixed run remains a metric contributor, as does a visible glyph selected
from a fallback font. If a line has no visible metric contributor, the
existing empty-line and whitespace fallback remains authoritative. This keeps
formatting controls available to bidi ordering without allowing the font used
to encode them to change the line height.
