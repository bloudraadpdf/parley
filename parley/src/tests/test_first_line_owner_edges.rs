// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    FollowingSourceSpace, FontFamily, InlineBox, InlineBoxBreakAffinity, InlineOwnerEdgeAdvance,
    Layout, LayoutContext, PositionedLayoutItem, StyleProperty, WhiteSpaceCollapse,
};
use alloc::vec::Vec;

fn owner_layout(
    text: &str,
    edges: impl IntoIterator<Item = InlineBox>,
    width: Option<f32>,
) -> Layout<ColorBrush> {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::Preserve,
    ));
    for edge in edges {
        builder.push_inline_box(edge);
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(width);
    layout
}

fn differing_cloned_edges(text: &str) -> [InlineBox; 2] {
    InlineBox::cloned_inline_edges(
        [801, 802],
        0..text.len(),
        [[30.0, 3.0], [40.0, 4.0]]
            .map(|[normal, first_line]| InlineOwnerEdgeAdvance::first_line(normal, first_line)),
        FollowingSourceSpace::RetainedAdvance,
    )
}

fn assert_owner_lines(layout: &Layout<ColorBrush>, expected: &[&[(u64, f32)]]) {
    assert_eq!(layout.len(), expected.len());
    for (line, expected) in layout.lines().zip(expected) {
        let mut edges = Vec::new();
        let mut glyphs = 0;
        for item in line.items() {
            match item {
                PositionedLayoutItem::InlineBox(edge) => edges.push((edge.id, edge.width)),
                PositionedLayoutItem::GlyphRun(run) => {
                    assert!(run.advance() > 0.0);
                    glyphs += run.glyphs().count();
                }
            }
        }
        assert!(glyphs > 0);
        assert_eq!(&edges, expected);
        assert!(line.metrics().advance > edges.iter().map(|(_, width)| width).sum::<f32>());
    }
}

#[test]
fn cloned_owner_edges_select_explicit_and_soft_line_advances() {
    for (text, width) in [("A\nB", None), ("A B", Some(16.0))] {
        let edges = InlineBox::cloned_inline_edges(
            [801, 802],
            0..text.len(),
            [
                InlineOwnerEdgeAdvance::first_line(30.0, 3.0),
                InlineOwnerEdgeAdvance::first_line(40.0, 4.0),
            ],
            FollowingSourceSpace::RetainedAdvance,
        );
        let layout = owner_layout(text, edges, width);
        assert_owner_lines(
            &layout,
            &[&[(801, 3.0), (802, 4.0)], &[(801, 30.0), (802, 40.0)]],
        );
    }
}

#[test]
fn sliced_source_edges_select_the_line_that_contains_each_edge() {
    let text = "A\nB";
    let edges = [
        InlineBox::inline_start_edge(
            801,
            0,
            InlineOwnerEdgeAdvance::first_line(30.0, 3.0),
            0.0,
            InlineBoxBreakAffinity::ToNext,
        ),
        InlineBox::inline_end_edge(
            802,
            text.len(),
            InlineOwnerEdgeAdvance::first_line(40.0, 4.0),
            0.0,
            InlineBoxBreakAffinity::ToPrevious,
        ),
    ];
    let layout = owner_layout(text, edges, None);
    assert_owner_lines(&layout, &[&[(801, 3.0)], &[(802, 40.0)]]);
}

#[test]
fn nested_cloned_edges_keep_owner_order_and_normal_continuation_widths() {
    let text = "A\nB";
    let [outer_start, outer_end] = InlineBox::cloned_inline_edges(
        [801, 802],
        0..text.len(),
        [
            InlineOwnerEdgeAdvance::first_line(30.0, 3.0),
            InlineOwnerEdgeAdvance::first_line(40.0, 4.0),
        ],
        FollowingSourceSpace::RetainedAdvance,
    );
    let [inner_start, inner_end] = InlineBox::cloned_inline_edges(
        [803, 804],
        0..text.len(),
        [
            InlineOwnerEdgeAdvance::first_line(50.0, 5.0),
            InlineOwnerEdgeAdvance::first_line(60.0, 6.0),
        ],
        FollowingSourceSpace::RetainedAdvance,
    );
    let layout = owner_layout(text, [outer_start, inner_start, inner_end, outer_end], None);
    assert_owner_lines(
        &layout,
        &[
            &[(801, 3.0), (803, 5.0), (804, 6.0), (802, 4.0)],
            &[(801, 30.0), (803, 50.0), (804, 60.0), (802, 40.0)],
        ],
    );
}

