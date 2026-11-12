// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::tests::test_bidi_topology::build_layout_with_direction;
use crate::{
    BaseDirection, InlineBox, InlineBoxBreakAffinity, InlineBoxSourceKind, InlineOwnerEdgeAdvance,
    PositionedLayoutItem,
};
use crate::{Layout, ParallelInlineError, PreparedParallelInline, PreparedParallelRow};
use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

fn row(text: &str, cuts: Vec<usize>) -> PreparedParallelRow<crate::tests::utils::ColorBrush> {
    sized_row(text, cuts, 10.0)
}

fn sized_row(
    text: &str,
    cuts: Vec<usize>,
    font_size: f32,
) -> PreparedParallelRow<crate::tests::utils::ColorBrush> {
    let prepared = PreparedParallelRow::new(sized_row_layout(text, font_size, []), cuts).unwrap();
    assert!(
        prepared
            .layout
            .data
            .styles
            .iter()
            .all(|style| style.text_wrap_mode == crate::TextWrapMode::NoWrap)
    );
    prepared
}

fn sized_row_layout(
    text: &str,
    font_size: f32,
    inline_boxes: impl IntoIterator<Item = InlineBox>,
) -> Layout<crate::tests::utils::ColorBrush> {
    let mut fonts = crate::tests::test_builders::create_font_context();
    let mut context = crate::LayoutContext::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(crate::StyleProperty::FontFamily(crate::FontFamily::named(
        "Roboto",
    )));
    builder.push_default(crate::StyleProperty::FontSize(font_size));
    builder.push_default(crate::StyleProperty::TextWrapMode(
        crate::TextWrapMode::NoWrap,
    ));
    for inline_box in inline_boxes {
        builder.push_inline_box(inline_box);
    }
    builder.build(text)
}

fn group() -> PreparedParallelInline<crate::tests::utils::ColorBrush> {
    PreparedParallelInline::new(vec![row("MMMM", vec![2]), row("MMMM", vec![1, 2, 3])]).unwrap()
}

fn fragments(
    layout: &Layout<crate::tests::utils::ColorBrush>,
) -> Vec<(usize, f32, Vec<Range<usize>>)> {
    layout
        .lines()
        .enumerate()
        .flat_map(|(line, data)| {
            data.items().filter_map(move |item| {
                let PositionedLayoutItem::InlineBox(item) = item else {
                    return None;
                };
                item.parallel_fragment()
                    .map(|fragment| (line, item.width, fragment.rows().to_vec()))
            })
        })
        .collect()
}

#[test]
fn parallel_rows_fit_actual_prefixes_and_rebreak_without_losing_source() {
    for direction in [BaseDirection::Ltr, BaseDirection::Rtl] {
        let group = group();
        let full = group.full_advance();
        let mut layout = build_layout_with_direction(
            "",
            [
                InlineBox::new(1, 0, full / 4.0, 10.0),
                InlineBox::new(2, 0, full, 10.0),
            ],
            Some(direction),
        );
        layout.set_parallel_inline_groups(vec![(2, group)]).unwrap();
        for width in [full * 0.75, full * 2.0, full * 0.75] {
            layout.break_all_lines(Some(width));
            let selected = fragments(&layout);
            if width > full {
                assert_eq!(selected.len(), 1);
                assert_eq!(selected[0].2, [0..2, 0..4]);
            } else {
                assert_eq!(selected.len(), 2);
                assert_eq!(
                    selected[0].0, 0,
                    "first piece must use the actual prefix's remaining space"
                );
                assert_eq!(selected[0].2, [0..1, 0..2]);
                assert_eq!(selected[1].2, [1..2, 2..4]);
            }
            assert!(
                selected
                    .iter()
                    .all(|(_, advance, _)| advance.is_finite() && *advance <= width),
                "direction={direction:?}, capacity={width}, full={full}, selected={selected:?}"
            );
        }
    }
}

