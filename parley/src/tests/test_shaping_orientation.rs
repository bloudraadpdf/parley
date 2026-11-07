// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, FontFeatures, LayoutContext, ShapingOrientation, StyleProperty};
use alloc::{sync::Arc, vec::Vec};

#[test]
fn upright_shaping_preserves_vertical_substitution_positioning_and_cache_direction() {
    use skrifa::raw::TableProvider;
    let bytes = include_bytes!(
        "../../../parley_dev/assets/fonts/noto_sans_jp_kana/NotoSansJP-kana_test-subset.otf"
    );
    let mut fcx = create_font_context();
    fcx.collection
        .register_fonts(peniko::Blob::new(Arc::new(bytes.to_vec())), None);
    let mut lcx = LayoutContext::<ColorBrush>::new();
    let text = "すす";
    let font = harfrust::FontRef::new(bytes).expect("Japanese fixture");
    let data = harfrust::ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let units = skrifa::FontRef::new(bytes)
        .expect("fixture")
        .head()
        .expect("head")
        .units_per_em() as f32;
    let mut vertical_advances = Vec::new();
    for orientation in [
        ShapingOrientation::Horizontal,
        ShapingOrientation::UprightVertical,
        ShapingOrientation::Horizontal,
    ] {
        for enabled in [0, 1] {
            let features = [crate::setting::FontFeature::new(
                crate::setting::Tag::from_bytes(*b"vpal"),
                enabled,
            )];
            let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
            builder.push_default(StyleProperty::FontFamily(FontFamily::named("Noto Sans JP")));
            builder.push_default(StyleProperty::FontSize(20.0));
            builder.push_default(StyleProperty::ShapingOrientation(orientation));
            builder.push_default(StyleProperty::FontFeatures(FontFeatures::List(
                (&features[..]).into(),
            )));
            let mut layout = builder.build(text);
            layout.break_all_lines(None);
            let mut actual = Vec::new();
            for line in layout.lines() {
                for run in line.runs() {
                    assert_eq!(run.shaping_orientation(), orientation);
                    for cluster in run.clusters() {
                        actual.extend(cluster.glyphs());
                    }
                }
            }
            let mut buffer = harfrust::UnicodeBuffer::new();
            buffer.push_str(text);
            buffer.guess_segment_properties();
            buffer.set_direction(if orientation == ShapingOrientation::UprightVertical {
                harfrust::Direction::TopToBottom
            } else {
                harfrust::Direction::LeftToRight
            });
            let hb_features = [harfrust::Feature::new(
                skrifa::Tag::new(b"vpal"),
                u32::from(enabled),
                ..,
            )];
            let expected = shaper.shape(
                buffer,
                harfrust::ShapeOptions::new()
                    .features(&hb_features)
                    .point_size(Some(20.0)),
            );
            assert_eq!(actual.len(), expected.len());
            for (glyph, (info, position)) in actual.iter().zip(
                expected
                    .glyph_infos()
                    .iter()
                    .zip(expected.glyph_positions()),
            ) {
                assert_eq!(glyph.id, info.glyph_id);
                let (advance, x, y) = if orientation == ShapingOrientation::UprightVertical {
                    (-position.y_advance, -position.y_offset, position.x_offset)
                } else {
                    (position.x_advance, position.x_offset, -position.y_offset)
                };
                for (actual, expected) in [(glyph.advance, advance), (glyph.x, x), (glyph.y, y)] {
                    assert!((actual - expected as f32 * 20.0 / units).abs() < 0.0001);
                }
            }
            if orientation == ShapingOrientation::UprightVertical {
                vertical_advances.push(actual.iter().map(|glyph| glyph.advance).sum::<f32>());
            }
        }
    }
    assert!(
        vertical_advances[1] < vertical_advances[0],
        "vpal changes vertical advances"
    );
}

#[test]
fn upright_fonts_without_vertical_metrics_use_em_advances_before_positioning() {
    let bytes = include_bytes!("../../../parley_dev/assets/fonts/roboto_fonts/Roboto-Regular.ttf");
    use skrifa::raw::TableProvider;
    assert!(
        skrifa::FontRef::new(bytes)
            .expect("Roboto fixture")
            .vmtx()
            .is_err()
    );
    let mut fcx = create_font_context();
    let mut lcx = LayoutContext::<ColorBrush>::new();
    let mut builder = lcx.ranged_builder(&mut fcx, "AA", 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(20.0));
    builder.push_default(StyleProperty::ShapingOrientation(
        ShapingOrientation::UprightVertical,
    ));
    let mut layout = builder.build("AA");
    layout.break_all_lines(Some(20.0));
    for line in layout.lines() {
        for run in line.runs() {
            assert_eq!(
                run.shaping_orientation(),
                ShapingOrientation::UprightVertical
            );
            for cluster in run.clusters() {
                for glyph in cluster.glyphs() {
                    assert_eq!(glyph.advance, 20.0);
                }
            }
        }
    }
    assert_eq!(
        layout
            .lines()
            .map(|line| line.metrics().advance)
            .sum::<f32>(),
        40.0
    );
}
