// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{collections::BTreeSet, vec, vec::Vec};

use super::utils::visual_glyphs as glyphs;
use super::{
    test_builders::create_font_context, test_physical_shaping::owner,
    test_shaping_context::naskh_font_context, utils::ColorBrush,
};
use crate::layout::{
    InlineOwnerShaping, InlineShapingEdge, PhysicalLineEdge, ShapingEdgePlacement,
};
use crate::{
    BaseDirection, FontFamily, InlineBox, InlineBoxBreakAffinity, Layout, LayoutContext,
    LineTabOrigin, StyleProperty,
};

fn arabic_layout(physical: bool, explicit: bool) -> Layout<ColorBrush> {
    let text = "السلامعليكم";
    let mut fonts = naskh_font_context();
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.set_direction(BaseDirection::Ltr);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(
        "Noto Naskh Arabic",
    )));
    builder.push_default(StyleProperty::FontSize(24.0));
    builder.push_default(StyleProperty::WordBreak(crate::WordBreak::BreakAll));
    if physical {
        builder.push_inline_owner_shaping(owner(
            12..text.len(),
            PhysicalLineEdge::Right,
            ShapingEdgePlacement::FirstLine,
        ));
    }
    if explicit {
        builder.push_shaping_boundary(12);
    }
    builder.build(text)
}

#[test]
fn physical_edge_glyphs_match_an_independent_explicit_boundary() {
    let mut actual = arabic_layout(true, false);
    let mut expected = arabic_layout(false, true);
    actual.break_all_lines(None);
    expected.break_all_lines(None);
    assert_eq!(glyphs(&actual), glyphs(&expected));
    assert_eq!(actual.width(), expected.width());
}

#[test]
fn physical_shape_is_selected_before_the_terminal_line_is_fitted() {
    let mut joined = arabic_layout(false, false);
    let mut expected = arabic_layout(false, true);
    joined.break_all_lines(None);
    expected.break_all_lines(None);
    assert_ne!(joined.width(), expected.width());
    let measure = (joined.width() + expected.width()) / 2.0;
    joined.break_all_lines(Some(measure));
    expected.break_all_lines(Some(measure));
    assert_ne!(
        joined.len(),
        expected.len(),
        "the reference must change line selection"
    );
    let mut actual = arabic_layout(true, false);
    actual.break_all_lines(Some(measure));
    assert_eq!(actual.len(), expected.len());
    assert_eq!(glyphs(&actual), glyphs(&expected));
}

#[test]
fn physical_shapes_reflow_without_retaining_rejected_glyphs() {
    let mut layout = arabic_layout(true, false);
    for measure in [None, Some(100.0), None, Some(100.0)] {
        let mut reference = arabic_layout(false, true);
        reference.break_all_lines(measure);
        layout.break_all_lines(measure);
        assert_eq!(glyphs(&layout), glyphs(&reference));
        let glyph_count = layout.data.glyphs.len();
        layout.break_all_lines(measure);
        assert_eq!(layout.data.glyphs.len(), glyph_count);
        assert_eq!(glyphs(&layout), glyphs(&reference));
    }
}

#[test]
fn a_reverted_physical_line_can_be_refitted_at_a_new_width() {
    let mut layout = arabic_layout(true, false);
    let mut reference = arabic_layout(false, true);
    reference.break_all_lines(None);
    let mut breaker = layout.break_lines();
    assert!(breaker.break_next(100.0, LineTabOrigin::ZERO).is_some());
    assert!(breaker.revert());
    breaker.break_remaining(f32::MAX);
    assert_eq!(glyphs(&layout), glyphs(&reference));
    assert_eq!(layout.width(), reference.width());
}

#[test]
fn physical_boundaries_preserve_optional_justification_and_feature_ranges() {
    use super::test_justification_shapes::{office_layout_configured, prepare};
    let mut fonts = naskh_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let text = "office office office";
    let mut actual = office_layout_configured(&mut context, &mut fonts, text, false, |builder| {
        builder.push_inline_owner_shaping(owner(
            1..3,
            PhysicalLineEdge::Right,
            ShapingEdgePlacement::FirstLine,
        ));
    });
    let mut expected = office_layout_configured(&mut context, &mut fonts, text, false, |builder| {
        builder.push_shaping_boundary(3);
    });
    for terminal_lines in [true, false] {
        for layout in [&mut actual, &mut expected] {
            prepare(&mut context, layout, &(1..text.len()).collect::<Vec<_>>());
            layout.set_justification_shape_policy(crate::JustificationShapePolicy {
                regular_lines: true,
                terminal_lines,
            });
            layout.break_all_lines(Some(85.0));
        }
        assert_eq!(glyphs(&actual), glyphs(&expected));
        assert_eq!(actual.width(), expected.width());
    }
}