#[test]
fn full_parallel_group_keeps_an_internal_candidate_for_following_punctuation() {
    let group = group();
    let full = group.full_advance();
    let mut layout = build_layout_with_direction(
        ")",
        [
            InlineBox::atomic_with_break_affinity(9, 0, full, 10.0, InlineBoxBreakAffinity::ToNext)
                .with_source_kind(InlineBoxSourceKind::InlineContent),
        ],
        Some(BaseDirection::Ltr),
    );
    layout.set_parallel_inline_groups(vec![(9, group)]).unwrap();
    layout.break_all_lines(Some(full));
    let selected = fragments(&layout);
    assert_eq!(layout.len(), 2);
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0].2, [0..1, 0..2]);
    assert_eq!(selected[1].2, [1..2, 2..4]);
    assert_eq!(layout.get(1).unwrap().text_range(), 0..1);
}

#[test]
fn parallel_intrinsics_use_paired_ranges_instead_of_summed_atomic_chunks() {
    let group =
        PreparedParallelInline::new(vec![row("MMii", vec![2]), row("iiMM", vec![2])]).unwrap();
    let full = group.full_advance();
    let mut layout = build_layout_with_direction("", [InlineBox::new(3, 0, full, 10.0)], None);
    layout.set_parallel_inline_groups(vec![(3, group)]).unwrap();
    let widths = layout.calculate_content_widths();
    assert!(
        widths.min < widths.max,
        "paired legal prefixes must reduce min-content"
    );
    assert!((widths.max - full).abs() < 0.01);
    layout.break_all_lines(Some(full));
    assert_eq!(
        fragments(&layout).len(),
        1,
        "a complete fitting group is not pre-split into independently widened chunks"
    );
    layout.break_all_lines(Some(0.0));
    let selected = fragments(&layout);
    assert_eq!(selected.len(), 2);
    assert!(selected.iter().map(|(_, width, _)| width).sum::<f32>() > full);
}

#[test]
fn a_row_without_an_internal_cut_keeps_the_parallel_group_indivisible() {
    let group =
        PreparedParallelInline::new(vec![row("MMMM", vec![2]), row("MMMM", vec![])]).unwrap();
    let full = group.full_advance();
    let mut layout = build_layout_with_direction("", [InlineBox::new(4, 0, full, 10.0)], None);
    layout.set_parallel_inline_groups(vec![(4, group)]).unwrap();
    layout.break_all_lines(Some(full / 2.0));
    assert_eq!(fragments(&layout).len(), 1);
    assert!((layout.calculate_content_widths().min - full).abs() < 0.01);
}

#[test]
fn invalid_parallel_sources_and_participant_bindings_are_rejected_atomically() {
    assert!(matches!(
        PreparedParallelRow::new(build_layout_with_direction("á", [], None), vec![1]),
        Err(ParallelInlineError::InvalidCut)
    ));
    assert!(matches!(
        PreparedParallelRow::new(build_layout_with_direction("office", [], None), vec![2]),
        Err(ParallelInlineError::InvalidCut)
    ));
    let mut layout = build_layout_with_direction("", [InlineBox::new(5, 0, 40.0, 10.0)], None);
    layout
        .set_parallel_inline_groups(vec![(5, group())])
        .unwrap();
    assert_eq!(
        layout.set_parallel_inline_groups(vec![(6, group())]),
        Err(ParallelInlineError::InvalidParticipant)
    );
    assert_eq!(layout.data.parallel_inline_groups.len(), 1);
    assert_eq!(
        layout.set_parallel_inline_groups(vec![(5, group()), (5, group())]),
        Err(ParallelInlineError::InvalidParticipant)
    );
}
#[test]
fn differently_sized_parallel_rows_rewind_to_equal_native_prefix_advances() {
    for direction in [BaseDirection::Ltr, BaseDirection::Rtl] {
        let group = PreparedParallelInline::new(vec![
            sized_row("mmmmm", (1..5).collect(), 20.0),
            sized_row("mmmmmmmmmm", (1..10).collect(), 10.0),
        ])
        .unwrap();
        let full = group.full_advance();
        let mut layout = build_layout_with_direction(
            ")",
            [InlineBox::atomic_with_break_affinity(
                17,
                0,
                full,
                20.0,
                InlineBoxBreakAffinity::ToNext,
            )
            .with_source_kind(InlineBoxSourceKind::InlineContent)],
            Some(direction),
        );
        layout
            .set_parallel_inline_groups(vec![(17, group)])
            .unwrap();
        layout.break_all_lines(Some(full));
        let selected = fragments(&layout);
        assert_eq!(layout.len(), 2);
        assert_eq!(selected[0].2, [0..4, 0..8]);
        assert_eq!(selected[1].2, [4..5, 8..10]);
        assert!(layout.lines().all(|line| line.measured_advance() <= full));
    }
}

