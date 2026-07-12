// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Line breaking around inline boxes separated by collapsible spaces.
//!
//! CSS Text 3 §3.1.2: collapsible spaces at the end of a line are
//! removed (they hang past the edge); a wrapped line never begins with
//! removable whitespace. A sequence of full-width inline boxes divided
//! by single spaces must therefore stack one box per line — the space
//! after each box hangs on that box's line instead of wrapping into a
//! line of its own.

use alloc::vec::Vec;

use super::test_builders::create_font_context;
use crate::{FontFamily, InlineBox, LayoutContext, StyleProperty};

use super::utils::ColorBrush;

#[test]
fn spaces_between_full_width_inline_boxes_hang_instead_of_wrapping() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    // Three boxes at 199px in a 200px line: each fills its line, and
    // the single space between consecutive boxes cannot fit after one.
    let text = "  ";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    for (id, index) in [(0_u64, 0_usize), (1, 1), (2, 2)] {
        builder.push_inline_box(InlineBox {
            id,
            index,
            width: 199.0,
            height: 8.0,
        });
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(200.0));

    assert_eq!(
        layout.len(),
        3,
        "three full-width boxes with collapsible spaces between them \
         must produce exactly three lines (the spaces hang at line \
         ends); lines: {:?}",
        layout
            .lines()
            .map(|line| (line.text_range(), line.metrics().advance))
            .collect::<Vec<_>>(),
    );
}
