// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A collapsible space or an other space separator kept at a line end.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, Layout, LayoutContext, LineEndSpace, StyleProperty};

const TEXT: &str = "aaa bbb ccc";

fn layout(text: &str, kept: Option<core::ops::Range<usize>>) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    if let Some(range) = kept {
        builder.push(StyleProperty::LineEndSpace(LineEndSpace::Kept), range);
    }
    builder.build(text)
}

fn natural_width(text: &str) -> f32 {
    let mut layout = layout(text, None);
    layout.break_all_lines(None);
    layout.full_width()
}

fn first_line_end(kept: Option<core::ops::Range<usize>>, width: f32) -> usize {
    let mut layout = layout(TEXT, kept);
    layout.break_all_lines(Some(width));
    let end = layout.lines().next().expect("a line").text_range().end;
    end
}

/// A space kept at a line end counts for the fit: a line that holds `aaa bbb` but not its space breaks before `bbb`
/// when the space is kept (Prince 16.2 keeps the space that ends generated content).
#[test]
fn a_kept_line_end_space_counts_for_the_fit() {
    let space = natural_width("a a") - natural_width("aa");
    let width = natural_width("aaa bbb") + space / 2.0;
    assert_eq!(first_line_end(None, width), 8);
    assert_eq!(first_line_end(Some(7..8), width), 4);
    assert_eq!(first_line_end(Some(3..4), width), 8);
}

/// An em space kept at a line end counts for the extent of the line; by default it hangs (Prince 16.2 and PDFreactor
/// 12.7.2 keep every other space separator).
#[test]
fn a_kept_line_end_em_space_counts_for_the_extent() {
    let text = "aa\u{2003}";
    let extent = |kept: Option<core::ops::Range<usize>>| {
        let mut layout = layout(text, kept);
        layout.break_all_lines(None);
        let metrics = *layout.lines().next().expect("a line").metrics();
        metrics.advance - metrics.trailing_whitespace
    };
    assert_eq!(extent(None), natural_width("aa"));
    assert_eq!(extent(Some(2..text.len())), natural_width(text));
}
