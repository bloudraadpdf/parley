// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Selection of discretionary (soft hyphen) break opportunities.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::layout::DiscretionaryBreak;
use crate::{FontFamily, Layout, LayoutContext, StyleProperty};
use alloc::{vec, vec::Vec};

fn roboto_layout(text: &str) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.build(text)
}

fn unwrapped_advance(text: &str) -> f32 {
    let mut layout = roboto_layout(text);
    layout.break_all_lines(None);
    layout.full_width()
}

#[test]
fn the_only_opportunity_is_taken_when_its_hyphen_overflows() {
    // CSS Text 4 §5.4: a word that fits no other way breaks at its
    // hyphenation opportunity even when the hyphen itself overflows the
    // line; the alternative is the whole word overflowing.
    let text = "im\u{00AD}plementation";
    let head = unwrapped_advance("im");
    let hyphen = unwrapped_advance("-");
    let max_advance = head + hyphen * 0.5;

    let mut layout = roboto_layout(text);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "im\u{00AD}".len(),
        advance: hyphen,
        max_consecutive_lines: None,
    }]);
    layout.break_all_lines(Some(max_advance));

    let lines: Vec<_> = layout.lines().collect();
    assert_eq!(lines.len(), 2, "the word must break at its soft hyphen");
    assert!(lines[0].ends_at_discretionary_break());
    assert_eq!(lines[0].text_range(), 0.."im\u{00AD}".len());
}

#[test]
fn an_earlier_fitting_opportunity_beats_an_overflowing_hyphen() {
    let text = "a im\u{00AD}plementation";
    let head = unwrapped_advance("a im");
    let hyphen = unwrapped_advance("-");
    let max_advance = head + hyphen * 0.5;

    let mut layout = roboto_layout(text);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "a im\u{00AD}".len(),
        advance: hyphen,
        max_consecutive_lines: None,
    }]);
    layout.break_all_lines(Some(max_advance));

    let first = layout.lines().next().expect("the text produces a line");
    assert!(!first.ends_at_discretionary_break());
    assert_eq!(first.text_range(), 0.."a ".len());
}
