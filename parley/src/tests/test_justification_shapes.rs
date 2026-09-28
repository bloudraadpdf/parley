// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, LayoutContext, StyleProperty};

#[test]
fn alternative_features_retain_original_glyphs_and_source_ranges() {
    let mut fonts = create_font_context();
    let mut context: LayoutContext<ColorBrush> = LayoutContext::new();
    let features = [crate::FontFeature {
        tag: crate::setting::Tag::from_bytes(*b"liga"),
        value: 0,
    }];
    let text = "office";
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push(
        StyleProperty::FontFeaturesForJustification((&features).into()),
        2..4,
    );
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    assert!(layout.has_justification_shape_candidates());
    assert_eq!(layout.data.runs.len(), 1);
    let glyphs = layout
        .lines()
        .flat_map(|line| line.runs())
        .map(|run| {
            run.clusters()
                .map(|cluster| cluster.glyphs().count())
                .sum::<usize>()
        })
        .sum::<usize>();
    assert_eq!(glyphs, 4);
    assert_eq!(layout.data.deferred_justification_shapes.len(), 1);
    let candidate = &layout.data.deferred_justification_shapes[0];
    assert_eq!(candidate.source.context, 0..text.len());
    assert_eq!(candidate.alternatives.len(), 1);
    assert_eq!(candidate.alternatives[0].source, 2..4);
    assert_eq!(candidate.source.character_infos.len(), text.chars().count());
}

#[test]
fn deferred_alternative_changes_only_eligible_source_ligature() {
    let mut fonts = create_font_context();
    let mut context: LayoutContext<ColorBrush> = LayoutContext::new();
    let features = [crate::FontFeature {
        tag: crate::setting::Tag::from_bytes(*b"liga"),
        value: 0,
    }];
    let text = "office office";
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::FontFeaturesForJustification(
        (&features).into(),
    ));
    let mut layout = builder.build(text);
    let original = layout.data.clusters.clone();
    prepare(&mut context, &mut layout, &[2, 3]);
    assert_eq!(layout.data.clusters, original);
    let shape = layout.data.deferred_justification_shapes[0]
        .prepared
        .as_ref()
        .expect("eligible source boundary");
    assert_eq!(shape.clusters.len(), original.len());
    assert_eq!(
        shape
            .clusters
            .iter()
            .map(|cluster| cluster.text_offset)
            .collect::<Vec<_>>(),
        original
            .iter()
            .map(|cluster| cluster.text_offset)
            .collect::<Vec<_>>()
    );
    assert!(!shape.clusters[1].is_ligature_start());
    assert!(!shape.clusters[2].is_ligature_component());
    assert!(shape.clusters[8].is_ligature_start());
    assert!(shape.clusters[9].is_ligature_component());
}

#[test]
fn candidate_measurement_restores_tracking_and_tab_mutations_until_commit() {
    let mut fonts = create_font_context();
    let mut context: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "office\t office office";
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::LetterSpacing(2.0));
    let mut layout = builder.build(text);
    let original_clusters = layout.data.clusters.clone();
    let original_glyphs = layout.data.glyphs.clone();
    let mut reference = layout.clone();
    reference.break_all_lines(Some(90.0));
    let candidate = {
        let mut breaker = layout.break_lines();
        breaker
            .preview_shape_candidate(90.0, crate::LineTabOrigin::ZERO)
            .expect("first line")
    };
    assert_eq!(layout.data.clusters, original_clusters);
    assert_eq!(layout.data.glyphs, original_glyphs);
    assert_eq!(layout.len(), 0);
    let mut breaker = layout.break_lines();
    breaker.commit_shape_candidate(candidate);
    breaker.break_remaining(90.0);
    assert_eq!(layout.data, reference.data);
}

fn office_layout(
    context: &mut LayoutContext<ColorBrush>,
    fonts: &mut crate::FontContext,
    text: &str,
    disable: bool,
) -> crate::Layout<ColorBrush> {
    office_layout_with_box(context, fonts, text, disable, None)
}

fn office_layout_with_box(
    context: &mut LayoutContext<ColorBrush>,
    fonts: &mut crate::FontContext,
    text: &str,
    disable: bool,
    inline_box: Option<crate::InlineBox>,
) -> crate::Layout<ColorBrush> {
    let features = [crate::FontFeature {
        tag: crate::setting::Tag::from_bytes(*b"liga"),
        value: 0,
    }];
    let mut builder = context.ranged_builder(fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::FontFeaturesForJustification(
        (&features).into(),
    ));
    if disable {
        builder.push_default(StyleProperty::FontFeatures(features.as_slice().into()));
    }
    if let Some(inline_box) = inline_box {
        builder.push_inline_box(inline_box);
    }
    builder.build(text)
}

