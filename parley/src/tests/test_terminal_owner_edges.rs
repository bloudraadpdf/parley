// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{borrow::ToOwned, format, vec};

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{
    Alignment, AlignmentOptions, FontFamily, InlineBox, InlineBoxBreakAffinity,
    InlineOwnerEdgeAdvance, Layout, LayoutContext, PositionedLayoutItem, StyleProperty,
    TextWrapMode, WhiteSpaceCollapse,
};

fn layout(text: &str, reduction: f32, following: bool) -> Layout<ColorBrush> {
    let source = if following {
        format!("{text}Z")
    } else {
        text.to_owned()
    };
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, &source, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(20.0));
    builder.push(
        StyleProperty::WhiteSpaceCollapse(WhiteSpaceCollapse::Preserve),
        1..text.len(),
    );
    builder.push(
        StyleProperty::TextWrapMode(TextWrapMode::NoWrap),
        1..text.len(),
    );
    builder.push_inline_box(InlineBox::inline_end_edge(
        7,
        text.len(),
        InlineOwnerEdgeAdvance::uniform(25.0)
            .with_terminal_preserved_space_reduction(reduction, reduction, 0),
        0.0,
        InlineBoxBreakAffinity::ToPrevious,
    ));
    let mut result = builder.build(&source);
    if following {
        result.set_line_break_overrides(vec![crate::LineBreakOverride::opportunity(text.len())]);
    }
    result
}

fn edge(layout: &Layout<ColorBrush>, line: usize) -> (f32, f32) {
    layout
        .lines()
        .nth(line)
        .unwrap()
        .items()
        .find_map(|item| match item {
            PositionedLayoutItem::InlineBox(edge) if edge.id == 7 => Some((edge.x, edge.width)),
            _ => None,
        })
        .unwrap()
}

#[test]
fn terminal_owner_edge_reduction_keeps_fitting_advance() {
    for text in ["X  ", "X  \n"] {
        let mut ordinary = layout(text, 0.0, false);
        ordinary.break_all_lines(None);
        let mut reduced = layout(text, 20.0, false);
        reduced.break_all_lines(None);
        assert_eq!(edge(&reduced, 0).1, 5.0);
        assert_eq!(
            reduced.data.lines[0].fitting_advance(),
            ordinary.data.lines[0].fitting_advance()
        );
        assert_eq!(
            reduced.data.lines[0].metrics.advance + 20.0,
            ordinary.data.lines[0].metrics.advance
        );
        ordinary.align(Some(100.0), Alignment::End, AlignmentOptions::default());
        reduced.align(Some(100.0), Alignment::End, AlignmentOptions::default());
        assert!((edge(&reduced, 0).0 - edge(&ordinary, 0).0 - 20.0).abs() < 0.001);
    }
}

#[test]
fn following_content_preserves_edge_until_the_selected_wrap() {
    let mut ordinary = layout("X  ", 0.0, true);
    ordinary.break_all_lines(None);
    let full = ordinary.data.lines[0].metrics.advance;
    ordinary.break_all_lines(Some(full - 2.0));
    assert_eq!(
        ordinary.len(),
        2,
        "the ordinary edge must wrap the following atomic"
    );
    let mut reduced = layout("X  ", 20.0, true);
    reduced.break_all_lines(Some(full));
    assert_eq!(edge(&reduced, 0).1, 25.0);
    assert_eq!(reduced.len(), 1);
    reduced.break_all_lines(Some(full - 2.0));
    assert_eq!(reduced.len(), 2);
    assert_eq!(edge(&reduced, 0).1, 5.0);
    let mut letters = layout("XYY", 20.0, false);
    letters.break_all_lines(None);
    assert_eq!(edge(&letters, 0).1, 25.0);
}

#[test]
fn preceding_sibling_space_does_not_trim_an_empty_owner() {
    let mut value = layout("X  ", 20.0, false);
    // The final edge belongs to an empty sibling beginning after the spaces.
    value.inline_boxes_mut()[0] = InlineBox::inline_end_edge(
        7,
        3,
        InlineOwnerEdgeAdvance::uniform(25.0)
            .with_terminal_preserved_space_reduction(20.0, 20.0, 3),
        0.0,
        InlineBoxBreakAffinity::ToPrevious,
    );
    value.break_all_lines(None);
    assert_eq!(edge(&value, 0).1, 25.0);
}

#[test]
fn overflowing_terminal_owner_keeps_preserved_spaces() {
    let mut value = layout("X  ", 20.0, false);
    value.break_all_lines(None);
    let full = value.data.lines[0].fitting_advance();
    value.break_all_lines(Some(full - 10.0));
    assert_eq!(value.len(), 1);
    assert_eq!(value.lines().next().unwrap().text_range(), 0..3);
    assert_eq!(edge(&value, 0).1, 5.0);
}

