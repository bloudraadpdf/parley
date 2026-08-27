// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Line box metrics under the CSS 2.1 §10.8 per-contributor model.
//!
//! With `quantize = false` (the print path), each run extends the line box
//! by its own half-leading around the shared baseline: the line box is
//! `max(above) + max(below)`, NOT `max(line_height)`. A run pairing a large
//! font with a small line-height contributes a tall above-extent and a
//! negative half-leading below, while a small run with generous line-height
//! contributes the deepest below-extent — the line box must span both.

use super::test_builders::create_font_context;
use crate::{
    FontFamily, InlineBox, InlineBoxBreakAffinity, Layout, LayoutContext, LineHeight, StyleProperty,
};

use super::utils::ColorBrush;

fn negative_leading_layout(
    inline_boxes: impl IntoIterator<Item = InlineBox>,
) -> Layout<ColorBrush> {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    let text = "negative leading";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    for property in [
        StyleProperty::FontFamily(FontFamily::named("Roboto")),
        StyleProperty::FontSize(30.0),
        StyleProperty::LineHeight(LineHeight::Absolute(10.0)),
    ] {
        builder.push_default(property);
    }
    for inline_box in inline_boxes {
        builder.push_inline_box(inline_box);
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

fn run_extents(metrics: &crate::RunMetrics) -> (f32, f32) {
    let half_leading = (metrics.line_height - metrics.ascent - metrics.descent) * 0.5;
    (
        metrics.ascent + half_leading,
        metrics.descent + half_leading,
    )
}

#[test]
fn unquantized_line_box_spans_per_run_half_leading_extents() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    let text = "small BIG";
    let boundary = text.find(' ').unwrap();
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    // Run A: 10px font in a 20px line box -> deep below-extent
    //   (descent + positive half-leading).
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::LineHeight(LineHeight::Absolute(20.0)));
    // Run B: 20px font in the same 20px line box -> tall above-extent
    //   (ascent + negative half-leading).
    builder.push(StyleProperty::FontSize(20.0), boundary + 1..text.len());
    let mut layout = builder.build(text);
    layout.break_all_lines(None);

    assert_eq!(layout.len(), 1, "the probe text must stay on one line");
    let line = layout.lines().next().unwrap();

    // Expected extents derived from the runs' own resolved metrics, so the
    // assertion pins the MODEL rather than any particular metric table.
    let mut expected_above = 0.0f32;
    let mut expected_below = 0.0f32;
    for run in line.runs() {
        let m = run.metrics();
        let half_leading = (m.line_height - (m.ascent + m.descent)) * 0.5;
        expected_above = expected_above.max(m.ascent + half_leading);
        expected_below = expected_below.max(m.descent + half_leading);
    }

    let metrics = line.metrics();
    assert!(
        metrics.line_height > 20.5,
        "the mixed line must exceed the shared 20px line-height (the old \
         max-of-line-heights model collapses it to exactly 20), got {}",
        metrics.line_height,
    );
    assert!(
        (metrics.line_height - (expected_above + expected_below)).abs() < 0.01,
        "line box must be max(above) + max(below) = {}, got {}",
        expected_above + expected_below,
        metrics.line_height,
    );
    assert!(
        (metrics.baseline - expected_above).abs() < 0.01,
        "baseline must sit at the max above-extent {}, got {}",
        expected_above,
        metrics.baseline,
    );
}

#[test]
fn unquantized_uniform_line_box_still_equals_the_line_height() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    let text = "uniform";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::LineHeight(LineHeight::Absolute(20.0)));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);

    let line = layout.lines().next().unwrap();
    assert!(
        (line.metrics().line_height - 20.0).abs() < 0.01,
        "single-style lines are unchanged by the per-contributor model, got {}",
        line.metrics().line_height,
    );
}

#[test]
fn large_grapheme_cluster_preserves_the_full_line_text_range() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = core::iter::once('e')
        .chain(core::iter::repeat_n('\u{0301}', 35_000))
        .chain(core::iter::once('X'))
        .collect::<alloc::string::String>();
    let mut builder = lcx.ranged_builder(&mut fcx, &text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    let mut layout = builder.build(&text);
    layout.break_all_lines(None);

    assert_eq!(layout.lines().next().unwrap().text_range(), 0..text.len());
}

