# Unicode bidi visual order

Unicode directional formatting controls participate in bidi resolution but do
not paint glyphs.  Rules X1 through X9 resolve their embedding effects before
the visual-order rules reorder the remaining text.  A formatting control can
therefore change the level of later text, but it cannot become a visible run
or split the resolved sequence at its source position.

Inline boxes are separate paragraph participants.  Their stable identities
and source affinities must survive bidi resolution, but inserting a zero-width
inline participant cannot change the visual order of the surrounding text.
The text-only paragraph and the paragraph with zero-width inline boundaries
must resolve the same visible sequence.

The closed output contract is the positioned visible sequence:

```text
source text + formatting controls + inline participants
                         |
                         v
              resolved bidi levels
                         |
                         v
        visible runs in increasing inline position
```

Line breaking and inline-edge attachment consume this sequence.  They cannot
repair a wrong paragraph-wide visual order.

Trailing whitespace is resolved before visual alignment from the final source
item on the materialised line.  The paragraph base direction cannot select a
visual extreme as a substitute: a styled left-to-right run ending in an
inter-word space can appear at that extreme inside a right-to-left paragraph,
but its space is not line-trailing whitespace.  Stable source item identity
therefore selects one of the closed states `Text`, `InlineBox`, or `Absent`;
only the source-terminal text state can supply a hanging whitespace advance.

## Regression controls

The CSS2 `bidi-005` control uses nested left-to-right and right-to-left
overrides to produce the visible sequence `abcdefghijklm`.  The direct Parley
control asserts that sequence before inline geometry is attached.  A second
control inserts the two source inline boundaries and requires the same
sequence.  Existing Hebrew, number, repeated line-breaking, and inline-box
identity controls remain unchanged.

Physical-left alignment controls cover two styled left-to-right runs inside a
right-to-left paragraph.  The internal space ending the first source run must
not shift the positioned line, while a genuine source-terminal space retains
the existing hanging behaviour.  A source-terminal inline box supplies no
text whitespace advance.