#[test]
fn normal_cloned_line_hit_testing_uses_the_positioned_glyph_interval() {
    let text = "A\nB";
    let edges = differing_cloned_edges(text);
    let layout = owner_layout(text, edges, None);
    let line = layout.get(1).expect("normal continuation line");
    let mut selected = 0;
    for item in line.items() {
        if let PositionedLayoutItem::GlyphRun(run) = item {
            assert_eq!(run.run().text_range(), 2..3);
            assert_eq!(run.glyphs().count(), 1);
            assert!(run.advance() > 0.0);
            assert_eq!(run.offset(), 30.0);
            let point = run.offset() + run.advance() * 0.5;
            let y = (line.metrics().min_coord + line.metrics().max_coord) * 0.5;
            let (cluster, _) = crate::Cluster::from_point_exact(&layout, point, y)
                .expect("the positive B glyph interval must be hit");
            assert_eq!(cluster.text_range(), 2..3);
            assert_eq!(cluster.visual_offset(), Some(run.offset()));
            selected += 1;
        }
    }
    assert_eq!(selected, 1);
}

#[test]
fn interior_selection_on_normal_cloned_line_matches_its_positioned_glyph() {
    use crate::editing::{Cursor, Selection};
    let text = "A\nBC";
    let layout = owner_layout(text, differing_cloned_edges(text), None);
    let selection = Selection::new(
        Cursor::from_byte_index(&layout, 3, crate::Affinity::Downstream),
        Cursor::from_byte_index(&layout, 4, crate::Affinity::Upstream),
    );
    let geometry = selection.geometry(&layout);
    assert_eq!(geometry.len(), 1, "{selection:?}: {geometry:?}");
    let (rect, line_index) = geometry[0];
    assert_eq!(line_index, 1);
    assert!(rect.x0 > 30.0);
    assert!(rect.x1 > rect.x0);
    assert!(rect.y1 > rect.y0);
    let cluster = crate::Cluster::from_byte_index(&layout, 3).expect("selected C cluster");
    assert_eq!(cluster.text_range(), 3..4);
    let line = layout.get(line_index).expect("selected normal line");
    let mut selected = 0;
    for item in line.items() {
        if let PositionedLayoutItem::GlyphRun(run) = item {
            assert_eq!(run.run().text_range(), 2..4);
            assert_eq!(run.offset(), 30.0);
            let glyphs: Vec<_> = run.positioned_glyphs().collect();
            assert_eq!(glyphs.len(), 2);
            assert!(glyphs[1].advance > 0.0);
            assert_eq!(rect.x0, f64::from(glyphs[1].x));
            assert_eq!(rect.x1, rect.x0 + f64::from(glyphs[1].advance));
            selected += 1;
        }
    }
    assert_eq!(selected, 1);
}

#[test]
fn explicit_clone_intrinsic_fragments_use_independent_line_advances() {
    let text = "A\nB";
    let edges = differing_cloned_edges(text);
    let layout = owner_layout(text, edges, None);
    let widths = layout.calculate_content_widths();
    let normal = layout.get(1).expect("normal continuation line");
    assert!(normal.metrics().advance > 70.0);
    assert_eq!(widths.min, normal.metrics().advance);
    assert_eq!(widths.max, normal.metrics().advance);
}

#[test]
fn sibling_owners_select_the_line_that_contains_their_source_edges() {
    let text = "A\nB";
    let first = InlineBox::cloned_inline_edges(
        [801, 802],
        0..1,
        [
            InlineOwnerEdgeAdvance::first_line(30.0, 3.0),
            InlineOwnerEdgeAdvance::first_line(40.0, 4.0),
        ],
        FollowingSourceSpace::RetainedAdvance,
    );
    let second = InlineBox::cloned_inline_edges(
        [803, 804],
        2..3,
        [
            InlineOwnerEdgeAdvance::first_line(50.0, 5.0),
            InlineOwnerEdgeAdvance::first_line(60.0, 6.0),
        ],
        FollowingSourceSpace::RetainedAdvance,
    );
    let layout = owner_layout(text, first.into_iter().chain(second), None);
    assert_owner_lines(
        &layout,
        &[&[(801, 3.0), (802, 4.0)], &[(803, 50.0), (804, 60.0)]],
    );
}

#[test]
fn soft_clone_min_content_keeps_max_content_first_line_authority() {
    let text = "A B";
    let edges = differing_cloned_edges(text);
    let wrapped = owner_layout(text, edges.clone(), Some(16.0));
    let unwrapped = owner_layout(text, edges, None);
    assert_eq!(wrapped.len(), 2);
    assert_eq!(unwrapped.len(), 1);
    let normal = wrapped.get(1).expect("normal continuation line");
    let first = unwrapped.get(0).expect("max-content first line");
    assert!(normal.metrics().advance > 70.0);
    assert!(first.metrics().advance > 7.0);
    let widths = wrapped.calculate_content_widths();
    assert_eq!(widths.min, normal.metrics().advance);
    assert_eq!(widths.max, first.metrics().advance);
}

#[test]
fn uniform_cloned_owner_edges_keep_the_same_advances() {
    let text = "A\nB";
    let edges = InlineBox::cloned_inline_edges(
        [801, 802],
        0..text.len(),
        [3.0, 4.0].map(InlineOwnerEdgeAdvance::uniform),
        FollowingSourceSpace::RetainedAdvance,
    );
    let layout = owner_layout(text, edges, None);
    assert_owner_lines(
        &layout,
        &[&[(801, 3.0), (802, 4.0)], &[(801, 3.0), (802, 4.0)]],
    );
}
