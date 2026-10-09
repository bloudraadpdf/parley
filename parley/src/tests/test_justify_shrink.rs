// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Word separators that shrink to fit and justify a line.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{Alignment, AlignmentOptions, FontFamily, Layout, LayoutContext, StyleProperty};
use alloc::format;
use alloc::vec::Vec;

const WORDS: &str = "x x x x x x";

fn layout(text: &str, shrink: f32) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::JustifyShrink(shrink));
    builder.build(text)
}

fn natural_width(text: &str) -> f32 {
    let mut layout = layout(text, 0.0);
    layout.break_all_lines(None);
    layout.full_width()
}

/// The width that the 5 spaces of the words reach at `ratio` of their advance.
fn width_at(ratio: f32) -> f32 {
    let natural = natural_width(WORDS);
    natural - (natural - natural_width("xxxxxx")) * (1.0 - ratio)
}

fn broken(shrink: f32, width: f32) -> Layout<ColorBrush> {
    let mut layout = layout(&format!("{WORDS} yyyyyyyyyyyy"), shrink);
    layout.break_all_lines(Some(width));
    layout
}

fn first_line_xs(layout: &Layout<ColorBrush>) -> usize {
    let line = layout.lines().next().expect("a line");
    WORDS[..line.text_range().end.min(WORDS.len())]
        .matches('x')
        .count()
}

fn cluster_advances(layout: &Layout<ColorBrush>, lines: usize) -> Vec<f32> {
    layout
        .lines()
        .take(lines)
        .flat_map(|line| line.runs().collect::<Vec<_>>())
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| cluster.advance())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn first_line_advance(layout: &Layout<ColorBrush>) -> f32 {
    cluster_advances(layout, 1).iter().sum()
}

fn justify(layout: &mut Layout<ColorBrush>, width: f32) {
    layout.align(Some(width), Alignment::Justify, AlignmentOptions::default());
}

#[test]
fn a_line_fits_when_its_separators_shrink_within_their_limit() {
    let width = width_at(0.9);
    let mut layout = broken(1.0 / 6.0, width);
    assert_eq!(first_line_xs(&layout), 6);
    justify(&mut layout, width);
    let advance = first_line_advance(&layout);
    assert!(
        (advance - width).abs() < 1e-3,
        "advance {advance}, width {width}"
    );
}

#[test]
fn a_line_breaks_when_its_separators_would_shrink_beyond_their_limit() {
    assert_eq!(first_line_xs(&broken(1.0 / 6.0, width_at(0.8))), 5);
}

#[test]
fn separators_without_shrink_keep_their_advance() {
    assert_eq!(first_line_xs(&broken(0.0, width_at(0.9))), 5);
}

#[test]
fn breaking_again_restores_the_shrunk_separators() {
    let width = width_at(0.9);
    let mut layout = broken(1.0 / 6.0, width);
    let natural = cluster_advances(&layout, usize::MAX);
    justify(&mut layout, width);
    layout.break_all_lines(Some(width));
    assert_eq!(cluster_advances(&layout, usize::MAX), natural);
}

#[test]
fn the_last_line_of_a_justified_layout_shrinks_to_fit() {
    let width = width_at(0.9);
    let mut layout = layout(WORDS, 1.0 / 6.0);
    layout.break_all_lines(Some(width));
    justify(&mut layout, width);
    let advance = first_line_advance(&layout);
    assert!(
        layout.len() == 1 && (advance - width).abs() < 1e-3,
        "advance {advance}, width {width}"
    );
}

#[test]
fn a_start_aligned_line_keeps_its_separators() {
    let width = width_at(0.9);
    let mut layout = broken(1.0 / 6.0, width);
    let natural = cluster_advances(&layout, 1);
    layout.align(Some(width), Alignment::Start, AlignmentOptions::default());
    assert_eq!(cluster_advances(&layout, 1), natural);
}
