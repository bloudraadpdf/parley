// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Line-break capabilities carried by overflowing whitespace.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, LayoutContext, OverflowWrap, StyleProperty};

fn build(text: &str, overflow_wrap: OverflowWrap) -> crate::Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::OverflowWrap(overflow_wrap));
    builder.build(text)
}

#[test]
fn overflowing_no_break_space_does_not_create_a_normal_wrap_opportunity() {
    let mut layout = build("A\u{00A0}B", OverflowWrap::Normal);

    layout.break_all_lines(Some(1.0));

    assert_eq!(layout.len(), 1);
}

#[test]
fn overflowing_collapsible_space_retains_its_normal_wrap_capability() {
    let mut layout = build("A B", OverflowWrap::Normal);

    layout.break_all_lines(Some(1.0));

    assert_eq!(layout.len(), 2);
}

#[test]
fn emergency_policy_supplies_a_distinct_break_for_no_break_glue() {
    let mut layout = build("A\u{00A0}B", OverflowWrap::Anywhere);

    layout.break_all_lines(Some(1.0));

    assert_eq!(layout.len(), 3);
}

#[test]
fn unbounded_no_break_glue_remains_on_one_line() {
    let mut layout = build("A\u{00A0}B", OverflowWrap::Normal);

    layout.break_all_lines(None);

    assert_eq!(layout.len(), 1);
}
