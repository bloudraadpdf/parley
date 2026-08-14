// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Greedy line breaking.

use alloc::vec::Vec;

#[cfg(feature = "libm")]
#[allow(unused_imports)]
use core_maths::CoreFloat;

use crate::analysis::cluster::Whitespace;
use crate::analysis::{AuthoredBreakUnit, Boundary};
use crate::data::ClusterData;
use crate::inline_box::{
    InlineBoxBidiAttachment, InlineBoxLineBreakParticipation, LogicalInlineEdgeSourceProjection,
};
use crate::layout::bidi::reorder_by_level_with_attachments;
use crate::layout::data::{
    LineBreakOverrideDisposition, NormalSoftWrapSelection, ProjectedSourceBoundary,
    ProjectedSourceClusterParticipation, SelectedSourceClusterAdvance,
};
use crate::layout::{
    BreakReason, Layout, LayoutData, LayoutItem, LayoutItemKind, LineData, LineItemData,
    LineMetrics, Run,
};
use crate::style::Brush;
use crate::style::SoftBreakPolicy;
use crate::{InlineBoxBreakAffinity, OverflowWrap, TextWrapMode, WordBreak};

use core::ops::Range;

#[derive(Default)]
struct LineLayout {
    lines: Vec<LineData>,
    line_items: Vec<LineItemData>,
}

impl LineLayout {
    fn swap<B: Brush>(&mut self, layout: &mut LayoutData<B>) {
        core::mem::swap(&mut self.lines, &mut layout.lines);
        core::mem::swap(&mut self.line_items, &mut layout.line_items);
    }
}

#[derive(Clone, Default)]
struct LineState {
    x: f32,
    fit_x: f32,
    items: Range<usize>,
    clusters: Range<usize>,
    num_spaces: usize,
    /// Of the line currently being built, the maximum line height seen so far.
    /// This represents a lower-bound on the eventual line height of the line.
    running_line_height: f32,

    /// We lag the text-wrap-mode by one cluster due to line-breaking boundaries only
    /// being triggered on the cluster after the linebreak.
    text_wrap_mode: TextWrapMode,
    /// We lag the resolved soft-break policy for the same boundary reason.
    soft_break_policy: SoftBreakPolicy,
    selected_source_cluster_advance: SelectedSourceClusterAdvance,
    /// Material charged only when the selected line ending is
    /// discretionary. It is absent from the unbroken flow.
    discretionary_advance: f32,
    discretionary_break: bool,
}

#[derive(Clone, Default)]
struct BoundarySnapshot {
    item_idx: usize,
    run_idx: usize,
    cluster_idx: usize,
    state: LineState,
}

/// A registered, non-negative advance that exists only when its conditional boundary is
/// selected. Private construction prevents ordinary authored punctuation from
/// being represented as inserted discretionary material.
#[derive(Clone, Copy)]
struct DiscretionaryAdvance(f32);

#[derive(Clone)]
enum RegularBreakCandidate {
    AuthoredDashPunctuation(BoundarySnapshot),
    InlineBoxEdge(BoundarySnapshot),
    ProjectedSource {
        snapshot: BoundarySnapshot,
        boundary: ProjectedSourceBoundary,
    },
    Ordinary(BoundarySnapshot),
    ConditionalMaterial(BoundarySnapshot),
    Unprioritized(BoundarySnapshot),
}

impl RegularBreakCandidate {
    fn into_snapshot(self) -> (BoundarySnapshot, Option<ProjectedSourceBoundary>) {
        match self {
            Self::AuthoredDashPunctuation(snapshot)
            | Self::InlineBoxEdge(snapshot)
            | Self::Ordinary(snapshot)
            | Self::ConditionalMaterial(snapshot)
            | Self::Unprioritized(snapshot) => (snapshot, None),
            Self::ProjectedSource { snapshot, boundary } => (snapshot, Some(boundary)),
        }
    }
}

#[derive(Clone, Copy)]
enum RegularBreakKind {
    AuthoredDashPunctuation,
    InlineBoxEdge,
    ProjectedSource(ProjectedSourceBoundary),
    Ordinary,
    ConditionalMaterial(DiscretionaryAdvance),
    Unprioritized,
}

#[derive(Clone, Copy)]
enum LineFit {
    Fits,
    TrailingCollapsibleSpaceOverflow(SoftWrapOpportunity),
    ContentOverflow,
}

#[derive(Clone, Copy)]
struct SoftWrapOpportunity;

#[derive(Clone, Copy)]
enum OverflowingWhitespace {
    CollapsibleSoftWrap(SoftWrapOpportunity),
    NoBreakGlue,
    Other,
}

impl OverflowingWhitespace {
    const fn classify(whitespace: Whitespace, text_wrap_mode: TextWrapMode) -> Self {
        match (whitespace, text_wrap_mode) {
            (Whitespace::Space, TextWrapMode::Wrap) => {
                Self::CollapsibleSoftWrap(SoftWrapOpportunity)
            }
            (Whitespace::NoBreakSpace, _) => Self::NoBreakGlue,
            _ => Self::Other,
        }
    }
}

#[derive(Clone)]
struct EmergencyBreakOpportunity(BoundarySnapshot);

#[derive(Clone, Default)]
struct BreakerState {
    /// The number of items that have been processed (used to revert state)
    items: usize,
    /// The number of lines that have been processed (used to revert state)
    lines: usize,

    /// Iteration state: the current item (within the layout)
    item_idx: usize,
    /// Iteration state: the current run (within the layout)
    run_idx: usize,
    /// Iteration state: the current cluster (within the layout)
    cluster_idx: usize,

    /// The y coordinate of the bottom of the last committed line (or else 0)
    /// Use of f64 here is important. f32 causes test failures due to accumulated error
    committed_y: f64,

    line: LineState,
    prev_boundary: Option<RegularBreakCandidate>,
    emergency_boundary: Option<EmergencyBreakOpportunity>,
    projected_source_boundary: Option<ProjectedSourceBoundary>,
    taken_projected_source_boundary: Option<ProjectedSourceBoundary>,
    last_appended_authored_unit: AuthoredBreakUnit,
    /// Consecutive committed lines ending at discretionary boundaries.
    consecutive_discretionary_lines: u32,
}

impl BreakerState {
    fn mark_projected_source_boundary(&mut self, projection: Option<ProjectedSourceBoundary>) {
        let projection = projection.expect("the projected source boundary must remain typed");
        self.mark_line_break_opportunity(RegularBreakKind::ProjectedSource(projection));
        self.projected_source_boundary = Some(projection);
    }

    fn resume_after_regular_break(
        &mut self,
        item_idx: usize,
        run_idx: usize,
        cluster_idx: usize,
        projected_source_boundary: Option<ProjectedSourceBoundary>,
    ) {
        self.item_idx = item_idx;
        self.run_idx = run_idx;
        self.cluster_idx = cluster_idx;
        self.taken_projected_source_boundary = projected_source_boundary;
    }

    /// Add the cluster(s) currently being evaluated to the current line
    fn append_cluster_to_line(
        &mut self,
        next_x: f32,
        next_fit_x: f32,
        clusters_height: f32,
        authored_break_unit: AuthoredBreakUnit,
    ) {
        self.line.items.end = self.item_idx + 1;
        self.line.clusters.end = self.cluster_idx + 1;
        self.line.x = next_x;
        self.line.fit_x = next_fit_x;
        self.add_line_height(clusters_height);
        self.last_appended_authored_unit = authored_break_unit;
        // Would like to add:
        // self.cluster_idx += 1;
    }

    /// Add inline box to line
    fn append_inline_box_to_line(&mut self, next_x: f32, next_fit_x: f32, box_height: f32) {
        // self.item_idx += 1;
        self.line.items.end += 1;
        self.line.x = next_x;
        self.line.fit_x = next_fit_x;
        self.add_line_height(box_height);
        self.last_appended_authored_unit = AuthoredBreakUnit::Other;
        // Would like to add:
        // self.item_idx += 1;
    }

