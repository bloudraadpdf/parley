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

use alloc::{format, string::String, vec, vec::Vec};
use core::ops::Range;

use super::test_builders::create_font_context;
use crate::{
    FontFamily, FontWeight, InlineBox, InlineBoxBreakAffinity, LayoutContext, LineBreakMode,
    LineBreakOverride, LineHeight, NormalSoftWrapSelection, OverflowWrap, RangedBuilder,
    StyleProperty, TextWrapMode, WhiteSpaceCollapse, WordBreak, layout::DiscretionaryBreak,
};

use super::utils::ColorBrush;

fn build_inline_box_layout(
    lcx: &mut LayoutContext<ColorBrush>,
    fcx: &mut crate::FontContext,
    text: &str,
    font_size: f32,
    line_height: Option<LineHeight>,
    inline_boxes: impl IntoIterator<Item = InlineBox>,
) -> crate::Layout<ColorBrush> {
    let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(font_size));
    if let Some(line_height) = line_height {
        builder.push_default(StyleProperty::LineHeight(line_height));
    }
    for inline_box in inline_boxes {
        builder.push_inline_box(inline_box);
    }
    builder.build(text)
}

#[test]
fn source_text_affinity_retains_source_breaks_in_intrinsic_measurement() {
    let mut fcx = create_font_context();
    let mut lcx = LayoutContext::new();
    let mut failures = Vec::new();
    for (text, override_, expected) in [
        ("", None, 200.0),
        (" ", None, 150.0),
        ("\u{200b}", None, 150.0),
        ("A B", None, 150.0),
        ("A\u{200b}B", None, 150.0),
        ("AB", None, 200.0),
        ("\u{a0}", None, 200.0),
        ("\u{2060}", None, 200.0),
        (" \u{2060}", None, 200.0),
        (" ", Some(LineBreakOverride::suppress(1)), 200.0),
        ("A", Some(LineBreakOverride::opportunity(1)), 150.0),
    ] {
        let mut layout = build_inline_box_layout(
            &mut lcx,
            &mut fcx,
            text,
            0.0,
            None,
            (0..4).map(|id| {
                InlineBox::atomic_with_break_affinity(
                    id,
                    if id == 0 { 0 } else { text.len() },
                    50.0,
                    20.0,
                    InlineBoxBreakAffinity::SourceText,
                )
            }),
        );
        layout.set_line_break_overrides(override_.into_iter().collect());
        let intrinsic = layout.calculate_content_widths().min;
        layout.break_all_lines(Some(0.0));
        let measured = layout
            .lines()
            .map(|line| line.metrics().advance)
            .fold(0.0_f32, f32::max);
        if (intrinsic - expected).abs() > 0.01 || (measured - expected).abs() > 0.01 {
            failures.push(format!(
                "{text:?}: intrinsic={intrinsic}, measured={measured}, expected={expected}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn transparent_anchor_does_not_create_a_soft_wrap_opportunity() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "XXXXXX";

    let mut text_only_builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    text_only_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    text_only_builder.push_default(StyleProperty::FontSize(10.0));
    let text_only = text_only_builder.build(text);
    let text_widths = text_only.calculate_content_widths();

    let mut transparent = build_inline_box_layout(
        &mut lcx,
        &mut fcx,
        text,
        10.0,
        None,
        [InlineBox::transparent_anchor(73, 3)],
    );
    let transparent_widths = transparent.calculate_content_widths();
    assert!((transparent_widths.min - text_widths.min).abs() < 0.01);
    assert!((transparent_widths.max - text_widths.max).abs() < 0.01);
    transparent.break_all_lines(Some(text_widths.max - 1.0));

    assert_eq!(transparent.len(), 1);
    assert_eq!(
        transparent
            .lines()
            .flat_map(|line| line.items())
            .filter(|item| matches!(item, crate::PositionedLayoutItem::InlineBox(_)))
            .count(),
        1,
    );

    let mut atomic = build_inline_box_layout(
        &mut lcx,
        &mut fcx,
        text,
        10.0,
        None,
        [InlineBox::new(74, 3, 0.0, 0.0)],
    );
    assert!(atomic.calculate_content_widths().min < text_widths.min);
    atomic.break_all_lines(Some(text_widths.max - 1.0));
    assert_eq!(atomic.len(), 2);
}

#[test]
fn contextual_spacing_disappears_when_its_boundary_wraps() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "AB";
    let spacing = 5.0;

    let mut unwrapped = build_inline_box_layout(
        &mut lcx,
        &mut fcx,
        text,
        10.0,
        None,
        [InlineBox::contextual_spacing(78, 1, spacing, 0.0)],
    );
    unwrapped.break_all_lines(None);
    let unwrapped_advance = unwrapped.lines().next().unwrap().metrics().advance;

    let mut wrapped = build_inline_box_layout(
        &mut lcx,
        &mut fcx,
        text,
        10.0,
        None,
        [InlineBox::contextual_spacing(79, 1, spacing, 0.0)],
    );
    wrapped.break_all_lines(Some((unwrapped_advance - spacing) * 0.6));

    assert_eq!(wrapped.len(), 2);
    let wrapped_advance = wrapped
        .lines()
        .map(|line| line.metrics().advance)
        .sum::<f32>();
    assert!((wrapped_advance + spacing - unwrapped_advance).abs() < 0.01);
    let positioned = positioned_inline_box_ids(&wrapped);
    assert!(positioned.is_empty(), "{positioned:?}");
}

#[test]
fn empty_text_retains_every_transparent_anchor_on_its_line() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = lcx.ranged_builder(&mut fcx, "", 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_inline_box(InlineBox::transparent_anchor(76, 0));
    builder.push_inline_box(InlineBox::transparent_anchor(77, 0));
    let mut layout = builder.build("");

    layout.break_all_lines(None);

    assert_eq!(layout.len(), 1);
    assert_eq!(positioned_inline_box_ids(&layout), [76, 77]);
}

#[test]
fn logical_boundaries_preserve_all_whitespace_run_metrics() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "\u{a0}";
    let mut layout = build_inline_box_layout(
        &mut lcx,
        &mut fcx,
        text,
        16.0,
        Some(LineHeight::Absolute(96.0)),
        [
            InlineBox::inline_start_edge(81, 0, 0.0, 0.0, InlineBoxBreakAffinity::ToNext),
            InlineBox::inline_end_edge(
                82,
                text.len(),
                0.0,
                0.0,
                InlineBoxBreakAffinity::ToPrevious,
            ),
        ],
    );

    layout.break_all_lines(None);

    let line = layout.lines().next().unwrap();
    let run = line.runs().next().unwrap();
    assert!((line.metrics().ascent - run.metrics().ascent).abs() < 0.01);
    assert!((line.metrics().descent - run.metrics().descent).abs() < 0.01);
    assert!((line.metrics().line_height - 96.0).abs() < 0.01);
}

#[test]
fn text_retains_consecutive_transparent_anchors_at_its_start() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "after";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(
        "Missing Fixture Face",
    )));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_inline_box(InlineBox::transparent_anchor(78, 0));
    builder.push_inline_box(InlineBox::transparent_anchor(79, 0));
    builder.push_inline_box(InlineBox::inline_end_edge(
        80,
        text.len(),
        0.0,
        0.0,
        InlineBoxBreakAffinity::ToPrevious,
    ));
    let mut layout = builder.build(text);

    layout.break_all_lines(None);

    assert_eq!(layout.len(), 1);
    assert_eq!(positioned_inline_box_ids(&layout), [78, 79, 80]);
}

