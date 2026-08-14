# Overflowing no-break glue

Unicode U+00A0 NO-BREAK SPACE has line-break class GL. Under normal wrapping,
it is spacing material but does not supply a soft-wrap opportunity. If text
joined by U+00A0 exceeds the available advance, the joined text remains on the
same overflowing line unless an explicit emergency wrapping policy applies.

The greedy breaker therefore keeps overflow classification separate from
spacing classification. An overflowing whitespace cluster is either
collapsible whitespace carrying a normal soft-wrap opportunity, no-break glue,
or other content. Only the first state may select the trailing-collapsible-
space transition. No-break glue follows the ordinary content-overflow path;
an emergency break requires a separately recorded emergency opportunity.

Focused controls cover U+0020 and U+00A0 under normal wrapping, U+00A0 under
emergency wrapping, and an unbounded layout. They ensure shared spacing and
justification predicates cannot manufacture a line-break capability.