fn line_glyph_count(layout: &crate::Layout<ColorBrush>, line: usize) -> usize {
    layout
        .get(line)
        .expect("line")
        .runs()
        .map(|run| {
            run.clusters()
                .map(|cluster| cluster.glyphs().count())
                .sum::<usize>()
        })
        .sum()
}

#[test]
fn expanded_line_selects_alternative_and_unexpanded_line_keeps_original() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut layout = office_layout(&mut context, &mut fonts, "office office", false);
    assert_eq!(layout.justification_shape_policy(), None);
    prepare(
        &mut context,
        &mut layout,
        &[1, 2, 3, 4, 5, 8, 9, 10, 11, 12],
    );
    let policy = crate::JustificationShapePolicy {
        regular_lines: true,
        terminal_lines: false,
    };
    layout.set_justification_shape_policy(policy);
    assert_eq!(layout.justification_shape_policy(), Some(policy));
    layout.break_all_lines(Some(45.0));
    assert_eq!(layout.len(), 2);
    assert_eq!(line_glyph_count(&layout, 0), 7);
    assert_eq!(line_glyph_count(&layout, 1), 4);
    layout.break_all_lines(Some(500.0));
    assert_eq!(layout.len(), 1);
    assert_eq!(line_glyph_count(&layout, 0), 9);
    assert_eq!(layout.justification_shape_policy(), Some(policy));
    layout.set_justification_shape_policy(crate::JustificationShapePolicy::default());
    assert_eq!(layout.justification_shape_policy(), None);
}

#[test]
fn alternative_advances_choose_the_line_boundary() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut natural = office_layout(&mut context, &mut fonts, "office office", false);
    natural.break_all_lines(None);
    let mut alternative = office_layout(&mut context, &mut fonts, "office office", true);
    alternative.break_all_lines(None);
    assert!(
        alternative.width() > natural.width(),
        "fixture has wider separate letters"
    );
    let width = (natural.width() + alternative.width()) * 0.5;
    let mut layout = office_layout(&mut context, &mut fonts, "office office office", false);
    prepare(&mut context, &mut layout, &(1..20).collect::<Vec<_>>());
    layout.set_justification_shape_policy(crate::JustificationShapePolicy {
        regular_lines: true,
        terminal_lines: false,
    });
    layout.break_all_lines(Some(width));
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.get(0).unwrap().text_range(), 0..7);
    assert_eq!(layout.get(1).unwrap().text_range(), 7..20);
    assert_eq!(line_glyph_count(&layout, 0), 7);
    assert_eq!(line_glyph_count(&layout, 1), 9);
}

fn prepare(
    context: &mut LayoutContext<ColorBrush>,
    layout: &mut crate::Layout<ColorBrush>,
    boundaries: &[usize],
) {
    layout.set_justification_opportunities(
        boundaries
            .iter()
            .map(|&boundary| crate::JustificationOpportunity::BetweenUnits {
                before: crate::JustificationUnit::Text(boundary - 1..boundary),
                after: crate::JustificationUnit::Text(boundary..boundary + 1),
            })
            .collect(),
    );
    context.prepare_justification_shapes(layout);
}

#[test]
fn exact_original_fit_preserves_ligatures_and_exact_alternative_fit_selects_them() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut original = office_layout(&mut context, &mut fonts, "office", false);
    original.break_all_lines(None);
    let mut alternative = office_layout(&mut context, &mut fonts, "office", true);
    alternative.break_all_lines(None);
    let mut layout = office_layout(&mut context, &mut fonts, "office", false);
    prepare(&mut context, &mut layout, &[1, 2, 3, 4, 5]);
    layout.set_justification_shape_policy(crate::JustificationShapePolicy {
        regular_lines: true,
        terminal_lines: true,
    });
    layout.break_all_lines(Some(original.width()));
    assert_eq!(line_glyph_count(&layout, 0), 4);
    assert!(!layout.get(0).unwrap().justification_shape_fallback());
    layout.break_all_lines(Some(alternative.width()));
    assert_eq!(line_glyph_count(&layout, 0), 6);
    assert_eq!(layout.width(), alternative.width());
    assert!(!layout.get(0).unwrap().justification_shape_fallback());
    layout.break_all_lines(Some((original.width() + alternative.width()) * 0.5));
    assert_eq!(line_glyph_count(&layout, 0), 4);
    assert_eq!(layout.width(), original.width());
    assert!(layout.get(0).unwrap().justification_shape_fallback());
}

