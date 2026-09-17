// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Justification opportunities.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{
    Alignment, AlignmentOptions, FontFamily, JustificationMode, Layout, LayoutContext,
    StyleProperty, WhiteSpaceCollapse,
};
use alloc::vec::Vec;

fn roboto_layout(text: &str, max_advance: Option<f32>, justify: bool) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::Preserve,
    ));
    let mut layout = builder.build(text);
    layout.break_all_lines(max_advance);
    if justify {
        layout.align(
            max_advance,
            Alignment::Justify,
            AlignmentOptions {
                justification_mode: JustificationMode::InterWord,
                ..AlignmentOptions::default()
            },
        );
    }
    layout
}

fn first_line_cluster_advances(layout: &Layout<ColorBrush>) -> Vec<f32> {
    layout
        .lines()
        .next()
        .expect("the text produces a line")
        .runs()
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| cluster.advance())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_line_with_a_preserved_tab_is_not_justified() {
    // CSS Text 3 §7.1: adjusted text must keep its tab stops lined up, so a
    // line that contains a preserved tab is left unjustified.
    let text = "a b\tc d e";
    let natural = roboto_layout("a b\tc d", None, false).full_width();
    let max_advance = Some(natural + 4.0);
    let plain = roboto_layout(text, max_advance, false);
    let justified = roboto_layout(text, max_advance, true);

    assert_eq!(plain.len(), 2, "the last word wraps");
    assert_eq!(
        first_line_cluster_advances(&justified),
        first_line_cluster_advances(&plain)
    );
}
