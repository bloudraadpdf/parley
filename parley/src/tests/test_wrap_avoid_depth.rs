// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A soft wrap opportunity in a shallower `wrap-inside: avoid` box is taken before one in a deeper box.

use alloc::vec::Vec;

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, Layout, LayoutContext, StyleProperty};

const TEXT: &str = "aaaa bbbb cccc dddd";

/// Breaks `TEXT` to a width that holds three words, with each `(range, depth)` an avoid box.
fn line_ends(boxes: &[(core::ops::Range<usize>, u8)]) -> Vec<usize> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, TEXT, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    for (range, depth) in boxes {
        builder.push(StyleProperty::WrapAvoidDepth(*depth), range.clone());
    }
    let mut layout: Layout<ColorBrush> = builder.build(TEXT);
    let mut natural = layout_context.ranged_builder(&mut font_context, "aaaa bbbb cccc", 1.0, false);
    natural.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    natural.push_default(StyleProperty::FontSize(12.0));
    let mut three_words: Layout<ColorBrush> = natural.build("aaaa bbbb cccc");
    three_words.break_all_lines(None);
    layout.break_all_lines(Some(three_words.full_width() + 1.0));
    layout.lines().map(|line| line.text_range().end).collect()
}

#[test]
fn text_breaks_at_the_last_opportunity_outside_any_avoid_box() {
    assert_eq!(line_ends(&[]), [15, 19]);
}

/// Breaking in the box is only a last resort: the earlier opportunity outside it is taken instead.
#[test]
fn a_box_is_kept_together_when_a_break_precedes_it() {
    assert_eq!(line_ends(&[(5..19, 1)]), [5, 19]);
}

#[test]
fn a_box_breaks_inside_when_no_other_opportunity_exists() {
    assert_eq!(line_ends(&[(0..19, 1)]), [15, 19]);
}

/// A break in an outer box is used before a break in an inner box.
#[test]
fn an_outer_box_breaks_before_an_inner_box() {
    assert_eq!(line_ends(&[(0..19, 1), (5..19, 2)]), [5, 19]);
}
