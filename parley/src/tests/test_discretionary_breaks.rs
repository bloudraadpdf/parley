// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Selection of discretionary (soft hyphen) break opportunities.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::layout::{DiscretionaryBreak, DiscretionaryFitAdvance, SourceClusterFitAdvance};
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
        condition: crate::layout::DiscretionaryBreakCondition::Normal,
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
        condition: crate::layout::DiscretionaryBreakCondition::Normal,
    }]);
    layout.break_all_lines(Some(max_advance));

    let first = layout.lines().next().expect("the text produces a line");
    assert!(!first.ends_at_discretionary_break());
    assert_eq!(first.text_range(), 0.."a ".len());
}

#[test]
fn overflow_discretionaries_follow_ordinary_breaks_and_do_not_reduce_intrinsic_width() {
    use crate::layout::DiscretionaryBreakCondition;

    let word = "ab\u{00ad}cd";
    let hyphen = unwrapped_advance("-");
    let word_width = unwrapped_advance(word);
    for prefix in ["", "X "] {
        let text = alloc::format!("{prefix}{word}");
        let mut layout = roboto_layout(&text);
        layout.set_discretionary_breaks(vec![DiscretionaryBreak {
            byte_index: prefix.len() + "ab\u{00ad}".len(),
            advance: hyphen,
            max_consecutive_lines: None,
            condition: DiscretionaryBreakCondition::Overflow,
        }]);
        assert!((layout.calculate_content_widths().min - word_width).abs() < 0.001);
        layout.break_all_lines(Some(word_width + 0.1));
        assert!(
            layout
                .lines()
                .all(|line| !line.ends_at_discretionary_break())
        );
        assert_eq!(
            layout.lines().count(),
            if prefix.is_empty() { 1 } else { 2 }
        );

        layout.break_all_lines(Some(unwrapped_advance("ab") + hyphen));
        let lines = layout.lines().collect::<Vec<_>>();
        let hyphenated = usize::from(!prefix.is_empty());
        assert!(lines[hyphenated].ends_at_discretionary_break());
        assert_eq!(
            lines[hyphenated].text_range(),
            prefix.len()..prefix.len() + "ab\u{00ad}".len()
        );

        layout.break_all_lines(None);
        assert_eq!(layout.lines().count(), 1);
        assert!(!layout.lines().next().unwrap().ends_at_discretionary_break());
        assert!((layout.calculate_content_widths().min - word_width).abs() < 0.001);
    }
}

#[test]
fn a_later_overflowing_discretionary_does_not_replace_a_fitting_fallback() {
    let text = "ab\u{00ad}c\u{00ad}de";
    let mut layout = roboto_layout(text);
    let advances = layout
        .runs()
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| {
                    let range = cluster.text_range();
                    SourceClusterFitAdvance::new(
                        range.start,
                        if &text[range.clone()] == "\u{00ad}" {
                            0.0
                        } else {
                            10.0
                        },
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    layout.set_source_cluster_fit_advances(advances);
    let breaks: Vec<_> = text
        .match_indices('\u{00ad}')
        .map(|(index, _)| DiscretionaryBreak {
            byte_index: index + '\u{00ad}'.len_utf8(),
            advance: unwrapped_advance("-"),
            max_consecutive_lines: None,
            condition: crate::layout::DiscretionaryBreakCondition::Overflow,
        })
        .collect();
    layout.set_discretionary_fit_advances(
        breaks
            .iter()
            .map(|entry| DiscretionaryFitAdvance::new(entry.byte_index, 10.0).unwrap())
            .collect(),
    );
    layout.set_discretionary_breaks(breaks);
    layout.break_all_lines(Some(35.0));
    assert_eq!(
        layout.lines().next().unwrap().text_range(),
        0.."ab\u{00ad}".len()
    );
    assert!(layout.lines().next().unwrap().ends_at_discretionary_break());
}

#[test]
fn projected_upright_advances_select_the_soft_hyphen_without_changing_rendered_width() {
    let text = "hyphen\u{00ad}ation";
    let break_at = "hyphen\u{00ad}".len();
    let mut layout = roboto_layout(text);
    let advances = layout
        .runs()
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| {
                    let range = cluster.text_range();
                    let physical = if &text[range.clone()] == "\u{00ad}" {
                        0.0
                    } else {
                        12.0
                    };
                    SourceClusterFitAdvance::new(range.start, physical).unwrap()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    layout.set_source_cluster_fit_advances(advances);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: break_at,
        advance: unwrapped_advance("-"),
        max_consecutive_lines: None,
        condition: crate::layout::DiscretionaryBreakCondition::Normal,
    }]);
    layout.set_discretionary_fit_advances(vec![
        DiscretionaryFitAdvance::new(break_at, 12.0).unwrap(),
    ]);
    layout.break_all_lines(Some(84.0));

    let lines: Vec<_> = layout.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].ends_at_discretionary_break());
    assert_eq!(lines[0].text_range(), 0..break_at);
    assert_eq!(lines[1].text_range(), break_at..text.len());
    assert!(
        lines[0].metrics().advance < 84.0,
        "line placement retains shaped advances"
    );
}

