// Copyright 2025 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::InlineBox;
use crate::layout::alignment::align;
use crate::layout::alignment::align_per_line;
use crate::layout::alignment::unjustify;
use crate::layout::data::LayoutData;
use crate::layout::{
    DiscretionaryBreak, DiscretionaryBreakCondition, DiscretionaryFitAdvance, LineBreakOverride,
    NormalSoftWrapSelection, SourceClusterFitAdvance,
};
use crate::style::Brush;
use alloc::{string::String, sync::Arc, vec::Vec};
use core::cmp::Ordering;

use crate::layout::{
    ContentWidths, Style, alignment::Alignment, alignment::AlignmentOptions, line::Line,
    line_break::BreakLines, run::Run,
};
use crate::{IndentOptions, IndentStart};

/// A shaped string inserted only at a selected discretionary boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscretionaryBreakShape<B: Brush> {
    pub(crate) byte_index: usize,
    pub(crate) max_consecutive_lines: Option<u32>,
    text: String,
    condition: DiscretionaryBreakCondition,
    layout: Arc<Layout<B>>,
}

/// A shaped string inserted at the start of a line only when wrapping
/// selects the source boundary. The unbroken source remains unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct FollowingBreakShape<B: Brush> {
    pub(crate) byte_index: usize,
    text: String,
    layout: Arc<Layout<B>>,
}

impl<B: Brush> FollowingBreakShape<B> {
    /// Retain a single-line shape for a source boundary.
    pub fn new(byte_index: usize, text: String, layout: Layout<B>) -> Option<Self> {
        if byte_index == 0
            || layout.len() > 1
            || (layout.len() == 0 && !text.is_empty())
            || layout.data.text_len != text.len()
            || !layout.inline_boxes().is_empty()
            || !layout.full_width().is_finite()
            || layout.full_width() < 0.0
        {
            return None;
        }
        Some(Self {
            byte_index,
            text,
            layout: Arc::new(layout),
        })
    }

    /// Text used to shape this line-start material.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Immutable glyph shape used for fitting and drawing.
    pub fn layout(&self) -> &Layout<B> {
        &self.layout
    }
}

impl<B: Brush> DiscretionaryBreakShape<B> {
    /// Retain a single-line shape with its source text and break limit.
    pub fn new(
        byte_index: usize,
        max_consecutive_lines: Option<u32>,
        text: String,
        layout: Layout<B>,
    ) -> Option<Self> {
        if byte_index == 0
            || layout.len() > 1
            || (layout.len() == 0 && !text.is_empty())
            || layout.data.text_len != text.len()
            || !layout.inline_boxes().is_empty()
            || !layout.full_width().is_finite()
            || layout.full_width() < 0.0
        {
            return None;
        }
        Some(Self {
            byte_index,
            max_consecutive_lines,
            text,
            condition: DiscretionaryBreakCondition::Normal,
            layout: Arc::new(layout),
        })
    }

    /// Set when this shaped replacement may participate in wrapping.
    pub fn with_condition(mut self, condition: DiscretionaryBreakCondition) -> Self {
        self.condition = condition;
        self
    }

    /// Text used to shape this replacement.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Immutable glyph shape used for both fitting and drawing.
    pub fn layout(&self) -> &Layout<B> {
        &self.layout
    }
}

/// Text layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout<B: Brush> {
    pub(crate) data: LayoutData<B>,
}

impl<B: Brush> Layout<B> {
    /// Retain caller-resolved source opportunities for expansion and shaping.
    pub fn set_justification_opportunities(
        &mut self,
        opportunities: Vec<super::JustificationOpportunity>,
    ) {
        unjustify(&mut self.data);
        self.data.selected_line_justification_opportunities = None;
        if self.justification_opportunities() != opportunities {
            self.clear_justification_shape_selection();
            for shape in &mut self.data.deferred_justification_shapes {
                shape.prepared = None;
            }
            self.data
                .justification_opportunities
                .set(opportunities, &self.data.inline_boxes);
        }
    }