    /// Store the current iteration state so that we can revert to it if we later want to take
    /// the line breaking opportunity at this point.
    fn mark_line_break_opportunity(&mut self, kind: RegularBreakKind) {
        let mut state = self.line.clone();
        if let RegularBreakKind::ConditionalMaterial(DiscretionaryAdvance(advance)) = kind {
            state.x += advance;
            state.fit_x += advance;
            state.discretionary_advance = advance;
            state.discretionary_break = true;
        }
        let snapshot = BoundarySnapshot {
            item_idx: self.item_idx,
            run_idx: self.run_idx,
            cluster_idx: self.cluster_idx,
            state,
        };
        self.prev_boundary = Some(match kind {
            RegularBreakKind::AuthoredDashPunctuation => {
                RegularBreakCandidate::AuthoredDashPunctuation(snapshot)
            }
            RegularBreakKind::InlineBoxEdge => RegularBreakCandidate::InlineBoxEdge(snapshot),
            RegularBreakKind::ProjectedSource(boundary) => {
                RegularBreakCandidate::ProjectedSource { snapshot, boundary }
            }
            RegularBreakKind::Ordinary => RegularBreakCandidate::Ordinary(snapshot),
            RegularBreakKind::ConditionalMaterial(_) => {
                RegularBreakCandidate::ConditionalMaterial(snapshot)
            }
            RegularBreakKind::Unprioritized => RegularBreakCandidate::Unprioritized(snapshot),
        });
    }

    fn mark_inline_box_break_after(&mut self, affinity: InlineBoxBreakAffinity) {
        match affinity {
            InlineBoxBreakAffinity::Independent => {
                self.mark_line_break_opportunity(RegularBreakKind::Ordinary);
            }
            InlineBoxBreakAffinity::ToPrevious => {
                self.mark_line_break_opportunity(RegularBreakKind::InlineBoxEdge);
            }
            InlineBoxBreakAffinity::ToNext | InlineBoxBreakAffinity::Both => {}
        }
    }

    /// Store the current iteration state so that we can revert to it if we later want to take
    /// an *emergency* line breaking opportunity at this point.
    fn mark_emergency_break_opportunity(&mut self) {
        self.emergency_boundary = Some(EmergencyBreakOpportunity(BoundarySnapshot {
            item_idx: self.item_idx,
            run_idx: self.run_idx,
            cluster_idx: self.cluster_idx,
            state: self.line.clone(),
        }));
    }

    #[inline(always)]
    fn add_line_height(&mut self, height: f32) {
        self.line.running_line_height = self.line.running_line_height.max(height);
    }
}

/// Whether `candidate` is within the forward-error bound of a positive f32
/// sum whose exact result is `max_advance`.
///
/// For `n` additions, Higham's standard bound is `gamma_n = n*u/(1-n*u)`,
/// where `u` is the unit roundoff. Line advances are non-negative, so their
/// absolute sum is the candidate itself. One further `u * max_advance` term
/// accounts for storing the independently resolved measure in `f32`.
#[inline]
fn line_advance_fits(candidate: f32, max_advance: f32, term_count: usize) -> bool {
    if candidate <= max_advance {
        return true;
    }
    if !candidate.is_finite() || !max_advance.is_finite() || max_advance < 0.0 {
        return false;
    }

    let unit_roundoff = f32::EPSILON * 0.5;
    let accumulated_roundoff = term_count.max(1) as f32 * unit_roundoff;
    if accumulated_roundoff >= 1.0 {
        return false;
    }
    let gamma = accumulated_roundoff / (1.0 - accumulated_roundoff);
    let error_bound = gamma * candidate.abs() + unit_roundoff * max_advance.abs();
    candidate - max_advance <= error_bound
}

/// Line breaking support for a paragraph.
pub struct BreakLines<'a, B: Brush> {
    layout: &'a mut Layout<B>,
    lines: LineLayout,
    state: BreakerState,
    prev_state: Option<BreakerState>,
    done: bool,
}

impl<'a, B: Brush> BreakLines<'a, B> {
    pub(crate) fn new(layout: &'a mut Layout<B>) -> Self {
        layout.data.width = 0.;
        layout.data.height = 0.;
        let mut lines = LineLayout::default();
        lines.swap(&mut layout.data);
        lines.lines.clear();
        lines.line_items.clear();
        Self {
            layout,
            lines,
            state: BreakerState::default(),
            prev_state: None,
            done: false,
        }
    }

    /// Reset state when a line has been committed
    fn start_new_line(&mut self) -> Option<(f32, f32)> {
        let line_height = self.state.line.running_line_height;
        let ended_at_discretionary = self.state.line.discretionary_break;

        self.state.items = self.lines.line_items.len();
        self.state.lines = self.lines.lines.len();
        self.state.line.x = 0.;
        self.state.line.fit_x = 0.;
        self.state.line.running_line_height = 0.;
        self.state.line.discretionary_advance = 0.;
        self.state.line.discretionary_break = false;
        self.state.prev_boundary = None; // Added by Nico
        self.state.emergency_boundary = None;
        self.state.last_appended_authored_unit = AuthoredBreakUnit::Other;
        self.state.line.selected_source_cluster_advance = self
            .state
            .taken_projected_source_boundary
            .map_or_else(SelectedSourceClusterAdvance::default, |boundary| {
                boundary.selected_source_cluster_advance()
            });

        self.state.consecutive_discretionary_lines = if ended_at_discretionary {
            self.state.consecutive_discretionary_lines.saturating_add(1)
        } else {
            0
        };

        self.finish_line(self.lines.lines.len() - 1, line_height);
        self.last_line_data()
    }

    fn last_line_data(&self) -> Option<(f32, f32)> {
        let line = self.lines.lines.last().unwrap();
        Some((line.metrics.advance, line.size()))
    }

    /// PDFreactor-parity space reclaim (gated by
    /// [`Layout::set_reclaim_space_before_inline_box`]): when the current
    /// line ends in collapsible whitespace whose removal lets `box_width`
    /// fit within `max_advance`, zero those clusters' advances (exactly
    /// what a wrap would do to a trailing space), shrink the line, and
    /// return the reclaimed advance. Returns `None` when the flag is off,
    /// the line has no trailing whitespace, or the box still would not
    /// fit.
    fn reclaim_trailing_space_for_box(&mut self, box_width: f32, max_advance: f32) -> Option<f32> {
        if !self.layout.data.reclaim_space_before_inline_box {
            return None;
        }
        let cluster_start = self.state.line.clusters.start;
        let end = self
            .state
            .line
            .clusters
            .end
            .min(self.layout.data.clusters.len());
        let mut idx = end;
        let mut reclaimed = 0.0f32;
        let mut reclaimed_fit = 0.0f32;
        let mut spaces = 0_usize;
        while idx > cluster_start {
            let cluster = &self.layout.data.clusters[idx - 1];
            if cluster.info.whitespace().is_space_or_nbsp() {
                reclaimed += cluster.advance;
                reclaimed_fit += cluster.line_break_advance;
                spaces += 1;
                idx -= 1;
            } else {
                break;
            }
        }
        if reclaimed <= 0.0
            || !self.advance_fits(
                self.state.line.fit_x - reclaimed_fit + box_width,
                max_advance,
            )
        {
            return None;
        }
        for cluster in &mut self.layout.data.clusters[idx..end] {
            cluster.advance = 0.0;
            cluster.line_break_advance = 0.0;
        }
        self.state.line.x -= reclaimed;
        self.state.line.fit_x -= reclaimed_fit;
        self.state.line.num_spaces = self.state.line.num_spaces.saturating_sub(spaces);
        Some(reclaimed)
    }

    /// Returns the y-coordinate of the top of the current line
    pub fn committed_y(&self) -> f64 {
        self.state.committed_y
    }

