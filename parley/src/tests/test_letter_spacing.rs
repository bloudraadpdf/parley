// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! CSS letter-spacing invariants around invisible formatting characters.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, LayoutContext, StyleProperty};

fn unwrapped_advance(text: &str, letter_spacing: f32) -> f32 {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    builder.push_default(StyleProperty::LetterSpacing(letter_spacing));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let advance = layout
        .lines()
        .next()
        .expect("the text must produce one line")
        .metrics()
        .advance;
    advance
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