    /// Set expansion sites between visual neighbours of the selected lines.
    /// This does not change the potential source boundaries used during shaping.
    /// Rebreaking discards these selected-line sites.
    pub fn set_selected_line_justification_opportunities(
        &mut self,
        opportunities: Vec<super::JustificationOpportunity>,
    ) {
        unjustify(&mut self.data);
        let mut selected = super::justification::JustificationOpportunities::default();
        selected.set(opportunities, &self.data.inline_boxes);
        self.data.selected_line_justification_opportunities = Some(selected);
    }

    /// The caller-resolved source opportunities.
    pub fn justification_opportunities(&self) -> &[super::JustificationOpportunity] {
        self.data.justification_opportunities.entries()
    }

    /// Eligible text boundaries whose two source units are within `source`.
    pub fn justification_text_boundaries(
        &self,
        source: core::ops::Range<usize>,
    ) -> impl Iterator<Item = (&core::ops::Range<usize>, &core::ops::Range<usize>)> {
        self.data
            .justification_opportunities
            .text_boundaries(source)
    }

    pub(crate) fn clear_justification_shape_selection(&mut self) {
        unjustify(&mut self.data);
        self.data.restore_line_end_letter_spacing();
        if let Some(variants) = self.data.line_shape_variants.take() {
            self.data.clusters = variants.original;
            self.data.glyphs.truncate(variants.original_glyph_len);
        }
    }

    /// Whether naturally broken lines need alternative character-justification shapes.
    pub fn needs_justification_shape_preparation(
        &self,
        policy: crate::JustificationShapePolicy,
    ) -> bool {
        self.has_justification_shape_candidates()
            && self.data.lines.iter().any(|line| {
                super::line_break::line_needs_justification_shape(
                    &self.data,
                    line,
                    &self.data.line_items[line.item_range.clone()],
                    policy,
                )
            })
    }

    /// Whether source-bound alternative feature settings are retained for a
    /// shaped cluster containing more than one source character.
    pub fn has_justification_shape_candidates(&self) -> bool {
        !self.data.deferred_justification_shapes.is_empty()
    }

    /// Set physical fitting advances for shaped source clusters. Rendering
    /// advances and Unicode break opportunities are unchanged. Replacing the
    /// list restores original fit advances before applying new entries; an
    /// empty list removes the projection.
    pub fn set_source_cluster_fit_advances(&mut self, mut advances: Vec<SourceClusterFitAdvance>) {
        let policy = self
            .data
            .line_shape_variants
            .as_ref()
            .map(|variants| variants.policy);
        self.clear_justification_shape_selection();
        if self.data.source_cluster_fit_baseline.len() == self.data.clusters.len() {
            for (cluster, original) in self
                .data
                .clusters
                .iter_mut()
                .zip(&self.data.source_cluster_fit_baseline)
            {
                cluster.line_break_advance = *original;
            }
        }
        self.data.source_cluster_fit_baseline.clear();
        if !advances.is_empty() {
            self.data.source_cluster_fit_baseline = self
                .data
                .clusters
                .iter()
                .map(|cluster| cluster.line_break_advance)
                .collect();
        }
        advances.sort_by_key(|entry| entry.byte_index);
        advances.dedup_by_key(|entry| entry.byte_index);
        apply_source_fit_projection(&self.data.runs, &mut self.data.clusters, &advances);
        self.data.source_cluster_fit_advances = advances;
        if let Some(policy) = policy {
            self.set_justification_shape_policy(policy);
        }
    }

    /// Set physical fitting advances for material inserted by selected
    /// discretionary breaks. The material's rendered advance is unchanged.
    pub fn set_discretionary_fit_advances(&mut self, mut advances: Vec<DiscretionaryFitAdvance>) {
        advances.sort_by_key(|entry| entry.byte_index);
        advances.dedup_by_key(|entry| entry.byte_index);
        advances.retain(|entry| entry.byte_index <= self.data.text_len);
        self.data.discretionary_fit_advances = advances;
    }

