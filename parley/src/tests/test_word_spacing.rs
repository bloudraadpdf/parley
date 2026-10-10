// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{sync::Arc, vec::Vec};

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, LayoutContext, StyleProperty, TextWrapMode, WhiteSpaceCollapse};

#[test]
fn word_spacing_preserves_spaces_in_mark_clusters() {
    let mut fonts = create_font_context();
    fonts.collection.register_fonts(
        fontique::Blob::new(Arc::new(
            parley_dev::fonts::MARK_ANCHOR_TEST
                .to_vec(),
        )),
        None,
    );
    let mut context: LayoutContext<ColorBrush> = LayoutContext::new();
    for (text, space_origin) in [
        (" ", 0.0),
        (" \u{0301}", 0.0),
        ("A A", 100.0),
        ("א א", 100.0),
        ("A \u{0301}", 100.0),
        ("A \u{0301}A", 100.0),
        ("א \u{0301}", 0.0),
        ("א \u{0301}א", 100.0),
        ("א \u{0301}\u{0300}א", 100.0),
    ] {
        for word_spacing in [0.0, 100.0, -50.0] {
            let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
            builder.push_default(StyleProperty::FontFamily(FontFamily::named(
                "mark-anchor-test",
            )));
            builder.push_default(StyleProperty::FontSize(100.0));
            builder.push_default(StyleProperty::WordSpacing(word_spacing));
            builder.push_default(StyleProperty::WhiteSpaceCollapse(
                WhiteSpaceCollapse::Preserve,
            ));
            builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
            let mut layout = builder.build(text);
            layout.break_all_lines(None);
            let mut ranges = layout
                .lines()
                .flat_map(|line| line.runs())
                .flat_map(|run| {
                    run.clusters()
                        .map(|cluster| cluster.text_range())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            ranges.sort_by_key(|range| range.start);
            let expected = text
                .char_indices()
                .map(|(start, character)| start..start + character.len_utf8())
                .collect::<Vec<_>>();
            assert_eq!(ranges, expected, "{text:?}, spacing={word_spacing}");
            let base_count = text
                .chars()
                .filter(|character| character.is_alphabetic())
                .count();
            assert_eq!(
                layout.lines().next().expect("one line").metrics().advance,
                (base_count + 1) as f32 * 100.0 + word_spacing,
                "{text:?}, spacing={word_spacing}"
            );
            let glyphs = layout
                .lines()
                .flat_map(|line| line.items())
                .flat_map(|item| match item {
                    crate::PositionedLayoutItem::GlyphRun(run) => run
                        .positioned_glyphs()
                        .map(|glyph| (glyph.id, glyph.x))
                        .collect::<Vec<_>>(),
                    crate::PositionedLayoutItem::InlineBox(_) => Vec::new(),
                })
                .collect::<Vec<_>>();
            let space = glyphs
                .iter()
                .find(|(id, _)| *id == 1)
                .expect("the space glyph")
                .1;
            assert_eq!(
                space,
                space_origin + word_spacing / 2.0,
                "half-side word spacing: {text:?}, spacing={word_spacing}"
            );
            for (_, mark) in glyphs.iter().filter(|(id, _)| matches!(id, 3 | 4)) {
                assert_eq!(*mark, space, "{text:?}, spacing={word_spacing}");
            }
        }
    }
}