    /// Returns true if all the text has been placed into lines.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Computes the next line in the paragraph. Returns the advance and size
    /// (width and height for horizontal layouts) of the line.
    pub fn break_next(&mut self, max_advance: f32) -> Option<(f32, f32)> {
        // Maintain iterator state
        if self.done {
            return None;
        }
        self.prev_state = Some(self.state.clone());

        // HACK: ignore max_advance for empty layouts
        // Prevents crash when width is too small (https://github.com/linebender/parley/issues/186)
        let max_advance =
            if self.layout.data.text_len == 0 && self.layout.data.inline_boxes.is_empty() {
                f32::MAX
            } else {
                max_advance
            };

        let line_indent = self.resolve_indent();

        let max_advance = max_advance - line_indent;

        // This macro simply calls the `commit_line` with the provided arguments and some parts of self.
        // It exists solely to cut down on the boilerplate for accessing the self variables while
        // keeping the borrow checker happy
        macro_rules! try_commit_line {
            ($break_reason:expr) => {
                try_commit_line(
                    self.layout,
                    &mut self.lines,
                    &mut self.state.line,
                    max_advance,
                    $break_reason,
                    line_indent,
                )
            };
        }

        // dbg!(&self.layout.items);

        // println!("\nBREAK NEXT");
        // dbg!(&self.state.line.items);

        // Iterate over remaining runs in the Layout
        let item_count = self.layout.data.items.len();
        while self.state.item_idx < item_count {
            let item = &self.layout.data.items[self.state.item_idx];

            // println!(
            //     "\nitem = {} {:?}. x: {}",
            //     self.state.item_idx, item.kind, self.state.line.x
            // );
            // dbg!(&self.state.line.items);

            match item.kind {
                LayoutItemKind::InlineBox => {
                    let inline_box = &self.layout.data.inline_boxes[item.index];
                    let break_affinity = match inline_box.line_break_participation() {
                        InlineBoxLineBreakParticipation::Atomic(break_affinity) => break_affinity,
                        InlineBoxLineBreakParticipation::LogicalOwnerEdge(edge) => {
                            let source_projection = edge.source_projection(inline_box.width());
                            let boundary = self.layout.data.source_soft_wrap_boundary_after(
                                self.state.item_idx,
                                inline_box.index,
                                edge,
                            );
                            let projection = boundary
                                .projection_from(inline_box.index, edge.following_source_space());
                            let project = projection.is_some()
                                && self.state.line.fit_x != 0.0
                                && source_projection != LogicalInlineEdgeSourceProjection::Absent
                                && (source_projection
                                    == LogicalInlineEdgeSourceProjection::AfterGeometry
                                    || self
                                        .state
                                        .projected_source_boundary
                                        .map(ProjectedSourceBoundary::target)
                                        != projection.map(ProjectedSourceBoundary::target))
                                && boundary.is_available_from(self.state.line.text_wrap_mode);
                            if source_projection
                                == LogicalInlineEdgeSourceProjection::BeforeGeometry
                                && project
                            {
                                self.state.mark_projected_source_boundary(projection);
                            }
                            self.state.item_idx += 1;
                            self.state.append_inline_box_to_line(
                                self.state.line.x + inline_box.width(),
                                self.state.line.fit_x + inline_box.width(),
                                inline_box.height(),
                            );
                            if source_projection == LogicalInlineEdgeSourceProjection::AfterGeometry
                                && project
                            {
                                self.state.mark_projected_source_boundary(projection);
                            }
                            continue;
                        }
                        InlineBoxLineBreakParticipation::TransparentAnchor => {
                            self.state.item_idx += 1;
                            self.state.append_inline_box_to_line(
                                self.state.line.x,
                                self.state.line.fit_x,
                                0.0,
                            );
                            continue;
                        }
                    };
                    let width = inline_box.width();
                    let height = inline_box.height();

                    // Compute the x position of the content being currently processed
                    let next_x = self.state.line.x + width;
                    let next_fit_x = self.state.line.fit_x + width;

                    // println!("BOX next_x: {}", next_x);

                    // If the box fits on the current line (or we are at the start of the current line)
                    // then simply move on to the next item
                    if self.advance_fits(next_fit_x, max_advance)
                        || self.state.line.text_wrap_mode != TextWrapMode::Wrap
                    {
                        // println!("BOX FITS");

                        self.state.item_idx += 1;

                        self.state
                            .append_inline_box_to_line(next_x, next_fit_x, height);

                        // We can always line break after a REPLACED inline
                        // box; a glued box (inline border/padding shim)
                        // binds to the adjacent text and offers no
                        // opportunity (CSS forbids a break between an
                        // inline's padding and its adjacent glyph).
                        self.state.mark_inline_box_break_after(break_affinity);
                    } else {
                        // If we're at the start of the line, this box will
                        // never fit, so consume it and accept the overflow.
                        // Do not commit the line yet: following collapsible
                        // whitespace belongs to this line and must be allowed
                        // to hang before the next break opportunity is used.
                        if self.state.line.fit_x == 0.0 {
                            self.state.item_idx += 1;
                            self.state
                                .append_inline_box_to_line(next_x, next_fit_x, height);
                            self.state.mark_inline_box_break_after(break_affinity);
                        } else if !break_affinity.allows_break_before() {
                            // A glued box (inline border/padding shim) binds
                            // to the adjacent text: no break exists before
                            // it, so it overflows with its run exactly like
                            // the tail of an unbreakable word.
                            let (next_x, box_height) = (next_x, height);
                            self.state.item_idx += 1;
                            self.state
                                .append_inline_box_to_line(next_x, next_fit_x, box_height);
                            self.state.mark_inline_box_break_after(break_affinity);
                        } else if let Some(reclaimed_x) = {
                            let (box_width, box_height) = (width, height);
                            self.reclaim_trailing_space_for_box(box_width, max_advance)
                                .map(|_| {
                                    (
                                        self.state.line.x + box_width,
                                        self.state.line.fit_x + box_width,
                                        box_height,
                                    )
                                })
                        } {
                            // PDFreactor's model: remove the collapsible
                            // trailing whitespace (its advance is zeroed —
                            // exactly what a wrap would do to it) and keep
                            // the box on the line now that it fits.
                            // `pdfreactor/W8BEN` row 10: `...article:
                            // <input 46.3%>` fits only without the space;
                            // the golden keeps the field on the text line.
                            let (next_x, next_fit_x, box_height) = reclaimed_x;
                            self.state.item_idx += 1;
                            self.state
                                .append_inline_box_to_line(next_x, next_fit_x, box_height);
                            self.state.mark_inline_box_break_after(break_affinity);
                        } else {
                            // println!("BOX BREAK");
                            if try_commit_line!(BreakReason::Regular) {
                                return self.start_new_line();
                            }
                        }
                    }
                }
                LayoutItemKind::TextRun => {
                    let run_idx = item.index;
                    let run_data = &self.layout.data.runs[run_idx];

                    let run = Run::new(self.layout, 0, 0, run_data, None);
                    let cluster_start = run_data.cluster_range.start;
                    let cluster_end = run_data.cluster_range.end;

                    // println!("TextRun ({:?})", &run_data.text_range);

                    // Iterate over remaining clusters in the Run
                    while self.state.cluster_idx < cluster_end {
                        let cluster = run.get(self.state.cluster_idx - cluster_start).unwrap();

                        // Retrieve metadata about the cluster
                        let is_ligature_continuation = cluster.is_ligature_continuation();
                        let whitespace = cluster.info().whitespace();
                        let is_newline = whitespace == Whitespace::Newline;
                        let is_space = whitespace.is_space_or_nbsp();
                        let boundary = cluster.info().boundary();
                        let byte_index = cluster.text_range().start;
                        let boundary_override = self
                            .layout
                            .data
                            .line_break_overrides
                            .binary_search_by_key(&byte_index, |entry| entry.byte_index())
                            .ok()
                            .map(|index| {
                                self.layout.data.line_break_overrides[index].disposition()
                            });
                        let style = &self.layout.data.styles[cluster.data.style_index as usize];

                        // Lag text_wrap_mode style by one cluster
                        let text_wrap_mode = self.state.line.text_wrap_mode;
                        self.state.line.text_wrap_mode = style.text_wrap_mode;
                        let soft_break_policy = self.state.line.soft_break_policy;
                        self.state.line.soft_break_policy = style.soft_break_policy;

                        let has_soft_break_opportunity = match boundary_override {
                            Some(LineBreakOverrideDisposition::Suppress) => false,
                            Some(
                                LineBreakOverrideDisposition::NormalOpportunity
                                | LineBreakOverrideDisposition::UnprioritizedOpportunity
                                | LineBreakOverrideDisposition::ResolvedCollapsedSourceOpportunity
                                | LineBreakOverrideDisposition::ResolvedRetainedSourceOpportunity,
                            ) => true,
                            None => boundary == Boundary::Line,
                        };
                        let projected_source_cluster =
                            self.state.taken_projected_source_boundary.map_or(
                                ProjectedSourceClusterParticipation::Normal,
                                |projection| projection.cluster_participation(byte_index),
                            );
                        let source_boundary_was_projected = self
                            .state
                            .projected_source_boundary
                            .is_some_and(|projection| projection.suppresses(byte_index));
                        if self
                            .state
                            .projected_source_boundary
                            .is_some_and(|projection| projection.target() <= byte_index)
                        {
                            self.state.projected_source_boundary = None;
                        }
                        if self
                            .state
                            .taken_projected_source_boundary
                            .is_some_and(|projection| projection.target() <= byte_index)
                        {
                            self.state.taken_projected_source_boundary = None;
                        }

                        let resolved_source_opportunity = matches!(
                            boundary_override,
                            Some(
                                LineBreakOverrideDisposition::ResolvedCollapsedSourceOpportunity
                                    | LineBreakOverrideDisposition::ResolvedRetainedSourceOpportunity
                            )
                        );

                        if has_soft_break_opportunity
                            && !source_boundary_was_projected
                            && (resolved_source_opportunity || text_wrap_mode == TextWrapMode::Wrap)
                        {
                            // We do not currently handle breaking within a ligature, so we ignore boundaries in such a position.
                            //
                            // We also don't record boundaries when the advance is 0. As we do not want overflowing content to cause extra consecutive
                            // line breaks. We should accept the overflowing fragment in that scenario.
                            if !is_ligature_continuation && self.state.line.fit_x != 0.0 {
                                let discretionary = self
                                    .layout
                                    .data
                                    .discretionary_breaks
                                    .binary_search_by_key(&byte_index, |entry| entry.byte_index)
                                    .ok()
                                    .map(|index| self.layout.data.discretionary_breaks[index]);
                                let candidate_kind = match (
                                    soft_break_policy,
                                    boundary_override,
                                    discretionary,
                                ) {
                                    (SoftBreakPolicy::Anywhere, _, _) => {
                                        Some(RegularBreakKind::Unprioritized)
                                    }
                                    (
                                        _,
                                        Some(
                                            LineBreakOverrideDisposition::UnprioritizedOpportunity,
                                        ),
                                        _,
                                    ) => Some(RegularBreakKind::Unprioritized),
                                    (_, Some(LineBreakOverrideDisposition::Suppress), _) => None,
                                    (_, _, Some(entry)) => {
                                        let consecutive_limit_allows =
                                            entry.max_consecutive_lines.is_none_or(|limit| {
                                                self.state.consecutive_discretionary_lines < limit
                                            });
                                        (consecutive_limit_allows
                                            && self.advance_fits(
                                                self.state.line.fit_x + entry.advance,
                                                max_advance,
                                            ))
                                        .then_some(
                                            RegularBreakKind::ConditionalMaterial(
                                                DiscretionaryAdvance(entry.advance),
                                            ),
                                        )
                                    }
                                    (SoftBreakPolicy::Unicode(WordBreak::BreakAll), _, None) => {
                                        Some(RegularBreakKind::Unprioritized)
                                    }
                                    (
                                        SoftBreakPolicy::Unicode(
                                            WordBreak::Normal | WordBreak::KeepAll,
                                        ),
                                        _,
                                        None,
                                    ) => Some(match self.state.last_appended_authored_unit {
                                        AuthoredBreakUnit::DashPunctuation => {
                                            RegularBreakKind::AuthoredDashPunctuation
                                        }
                                        AuthoredBreakUnit::Other => RegularBreakKind::Ordinary,
                                    }),
                                };
                                if let Some(candidate_kind) = candidate_kind {
                                    self.state.mark_line_break_opportunity(candidate_kind);
                                }
                                // break_opportunity = true;
                            }
                        } else if is_newline {
                            self.state.append_cluster_to_line(
                                self.state.line.x,
                                self.state.line.fit_x,
                                run.metrics().line_height,
                                cluster.info().authored_break_unit(),
                            );
                            if try_commit_line!(BreakReason::Explicit) {
                                // TODO: can this be hoisted out of the conditional?
                                self.state.cluster_idx += 1;
                                return self.start_new_line();
                            }
                        } else if
                        // This text can contribute "emergency" line breaks.
                        style.overflow_wrap != OverflowWrap::Normal && !is_ligature_continuation
                        && text_wrap_mode == TextWrapMode::Wrap
                        // If we're at the start of the line, this particular cluster will never fit, so it's not a valid emergency break opportunity.
                        && self.state.line.fit_x != 0.0
                        {
                            self.state.mark_emergency_break_opportunity();
                        }

                        // If current cluster is the start of a ligature, then advance state to include
                        // the remaining clusters that make up the ligature
                        let (mut advance, mut fit_advance) = match projected_source_cluster {
                            ProjectedSourceClusterParticipation::Normal => {
                                (cluster.advance(), cluster.data.line_break_advance)
                            }
                            ProjectedSourceClusterParticipation::CollapsedSourceSpace => (0.0, 0.0),
                        };
                        if cluster.is_ligature_start() {
                            while let Some(cluster) = run.get(self.state.cluster_idx + 1) {
                                if !cluster.is_ligature_continuation() {
                                    break;
                                } else {
                                    advance += cluster.advance();
                                    fit_advance += cluster.data.line_break_advance;
                                    self.state.cluster_idx += 1;
                                }
                            }
                        }

                        // Override tab advance with position-dependent value
                        if whitespace == Whitespace::Tab {
                            let tab_interval =
                                style.tab_size.interval(run_data.metrics.space_advance);
                            if tab_interval > 0.0 {
                                advance = ((self.state.line.x / tab_interval).floor() + 1.0)
                                    * tab_interval
                                    - self.state.line.x;
                                fit_advance = ((self.state.line.fit_x / tab_interval).floor()
                                    + 1.0)
                                    * tab_interval
                                    - self.state.line.fit_x;
                            } else {
                                advance = 0.0;
                                fit_advance = 0.0;
                            }
                        }

                        // Compute the x position of the content being currently processed
                        let next_x = self.state.line.x + advance;
                        let next_fit_x = self.state.line.fit_x + fit_advance;

                        // println!("Cluster {} next_x: {}", self.state.cluster_idx, next_x);

                        let line_fit = if self.advance_fits(next_fit_x, max_advance) {
                            LineFit::Fits
                        } else {
                            match OverflowingWhitespace::classify(whitespace, style.text_wrap_mode)
                            {
                                OverflowingWhitespace::CollapsibleSoftWrap(opportunity) => {
                                    LineFit::TrailingCollapsibleSpaceOverflow(opportunity)
                                }
                                OverflowingWhitespace::NoBreakGlue
                                | OverflowingWhitespace::Other => LineFit::ContentOverflow,
                            }
                        };

                        match line_fit {
                            LineFit::Fits => {
                                let line_height = run.metrics().line_height;
                                self.state.append_cluster_to_line(
                                    next_x,
                                    next_fit_x,
                                    line_height,
                                    cluster.info().authored_break_unit(),
                                );
                                self.state.cluster_idx += 1;
                                if is_space
                                    && projected_source_cluster
                                        == ProjectedSourceClusterParticipation::Normal
                                {
                                    self.state.line.num_spaces += 1;
                                }
                            }
                            LineFit::TrailingCollapsibleSpaceOverflow(opportunity) => {
                                let SoftWrapOpportunity = opportunity;
                                // Normal priority composition may select an authored dash before
                                // this later word separator. Greedy composition keeps the complete
                                // fitting word and hangs the collapsible space.
                                match (
                                    self.layout.data.normal_soft_wrap_selection,
                                    self.state.prev_boundary.take(),
                                ) {
                                    (
                                        NormalSoftWrapSelection::PriorityClasses,
                                        Some(RegularBreakCandidate::AuthoredDashPunctuation(prev)),
                                    ) => {
                                        self.state.line = prev.state;
                                        if try_commit_line!(BreakReason::Regular) {
                                            self.state.resume_after_regular_break(
                                                prev.item_idx,
                                                prev.run_idx,
                                                prev.cluster_idx,
                                                None,
                                            );
                                            return self.start_new_line();
                                        }
                                    }
                                    (
                                        _,
                                        Some(
                                            candidate @ (RegularBreakCandidate::InlineBoxEdge(_)
                                            | RegularBreakCandidate::ProjectedSource {
                                                ..
                                            }),
                                        ),
                                    ) => {
                                        let (prev, projected_source_boundary) =
                                            candidate.into_snapshot();
                                        self.state.line = prev.state;
                                        if try_commit_line!(BreakReason::Regular) {
                                            self.state.resume_after_regular_break(
                                                prev.item_idx,
                                                prev.run_idx,
                                                prev.cluster_idx,
                                                projected_source_boundary,
                                            );
                                            return self.start_new_line();
                                        }
                                    }
                                    (_, candidate) => {
                                        self.state.prev_boundary = candidate;
                                        let line_height = run.metrics().line_height;
                                        self.state.append_cluster_to_line(
                                            next_x,
                                            next_fit_x,
                                            line_height,
                                            cluster.info().authored_break_unit(),
                                        );
                                        if try_commit_line!(BreakReason::Regular) {
                                            self.state.cluster_idx += 1;
                                            return self.start_new_line();
                                        }
                                    }
                                }
                            }
                            LineFit::ContentOverflow => {
                                // Take the most recent regular candidate regardless of its
                                // provenance. Priority only changes the overflowing-separator
                                // case above; actual content overflow remains greedy.
                                if let Some(candidate) = self.state.prev_boundary.take() {
                                    let (prev, projected_source_boundary) =
                                        candidate.into_snapshot();
                                    self.state.line = prev.state;
                                    if try_commit_line!(BreakReason::Regular) {
                                        self.state.resume_after_regular_break(
                                            prev.item_idx,
                                            prev.run_idx,
                                            prev.cluster_idx,
                                            projected_source_boundary,
                                        );
                                        return self.start_new_line();
                                    }
                                } else if let Some(prev_emergency) =
                                    self.state.emergency_boundary.take()
                                {
                                    let prev_emergency = prev_emergency.0;
                                    self.state.line = prev_emergency.state;
                                    if try_commit_line!(BreakReason::Emergency) {
                                        self.state.item_idx = prev_emergency.item_idx;
                                        self.state.run_idx = prev_emergency.run_idx;
                                        self.state.cluster_idx = prev_emergency.cluster_idx;
                                        return self.start_new_line();
                                    }
                                } else {
                                    let line_height = run.metrics().line_height;
                                    self.state.append_cluster_to_line(
                                        next_x,
                                        next_fit_x,
                                        line_height,
                                        cluster.info().authored_break_unit(),
                                    );
                                    self.state.cluster_idx += 1;
                                }
                            }
                        }
                    }
                    self.state.run_idx += 1;
                    self.state.item_idx += 1;
                }
            }
        }

        if self.state.line.items.end == 0 {
            self.state.line.items.end = 1;
        }
        if try_commit_line!(BreakReason::None) {
            self.done = true;
            return self.start_new_line();
        }

        None
    }

