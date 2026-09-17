// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Joining context across shaping segment boundaries.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, InlineBox, LayoutContext, StyleProperty};
use alloc::{sync::Arc, vec::Vec};

fn naskh_glyph_ids(text: &str, enlarged: Option<core::ops::Range<usize>>) -> Vec<u32> {
    naskh_glyph_ids_with(text, enlarged, None)
}

#[test]
fn an_inline_box_with_advance_stops_the_joining_context() {
    // CSS Text 4 §8.3: padding, margin or border between letters breaks
    // the cursive connection, exactly like a zero-width non-joiner.
    let text = "ععع";
    let mut expected = naskh_glyph_ids("ع", None);
    expected.extend(naskh_glyph_ids("عع", None));
    let mut boxed = naskh_glyph_ids_with(text, None, Some('ع'.len_utf8()));
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
    inline_box_at: Option<usize>,
) -> Vec<u32> {
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
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(
        "Noto Naskh Arabic",
    )));
    builder.push_default(StyleProperty::FontSize(12.0));
    if let Some(range) = enlarged {
        builder.push(StyleProperty::FontSize(18.0), range);
    }
    if let Some(index) = inline_box_at {
        builder.push_inline_box(InlineBox::new(1, index, 6.0, 0.0));
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
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