#[test]
fn zero_width_owner_end_does_not_follow_hanging_spaces_onto_an_empty_line() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "XX    ";
    let measure = {
        let mut builder = lcx.ranged_builder(&mut fcx, "XX", 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        let mut layout = builder.build("XX");
        layout.break_all_lines(None);
        layout.full_width()
    };
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::Preserve,
    ));
    builder.push_inline_box(InlineBox::new(83, text.len(), 0.0, 0.0));
    let mut layout = builder.build(text);

    layout.break_all_lines(Some(measure));

    assert_eq!(layout.len(), 1);
    assert_eq!(inline_box_line(&layout, 83), 0);
}

fn positioned_inline_box_ids(layout: &crate::Layout<ColorBrush>) -> Vec<u64> {
    layout
        .lines()
        .flat_map(|line| line.items())
        .filter_map(|item| match item {
            crate::PositionedLayoutItem::InlineBox(inline_box) => Some(inline_box.id),
            crate::PositionedLayoutItem::GlyphRun(_) => None,
        })
        .collect()
}

fn logical_inline_edge_fixture(
    text: &str,
    edge_index: usize,
    edge_depth: usize,
    text_wrap_mode: TextWrapMode,
) -> (crate::Layout<ColorBrush>, f32, f32) {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    let mut text_only_builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    text_only_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    text_only_builder.push_default(StyleProperty::FontSize(10.0));
    text_only_builder.push_default(StyleProperty::TextWrapMode(text_wrap_mode));
    let text_only_widths = text_only_builder.build(text).calculate_content_widths();

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::TextWrapMode(text_wrap_mode));
    for depth in 0..edge_depth {
        builder.push_inline_box(InlineBox::inline_end_edge(
            depth as u64,
            edge_index,
            0.0,
            0.0,
            InlineBoxBreakAffinity::ToPrevious,
        ));
    }
    for depth in 0..edge_depth {
        builder.push_inline_box(InlineBox::inline_start_edge(
            (edge_depth + depth) as u64,
            edge_index,
            0.0,
            0.0,
            InlineBoxBreakAffinity::ToNext,
        ));
    }
    (
        builder.build(text),
        text_only_widths.min,
        text_only_widths.max,
    )
}

fn inline_box_line(layout: &crate::Layout<ColorBrush>, id: u64) -> usize {
    layout
        .lines()
        .enumerate()
        .find_map(|(line_index, line)| {
            line.items().find_map(|item| match item {
                crate::PositionedLayoutItem::InlineBox(inline_box) if inline_box.id == id => {
                    Some(line_index)
                }
                _ => None,
            })
        })
        .expect("the logical edge must remain positioned")
}

fn inline_boundary_shaping_layout(edge_width: f32) -> crate::Layout<ColorBrush> {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "AV";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(40.0));
    builder.push_inline_box(InlineBox::inline_end_edge(
        90,
        1,
        edge_width,
        0.0,
        InlineBoxBreakAffinity::ToPrevious,
    ));
    builder.build(text)
}

fn first_line_advance(layout: &mut crate::Layout<ColorBrush>) -> f32 {
    layout.break_all_lines(None);
    layout
        .lines()
        .next()
        .expect("the text must produce one line")
        .metrics()
        .advance
}

#[test]
fn zero_width_logical_edge_preserves_cross_boundary_shaping() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = lcx.ranged_builder(&mut fcx, "AV", 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(40.0));
    let uninterrupted = first_line_advance(&mut builder.build("AV"));

    let mut transparent_layout = inline_boundary_shaping_layout(0.0);
    let transparent = first_line_advance(&mut transparent_layout);
    let intervening = first_line_advance(&mut inline_boundary_shaping_layout(4.0));

    assert_eq!(transparent, uninterrupted);
    assert!(intervening > transparent + 3.9);
    assert_eq!(inline_box_line(&transparent_layout, 90), 0);

    transparent_layout.break_all_lines(Some(uninterrupted));
    assert_eq!(
        transparent_layout
            .lines()
            .next()
            .expect("the transparent edge must survive relayout")
            .metrics()
            .advance,
        uninterrupted,
    );
}

#[test]
fn logical_inline_edge_pair_does_not_split_an_unbreakable_word() {
    let (mut layout, unbroken_min, unbroken_max) =
        logical_inline_edge_fixture("XXXXXXXXXX", 5, 1, TextWrapMode::Wrap);
    let widths = layout.calculate_content_widths();
    layout.break_all_lines(Some(unbroken_max * 0.55));

    assert_eq!(layout.len(), 1);
    assert!((widths.min - unbroken_min).abs() < 0.01);
}

#[test]
fn logical_inline_edges_cannot_retain_a_no_wrap_break_snapshot() {
    let (mut layout, _, unbroken_max) =
        logical_inline_edge_fixture("XXXXXXXXXX", 5, 1, TextWrapMode::NoWrap);
    layout.break_all_lines(Some(unbroken_max * 0.55));

    assert_eq!(layout.len(), 1);
}

#[test]
fn source_space_across_logical_inline_edges_remains_a_wrap_opportunity() {
    let (mut layout, _, unbroken_max) =
        logical_inline_edge_fixture("XXXXX XXXXX", 6, 2, TextWrapMode::Wrap);
    layout.break_all_lines(Some(unbroken_max * 0.55));

    assert_eq!(layout.len(), 2);
    assert_eq!(inline_box_line(&layout, 0), 0);
    assert_eq!(inline_box_line(&layout, 1), 0);
    assert_eq!(inline_box_line(&layout, 2), 1);
    assert_eq!(inline_box_line(&layout, 3), 1);
}

#[test]
fn nested_logical_inline_edges_do_not_split_rtl_text() {
    let text = "אבגדהאבגדה";
    let (mut layout, _, unbroken_max) =
        logical_inline_edge_fixture(text, "אבגדה".len(), 2, TextWrapMode::Wrap);
    layout.break_all_lines(Some(unbroken_max * 0.55));

    assert_eq!(layout.len(), 1);
}

fn owner_transition_layout(
    text: &str,
    edge_index: usize,
    preceding_wrap_mode: TextWrapMode,
    following_wrap_mode: TextWrapMode,
    edge_width: f32,
) -> (crate::Layout<ColorBrush>, f32) {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::TextWrapMode(following_wrap_mode));
    builder.push(
        StyleProperty::TextWrapMode(preceding_wrap_mode),
        0..edge_index,
    );
    builder.push_inline_box(InlineBox::inline_end_edge(
        80,
        edge_index,
        0.0,
        0.0,
        InlineBoxBreakAffinity::ToPrevious,
    ));
    builder.push_inline_box(InlineBox::inline_start_edge(
        81,
        edge_index,
        edge_width,
        0.0,
        InlineBoxBreakAffinity::ToNext,
    ));
    let layout = builder.build(text);
    let max_width = layout.calculate_content_widths().max;
    (layout, max_width)
}

#[test]
fn unicode_opportunity_survives_a_no_wrap_owner_boundary() {
    let (mut layout, max_width) =
        owner_transition_layout("X X", 2, TextWrapMode::NoWrap, TextWrapMode::Wrap, 0.0);
    layout.break_all_lines(Some(max_width * 0.55));

    assert_eq!(layout.len(), 2);
    assert_eq!(inline_box_line(&layout, 80), 0);
    assert_eq!(inline_box_line(&layout, 81), 1);
}