    /// Compare an accumulated line advance with its measure.
    ///
    /// Fixed-grid projection gives each cluster an exact decimal meaning, but
    /// the public layout representation stores those advances as `f32`.
    /// Repeated positive additions can therefore finish a handful of ulps on
    /// the wrong side of the same grid-aligned measure. Use the standard
    /// forward-error bound for a sum of positive floating-point terms only
    /// when that fixed-grid contract is active; ordinary shaping retains its
    /// strict comparison.
    fn advance_fits(&self, candidate: f32, max_advance: f32) -> bool {
        candidate <= max_advance
            || (self.layout.data.font_metric_advance_quantization.is_some()
                && line_advance_fits(
                    candidate,
                    max_advance,
                    self.state.line.clusters.len() + self.state.line.items.len() + 1,
                ))
    }

    /// Computes the next line in the paragraph by character count.
    ///
    /// This method breaks lines based on the number of characters rather than advance width.
    /// Each text cluster (including whitespace and newlines) counts as 1 character.
    /// Each inline box also counts as 1 character.
    /// Ligature components each count separately (matching character count).
    ///
    /// Unlike `break_next`, this method does not respect normal line break opportunities and
    /// will break exactly when the character limit is reached. It does not break on newlines, for example.
    ///
    /// Inline boxes are supported and each contributes as 1 character.
    pub fn break_next_with_length(&mut self, max_chars: u32) -> Option<()> {
        if self.done {
            return None;
        }

        let line_indent = self.resolve_indent();

        // Track cluster count for this line
        let mut char_count: u32 = 0;

        // This macro simply calls the `commit_line` with the provided arguments and some parts of self.
        macro_rules! try_commit_line {
            ($break_reason:expr) => {
                try_commit_line(
                    self.layout,
                    &mut self.lines,
                    &mut self.state.line,
                    f32::MAX, // No advance limit
                    $break_reason,
                    line_indent,
                )
            };
        }

        let item_count = self.layout.data.items.len();
        while self.state.item_idx < item_count {
            let item = &self.layout.data.items[self.state.item_idx];

            match item.kind {
                LayoutItemKind::InlineBox => {
                    let inline_box = &self.layout.data.inline_boxes[item.index];

                    if inline_box.is_transparent_anchor() {
                        self.state.item_idx += 1;
                        self.state.append_inline_box_to_line(
                            self.state.line.x,
                            self.state.line.fit_x,
                            0.0,
                        );
                        continue;
                    }

                    // Check if adding this box would exceed the limit
                    if char_count >= max_chars && max_chars != 0 {
                        // Break before this box
                        if try_commit_line!(BreakReason::Regular) {
                            self.start_new_line();
                            return Some(());
                        }
                    }

                    // Compute the x position for the line width tracking
                    let next_x = self.state.line.x + inline_box.width();
                    let next_fit_x = self.state.line.fit_x + inline_box.width();
                    self.state.item_idx += 1;
                    self.state
                        .append_inline_box_to_line(next_x, next_fit_x, inline_box.height());
                    char_count += u32::from(matches!(
                        inline_box.line_break_participation(),
                        InlineBoxLineBreakParticipation::Atomic(_)
                    ));

                    // Check if we've reached the limit after adding this box
                    if char_count >= max_chars {
                        // Check if we've consumed all content (this is the last line).
                        let is_last_item = self.state.item_idx >= self.layout.data.items.len();
                        let break_reason = if is_last_item {
                            BreakReason::None
                        } else {
                            BreakReason::Regular
                        };

                        if try_commit_line!(break_reason) {
                            if break_reason == BreakReason::None {
                                self.done = true;
                            }
                            self.start_new_line();
                            return Some(());
                        }
                    }
                }
                LayoutItemKind::TextRun => {
                    let run_idx = item.index;
                    let run_data = &self.layout.data.runs[run_idx];
                    let run = Run::new(self.layout, 0, 0, run_data, None);
                    let cluster_start = run_data.cluster_range.start;
                    let cluster_end = run_data.cluster_range.end;

                    while self.state.cluster_idx < cluster_end {
                        let cluster = run.get(self.state.cluster_idx - cluster_start).unwrap();

                        // Check if we should break before this cluster
                        if char_count >= max_chars
                            && max_chars != 0
                            && try_commit_line!(BreakReason::Regular)
                        {
                            self.start_new_line();
                            return Some(());
                        }

                        let whitespace = cluster.info().whitespace();
                        let is_newline = whitespace == Whitespace::Newline;
                        let is_space = whitespace.is_space_or_nbsp();
                        let advance = cluster.advance();
                        let fit_advance = cluster.data.line_break_advance;

                        // Compute the x position.
                        // Newlines don't contribute to line width (matching break_next behavior).
                        let next_x = if is_newline {
                            self.state.line.x
                        } else {
                            self.state.line.x + advance
                        };
                        let next_fit_x = if is_newline {
                            self.state.line.fit_x
                        } else {
                            self.state.line.fit_x + fit_advance
                        };
                        let line_height = run.metrics().line_height;
                        self.state.append_cluster_to_line(
                            next_x,
                            next_fit_x,
                            line_height,
                            cluster.info().authored_break_unit(),
                        );
                        self.state.cluster_idx += 1;
                        char_count += 1;

                        if is_space {
                            self.state.line.num_spaces += 1;
                        }

                        // Check if we've reached the limit after adding this cluster
                        if char_count >= max_chars {
                            // Determine the break reason:
                            // - BreakReason::None for the last line (end of content)
                            // - BreakReason::Explicit if this line ends with a newline
                            // - BreakReason::Regular for soft wraps
                            let is_last_cluster_of_run = self.state.cluster_idx >= cluster_end;
                            let is_last_item =
                                self.state.item_idx + 1 >= self.layout.data.items.len();
                            let break_reason = if is_last_cluster_of_run && is_last_item {
                                BreakReason::None
                            } else if is_newline {
                                BreakReason::Explicit
                            } else {
                                BreakReason::Regular
                            };

                            if try_commit_line!(break_reason) {
                                if break_reason == BreakReason::None {
                                    self.done = true;
                                }
                                self.start_new_line();
                                return Some(());
                            }
                        }
                    }
                    self.state.run_idx += 1;
                    self.state.item_idx += 1;
                }
            }
        }

        // Commit the final line (only reached if content remains after all break_next_with_length calls)
        if self.state.line.items.end == 0 {
            self.state.line.items.end = 1;
        }
        if try_commit_line!(BreakReason::None) {
            self.done = true;
            self.start_new_line();
            return Some(());
        }

        None
    }