#[test]
fn parallel_source_space_cuts_use_the_rows_terminal_whitespace_policy() {
    let group =
        PreparedParallelInline::new(vec![row("AA BB", vec![3]), row("cc dd", vec![3])]).unwrap();
    let mut first_base = build_layout_with_direction("AA", [], None);
    first_base.break_all_lines(None);
    let mut first_annotation = build_layout_with_direction("cc", [], None);
    first_annotation.break_all_lines(None);
    let first_width = first_base.width().max(first_annotation.width());
    let full = group.full_advance();
    let mut layout = build_layout_with_direction("", [InlineBox::new(18, 0, full, 10.0)], None);
    layout
        .set_parallel_inline_groups(vec![(18, group)])
        .unwrap();
    for measure in [first_width, full, first_width] {
        layout.break_all_lines(Some(measure));
        let selected = fragments(&layout);
        if measure < full {
            assert_eq!(selected.len(), 2);
            assert_eq!(selected[0].2, [0..1, 0..1]);
            assert_eq!(selected[1].2, [1..2, 1..2]);
            assert!(
                (selected[0].1 - first_width).abs() < 0.01,
                "a selected trailing source space must not inflate fragment width: capacity={measure}, expected={first_width}, full={full}, selected={selected:?}"
            );
        } else {
            assert_eq!(selected.len(), 1);
        }
    }
}

#[test]
fn minimum_parallel_measure_advances_every_row_to_the_feasible_shared_extent() {
    let group = PreparedParallelInline::new(vec![
        sized_row("MMMM", (1..4).collect(), 20.0),
        sized_row("MMMMMMMM", (1..8).collect(), 10.0),
    ])
    .unwrap();
    let full = group.full_advance();
    let minimum = full / 4.0;
    let mut layout = build_layout_with_direction("", [InlineBox::new(19, 0, full, 20.0)], None);
    layout
        .set_parallel_inline_groups(vec![(19, group)])
        .unwrap();
    let intrinsic = layout.calculate_content_widths();
    assert!(
        (intrinsic.min - minimum).abs() < 0.01,
        "minimum={minimum}, actual={intrinsic:?}"
    );
    assert!((intrinsic.max - full).abs() < 0.01);
    layout.break_all_lines(Some(0.0));
    let selected = fragments(&layout);
    assert_eq!(selected.len(), 4);
    for (index, (_, advance, rows)) in selected.iter().enumerate() {
        assert_eq!(rows, &[index..index + 1, 2 * index..2 * index + 2]);
        assert!((*advance - minimum).abs() < 0.01, "selected={selected:?}");
    }
}

#[test]
fn unequal_full_rows_balance_remaining_native_progress_without_stranding_a_tail() {
    let group = PreparedParallelInline::new(vec![
        row("MMMMMM", (1..6).collect()),
        row("MMMMMMMMMMMM", (1..12).collect()),
    ])
    .unwrap();
    let full = group.full_advance();
    let measure = full / 2.0;
    let mut layout = build_layout_with_direction("", [InlineBox::new(20, 0, full, 10.0)], None);
    layout
        .set_parallel_inline_groups(vec![(20, group)])
        .unwrap();
    layout.break_all_lines(Some(measure));
    let selected = fragments(&layout);
    assert_eq!(
        selected.len(),
        2,
        "measure={measure}, selected={selected:?}"
    );
    assert_eq!(selected[0].2, [0..3, 0..6]);
    assert_eq!(selected[1].2, [3..6, 6..12]);
    assert!(
        selected.iter().all(|(_, width, _)| *width <= measure),
        "selected={selected:?}"
    );
    let intrinsic = layout.calculate_content_widths();
    assert!(
        (intrinsic.min - full / 6.0).abs() < 0.01,
        "intrinsic={intrinsic:?}, full={full}"
    );
}

