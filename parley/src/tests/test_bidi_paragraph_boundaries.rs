// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{format, string::String, vec::Vec};

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    Alignment, AlignmentOptions, BaseDirection, FontFamily, Layout, LayoutContext, StyleProperty,
};

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

#[test]
fn alignment_uses_each_lines_paragraph_direction() {
    let width = 300.0;
    for (text, automatic_directions) in [
        ("ABC\n\u{200f}DEF", [false, true]),
        ("\u{200f}ABC\nDEF", [true, false]),
        ("ABC\u{2029}\u{200f}DEF", [false, true]),
        ("\u{200f}ABC\u{2028}DEF", [true, true]),
    ] {
        for direction in [BaseDirection::Auto, BaseDirection::Ltr, BaseDirection::Rtl] {
            let mut layout = build_layout_with_direction(text, direction);
            for alignment in [
                Alignment::Start,
                Alignment::End,
                Alignment::Left,
                Alignment::Right,
                Alignment::Center,
                Alignment::Justify,
            ] {
                layout.align(Some(width), alignment, AlignmentOptions::default());
                for (line, automatic_rtl) in layout.lines().zip(automatic_directions) {
                    let is_rtl = match direction {
                        BaseDirection::Auto => automatic_rtl,
                        BaseDirection::Ltr => false,
                        BaseDirection::Rtl => true,
                    };
                    let free = width - line.metrics().advance;
                    assert_eq!(line.is_rtl(), is_rtl, "{text:?}, {direction:?}");
                    let expected = match (alignment, is_rtl) {
                        (Alignment::Right, _)
                        | (Alignment::Start | Alignment::Justify, true)
                        | (Alignment::End, false) => free,
                        (Alignment::Center, _) => free * 0.5,
                        _ => 0.0,
                    };
                    assert!(
                        (line.metrics().offset - expected).abs() < 0.001,
                        "{text:?}, {direction:?}, {alignment:?}: offset={}, expected={expected}",
                        line.metrics().offset
                    );
                }
            }
        }
    }
}
