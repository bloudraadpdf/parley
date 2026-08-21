// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    FontFamily, LayoutContext, LineBreakOverride, StyleProperty, TabSize, WhiteSpaceCollapse,
};
use alloc::{string::ToString, vec, vec::Vec};

#[test]
fn caller_overrides_replace_unicode_slash_opportunities() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "aa/bb/cc";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    let mut layout = builder.build(text);

    // Suppress the Unicode opportunities after each slash and add the custom
    // opportunities immediately before them.
    layout.set_line_break_overrides(vec![
        LineBreakOverride::opportunity(2),
        LineBreakOverride::suppress(3),
        LineBreakOverride::opportunity(5),
        LineBreakOverride::suppress(6),
    ]);
    layout.break_all_lines(Some(20.0));

    let lines = layout
        .lines()
        .map(|line| text[line.text_range()].to_string())
        .collect::<Vec<_>>();
    assert_eq!(lines, ["aa", "/bb", "/cc"]);
}

#[test]
fn exact_tab_stop_uses_the_line_fit_error_bound() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "XX\t\tXX";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::TabSize(TabSize::Length(120.0)));
    let mut layout = builder.build(text);

    layout.set_line_break_overrides(vec![
        LineBreakOverride::opportunity(3),
        LineBreakOverride::opportunity(4),
    ]);
    layout.break_all_lines(Some(239.99998));

    let lines = layout
        .lines()
        .map(|line| text[line.text_range()].to_string())
        .collect::<Vec<_>>();
    assert_eq!(lines, ["XX\t\t", "XX"]);
}

#[test]
fn break_spaces_rewinds_before_an_overflowing_preserved_space() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "X\t X";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::TabSize(TabSize::Length(120.0)));
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::BreakSpaces,
    ));
    let mut layout = builder.build(text);

    layout.set_line_break_overrides(vec![LineBreakOverride::opportunity(2)]);
    layout.break_all_lines(Some(12.0));

    let lines = layout
        .lines()
        .map(|line| text[line.text_range()].to_string())
        .collect::<Vec<_>>();
    assert_eq!(lines, ["X\t", " X"]);
}