#[test]
fn selected_shapes_survive_reversion_and_partial_drop() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut layout = office_layout(&mut context, &mut fonts, "office office", false);
    prepare(&mut context, &mut layout, &(1..13).collect::<Vec<_>>());
    layout.set_justification_shape_policy(crate::JustificationShapePolicy {
        regular_lines: true,
        terminal_lines: false,
    });
    let mut reference = layout.clone();
    {
        let mut breaker = reference.break_lines();
        breaker
            .break_next(45.0, crate::LineTabOrigin::ZERO)
            .unwrap();
        breaker
            .break_next(500.0, crate::LineTabOrigin::ZERO)
            .unwrap();
    }
    {
        let mut breaker = layout.break_lines();
        breaker
            .break_next(45.0, crate::LineTabOrigin::ZERO)
            .unwrap();
        let _rejected = breaker
            .preview_shape_candidate(500.0, crate::LineTabOrigin::ZERO)
            .unwrap();
        assert!(breaker.revert());
        breaker
            .break_next(45.0, crate::LineTabOrigin::ZERO)
            .unwrap();
    }
    assert_eq!(layout.len(), 1);
    assert_eq!(line_glyph_count(&layout, 0), 7);
    {
        let mut breaker = layout.break_lines();
        breaker
            .break_next(45.0, crate::LineTabOrigin::ZERO)
            .unwrap();
        breaker
            .break_next(45.0, crate::LineTabOrigin::ZERO)
            .unwrap();
        assert!(
            breaker
                .break_next(45.0, crate::LineTabOrigin::ZERO)
                .is_none()
        );
        assert!(breaker.revert());
        breaker
            .break_next(500.0, crate::LineTabOrigin::ZERO)
            .unwrap();
    }
    assert_eq!(layout, reference);
    {
        let mut breaker = layout.break_lines();
        breaker
            .break_next(45.0, crate::LineTabOrigin::ZERO)
            .unwrap();
        breaker.break_next_with_length(7).unwrap();
        assert!(breaker.revert());
        breaker
            .break_next(500.0, crate::LineTabOrigin::ZERO)
            .unwrap();
    }
    assert_eq!(layout, reference);
}

#[test]
fn forced_unexpanded_lines_and_preserved_tabs_keep_original_shapes() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    for text in ["office\noffice", "office\toffice"] {
        let mut original = office_layout(&mut context, &mut fonts, text, false);
        original.break_all_lines(Some(200.0));
        let mut layout = office_layout(&mut context, &mut fonts, text, false);
        prepare(&mut context, &mut layout, &(1..13).collect::<Vec<_>>());
        layout.set_justification_shape_policy(crate::JustificationShapePolicy {
            regular_lines: true,
            terminal_lines: text.contains('\t'),
        });
        layout.break_all_lines(Some(200.0));
        assert_eq!(layout.len(), original.len());
        for index in 0..layout.len() {
            assert_eq!(
                line_glyph_count(&layout, index),
                line_glyph_count(&original, index)
            );
            assert_eq!(
                layout.get(index).unwrap().metrics(),
                original.get(index).unwrap().metrics()
            );
            assert!(!layout.get(index).unwrap().justification_shape_fallback());
        }
    }
}

fn glyph_positions(layout: &crate::Layout<ColorBrush>, index: usize) -> Vec<(u32, f32, f32)> {
    layout
        .get(index)
        .unwrap()
        .items()
        .flat_map(|item| match item {
            crate::PositionedLayoutItem::GlyphRun(run) => run
                .positioned_glyphs()
                .map(|glyph| (glyph.id, glyph.x, glyph.advance))
                .collect(),
            crate::PositionedLayoutItem::InlineBox(_) => Vec::new(),
        })
        .collect()
}