#[test]
fn character_count_breaking_uses_the_selected_physical_glyphs() {
    let mut actual = arabic_layout(true, false);
    let mut expected = arabic_layout(false, true);
    for layout in [&mut actual, &mut expected] {
        let mut breaker = layout.break_lines();
        assert_eq!(breaker.break_next_with_length(u32::MAX), Some(()));
        breaker.finish();
    }
    assert_eq!(glyphs(&actual), glyphs(&expected));
    assert_eq!(actual.width(), expected.width());
}

#[test]
fn unavoidable_physical_overflow_consumes_each_source_character_once() {
    let mut actual = arabic_layout(true, false);
    actual.break_all_lines(Some(0.0));
    let mut end = 0;
    for line in actual.lines() {
        assert_eq!(line.text_range().start, end);
        assert!(line.text_range().end > end);
        end = line.text_range().end;
    }
    assert_eq!(end, "السلامعليكم".len());
    let first = glyphs(&actual);
    actual.break_all_lines(Some(0.0));
    assert_eq!(glyphs(&actual), first);
}

#[test]
fn intrinsic_maximum_uses_physical_glyphs_without_changing_the_layout() {
    let actual = arabic_layout(true, false);
    let before = actual.clone();
    let mut expected = arabic_layout(false, true);
    expected.break_all_lines(None);
    assert_eq!(actual.calculate_content_widths().max, expected.width());
    assert_eq!(actual, before);
}

#[test]
fn zero_advance_owner_edges_keep_intrinsic_physical_boundaries() {
    let text = "السلامعليكم";
    let mut fonts = naskh_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut build = |physical| {
        let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
        builder.set_direction(BaseDirection::Ltr);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(
            "Noto Naskh Arabic",
        )));
        builder.push_default(StyleProperty::FontSize(24.0));
        builder.push_inline_box(
            InlineBox::inline_start_edge(41, 0, 0.0, 0.0, InlineBoxBreakAffinity::ToNext)
                .with_continuous_shaping(),
        );
        builder.push_inline_box(
            InlineBox::inline_end_edge(42, 12, 0.0, 0.0, InlineBoxBreakAffinity::ToPrevious)
                .with_continuous_shaping(),
        );
        if physical {
            let mut owner = owner(
                0..12,
                PhysicalLineEdge::Left,
                ShapingEdgePlacement::LastLine,
            );
            owner.inline_boxes = alloc::vec![41, 42];
            builder.push_inline_owner_shaping(owner);
        } else {
            builder.push_shaping_boundary(12);
        }
        builder.build(text)
    };
    let mut actual = build(true);
    let mut reference = build(false);
    assert_eq!(
        actual.calculate_content_widths().max,
        reference.calculate_content_widths().max
    );
    actual.break_all_lines(None);
    reference.break_all_lines(None);
    assert_eq!(glyphs(&actual), glyphs(&reference));
}

#[test]
fn a_line_of_only_an_owner_edge_fits_without_a_text_start() {
    let text = "\u{56fd}\u{56fd}XX\u{56fd}";
    for direction in [BaseDirection::Ltr, BaseDirection::Rtl] {
        let mut fonts = create_font_context();
        let mut context = LayoutContext::<ColorBrush>::new();
        let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
        builder.set_direction(direction);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.push_inline_box(
            InlineBox::inline_start_edge(41, 6, 5.0, 0.0, InlineBoxBreakAffinity::ToNext)
                .with_continuous_shaping(),
        );
        builder.push_inline_box(
            InlineBox::inline_end_edge(42, 8, 5.0, 0.0, InlineBoxBreakAffinity::ToPrevious)
                .with_continuous_shaping(),
        );
        builder.push_inline_owner_shaping(InlineOwnerShaping {
            text: 6..8,
            inline_boxes: vec![41, 42],
            edges: vec![
                InlineShapingEdge {
                    side: PhysicalLineEdge::Left,
                    placement: ShapingEdgePlacement::FirstLine,
                },
                InlineShapingEdge {
                    side: PhysicalLineEdge::Right,
                    placement: ShapingEdgePlacement::LastLine,
                },
            ],
        });
        let mut layout = builder.build(text);
        layout.calculate_content_widths();
        layout.break_all_lines(Some(0.0));
        let ranges = || layout.lines().map(|line| line.text_range());
        assert_eq!(
            (
                ranges().map(|range| range.start).min(),
                ranges().map(|range| range.end).max()
            ),
            (Some(0), Some(text.len()))
        );
    }
}