    pub(crate) fn discretionary_fit_advance(&self, byte_index: usize, rendered: f32) -> f32 {
        self.data
            .discretionary_fit_advances
            .binary_search_by_key(&byte_index, |entry| entry.byte_index)
            .map_or(rendered, |index| {
                self.data.discretionary_fit_advances[index].advance
            })
    }

    /// Set position-dependent fitting advances for line-start shaping.
    /// These constraints do not change the shaped glyphs or add break opportunities.
    pub fn set_line_start_fit_advances(&mut self, mut advances: Vec<super::LineStartFitAdvance>) {
        advances.sort_by_key(|entry| entry.byte_index);
        advances.dedup_by_key(|entry| entry.byte_index);
        advances.retain(|entry| entry.byte_index < self.data.text_len);
        self.data.line_start_fit_advances = advances;
    }

    pub(crate) fn line_start_fit_advance(&self, byte_index: usize) -> Option<f32> {
        self.data
            .line_start_fit_advances
            .binary_search_by_key(&byte_index, |entry| entry.byte_index)
            .ok()
            .map(|index| self.data.line_start_fit_advances[index].inside_line)
    }

    /// Creates an empty layout.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the scale factor provided when creating the layout.
    pub fn scale(&self) -> f32 {
        self.data.scale
    }

    /// Reclaim collapsible trailing whitespace when doing so lets the
    /// following inline box fit on the current line, instead of wrapping
    /// the box to the next line.
    ///
    /// Browsers wrap the box (the trailing space is then removed as part
    /// of the wrap); PDF formatters in PDFreactor's tradition remove the
    /// space FIRST and keep the box when it then fits. Off by default;
    /// set before calling [`break_all_lines`](Self::break_all_lines).
    pub fn set_reclaim_space_before_inline_box(&mut self, reclaim: bool) {
        self.data.reclaim_space_before_inline_box = reclaim;
    }

    /// Select how normal soft-wrap opportunities compete when a complete word
    /// fits but its following collapsible space does not.
    ///
    /// This does not affect content overflow, emergency opportunities,
    /// discretionary material, `line-break: anywhere`, or
    /// `word-break: break-all`.
    pub fn set_normal_soft_wrap_selection(&mut self, selection: NormalSoftWrapSelection) {
        self.data.normal_soft_wrap_selection = selection;
    }

    /// Whether the letter-spacing after the last character of each line is
    /// removed (CSS Text 4 §8.2). Pass `false` for a paragraph that is only
    /// a fragment of a line, such as a ruby base. On by default; set before
    /// calling [`break_all_lines`](Self::break_all_lines).
    pub fn set_line_end_letter_spacing_trim(&mut self, trim: bool) {
        self.data.trim_line_end_letter_spacing = trim;
    }

    /// Override soft line-break decisions at selected UTF-8 byte boundaries.
    ///
    /// Overrides are applied after Unicode boundary analysis and before greedy
    /// line breaking. They cannot suppress mandatory newline breaks or split a
    /// shaped ligature. Duplicate byte indices use the last supplied decision.
    pub fn set_line_break_overrides(&mut self, mut overrides: Vec<LineBreakOverride>) {
        overrides.sort_by_key(|entry| entry.byte_index());
        let mut canonical: Vec<LineBreakOverride> = Vec::with_capacity(overrides.len());
        for entry in overrides {
            if entry.byte_index() > self.data.text_len {
                continue;
            }
            if let Some(previous) = canonical.last_mut() {
                if previous.byte_index() == entry.byte_index() {
                    *previous = entry;
                    continue;
                }
            }
            canonical.push(entry);
        }
        self.data.line_break_overrides = canonical;
    }

