// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! CSS letter-spacing invariants around invisible formatting characters.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, Layout, LayoutContext, StyleProperty};
use alloc::{sync::Arc, vec::Vec};

fn unwrapped_layout(family: &str, text: &str, letter_spacing: f32) -> Layout<ColorBrush> {
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
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::LetterSpacing(letter_spacing));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

fn unwrapped_advance(text: &str, letter_spacing: f32) -> f32 {
    unwrapped_layout("Roboto", text, letter_spacing)
        .lines()
        .next()
        .expect("the text must produce one line")
        .metrics()
        .advance
}

fn glyph_count(layout: &Layout<ColorBrush>) -> usize {
    layout
        .lines()
        .flat_map(|line| line.runs().collect::<Vec<_>>())
        .map(|run| {
            run.clusters()
                .map(|cluster| cluster.glyphs().count())
                .sum::<usize>()
        })
        .sum()
}

#[test]
fn letter_spacing_disables_optional_ligatures() {
    // CSS Text 4 §8.2: non-zero tracking must not apply optional ligatures.
    let ligated = unwrapped_layout("Roboto", "fi", 0.0);
    let tracked = unwrapped_layout("Roboto", "fi", 2.0);

    assert_eq!(glyph_count(&ligated), 1, "Roboto ligates fi by default");
    assert_eq!(glyph_count(&tracked), 2);
}

#[test]
fn cursive_scripts_receive_no_letter_spacing() {
    // CSS Text 4 §8.2.1: letter-spacing is not applied within cursive scripts.
    let plain = unwrapped_layout("Noto Naskh Arabic", "ععع", 0.0);
    let tracked = unwrapped_layout("Noto Naskh Arabic", "ععع", 5.0);

    let advance = |layout: &Layout<ColorBrush>| layout.lines().next().unwrap().metrics().advance;
    assert!((advance(&tracked) - advance(&plain)).abs() < 0.001);
}

#[test]
fn default_ignorable_soft_hyphen_does_not_create_a_letter_spacing_interval() {
    // CSS Text defines letter-spacing over typographic character units.
    // U+00AD is default-ignorable in unbroken flow: it contributes only a
    // discretionary break opportunity and must not widen the word. A selected
    // break's visible hyphen is modelled separately by `DiscretionaryBreak`.
    let letter_spacing = 7.5;
    let plain = unwrapped_advance("ARIZONA", letter_spacing);
    let discretionary = unwrapped_advance("ARI\u{00AD}ZONA", letter_spacing);

    assert!(
        (discretionary - plain).abs() < 0.001,
        "an unselected soft hyphen must not add tracking to unbroken flow: plain={plain}, discretionary={discretionary}"
    );
}
