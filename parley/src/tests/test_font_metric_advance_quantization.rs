// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Projection of shaped base advances onto a consumer's fixed font-width grid.

use alloc::format;
use core::num::NonZeroU16;

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    FontFamily, FontMetricAdvanceQuantization, LayoutContext, PositionedLayoutItem, StyleProperty,
};

const FIXED_WIDTH_DENOMINATOR: NonZeroU16 = NonZeroU16::new(1000).unwrap();

fn measure_advance(
    lcx: &mut LayoutContext<ColorBrush>,
    fcx: &mut crate::FontContext,
    text: &str,
    quantization: Option<FontMetricAdvanceQuantization>,
    word_spacing: Option<f32>,
) -> f32 {
    lcx.set_font_metric_advance_quantization(quantization);
    let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    if let Some(spacing) = word_spacing {
        builder.push_default(StyleProperty::WordSpacing(spacing));
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let advance = layout.lines().next().expect("line").metrics().advance;
    advance
}

#[test]
fn projected_font_metric_advances_drive_the_line_break() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha beta";

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(12.0));
        builder.build(text)
    };

    let mut natural = build(&mut lcx, &mut fcx);
    natural.break_all_lines(None);
    let natural_advance = natural
        .lines()
        .next()
        .expect("natural line")
        .metrics()
        .advance;

    lcx.set_font_metric_advance_quantization(Some(FontMetricAdvanceQuantization::All(
        FIXED_WIDTH_DENOMINATOR,
    )));
    let mut projected = build(&mut lcx, &mut fcx);
    projected.break_all_lines(None);
    let projected_advance = projected
        .lines()
        .next()
        .expect("projected line")
        .metrics()
        .advance;
    assert!(
        projected_advance < natural_advance,
        "floor-projecting base hmtx advances must remove their sub-grid residual: \
         natural={natural_advance}, projected={projected_advance}",
    );

    let line_width = projected_advance + f32::EPSILON;
    lcx.set_font_metric_advance_quantization(None);
    let mut natural_wrap = build(&mut lcx, &mut fcx);
    natural_wrap.break_all_lines(Some(line_width));
    assert_eq!(
        natural_wrap.len(),
        2,
        "the unprojected FUnit residual must wrap beta"
    );

    lcx.set_font_metric_advance_quantization(Some(FontMetricAdvanceQuantization::All(
        FIXED_WIDTH_DENOMINATOR,
    )));
    let mut projected_fit = build(&mut lcx, &mut fcx);
    projected_fit.break_all_lines(Some(line_width));
    assert_eq!(
        projected_fit.len(),
        1,
        "line breaking must consume the same projected advances exposed to the consumer",
    );
}

#[test]
fn fixed_grid_line_fit_is_stable_across_many_clusters() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let quantization = Some(FontMetricAdvanceQuantization::All(FIXED_WIDTH_DENOMINATOR));

    // Derive the mathematical line width from one repeated ` space + i`
    // unit in f64. The full paragraph accumulates the same projected f32
    // advances in a different association order, which must not turn an
    // exact fixed-grid boundary into a wrap.
    let i_advance = measure_advance(&mut lcx, &mut fcx, "i", quantization, None);
    let pair_advance = measure_advance(&mut lcx, &mut fcx, "i i", quantization, None);
    let repeated_unit = f64::from(pair_advance - i_advance);
    let repetitions = 100usize;
    let width = (f64::from(i_advance) + repeated_unit * repetitions as f64) as f32;
    let text = format!("{}i", "i ".repeat(repetitions));

    lcx.set_font_metric_advance_quantization(quantization);
    let mut builder = lcx.ranged_builder(&mut fcx, &text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    let mut layout = builder.build(&text);
    layout.break_all_lines(Some(width));

    assert_eq!(
        layout.len(),
        1,
        "an exact fixed-grid boundary must not wrap from f32 accumulation: width={width}",
    );
}

