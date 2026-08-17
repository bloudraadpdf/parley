// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{format, string::String, vec::Vec};

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{BaseDirection, FontFamily, Layout, LayoutContext, StyleProperty};

fn build_layout_with_direction(text: &str, direction: BaseDirection) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.set_direction(direction);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

fn build_layout(text: &str) -> Layout<ColorBrush> {
    build_layout_with_direction(text, BaseDirection::Ltr)
}

fn visible_lines(layout: &Layout<ColorBrush>, text: &str) -> Vec<String> {
    layout
        .lines()
        .map(|line| {
            line.runs()
                .flat_map(|run| {
                    let mut visible = text[run.text_range()]
                        .chars()
                        .filter(|character| {
                            !character.is_whitespace()
                                && !matches!(character, '\u{2028}' | '\u{2029}')
                        })
                        .collect::<Vec<_>>();
                    if run.is_rtl() {
                        visible.reverse();
                    }
                    visible
                })
                .collect()
        })
        .collect()
}

#[test]
fn b_class_boundaries_resolve_each_paragraph_independently() {
    for separator in ['\n', '\u{2029}'] {
        let text = format!("א + - × ÷ \u{a0}{separator}\u{a0} + - × ÷ ת");
        let layout = build_layout(&text);

        assert_eq!(
            visible_lines(&layout, &text),
            ["א+-×÷", "+-×÷ת"],
            "{separator:?}"
        );
    }
}

#[test]
fn line_separator_retains_one_bidi_paragraph() {
    let text = "א + - × ÷ \u{a0}\u{2028}\u{a0} + - × ÷ ת";
    let layout = build_layout(text);

    assert_eq!(visible_lines(&layout, text), ["÷×-+א", "ת÷×-+"]);
}

#[test]
fn automatic_base_direction_is_resolved_per_paragraph() {
    let text = "abc\nאבג";
    let layout = build_layout_with_direction(text, BaseDirection::Auto);

    assert_eq!(visible_lines(&layout, text), ["abc", "גבא"]);
}
