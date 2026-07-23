// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{FontFamily, LayoutContext, LineBreakOverride, StyleProperty};
use alloc::{string::ToString, vec, vec::Vec};

#[test]
fn caller_overrides_replace_unicode_slash_opportunities() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "aa/bb/cc";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    let mut layout = builder.build(text);

    // Suppress the Unicode opportunities after each slash and add the custom
    // opportunities immediately before them.
    layout.set_line_break_overrides(vec![
        LineBreakOverride {
            byte_index: 2,
            opportunity: true,
        },
        LineBreakOverride {
            byte_index: 3,
            opportunity: false,
        },
        LineBreakOverride {
            byte_index: 5,
            opportunity: true,
        },
        LineBreakOverride {
            byte_index: 6,
            opportunity: false,
        },
    ]);
    layout.break_all_lines(Some(20.0));

    let lines = layout
        .lines()
        .map(|line| text[line.text_range()].to_string())
        .collect::<Vec<_>>();
    assert_eq!(lines, ["aa", "/bb", "/cc"]);
}