#[test]
fn unicode_opportunity_survives_a_wrapping_to_no_wrap_boundary() {
    let (mut layout, max_width) =
        owner_transition_layout("X X", 2, TextWrapMode::Wrap, TextWrapMode::NoWrap, 0.0);
    layout.break_all_lines(Some(max_width * 0.55));

    assert_eq!(layout.len(), 2);
}

#[test]
fn no_wrap_content_with_owner_edges_remains_unbreakable() {
    let (mut layout, max_width) =
        owner_transition_layout("X X", 2, TextWrapMode::NoWrap, TextWrapMode::NoWrap, 0.0);
    layout.break_all_lines(Some(max_width * 0.55));

    assert_eq!(layout.len(), 1);
}

#[test]
fn logical_edge_geometry_takes_only_a_source_opportunity() {
    let (mut breakable, breakable_width) =
        owner_transition_layout("X X", 2, TextWrapMode::Wrap, TextWrapMode::Wrap, 2.0);
    breakable.break_all_lines(Some(breakable_width * 0.55));
    assert_eq!(breakable.len(), 2);

    let (mut unbreakable, unbreakable_width) =
        owner_transition_layout("XX", 1, TextWrapMode::Wrap, TextWrapMode::Wrap, 2.0);
    unbreakable.break_all_lines(Some(unbreakable_width * 0.55));
    assert_eq!(unbreakable.len(), 1);
}

#[test]
fn logical_start_geometry_wraps_before_its_owner_fragment() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut measure = lcx.ranged_builder(&mut fcx, "X", 1.0, false);
    measure.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    measure.push_default(StyleProperty::FontSize(10.0));
    let character_width = measure.build("X").calculate_content_widths().max;

    let text = "XX X XX";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_inline_box(InlineBox::inline_start_edge(
        82,
        3,
        character_width * 2.5,
        0.0,
        InlineBoxBreakAffinity::ToNext,
    ));
    builder.push_inline_box(InlineBox::inline_end_edge(
        83,
        4,
        0.0,
        0.0,
        InlineBoxBreakAffinity::ToPrevious,
    ));
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(character_width * 4.0));

    assert_eq!(layout.len(), 3);
    assert_eq!(inline_box_line(&layout, 82), 1);
    assert_eq!(inline_box_line(&layout, 83), 1);
}

fn assert_consumer_owner_geometry_retains_following_space() {
    let layout = resolved_source_boundary_layout(
        "XX X XX",
        &[],
        &[
            (87, SourceFixtureEdge::Start, 3, 2.5),
            (88, SourceFixtureEdge::End, 4, 0.0),
        ],
        &[],
        4.0,
    );
    assert_eq!(line_text_ranges(&layout), [0..3, 3..4, 4..7]);
    let line_advances = layout
        .lines()
        .map(|line| line.metrics().advance)
        .collect::<Vec<_>>();
    assert_eq!(line_advances[2], line_advances[0]);
}

#[test]
fn consumer_border_owner_geometry_retains_the_following_collapsed_space() {
    assert_consumer_owner_geometry_retains_following_space();
}

#[test]
fn consumer_padding_owner_geometry_retains_the_following_collapsed_space() {
    assert_consumer_owner_geometry_retains_following_space();
}

#[test]
fn start_only_geometry_with_available_measure_retains_the_following_space() {
    let layout = resolved_source_boundary_layout(
        "XX X XX",
        &[],
        &[
            (126, SourceFixtureEdge::Start, 3, 2.5),
            (127, SourceFixtureEdge::CollapseAfterFilledOwnerEnd, 4, 0.0),
        ],
        &[],
        4.0,
    );

    assert_eq!(line_text_ranges(&layout), [0..3, 3..4, 4..7]);
    let advances = layout
        .lines()
        .map(|line| line.metrics().advance)
        .collect::<Vec<_>>();
    assert_eq!(advances[2], advances[0]);
}

#[test]
fn logical_owner_edges_retain_the_following_space_after_wrap() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut measure = lcx.ranged_builder(&mut fcx, "X", 1.0, false);
    measure.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    measure.push_default(StyleProperty::FontSize(10.0));
    let character_width = measure.build("X").calculate_content_widths().max;

    let mut owner = lcx.ranged_builder(&mut fcx, "XX X XX", 1.0, false);
    owner.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    owner.push_default(StyleProperty::FontSize(10.0));
    owner.push_inline_box(InlineBox::inline_start_edge(
        84,
        3,
        character_width * 2.5,
        0.0,
        InlineBoxBreakAffinity::ToNext,
    ));
    owner.push_inline_box(InlineBox::inline_end_edge(
        85,
        4,
        0.0,
        0.0,
        InlineBoxBreakAffinity::ToPrevious,
    ));
    let mut layout = owner.build("XX X XX");
    layout.break_all_lines(Some(character_width * 4.0));

    assert_eq!(line_text_ranges(&layout), [0..3, 3..4, 4..7]);
}

#[test]
fn logical_owner_end_projection_collapses_the_traversed_unicode_space() {
    let text = "XXXXXXX XXXXXXXXXXXXXXX XXXX XXXXXXX XXXXXXXXXX";
    let control = source_boundary_layout(text, &[], &[], [], 24.0);
    let with_owner = source_boundary_layout(
        text,
        &[],
        &[
            (110, SourceFixtureEdge::Start, 0, 0.0),
            (111, SourceFixtureEdge::CollapsingEnd, 7, 0.0),
            (112, SourceFixtureEdge::Start, 8, 0.0),
            (113, SourceFixtureEdge::CollapsingEnd, 23, 0.0),
            (114, SourceFixtureEdge::Start, 28, 0.0),
            (115, SourceFixtureEdge::CollapsingEnd, 35, 0.0),
        ],
        [],
        24.0,
    );

    assert_eq!(
        with_owner
            .lines()
            .map(|line| text[line.text_range()].trim())
            .collect::<Vec<_>>(),
        control
            .lines()
            .map(|line| text[line.text_range()].trim())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        with_owner
            .lines()
            .map(|line| line.metrics().advance - line.metrics().trailing_whitespace)
            .collect::<Vec<_>>(),
        control
            .lines()
            .map(|line| line.metrics().advance - line.metrics().trailing_whitespace)
            .collect::<Vec<_>>(),
    );
}

#[test]
fn taken_zero_geometry_owner_end_projection_discards_the_space_visual_advance() {
    let control = source_boundary_layout("X X", &[], &[], [], 10.0);
    let mut layout = source_boundary_layout(
        "X X",
        &[],
        &[
            (117, SourceFixtureEdge::Start, 0, 0.0),
            (118, SourceFixtureEdge::CollapsingEnd, 1, 0.0),
            (119, SourceFixtureEdge::Start, 2, 0.0),
            (120, SourceFixtureEdge::End, 3, 0.0),
        ],
        [],
        1.0,
    );

    assert_eq!(layout.len(), 2);
    let final_glyph_offsets = layout
        .lines()
        .map(|line| {
            line.items()
                .filter_map(|item| match item {
                    crate::PositionedLayoutItem::GlyphRun(run) => Some(run.offset()),
                    crate::PositionedLayoutItem::InlineBox(_) => None,
                })
                .last()
                .expect("each line must retain a visible glyph")
        })
        .collect::<Vec<_>>();
    assert_eq!(final_glyph_offsets, [0.0, 0.0]);

    layout.break_all_lines(None);
    assert_eq!(layout.len(), 1);
    assert_eq!(
        layout.lines().next().unwrap().metrics().advance,
        control.lines().next().unwrap().metrics().advance,
    );
}

