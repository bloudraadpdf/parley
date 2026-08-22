// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    Alignment, AlignmentOptions, BaseDirection, BreakReason, FontFamily, InlineBox,
    InlineBoxBreakAffinity, Layout, LayoutContext, LineBreakOverride, RangedBuilder, StyleProperty,
    TabSize, TextWrapMode, WhiteSpaceCollapse,
};
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};
use peniko::color::palette;

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

fn first_line_visible_advance(layout: &Layout<ColorBrush>) -> f32 {
    layout
        .lines()
        .next()
        .unwrap()
        .runs()
        .map(|run| run.clusters().map(|cluster| cluster.advance()).sum::<f32>())
        .sum()
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
    configured_layout(text, |builder| {
        if let Some(white_space_collapse) = white_space_collapse {
            builder.push_default(StyleProperty::WhiteSpaceCollapse(white_space_collapse));
        }
        builder.push_default(StyleProperty::TextWrapMode(text_wrap_mode));
    })
}

fn configured_layout(
    text: &str,
    configure: impl FnOnce(&mut RangedBuilder<'_, ColorBrush>),
) -> Layout<ColorBrush> {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    set_roboto(&mut builder);
    configure(&mut builder);
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

fn configured_pre_wrap_layout(
    text: &str,
    configure: impl FnOnce(&mut RangedBuilder<'_, ColorBrush>),
) -> Layout<ColorBrush> {
    configured_layout(text, |builder| {
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::Preserve,
        ));
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::Wrap));
        configure(builder);
    })
}

fn split_ranged_pre_wrap_layout(text: &str) -> Layout<ColorBrush> {
    configured_pre_wrap_layout(text, |builder| {
        builder.push(StyleProperty::FontSize(18.0), 3..4);
    })
}

fn align_first_line(
    layout: &mut Layout<ColorBrush>,
    width: f32,
    alignment: Alignment,
) -> (f32, f32) {
    layout.align(Some(width), alignment, AlignmentOptions::default());
    let first = layout.lines().next().unwrap();
    let metrics = first.metrics();
    (metrics.offset, metrics.trailing_whitespace)
}

fn assert_constrained_terminal_whitespace(
    mut layout: Layout<ColorBrush>,
    content_advance: f32,
    expected_hanging: f32,
) {
    layout.break_all_lines(Some(content_advance));
    let first = layout.lines().next().unwrap();
    assert_eq!(first.break_reason(), BreakReason::Explicit);
    assert_close(first.metrics().trailing_whitespace, expected_hanging);
    assert_close(layout.width(), content_advance);
}

fn assert_centered_terminal_side(
    layout: &mut Layout<ColorBrush>,
    expected_offset: impl FnOnce(f32) -> f32,
) {
    let first = layout.lines().next().unwrap();
    let canonical_trailing = first.metrics().trailing_whitespace;
    let content_advance = first.metrics().advance - canonical_trailing;
    assert!(canonical_trailing > 0.0);

    let (offset, trailing) = align_first_line(layout, content_advance + 100.0, Alignment::Center);
    assert_close(offset, expected_offset(canonical_trailing));
    assert_close(trailing, canonical_trailing);
}

fn assert_terminal_space_is_measured(mut layout: Layout<ColorBrush>, expected: f32) {
    let widths = layout.calculate_content_widths();
    layout.break_all_lines(None);

    assert_eq!(widths.min, expected);
    assert_eq!(widths.max, expected);
    assert_eq!(layout.width(), expected);
    assert_eq!(layout.full_width(), expected);
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

    let line = layout.lines().next().unwrap();
    assert!(line.metrics().trailing_whitespace > 0.0);
    let visible_advance = first_line_visible_advance(&layout);
    assert!(visible_advance > expected_width);
    assert_eq!(layout.width(), expected_width);
    assert_eq!(layout.calculate_content_widths().max, expected_width);
}