#[test]
fn zero_owner_edges_keep_their_order_and_source_cuts_through_rebreaking() {
    for (text, starts_and_ends, cut) in [
        ("MMMMMM", [(0, 3), (3, 6)], 3),
        ("AA BB", [(0, 2), (3, 5)], 3),
    ] {
        let mut edges = Vec::new();
        for (index, (start, end)) in starts_and_ends.into_iter().enumerate() {
            edges.push(InlineBox::inline_start_edge(
                100 + 2 * index as u64,
                start,
                InlineOwnerEdgeAdvance::uniform(0.0),
                0.0,
                InlineBoxBreakAffinity::Both,
            ));
            edges.push(InlineBox::inline_end_edge_with_following_source_space(
                101 + 2 * index as u64,
                end,
                InlineOwnerEdgeAdvance::uniform(0.0),
                0.0,
                crate::FollowingSourceSpace::CollapsedAfterProjectedBreak,
                InlineBoxBreakAffinity::Both,
            ));
        }
        let row = PreparedParallelRow::new(sized_row_layout(text, 10.0, edges.clone()), vec![cut])
            .unwrap();
        let retained = row
            .layout
            .inline_boxes()
            .iter()
            .map(|edge| (edge.id, edge.index))
            .collect::<Vec<_>>();
        assert_eq!(
            retained,
            edges
                .iter()
                .map(|edge| (edge.id, edge.index))
                .collect::<Vec<_>>()
        );
        let mut prefix = sized_row_layout(text[..cut].trim_end(), 10.0, []);
        prefix.break_all_lines(None);
        let minimum = prefix.width();
        let group = PreparedParallelInline::new(vec![row.clone(), row.clone()]).unwrap();
        let full = group.full_advance();
        let mut layout = build_layout_with_direction("", [InlineBox::new(21, 0, full, 10.0)], None);
        layout
            .set_parallel_inline_groups(vec![(21, group)])
            .unwrap();
        for measure in [minimum, full, minimum] {
            layout.break_all_lines(Some(measure));
            let selected = fragments(&layout);
            if measure < full {
                assert_eq!(selected.len(), 2, "text={text:?}, selected={selected:?}");
                assert_eq!(selected[0].2, [0..1, 0..1]);
                assert_eq!(selected[1].2, [1..2, 1..2]);
                assert!(
                    (selected[0].1 - minimum).abs() < 0.01,
                    "text={text:?}, selected={selected:?}"
                );
            } else {
                assert_eq!(selected.len(), 1);
            }
        }
        assert_eq!(
            row.layout
                .inline_boxes()
                .iter()
                .map(|edge| (edge.id, edge.index))
                .collect::<Vec<_>>(),
            retained
        );
    }
    for unsupported in [
        InlineBox::new(300, 0, 0.0, 0.0),
        InlineBox::contextual_spacing(301, 0, 0.0, 0.0),
        InlineBox::inline_start_edge(
            302,
            0,
            InlineOwnerEdgeAdvance::uniform(1.0),
            0.0,
            InlineBoxBreakAffinity::Both,
        ),
        InlineBox::inline_end_edge(
            303,
            2,
            InlineOwnerEdgeAdvance::uniform(0.0)
                .with_terminal_preserved_space_reduction(1.0, 1.0, 0),
            0.0,
            InlineBoxBreakAffinity::Both,
        ),
        InlineBox::new(304, 0, 0.0, 0.0).with_terminal_source_space_reduction(0.0),
    ] {
        let result =
            PreparedParallelRow::new(sized_row_layout("MM", 10.0, [unsupported.clone()]), vec![1]);
        assert!(
            matches!(result, Err(ParallelInlineError::UnsupportedRow)),
            "unsupported={unsupported:?}, result={:?}",
            result.as_ref().map(|row| row.boundaries())
        );
    }
}