#[test]
fn untaken_owner_end_projection_keeps_its_unicode_space_advance() {
    let text = "XX X";
    let control = source_boundary_layout(text, &[], &[], [], 10.0);
    let with_owner = source_boundary_layout(
        text,
        &[],
        &[(116, SourceFixtureEdge::End, 2, 0.0)],
        [],
        10.0,
    );

    assert_eq!(with_owner.len(), 1);
    assert_eq!(
        with_owner
            .lines()
            .next()
            .expect("owner line must exist")
            .metrics()
            .advance,
        control
            .lines()
            .next()
            .expect("control line must exist")
            .metrics()
            .advance,
    );
}

#[test]
fn exact_fit_owner_fragment_discards_its_following_space_at_the_next_line_start() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let max_width = source_fixture_text_width(&mut lcx, &mut fcx, "XX XX XX XX");
    let owner_text_width = source_fixture_text_width(&mut lcx, &mut fcx, "XX");

    let text = "XX XX XX XX XX";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_inline_box(InlineBox::inline_start_edge(
        124,
        0,
        max_width - owner_text_width,
        0.0,
        InlineBoxBreakAffinity::ToNext,
    ));
    builder.push_inline_box(InlineBox::inline_end_edge_with_following_source_space(
        125,
        2,
        0.0,
        0.0,
        crate::FollowingSourceSpace::CollapseAfterFilledOwnerFragment,
        InlineBoxBreakAffinity::ToPrevious,
    ));
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(max_width));

    assert_eq!(line_text_ranges(&layout), [0..2, 2..14]);

    layout.break_all_lines(None);
    assert_eq!(layout.len(), 1);
}

#[derive(Clone, Copy)]
enum SourceFixtureEdge {
    Start,
    End,
    CollapsingEnd,
    CollapseAfterFilledOwnerEnd,
    UnicodeOwnedEnd,
}

fn source_fixture_text_width(
    lcx: &mut LayoutContext<ColorBrush>,
    fcx: &mut crate::FontContext,
    text: &str,
) -> f32 {
    let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.build(text).calculate_content_widths().max
}

fn resolved_source_boundary_layout(
    text: &str,
    styles: &[(Range<usize>, TextWrapMode)],
    edges: &[(u64, SourceFixtureEdge, usize, f32)],
    boundaries: &[usize],
    max_width_in_characters: f32,
) -> crate::Layout<ColorBrush> {
    source_boundary_layout(
        text,
        styles,
        edges,
        boundaries
            .iter()
            .copied()
            .map(LineBreakOverride::resolved_collapsed_source_opportunity),
        max_width_in_characters,
    )
}

fn source_boundary_layout(
    text: &str,
    styles: &[(Range<usize>, TextWrapMode)],
    edges: &[(u64, SourceFixtureEdge, usize, f32)],
    overrides: impl IntoIterator<Item = LineBreakOverride>,
    max_width_in_characters: f32,
) -> crate::Layout<ColorBrush> {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let character_width = source_fixture_text_width(&mut lcx, &mut fcx, "X");

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    for (range, mode) in styles {
        builder.push(StyleProperty::TextWrapMode(*mode), range.clone());
    }
    for (id, edge, index, width) in edges {
        let inline_box = match edge {
            SourceFixtureEdge::Start => InlineBox::inline_start_edge(
                *id,
                *index,
                character_width * width,
                0.0,
                InlineBoxBreakAffinity::ToNext,
            ),
            SourceFixtureEdge::End => InlineBox::inline_end_edge(
                *id,
                *index,
                character_width * width,
                0.0,
                InlineBoxBreakAffinity::ToPrevious,
            ),
            SourceFixtureEdge::CollapsingEnd => {
                InlineBox::inline_end_edge_with_following_source_space(
                    *id,
                    *index,
                    character_width * width,
                    0.0,
                    crate::FollowingSourceSpace::CollapsedAfterProjectedBreak,
                    InlineBoxBreakAffinity::ToPrevious,
                )
            }
            SourceFixtureEdge::CollapseAfterFilledOwnerEnd => {
                InlineBox::inline_end_edge_with_following_source_space(
                    *id,
                    *index,
                    character_width * width,
                    0.0,
                    crate::FollowingSourceSpace::CollapseAfterFilledOwnerFragment,
                    InlineBoxBreakAffinity::ToPrevious,
                )
            }
            SourceFixtureEdge::UnicodeOwnedEnd => {
                InlineBox::inline_end_edge_with_following_source_space(
                    *id,
                    *index,
                    character_width * width,
                    0.0,
                    crate::FollowingSourceSpace::UnicodeBoundary,
                    InlineBoxBreakAffinity::ToPrevious,
                )
            }
        };
        builder.push_inline_box(inline_box);
    }
    let mut layout = builder.build(text);
    layout.set_line_break_overrides(overrides.into_iter().collect());
    layout.break_all_lines(Some(character_width * max_width_in_characters));
    layout
}

fn line_text_ranges(layout: &crate::Layout<ColorBrush>) -> Vec<Range<usize>> {
    layout.lines().map(|line| line.text_range()).collect()
}

fn assert_start_geometry_source_projection() {
    let layout = resolved_source_boundary_layout(
        "XX X XX",
        &[],
        &[
            (90, SourceFixtureEdge::Start, 3, 2.5),
            (91, SourceFixtureEdge::End, 4, 0.0),
        ],
        &[3, 5],
        4.0,
    );
    assert_eq!(line_text_ranges(&layout), [0..3, 3..4, 4..7]);
}

#[test]
fn border_start_geometry_retains_the_atomic_source_topology() {
    assert_start_geometry_source_projection();
}

#[test]
fn padding_start_geometry_retains_the_atomic_source_topology() {
    assert_start_geometry_source_projection();
}

fn assert_end_geometry_source_projection() {
    let layout = resolved_source_boundary_layout(
        "XX XX",
        &[],
        &[(92, SourceFixtureEdge::End, 2, 2.5)],
        &[3],
        4.0,
    );
    assert_eq!(line_text_ranges(&layout), [0..2, 2..5]);
}

#[test]
fn margin_end_geometry_retains_the_following_source_boundary() {
    assert_end_geometry_source_projection();
}

#[test]
fn padding_end_geometry_retains_the_following_source_boundary() {
    assert_end_geometry_source_projection();
}

#[test]
fn overflowing_glued_end_edge_reuses_the_preceding_text_opportunity() {
    let text = "XX XX";
    let layout = source_boundary_layout(
        text,
        &[],
        &[(123, SourceFixtureEdge::End, text.len(), 4.0)],
        core::iter::empty(),
        5.0,
    );

    assert_eq!(
        layout
            .lines()
            .map(|line| text[line.text_range()].trim())
            .collect::<Vec<_>>(),
        ["XX", "XX"],
    );
    assert_eq!(inline_box_line(&layout, 123), 1);
}

#[test]
fn decorated_owner_end_leaves_the_following_space_to_unicode() {
    let layout = source_boundary_layout(
        "XX XX",
        &[],
        &[(121, SourceFixtureEdge::UnicodeOwnedEnd, 2, 2.5)],
        [],
        4.0,
    );

    assert_eq!(line_text_ranges(&layout), [0..3, 3..5]);
}

#[test]
fn zero_width_owner_start_keeps_the_latest_fitting_unicode_boundary() {
    let layout = source_boundary_layout(
        "XX XX XX",
        &[],
        &[(122, SourceFixtureEdge::Start, 3, 0.0)],
        [],
        4.5,
    );

    assert_eq!(line_text_ranges(&layout), [0..6, 6..8]);
}

