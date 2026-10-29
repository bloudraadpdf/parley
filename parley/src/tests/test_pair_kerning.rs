// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{
    test_builders::create_font_context, test_justification_shapes::prepare, utils::ColorBrush,
};
use crate::{FontFamily, FontFeature, LayoutContext, StyleProperty};

fn kern(value: u16) -> FontFeature {
    FontFeature::new(crate::setting::Tag::from_bytes(*b"kern"), value)
}

#[test]
fn final_justification_feature_value_controls_pair_kerning() {
    let vkrn = FontFeature::new(crate::setting::Tag::from_bytes(*b"vkrn"), 0);
    for (features, disabled) in [
        (&[kern(0), kern(1)][..], false),
        (&[kern(1), kern(0)][..], true),
        (&[vkrn][..], false),
    ] {
        let mut fonts = create_font_context();
        let mut context = LayoutContext::<ColorBrush>::new();
        let text = "AVAV";
        let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(24.0));
        builder.push_default(StyleProperty::FontFeaturesForJustification(features.into()));
        let mut layout = builder.build(text);
        prepare(&mut context, &mut layout, &[2]);
        assert_eq!(
            layout
                .data
                .deferred_justification_shapes
                .iter()
                .any(|shape| shape.prepared.is_some()),
            disabled
        );
    }
}

#[test]
fn utf8_boundaries_rebase_to_run_scalar_clusters() {
    for prefix in ["\u{e9}", "\u{e9}\u{e9}"] {
        for separate_run in [false, true] {
            let mut fonts = create_font_context();
            let mut context = LayoutContext::<ColorBrush>::new();
            let text = alloc::format!("{prefix}AVAV");
            let features = [kern(0)];
            let mut build = |disable| {
                let mut builder = context.ranged_builder(&mut fonts, &text, 1.0, false);
                builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
                builder.push_default(StyleProperty::FontSize(24.0));
                builder.push_default(StyleProperty::FontFeaturesForJustification(
                    features.as_slice().into(),
                ));
                if separate_run {
                    builder.push(StyleProperty::FontSize(18.0), 0..prefix.len());
                }
                if disable {
                    builder.push_default(StyleProperty::FontFeatures(features.as_slice().into()));
                }
                builder.build(&text)
            };
            let mut layout = build(false);
            let unkerned = build(true);
            let boundary = prefix.len() + 2;
            let run_index = layout
                .data
                .runs
                .iter()
                .position(|run| run.text_range.contains(&boundary))
                .unwrap();
            let run = &layout.data.runs[run_index];
            let original = layout.data.clusters[run.cluster_range.clone()].to_vec();
            let unkerned =
                &unkerned.data.clusters[unkerned.data.runs[run_index].cluster_range.clone()];
            let selected = if separate_run {
                1
            } else {
                prefix.chars().count() + 1
            };
            assert_ne!(original[selected].advance, unkerned[selected].advance);
            prepare(&mut context, &mut layout, &[boundary]);
            let prepared = layout
                .data
                .deferred_justification_shapes
                .iter()
                .find(|candidate| candidate.source.run_index == run_index)
                .unwrap()
                .prepared
                .as_ref()
                .unwrap();
            assert_eq!(prepared.clusters.len(), original.len());
            for (index, cluster) in prepared.clusters.iter().enumerate() {
                let expected = if index == selected {
                    unkerned[index].advance
                } else {
                    original[index].advance
                };
                assert_eq!(
                    cluster.advance, expected,
                    "prefix {prefix:?}, separate {separate_run}, cluster {index}"
                );
                assert_eq!(cluster.text_offset, original[index].text_offset);
            }
        }
    }
}