    /// Reverts the last computed line, returning to the previous state.
    pub fn revert(&mut self) -> bool {
        if let Some(state) = self.prev_state.take() {
            self.state = state;
            self.lines.lines.truncate(self.state.lines);
            self.lines.line_items.truncate(self.state.items);
            self.done = false;
            true
        } else {
            false
        }
    }

    /// Breaks all remaining lines with the specified maximum advance. This
    /// consumes the line breaker.
    pub fn break_remaining(mut self, max_advance: f32) {
        // println!("\nDEBUG ITEMS");
        // for item in &self.layout.items {
        //     match item.kind {
        //         LayoutItemKind::InlineBox => println!("{:?}", item.kind),
        //         LayoutItemKind::TextRun => {
        //             let run_data = &self.layout.runs[item.index];
        //             println!("{:?} ({:?})", item.kind, &run_data.text_range);
        //         }
        //     }
        // }

        // println!("\nBREAK ALL");

        while self.break_next(max_advance).is_some() {}
        self.finish();
    }

    /// Consumes the line breaker and finalizes all line computations.
    pub fn finish(mut self) {
        if self.layout.data.text_len == 0 {
            if let Some(line) = self.lines.line_items.first_mut() {
                line.text_range = 0..0;
                line.cluster_range = 0..0;
            }
        }
    }