#[test]
fn collapsed_space_opportunity_survives_a_no_wrap_owner_end() {
    let layout = resolved_source_boundary_layout(
        "AA BB",
        &[(0..5, TextWrapMode::NoWrap)],
        &[(93, SourceFixtureEdge::End, 3, 0.0)],
        &[3],
        2.5,
    );
    assert_eq!(line_text_ranges(&layout), [0..3, 3..5]);
}

#[test]
fn repeated_collapsed_opportunities_survive_no_wrap_owner_pairs() {
    let layout = resolved_source_boundary_layout(
        "X X X X",
        &[(0..7, TextWrapMode::NoWrap)],
        &[
            (94, SourceFixtureEdge::End, 2, 0.0),
            (95, SourceFixtureEdge::Start, 2, 0.0),
            (96, SourceFixtureEdge::End, 4, 0.0),
            (97, SourceFixtureEdge::Start, 4, 0.0),
            (98, SourceFixtureEdge::End, 6, 0.0),
            (99, SourceFixtureEdge::Start, 6, 0.0),
        ],
        &[2, 4, 6],
        4.0,
    );
    assert_eq!(layout.len(), 2);
}

#[test]
fn retained_source_space_break_starts_the_following_owner_at_zero() {
    let layout = source_boundary_layout(
        "X X",
        &[],
        &[
            (106, SourceFixtureEdge::Start, 0, 0.0),
            (107, SourceFixtureEdge::End, 1, 0.0),
            (108, SourceFixtureEdge::Start, 2, 0.0),
            (109, SourceFixtureEdge::End, 3, 0.0),
        ],
        [LineBreakOverride::resolved_retained_source_opportunity(2)],
        1.0,
    );

    assert_eq!(line_text_ranges(&layout), [0..2, 2..3]);
    let glyph_offsets = layout
        .lines()
        .filter_map(|line| {
            line.items().find_map(|item| match item {
                crate::PositionedLayoutItem::GlyphRun(run) => Some(run.offset()),
                crate::PositionedLayoutItem::InlineBox(_) => None,
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(glyph_offsets, [0.0, 0.0]);
}

#[test]
fn retained_boundary_after_an_exact_fit_nonwrapping_participant_wins() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let prefix = "X X X X X X X X X X";
    let mut measure = lcx.ranged_builder(&mut fcx, prefix, 1.0, false);
    measure.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    measure.push_default(StyleProperty::FontSize(10.0));
    let max_width = measure.build(prefix).calculate_content_widths().max;

    let text = "X X X X X X X X X X 123";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
    for start in (0..18).step_by(2) {
        builder.push(
            StyleProperty::TextWrapMode(TextWrapMode::Wrap),
            start..start + 2,
        );
    }
    builder.push(StyleProperty::TextWrapMode(TextWrapMode::NoWrap), 18..19);
    builder.push(
        StyleProperty::TextWrapMode(TextWrapMode::Wrap),
        19..text.len(),
    );
    let mut layout = builder.build(text);
    layout.set_line_break_overrides(vec![
        LineBreakOverride::resolved_retained_source_opportunity(18),
        LineBreakOverride::resolved_retained_source_opportunity(20),
    ]);
    layout.set_normal_soft_wrap_selection(NormalSoftWrapSelection::GreedyLatest);
    layout.break_all_lines(Some(max_width));

    assert_eq!(line_text_ranges(&layout), [0..20, 20..23]);
}

#[test]
fn overflowing_nonwrapping_space_after_wrapping_text_remains_glue() {
    let layout = source_boundary_layout(
        "XX XX",
        &[
            (0..2, TextWrapMode::Wrap),
            (2..3, TextWrapMode::NoWrap),
            (3..5, TextWrapMode::Wrap),
        ],
        &[],
        [],
        2.0,
    );

    assert_eq!(layout.len(), 1);
}

#[test]
fn first_line_style_topology_retains_an_end_source_boundary() {
    let layout = resolved_source_boundary_layout(
        "XX XX",
        &[(0..3, TextWrapMode::Wrap), (3..5, TextWrapMode::NoWrap)],
        &[
            (100, SourceFixtureEdge::End, 2, 1.5),
            (101, SourceFixtureEdge::Start, 3, 0.0),
        ],
        &[3],
        3.0,
    );
    assert_eq!(line_text_ranges(&layout), [0..2, 2..5]);
}

#[test]
fn mixed_white_space_styles_retain_resolved_source_boundaries() {
    let layout = resolved_source_boundary_layout(
        "X X X",
        &[
            (0..2, TextWrapMode::NoWrap),
            (2..4, TextWrapMode::NoWrap),
            (4..5, TextWrapMode::NoWrap),
        ],
        &[
            (102, SourceFixtureEdge::End, 2, 0.0),
            (103, SourceFixtureEdge::Start, 2, 0.0),
            (104, SourceFixtureEdge::End, 4, 0.0),
            (105, SourceFixtureEdge::Start, 4, 0.0),
        ],
        &[2, 4],
        1.5,
    );
    assert_eq!(layout.len(), 3);
}

fn collapsed_no_wrap_owner_layout(boundaries: &[usize]) -> crate::Layout<ColorBrush> {
    let text = "X X X X X X X X X X";
    let styles = (0..10)
        .map(|index| {
            let start = index * 2;
            (start..(start + 2).min(text.len()), TextWrapMode::NoWrap)
        })
        .collect::<Vec<_>>();
    let edges = (0..10)
        .flat_map(|index| {
            let start = index * 2;
            let end = (start + 2).min(text.len());
            [
                (200 + index as u64 * 2, SourceFixtureEdge::Start, start, 0.0),
                (201 + index as u64 * 2, SourceFixtureEdge::End, end, 0.0),
            ]
        })
        .collect::<Vec<_>>();
    resolved_source_boundary_layout(text, &styles, &edges, boundaries, 10.0)
}

#[test]
fn consumer_resolved_parent_spaces_wrap_between_no_wrap_owners() {
    let layout = collapsed_no_wrap_owner_layout(&[2, 4, 6, 8, 10, 12, 14, 16, 18]);
    assert_eq!(layout.len(), 2);
}

#[test]
fn no_wrap_owned_spaces_without_a_source_handoff_remain_unbreakable() {
    let layout = collapsed_no_wrap_owner_layout(&[]);
    assert_eq!(layout.len(), 1);
}

#[test]
fn transparent_anchor_preserves_a_mandatory_break() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "XXX\nXXX";
    let mut layout = build_inline_box_layout(
        &mut lcx,
        &mut fcx,
        text,
        10.0,
        None,
        [InlineBox::transparent_anchor(75, 4)],
    );

    layout.break_all_lines(None);

    assert_eq!(layout.len(), 2);
    let anchor_line = layout
        .lines()
        .enumerate()
        .find_map(|(line_index, line)| {
            line.items().find_map(|item| match item {
                crate::PositionedLayoutItem::InlineBox(inline_box) if inline_box.id == 75 => {
                    Some(line_index)
                }
                _ => None,
            })
        })
        .expect("the transparent anchor must remain positioned");
    assert_eq!(anchor_line, 1);
}

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
        builder.push_inline_box(InlineBox::new(id, index, 199.0, 8.0));
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

#[test]
fn spaces_after_overwide_inline_boxes_hang_instead_of_forming_lines() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    // Content-box sizing can make a replaced inline's margin box slightly
    // wider than the line. The overflow does not change CSS Text's
    // whitespace rule: the following collapsible space still hangs from
    // the box's line and must not become a line by itself.
    let text = "  ";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    for (id, index) in [(0_u64, 0_usize), (1, 1), (2, 2)] {
        builder.push_inline_box(InlineBox::new(id, index, 201.0, 8.0));
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(200.0));

    assert_eq!(
        layout.len(),
        3,
        "three overwide boxes with collapsible spaces between them \
         must produce exactly three overflowing lines; lines: {:?}",
        layout
            .lines()
            .map(|line| (line.text_range(), line.metrics().advance))
            .collect::<Vec<_>>(),
    );
}

/// PDFreactor-parity trailing-space reclaim: `[text][space][box]` where
/// text + space + box overflows but text + box fits. Browsers wrap the
/// box; with `set_reclaim_space_before_inline_box(true)` the space's
/// advance is removed (exactly as a wrap would remove it) and the box
/// stays on the text's line.
#[test]
fn reclaimed_trailing_space_keeps_the_following_box_on_the_line() {
    let mut fcx = create_font_context();

    // "word " ~= 5 glyphs; box 170px; max 200px. The text alone is well
    // under 30px at 10px Roboto, text + space + 170px box overflows only
    // by less than the space's advance.
    let text = "word ";
    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.push_inline_box(InlineBox::new(0, text.len(), 170.0, 8.0));
        builder.build(text)
    };

    // Compute the tight max_advance from a text-only probe so the
    // assertion is metric-independent: text+box fits, text+space+box
    // does not.
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut probe_builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    probe_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    probe_builder.push_default(StyleProperty::FontSize(10.0));
    let mut probe = probe_builder.build(text);
    probe.break_all_lines(None);
    let metrics = probe.lines().next().unwrap().metrics().clone();
    let word = metrics.advance - metrics.trailing_whitespace;
    let space = metrics.trailing_whitespace;
    assert!(space > 0.0, "probe geometry: the text must end in a space");
    let max_advance = word + 170.0 + space * 0.5;

    let mut wrapping = build(&mut lcx, &mut fcx);
    wrapping.break_all_lines(Some(max_advance));
    assert_eq!(
        wrapping.len(),
        2,
        "without the reclaim flag the box wraps to its own line"
    );

    let mut reclaiming = build(&mut lcx, &mut fcx);
    reclaiming.set_reclaim_space_before_inline_box(true);
    reclaiming.break_all_lines(Some(max_advance));
    assert_eq!(
        reclaiming.len(),
        1,
        "with the reclaim flag the space is removed and the box stays; lines: {:?}",
        reclaiming
            .lines()
            .map(|line| (line.text_range(), line.metrics().advance))
            .collect::<Vec<_>>(),
    );
}