#[test]
fn collapsed_terminal_sequence_preserves_interspersed_spaces() {
    let text = "XX\u{3000} \u{3000}";
    let mut measured = roboto_layout(text, Some(WhiteSpaceCollapse::BreakSpaces));
    measured.break_all_lines(None);
    let expected_visible = first_line_visible_advance(&measured);
    let mut layout = roboto_layout(text, None);

    layout.break_all_lines(None);

    let visible_advance = first_line_visible_advance(&layout);
    assert_close(visible_advance, expected_visible);
    assert_close(layout.width(), full_width("XX"));
}

#[test]
fn collapsed_terminal_sequence_removes_only_the_collapsible_suffix() {
    let visible_advance = |text| {
        let mut layout = roboto_layout(text, None);
        layout.break_all_lines(None);
        first_line_visible_advance(&layout)
    };

    assert_close(
        visible_advance("XX\u{3000} "),
        visible_advance("XX\u{3000}"),
    );
    let mut terminal_ogham = roboto_layout("XX\u{1680}", None);
    terminal_ogham.break_all_lines(None);
    assert_eq!(
        terminal_ogham.data.lines[0].removed_terminal_source_ranges,
        [2..5]
    );
    let mut interspersed_ogham = roboto_layout("XX\u{1680}\u{3000}", None);
    interspersed_ogham.break_all_lines(None);
    assert!(
        interspersed_ogham.data.lines[0]
            .removed_terminal_source_ranges
            .is_empty()
    );
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
    let text = "XX ";
    let layout = configured_layout(text, |builder| {
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
    });
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
fn conditional_terminal_forced_break_measures_a_fitting_space() {
    let text = "XX \nX";
    let expected = full_width("XX ");
    let alignment_width = expected * 2.0;
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

    layout.break_all_lines(Some(alignment_width));
    assert_close(layout.width(), expected);
    assert_close(layout.full_width(), expected);

    let first = layout.lines().next().unwrap();
    assert_eq!(first.break_reason(), BreakReason::Explicit);
    assert_eq!(first.metrics().trailing_whitespace, 0.0);

    let (offset, trailing) = align_first_line(&mut layout, alignment_width, Alignment::Center);
    assert_close(offset, (alignment_width - expected) * 0.5);
    assert_eq!(trailing, 0.0);
}

#[test]
fn conditional_terminal_forced_break_hangs_only_the_overflow() {
    let text = "XX \nX";
    let word = full_width("XX");
    let with_space = full_width("XX ");
    let space = with_space - word;
    let constrained_width = word + space * 0.5;
    let expected_hanging = with_space - constrained_width;
    let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

    layout.break_all_lines(Some(constrained_width));
    let first = layout.lines().next().unwrap();
    assert_eq!(first.break_reason(), BreakReason::Explicit);
    assert_close(first.metrics().trailing_whitespace, expected_hanging);
    assert_close(layout.width(), constrained_width);
    assert_close(layout.full_width(), with_space);

    let (offset, trailing) = align_first_line(&mut layout, constrained_width, Alignment::End);
    assert_close(offset, 0.0);
    assert_close(trailing, expected_hanging);

    let wider_alignment = with_space * 2.0;
    let (offset, trailing) = align_first_line(&mut layout, wider_alignment, Alignment::End);
    assert_close(offset, wider_alignment - with_space);
    assert_close(trailing, expected_hanging);

    let (offset, trailing) = align_first_line(&mut layout, constrained_width, Alignment::End);
    assert_close(offset, 0.0);
    assert_close(trailing, expected_hanging);
}

#[test]
fn conditional_terminal_spans_ranged_text_runs() {
    let text = "XX  \nX";
    let content = full_width("XX");
    let mut probe = split_ranged_pre_wrap_layout(text);
    probe.break_all_lines(None);
    let expected_hanging = probe.lines().next().unwrap().metrics().advance - content;
    assert_constrained_terminal_whitespace(
        split_ranged_pre_wrap_layout(text),
        content,
        expected_hanging,
    );
}

#[test]
fn conditional_terminal_spans_a_transparent_owner_edge() {
    let text = "XX  \nX";
    let content = full_width("XX");
    let expected_hanging = full_width("XX  ") - content;
    let layout = configured_pre_wrap_layout(text, |builder| {
        builder.push_inline_box(InlineBox::inline_end_edge(
            2,
            3,
            0.0,
            0.0,
            InlineBoxBreakAffinity::ToPrevious,
        ));
    });

    assert_constrained_terminal_whitespace(layout, content, expected_hanging);
}

#[test]
fn conditional_terminal_scans_rtl_clusters_in_logical_order() {
    let text = "אב  \nא";
    let content = full_width("אב");
    let expected_hanging = full_width("אב  ") - content;
    let layout = configured_pre_wrap_layout(text, |builder| {
        builder.set_direction(BaseDirection::Rtl);
    });
    assert_constrained_terminal_whitespace(layout, content, expected_hanging);
}

#[test]
fn conditional_terminal_alignment_uses_rtl_paragraph_end() {
    let text = "one two three four";
    let mut layout = configured_pre_wrap_layout(text, |builder| {
        builder.set_direction(BaseDirection::Rtl);
    });
    layout.set_line_break_overrides(vec![LineBreakOverride::opportunity(14)]);
    layout.break_all_lines(Some(full_width("one two three")));

    assert_centered_terminal_side(&mut layout, |trailing| 50.0 - trailing);
}

#[test]
fn conditional_terminal_alignment_uses_ltr_paragraph_end() {
    let text = "\u{202e}one two \u{202c}";
    let mut layout = configured_pre_wrap_layout(text, |builder| {
        builder.set_direction(BaseDirection::Ltr);
    });
    layout.break_all_lines(None);

    assert_centered_terminal_side(&mut layout, |_| 50.0);
}

#[test]
fn conditional_terminal_length_breaker_preserves_a_final_newline() {
    for max_chars in [4, 100] {
        let text = "XX \n";
        let expected = full_width("XX ");
        let mut layout = roboto_layout(text, Some(WhiteSpaceCollapse::Preserve));

        {
            let mut lines = layout.break_lines();
            assert_eq!(lines.break_next_with_length(max_chars), Some(()));
            assert_eq!(lines.break_next_with_length(max_chars), Some(()));
        }

        let lines = layout.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].break_reason(), BreakReason::Explicit);
        assert_close(lines[0].metrics().advance, expected);
        assert_close(lines[0].metrics().trailing_whitespace, 0.0);
        assert_eq!(lines[1].break_reason(), BreakReason::None);
    }
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

