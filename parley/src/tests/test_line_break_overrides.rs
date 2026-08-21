// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    Alignment, AlignmentOptions, BreakReason, FontFamily, InlineBox, InlineBoxBreakAffinity,
    Layout, LayoutContext, LineBreakOverride, RangedBuilder, StyleProperty, TabSize, TextWrapMode,
    WhiteSpaceCollapse,
};
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

fn set_roboto(builder: &mut RangedBuilder<'_, ColorBrush>) {
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
}

fn line_texts(layout: &Layout<ColorBrush>, text: &str) -> Vec<String> {
    layout
        .lines()
        .map(|line| text[line.text_range()].to_string())
        .collect()
}

fn roboto_layout(
    text: &str,
    white_space_collapse: Option<WhiteSpaceCollapse>,
) -> Layout<ColorBrush> {
    roboto_layout_with_white_space(text, white_space_collapse, TextWrapMode::Wrap)
}

fn roboto_layout_with_white_space(
    text: &str,
    white_space_collapse: Option<WhiteSpaceCollapse>,
    text_wrap_mode: TextWrapMode,
) -> Layout<ColorBrush> {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    set_roboto(&mut builder);
    if let Some(white_space_collapse) = white_space_collapse {
        builder.push_default(StyleProperty::WhiteSpaceCollapse(white_space_collapse));
    }
    builder.push_default(StyleProperty::TextWrapMode(text_wrap_mode));
    builder.build(text)
}

fn full_width(text: &str) -> f32 {
    let mut layout = roboto_layout(text, None);
    layout.break_all_lines(None);
    layout.full_width()
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.001,
        "expected {expected}, got {actual}",
    );
}

fn align_end_first_line(layout: &mut Layout<ColorBrush>, width: f32) -> (f32, f32) {
    layout.align(Some(width), Alignment::End, AlignmentOptions::default());
    let first = layout.lines().next().unwrap();
    let metrics = first.metrics();
    (metrics.offset, metrics.trailing_whitespace)
}

fn assert_terminal_space_is_measured(mut layout: Layout<ColorBrush>, expected: f32) {
    let widths = layout.calculate_content_widths();
    layout.break_all_lines(None);

    assert_eq!(widths.min, expected);
    assert_eq!(widths.max, expected);
    assert_close(layout.width(), expected);
    assert_close(layout.full_width(), expected);
    assert_eq!(
        layout.lines().next().unwrap().metrics().trailing_whitespace,
        0.0
    );
}

#[test]
fn caller_overrides_replace_unicode_slash_opportunities() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "aa/bb/cc";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    set_roboto(&mut builder);
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

    assert_eq!(line_texts(&layout, text), ["aa", "/bb", "/cc"]);
}

#[test]
fn exact_tab_stop_uses_the_line_fit_error_bound() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "XX\t\tXX";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    set_roboto(&mut builder);
    builder.push_default(StyleProperty::TabSize(TabSize::Length(120.0)));
    let mut layout = builder.build(text);

    layout.set_line_break_overrides(vec![
        LineBreakOverride::opportunity(3),
        LineBreakOverride::opportunity(4),
    ]);
    layout.break_all_lines(Some(239.99998));

    assert_eq!(line_texts(&layout, text), ["XX\t\t", "XX"]);
}

#[test]
fn tabs_observe_the_half_zero_advance_threshold() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let zero_advance = full_width("0");
    let following_advance = full_width("P");
    let interval = 80.0;
    let text = "\tP";

    for (remaining_zero_fraction, stop_count) in [(0.4, 2.0), (0.6, 1.0)] {
        let inline_advance = interval - zero_advance * remaining_zero_fraction;
        let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
        set_roboto(&mut builder);
        builder.push_default(StyleProperty::TabSize(TabSize::Length(interval)));
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::Preserve,
        ));
        builder.push_inline_box(InlineBox::new(1, 0, inline_advance, 0.0));
        let mut layout = builder.build(text);

        layout.break_all_lines(None);

        assert!((layout.full_width() - (interval * stop_count + following_advance)).abs() < 0.01);
    }
}

