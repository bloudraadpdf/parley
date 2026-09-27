# Shaped discretionary replacements

A conditional hyphen can use a fallback font. Its advance, glyphs, and line
metrics must come from that font. Font fallback must not change the metrics
of a line that does not take the conditional break.

`DiscretionaryBreakShape` retains one unwrapped layout and its text. The
constructor rejects multiple lines and inline boxes. Register shapes with
`Layout::set_discretionary_break_shapes`; read the same shape at emission.
The shape supplies the registered discretionary advance. Its run metrics
contribute to the line box only when that boundary is selected.

The regression uses Roboto text and a Noto Kufi Arabic hyphen at 12 units.
Before the change the selected line ascent was 11.1328125, whereas the
explicit fallback reference had ascent 15.384. The selected-break control
and the unbroken-line control cover CSS Text discretionary hyphens and
CSS inline line-box metric contributions.