/// CSS Text permits UA-defined priority classes for punctuation and word
/// separators. Parley's normal composition policy selects an authored
/// punctuation opportunity ahead of a following separator that only fails
/// because its collapsible advance crosses the measure.
fn first_line_with_overflowing_trailing_space(
    text: &str,
    configure: impl Fn(&mut RangedBuilder<'_, ColorBrush>),
) -> String {
    first_line_with_overflowing_trailing_space_and_selection(
        text,
        configure,
        NormalSoftWrapSelection::PriorityClasses,
    )
}

fn first_line_with_overflowing_trailing_space_and_selection(
    text: &str,
    configure: impl Fn(&mut RangedBuilder<'_, ColorBrush>),
    selection: NormalSoftWrapSelection,
) -> String {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        configure(&mut builder);
        builder.build(text)
    };

    let mut probe = build(&mut lcx, &mut fcx);
    probe.break_all_lines(None);
    let metrics = probe.lines().next().unwrap().metrics().clone();
    let word_advance = metrics.advance - metrics.trailing_whitespace;
    let max_advance = word_advance + metrics.trailing_whitespace * 0.5;

    let mut layout = build(&mut lcx, &mut fcx);
    layout.set_normal_soft_wrap_selection(selection);
    layout.break_all_lines(Some(max_advance));
    let first_line = layout
        .lines()
        .next()
        .map(|line| String::from(&text[line.text_range()]))
        .unwrap();
    first_line
}

#[test]
fn greedy_latest_keeps_a_complete_fitting_word_when_only_its_space_overflows() {
    let text = "alpha-beta ";
    assert_eq!(
        first_line_with_overflowing_trailing_space_and_selection(
            text,
            |_| {},
            NormalSoftWrapSelection::GreedyLatest,
        ),
        text,
        "greedy normal composition must keep the later fitting word separator"
    );
}

#[test]
fn greedy_latest_still_uses_the_authored_dash_on_real_content_overflow() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha-beta ";

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.build(text)
    };

    let mut prefix_builder = lcx.ranged_builder(&mut fcx, "alpha-", 1.0, false);
    prefix_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    prefix_builder.push_default(StyleProperty::FontSize(10.0));
    let mut prefix = prefix_builder.build("alpha-");
    prefix.break_all_lines(None);
    let prefix_advance = prefix.lines().next().unwrap().metrics().advance;

    let mut layout = build(&mut lcx, &mut fcx);
    layout.set_normal_soft_wrap_selection(NormalSoftWrapSelection::GreedyLatest);
    layout.break_all_lines(Some(prefix_advance + 0.01));

    assert_eq!(
        layout.lines().next().map(|line| &text[line.text_range()]),
        Some("alpha-"),
        "greedy selection changes only trailing-space overflow, not real content overflow"
    );
}

fn first_line_with_overflowing_space_after_punctuation(punctuation: char) -> String {
    let text = format!("alpha{punctuation}beta ");
    first_line_with_overflowing_trailing_space(&text, |_| {})
}

#[test]
fn authored_hyphen_minus_wins_when_the_following_collapsible_space_overflows() {
    assert_eq!(
        first_line_with_overflowing_space_after_punctuation('-'),
        "alpha-",
        "authored U+002D punctuation must remain distinct from the overflowing word separator"
    );
}

#[test]
fn authored_unicode_hyphen_wins_when_the_following_collapsible_space_overflows() {
    assert_eq!(
        first_line_with_overflowing_space_after_punctuation('\u{2010}'),
        "alpha\u{2010}",
        "authored U+2010 punctuation must remain distinct from the overflowing word separator"
    );
}

#[test]
fn authored_dash_provenance_crosses_a_pure_style_boundary() {
    let text = "alpha-beta ";
    assert_eq!(
        first_line_with_overflowing_trailing_space(text, |builder| {
            builder.push(
                StyleProperty::FontWeight(FontWeight::BOLD),
                0.."alpha-".len(),
            );
        }),
        "alpha-",
        "a style-run boundary must not erase the authored unit that created the boundary"
    );
}

#[test]
fn atomic_inline_box_ends_authored_dash_provenance() {
    let text = "alpha-beta ";
    assert_eq!(
        first_line_with_overflowing_trailing_space(text, |builder| {
            builder.push_inline_box(InlineBox::atomic_with_break_affinity(
                41,
                "alpha-".len(),
                1.0,
                8.0,
                InlineBoxBreakAffinity::Both,
            ));
        }),
        text,
        "an atomic inline between the dash and boundary must make that candidate ordinary"
    );
}

#[test]
fn word_break_break_all_opportunities_are_not_prioritized() {
    let text = "alpha-beta ";
    assert_eq!(
        first_line_with_overflowing_trailing_space(text, |builder| {
            builder.push_default(StyleProperty::WordBreak(WordBreak::BreakAll));
        }),
        text,
        "word-break: break-all does not use word-separator-based priority classes"
    );
}

#[test]
fn overflow_wrap_anywhere_remains_an_emergency_policy() {
    let text = "alpha-beta ";
    assert_eq!(
        first_line_with_overflowing_trailing_space(text, |builder| {
            builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        }),
        "alpha-",
        "overflow-wrap: anywhere must not erase normal authored-dash priority"
    );
}

