// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! CSS letter-spacing invariants around invisible formatting characters.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{
    FontFamily, Layout, LayoutContext, RangedBuilder, StyleProperty, TextWrapMode,
    WhiteSpaceCollapse, WordBreak,
};
use alloc::{string::ToString, sync::Arc, vec::Vec};

fn unwrapped_layout(family: &str, text: &str, letter_spacing: f32) -> Layout<ColorBrush> {
    unwrapped_layout_with_style_range(family, text, letter_spacing, None)
}

fn unwrapped_layout_with_style_range(
    family: &str,
    text: &str,
    letter_spacing: f32,
    enlarged: Option<core::ops::Range<usize>>,
) -> Layout<ColorBrush> {
    configured_layout(family, text, letter_spacing, |builder| {
        if let Some(range) = enlarged {
            builder.push(StyleProperty::FontSize(12.0), range);
        }
    })
}

fn configured_layout(
    family: &str,
    text: &str,
    letter_spacing: f32,
    configure: impl FnOnce(&mut RangedBuilder<'_, ColorBrush>),
) -> Layout<ColorBrush> {
    let mut layout = configured_unbroken_layout(family, text, letter_spacing, configure);
    layout.break_all_lines(None);
    layout
}

fn configured_unbroken_layout(
    family: &str,
    text: &str,
    letter_spacing: f32,
    configure: impl FnOnce(&mut RangedBuilder<'_, ColorBrush>),
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
    font_context.collection.register_fonts(
        fontique::Blob::new(Arc::new(
            include_bytes!(
                "../../../parley_dev/assets/fonts/noto_sans_bengali/NotoSansBengali-Regular.ttf"
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
    configure(&mut builder);
    builder.build(text)
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

fn glyph_origins(layout: &Layout<ColorBrush>) -> Vec<f32> {
    layout
        .lines()
        .flat_map(|line| line.items())
        .flat_map(|item| match item {
            crate::PositionedLayoutItem::GlyphRun(run) => run
                .positioned_glyphs()
                .map(|glyph| glyph.x)
                .collect::<Vec<_>>(),
            crate::PositionedLayoutItem::InlineBox(_) => Vec::new(),
        })
        .collect()
}

#[test]
fn adjacent_spacing_values_contribute_half_each_after_bidi() {
    for (text, values, expected) in [
        (
            "ABCD",
            &[0.0, 10.0, 20.0, 0.0][..],
            &[0.0, 5.0, 20.0, 30.0][..],
        ),
        (
            "AאבB",
            &[0.0, 10.0, 20.0, 0.0][..],
            &[0.0, 10.0, 25.0, 30.0][..],
        ),
        ("אבג", &[10.0, 20.0, 30.0][..], &[0.0, 25.0, 40.0][..]),
    ] {
        let plain = unwrapped_layout("Arimo", text, 0.0);
        let tracked = configured_layout("Arimo", text, 0.0, |builder| {
            for ((start, character), spacing) in text.char_indices().zip(values) {
                builder.push(
                    StyleProperty::LetterSpacing(*spacing),
                    start..start + character.len_utf8(),
                );
            }
        });
        let actual = glyph_origins(&tracked);
        let natural = glyph_origins(&plain);
        assert_eq!(actual.len(), expected.len());
        assert_eq!(natural.len(), expected.len());
        for ((actual, natural), expected) in actual.iter().zip(&natural).zip(expected) {
            assert!(
                (actual - natural - expected).abs() < 0.001,
                "{text}: {actual} - {natural} != {expected}"
            );
        }
        assert!(
            (tracked.full_width() - plain.full_width() - expected.last().unwrap()).abs() < 0.001
        );
    }
}

#[test]
fn unequal_spacing_fits_with_visual_half_edges_and_restores_before_rebreak() {
    for (text, first, last) in [("AB", 0.0, 10.0), ("אב", 10.0, 30.0), ("AB", 0.0, -4.0)] {
        let plain = unwrapped_layout("Arimo", text, 0.0).full_width();
        let split = text.char_indices().nth(1).unwrap().0;
        let mut layout = configured_layout("Arimo", text, first, |builder| {
            builder.push(StyleProperty::LetterSpacing(last), split..text.len());
            builder.push_default(StyleProperty::LineBreakMode(
                crate::style::LineBreakMode::Anywhere,
            ));
        });
        let expected = plain + (first + last) * 0.5;
        assert!((layout.full_width() - expected).abs() < 0.001);
        layout.break_all_lines(Some(expected - 0.01));
        assert_eq!(layout.lines().count(), 2, "{text}: {first}, {last}");
        layout.break_all_lines(Some(expected));
        assert_eq!(layout.lines().count(), 1, "{text}: {first}, {last}");
        assert!((layout.full_width() - expected).abs() < 0.001);
        layout.break_all_lines(None);
        assert!((layout.full_width() - expected).abs() < 0.001);
    }
}

#[test]
fn unequal_spacing_preserves_requested_fragment_end_spacing() {
    let plain = unwrapped_layout("Arimo", "AB", 0.0).full_width();
    let mut layout = configured_layout("Arimo", "AB", 0.0, |builder| {
        builder.push(StyleProperty::LetterSpacing(10.0), 1..2);
    });
    layout.set_line_end_letter_spacing_trim(false);
    layout.break_all_lines(None);
    assert!((layout.full_width() - plain - 15.0).abs() < 0.001);
    layout.break_all_lines(None);
    assert!((layout.full_width() - plain - 15.0).abs() < 0.001);
}

#[test]
fn measured_whitespace_fits_its_resolved_edge_tracking() {
    for text in ["A ", "A\u{00a0}"] {
        let configure = |builder: &mut RangedBuilder<'_, ColorBrush>| {
            let features = [crate::setting::FontFeature {
                tag: crate::setting::Tag::from_bytes(*b"kern"),
                value: 0,
            }];
            builder.push_default(StyleProperty::FontFeatures((&features).into()));
            builder.push_default(StyleProperty::WhiteSpaceCollapse(
                WhiteSpaceCollapse::BreakSpaces,
            ));
            builder.push_default(StyleProperty::LineBreakMode(
                crate::style::LineBreakMode::Anywhere,
            ));
        };
        let plain = configured_layout("Arimo", text, 0.0, configure);
        let mut tracked = configured_layout("Arimo", text, 10.0, |builder| {
            configure(builder);
            builder.push(StyleProperty::LetterSpacing(30.0), 1..text.len());
        });
        let expected = plain.full_width() + 20.0;
        assert!(
            (tracked.full_width() - expected).abs() < 0.001,
            "{text:?}: actual={}, expected={expected}, glyphs={:?}",
            tracked.full_width(),
            glyph_origins(&tracked)
        );
        tracked.break_all_lines(Some(expected));
        assert_eq!(tracked.lines().count(), 1, "{text:?}");
        assert!(
            (tracked.full_width() - expected).abs() < 0.001,
            "{text:?}: actual={}, expected={expected}, glyphs={:?}",
            tracked.full_width(),
            glyph_origins(&tracked)
        );
    }
}

#[test]
fn unequal_tracking_fits_the_same_tab_stop_as_positioning() {
    let before_tab = unwrapped_layout("Arimo", "A", 0.0).full_width() + 5.0;
    let following = unwrapped_layout("Arimo", "B", 0.0).full_width();
    for (interval, stops) in [
        (15.0, 1.0),
        (40.0, 1.0),
        (before_tab, 2.0),
        (before_tab + 0.01, 2.0),
    ] {
        let mut layout = configured_layout("Arimo", "A\tB", 10.0, |builder| {
            builder.push_default(StyleProperty::WhiteSpaceCollapse(
                WhiteSpaceCollapse::Preserve,
            ));
            builder.push_default(StyleProperty::TabSize(crate::style::TabSize::Length(
                interval,
            )));
            builder.push(StyleProperty::LetterSpacing(0.0), 1..2);
            builder.push(StyleProperty::LetterSpacing(20.0), 2..3);
        });
        let expected = interval * stops + following;
        assert!((glyph_origins(&layout)[2] - interval * stops).abs() < 0.001);
        assert!((layout.full_width() - expected).abs() < 0.001);
        layout.break_all_lines(Some(expected));
        assert_eq!(layout.lines().count(), 1, "interval={interval}");
        assert!((glyph_origins(&layout)[2] - interval * stops).abs() < 0.001);
        assert!((layout.full_width() - expected).abs() < 0.001);
        layout.break_all_lines(None);
        assert!((glyph_origins(&layout)[2] - interval * stops).abs() < 0.001);
        assert!((layout.full_width() - expected).abs() < 0.001);
    }
}

#[test]
fn atomic_tracking_uses_each_outer_value_and_groups_consecutive_boxes() {
    for (text, first, last, atoms, first_box_delta, final_text_delta) in [
        ("AB", 10.0, 10.0, &[10.0, 10.0][..], 10.0, 20.0),
        ("AB", 10.0, -10.0, &[10.0][..], 10.0, 10.0),
        ("AB", 0.0, 20.0, &[10.0][..], 5.0, 20.0),
        ("AB", 20.0, 0.0, &[-10.0][..], 5.0, 0.0),
        ("AB", 10.0, 20.0, &[5.0, 25.0][..], 7.5, 30.0),
        ("אב", 10.0, 20.0, &[15.0][..], 17.5, 30.0),
    ] {
        let split = text.char_indices().nth(1).unwrap().0;
        let make = |tracked| {
            configured_layout(
                "Arimo",
                text,
                if tracked { first } else { 0.0 },
                |builder| {
                    builder.push(
                        StyleProperty::LetterSpacing(if tracked { last } else { 0.0 }),
                        split..text.len(),
                    );
                    for (index, &spacing) in atoms.iter().enumerate() {
                        builder.push_inline_box(
                            crate::InlineBox::new(index as u64, split, 10.0, 10.0)
                                .with_letter_spacing(if tracked { spacing } else { 0.0 }),
                        );
                    }
                },
            )
        };
        let plain = make(false);
        let mut tracked = make(true);
        let boxes = |layout: &Layout<ColorBrush>| {
            layout
                .lines()
                .flat_map(|line| line.items())
                .filter_map(|item| {
                    if let crate::PositionedLayoutItem::InlineBox(item) = item {
                        Some(item.x)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        let initial_boxes = boxes(&plain);
        let tracked_boxes = boxes(&tracked);
        for (initial, actual) in initial_boxes.iter().zip(tracked_boxes) {
            assert!(
                (actual - initial - first_box_delta).abs() < 0.001,
                "{text}: {first}, {last}, {atoms:?}"
            );
        }
        assert!(
            (glyph_origins(&tracked)[1] - glyph_origins(&plain)[1] - final_text_delta).abs()
                < 0.001
        );
        assert!((tracked.full_width() - plain.full_width() - final_text_delta).abs() < 0.001);
        let expected = plain.full_width() + final_text_delta;
        tracked.break_all_lines(Some(expected));
        assert_eq!(
            tracked.lines().count(),
            1,
            "{text}: {first}, {last}, {atoms:?}"
        );
        tracked.break_all_lines(Some(expected - 0.01));
        assert_eq!(
            tracked.lines().count(),
            2,
            "{text}: {first}, {last}, {atoms:?}"
        );
        tracked.break_all_lines(None);
        assert!((tracked.full_width() - plain.full_width() - final_text_delta).abs() < 0.001);
    }
}

#[test]
fn intrinsic_tracking_uses_fragment_edges_before_and_after_line_breaking() {
    for (text, first, last, atoms, delta) in [
        ("AB", 0.0, 20.0, &[][..], 10.0),
        ("AB", 10.0, 20.0, &[5.0, 25.0][..], 30.0),
        ("אב", 10.0, 20.0, &[15.0][..], 30.0),
    ] {
        let split = text.char_indices().nth(1).unwrap().0;
        let plain = unwrapped_layout("Arimo", text, 0.0).full_width() + atoms.len() as f32 * 10.0;
        for nowrap in [false, true] {
            let mut layout = configured_unbroken_layout("Arimo", text, first, |builder| {
                builder.push(StyleProperty::LetterSpacing(last), split..text.len());
                builder.push_default(StyleProperty::OverflowWrap(crate::OverflowWrap::Anywhere));
                if nowrap {
                    builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
                }
                for (index, &spacing) in atoms.iter().enumerate() {
                    builder.push_inline_box(
                        crate::InlineBox::new(index as u64, split, 10.0, 10.0)
                            .with_letter_spacing(spacing),
                    );
                }
            });
            let expected_max = plain + delta;
            let expected_min = if nowrap {
                expected_max
            } else {
                text.chars()
                    .map(|character| {
                        unwrapped_layout("Arimo", &character.to_string(), 0.0).full_width()
                    })
                    .fold(if atoms.is_empty() { 0.0 } else { 10.0 }, f32::max)
            };
            for measure in [None, Some(expected_min), None] {
                let widths = layout.calculate_content_widths();
                assert!(
                    (widths.max - expected_max).abs() < 0.001,
                    "{text}, nowrap={nowrap}: max {} != {expected_max}",
                    widths.max
                );
                assert!(
                    (widths.min - expected_min).abs() < 0.001,
                    "{text}, nowrap={nowrap}: min {} != {expected_min}",
                    widths.min
                );
                layout.break_all_lines(measure);
            }
        }
    }
}

#[test]
fn zero_width_atomic_still_has_a_tracking_boundary_when_fitting() {
    let plain = unwrapped_layout("Arimo", "A", 0.0).full_width();
    let mut layout = configured_layout("Arimo", "A", 10.0, |builder| {
        builder.push_inline_box(crate::InlineBox::new(0, 1, 0.0, 10.0));
    });
    assert!((layout.full_width() - plain - 5.0).abs() < 0.001);
    layout.break_all_lines(Some(plain));
    assert_eq!(layout.lines().count(), 2);
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
fn explicitly_enabled_ligatures_keep_their_grapheme_spacing() {
    let shape = |spacing| {
        configured_layout("Roboto", "fi", spacing, |builder| {
            let features = [crate::setting::FontFeature {
                tag: crate::setting::Tag::from_bytes(*b"liga"),
                value: 1,
            }];
            builder.push_default(StyleProperty::FontFeatures((&features).into()));
        })
    };
    let plain = shape(0.0);
    let tracked = shape(5.0);
    assert_eq!(glyph_count(&plain), 1);
    assert_eq!(glyph_count(&tracked), 1);
    assert!((tracked.full_width() - plain.full_width() - 5.0).abs() < 0.001);
}

#[test]
fn break_all_keeps_bengali_vowel_conjuncts_whole() {
    let text = "অ্যাএ্যা";
    let mut layout = configured_layout("Noto Sans Bengali", text, 5.0, |builder| {
        builder.push_default(StyleProperty::WordBreak(WordBreak::BreakAll));
    });
    layout.break_all_lines(Some(1.0));
    assert_eq!(
        layout.lines().map(|line| line.text_range()).collect::<Vec<_>>(),
        [0..12, 12..24]
    );
}

#[test]
fn indic_linker_boundaries_follow_unicode_18_gb9c() {
    let analysis = crate::analysis::AnalysisDataSources::new();
    for text in ["অ্যা", "এ্যা", "অ\u{200d}্যা", "অ্\u{200d}যা", "অ্ক", "্য"] {
        assert_eq!(
            analysis.grapheme_boundaries(text).collect::<Vec<_>>(),
            [0, text.len()]
        );
    }
    for (text, boundaries) in [
        ("অ্\u{200c}যা", &[0, 9, 15][..]),
        ("অ্যা এ্যা", &[0, 12, 13, 25][..]),
        ("অ্ ক", &[0, 6, 7, 10][..]),
        ("fi", &[0, 1, 2][..]),
    ] {
        assert_eq!(
            analysis.grapheme_boundaries(text).collect::<Vec<_>>(),
            boundaries
        );
    }
}

#[test]
fn bengali_vowel_conjunct_is_one_letter_spacing_unit() {
    for (text, intervals) in [
        ("অ্যা", 0.0),
        ("এ্যা", 0.0),
        ("অ্যা এ্যা", 2.0),
        ("অ্যান্টিগুয়া", 3.0),
        ("এ্যাডভোকেট", 4.0),
    ] {
        let plain = unwrapped_layout("Noto Sans Bengali", text, 0.0);
        let tracked = unwrapped_layout("Noto Sans Bengali", text, 5.0);
        let plain = plain.lines().next().unwrap().metrics().advance;
        let tracked = tracked.lines().next().unwrap().metrics().advance;
        assert!(
            (tracked - plain - intervals * 5.0).abs() < 0.001,
            "{text}: plain={plain}, tracked={tracked}"
        );
    }
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
fn rtl_tracking_leaves_both_visual_line_edges_flush() {
    for text in ["ABC", "אבג", "AאבB", "אABב"] {
        let plain = unwrapped_layout("Arimo", text, 0.0);
        let tracked = unwrapped_layout("Arimo", text, 5.0);
        let plain_origins = glyph_origins(&plain);
        let tracked_origins = glyph_origins(&tracked);
        assert_eq!(tracked_origins.len(), text.chars().count());
        for ((actual, initial), extra) in tracked_origins
            .iter()
            .zip(&plain_origins)
            .zip([0.0, 5.0, 10.0, 15.0])
        {
            assert!(
                (actual - initial - extra).abs() < 0.001,
                "{text}: {plain_origins:?} {tracked_origins:?}"
            );
        }
    }
}

#[test]
fn cluster_reports_only_letter_spacing_that_survives_line_end_trimming() {
    let layout = unwrapped_layout("Roboto", "ABC", 10.0);
    let spacing = layout
        .lines()
        .flat_map(|line| {
            line.runs()
                .flat_map(|run| {
                    run.clusters()
                        .map(|cluster| cluster.applied_letter_spacing())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(spacing, [10.0, 10.0, 0.0]);
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