#[test]
fn projection_preserves_shaping_adjustments() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    let residual = |text, lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        measure_advance(lcx, fcx, text, None, None)
            - measure_advance(
                lcx,
                fcx,
                text,
                Some(FontMetricAdvanceQuantization::All(FIXED_WIDTH_DENOMINATOR)),
                None,
            )
    };
    let pair_residual = residual("AV", &mut lcx, &mut fcx);
    let separate_residual = residual("A", &mut lcx, &mut fcx) + residual("V", &mut lcx, &mut fcx);

    assert!(
        (pair_residual - separate_residual).abs() < 0.0001,
        "projection must remove only base-metric residuals and leave kerning intact: \
         pair={pair_residual}, separate={separate_residual}",
    );
}

#[test]
fn nominal_metric_line_breaks_retain_kerned_output_advances() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "AV AV AV";
    let quantization = Some(FontMetricAdvanceQuantization::All(FIXED_WIDTH_DENOMINATOR));

    let shaped_advance = measure_advance(&mut lcx, &mut fcx, text, quantization, None);

    lcx.set_font_metric_advance_quantization(quantization);
    lcx.set_nominal_font_metric_line_breaks(true);
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    let mut nominal_fit = builder.build(text);
    nominal_fit.break_all_lines(None);
    let nominal_mode_output_advance = nominal_fit.lines().next().expect("line").metrics().advance;
    assert!(
        (nominal_mode_output_advance - shaped_advance).abs() < 0.0001,
        "the fit policy must not remove kerning from positioned output: shaped={shaped_advance}, nominal-mode={nominal_mode_output_advance}",
    );

    nominal_fit.break_all_lines(Some(shaped_advance + f32::EPSILON));
    assert!(
        nominal_fit.len() > 1,
        "nominal hmtx widths must drive wrapping even though the fully kerned line fits"
    );
}

#[test]
fn projection_preserves_authored_word_spacing() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let authored_spacing = 3.75;
    let plain = measure_advance(
        &mut lcx,
        &mut fcx,
        "alpha beta",
        Some(FontMetricAdvanceQuantization::All(FIXED_WIDTH_DENOMINATOR)),
        None,
    );
    let spaced = measure_advance(
        &mut lcx,
        &mut fcx,
        "alpha beta",
        Some(FontMetricAdvanceQuantization::All(FIXED_WIDTH_DENOMINATOR)),
        Some(authored_spacing),
    );

    assert!(
        (spaced - plain - authored_spacing).abs() < 0.0001,
        "fixed-grid projection must not quantize authored word spacing: \
         plain={plain}, spaced={spaced}",
    );
}

#[test]
fn projected_ligature_cluster_and_glyph_advances_stay_consistent() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    lcx.set_font_metric_advance_quantization(Some(FontMetricAdvanceQuantization::All(
        FIXED_WIDTH_DENOMINATOR,
    )));
    let text = "office";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let line = layout.lines().next().expect("line");
    let glyph_advance: f32 = line
        .items()
        .filter_map(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => Some(run.advance()),
            PositionedLayoutItem::InlineBox(_) => None,
        })
        .sum();

    assert!(
        (glyph_advance - line.metrics().advance).abs() < 0.0001,
        "projected ligature glyphs and clusters must describe the same advance: \
         glyphs={glyph_advance}, line={}",
        line.metrics().advance,
    );
}

#[test]
fn latin_projection_scope_leaves_complex_script_advances_untouched() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    let natural_arabic = measure_advance(&mut lcx, &mut fcx, "سلام", None, None);
    let scoped_arabic = measure_advance(
        &mut lcx,
        &mut fcx,
        "سلام",
        Some(FontMetricAdvanceQuantization::Latin(
            FIXED_WIDTH_DENOMINATOR,
        )),
        None,
    );
    assert_eq!(
        scoped_arabic, natural_arabic,
        "Latin metric projection must leave quality-shaped Arabic advances unchanged",
    );

    let natural_latin = measure_advance(&mut lcx, &mut fcx, "alpha", None, None);
    let scoped_latin = measure_advance(
        &mut lcx,
        &mut fcx,
        "alpha",
        Some(FontMetricAdvanceQuantization::Latin(
            FIXED_WIDTH_DENOMINATOR,
        )),
        None,
    );
    assert!(
        scoped_latin < natural_latin,
        "Latin metric projection must still project Latin base advances",
    );
}
