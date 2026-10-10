// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Joining context across shaping segment boundaries.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, InlineBox, LayoutContext, StyleProperty};
use alloc::{sync::Arc, vec::Vec};

fn naskh_glyph_ids(text: &str, enlarged: Option<core::ops::Range<usize>>) -> Vec<u32> {
    naskh_glyph_ids_with(text, enlarged, None, &[])
}

#[test]
fn an_inline_box_with_advance_stops_the_joining_context() {
    // CSS Text 4 §8.3: padding, margin or border between letters breaks
    // the cursive connection, exactly like a zero-width non-joiner.
    let text = "ععع";
    let mut expected = naskh_glyph_ids("ع", None);
    expected.extend(naskh_glyph_ids("عع", None));
    let mut boxed = naskh_glyph_ids_with(
        text,
        None,
        Some(InlineBox::new(1, 'ع'.len_utf8(), 6.0, 0.0)),
        &[],
    );
    expected.sort_unstable();
    boxed.sort_unstable();

    assert_eq!(boxed.len(), 3);
    assert_eq!(
        boxed, expected,
        "the letters beside the box must take their unjoined forms"
    );
}

fn naskh_glyph_ids_with(
    text: &str,
    enlarged: Option<core::ops::Range<usize>>,
    inline_box: Option<InlineBox>,
    shaping_boundaries: &[usize],
) -> Vec<u32> {
    let mut font_context = naskh_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(
        "Noto Naskh Arabic",
    )));
    builder.push_default(StyleProperty::FontSize(12.0));
    if let Some(range) = enlarged {
        builder.push(StyleProperty::FontSize(18.0), range);
    }
    if let Some(inline_box) = inline_box {
        builder.push_inline_box(inline_box);
    }
    for boundary in shaping_boundaries {
        builder.push_shaping_boundary(*boundary);
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    logical_glyph_ids(&layout)
}

fn logical_glyph_ids(layout: &crate::Layout<ColorBrush>) -> Vec<u32> {
    layout
        .lines()
        .flat_map(|line| line.runs().collect::<Vec<_>>())
        .flat_map(|run| {
            run.clusters()
                .flat_map(|cluster| cluster.glyphs().map(|glyph| glyph.id))
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn naskh_font_context() -> crate::FontContext {
    let mut font_context = create_font_context();
    font_context.collection.register_fonts(
        fontique::Blob::new(Arc::new(parley_dev::fonts::NOTO_NASKH_ARABIC.to_vec())),
        None,
    );
    font_context
}

#[test]
fn explicit_boundaries_stop_arabic_joining_without_layout_boxes() {
    let mut expected = naskh_glyph_ids("ع", None);
    expected.extend(naskh_glyph_ids("عع", None));
    let mut actual = naskh_glyph_ids_with("ععع", None, None, &["ع".len()]);
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn explicit_boundaries_split_ligatures_without_line_breaks_or_context_leaks() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    for (boundaries, glyph_count) in [(&[1, 1][..], 2), (&[][..], 1)] {
        let mut builder = context.ranged_builder(&mut fonts, "fi", 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(24.0));
        for boundary in boundaries {
            builder.push_shaping_boundary(*boundary);
        }
        let mut layout = builder.build("fi");
        layout.break_all_lines(Some(1.0));
        assert!(layout.data.inline_boxes.is_empty());
        assert_eq!(layout.lines().count(), 1);
        assert_eq!(
            layout
                .lines()
                .flat_map(|line| line.runs().collect::<Vec<_>>())
                .map(|run| run
                    .clusters()
                    .map(|cluster| cluster.glyphs().count())
                    .sum::<usize>())
                .sum::<usize>(),
            glyph_count
        );
    }
}

#[test]
#[should_panic(expected = "shaping boundaries must be UTF-8 positions")]
fn explicit_boundaries_reject_an_offset_inside_a_codepoint() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, "ع", 1.0, false);
    builder.push_shaping_boundary(1);
    let _ = builder.build("ع");
}

#[test]
fn continuous_owner_edges_keep_ligatures_and_their_advance() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let make_layout = |context: &mut LayoutContext<ColorBrush>,
                       fonts: &mut crate::FontContext,
                       width| {
        let mut builder = context.ranged_builder(fonts, "fi", 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(24.0));
        builder.push_inline_box(
            InlineBox::inline_start_edge(91, 1, width, 0.0, crate::InlineBoxBreakAffinity::ToNext)
                .with_continuous_shaping(),
        );
        builder.build("fi")
    };
    let mut plain = make_layout(&mut context, &mut fonts, 0.0);
    plain.break_all_lines(None);
    let mut edged = make_layout(&mut context, &mut fonts, 10.0);
    edged.break_all_lines(None);
    assert_eq!(logical_glyph_ids(&plain), logical_glyph_ids(&edged));
    assert_eq!(logical_glyph_ids(&edged).len(), 1);
    assert_eq!(edged.width(), plain.width() + 10.0);
    let mut breaker = edged.break_lines();
    assert_eq!(breaker.break_next_with_length(10), Some(()));
    breaker.finish();
    assert_eq!(edged.width(), plain.width() + 10.0);
    assert_eq!(logical_glyph_ids(&plain), logical_glyph_ids(&edged));
}

#[test]
fn continuous_owner_edges_preserve_joining_with_separate_context_boundaries() {
    let text = "ععع";
    let edge =
        InlineBox::inline_start_edge(91, 2, 10.0, 0.0, crate::InlineBoxBreakAffinity::ToNext)
            .with_continuous_shaping();
    let mut continuous = naskh_glyph_ids_with(text, None, Some(edge.clone()), &[]);
    let mut joined = naskh_glyph_ids(text, None);
    continuous.sort_unstable();
    joined.sort_unstable();
    assert_eq!(continuous, joined);
    let mut expected = naskh_glyph_ids("عع", None);
    expected.extend(naskh_glyph_ids("ع", None));
    let mut actual = naskh_glyph_ids_with(text, None, Some(edge), &[4]);
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn arabic_joining_survives_a_font_size_boundary() {
    // CSS Text 4 §8.3: a style boundary must not break the cursive
    // connection, so the letters keep their initial, medial and final forms.
    let text = "ععع";
    let middle = 'ع'.len_utf8()..2 * 'ع'.len_utf8();
    let mut joined = naskh_glyph_ids(text, None);
    let mut split = naskh_glyph_ids(text, Some(middle));
    // A single right-to-left run stores its glyphs visually reversed, while
    // three runs follow logical order; compare the forms, not the order.
    joined.sort_unstable();
    split.sort_unstable();

    assert_eq!(joined.len(), 3);
    assert_eq!(
        split, joined,
        "each segment must be shaped with its neighbours as context"
    );
}