    /// Set the material widths associated with discretionary break
    /// opportunities such as soft hyphens.
    ///
    /// A discretionary advance is charged only when its boundary is selected;
    /// it is invisible and occupies no space when the text remains unbroken.
    /// Duplicate byte indices use the last supplied value.
    pub fn set_discretionary_breaks(&mut self, mut breaks: Vec<DiscretionaryBreak>) {
        breaks.sort_by_key(|entry| entry.byte_index);
        let mut canonical: Vec<DiscretionaryBreak> = Vec::with_capacity(breaks.len());
        for entry in breaks {
            if entry.byte_index > self.data.text_len
                || !entry.advance.is_finite()
                || entry.advance < 0.0
            {
                continue;
            }
            if let Some(previous) = canonical.last_mut() {
                if previous.byte_index == entry.byte_index {
                    *previous = entry;
                    continue;
                }
            }
            canonical.push(entry);
        }
        self.data.discretionary_breaks = canonical;
        self.data.discretionary_break_shapes.clear();
    }

    /// Set shaped replacements for discretionary boundaries.
    /// Their widths and metrics contribute only when the boundary is selected.
    pub fn set_discretionary_break_shapes(&mut self, mut shapes: Vec<DiscretionaryBreakShape<B>>) {
        shapes.sort_by_key(|shape| shape.byte_index);
        shapes.dedup_by_key(|shape| shape.byte_index);
        shapes.retain(|shape| shape.byte_index <= self.data.text_len);
        self.set_discretionary_breaks(
            shapes
                .iter()
                .map(|shape| DiscretionaryBreak {
                    byte_index: shape.byte_index,
                    advance: shape.layout().full_width(),
                    max_consecutive_lines: shape.max_consecutive_lines,
                    condition: shape.condition,
                })
                .collect(),
        );
        self.data.discretionary_break_shapes = shapes;
    }

    /// Shaped replacement retained at a discretionary boundary.
    pub fn discretionary_break_shape(
        &self,
        byte_index: usize,
    ) -> Option<&DiscretionaryBreakShape<B>> {
        self.data
            .discretionary_break_shapes
            .binary_search_by_key(&byte_index, |shape| shape.byte_index)
            .ok()
            .map(|index| &self.data.discretionary_break_shapes[index])
    }

    /// Set shaped material shown on the following line when its boundary is selected.
    pub fn set_following_break_shapes(&mut self, mut shapes: Vec<FollowingBreakShape<B>>) {
        shapes.sort_by_key(|shape| shape.byte_index);
        shapes.dedup_by_key(|shape| shape.byte_index);
        shapes.retain(|shape| shape.byte_index < self.data.text_len);
        self.data.following_break_shapes = shapes;
    }

    /// Shaped material at the start of a line beginning at this source boundary.
    pub fn following_break_shape(&self, byte_index: usize) -> Option<&FollowingBreakShape<B>> {
        self.data
            .following_break_shapes
            .binary_search_by_key(&byte_index, |shape| shape.byte_index)
            .ok()
            .map(|index| &self.data.following_break_shapes[index])
    }

    /// Omit source ranges only when a selected line begins at their start.
    /// The source remains present in unbroken text and in line text ranges.
    pub fn set_conditional_line_start_omissions(
        &mut self,
        mut ranges: Vec<core::ops::Range<usize>>,
    ) {
        ranges.sort_by_key(|range| range.start);
        ranges.dedup_by_key(|range| range.start);
        ranges.retain(|range| range.start < range.end && range.end <= self.data.text_len);
        self.data.conditional_line_start_omissions = ranges;
    }

    pub(crate) fn conditional_line_start_omission(
        &self,
        byte_index: usize,
    ) -> Option<&core::ops::Range<usize>> {
        self.data
            .conditional_line_start_omissions
            .binary_search_by_key(&byte_index, |range| range.start)
            .ok()
            .map(|index| &self.data.conditional_line_start_omissions[index])
    }

    /// Select whether subsequent line breaking restores overflow-only opportunities.
    pub fn set_line_break_purpose(&mut self, purpose: super::LineBreakPurpose) {
        self.data.line_break_purpose = purpose;
    }

    /// Returns the style collection for the layout.
    pub fn styles(&self) -> &[Style<B>] {
        &self.data.styles
    }

    /// Returns the width of the layout.
    pub fn width(&self) -> f32 {
        self.data.width
    }

    /// Returns the width of the layout, including the width of any trailing
    /// whitespace.
    pub fn full_width(&self) -> f32 {
        self.data.full_width
    }

