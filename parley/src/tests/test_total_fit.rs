// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Paragraph-at-a-time line breaking.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, Layout, LayoutContext, LineBreakStrategy, StyleProperty, TotalFit};
use alloc::vec::Vec;

/// Words that Prince 16.2 sets at 200 pt in 12 pt Roboto, justified, as 6, 6 and 5 words; first fit takes 7 words
/// on the first line by shrinking its separators.
const TEXT: &str = "xxxxxx xx x xxxxxxxx xxx xxxxxxxxx xx xxxxxx xxxx xxxxxxxx xxxxxxx xxxx xxxxxxxx xx x xxxx xxx";
const WIDTH: f32 = 200.0;
const PRINCE: TotalFit = TotalFit {
    stretch: 0.5,
    tolerance: 800.0,
    line_penalty: 10.0,
};

fn layout(text: &str, strategy: LineBreakStrategy) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
    let mut layout = builder.build(text);
    layout.set_line_break_strategy(strategy);
    layout
}

fn words_per_line(text: &str, layout: &Layout<ColorBrush>) -> Vec<usize> {
    layout
        .lines()
        .map(|line| text[line.text_range()].split_whitespace().count())
        .collect()
}

fn broken(text: &str, strategy: LineBreakStrategy) -> Vec<usize> {
    let mut layout = layout(text, strategy);
    layout.break_all_lines(Some(WIDTH));
    words_per_line(text, &layout)
}

#[test]
fn total_fit_evens_the_separators_of_a_paragraph() {
    assert_eq!(broken(TEXT, LineBreakStrategy::FirstFit), [7, 5, 5]);
    assert_eq!(broken(TEXT, LineBreakStrategy::TotalFit(PRINCE)), [6, 6, 5]);
}

#[test]
fn each_segment_between_forced_breaks_has_its_own_plan() {
    let text = alloc::format!("{TEXT}\n{TEXT}");
    assert_eq!(
        broken(&text, LineBreakStrategy::TotalFit(PRINCE)),
        [6, 6, 5, 6, 6, 5]
    );
}

#[test]
fn a_line_of_another_measure_breaks_the_rest_of_the_segment_first_fit() {
    let mut layout = layout(TEXT, LineBreakStrategy::TotalFit(PRINCE));
    let mut breaker = layout.break_lines();
    let mut widths = [WIDTH, WIDTH + 50.0, WIDTH + 50.0].into_iter();
    while breaker
        .break_next(widths.next().unwrap_or(WIDTH), crate::LineTabOrigin::ZERO)
        .is_some()
    {}
    breaker.finish();
    assert_eq!(words_per_line(TEXT, &layout), [6, 7, 4]);
}