#[test]
fn break_spaces_rewinds_before_an_overflowing_preserved_space() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "X\t X";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    set_roboto(&mut builder);
    builder.push_default(StyleProperty::TabSize(TabSize::Length(120.0)));
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::BreakSpaces,
    ));
    let mut layout = builder.build(text);

    layout.set_line_break_overrides(vec![LineBreakOverride::opportunity(2)]);
    layout.break_all_lines(Some(12.0));

    assert_eq!(line_texts(&layout, text), ["X\t", " X"]);
}

#[test]
fn pre_wrap_hangs_a_complete_preserved_space_sequence() {
    let text = "XX    XX";
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

    layout.set_line_break_overrides(vec![LineBreakOverride::opportunity(6)]);
    layout.break_all_lines(Some(18.0));

    assert_eq!(line_texts(&layout, text), ["XX    ", "XX"]);
}

#[test]
fn pre_wrap_hanging_space_does_not_take_an_earlier_opportunity() {
    let measure = full_width("X X");
    let text = "X X X X ";
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

    layout.set_line_break_overrides(vec![
        LineBreakOverride::opportunity(2),
        LineBreakOverride::opportunity(4),
        LineBreakOverride::opportunity(6),
    ]);
    layout.break_all_lines(Some(measure));

    assert_eq!(line_texts(&layout, text), ["X X ", "X X "]);
}

#[test]
fn pre_wrap_hanging_space_does_not_take_an_earlier_opportunity_across_controls() {
    let measure = full_width("X X");
    let text = "X \u{200b}X \u{200b}X \u{200b}X ";
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

    layout.break_all_lines(Some(measure));

    assert_eq!(
        line_texts(&layout, text),
        ["X \u{200b}X \u{200b}", "X \u{200b}X "]
    );
}

#[test]
fn pre_wrap_space_hangs_through_a_default_ignorable_boundary() {
    let text = "X \u{200b}";
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));
    layout.break_all_lines(None);

    assert!(layout.lines().next().unwrap().metrics().trailing_whitespace > 0.0);
}

#[test]
fn other_space_separators_hang_as_a_complete_sequence() {
    let expected_width = full_width("XX");
    let text = "XX\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{202f}\u{205f}\u{3000}";
    let mut layout = roboto_layout(text, None);

    layout.break_all_lines(None);

    assert!(layout.lines().next().unwrap().metrics().trailing_whitespace > 0.0);
    assert_eq!(layout.width(), expected_width);
    assert_eq!(layout.calculate_content_widths().max, expected_width);
}

#[test]
fn mixed_other_space_separator_sequence_hangs_before_following_text() {
    let text = "XX\u{3000}\u{3000} \u{3000} \u{3000}XX";
    let mut layout = roboto_layout(text, None);

    layout.break_all_lines(Some(30.0));

    assert_eq!(line_texts(&layout, text), [&text[..text.len() - 2], "XX"]);
}

#[test]
fn break_spaces_measures_other_space_separators_as_content() {
    let expected_width = full_width("XX");
    let text = "XX\u{1680}\u{2000}\u{3000}";
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::BreakSpaces));

    layout.break_all_lines(None);

    assert!(layout.full_width() > expected_width);
    assert!(layout.calculate_content_widths().max > expected_width);
    assert_eq!(
        layout.lines().next().unwrap().metrics().trailing_whitespace,
        0.0
    );
}

#[test]
fn pre_terminal_space_is_measured() {
    let text = "XX ";
    let expected = full_width(text);
    let layout = roboto_layout_with_white_space(
        text,
        Some(WhiteSpaceCollapse::Preserve),
        TextWrapMode::NoWrap,
    );
    assert_terminal_space_is_measured(layout, expected);
}

#[test]
fn pre_terminal_space_is_measured_across_transparent_owner_end() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "XX ";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    set_roboto(&mut builder);
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::Preserve,
    ));
    builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
    builder.push_inline_box(InlineBox::inline_end_edge(
        1,
        text.len(),
        0.0,
        0.0,
        InlineBoxBreakAffinity::ToPrevious,
    ));
    let layout = builder.build(text);
    let expected = full_width(text);
    assert_terminal_space_is_measured(layout, expected);
}

#[test]
fn pre_forced_break_space_is_measured() {
    let expected = full_width("XX ");
    let mut layout = roboto_layout_with_white_space(
        "XX \nX",
        Some(WhiteSpaceCollapse::Preserve),
        TextWrapMode::NoWrap,
    );

    assert_eq!(layout.calculate_content_widths().max, expected);
    layout.break_all_lines(None);
    let first = layout.lines().next().unwrap();
    assert_eq!(first.metrics().advance, expected);
    assert_eq!(first.metrics().trailing_whitespace, 0.0);
}

