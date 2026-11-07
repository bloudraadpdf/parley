// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A collapsible space removed at a line end takes its pair kerning with it.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, LayoutContext, StyleProperty};

fn content_advance(text: &str, width: Option<f32>, line: usize) -> f32 {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Arimo")));
    builder.push_default(StyleProperty::FontSize(40.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(width);
    let metrics = *layout.lines().nth(line).expect("the line").metrics();
    metrics.advance - metrics.trailing_whitespace
}

/// Arimo kerns `A` before a space. The collapsible space at the end of a line is removed (CSS Text
/// 3 §4.1.2), so `A` keeps no kerning against it: at a soft wrap and at the paragraph end.
#[test]
fn a_removed_line_end_space_takes_its_pair_kerning_with_it() {
    let isolated = content_advance("A", None, 0);
    assert_eq!(content_advance("P A S", Some(40.0), 1), isolated);
    assert_eq!(content_advance("A ", None, 0), isolated);
}