#[test]
fn variant_selection_preserves_bidi_atomic_tracking_geometry() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let text = "אב office office";
    for spacing in [2.0, -2.0] {
        let inline_box = crate::InlineBox::new(7, 11, 6.0, 10.0).with_letter_spacing(spacing);
        let mut reference = office_layout_with_box(
            &mut context,
            &mut fonts,
            text,
            true,
            Some(inline_box.clone()),
        );
        reference.break_all_lines(Some(65.0));
        let mut layout =
            office_layout_with_box(&mut context, &mut fonts, text, false, Some(inline_box));
        let boundaries = (6..11).chain(13..18).collect::<Vec<_>>();
        prepare(&mut context, &mut layout, &boundaries);
        layout.set_justification_shape_policy(crate::JustificationShapePolicy {
            regular_lines: true,
            terminal_lines: false,
        });
        layout.break_all_lines(Some(65.0));
        assert_eq!(layout.len(), 2);
        assert_eq!(
            layout.get(0).unwrap().text_range(),
            reference.get(0).unwrap().text_range()
        );
        assert_eq!(glyph_positions(&layout, 0), glyph_positions(&reference, 0));
        let boxes = |layout: &crate::Layout<ColorBrush>| {
            layout
                .get(0)
                .unwrap()
                .items()
                .filter_map(|item| match item {
                    crate::PositionedLayoutItem::InlineBox(value) => {
                        Some((value.id, value.x, value.width))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(boxes(&layout), boxes(&reference));
        assert_eq!(boxes(&layout).len(), 1);
        let first = layout.clone();
        layout.break_all_lines(Some(500.0));
        layout.break_all_lines(Some(65.0));
        assert_eq!(layout, first);
    }
}

#[test]
fn unexpanded_lines_do_not_request_deferred_shaping() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut layout = office_layout(&mut context, &mut fonts, "office", false);
    layout.set_justification_opportunities(
        (1..6)
            .map(|boundary| crate::JustificationOpportunity::BetweenUnits {
                before: crate::JustificationUnit::Text(boundary - 1..boundary),
                after: crate::JustificationUnit::Text(boundary..boundary + 1),
            })
            .collect(),
    );
    layout.break_all_lines(Some(100.0));
    assert!(
        !layout.needs_justification_shape_preparation(crate::JustificationShapePolicy {
            regular_lines: true,
            terminal_lines: false,
        })
    );
    assert!(
        layout.needs_justification_shape_preparation(crate::JustificationShapePolicy {
            regular_lines: true,
            terminal_lines: true,
        })
    );
    assert!(
        layout
            .data
            .deferred_justification_shapes
            .iter()
            .all(|candidate| candidate.prepared.is_none())
    );
    layout.break_all_lines(None);
    assert!(
        !layout.needs_justification_shape_preparation(crate::JustificationShapePolicy {
            regular_lines: true,
            terminal_lines: true,
        })
    );
}

#[test]
fn source_fit_projection_survives_variant_selection_and_can_be_removed() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let text = "office office";
    for projection_first in [false, true] {
        let mut layout = office_layout(&mut context, &mut fonts, text, false);
        prepare(
            &mut context,
            &mut layout,
            &[1, 2, 3, 4, 5, 8, 9, 10, 11, 12],
        );
        let project = |layout: &mut crate::Layout<ColorBrush>| {
            layout.set_source_cluster_fit_advances(
                text.char_indices()
                    .map(|(index, _)| crate::SourceClusterFitAdvance::new(index, 20.0).unwrap())
                    .collect(),
            );
        };
        let policy = crate::JustificationShapePolicy {
            regular_lines: true,
            terminal_lines: true,
        };
        if projection_first {
            project(&mut layout);
        }
        layout.set_justification_shape_policy(policy);
        if !projection_first {
            project(&mut layout);
        }
        layout.break_all_lines(Some(150.0));
        assert_eq!(layout.len(), 2, "projection first: {projection_first}");
        for _ in 0..2 {
            layout.break_all_lines(Some(150.0));
            assert_eq!(layout.len(), 2);
            assert_eq!(line_glyph_count(&layout, 0), 7);
        }
        layout.set_source_cluster_fit_advances(Vec::new());
        layout.break_all_lines(Some(150.0));
        assert_eq!(layout.len(), 1);
        assert_eq!(line_glyph_count(&layout, 0), 13);
    }
}

#[test]
fn changed_potential_opportunities_invalidate_prepared_alternatives() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut layout = office_layout(&mut context, &mut fonts, "office", false);
    prepare(&mut context, &mut layout, &[1, 2, 3, 4, 5]);
    let policy = crate::JustificationShapePolicy {
        regular_lines: true,
        terminal_lines: true,
    };
    layout.set_justification_shape_policy(policy);
    layout.break_all_lines(Some(100.0));
    assert_eq!(line_glyph_count(&layout, 0), 6);
    layout.set_justification_opportunities(Vec::new());
    context.prepare_justification_shapes(&mut layout);
    assert!(
        layout
            .data
            .deferred_justification_shapes
            .iter()
            .all(|shape| shape.prepared.is_none())
    );
    layout.set_justification_shape_policy(policy);
    layout.break_all_lines(Some(100.0));
    assert_eq!(line_glyph_count(&layout, 0), 4);
}

