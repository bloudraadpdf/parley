// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{FontFamily, LayoutContext, StyleProperty};

#[test]
fn a_build_keeps_no_buffers_of_its_styles_or_characters() {
    let text = "alpha beta gamma";
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push(StyleProperty::FontSize(20.0), 6..10);
    let _layout = builder.build(text);
    assert_eq!(layout_context.style_table.capacity(), 0);
    assert_eq!(layout_context.style_runs.capacity(), 0);
    assert_eq!(layout_context.info.capacity(), 0);
}
