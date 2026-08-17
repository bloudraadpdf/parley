# Bidi paragraph boundaries

Parley resolves the Unicode Bidirectional Algorithm for each paragraph.
U+000A LINE FEED and U+2029 PARAGRAPH SEPARATOR end the current paragraph.
U+2028 LINE SEPARATOR ends only the current line.

```text
paragraph 1 + B boundary + paragraph 2 -> resolve 1 -> resolve 2
paragraph 1 + LS         + continuation -> one bidi resolution
```

Weak and neutral resolution cannot cross a B-class boundary. Each paragraph
can select its own automatic base level. The layout keeps the first paragraph
base level as the layout-level direction and retains each resolved character
level for line reordering.