#[test]
fn projected_upright_advances_do_not_invent_breaks_inside_an_unbreakable_word() {
    let text = "hyphenation";
    let mut layout = roboto_layout(text);
    let advances = layout
        .runs()
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| {
                    SourceClusterFitAdvance::new(cluster.text_range().start, 12.0).unwrap()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    layout.set_source_cluster_fit_advances(advances);
    layout.break_all_lines(Some(84.0));
    assert_eq!(layout.lines().count(), 1);
}

#[test]
fn projected_discretionary_material_must_fit_before_preempting_an_earlier_space() {
    let text = "a hy\u{00ad}phenation";
    let break_at = "a hy\u{00ad}".len();
    let mut layout = roboto_layout(text);
    let advances = layout
        .runs()
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| {
                    let range = cluster.text_range();
                    let physical = if &text[range.clone()] == "\u{00ad}" {
                        0.0
                    } else {
                        12.0
                    };
                    SourceClusterFitAdvance::new(range.start, physical).unwrap()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    layout.set_source_cluster_fit_advances(advances);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: break_at,
        advance: unwrapped_advance("-"),
        max_consecutive_lines: None,
        condition: crate::layout::DiscretionaryBreakCondition::Normal,
    }]);
    layout.set_discretionary_fit_advances(vec![
        DiscretionaryFitAdvance::new(break_at, 50.0).unwrap(),
    ]);
    layout.break_all_lines(Some(60.0));

    let first = layout.lines().next().unwrap();
    assert!(!first.ends_at_discretionary_break());
    assert_eq!(first.text_range(), 0.."a ".len());
}

#[test]
fn projected_upright_advances_preserve_unicode_cjk_breaks() {
    let text = "漢字漢字";
    let mut layout = roboto_layout(text);
    let advances = layout
        .runs()
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| {
                    SourceClusterFitAdvance::new(cluster.text_range().start, 12.0).unwrap()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    layout.set_source_cluster_fit_advances(advances);
    layout.break_all_lines(Some(24.0));

    let ranges = layout
        .lines()
        .map(|line| line.text_range())
        .collect::<Vec<_>>();
    assert_eq!(ranges, vec![0.."漢字".len(), "漢字".len()..text.len()]);
}

#[test]
fn replacing_source_fit_projection_restores_the_original_cluster_advances() {
    let text = "a b";
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::LetterSpacing(5.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let natural = layout.lines().next().unwrap().metrics().advance;
    let projected = layout
        .runs()
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| {
                    SourceClusterFitAdvance::new(cluster.text_range().start, 100.0).unwrap()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    layout.set_source_cluster_fit_advances(projected);
    layout.break_all_lines(Some(natural + 1.0));
    assert!(layout.lines().count() > 1);
    layout.set_source_cluster_fit_advances(Vec::new());
    layout.break_all_lines(Some(natural + 1.0));
    assert_eq!(layout.lines().count(), 1);
    assert!((layout.lines().next().unwrap().metrics().advance - natural).abs() < 0.001);
}

fn fallback_hyphen_layout(text: &str, fallback_start: usize) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push(
        StyleProperty::FontFamily(FontFamily::named("Noto Kufi Arabic")),
        fallback_start..text.len(),
    );
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

#[test]
fn selected_discretionary_fallback_contributes_its_own_line_metrics() {
    for condition in [
        crate::layout::DiscretionaryBreakCondition::Normal,
        crate::layout::DiscretionaryBreakCondition::Overflow,
    ] {
        let text = "aa\u{00ad}bbbb";
        let hyphen = fallback_hyphen_layout("-", 0);
        let reference = fallback_hyphen_layout("aa-", 2);
        let mut layout = roboto_layout(text);
        layout.set_discretionary_break_shapes(vec![
            crate::layout::DiscretionaryBreakShape::new(
                "aa\u{00ad}".len(),
                None,
                "-".into(),
                hyphen,
            )
            .unwrap()
            .with_condition(condition),
        ]);
        layout.break_all_lines(Some(reference.width()));
        let actual = layout.get(0).unwrap();
        let expected = reference.get(0).unwrap();
        assert!(actual.ends_at_discretionary_break());
        assert_eq!(actual.metrics().ascent, expected.metrics().ascent);
        assert_eq!(actual.metrics().descent, expected.metrics().descent);
        assert_eq!(actual.metrics().line_height, expected.metrics().line_height);
    }
}

#[test]
fn unused_discretionary_fallback_does_not_change_line_metrics() {
    let text = "aa\u{00ad}bbbb";
    let mut reference = roboto_layout(text);
    reference.break_all_lines(None);
    let mut layout = roboto_layout(text);
    layout.set_discretionary_break_shapes(vec![
        crate::layout::DiscretionaryBreakShape::new(
            "aa\u{00ad}".len(),
            None,
            "-".into(),
            fallback_hyphen_layout("-", 0),
        )
        .unwrap(),
    ]);
    layout.break_all_lines(None);
    let actual = *layout.get(0).unwrap().metrics();
    let expected = *reference.get(0).unwrap().metrics();
    assert_eq!(actual.ascent, expected.ascent);
    assert_eq!(actual.descent, expected.descent);
    assert_eq!(actual.line_height, expected.line_height);
    assert_eq!(actual.advance, expected.advance);
}
