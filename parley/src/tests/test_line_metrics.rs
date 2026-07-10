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
use crate::{FontFamily, LayoutContext, LineHeight, StyleProperty};

use super::utils::ColorBrush;

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