#[test]
fn terminal_space_metrics_distinguish_measured_and_hanging_advance() {
    let width = full_width("XXX");

    let mut pre = roboto_layout_with_white_space(
        "XXX \nX",
        Some(WhiteSpaceCollapse::Preserve),
        TextWrapMode::NoWrap,
    );
    pre.break_all_lines(Some(width));
    let pre_metrics = *pre.lines().next().unwrap().metrics();
    assert_eq!(pre_metrics.trailing_whitespace, 0.0);
    assert_eq!(pre_metrics.hanging_whitespace, 0.0);

    let mut pre_wrap = roboto_layout("XXX X", Some(WhiteSpaceCollapse::Preserve));
    pre_wrap.break_all_lines(Some(width));
    let pre_wrap_metrics = *pre_wrap.lines().next().unwrap().metrics();
    assert!(pre_wrap_metrics.trailing_whitespace > 0.0);
    assert_eq!(
        pre_wrap_metrics.hanging_whitespace,
        pre_wrap_metrics.trailing_whitespace
    );

    let mut break_spaces = roboto_layout("XXX X", Some(WhiteSpaceCollapse::BreakSpaces));
    break_spaces.break_all_lines(Some(width));
    let break_spaces_metrics = *break_spaces.lines().next().unwrap().metrics();
    assert_eq!(break_spaces_metrics.trailing_whitespace, 0.0);
    assert_eq!(break_spaces_metrics.hanging_whitespace, 0.0);
}