#[test]
fn geometry_only_owner_has_no_text_to_reshape() {
    let mut fonts = naskh_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut build = |physical| {
        let mut builder = context.ranged_builder(&mut fonts, "", 1.0, false);
        builder.push_inline_box(InlineBox::inline_start_edge(
            41,
            0,
            10.0,
            0.0,
            InlineBoxBreakAffinity::ToNext,
        ));
        if physical {
            let mut owner = owner(
                0..0,
                PhysicalLineEdge::Left,
                ShapingEdgePlacement::FirstLine,
            );
            owner.inline_boxes = alloc::vec![41];
            builder.push_inline_owner_shaping(owner);
        }
        builder.build("")
    };
    let mut actual = build(true);
    let mut reference = build(false);
    let actual_widths = actual.calculate_content_widths();
    let reference_widths = reference.calculate_content_widths();
    assert_eq!(actual_widths.min, reference_widths.min);
    assert_eq!(actual_widths.max, reference_widths.max);
    actual.break_all_lines(None);
    reference.break_all_lines(None);
    assert_eq!(actual.width(), reference.width());
    assert_eq!(actual.len(), reference.len());
}

#[test]
fn physical_intrinsics_distinguish_anywhere_from_break_word() {
    use super::test_justification_shapes::office_layout_configured;
    let mut fonts = naskh_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    for wrap in [
        crate::OverflowWrap::Normal,
        crate::OverflowWrap::BreakWord,
        crate::OverflowWrap::Anywhere,
    ] {
        let make = |builder: &mut crate::RangedBuilder<'_, ColorBrush>| {
            builder.push_default(StyleProperty::OverflowWrap(wrap));
        };
        let reference = office_layout_configured(&mut context, &mut fonts, "abcd", false, make);
        let actual = office_layout_configured(&mut context, &mut fonts, "abcd", false, |builder| {
            make(builder);
            builder.push_inline_owner_shaping(owner(
                0..4,
                PhysicalLineEdge::Left,
                ShapingEdgePlacement::FirstLine,
            ));
        });
        let expected = reference.calculate_content_widths();
        let measured = actual.calculate_content_widths();
        assert_eq!(measured.min, expected.min, "{wrap:?}");
        assert_eq!(measured.max, expected.max, "{wrap:?}");
        let mut laid_out = actual.clone();
        laid_out.set_line_break_purpose(crate::layout::LineBreakPurpose::IntrinsicSizing);
        laid_out.break_all_lines(Some(0.0));
        assert_eq!(laid_out.width(), expected.min, "{wrap:?}");
    }
}

const EDGED_WORD: &str = "word ";
const EDGED_WORDS: usize = 40;

/// The owner of each word: its text, or an empty milestone with an edge box on each side.
#[derive(Clone, Copy)]
enum WordOwner {
    Text,
    Milestone,
}

fn edged_words() -> Layout<ColorBrush> {
    owned_words("Roboto", EDGED_WORDS, WordOwner::Text)
}

fn owned_words(family: &str, words: usize, owner: WordOwner) -> Layout<ColorBrush> {
    let text = EDGED_WORD.repeat(words);
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, &text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    builder.push_default(StyleProperty::FontSize(10.0));
    for (word, start) in (0..text.len()).step_by(EDGED_WORD.len()).enumerate() {
        let edges = vec![
            InlineShapingEdge {
                side: PhysicalLineEdge::Left,
                placement: ShapingEdgePlacement::FirstLine,
            },
            InlineShapingEdge {
                side: PhysicalLineEdge::Right,
                placement: ShapingEdgePlacement::LastLine,
            },
        ];
        let owned = match owner {
            WordOwner::Text => InlineOwnerShaping {
                text: start..start + EDGED_WORD.trim_end().len(),
                inline_boxes: Vec::new(),
                edges,
            },
            WordOwner::Milestone => {
                let id = 2 * word as u64;
                builder.push_inline_box(
                    InlineBox::inline_start_edge(
                        id,
                        start,
                        2.0,
                        0.0,
                        InlineBoxBreakAffinity::ToNext,
                    )
                    .with_continuous_shaping(),
                );
                builder.push_inline_box(
                    InlineBox::inline_end_edge(
                        id + 1,
                        start,
                        2.0,
                        0.0,
                        InlineBoxBreakAffinity::ToPrevious,
                    )
                    .with_continuous_shaping(),
                );
                InlineOwnerShaping {
                    text: start..start,
                    inline_boxes: vec![id, id + 1],
                    edges,
                }
            }
        };
        builder.push_inline_owner_shaping(owned);
    }
    builder.build(&text)
}