    #[inline]
    fn resolve_indent(&self) -> f32 {
        let should_indent = {
            let is_scope_line = if self.lines.lines.is_empty() {
                indent_start_is_scope_line(
                    self.layout.data.indent_start,
                    self.layout.data.indent_options.each_line,
                )
            } else {
                self.layout.data.indent_options.each_line
                    && self.lines.lines.last().map(|line| line.break_reason)
                        == Some(BreakReason::Explicit)
            };
            is_scope_line ^ self.layout.data.indent_options.hanging
        };

        if should_indent {
            self.layout.data.indent_amount
        } else {
            0.0
        }
    }

    fn finish_line(&mut self, line_idx: usize, line_height: f32) {
        let prev_line_metrics = match line_idx {
            0 => None,
            idx => Some(self.lines.lines[idx - 1].metrics),
        };
        let line = &mut self.lines.lines[line_idx];

        // Reset metrics for line
        line.metrics.ascent = 0.;
        line.metrics.descent = 0.;
        line.metrics.leading = 0.;
        line.metrics.offset = 0.;
        line.text_range.start = usize::MAX;

        line.metrics.line_height = line_height;

        if line.item_range.is_empty() {
            line.text_range = self.layout.data.text_len..self.layout.data.text_len;
        }

        // Forward pass: fix tab cluster advances for correct rendering.
        // Tabs are position-dependent, so we need cumulative x from line start.
        {
            let mut line_x = 0.0_f32;
            for line_item in &self.lines.line_items[line.item_range.clone()] {
                match line_item.kind {
                    LayoutItemKind::InlineBox => {
                        line_x += self.layout.data.inline_boxes[line_item.index].width();
                    }
                    LayoutItemKind::TextRun => {
                        let run = &self.layout.data.runs[line_item.index];
                        let space_advance = run.metrics.space_advance;
                        let glyph_start = run.glyph_start;
                        let tab_size = self
                            .layout
                            .data
                            .clusters
                            .get(line_item.cluster_range.start)
                            .map(|c| self.layout.data.styles[c.style_index as usize].tab_size)
                            .unwrap_or_default();
                        let tab_interval = tab_size.interval(space_advance);
                        let cluster_range = line_item.cluster_range.clone();

                        for cluster in &mut self.layout.data.clusters[cluster_range] {
                            if tab_interval > 0.0 && cluster.info.whitespace() == Whitespace::Tab {
                                let next_stop =
                                    ((line_x / tab_interval).floor() + 1.0) * tab_interval;
                                let new_advance = next_stop - line_x;
                                // Keep glyph array advance in sync for non-inline clusters.
                                if cluster.glyph_len != 0xFF {
                                    let delta = new_advance - cluster.advance;
                                    let start = glyph_start + cluster.glyph_offset as usize;
                                    let end = start + cluster.glyph_len as usize;
                                    if let Some(last) =
                                        self.layout.data.glyphs[start..end].last_mut()
                                    {
                                        last.advance += delta;
                                    }
                                }
                                cluster.advance = new_advance;
                            }
                            line_x += line
                                .selected_source_cluster_advance
                                .resolve(cluster.text_range(run).start, cluster.advance);
                        }
                    }
                }
            }
        }

        // Compute metrics for the line, but ignore trailing whitespace.
        //
        // Alongside the font ascent/descent maxima, accumulate the CSS 2.1
        // §10.8 per-contributor extents: each run extends the line box by
        // its own half-leading around the shared baseline, so the line box
        // is max(above) + max(below) — NOT max(line-height). A run pairing
        // a deep-descent font with a small line-height can push the line's
        // below-extent past the tallest run's (PDFreactor golden: 10pt
        // Cousine `code` spans inside 10pt/1.25 Arimo paragraphs make the
        // line 13.06pt, not 12.5pt). Uniform-style lines are unchanged:
        // above + below == line-height there.
        let mut max_above = 0.0f32;
        let mut max_below = 0.0f32;
        let mut have_extents = false;
        let mut have_metrics = false;
        let mut needs_reorder = false;
        for line_item in self.lines.line_items[line.item_range.clone()]
            .iter_mut()
            .rev()
        {
            match line_item.kind {
                LayoutItemKind::InlineBox => {
                    let item = &self.layout.data.inline_boxes[line_item.index];

                    if line_item.bidi_level != 0 {
                        needs_reorder = true;
                    }

                    // Advance is already computed in "commit line" for items

                    // Default vertical alignment is to align the bottom of boxes with the text baseline.
                    // This is equivalent to the entire height of the box being "ascent"
                    line.metrics.ascent = line.metrics.ascent.max(item.height());
                    max_above = max_above.max(item.height());
                    have_extents = true;

                    // Mark us as having seen non-whitespace content on this line
                    have_metrics = true;
                }
                LayoutItemKind::TextRun => {
                    line_item.compute_whitespace_properties(&self.layout.data);

                    // Compute the text range for the line
                    // Q: Can we not simplify this computation by assuming that items are in order?
                    line.text_range.end = line.text_range.end.max(line_item.text_range.end);
                    line.text_range.start = line.text_range.start.min(line_item.text_range.start);

                    // Mark line as needing bidi re-ordering if it contains any runs with non-zero bidi level
                    // (zero is the default level, so this is equivalent to marking lines that have multiple levels)
                    if line_item.bidi_level != 0 {
                        needs_reorder = true;
                    }

                    let run = &self.layout.data.runs[line_item.index];
                    // Compute the run's advance by summing the advances of its constituent clusters
                    line_item.advance = self.layout.data.clusters[line_item.cluster_range.clone()]
                        .iter()
                        .map(|cluster| {
                            line.selected_source_cluster_advance
                                .resolve(cluster.text_range(run).start, cluster.advance)
                        })
                        .sum();

                    // Ignore trailing whitespace for metrics computation
                    // (we are iterating backwards so trailing whitespace comes first)
                    if !have_metrics && line_item.is_whitespace {
                        continue;
                    }

                    // Compute the run's vertical metrics
                    line.metrics.ascent = line.metrics.ascent.max(run.metrics.ascent);
                    line.metrics.descent = line.metrics.descent.max(run.metrics.descent);
                    let half_leading = (run.metrics.line_height
                        - (run.metrics.ascent + run.metrics.descent))
                        * 0.5;
                    max_above = max_above.max(run.metrics.ascent + half_leading);
                    max_below = max_below.max(run.metrics.descent + half_leading);
                    have_extents = true;

                    // Mark us as having seen non-whitespace content on this line
                    have_metrics = true;
                }
            }
        }

        // Reorder the items within the line (if required). Reordering is required if the line contains
        // a mix of bidi levels (a mix of LTR and RTL text)
        let item_count = line.item_range.end - line.item_range.start;
        if needs_reorder && item_count > 1 {
            reorder_line_items(
                &mut self.lines.line_items[line.item_range.clone()],
                &self.layout.data.inline_boxes,
            );
        }

        // Compute size of line's trailing whitespace. "Trailing" is considered the right edge
        // for LTR text and the left edge for RTL text.
        let run = if self.layout.is_rtl() {
            self.lines.line_items[line.item_range.clone()].first()
        } else {
            self.lines.line_items[line.item_range.clone()].last()
        };
        line.metrics.trailing_whitespace = run
            .filter(|item| item.is_text_run() && item.has_trailing_whitespace)
            .map(|run| {
                fn whitespace_advance<'c, I: Iterator<Item = &'c ClusterData>>(clusters: I) -> f32 {
                    clusters
                        .take_while(|cluster| cluster.info.whitespace() != Whitespace::None)
                        .map(|cluster| cluster.advance)
                        .sum()
                }

                let clusters = &self.layout.data.clusters[run.cluster_range.clone()];
                if run.is_rtl() {
                    whitespace_advance(clusters.iter())
                } else {
                    whitespace_advance(clusters.iter().rev())
                }
            })
            .unwrap_or(0.0);

        if !have_metrics {
            // Line consisting entirely of whitespace?
            if !line.item_range.is_empty() {
                let line_item = &self.lines.line_items[line.item_range.start];
                if line_item.is_text_run() {
                    let run = &self.layout.data.runs[line_item.index];
                    line.metrics.ascent = run.metrics.ascent;
                    line.metrics.descent = run.metrics.descent;
                    let half_leading = (run.metrics.line_height
                        - (run.metrics.ascent + run.metrics.descent))
                        * 0.5;
                    max_above = max_above.max(run.metrics.ascent + half_leading);
                    max_below = max_below.max(run.metrics.descent + half_leading);
                    have_extents = true;
                }
            } else if let Some(metrics) = prev_line_metrics {
                // HACK: copy metrics from previous line if we don't have
                // any; this should only occur for an empty line following
                // a newline at the end of a layout
                line.metrics = metrics;
                // Reconstruct the copied line's above/below extents from its
                // baseline geometry (committed_y still sits at THIS line's
                // top, i.e. the previous line's bottom), so the CSS
                // per-contributor model reproduces the copied geometry
                // exactly at the new offset.
                let prev_top = self.state.committed_y as f32 - metrics.line_height;
                max_above = metrics.baseline - prev_top;
                max_below = metrics.line_height - max_above;
                have_extents = true;
                // If we have no items on this line, it must be the last (empty)
                // line in a layout following a newline. Commit an empty run so
                // that AccessKit has a node with which to identify the visual
                // cursor position
                if let Some((index, run)) = self
                    .layout
                    .data
                    .runs
                    .iter()
                    .enumerate()
                    .rfind(|(_, run)| !run.text_range.is_empty())
                {
                    let run_index = self.lines.line_items.len();
                    let cluster = run.cluster_range.end;
                    let text = run.text_range.end;
                    self.lines.line_items.push(LineItemData {
                        kind: LayoutItemKind::TextRun,
                        index,
                        bidi_level: 0,
                        advance: 0.,
                        is_whitespace: false,
                        has_trailing_whitespace: false,
                        cluster_range: cluster..cluster,
                        text_range: text..text,
                    });
                    line.item_range = run_index..run_index + 1;
                }
            }
        }

        // Whether metrics should be quantized to pixel boundaries
        let quantize = self.layout.data.quantize;

        // CSS 2.1 §10.8 per-contributor line box (unquantized/print path
        // only): the line box spans max(above) to max(below), where each
        // run contributes its font extent plus its OWN half-leading. The
        // quantized path keeps the legacy Chrome-mimicking model — its
        // pixel rounding is calibrated against Chromium fixtures and
        // uniform-style UI text does not exercise the difference.
        if !quantize && have_extents {
            line.metrics.line_height = max_above + max_below;
        }

        line.metrics.leading =
            line.metrics.line_height - (line.metrics.ascent + line.metrics.descent);

        let (ascent, descent) = if quantize {
            // We mimic Chrome in rounding ascent and descent separately,
            // before calculating the rest.
            // See lines_integral_line_height_ascent_descent_rounding() for more details.
            (line.metrics.ascent.round(), line.metrics.descent.round())
        } else {
            (line.metrics.ascent, line.metrics.descent)
        };

        let (leading_above, leading_below) = if quantize {
            // Calculate leading using the rounded ascent and descent.
            let leading = line.metrics.line_height - (ascent + descent);
            // We mimic Chrome in giving 'below' the larger leading half.
            // Although the comment in Chromium's NGLineHeightMetrics::AddLeading function
            // in ng_line_height_metrics.cc claims it's for legacy test compatibility.
            // So we might want to think about giving 'above' the larger half instead.
            let above = (leading * 0.5).floor();
            let below = leading.round() - above;
            (above, below)
        } else if have_extents {
            // Anchor the baseline at the per-contributor above-extent; the
            // leading split is whatever the extents dictate rather than an
            // even halving of the total.
            (
                max_above - line.metrics.ascent,
                max_below - line.metrics.descent,
            )
        } else {
            (line.metrics.leading * 0.5, line.metrics.leading * 0.5)
        };

        let y = self.state.committed_y;
        line.metrics.baseline =
            ascent + leading_above + if quantize { y.round() as f32 } else { y as f32 };

        // Small line heights will cause leading to be negative.
        // Negative leadings are correct for baseline calculation, but not for min/max coords.
        // We clamp leading to zero for the purposes of min/max coords,
        // which in turn clamps the selection box minimum height to ascent + descent.
        line.metrics.min_coord = line.metrics.baseline - ascent - leading_above.max(0.);
        line.metrics.max_coord = line.metrics.baseline + descent + leading_below.max(0.);

        self.state.committed_y += line.metrics.line_height as f64;
    }
}