#[test]
fn atomic_boundary_affinity_survives_a_preserved_child() {
    for (affinity, configured_fit, expected_lines) in [
        (InlineBoxBreakAffinity::Independent, false, 2),
        (InlineBoxBreakAffinity::Both, false, 1),
        (InlineBoxBreakAffinity::Independent, true, 2),
        (InlineBoxBreakAffinity::Both, true, 1),
    ] {
        let text = "X  ";
        let mut fonts = create_font_context();
        let mut context = LayoutContext::<ColorBrush>::new();
        let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(20.0));
        builder.push(
            StyleProperty::WhiteSpaceCollapse(WhiteSpaceCollapse::Preserve),
            1..3,
        );
        builder.push(StyleProperty::TextWrapMode(TextWrapMode::NoWrap), 1..3);
        builder.push_inline_box(InlineBox::inline_end_edge(
            7,
            3,
            InlineOwnerEdgeAdvance::uniform(25.0)
                .with_terminal_preserved_space_reduction(20.0, 20.0, 0),
            0.0,
            InlineBoxBreakAffinity::ToPrevious,
        ));
        builder.push_inline_box(InlineBox::atomic_with_break_affinity(
            8, 3, 5.0, 5.0, affinity,
        ));
        let mut value = builder.build(text);
        if configured_fit {
            value.set_normal_soft_wrap_selection(crate::NormalSoftWrapSelection::GreedyLatest);
            value.set_line_fit_tolerance(0.0075);
            value.set_line_end_letter_spacing_trim(false);
        }
        value.break_all_lines(None);
        let full = value.data.lines[0].fitting_advance();
        for available in [full - 15.0, full - 5.0, full, full + 15.0] {
            value.break_all_lines(Some(available));
            let expected = if available < full { expected_lines } else { 1 };
            assert_eq!(
                value.len(),
                expected,
                "{affinity:?}, configured_fit={configured_fit}, width={available}"
            );
            assert_eq!(edge(&value, 0).1, if expected == 2 { 5.0 } else { 25.0 });
        }
    }
}

#[test]
fn bidi_isolation_keeps_nowrap_source_boundary_context() {
    let build = |outer: bool, inner: bool| {
        let mut text = alloc::string::String::from("お");
        let mut edges = vec![];
        if outer {
            text.push('\u{2066}');
        }
        let outer_start = text.len();
        let mut ranges = vec![];
        for base in ["あい", "うえ"] {
            if inner {
                text.push('\u{2066}');
            }
            let start = text.len();
            text.push_str(base);
            let end = text.len();
            ranges.push(start..end);
            if inner {
                text.push('\u{2069}');
            }
            edges.push((start, text.len()));
        }
        if outer {
            text.push('\u{2069}');
        }
        edges.push((outer_start, text.len()));
        text.push('お');
        let mut fonts = create_font_context();
        let mut context = LayoutContext::<ColorBrush>::new();
        let mut builder = context.ranged_builder(&mut fonts, &text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(16.0));
        for range in ranges {
            builder.push(StyleProperty::TextWrapMode(TextWrapMode::NoWrap), range);
        }
        for (index, (start, end)) in edges.into_iter().enumerate() {
            builder.push_inline_box(InlineBox::inline_start_edge(
                index as u64 * 2,
                start,
                InlineOwnerEdgeAdvance::uniform(0.0),
                0.0,
                InlineBoxBreakAffinity::ToNext,
            ));
            builder.push_inline_box(InlineBox::inline_end_edge(
                index as u64 * 2 + 1,
                end,
                InlineOwnerEdgeAdvance::uniform(0.0),
                0.0,
                InlineBoxBreakAffinity::ToPrevious,
            ));
        }
        builder.build(&text)
    };
    let mut reference = build(false, false);
    reference.break_all_lines(None);
    let expected = reference.calculate_content_widths();
    reference.break_all_lines(Some(expected.min));
    let expected_lines: alloc::vec::Vec<_> = reference
        .lines()
        .map(|line| line.metrics().advance)
        .collect();
    for (outer, inner) in [(true, false), (false, true), (true, true)] {
        let mut actual = build(outer, inner);
        actual.break_all_lines(None);
        let widths = actual.calculate_content_widths();
        assert!(
            (widths.min - expected.min).abs() < 0.001,
            "outer={outer} inner={inner}: {} != {}",
            widths.min,
            expected.min
        );
        actual.break_all_lines(Some(expected.min));
        let lines: alloc::vec::Vec<_> = actual.lines().map(|line| line.metrics().advance).collect();
        assert_eq!(lines, expected_lines, "outer={outer} inner={inner}");
    }
}
