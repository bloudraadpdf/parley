// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! CSS letter-spacing invariants around invisible formatting characters.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, Layout, LayoutContext, StyleProperty, TextWrapMode, WhiteSpaceCollapse};
use alloc::{sync::Arc, vec::Vec};

fn unwrapped_layout(family: &str, text: &str, letter_spacing: f32) -> Layout<ColorBrush> {
    unwrapped_layout_with_style_range(family, text, letter_spacing, None)
}

fn unwrapped_layout_with_style_range(
    family: &str,
    text: &str,
    letter_spacing: f32,
    enlarged: Option<core::ops::Range<usize>>,
) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    font_context.collection.register_fonts(
        fontique::Blob::new(Arc::new(
            include_bytes!(
                "../../../parley_dev/assets/fonts/noto_naskh_arabic/NotoNaskhArabic.ttf"
            )
            .to_vec(),
        )),
        None,
    );
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::LetterSpacing(letter_spacing));
    if let Some(range) = enlarged {
        builder.push(StyleProperty::FontSize(12.0), range);
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

fn unwrapped_advance(text: &str, letter_spacing: f32) -> f32 {
    unwrapped_layout("Roboto", text, letter_spacing)
        .lines()
        .next()
        .expect("the text must produce one line")
        .metrics()
        .advance
}

fn glyph_count(layout: &Layout<ColorBrush>) -> usize {
    layout
        .lines()
        .flat_map(|line| line.runs().collect::<Vec<_>>())
        .map(|run| {
            run.clusters()
                .map(|cluster| cluster.glyphs().count())
                .sum::<usize>()
        })
        .sum()
}

#[test]
fn letter_spacing_disables_optional_ligatures() {
    // CSS Text 4 §8.2: non-zero tracking must not apply optional ligatures.
    let ligated = unwrapped_layout("Roboto", "fi", 0.0);
    let tracked = unwrapped_layout("Roboto", "fi", 2.0);

    assert_eq!(glyph_count(&ligated), 1, "Roboto ligates fi by default");
    assert_eq!(glyph_count(&tracked), 2);
}

#[test]
fn cursive_scripts_receive_no_letter_spacing() {
    // CSS Text 4 §8.2.1: letter-spacing is not applied within cursive scripts.
    let plain = unwrapped_layout("Noto Naskh Arabic", "ععع", 0.0);
    let tracked = unwrapped_layout("Noto Naskh Arabic", "ععع", 5.0);

    let advance = |layout: &Layout<ColorBrush>| layout.lines().next().unwrap().metrics().advance;
    assert!((advance(&tracked) - advance(&plain)).abs() < 0.001);

    // The word separator between cursive words is still spaced.
    let spaced_plain = unwrapped_layout("Noto Naskh Arabic", "ع ع", 0.0);
    let spaced_tracked = unwrapped_layout("Noto Naskh Arabic", "ع ع", 5.0);
    assert!((advance(&spaced_tracked) - (advance(&spaced_plain) + 5.0)).abs() < 0.001);
}

#[test]
fn default_ignorable_soft_hyphen_does_not_create_a_letter_spacing_interval() {
    // CSS Text defines letter-spacing over typographic character units.
    // U+00AD is default-ignorable in unbroken flow: it contributes only a
    // discretionary break opportunity and must not widen the word. A selected
    // break's visible hyphen is modelled separately by `DiscretionaryBreak`.
    let letter_spacing = 7.5;
    let plain = unwrapped_advance("ARIZONA", letter_spacing);
    let discretionary = unwrapped_advance("ARI\u{00AD}ZONA", letter_spacing);

    assert!(
        (discretionary - plain).abs() < 0.001,
        "an unselected soft hyphen must not add tracking to unbroken flow: plain={plain}, discretionary={discretionary}"
    );
}

#[test]
fn preserved_space_after_zero_width_space_starts_the_next_line() {
    let text = "xx \u{200B} x \u{200B} xx";
    for styled_range in [None, Some(6..9), Some(3..12)] {
        let mut font_context = create_font_context();
        let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
        let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.push_default(StyleProperty::LetterSpacing(10.0));
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::Preserve,
        ));
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::Wrap));
        if let Some(range) = styled_range.clone() {
            builder.push(StyleProperty::LetterSpacing(9.0), range);
        }
        let mut layout = builder.build(text);
        layout.break_all_lines(Some(50.0));
        let ranges = layout
            .lines()
            .map(|line| line.text_range())
            .collect::<Vec<_>>();
        assert_eq!(
            ranges,
            [0..6, 6..12, 12..15],
            "styled_range={styled_range:?}"
        );
    }
}

#[test]
fn letter_spacing_is_not_applied_after_the_last_character_of_a_line() {
    // CSS Text 4 §8.2: tracking is not applied at the end of a line.
    let plain = unwrapped_advance("ab", 0.0);
    let tracked = unwrapped_advance("ab", 5.0);

    assert!(
        (tracked - (plain + 5.0)).abs() < 0.001,
        "plain={plain}, tracked={tracked}"
    );
}