#[inline]
const fn indent_start_is_scope_line(start: crate::IndentStart, each_line: bool) -> bool {
    match start {
        crate::IndentStart::ElementStart => true,
        crate::IndentStart::ContinuationAfterSoftWrap => false,
        crate::IndentStart::ContinuationAfterExplicitBreak => each_line,
    }
}

impl<B: Brush> Drop for BreakLines<'_, B> {
    fn drop(&mut self) {
        // Compute the overall width and height of the entire layout
        // The "width" excludes trailing whitespace. The "full_width" includes it.
        let mut width = 0_f32;
        let mut full_width = 0_f32;
        let mut height = 0_f64; // f32 causes test failures due to accumulated error
        for line in &self.lines.lines {
            let indent_extra = line.indent.max(0.0);
            width =
                width.max(line.metrics.advance + indent_extra - line.metrics.trailing_whitespace);
            full_width = full_width.max(line.metrics.advance + indent_extra);
            height += line.metrics.line_height as f64;
        }

        // Save the computed widths/height to the layout
        self.layout.data.width = width;
        self.layout.data.full_width = full_width;
        self.layout.data.height = height as f32;

        // for (i, line) in self.lines.lines.iter().enumerate() {
        //     println!("LINE {i}");
        //     for item_idx in line.item_range.clone() {
        //         let item = &self.lines.line_items[item_idx];
        //         println!("  ITEM {:?} ({})", item.kind, item.advance);
        //     }
        // }

        // Save the computed lines to the layout
        self.lines.swap(&mut self.layout.data);
    }
}

// fn cluster_range_is_valid(
//     mut cluster_range: Range<usize>,
//     state_cluster_range: Range<usize>,
//     is_first: bool,
//     is_last: bool,
//     is_empty: bool,
// ) -> bool {
//     // Compute cluster range
//     if is_first {
//         cluster_range.start = state_cluster_range.start;
//     }
//     if is_last {
//         cluster_range.end = state_cluster_range.end;
//     }

//     // Return true if cluster is valid. Else false.
//     cluster_range.start < cluster_range.end
//         || (cluster_range.start == cluster_range.end && is_empty)
// }

// fn should_commit_line<B: Brush>(
//     layout: &LayoutData<B>,
//     state: &mut LineState,
//     is_last: bool,
// ) -> bool {
//     // Compute end cluster
//     state.clusters.end = state.clusters.end.min(layout.clusters.len());
//     if state.runs.end == 0 && is_last {
//         state.runs.end = 1;
//     }

