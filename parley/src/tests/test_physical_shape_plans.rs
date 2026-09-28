// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

use super::{
    test_physical_shaping::owner, test_shaping_context::naskh_font_context, utils::ColorBrush,
};
use crate::layout::{PhysicalLineEdge, ShapingEdgePlacement};
use crate::{FontFamily, Layout, LayoutContext, RangedBuilder, StyleProperty};

fn retained_layout(
    text: &str,
    family: &str,
    boundaries: &[usize],
    configure: impl FnOnce(&mut RangedBuilder<'_, ColorBrush>),
) -> Layout<ColorBrush> {
    let mut fonts = naskh_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_inline_owner_shaping(owner(
        0..text.len(),
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::FirstLine,
    ));
    for boundary in boundaries {
        builder.push_shaping_boundary(*boundary);
    }
    configure(&mut builder);
    builder.build(text)
}

fn glyphs(layout: &mut Layout<ColorBrush>) -> Vec<(u32, f32, f32, f32)> {
    let owners = core::mem::take(&mut layout.data.inline_owner_shaping);
    layout.break_all_lines(None);
    layout.data.inline_owner_shaping = owners;
    super::utils::visual_glyphs(layout)
        .into_iter()
        .flatten()
        .collect()
}

fn apply_boundaries(
    shaper: &mut crate::shape::physical::PhysicalShaper<ColorBrush>,
    layout: &mut Layout<ColorBrush>,
    boundaries: &[usize],
) {
    let shape = shaper.shape_line(&layout.data, boundaries, &[], false);
    shape.install(&mut layout.data);
}

#[test]
fn prepared_boundaries_split_and_restore_ligatures_without_new_source_items() {
    let mut layout = retained_layout("office", "Roboto", &[], |_| {});
    let mut shaper = crate::shape::physical::PhysicalShaper::new(&layout.data);
    let joined = glyphs(&mut layout);
    let source_items = layout.data.items.clone();
    let retained_glyphs = layout.data.glyphs.clone();
    let mut expected = retained_layout("office", "Roboto", &[2, 3], |_| {});
    let separated = glyphs(&mut expected);
    assert_ne!(joined, separated);
    apply_boundaries(&mut shaper, &mut layout, &[3, 2, 2]);
    assert_eq!(layout.data.glyphs[..retained_glyphs.len()], retained_glyphs);
    assert_eq!(glyphs(&mut layout), separated);
    assert_eq!(layout.data.items, source_items);
    apply_boundaries(&mut shaper, &mut layout, &[]);
    assert_eq!(glyphs(&mut layout), joined);
}

#[test]
fn prepared_context_boundaries_propagate_across_arabic_font_size_runs() {
    let text = "ععع";
    let enlarged = |builder: &mut RangedBuilder<'_, ColorBrush>| {
        builder.push(StyleProperty::FontSize(18.0), 2..4);
    };
    let mut layout = retained_layout(text, "Noto Naskh Arabic", &[], enlarged);
    let mut shaper = crate::shape::physical::PhysicalShaper::new(&layout.data);
    let joined = glyphs(&mut layout);
    let mut expected = retained_layout(text, "Noto Naskh Arabic", &[4], enlarged);
    let separated = glyphs(&mut expected);
    assert_ne!(joined, separated);
    apply_boundaries(&mut shaper, &mut layout, &[4]);
    assert_eq!(glyphs(&mut layout), separated);
    apply_boundaries(&mut shaper, &mut layout, &[]);
    assert_eq!(glyphs(&mut layout), joined);
}

#[test]
fn prepared_physical_context_does_not_remove_an_explicit_hard_boundary() {
    let text = "ععع";
    let mut layout = retained_layout(text, "Noto Naskh Arabic", &[2], |_| {});
    let mut shaper = crate::shape::physical::PhysicalShaper::new(&layout.data);
    let original = glyphs(&mut layout);
    apply_boundaries(&mut shaper, &mut layout, &[]);
    assert_eq!(glyphs(&mut layout), original);
    let mut expected = retained_layout(text, "Noto Naskh Arabic", &[2, 4], |_| {});
    apply_boundaries(&mut shaper, &mut layout, &[4]);
    assert_eq!(glyphs(&mut layout), glyphs(&mut expected));
}

#[test]
fn source_grapheme_and_tracking_boundaries_survive_shaping_segments() {
    let text = "a\u{301}b";
    let tracking = |builder: &mut RangedBuilder<'_, ColorBrush>| {
        builder.push_default(StyleProperty::LetterSpacing(3.0));
    };
    let mut layout = retained_layout(text, "Roboto", &[], tracking);
    let mut shaper = crate::shape::physical::PhysicalShaper::new(&layout.data);
    let source_flags = |layout: &Layout<ColorBrush>| {
        let mask = crate::layout::data::ClusterData::GRAPHEME_START
            | crate::layout::data::ClusterData::LETTER_SPACING_BOUNDARY;
        layout
            .data
            .clusters
            .iter()
            .map(|cluster| cluster.flags & mask)
            .collect::<Vec<_>>()
    };
    let original = source_flags(&layout);
    apply_boundaries(&mut shaper, &mut layout, &[1]);
    assert_eq!(source_flags(&layout), original);
    let mut expected = retained_layout(text, "Roboto", &[1], tracking);
    assert_eq!(glyphs(&mut layout), glyphs(&mut expected));
}

#[test]
fn a_changed_boundary_replaces_only_context_dependent_glyphs() {
    let text = "office office office";
    let mut layout = retained_layout(text, "Roboto", &[], |_| {});
    let mut shaper = crate::shape::physical::PhysicalShaper::new(&layout.data);
    let original_glyph_count = layout
        .runs()
        .map(|run| {
            run.clusters()
                .map(|cluster| cluster.glyphs().count())
                .sum::<usize>()
        })
        .sum::<usize>();
    for boundary in [2, 9, 2] {
        let before = layout.data.glyphs.len();
        apply_boundaries(&mut shaper, &mut layout, &[boundary]);
        assert!(
            layout.data.glyphs.len() - before < original_glyph_count,
            "unchanged words must reuse their glyphs"
        );
        let mut expected = retained_layout(text, "Roboto", &[boundary], |_| {});
        assert_eq!(glyphs(&mut layout), glyphs(&mut expected));
    }
}