/// An ordinary earlier inter-word boundary is not eligible for punctuation
/// preference when a later collapsible space overflows.
#[test]
fn ordinary_inter_word_boundary_does_not_displace_a_fitting_word() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha beta ";

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.build(text)
    };

    let mut probe = build(&mut lcx, &mut fcx);
    probe.break_all_lines(None);
    let metrics = probe.lines().next().unwrap().metrics().clone();
    let word_advance = metrics.advance - metrics.trailing_whitespace;
    let max_advance = word_advance + metrics.trailing_whitespace * 0.5;

    let mut layout = build(&mut lcx, &mut fcx);
    layout.break_all_lines(Some(max_advance));

    assert_eq!(
        layout.lines().next().map(|line| &text[line.text_range()]),
        Some(text),
        "an ordinary inter-word candidate cannot be selected by punctuation preference"
    );
}

/// Inserted conditional material remains distinct from authored punctuation.
/// It is considered when the word itself overflows, not merely because the
/// following collapsible space does.
#[test]
fn inserted_discretionary_hyphen_does_not_displace_a_fitting_word() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha\u{00AD}beta ";

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let metrics = layout.lines().next().unwrap().metrics().clone();
    let word_advance = metrics.advance - metrics.trailing_whitespace;
    let max_advance = word_advance + metrics.trailing_whitespace * 0.5;

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let mut layout = builder.build(text);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "alpha\u{00AD}".len(),
        advance: 4.0,
        max_consecutive_lines: None,
    }]);
    layout.break_all_lines(Some(max_advance));

    assert_eq!(
        layout.lines().next().map(|line| &text[line.text_range()]),
        Some(text),
        "the complete fitting word must remain intact while its trailing space hangs"
    );
}

/// CSS Text's `line-break: anywhere` opportunities are not prioritized.
#[test]
fn anywhere_opportunities_are_not_prioritized() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha-beta ";

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.push_default(StyleProperty::LineBreakMode(LineBreakMode::Anywhere));
        builder.push(
            StyleProperty::FontWeight(FontWeight::BOLD),
            0.."alpha-".len(),
        );
        builder.build(text)
    };

    let mut probe = build(&mut lcx, &mut fcx);
    probe.break_all_lines(None);
    let metrics = probe.lines().next().unwrap().metrics().clone();
    let word_advance = metrics.advance - metrics.trailing_whitespace;
    let max_advance = word_advance + metrics.trailing_whitespace * 0.5;

    let mut layout = build(&mut lcx, &mut fcx);
    layout.break_all_lines(Some(max_advance));

    assert_eq!(
        layout.lines().next().map(|line| &text[line.text_range()]),
        Some(text),
        "line-break: anywhere opportunities must not be prioritized"
    );
}

#[test]
fn anywhere_chooses_the_latest_fitting_boundary_before_a_preserved_space() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "X XX X";
    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext, text| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(25.0));
        builder.push_default(StyleProperty::LineBreakMode(LineBreakMode::Anywhere));
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::BreakSpaces,
        ));
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::Wrap));
        builder.build(text)
    };

    let mut probe = build(&mut lcx, &mut fcx, &text[..4]);
    probe.break_all_lines(None);
    let max_advance = probe.lines().next().unwrap().metrics().advance;
    let mut layout = build(&mut lcx, &mut fcx, text);
    layout.set_line_break_overrides(vec![
        LineBreakOverride::opportunity(2),
        LineBreakOverride::opportunity(5),
    ]);
    layout.break_all_lines(Some(max_advance));

    assert_eq!(
        layout
            .lines()
            .map(|line| line.text_range())
            .collect::<Vec<_>>(),
        [0..4, 4..6],
    );
}

/// Caller-created equal-priority opportunities carry their provenance through
/// the public override API instead of being inferred from a boolean override.
#[test]
fn explicitly_unprioritized_overrides_are_not_prioritized() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha-beta ";

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        let mut layout = builder.build(text);
        layout.set_line_break_overrides(
            text.char_indices()
                .skip(1)
                .map(|(byte_index, _)| LineBreakOverride::unprioritized_opportunity(byte_index))
                .collect(),
        );
        layout
    };

    let mut probe = build(&mut lcx, &mut fcx);
    probe.break_all_lines(None);
    let metrics = probe.lines().next().unwrap().metrics().clone();
    let word_advance = metrics.advance - metrics.trailing_whitespace;
    let max_advance = word_advance + metrics.trailing_whitespace * 0.5;

    let mut layout = build(&mut lcx, &mut fcx);
    layout.break_all_lines(Some(max_advance));

    assert_eq!(
        layout.lines().next().map(|line| &text[line.text_range()]),
        Some(text),
        "explicitly unprioritized opportunities must not be promoted by authored punctuation"
    );
}

/// Exact cached visual regression from
/// `writing-modes/writing-mode/nested-writing-modes`: 12pt Arimo in a
/// 52.5pt measure previously ended the first affected line at `Vertical-`.
#[test]
fn nested_writing_modes_cached_measure_breaks_after_authored_hyphen() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "Vertical-lr inside horizontal inside vertical-rl.";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Arimo")));
    builder.push_default(StyleProperty::FontSize(12.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(52.5));

    let first = layout
        .lines()
        .next()
        .map(|line| text[line.text_range()].trim_end());
    assert_eq!(
        first,
        Some("Vertical-"),
        "the cached 52.5pt scenario must retain the authored-punctuation line ending"
    );
}

#[test]
fn discretionary_material_participates_in_line_fit_and_metrics_only_when_taken() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha\u{00AD}beta";

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.build(text)
    };

    let mut prefix_builder = lcx.ranged_builder(&mut fcx, "alpha", 1.0, false);
    prefix_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    prefix_builder.push_default(StyleProperty::FontSize(10.0));
    let mut prefix = prefix_builder.build("alpha");
    prefix.break_all_lines(None);
    let prefix_advance = prefix.lines().next().unwrap().metrics().advance;

    let inserted_advance = 4.0;
    let mut layout = build(&mut lcx, &mut fcx);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "alpha\u{00AD}".len(),
        advance: inserted_advance,
        max_consecutive_lines: None,
    }]);
    layout.break_all_lines(Some(prefix_advance + inserted_advance + 0.25));

    let first = layout.lines().next().expect("the prefix must form a line");
    assert_eq!(&text[first.text_range()], "alpha\u{00AD}");
    assert!((first.discretionary_advance() - inserted_advance).abs() < 0.001);
    assert!(
        (first.metrics().advance - (prefix_advance + inserted_advance)).abs() < 0.01,
        "the selected line must reserve the visible discretionary material: {:?}",
        first.metrics(),
    );

    let mut unbroken = build(&mut lcx, &mut fcx);
    unbroken.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "alpha\u{00AD}".len(),
        advance: inserted_advance,
        max_consecutive_lines: None,
    }]);
    unbroken.break_all_lines(None);
    assert_eq!(unbroken.len(), 1);
    assert_eq!(
        unbroken.lines().next().unwrap().discretionary_advance(),
        0.0
    );

    let mut too_narrow = build(&mut lcx, &mut fcx);
    too_narrow.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "alpha\u{00AD}".len(),
        advance: inserted_advance,
        max_consecutive_lines: None,
    }]);
    too_narrow.break_all_lines(Some(prefix_advance + inserted_advance - 0.25));
    assert_eq!(
        too_narrow.len(),
        1,
        "a discretionary boundary whose inserted material does not fit is not a valid line ending"
    );
    assert_eq!(
        too_narrow.lines().next().unwrap().discretionary_advance(),
        0.0
    );
}

