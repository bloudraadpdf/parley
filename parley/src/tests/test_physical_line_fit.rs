// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

use super::utils::visual_glyphs as glyphs;
use super::{
    test_physical_shaping::owner, test_shaping_context::naskh_font_context, utils::ColorBrush,
};
use crate::layout::{PhysicalLineEdge, ShapingEdgePlacement};
use crate::{BaseDirection, FontFamily, Layout, LayoutContext, StyleProperty};

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
    assert!(
        breaker
            .break_next(100.0, crate::LineTabOrigin::ZERO)
            .is_some()
    );
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
            crate::InlineBox::inline_start_edge(
                41,
                0,
                0.0,
                0.0,
                crate::InlineBoxBreakAffinity::ToNext,
            )
            .with_continuous_shaping(),
        );
        builder.push_inline_box(
            crate::InlineBox::inline_end_edge(
                42,
                12,
                0.0,
                0.0,
                crate::InlineBoxBreakAffinity::ToPrevious,
            )
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
fn geometry_only_owner_has_no_text_to_reshape() {
    let mut fonts = naskh_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut build = |physical| {
        let mut builder = context.ranged_builder(&mut fonts, "", 1.0, false);
        builder.push_inline_box(crate::InlineBox::inline_start_edge(
            41,
            0,
            10.0,
            0.0,
            crate::InlineBoxBreakAffinity::ToNext,
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