#[test]
fn unexpandable_variant_uses_last_alignment_without_expanding_original_glyphs() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut original = office_layout(&mut context, &mut fonts, "office", false);
    original.break_all_lines(None);
    let mut alternative = office_layout(&mut context, &mut fonts, "office", true);
    alternative.break_all_lines(None);
    let width = (original.width() + alternative.width()) * 0.5;
    let mut reference = office_layout(&mut context, &mut fonts, "office office", false);
    reference.break_all_lines(Some(width));
    let original_positions = glyph_positions(&reference, 0);
    let free = width - original.width();
    for (last, fraction) in [
        (crate::Alignment::Start, 0.0),
        (crate::Alignment::End, 1.0),
        (crate::Alignment::Center, 0.5),
        (crate::Alignment::Justify, 0.5),
    ] {
        let mut layout = office_layout(&mut context, &mut fonts, "office office", false);
        prepare(&mut context, &mut layout, &(1..13).collect::<Vec<_>>());
        layout.set_justification_shape_policy(crate::JustificationShapePolicy {
            regular_lines: true,
            terminal_lines: last == crate::Alignment::Justify,
        });
        layout.break_all_lines(Some(width));
        assert!(layout.get(0).unwrap().justification_shape_fallback());
        let options = crate::AlignmentOptions {
            justification_mode: crate::JustificationMode::SourceOpportunities,
            last_line_alignment: Some(last),
            ..Default::default()
        };
        layout.align(Some(width), crate::Alignment::Justify, options);
        assert_eq!(
            layout.get(0).unwrap().requested_alignment(),
            Some(crate::Alignment::Justify)
        );
        assert_eq!(
            layout.get(0).unwrap().alignment(),
            Some(if last == crate::Alignment::Justify {
                crate::Alignment::Center
            } else {
                last
            })
        );
        let positions = glyph_positions(&layout, 0);
        assert_eq!(positions.len(), original_positions.len());
        for (actual, expected) in positions.iter().zip(&original_positions) {
            assert_eq!(actual.0, expected.0);
            assert!(
                (actual.1 - expected.1 - free * fraction).abs() < 0.0001,
                "{last:?}: {actual:?} vs {expected:?}"
            );
            assert_eq!(actual.2, expected.2);
        }
        // Undo and reapply must preserve the explicit no-expansion outcome.
        let aligned = layout.clone();
        layout.align(Some(width), crate::Alignment::Justify, options);
        assert_eq!(layout, aligned);
    }
}

#[test]
fn variant_fitting_uses_physical_source_projection_instead_of_rendered_advance() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let text = "office";
    let mut layout = office_layout(&mut context, &mut fonts, text, false);
    layout.break_all_lines(None);
    let width = layout.width() - 1.0;
    prepare(&mut context, &mut layout, &[1, 2, 3, 4, 5]);
    layout.set_source_cluster_fit_advances(
        (0..text.len())
            .map(|index| crate::SourceClusterFitAdvance::new(index, 1.0).unwrap())
            .collect(),
    );
    layout.set_justification_shape_policy(crate::JustificationShapePolicy {
        regular_lines: true,
        terminal_lines: true,
    });
    layout.break_all_lines(Some(width));
    assert_eq!(line_glyph_count(&layout, 0), 6);
    assert!(!layout.get(0).unwrap().justification_shape_fallback());
}

fn wrapping_ligature_layout(
    context: &mut LayoutContext<ColorBrush>,
    fonts: &mut crate::FontContext,
    text: &str,
    disabled: bool,
) -> crate::Layout<ColorBrush> {
    let mut features = alloc::vec![crate::FontFeature::new(
        crate::setting::Tag::from_bytes(*b"dlig"),
        1
    )];
    let alternatives = [*b"liga", *b"dlig"]
        .map(|tag| crate::FontFeature::new(crate::setting::Tag::from_bytes(tag), 0));
    if disabled {
        features.extend(alternatives);
    }
    let mut builder = context.ranged_builder(fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::OverflowWrap(crate::OverflowWrap::Anywhere));
    builder.push_default(StyleProperty::FontFeatures(features.as_slice().into()));
    builder.push_default(StyleProperty::FontFeaturesForJustification(
        (&alternatives).into(),
    ));
    let mut layout = builder.build(text);
    let units = text
        .char_indices()
        .map(|(start, ch)| start..start + ch.len_utf8())
        .collect::<Vec<_>>();
    layout.set_justification_opportunities(
        units
            .windows(2)
            .map(|units| crate::JustificationOpportunity::BetweenUnits {
                before: crate::JustificationUnit::Text(units[0].clone()),
                after: crate::JustificationUnit::Text(units[1].clone()),
            })
            .collect(),
    );
    layout
}