#[test]
fn pre_wrap_forced_break_measures_a_fitting_terminal_space() {
    let text = "XX \nX";
    let expected = full_width("XX ");
    let alignment_width = expected * 2.0;
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

    layout.break_all_lines(Some(alignment_width));
    assert_eq!(layout.width(), expected);
    assert_eq!(layout.full_width(), expected);

    let first = layout.lines().next().unwrap();
    assert_eq!(first.break_reason(), BreakReason::Explicit);
    assert_eq!(first.metrics().trailing_whitespace, 0.0);

    layout.align(
        Some(alignment_width),
        Alignment::Center,
        AlignmentOptions::default(),
    );
    assert_close(
        layout.lines().next().unwrap().metrics().offset,
        (alignment_width - expected) * 0.5,
    );
}

#[test]
fn pre_wrap_forced_break_hangs_an_overflowing_terminal_space() {
    let text = "XX \nX";
    let word = full_width("XX");
    let with_space = full_width("XX ");
    let space = with_space - word;
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

    let constrained_width = word + space * 0.5;
    let expected_hanging = with_space - constrained_width;
    layout.break_all_lines(Some(constrained_width));

    let first = layout.lines().next().unwrap();
    assert_eq!(first.break_reason(), BreakReason::Explicit);
    assert_close(first.metrics().trailing_whitespace, expected_hanging);
    assert_close(layout.width(), constrained_width);
    assert_close(layout.full_width(), with_space);

    let (offset, trailing) = align_end_first_line(&mut layout, constrained_width);
    assert_close(offset, 0.0);
    assert_close(trailing, expected_hanging);

    let wider_alignment = with_space * 2.0;
    let (offset, trailing) = align_end_first_line(&mut layout, wider_alignment);
    assert_close(offset, wider_alignment - with_space);
    assert_close(trailing, expected_hanging);

    let (offset, trailing) = align_end_first_line(&mut layout, constrained_width);
    assert_close(offset, 0.0);
    assert_close(trailing, expected_hanging);
}

#[test]
fn length_breaker_preserves_a_terminal_forced_break() {
    let text = "XX \n";
    let expected = full_width("XX ");
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

    {
        let mut lines = layout.break_lines();
        assert_eq!(lines.break_next_with_length(4), Some(()));
        assert_eq!(lines.break_next_with_length(4), Some(()));
    }

    let lines = layout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].break_reason(), BreakReason::Explicit);
    assert_close(lines[0].metrics().advance, expected);
    assert_close(lines[0].metrics().trailing_whitespace, 0.0);
    assert_eq!(lines[1].break_reason(), BreakReason::None);
}

#[test]
fn pre_wrap_forced_break_spaces_only_hang_from_min_content() {
    let forced_break = roboto_layout("XX   \nX", Some(WhiteSpaceCollapse::Preserve));
    let expected_max = {
        let mut layout = roboto_layout("XX   ", Some(WhiteSpaceCollapse::BreakSpaces));
        layout.break_all_lines(None);
        layout.full_width()
    };
    let terminal = roboto_layout("XX   ", Some(WhiteSpaceCollapse::Preserve));

    assert_eq!(forced_break.calculate_content_widths().max, expected_max);
    assert_eq!(terminal.calculate_content_widths().max, full_width("XX"));
}

#[test]
fn other_space_separator_before_forced_break_hangs_from_max_content() {
    let layout = roboto_layout("XX\u{3000}\nX", None);

    assert_eq!(layout.calculate_content_widths().max, full_width("XX"));
}

#[test]
fn break_spaces_preserves_trailing_space_measurement() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "X  ";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    set_roboto(&mut builder);
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::BreakSpaces,
    ));
    let mut layout = builder.build(text);
    layout.set_line_break_overrides(vec![LineBreakOverride::opportunity(2)]);

    let expected_min = {
        let text = "X ";
        let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
        set_roboto(&mut builder);
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::BreakSpaces,
        ));
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout.full_width()
    };
    assert_eq!(layout.calculate_content_widths().min, expected_min);

    layout.break_all_lines(None);
    assert_eq!(
        layout.lines().next().unwrap().metrics().trailing_whitespace,
        0.0
    );
}
