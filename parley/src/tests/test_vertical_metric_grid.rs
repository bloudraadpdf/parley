// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rounding of the vertical font metrics of each run to a consumer's grid.

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    FontFamily, Layout, LayoutContext, LineHeight, RunMetrics, StyleProperty, VerticalMetricGrid,
};

const CSS_PIXEL: f32 = 0.75;

fn layout(grid: Option<VerticalMetricGrid>) -> Layout<ColorBrush> {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    lcx.set_vertical_metric_grid(grid);
    let text = "Ag";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::LineHeight(LineHeight::MetricsRelative(1.0)));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

fn run_metrics(layout: &Layout<ColorBrush>) -> RunMetrics {
    layout.data.runs[0].metrics
}

#[test]
fn each_vertical_font_metric_of_a_run_rounds_to_the_grid() {
    let exact = run_metrics(&layout(None));
    let on_grid = |value: f32| (value / CSS_PIXEL - 0.5).ceil() * CSS_PIXEL;
    let (ascent, descent, leading) = (
        on_grid(exact.ascent),
        on_grid(exact.descent),
        on_grid(exact.leading),
    );
    assert_ne!(exact.ascent, ascent);

    let rounded = layout(VerticalMetricGrid::new(CSS_PIXEL));
    let metrics = run_metrics(&rounded);
    assert_eq!(
        (metrics.ascent, metrics.descent, metrics.leading),
        (ascent, descent, leading)
    );
    assert_eq!(metrics.line_height, ascent + descent + leading);
    let line = *rounded.lines().next().expect("line").metrics();
    assert_eq!(line.line_height, ascent + descent + leading);
    assert_eq!(line.baseline, ascent + leading * 0.5);
}

#[test]
fn a_grid_needs_a_finite_positive_unit() {
    assert!(VerticalMetricGrid::new(0.0).is_none());
    assert!(VerticalMetricGrid::new(-0.75).is_none());
    assert!(VerticalMetricGrid::new(f32::NAN).is_none());
    assert!(VerticalMetricGrid::new(CSS_PIXEL).is_some());
}

#[test]
fn a_half_rounds_toward_zero() {
    let grid = VerticalMetricGrid::new(CSS_PIXEL).expect("grid");
    assert_eq!(grid.round(3.5 * CSS_PIXEL), 3.0 * CSS_PIXEL);
    assert_eq!(grid.round(3.6 * CSS_PIXEL), 4.0 * CSS_PIXEL);
    assert_eq!(grid.round(-3.5 * CSS_PIXEL), -3.0 * CSS_PIXEL);
}