#[test]
fn internal_ligature_cut_reshapes_default_suffix_on_unexpanded_last_line() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut prefix = wrapping_ligature_layout(&mut context, &mut fonts, "oof", true);
    prefix.break_all_lines(None);
    for tail in [alloc::string::String::new(), " office".repeat(512)] {
        let suffix_text = alloc::format!("fice{tail}");
        let mut suffix = wrapping_ligature_layout(&mut context, &mut fonts, &suffix_text, false);
        suffix.break_all_lines(None);
        assert_eq!(line_glyph_count(&suffix, 0), 3 + tail.len() / 7 * 5);
        let text = alloc::format!("ooffice{tail}");
        let mut layout = wrapping_ligature_layout(&mut context, &mut fonts, &text, false);
        context.prepare_justification_shapes(&mut layout);
        layout.set_justification_shape_policy(crate::JustificationShapePolicy {
            regular_lines: true,
            terminal_lines: false,
        });
        let baseline_glyphs = layout.data.glyphs.len();
        let boundary_limit = layout.data.deferred_justification_shapes[0]
            .source
            .safe_concat_boundaries
            .iter()
            .copied()
            .filter(|boundary| *boundary > 3)
            .nth(1)
            .unwrap_or(text.len());
        let break_selected = |layout: &mut crate::Layout<ColorBrush>| {
            let mut breaker = layout.break_lines();
            breaker
                .break_next(prefix.width() + 0.1, crate::LineTabOrigin::ZERO)
                .unwrap();
            breaker
                .break_next(f32::MAX, crate::LineTabOrigin::ZERO)
                .unwrap();
            assert!(breaker.revert());
            breaker
                .break_next(f32::MAX, crate::LineTabOrigin::ZERO)
                .unwrap();
        };
        break_selected(&mut layout);
        assert_eq!(layout.get(0).unwrap().text_range(), 0..3);
        assert_eq!(layout.get(1).unwrap().text_range(), 3..text.len());
        assert_eq!(glyph_positions(&layout, 1), glyph_positions(&suffix, 0));
        // The Roboto fixture reaches a verified safe boundary within the
        // first lookahead span. Its 512 unaffected words add no suffix work.
        assert!(layout.data.glyphs.len() - baseline_glyphs <= boundary_limit - 3);
        let selected = layout.clone();
        break_selected(&mut layout);
        assert_eq!(layout, selected);
    }
}

#[test]
fn narrower_alternative_cannot_turn_emergency_line_into_unexpanded_terminal_line() {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    // Roboto's discretionary long-s/t ligature is 1262 units; its components
    // total 1177. Removing it therefore makes this alternative narrower.
    let text = "xxſt";
    let mut original = wrapping_ligature_layout(&mut context, &mut fonts, text, false);
    original.break_all_lines(None);
    let mut disabled = wrapping_ligature_layout(&mut context, &mut fonts, text, true);
    disabled.break_all_lines(None);
    assert!(disabled.width() < original.width());
    assert_eq!(line_glyph_count(&original, 0), 3);
    assert_eq!(line_glyph_count(&disabled, 0), 4);
    let width = (original.width() + disabled.width()) * 0.5;
    let mut layout = wrapping_ligature_layout(&mut context, &mut fonts, text, false);
    context.prepare_justification_shapes(&mut layout);
    layout.set_justification_shape_policy(crate::JustificationShapePolicy {
        regular_lines: true,
        terminal_lines: false,
    });
    layout.break_all_lines(Some(width));
    assert_eq!(layout.len(), 2);
    assert_eq!(
        layout.get(0).unwrap().break_reason(),
        crate::BreakReason::Emergency
    );
    assert_eq!(
        layout.get(1).unwrap().break_reason(),
        crate::BreakReason::None
    );
    assert_eq!(line_glyph_count(&layout, 1), 1);
}
