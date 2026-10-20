// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Context for layout.

use alloc::{vec, vec::Vec};
use core::num::NonZeroU16;

#[cfg(feature = "libm")]
#[allow(unused_imports)]
use core_maths::CoreFloat;

use super::FontContext;
use super::builder::{RangedBuilder, StyleRunBuilder};
use super::resolve::tree::TreeStyleBuilder;
use super::resolve::{RangedStyleBuilder, ResolveContext, ResolvedStyle, StyleRun};
use super::style::{Brush, TextStyle};

use crate::analysis::{AnalysisDataSources, CharInfo};
use crate::bidi::BidiResolver;
use crate::builder::TreeBuilder;
use crate::inline_box::InlineBox;
use crate::shape::ShapeContext;
use parlance::BaseDirection;

/// Optional projection of base font-metric advances onto a fixed em grid.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FontMetricAdvanceQuantization {
    /// Project every shaping run.
    All(NonZeroU16),
    /// Project only runs on the alphabetic shaping path: Latin, Greek and
    /// Cyrillic. Common and inherited characters follow the script assigned to
    /// their surrounding run.
    Alphabetic(NonZeroU16),
}

impl FontMetricAdvanceQuantization {
    pub(crate) fn denominator(self) -> NonZeroU16 {
        match self {
            Self::All(denominator) | Self::Alphabetic(denominator) => denominator,
        }
    }

    pub(crate) fn applies_to_script(self, script: icu_properties::props::Script) -> bool {
        use icu_properties::props::Script;
        matches!(self, Self::All(_))
            || matches!(script, Script::Latin | Script::Greek | Script::Cyrillic)
    }
}

/// The grid to which the ascent, descent and line gap of each run round.
///
/// Browsers round these font metrics to whole CSS pixels before they lay out
/// a line: the `normal` line height is the sum of the 3 rounded metrics. A
/// half rounds toward zero, as in Chrome 148 on macOS (Ahem at 17.5 px: a
/// 14 px ascent and a 3 px descent).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct VerticalMetricGrid(f32);

impl VerticalMetricGrid {
    /// A grid of `unit` layout units, if `unit` is finite and positive.
    pub fn new(unit: f32) -> Option<Self> {
        (unit.is_finite() && unit > 0.0).then_some(Self(unit))
    }

    pub(crate) fn round(self, value: f32) -> f32 {
        let units = value / self.0;
        units.signum() * (units.abs() - 0.5).ceil() * self.0
    }
}

/// Shared scratch space used when constructing text layouts.
///
/// This type is designed to be a global resource with only one per-application (or per-thread).
pub struct LayoutContext<B: Brush = [u8; 4]> {
    pub(crate) rcx: ResolveContext,
    pub(crate) style_table: Vec<ResolvedStyle<B>>,
    pub(crate) style_runs: Vec<StyleRun>,
    pub(crate) inline_boxes: Vec<InlineBox>,
    pub(crate) shaping_boundaries: Vec<usize>,
    pub(crate) inline_owner_shaping: Vec<crate::layout::InlineOwnerShaping>,
    pub(crate) bidi: BidiResolver,
    pub(crate) direction: BaseDirection,

    // Reusable style builders (to amortise allocations)
    pub(crate) ranged_style_builder: RangedStyleBuilder<B>,
    pub(crate) tree_style_builder: TreeStyleBuilder<B>,

    // u16: style index for character
    pub(crate) info: Vec<(CharInfo, u16)>,
    pub(crate) scx: ShapeContext,

    // Unicode analysis data sources (provided by icu)
    pub(crate) analysis_data_sources: AnalysisDataSources,

    // Optional fixed grid used by consumers when serializing font advances.
    pub(crate) font_metric_advance_quantization: Option<FontMetricAdvanceQuantization>,
    // Whether line fit uses nominal font metrics while glyph positioning and
    // emitted advances retain shaping adjustments such as kerning.
    pub(crate) nominal_font_metric_line_breaks: bool,
    pub(crate) vertical_metric_grid: Option<VerticalMetricGrid>,
    #[cfg(test)]
    pub(crate) owner_box_reads: usize,
}

impl<B: Brush> LayoutContext<B> {
    pub fn new() -> Self {
        Self {
            rcx: ResolveContext::default(),
            style_table: vec![],
            style_runs: vec![],
            inline_boxes: vec![],
            shaping_boundaries: vec![],
            inline_owner_shaping: vec![],
            bidi: BidiResolver::new(),
            direction: BaseDirection::default(),
            ranged_style_builder: RangedStyleBuilder::default(),
            tree_style_builder: TreeStyleBuilder::default(),
            info: vec![],
            analysis_data_sources: AnalysisDataSources::new(),
            scx: ShapeContext::default(),
            font_metric_advance_quantization: None,
            nominal_font_metric_line_breaks: false,
            vertical_metric_grid: None,
            #[cfg(test)]
            owner_box_reads: 0,
        }
    }

    /// Projects the base font-metric component of shaped glyph advances onto
    /// a fixed-width grid whose denominator is expressed in em units.
    ///
    /// Positioning adjustments and authored letter/word spacing retain their
    /// full precision. For example, a denominator of 1000 projects base
    /// advances to integer thousandths of an em while preserving kerning.
    pub fn set_font_metric_advance_quantization(
        &mut self,
        quantization: Option<FontMetricAdvanceQuantization>,
    ) {
        self.font_metric_advance_quantization = quantization;
    }

    /// Select nominal font-metric advances for line-fit decisions while
    /// retaining fully shaped advances for glyph positioning and line output.
    ///
    /// This models producers that choose line boundaries on an hmtx width
    /// grid, then apply GPOS/kern adjustments when positioning the accepted
    /// line. When fixed metric quantization is configured, the nominal fit
    /// advances use that same grid.
    pub fn set_nominal_font_metric_line_breaks(&mut self, enabled: bool) {
        self.nominal_font_metric_line_breaks = enabled;
    }