#[test]
fn discretionary_break_respects_consecutive_line_limit() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "aa\u{00AD}aa\u{00AD}aa\u{00AD}aa";

    let mut segment_builder = lcx.ranged_builder(&mut fcx, "aa", 1.0, false);
    segment_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    segment_builder.push_default(StyleProperty::FontSize(10.0));
    let mut segment = segment_builder.build("aa");
    segment.break_all_lines(None);
    let measure = segment.lines().next().unwrap().metrics().advance + 2.0;

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let mut layout = builder.build(text);
    layout.set_discretionary_breaks(
        text.match_indices('\u{00AD}')
            .map(|(index, _)| DiscretionaryBreak {
                byte_index: index + '\u{00AD}'.len_utf8(),
                advance: 2.0,
                max_consecutive_lines: Some(1),
            })
            .collect(),
    );
    layout.break_all_lines(Some(measure));

    assert_eq!(
        layout
            .lines()
            .filter(|line| line.discretionary_advance() > 0.0)
            .count(),
        1,
        "the configured cap must suppress a second consecutive discretionary line ending",
    );
}

#[test]
fn zero_advance_discretionary_break_reports_selected_boundary() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "aa\u{00AD}aa";

    let mut segment_builder = lcx.ranged_builder(&mut fcx, "aa", 1.0, false);
    segment_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    segment_builder.push_default(StyleProperty::FontSize(10.0));
    let mut segment = segment_builder.build("aa");
    segment.break_all_lines(None);
    let measure = segment.lines().next().unwrap().metrics().advance + 0.25;

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let mut layout = builder.build(text);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "aa\u{00AD}".len(),
        advance: 0.0,
        max_consecutive_lines: None,
    }]);
    layout.break_all_lines(Some(measure));

    let first = layout.lines().next().expect("the prefix must form a line");
    assert_eq!(&text[first.text_range()], "aa\u{00AD}");
    assert!(first.ends_at_discretionary_break());
    assert_eq!(first.discretionary_advance(), 0.0);
}

/// A glued inline box (a border/padding shim) binds to the adjacent
/// text: min-content measurement sums the shim widths into the text's
/// unbreakable run, and the breaker never wraps between a shim and its
/// glyphs. A replaced (non-glued) box keeps its wrap opportunities.
#[test]
fn glued_boxes_bind_to_text_in_measurement_and_breaking() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    // "11" bracketed by an 8px and a 6.75px padding shim (the W8BEN
    // item-11 cell). The unit's min-content is the SUM of all three.
    let text = "11";
    let build = |lcx: &mut LayoutContext<ColorBrush>,
                 fcx: &mut crate::FontContext,
                 break_affinity: InlineBoxBreakAffinity| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        let leading = InlineBox::atomic_with_break_affinity(0, 0, 8.0, 0.0, break_affinity);
        let trailing =
            InlineBox::atomic_with_break_affinity(1, text.len(), 6.75, 0.0, break_affinity);
        builder.push_inline_box(leading);
        builder.push_inline_box(trailing);
        builder.build(text)
    };

    let glued = build(&mut lcx, &mut fcx, InlineBoxBreakAffinity::Both);
    let widths = glued.calculate_content_widths();
    let mut text_only_builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    text_only_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    text_only_builder.push_default(StyleProperty::FontSize(10.0));
    let text_only = text_only_builder.build(text).calculate_content_widths();
    assert!(
        (widths.min - (text_only.min + 8.0 + 6.75)).abs() < 0.01,
        "glued min-content must sum the shims into the text run: got {} \
         for text min {}",
        widths.min,
        text_only.min,
    );

    // Break at a width below the unit: the glued unit must stay on one
    // line (overflowing), never splitting between shim and glyphs.
    let mut layout = build(&mut lcx, &mut fcx, InlineBoxBreakAffinity::Both);
    layout.break_all_lines(Some(widths.min - 2.0));
    assert_eq!(
        layout.len(),
        1,
        "a glued unit narrower than its column overflows on ONE line; \
         lines: {:?}",
        layout
            .lines()
            .map(|line| (line.text_range(), line.metrics().advance))
            .collect::<Vec<_>>(),
    );

    // The same shapes as REPLACED boxes keep their wrap opportunities.
    let unglued_widths =
        build(&mut lcx, &mut fcx, InlineBoxBreakAffinity::Independent).calculate_content_widths();
    assert!(
        unglued_widths.min < widths.min,
        "replaced boxes keep per-box wrap opportunities: {} vs glued {}",
        unglued_widths.min,
        widths.min,
    );
}

fn directional_affinity_layout(
    index: usize,
    break_affinity: InlineBoxBreakAffinity,
) -> (Vec<String>, usize) {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "xx x xx";

    let mut probe_builder = lcx.ranged_builder(&mut fcx, "x", 1.0, false);
    probe_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    probe_builder.push_default(StyleProperty::FontSize(10.0));
    let mut probe = probe_builder.build("x");
    probe.break_all_lines(None);
    let owner_width = probe.lines().next().unwrap().metrics().advance;

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let edge =
        InlineBox::atomic_with_break_affinity(31, index, owner_width * 2.5, 0.0, break_affinity);
    builder.push_inline_box(edge);
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(owner_width * 3.5));

    let lines = layout
        .lines()
        .map(|line| String::from(text[line.text_range()].trim()))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let edge_line = layout
        .lines()
        .enumerate()
        .find_map(|(line_index, line)| {
            line.items().find_map(|item| match item {
                crate::PositionedLayoutItem::InlineBox(inline_box) if inline_box.id == 31 => {
                    Some(line_index)
                }
                _ => None,
            })
        })
        .expect("the directional edge must be positioned");
    (lines, edge_line)
}

#[test]
fn affinity_to_next_moves_a_leading_edge_with_its_text() {
    let (lines, edge_line) = directional_affinity_layout(3, InlineBoxBreakAffinity::ToNext);

    assert_eq!(lines, ["xx", "x", "xx"]);
    assert_eq!(edge_line, 1);
}

#[test]
fn affinity_to_previous_keeps_a_trailing_edge_with_its_text() {
    let (lines, edge_line) = directional_affinity_layout(4, InlineBoxBreakAffinity::ToPrevious);

    assert_eq!(lines, ["xx x", "xx"]);
    assert_eq!(edge_line, 0);
}

#[test]
fn paired_edge_affinities_preserve_the_following_space_after_wrap() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "xx x xx";

    let mut probe_builder = lcx.ranged_builder(&mut fcx, "x", 1.0, false);
    probe_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    probe_builder.push_default(StyleProperty::FontSize(10.0));
    let mut probe = probe_builder.build("x");
    probe.break_all_lines(None);
    let owner_width = probe.lines().next().unwrap().metrics().advance;

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let start = InlineBox::atomic_with_break_affinity(
        31,
        3,
        owner_width * 2.5,
        0.0,
        InlineBoxBreakAffinity::ToNext,
    );
    let end =
        InlineBox::atomic_with_break_affinity(32, 4, 0.0, 0.0, InlineBoxBreakAffinity::ToPrevious);
    builder.push_inline_box(start);
    builder.push_inline_box(end);
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(owner_width * 3.75));

    assert_eq!(
        layout
            .lines()
            .map(|line| String::from(&text[line.text_range()]))
            .collect::<Vec<_>>(),
        ["xx ", "x", " xx"],
    );
}