//     let last_run = state.runs.len() - 1;
//     let is_empty = layout.text_len == 0;

//     // Iterate over runs. Checking if any have a valid cluster range.
//     let runs = &layout.runs[state.runs.clone()];
//     runs.iter().enumerate().any(|(i, run_data)| {
//         cluster_range_is_valid(
//             run_data.cluster_range.clone(),
//             state.clusters.clone(),
//             i == 0,
//             i == last_run,
//             is_empty,
//         )
//     })
// }

fn try_commit_line<B: Brush>(
    layout: &Layout<B>,
    lines: &mut LineLayout,
    state: &mut LineState,
    max_advance: f32,
    break_reason: BreakReason,
    line_indent: f32,
) -> bool {
    // Ensure that the cluster and item endpoints are within range
    state.clusters.end = state.clusters.end.min(layout.data.clusters.len());
    state.items.end = state.items.end.min(layout.data.items.len());

    let start_item_idx = lines.line_items.len();
    // let start_run_idx = lines.line_items.last().map(|item| item.index).unwrap_or(0);

    let items_to_commit = &layout.data.items[state.items.clone()];

    // Compute first and last run index
    let is_text_run = |item: &LayoutItem| item.kind == LayoutItemKind::TextRun;
    let first_run_pos = items_to_commit.iter().position(is_text_run).unwrap_or(0);
    let last_run_pos = items_to_commit.iter().rposition(is_text_run).unwrap_or(0);

    // // Return if line contains no runs
    // let (Some(first_run_pos), Some(last_run_pos)) = (first_run_pos, last_run_pos) else {
    //     return false;
    // };

    //let runs = &layout.runs[state.runs.clone()];
    // let start_run_idx = items_to_commit[first_run_pos].index;
    // let end_run_idx = items_to_commit[last_run_pos].index;

    // Iterate over the items to commit
    // println!("\nCOMMIT LINE");
    let mut last_item_kind = LayoutItemKind::TextRun;
    let mut committed_text_run = false;
    for (i, item) in items_to_commit.iter().enumerate() {
        // println!("i = {} index = {} {:?}", i, item.index, item.kind);

        match item.kind {
            LayoutItemKind::InlineBox => {
                let inline_box = &layout.data.inline_boxes[item.index];

                lines.line_items.push(LineItemData {
                    kind: LayoutItemKind::InlineBox,
                    index: item.index,
                    bidi_level: item.bidi_level,
                    advance: inline_box.width(),

                    // These properties are ignored for inline boxes. So we just put a dummy value.
                    is_whitespace: false,
                    has_trailing_whitespace: false,
                    cluster_range: 0..0,
                    text_range: 0..0,
                });

                last_item_kind = item.kind;
            }
            LayoutItemKind::TextRun => {
                let run_data = &layout.data.runs[item.index];

                // Compute cluster range
                // The first and last ranges have overrides to account for line-breaks within runs
                let mut cluster_range = run_data.cluster_range.clone();
                if i == first_run_pos {
                    cluster_range.start = state.clusters.start;
                }
                if i == last_run_pos {
                    cluster_range.end = state.clusters.end;
                }

                if cluster_range.start >= run_data.cluster_range.end {
                    // println!("INVALID CLUSTER");
                    // dbg!(&run_data.text_range);
                    // dbg!(cluster_range);
                    continue;
                }

                last_item_kind = item.kind;
                committed_text_run = true;

                // Push run to line
                let run = Run::new(layout, 0, 0, run_data, None);
                let text_range = if run_data.cluster_range.is_empty() {
                    0..0
                } else {
                    let first_cluster = run
                        .get(cluster_range.start - run_data.cluster_range.start)
                        .unwrap();
                    let last_cluster = run
                        .get((cluster_range.end - run_data.cluster_range.start).saturating_sub(1))
                        .unwrap();
                    first_cluster.text_range().start..last_cluster.text_range().end
                };

                lines.line_items.push(LineItemData {
                    kind: LayoutItemKind::TextRun,
                    index: item.index,
                    bidi_level: run_data.bidi_level,
                    advance: 0.,
                    is_whitespace: false,
                    has_trailing_whitespace: false,
                    cluster_range,
                    text_range,
                });
            }
        }
    }
    // let end_run_idx = lines.line_items.last().map(|item| item.index).unwrap_or(0);
    let end_item_idx = lines.line_items.len();

    // Return false and don't commit line if there were no items to process
    // FIXME: support lines with only inlines boxes
    // if start_item_idx == end_item_idx {
    //     // } || first_run_pos == last_run_pos {
    //     return false;
    // }

    // Exclude the trailing space from justification space count.
    // Only subtract if the line actually ends with a space — with
    // WordBreak::BreakAll, regular breaks can land between non-space
    // characters, in which case there is no trailing space to exclude.
    let mut num_spaces = state.num_spaces;
    if break_reason == BreakReason::Regular
        && state.clusters.start < state.clusters.end
        && layout.data.clusters[state.clusters.end - 1]
            .info
            .whitespace()
            .is_space_or_nbsp()
    {
        num_spaces = num_spaces.saturating_sub(1);
    }

    lines.lines.push(LineData {
        item_range: start_item_idx..end_item_idx,
        max_advance,
        break_reason,
        num_spaces,
        indent: line_indent,
        discretionary_advance: state.discretionary_advance,
        selected_source_cluster_advance: state.selected_source_cluster_advance.clone(),
        ends_at_discretionary_break: state.discretionary_break,
        metrics: LineMetrics {
            advance: state.x,
            ..Default::default()
        },
        ..Default::default()
    });

    // Reset state for the new line
    state.num_spaces = 0;
    if committed_text_run {
        state.clusters.start = state.clusters.end;
    }

    state.items.start = match last_item_kind {
        // For text runs, the first item of line N+1 needs to be the SAME as
        // the last item for line N. This is because the item (if it a text run
        // may be split across the two lines with some clusters in line N and some
        // in line N+1). The item is later filtered out (see `continue` in loop above)
        // if there are not actually any clusters in line N+1.
        LayoutItemKind::TextRun => state.items.end.saturating_sub(1),
        // Inline boxes cannot be spread across multiple lines, so we should set
        // the first item of line N+1 to be the item AFTER the last item in line N.
        LayoutItemKind::InlineBox => state.items.end,
    };

    true
}

/// Reorder items within line according to the bidi levels of the items
fn reorder_line_items(runs: &mut [LineItemData], inline_boxes: &[crate::InlineBox]) {
    let mut visual_indices = (0..runs.len()).collect::<Vec<_>>();
    reorder_by_level_with_attachments(
        &mut visual_indices,
        |index| runs[index].bidi_level,
        |index| match runs[index].kind {
            LayoutItemKind::TextRun => InlineBoxBidiAttachment::Independent,
            LayoutItemKind::InlineBox => inline_boxes[runs[index].index].bidi_attachment(),
        },
    );
    let reordered = visual_indices
        .into_iter()
        .map(|index| runs[index].clone())
        .collect::<Vec<_>>();
    runs.clone_from_slice(&reordered);
}

#[cfg(test)]
mod tests {
    use super::{indent_start_is_scope_line, line_advance_fits};
    use crate::IndentStart;

    #[test]
    fn line_fit_absorbs_only_the_bound_of_float_accumulation_error() {
        // 115 positive cluster advances accumulate to 469.88995 in f32,
        // while the same fixed-grid values and measure are 469.8898 when
        // associated at their serialisation boundaries.
        assert!(line_advance_fits(469.88995, 469.8898, 115));
        assert!(
            !line_advance_fits(469.90, 469.8898, 115),
            "a genuine 0.01pt overflow remains a wrap",
        );
    }

    #[test]
    fn continuation_indent_scope_depends_on_preceding_break_and_each_line() {
        assert!(!indent_start_is_scope_line(
            IndentStart::ContinuationAfterSoftWrap,
            false,
        ));
        assert!(!indent_start_is_scope_line(
            IndentStart::ContinuationAfterSoftWrap,
            true,
        ));
        assert!(!indent_start_is_scope_line(
            IndentStart::ContinuationAfterExplicitBreak,
            false,
        ));
        assert!(indent_start_is_scope_line(
            IndentStart::ContinuationAfterExplicitBreak,
            true,
        ));
    }
}