    /// Round the ascent, descent and line gap of each run to `grid`.
    pub fn set_vertical_metric_grid(&mut self, grid: Option<VerticalMetricGrid>) {
        self.vertical_metric_grid = grid;
    }

    fn resolve_style_set(
        &mut self,
        font_ctx: &mut FontContext,
        scale: f32,
        raw_style: &TextStyle<'_, '_, B>,
    ) -> ResolvedStyle<B> {
        self.rcx
            .resolve_entire_style_set(font_ctx, raw_style, scale)
    }

    /// Create a ranged style layout builder.
    ///
    /// Set `quantize` as `true` to have the layout coordinates aligned to pixel boundaries.
    /// That is the easiest way to avoid blurry text and to receive ready-to-paint layout metrics.
    ///
    /// For advanced rendering use cases you can set `quantize` as `false` and receive
    /// fractional coordinates. This ensures the most accurate results if you want to perform
    /// some post-processing on the coordinates before painting. To avoid blurry text you will
    /// still need to quantize the coordinates just before painting.
    ///
    /// Your should round at least the following:
    /// * Glyph run baseline
    /// * Inline box baseline
    ///   - `box.y = (box.y + box.height).round() - box.height`
    /// * Selection geometry's `y0` & `y1`
    /// * Cursor geometry's `y0` & `y1`
    ///
    /// Keep in mind that for the simple `f32::round` to be effective,
    /// you need to first ensure the coordinates are in physical pixel space.
    pub fn ranged_builder<'a>(
        &'a mut self,
        fcx: &'a mut FontContext,
        text: &'a str,
        scale: f32,
        quantize: bool,
    ) -> RangedBuilder<'a, B> {
        self.begin();

        let resolved_root_style = self.resolve_style_set(fcx, scale, &TextStyle::default());
        self.ranged_style_builder
            .begin(resolved_root_style, text.len());

        fcx.source_cache.prune(128, false);

        RangedBuilder {
            scale,
            quantize,
            lcx: self,
            fcx,
        }
    }

    /// Create a builder for constructing a layout from indexed style runs.
    ///
    /// Unlike [`Self::ranged_builder`], this builder expects callers to provide:
    /// - a style table of fully specified [`TextStyle`] values (via [`StyleRunBuilder::push_style`])
    /// - a complete sequence of **contiguous**, **non-overlapping** spans that cover
    ///   `0..text.len()` and reference style indices (via [`StyleRunBuilder::push_style_run`])
    ///
    /// Parley then skips its internal range-splitting logic.
    pub fn style_run_builder<'a>(
        &'a mut self,
        fcx: &'a mut FontContext,
        text: &'a str,
        scale: f32,
        quantize: bool,
    ) -> StyleRunBuilder<'a, B> {
        self.begin();

        fcx.source_cache.prune(128, false);

        StyleRunBuilder {
            scale,
            quantize,
            len: text.len(),
            lcx: self,
            fcx,
            cursor: 0,
        }
    }

    /// Create a tree style layout builder.
    ///
    /// Set `quantize` as `true` to have the layout coordinates aligned to pixel boundaries.
    /// That is the easiest way to avoid blurry text and to receive ready-to-paint layout metrics.
    ///
    /// For advanced rendering use cases you can set `quantize` as `false` and receive
    /// fractional coordinates. This ensures the most accurate results if you want to perform
    /// some post-processing on the coordinates before painting. To avoid blurry text you will
    /// still need to quantize the coordinates just before painting.
    ///
    /// Your should round at least the following:
    /// * Glyph run baseline
    /// * Inline box baseline
    ///   - `box.y = (box.y + box.height).round() - box.height`
    /// * Selection geometry's `y0` & `y1`
    /// * Cursor geometry's `y0` & `y1`
    ///
    /// Keep in mind that for the simple `f32::round` to be effective,
    /// you need to first ensure the coordinates are in physical pixel space.
    pub fn tree_builder<'a>(
        &'a mut self,
        fcx: &'a mut FontContext,
        scale: f32,
        quantize: bool,
        root_style: &TextStyle<'_, '_, B>,
    ) -> TreeBuilder<'a, B> {
        self.begin();

        let resolved_root_style = self.resolve_style_set(fcx, scale, root_style);
        self.tree_style_builder.begin(resolved_root_style);

        fcx.source_cache.prune(128, false);

        TreeBuilder {
            scale,
            quantize,
            lcx: self,
            fcx,
        }
    }

    /// Drops the buffers that only a build reads, so a long text does not keep them after its build.
    pub(crate) fn release_build_buffers(&mut self) {
        self.ranged_style_builder.release_buffers();
        self.style_table = Vec::new();
        self.style_runs = Vec::new();
        self.info = Vec::new();
    }

    fn begin(&mut self) {
        self.rcx.clear();
        self.style_table.clear();
        self.style_runs.clear();
        self.inline_boxes.clear();
        self.shaping_boundaries.clear();
        self.inline_owner_shaping.clear();
        self.info.clear();
        self.bidi.clear();
        self.direction = BaseDirection::default();
    }
}

impl<B: Brush> Default for LayoutContext<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Brush> Clone for LayoutContext<B> {
    fn clone(&self) -> Self {
        // Scratch allocations are deliberately not cloned, but persistent
        // caller configuration must survive cloning.
        let mut cloned = Self::new();
        cloned.font_metric_advance_quantization = self.font_metric_advance_quantization;
        cloned.nominal_font_metric_line_breaks = self.nominal_font_metric_line_breaks;
        cloned.vertical_metric_grid = self.vertical_metric_grid;
        cloned
    }
}