#[test]
fn rtl_override_centres_source_terminal_hanging_whitespace() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "\u{202e}one \u{200b}two \u{200b}three \u{200b}four\u{202c}";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.set_direction(BaseDirection::Rtl);
    set_roboto(&mut builder);
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::Preserve,
    ));
    let mut layout = builder.build(text);
    layout.set_line_break_overrides(vec![
        LineBreakOverride::suppress(text.find("two").unwrap()),
        LineBreakOverride::suppress(text.find("three").unwrap()),
        LineBreakOverride::opportunity(text.find("four").unwrap()),
    ]);
    layout.break_all_lines(Some(full_width("one two three") + 5.0));

    let line = layout.lines().next().unwrap();
    let trailing_whitespace = line.metrics().trailing_whitespace;
    let content_advance = line.metrics().advance - trailing_whitespace;
    let alignment_width = content_advance + 100.0;
    assert!(
        trailing_whitespace > 0.0,
        "runs={:?}",
        line.runs()
            .map(|run| (run.text_range(), run.is_rtl(), run.advance()))
            .collect::<Vec<_>>()
    );

    layout.align(
        Some(alignment_width),
        Alignment::Center,
        AlignmentOptions::default(),
    );

    let offset = layout.lines().next().unwrap().metrics().offset;
    assert!((offset - (50.0 - trailing_whitespace)).abs() < 0.01);
}

#[test]
fn selected_line_end_applies_paragraph_level_before_bidi_reordering() {
    let text = "XXX X";
    let mut layout = configured_pre_wrap_layout(text, |builder| {
        builder.set_direction(BaseDirection::Rtl);
    });
    layout.set_line_break_overrides(vec![LineBreakOverride::opportunity(4)]);
    layout.break_all_lines(Some(full_width("XXX")));

    let first = layout.lines().next().unwrap();
    assert_eq!(&text[first.text_range()], "XXX ");
    assert_eq!(
        first.runs().map(|run| run.text_range()).collect::<Vec<_>>(),
        [3..4, 0..3]
    );
}

#[test]
fn selected_line_end_uses_the_automatic_level_of_its_paragraph() {
    let text = "abc\nא XXX X";
    let mut layout = configured_pre_wrap_layout(text, |builder| {
        builder.set_direction(BaseDirection::Auto);
    });
    layout.set_line_break_overrides(vec![LineBreakOverride::opportunity(11)]);
    layout.break_all_lines(Some(full_width("א XXX")));

    let second = layout.lines().nth(1).unwrap();
    assert_eq!(&text[second.text_range()], "א XXX ");
    assert_eq!(second.runs().next().unwrap().text_range(), 10..11);
}

#[test]
fn soft_wrapped_collapsible_space_is_removed_before_bidi_reordering() {
    let text = "A B ا ب";
    let final_word = text.rfind('ب').unwrap();
    let mut layout = configured_layout(text, |builder| {
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::Collapse,
        ));
        builder.push(
            StyleProperty::Brush(ColorBrush::new(palette::css::BLUE)),
            1..2,
        );
        builder.push(
            StyleProperty::Brush(ColorBrush::new(palette::css::RED)),
            3..4,
        );
        builder.push(
            StyleProperty::Brush(ColorBrush::new(palette::css::GREEN)),
            final_word - 1..final_word,
        );
        for (id, range) in [(10, 1..2), (20, 3..4), (30, final_word - 1..final_word)] {
            builder.push_inline_box(InlineBox::inline_start_edge(
                id,
                range.start,
                0.0,
                0.0,
                InlineBoxBreakAffinity::ToNext,
            ));
            builder.push_inline_box(InlineBox::inline_end_edge(
                id + 1,
                range.end,
                0.0,
                0.0,
                InlineBoxBreakAffinity::ToPrevious,
            ));
        }
    });
    layout.break_all_lines(Some(full_width("A B ا")));

    let first = layout.lines().next().unwrap();
    assert_eq!(&text[first.text_range()], "A B ا ");
    let terminal_space_advance = first
        .runs()
        .find_map(|run| {
            run.clusters()
                .find(|cluster| cluster.text_range() == (final_word - 1..final_word))
                .map(|cluster| cluster.advance())
        })
        .expect("the selected line retains its terminal source cluster");
    assert_close(terminal_space_advance, 0.0);
}
