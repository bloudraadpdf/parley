// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{sync::Arc, vec::Vec};

use fontique::{Blob, Collection, CollectionOptions, FontInfoOverride, FontStyle, SourceCache};

use crate::{
    FontContext, FontFamily, FontSynthesisStyle, LayoutContext, StyleProperty,
    tests::utils::ColorBrush,
};

const TEST_FAMILY: &str = "Parley style synthesis test";

fn test_font_context_with_style(second_style: FontStyle) -> (FontContext, u64, u64) {
    let regular = Blob::new(Arc::new(
        include_bytes!("../../../parley_dev/assets/fonts/roboto_fonts/Roboto-Regular.ttf").to_vec(),
    ));
    let italic = Blob::new(Arc::new(
        include_bytes!("../../../parley_dev/assets/fonts/arimo_fonts/Arimo-VariableFont_wght.ttf")
            .to_vec(),
    ));
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
