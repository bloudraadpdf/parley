// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{vec, vec::Vec};
use core::ops::Range;

use super::test_bidi_topology::build_layout_with_direction;
use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::layout::{
    InlineOwnerShaping, InlineShapingEdge, PhysicalLineEdge, ShapingEdgePlacement,
};
use crate::{BaseDirection, InlineBox, InlineBoxBreakAffinity};

pub(super) fn owner(
    text: Range<usize>,
    side: PhysicalLineEdge,
    placement: ShapingEdgePlacement,
) -> InlineOwnerShaping {
    InlineOwnerShaping {
        text,
        inline_boxes: Vec::new(),
        edges: vec![InlineShapingEdge { side, placement }],
    }
}

#[test]
fn arabic_physical_sides_do_not_follow_the_paragraph_direction() {
    for direction in [BaseDirection::Ltr, BaseDirection::Rtl] {
        let mut layout = build_layout_with_direction("ععع", [], Some(direction));
        layout.break_all_lines(None);
        let line = layout.lines().next().unwrap();
        for (side, expected) in [(PhysicalLineEdge::Left, 6), (PhysicalLineEdge::Right, 2)] {
            let owner = owner(2..6, side, ShapingEdgePlacement::FirstLine);
            assert_eq!(line.physical_shaping_boundaries(&[owner]), [expected]);
        }
    }
}

#[test]
fn source_first_edge_is_absent_after_a_forced_break_but_clone_is_present() {
    let mut layout = build_layout_with_direction("ع\nعع", [], Some(BaseDirection::Ltr));
    layout.break_all_lines(None);
    check_fragmented_owner(&layout);
}

fn check_fragmented_owner(layout: &crate::Layout<ColorBrush>) {
    let lines = layout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    let sliced = owner(
        0..5,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::FirstLine,
    );
    assert!(lines[1].physical_shaping_boundaries(&[sliced]).is_empty());
    let cloned = owner(
        0..5,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::EveryFragment,
    );
    assert_eq!(lines[1].physical_shaping_boundaries(&[cloned]), [5]);
    let end = owner(
        0..5,
        PhysicalLineEdge::Right,
        ShapingEdgePlacement::LastLine,
    );
    assert!(
        lines[0]
            .physical_shaping_boundaries(core::slice::from_ref(&end))
            .is_empty()
    );
    assert_eq!(lines[1].physical_shaping_boundaries(&[end]), [3]);
}

#[test]
fn source_first_edge_is_absent_after_a_soft_wrap_but_clone_is_present() {
    let mut word = build_layout_with_direction("عع", [], Some(BaseDirection::Ltr));
    word.break_all_lines(None);
    let mut layout = build_layout_with_direction("ع عع", [], Some(BaseDirection::Ltr));
    layout.break_all_lines(Some(word.width()));
    check_fragmented_owner(&layout);
}

#[test]
fn nested_owner_edges_retain_separate_boundaries() {
    let mut layout = build_layout_with_direction("ععع", [], Some(BaseDirection::Ltr));
    layout.break_all_lines(None);
    let owners = [
        owner(
            0..4,
            PhysicalLineEdge::Left,
            ShapingEdgePlacement::FirstLine,
        ),
        owner(
            2..4,
            PhysicalLineEdge::Right,
            ShapingEdgePlacement::LastLine,
        ),
    ];
    assert_eq!(
        layout
            .lines()
            .next()
            .unwrap()
            .physical_shaping_boundaries(&owners),
        [2, 4]
    );
}

#[test]
fn atomics_at_the_same_byte_offset_keep_distinct_owner_membership() {
    let mut layout = build_layout_with_direction(
        "ab",
        [
            InlineBox::new(11, 1, 10.0, 10.0),
            InlineBox::new(12, 1, 10.0, 10.0),
        ],
        Some(BaseDirection::Ltr),
    );
    let mut breaker = layout.break_lines();
    while breaker.break_next_with_length(2).is_some() {}
    breaker.finish();
    let mut owned = owner(
        1..1,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::FirstLine,
    );
    owned.inline_boxes.push(11);
    let lines = layout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0].physical_shaping_boundaries(core::slice::from_ref(&owned)),
        [1]
    );
    assert!(lines[1].physical_shaping_boundaries(&[owned]).is_empty());
}