#[test]
fn letter_spacing_counts_graphemes_instead_of_combining_components() {
    for (family, text, enlarged) in [
        ("Roboto", "A\u{301}A\u{301}", None),
        ("Roboto", "A\u{301}\u{302}B", None),
        ("Roboto", "A\u{fe0f}B", None),
        ("Roboto", "\u{301}A", None),
        ("Roboto", "A\u{301}B", Some(1..3)),
        ("Arimo", "\u{5d0}\u{301}\u{5d0}\u{301}", None),
    ] {
        let plain_layout = unwrapped_layout_with_style_range(family, text, 0.0, enlarged.clone());
        let plain = plain_layout.lines().next().unwrap().metrics().advance;
        let mut layout = unwrapped_layout_with_style_range(family, text, 5.0, enlarged);
        let tracked = layout.lines().next().unwrap().metrics().advance;
        assert!(
            (tracked - (plain + 5.0)).abs() < 0.001,
            "{text:?}: plain={plain}, tracked={tracked}"
        );
        let glyph_advance: f32 = layout
            .lines()
            .flat_map(|line| line.items())
            .map(|item| match item {
                crate::PositionedLayoutItem::GlyphRun(run) => run.advance(),
                crate::PositionedLayoutItem::InlineBox(_) => 0.0,
            })
            .sum();
        assert!(
            (glyph_advance - tracked).abs() < 0.001,
            "{text:?}: glyphs={glyph_advance}, line={tracked}"
        );
        layout.break_all_lines(None);
        assert!((layout.lines().next().unwrap().metrics().advance - tracked).abs() < 0.001);
    }
}

#[test]
fn trailing_letter_spacing_does_not_decide_the_line_fit() {
    let plain = unwrapped_advance("ab", 0.0);
    let mut layout = unwrapped_layout("Roboto", "ab ab", 5.0);
    layout.break_all_lines(Some(plain + 5.0 + 0.05));

    let lines: Vec<_> = layout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "the tracked word fits once its trailing spacing hangs"
    );
    let first = lines[0].metrics();
    assert!((first.advance - first.trailing_whitespace - (plain + 5.0)).abs() < 0.001);
}

#[test]
fn preserved_terminal_space_owns_the_last_tracking_interval() {
    let shape = |tracking| {
        let mut font_context = create_font_context();
        let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
        let mut builder = layout_context.ranged_builder(&mut font_context, "x ", 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.push_default(StyleProperty::LetterSpacing(tracking));
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::Preserve,
        ));
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::Wrap));
        let mut layout = builder.build("x ");
        layout.break_all_lines(None);
        let advances = layout
            .lines()
            .next()
            .unwrap()
            .runs()
            .flat_map(|run| {
                run.clusters()
                    .map(|cluster| cluster.advance())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        advances
    };
    let plain = shape(0.0);
    let tracked = shape(5.0);
    assert_eq!(plain.len(), 2);
    assert_eq!(tracked.len(), 2);
    assert!(
        (tracked[0] - plain[0] - 5.0).abs() < 0.001,
        "{plain:?} {tracked:?}"
    );
    assert!(
        (tracked[1] - plain[1]).abs() < 0.001,
        "{plain:?} {tracked:?}"
    );
}

#[test]
fn a_line_fragment_keeps_its_trailing_letter_spacing_on_request() {
    // A ruby base is only part of a line, so its last character keeps the
    // tracking that the following fragment continues.
    let plain = unwrapped_advance("ab", 0.0);
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, "ab", 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::LetterSpacing(5.0));
    let mut layout = builder.build("ab");
    layout.set_line_end_letter_spacing_trim(false);
    layout.break_all_lines(None);

    let advance = layout.lines().next().unwrap().metrics().advance;
    assert!(
        (advance - (plain + 10.0)).abs() < 0.001,
        "plain={plain}, kept={advance}"
    );
}

#[test]
fn breaking_the_layout_again_restores_the_trimmed_tracking_first() {
    let plain = unwrapped_advance("ab", 0.0);
    let mut layout = unwrapped_layout("Roboto", "ab", 5.0);
    let first = layout.lines().next().unwrap().metrics().advance;
    layout.break_all_lines(None);
    let second = layout.lines().next().unwrap().metrics().advance;
    assert!((first - (plain + 5.0)).abs() < 0.001 && (second - first).abs() < 0.001);

    layout.set_line_end_letter_spacing_trim(false);
    layout.break_all_lines(None);
    let kept = layout.lines().next().unwrap().metrics().advance;
    assert!(
        (kept - (plain + 10.0)).abs() < 0.001,
        "plain={plain}, kept={kept}"
    );
}