#[test]
fn unquantized_uniform_negative_leading_keeps_the_authored_line_height() {
    let layout = negative_leading_layout([]);

    let line = layout.lines().next().unwrap();
    let run = line.runs().next().unwrap();
    let (expected_above, expected_below) = run_extents(run.metrics());

    assert!(
        expected_below < 0.0,
        "the fixture must have negative under-leading"
    );
    assert!((line.metrics().baseline - expected_above).abs() < 0.01);
    assert!((line.metrics().line_height - 10.0).abs() < 0.01);
}

#[test]
fn zero_height_boundaries_keep_negative_line_extents() {
    let text = "negative leading";
    let cases = [
        alloc::vec![
            InlineBox::inline_start_edge(1, 0, 0.0, 0.0, InlineBoxBreakAffinity::ToNext),
            InlineBox::inline_end_edge(2, text.len(), 0.0, 0.0, InlineBoxBreakAffinity::ToPrevious,),
        ],
        alloc::vec![InlineBox::transparent_anchor(3, 0)],
    ];
    for inline_boxes in cases {
        let layout = negative_leading_layout(inline_boxes);
        let line = layout.lines().next().unwrap();
        assert!((line.metrics().line_height - 10.0).abs() < 0.01);
    }
}

#[test]
fn zero_height_atomic_inline_contributes_its_baseline_edge() {
    let layout = negative_leading_layout([InlineBox::new(1, 0, 0.0, 0.0)]);
    let line = layout.lines().next().unwrap();
    let run = line.runs().next().unwrap();
    let (expected_above, _) = run_extents(run.metrics());
    assert!((line.metrics().line_height - expected_above).abs() < 0.01);
}

#[test]
fn runs_keep_their_own_style_index_across_boundaries() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    let text = "small BIG";
    let boundary = text.find(' ').unwrap();
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(2.0)));
    builder.push(StyleProperty::FontSize(20.0), boundary + 1..text.len());
    builder.push(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.0)), boundary + 1..text.len());
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let line = layout.lines().next().unwrap();
    let lhs: alloc::vec::Vec<(f32, f32)> = line.runs().map(|r| (r.font_size(), r.metrics().line_height)).collect();
    assert_eq!(lhs.len(), 2);
    assert!((lhs[0].1 - 20.0).abs() < 0.01, "run A (10px, 2.0x) line-height should be 20, got {:?}", lhs);
    assert!((lhs[1].1 - 20.0).abs() < 0.01, "run B (20px, 1.0x) line-height should be 20, got {:?}", lhs);
}


#[test]
fn default_font_size_relative_line_height_resolves() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "plain";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(2.0)));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let line = layout.lines().next().unwrap();
    let run = line.runs().next().unwrap();
    assert!((run.metrics().line_height - 20.0).abs() < 0.01,
        "default-only FontSizeRelative(2.0) at 10px should give 20, got {}", run.metrics().line_height);
}

#[test]
fn default_ignorable_bidi_controls_do_not_enlarge_the_line_box() {
    fn line_metrics(
        text: &str,
        enlarged_range: core::ops::Range<usize>,
    ) -> crate::layout::LineMetrics {
        let mut fcx = create_font_context();
        let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
        let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(20.0));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.0)));
        if !enlarged_range.is_empty() {
            builder.push(StyleProperty::FontSize(100.0), enlarged_range.clone());
            builder.push(
                StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.0)),
                enlarged_range,
            );
        }
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        let metrics = *layout
            .lines()
            .next()
            .expect("the probe has one line")
            .metrics();
        metrics
    }

    let plain = line_metrics("xx", 0..0);
    let controls = line_metrics("x\u{202e}\u{202c}x", 1..7);
    assert_eq!(controls.line_height, plain.line_height);
    assert_eq!(controls.baseline, plain.baseline);

    let visible_fallback = line_metrics("x\u{05d0}x", 1..3);
    assert!(visible_fallback.line_height > plain.line_height);
}