fn four_edged_words_measure() -> f32 {
    four_words_measure("Roboto", EDGED_WORDS, WordOwner::Text)
}

fn four_words_measure(family: &str, words: usize, owner: WordOwner) -> f32 {
    let mut layout = owned_words(family, words, owner);
    layout.break_all_lines(None);
    layout.width() * 4.5 / words as f32
}

fn physical_line_fit_work(
    family: &str,
    words: usize,
    owner: WordOwner,
) -> crate::shape::physical::PhysicalWork {
    let measure = four_words_measure(family, words, owner);
    let mut layout = owned_words(family, words, owner);
    let mut breaker = layout.break_lines();
    while breaker.break_next(measure, LineTabOrigin::ZERO).is_some() {}
    breaker.physical_work()
}

#[test]
fn a_physical_line_fit_shapes_source_in_proportion_to_its_lines() {
    let short = physical_line_fit_work("Roboto Flex", 80, WordOwner::Text);
    let long = physical_line_fit_work("Roboto Flex", 160, WordOwner::Text);
    assert!(
        long.shaped_source * 10 <= short.shaped_source * 22,
        "{short:?} to {long:?}"
    );
}

#[test]
fn a_physical_line_fit_installs_clusters_in_proportion_to_its_lines() {
    let short = physical_line_fit_work("Roboto Flex", 80, WordOwner::Text);
    let long = physical_line_fit_work("Roboto Flex", 160, WordOwner::Text);
    assert!(
        long.installed_clusters * 10 <= short.installed_clusters * 22,
        "{short:?} to {long:?}"
    );
}

#[test]
fn a_physical_line_fit_collects_safe_boundaries_in_proportion_to_its_lines() {
    let short = physical_line_fit_work("Roboto", 80, WordOwner::Text);
    let long = physical_line_fit_work("Roboto", 160, WordOwner::Text);
    assert!(
        long.safe_boundaries * 10 <= short.safe_boundaries * 22,
        "{short:?} to {long:?}"
    );
}

#[test]
fn a_physical_line_fit_releases_segments_in_proportion_to_its_lines() {
    let short = physical_line_fit_work("Roboto Flex", 80, WordOwner::Text);
    let long = physical_line_fit_work("Roboto Flex", 160, WordOwner::Text);
    assert!(
        long.released_segments * 10 <= short.released_segments * 22,
        "{short:?} to {long:?}"
    );
}

#[test]
fn physical_boundaries_visit_owners_in_proportion_to_the_line() {
    let visits = |words: usize| {
        let mut layout = owned_words("Roboto", words, WordOwner::Milestone);
        let mut breaker = layout.break_lines();
        while breaker.break_next(f32::MAX, LineTabOrigin::ZERO).is_some() {}
        breaker.physical_work().owner_visits
    };
    let short = visits(80);
    let long = visits(160);
    assert!(long * 10 <= short * 22, "{short} to {long}");
}

#[test]
fn a_physical_line_fit_shapes_no_end_after_the_first_overflowing_word() {
    let measure = four_edged_words_measure();
    let mut reference = edged_words();
    reference.break_all_lines(Some(measure));
    let first_line_end = reference.lines().next().unwrap().text_range().end;
    let overflowing_word_end = first_line_end + EDGED_WORD.len();
    let mut layout = edged_words();
    let mut breaker = layout.break_lines();
    assert!(
        breaker
            .preview_shape_candidate(measure, LineTabOrigin::ZERO)
            .is_some()
    );
    let shaped = breaker.shaped_line_boundaries();
    assert!(
        shaped.iter().all(|end| *end <= overflowing_word_end),
        "{shaped:?} after {overflowing_word_end}"
    );
}

#[test]
fn a_committed_physical_line_keeps_no_soft_boundary_window() {
    let measure = four_edged_words_measure();
    let mut layout = edged_words();
    let mut breaker = layout.break_lines();
    assert!(breaker.break_next(measure, LineTabOrigin::ZERO).is_some());
    assert_eq!(breaker.shaped_line_boundaries(), BTreeSet::new());
}
