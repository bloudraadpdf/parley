// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{sync::Arc, vec::Vec};

use fontique::{Blob, Collection, CollectionOptions, FontInfoOverride, FontStyle, SourceCache};

use crate::{
    FontContext, FontFamily, FontSynthesisStyle, LayoutContext, StyleProperty,
    tests::utils::ColorBrush,
};

const TEST_FAMILY: &str = "Parley style synthesis test";

#[test]
fn selected_fallback_weight_policy_preserves_other_axes_and_cache_identity() {
    let blob = Blob::new(Arc::new(parley_dev::fonts::ROBOTO_FLEX.to_vec()));
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    collection.register_fonts(
        blob.clone(),
        Some(FontInfoOverride {
            family_name: Some(TEST_FAMILY),
            ..Default::default()
        }),
    );
    let family = collection.family_id(TEST_FAMILY).expect("registered family");
    collection.set_fallbacks(
        fontique::FallbackKey::new(fontique::Script::from_bytes(*b"Latn"), None),
        core::iter::once(family),
    );
    let mut fcx = FontContext {
        collection,
        source_cache: SourceCache::default(),
    };
    let mut lcx = LayoutContext::<ColorBrush>::new();
    let mut shape = |lcx: &mut LayoutContext<ColorBrush>, settings: &str| {
        let text = "office Hamburgefonts";
        let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("absent authored family")));
        builder.push_default(StyleProperty::FontSize(20.0));
        builder.push_default(StyleProperty::FontWeight(fontique::FontWeight::new(900.0)));
        builder.push_default(StyleProperty::FontVariations(settings.into()));
        builder.push_inline_owner_shaping(super::test_physical_shaping::owner(
            0..text.len(),
            crate::layout::PhysicalLineEdge::Left,
            crate::layout::ShapingEdgePlacement::FirstLine,
        ));
        let mut layout = builder.build(text);
        let run = layout.runs().next().expect("fallback run");
        assert_eq!(run.font().data.id(), blob.id());
        let initial = (run.normalized_coords().to_vec(), run.advance());
        let policy = lcx.use_default_font_weight;
        lcx.set_use_default_font_weight(!policy);
        let mut replay = crate::shape::physical::PhysicalShaper::new(&layout.data);
        let shape = replay.shape_line(&layout.data, &[2], &[], 0, None, false);
        assert!(replay.work().shaped_source > 0);
        replay.install(&mut layout.data, &shape);
        layout.data.inline_owner_shaping.clear();
        layout.break_all_lines(None);
        let glyphs = super::utils::visual_glyphs(&layout);
        lcx.set_use_default_font_weight(policy);
        (initial, glyphs)
    };
    let standard = shape(&mut lcx, "'wght' 900, 'wdth' 75");
    let default = shape(&mut lcx, "'wght' 400, 'wdth' 75");
    assert_ne!(standard, default);
    lcx.set_use_default_font_weight(true);
    assert_eq!(shape(&mut lcx, "'wght' 900, 'wdth' 75"), default);
    assert_eq!(shape(&mut lcx.clone(), "'wght' 900, 'wdth' 75"), default);
    lcx.set_use_default_font_weight(false);
    assert_eq!(shape(&mut lcx, "'wght' 900, 'wdth' 75"), standard);
}

fn test_font_context_with_style(second_style: FontStyle) -> (FontContext, u64, u64) {
    let regular = Blob::new(Arc::new(parley_dev::fonts::ROBOTO_REGULAR.to_vec()));
    let italic = Blob::new(Arc::new(parley_dev::fonts::ARIMO_VARIABLE.to_vec()));
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    collection.register_fonts(
        regular.clone(),
        Some(FontInfoOverride {
            family_name: Some(TEST_FAMILY),
            style: Some(FontStyle::Normal),
            ..FontInfoOverride::default()
        }),
    );
    collection.register_fonts(
        italic.clone(),
        Some(FontInfoOverride {
            family_name: Some(TEST_FAMILY),
            style: Some(second_style),
            ..FontInfoOverride::default()
        }),
    );
    (
        FontContext {
            collection,
            source_cache: SourceCache::default(),
        },
        regular.id(),
        italic.id(),
    )
}

#[test]
fn style_synthesis_policy_changes_face_selection_within_one_query() {
    let (mut font_context, regular_id, italic_id) = test_font_context_with_style(FontStyle::Italic);
    let mut layout_context = LayoutContext::<ColorBrush>::new();
    let text = "AB";
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(TEST_FAMILY)));
    builder.push_default(StyleProperty::FontStyle(FontStyle::Oblique(Some(10.0))));
    builder.push_default(StyleProperty::FontSynthesisStyle(FontSynthesisStyle::Auto));
    builder.push(
        StyleProperty::FontSynthesisStyle(FontSynthesisStyle::None),
        1..2,
    );

    let layout = builder.build(text);
    let runs: Vec<_> = layout.runs().collect();
    assert_eq!(runs.len(), 2);

    assert_eq!(runs[0].font().data.id(), regular_id);
    assert_eq!(runs[0].synthesis().skew(), Some(10.0));

    assert_eq!(runs[1].font().data.id(), italic_id);
    assert_eq!(runs[1].synthesis().skew(), None);
}

#[test]
fn oblique_only_does_not_use_an_oblique_face_for_italic() {
    let (mut font_context, regular_id, oblique_id) =
        test_font_context_with_style(FontStyle::Oblique(Some(14.0)));
    let mut layout_context = LayoutContext::<ColorBrush>::new();
    let text = "AB";
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(TEST_FAMILY)));
    builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
    builder.push_default(StyleProperty::FontSynthesisStyle(FontSynthesisStyle::Auto));
    builder.push(
        StyleProperty::FontSynthesisStyle(FontSynthesisStyle::ObliqueOnly),
        1..2,
    );

    let layout = builder.build(text);
    let runs: Vec<_> = layout.runs().collect();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].font().data.id(), oblique_id);
    assert_eq!(runs[1].font().data.id(), regular_id);
}