    /// Calculates the lower and upper bounds on the width of the layout. These
    /// are recalculated every time this method is called.
    ///
    /// This method currently may not return the correct results for
    /// mixed-direction text.
    pub fn calculate_content_widths(&self) -> ContentWidths {
        if !self.data.inline_owner_shaping.is_empty() {
            let mut measured = self.clone();
            measured.clear_justification_shape_selection();
            measured.set_line_break_purpose(super::LineBreakPurpose::IntrinsicSizing);
            measured.break_all_lines(None);
            let max = measured.width();
            measured.break_all_lines(Some(0.0));
            return ContentWidths {
                min: measured.width(),
                max,
            };
        }
        self.data.calculate_content_widths()
    }

    /// Returns the height of the layout.
    pub fn height(&self) -> f32 {
        self.data.height
    }

    /// Returns the number of lines in the layout.
    pub fn len(&self) -> usize {
        self.data.lines.len()
    }

    /// Returns `true` if the layout is empty.
    pub fn is_empty(&self) -> bool {
        self.data.lines.is_empty()
    }

    /// Returns the line at the specified index.
    ///
    /// Returns `None` if the index is out of bounds, i.e. if it's
    /// not less than [`self.len()`](Self::len).
    pub fn get(&self, index: usize) -> Option<Line<'_, B>> {
        Some(Line {
            index: index as u32,
            layout: self,
            data: self.data.lines.get(index)?,
        })
    }

    /// Returns `true` if the dominant direction of the layout is right-to-left.
    pub fn is_rtl(&self) -> bool {
        self.data.base_level & 1 != 0
    }

    pub fn inline_boxes(&self) -> &[InlineBox] {
        &self.data.inline_boxes
    }

    pub fn inline_boxes_mut(&mut self) -> &mut [InlineBox] {
        &mut self.data.inline_boxes
    }

    /// Returns the shaped font runs before or after line breaking.
    ///
    /// Before line breaking these cover the complete paragraph and expose the
    /// actual selected font, size, variation coordinates, and source range.
    pub fn runs(&self) -> impl ExactSizeIterator<Item = Run<'_, B>> + '_ + Clone {
        self.data
            .runs
            .iter()
            .enumerate()
            .map(move |(index, data)| Run::new(self, 0, index as u32, data, None))
    }

    /// Returns an iterator over the lines in the layout.
    pub fn lines(
        &self,
    ) -> impl ExactSizeIterator<Item = Line<'_, B>> + DoubleEndedIterator + '_ + Clone {
        self.data
            .lines
            .iter()
            .enumerate()
            .map(move |(index, data)| Line {
                index: index as u32,
                layout: self,
                data,
            })
    }

    /// Sets the text-indent for the layout.
    ///
    /// The indent is applied as a margin on the start edge of indented lines, reducing the
    /// available width for line breaking and offsetting content during alignment. Negative
    /// values cause the line to protrude beyond the start edge.
    ///
    /// This must be called before [`Layout::break_all_lines`] or [`Layout::break_lines`],
    /// and before [`Layout::align`].
    pub fn set_text_indent(&mut self, amount: f32, options: IndentOptions) {
        self.set_text_indent_with_start(amount, options, IndentStart::ElementStart);
    }

    /// Sets text indentation for content whose first local line has a known
    /// relationship to the element's formatting scope.
    ///
    /// This is the fragmentation-aware counterpart of [`Self::set_text_indent`].
    /// `start` affects only the first local line; subsequent lines continue to
    /// derive `each-line` behavior from their preceding break reason.
    pub fn set_text_indent_with_start(
        &mut self,
        amount: f32,
        options: IndentOptions,
        start: IndentStart,
    ) {
        self.data.indent_amount = amount;
        self.data.indent_options = options;
        self.data.indent_start = start;
    }

    /// Returns line breaker to compute lines for the layout.
    pub fn break_lines(&mut self) -> BreakLines<'_, B> {
        unjustify(&mut self.data);
        self.data.selected_line_justification_opportunities = None;
        self.data.restore_line_end_letter_spacing();
        BreakLines::new(self)
    }

    /// Breaks all lines with the specified maximum advance.
    pub fn break_all_lines(&mut self, max_advance: Option<f32>) {
        self.break_lines()
            .break_remaining(max_advance.unwrap_or(f32::MAX));
    }

    /// Apply alignment to the layout relative to the specified container width or full layout
    /// width.
    ///
    /// You must perform line breaking prior to aligning, through [`Layout::break_lines`] or
    /// [`Layout::break_all_lines`]. If `container_width` is not specified, the layout's
    /// [`Layout::width`] is used.
    pub fn align(
        &mut self,
        container_width: Option<f32>,
        alignment: Alignment,
        options: AlignmentOptions,
    ) {
        unjustify(&mut self.data);
        align(&mut self.data, container_width, alignment, options);
    }

    /// Align the layout with per-line alignment widths.
    ///
    /// Each entry in `alignment_widths` overrides the layout's single
    /// alignment width for the corresponding line index. Lines past
    /// `alignment_widths.len()` use the LAST per-line width as a fallback.
    /// Passing an empty slice falls back fully to [`Layout::width`].
    ///
    /// Justified text (CSS `text-align: justify`) computes free space per
    /// line as `alignment_width - line.advance + trailing_whitespace`. When
    /// lines were broken at different per-line max advances (e.g. CSS 2.1
    /// §9.5 Rule 9 line-box shortening adjacent to floats), each line must
    /// be justified against ITS OWN max advance — otherwise band-narrowed
    /// lines over-stretch and overflow the band's right edge.
    ///
    /// You must perform line breaking prior to aligning, through
    /// [`Layout::break_lines`] or [`Layout::break_all_lines`].
    pub fn align_per_line(
        &mut self,
        alignment_widths: &[f32],
        alignment: Alignment,
        options: AlignmentOptions,
    ) {
        unjustify(&mut self.data);
        align_per_line(&mut self.data, alignment_widths, alignment, options);
    }

    /// Returns the index and `Line` object for the line containing the
    /// given byte `index` in the source text.
    pub(crate) fn line_for_byte_index(&self, index: usize) -> Option<(usize, Line<'_, B>)> {
        let line_index = self
            .data
            .lines
            .binary_search_by(|line| {
                if index < line.text_range.start {
                    Ordering::Greater
                } else if index >= line.text_range.end {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .ok()?;
        Some((line_index, self.get(line_index)?))
    }

    /// Returns the index and `Line` object for the line containing the
    /// given `offset`.
    ///
    /// The offset is specified in the direction orthogonal to line direction.
    /// For horizontal text, this is a vertical or y offset. If the offset is
    /// on a line boundary, it is considered to be contained by the later line.
    pub(crate) fn line_for_offset(&self, offset: f32) -> Option<(usize, Line<'_, B>)> {
        if offset < 0.0 {
            return Some((0, self.get(0)?));
        }
        let maybe_line_index = self.data.lines.binary_search_by(|line| {
            if offset < line.metrics.min_coord {
                Ordering::Greater
            } else if offset >= line.metrics.max_coord {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        });
        let line_index = match maybe_line_index {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        Some((line_index, self.get(line_index)?))
    }
}

impl<B: Brush> Default for Layout<B> {
    fn default() -> Self {
        Self {
            data: LayoutData::default(),
        }
    }
}

/// Apply caller-owned physical fit values to source-identical cluster variants.
pub(crate) fn apply_source_fit_projection(
    runs: &[super::data::RunData],
    clusters: &mut [super::data::ClusterData],
    advances: &[SourceClusterFitAdvance],
) {
    if advances.is_empty() {
        return;
    }
    for run in runs {
        for cluster in &mut clusters[run.cluster_range.clone()] {
            let byte_index = cluster.text_range(run).start;
            if let Ok(index) = advances.binary_search_by_key(&byte_index, |entry| entry.byte_index)
            {
                cluster.line_break_advance = advances[index].advance;
            }
        }
    }
}