#[test]
fn empty_owner_edges_break_context_even_with_zero_total_advance() {
    let boxes = [
        InlineBox::inline_start_edge(11, 1, 0.0, 0.0, InlineBoxBreakAffinity::ToNext),
        InlineBox::inline_end_edge(12, 1, 0.0, 0.0, InlineBoxBreakAffinity::ToPrevious),
    ];
    let mut layout = build_layout_with_direction("ab", boxes, Some(BaseDirection::Ltr));
    layout.break_all_lines(None);
    let mut owned = owner(
        1..1,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::FirstLine,
    );
    owned.inline_boxes.extend([11, 12]);
    assert_eq!(
        layout
            .lines()
            .next()
            .unwrap()
            .physical_shaping_boundaries(&[owned]),
        [1]
    );
}

#[test]
fn cloned_edges_apply_to_each_bidi_fragment() {
    let mut layout = build_layout_with_direction("aععb", [], Some(BaseDirection::Ltr));
    layout.break_all_lines(None);
    let line = layout.lines().next().unwrap();
    let sliced = owner(
        0..3,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::FirstLine,
    );
    assert_eq!(line.physical_shaping_boundaries(&[sliced]), [0]);
    let cloned = owner(
        0..3,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::EveryFragment,
    );
    assert_eq!(line.physical_shaping_boundaries(&[cloned]), [0, 3]);
}

#[test]
fn an_owner_without_breaking_components_has_no_boundaries() {
    let mut layout = build_layout_with_direction("ععع", [], Some(BaseDirection::Ltr));
    layout.break_all_lines(None);
    let owned = InlineOwnerShaping {
        text: 2..6,
        inline_boxes: Vec::new(),
        edges: Vec::new(),
    };
    assert!(
        layout
            .lines()
            .next()
            .unwrap()
            .physical_shaping_boundaries(&[owned])
            .is_empty()
    );
}

#[test]
fn physical_owners_retain_source_without_an_optional_ligature_policy() {
    let mut fonts = create_font_context();
    let mut context = crate::LayoutContext::<ColorBrush>::new();
    let text = "abcععع";
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(crate::StyleProperty::FontFamily(crate::FontFamily::named(
        "Roboto",
    )));
    builder.push_inline_owner_shaping(owner(
        3..text.len(),
        PhysicalLineEdge::Right,
        ShapingEdgePlacement::LastLine,
    ));
    let layout = builder.build(text);
    assert!(!layout.has_justification_shape_candidates());
    assert_eq!(layout.data.shaping_source_text.as_deref(), Some(text));
    assert!(!layout.data.deferred_physical_shapes.is_empty());
    assert_eq!(
        layout.data.deferred_physical_shapes.len(),
        layout.data.runs.len()
    );
    for (index, source) in layout.data.deferred_physical_shapes.iter().enumerate() {
        let run = &layout.data.runs[index];
        assert_eq!(source.run_index, index);
        assert_eq!(
            source.character_infos.len(),
            text[run.text_range.clone()].chars().count()
        );
        assert_eq!(source.context, 0..text.len());
    }
    let plain = context
        .ranged_builder(&mut fonts, text, 1.0, false)
        .build(text);
    assert!(plain.data.inline_owner_shaping.is_empty());
    assert!(plain.data.deferred_physical_shapes.is_empty());
    assert!(plain.data.shaping_source_text.is_none());
}

#[test]
#[should_panic(expected = "inline owners must use UTF-8 source ranges")]
fn physical_owner_membership_rejects_a_partial_codepoint() {
    let mut fonts = create_font_context();
    let mut context = crate::LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, "ع", 1.0, false);
    builder.push_inline_owner_shaping(owner(
        1..2,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::FirstLine,
    ));
    let _ = builder.build("ع");
}

#[test]
#[should_panic(expected = "inline owner boxes must occur in the source layout")]
fn physical_owner_membership_rejects_an_unknown_atomic() {
    let mut fonts = create_font_context();
    let mut context = crate::LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, "a", 1.0, false);
    let mut owned = owner(
        0..1,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::FirstLine,
    );
    owned.inline_boxes.push(99);
    builder.push_inline_owner_shaping(owned);
    let _ = builder.build("a");
}
