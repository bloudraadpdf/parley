// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{Layout, LayoutItemKind, LineBreakOverride};
use crate::{Brush, data::ClusterData};
use alloc::{sync::Arc, vec::Vec};
use core::ops::Range;

/// Invalid source or participant identity in a prepared parallel inline group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParallelInlineError {
    /// Rows must contain text with only zero-advance structural anchors, without forced breaks or tabs.
    UnsupportedRow,
    /// Internal cuts must be ordered, distinct shaped grapheme boundaries.
    InvalidCut,
    /// At least one parallel row is required.
    EmptyGroup,
    /// A group must bind to exactly one existing atomic inline-box identity.
    InvalidParticipant,
}

/// An immutable shaped row and caller-resolved legal internal source boundaries.
/// The caller resolves wrapping policy before supplying cuts. Structural
/// no-wrap on the shaped row remains unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedParallelRow<B: Brush> {
    pub(crate) layout: Arc<Layout<B>>,
    pub(crate) boundaries: Arc<[usize]>,
    pub(crate) cursors: Arc<[(usize, usize)]>,
    pub(crate) boundary_advances: Arc<[f32]>,
}

impl<B: Brush> PreparedParallelRow<B> {
    /// Retain a shaped row with INTERNAL legal byte cuts. Zero and the terminal
    /// source boundary are inserted automatically.
    pub fn new(mut layout: Layout<B>, legal_cuts: Vec<usize>) -> Result<Self, ParallelInlineError> {
        if layout.data.text_len == 0
            || layout.data.inline_boxes.iter().any(|item| {
                item.cloned_owner().is_some()
                    || item.height() != 0.0
                    || item.width_for_line(0) != 0.0
                    || item.width_for_line(1) != 0.0
                    || item.advance_for_line(0) != 0.0
                    || item.advance_for_line(1) != 0.0
                    || item.needs_letter_spacing_restore()
                    || item
                        .terminal_owner_edge_reduction(0)
                        .is_some_and(|(_, reduction)| reduction != 0.0)
                    || item
                        .terminal_owner_edge_reduction(1)
                        .is_some_and(|(_, reduction)| reduction != 0.0)
                    || !matches!(
                        item.line_break_participation(),
                        crate::inline_box::InlineBoxLineBreakParticipation::LogicalOwnerEdge(_)
                            | crate::inline_box::InlineBoxLineBreakParticipation::TransparentAnchor
                    )
            })
            || !layout.data.parallel_inline_groups.is_empty()
            || layout.data.indent_amount != 0.0
            || layout
                .data
                .styles
                .iter()
                .any(|style| style.overflow_wrap != crate::OverflowWrap::Normal)
            || layout.data.clusters.iter().any(|cluster| {
                matches!(
                    cluster.info.whitespace(),
                    crate::analysis::cluster::Whitespace::Tab
                        | crate::analysis::cluster::Whitespace::Newline
                )
            })
        {
            return Err(ParallelInlineError::UnsupportedRow);
        }
        let mut shaped_boundaries = Vec::new();
        for (item_index, item) in layout
            .data
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind == LayoutItemKind::TextRun)
        {
            let run = &layout.data.runs[item.index];
            for cluster_index in item.cluster_range.clone() {
                let cluster = &layout.data.clusters[cluster_index];
                if cluster.flags & ClusterData::GRAPHEME_START != 0
                    && !cluster.is_ligature_component()
                {
                    shaped_boundaries.push((
                        cluster.text_range(run).start,
                        item_index,
                        cluster_index,
                    ));
                }
            }
        }
        shaped_boundaries.sort_unstable_by_key(|entry| entry.0);
        shaped_boundaries.dedup_by_key(|entry| entry.0);
        let mut previous = 0;
        for &cut in &legal_cuts {
            if cut <= previous
                || cut >= layout.data.text_len
                || shaped_boundaries
                    .binary_search_by_key(&cut, |entry| entry.0)
                    .is_err()
            {
                return Err(ParallelInlineError::InvalidCut);
            }
            previous = cut;
        }
        // Resolved source opportunities can cross a structural no-wrap fence.
        // Other opportunities are suppressed so the caller's table is exact.
        layout.set_line_break_overrides(
            shaped_boundaries
                .iter()
                .map(|&(byte, _, _)| {
                    if legal_cuts.binary_search(&byte).is_ok() {
                        LineBreakOverride::resolved_retained_source_opportunity(byte)
                    } else {
                        LineBreakOverride::suppress(byte)
                    }
                })
                .collect(),
        );
        let mut boundaries = Vec::with_capacity(legal_cuts.len() + 2);
        boundaries.push(0);
        boundaries.extend(legal_cuts);
        boundaries.push(layout.data.text_len);
        let boundary_advances =
            super::line_break::parallel_boundary_advances(&mut layout, &boundaries)
                .ok_or(ParallelInlineError::UnsupportedRow)?;
        if layout.len() != 1 || !layout.full_width().is_finite() || layout.full_width() < 0.0 {
            return Err(ParallelInlineError::UnsupportedRow);
        }
        let cursors = boundaries[..boundaries.len() - 1]
            .iter()
            .map(|byte| {
                let index = shaped_boundaries
                    .binary_search_by_key(byte, |entry| entry.0)
                    .map_err(|_| ParallelInlineError::InvalidCut)?;
                let (_, mut item, cluster) = shaped_boundaries[index];
                // A source cut begins before its boundary anchors, preserving
                // their original order and the row-start owner context.
                while item > 0 {
                    let previous = &layout.data.items[item - 1];
                    if previous.kind != LayoutItemKind::InlineBox
                        || layout.data.inline_boxes[previous.index].index != *byte
                    {
                        break;
                    }
                    item -= 1;
                }
                Ok((item, cluster))
            })
            .collect::<Result<Vec<_>, ParallelInlineError>>()?;
        Ok(Self {
            layout: Arc::new(layout),
            boundaries: boundaries.into(),
            cursors: cursors.into(),
            boundary_advances: boundary_advances.into(),
        })
    }

    /// Complete source boundary table, including zero and the terminal offset.
    pub fn boundaries(&self) -> &[usize] {
        &self.boundaries
    }
}

/// Parallel shaped rows occupying a single inline participant.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedParallelInline<B: Brush> {
    pub(crate) rows: Arc<[PreparedParallelRow<B>]>,
    full_advance: f32,
}

impl<B: Brush> PreparedParallelInline<B> {
    /// Retain rows in caller-defined order.
    pub fn new(rows: Vec<PreparedParallelRow<B>>) -> Result<Self, ParallelInlineError> {
        if rows.is_empty() {
            return Err(ParallelInlineError::EmptyGroup);
        }
        let full_advance = rows
            .iter()
            .map(|row| row.layout.width())
            .fold(0.0, f32::max);
        Ok(Self {
            rows: rows.into(),
            full_advance,
        })
    }
    /// Natural inline extent of all complete rows before fragmentation.
    pub fn full_advance(&self) -> f32 {
        self.full_advance
    }
}

/// Source intervals selected together for one positioned parallel fragment.
#[derive(Clone, Debug, PartialEq)]
pub struct ParallelInlineFragment {
    pub(crate) rows: Arc<[Range<usize>]>,
    pub(crate) inline_advance: f32,
}

impl ParallelInlineFragment {
    /// Selected geometric inline extent, excluding surrounding tracking.
    pub fn inline_advance(&self) -> f32 {
        self.inline_advance
    }
    /// One boundary-ordinal interval per prepared row, in its original order.
    pub fn rows(&self) -> &[Range<usize>] {
        &self.rows
    }
}

impl<B: Brush> Layout<B> {
    /// Shaped grapheme boundaries that do not bisect ligature material.
    /// These are geometric candidates; callers must still resolve line-break policy.
    pub fn source_grapheme_boundaries(&self) -> Vec<usize> {
        let mut boundaries = Vec::new();
        for item in self
            .data
            .items
            .iter()
            .filter(|item| item.kind == LayoutItemKind::TextRun)
        {
            let run = &self.data.runs[item.index];
            boundaries.extend(
                self.data.clusters[item.cluster_range.clone()]
                    .iter()
                    .filter(|cluster| {
                        cluster.flags & ClusterData::GRAPHEME_START != 0
                            && !cluster.is_ligature_component()
                    })
                    .map(|cluster| cluster.text_range(run).start),
            );
        }
        boundaries.push(self.data.text_len);
        boundaries.sort_unstable();
        boundaries.dedup();
        boundaries
    }

    /// Bind groups to existing atomic identities. Invalid bindings leave the
    /// previous groups unchanged.
    pub fn set_parallel_inline_groups(
        &mut self,
        mut groups: Vec<(u64, PreparedParallelInline<B>)>,
    ) -> Result<(), ParallelInlineError> {
        groups.sort_by_key(|(id, _)| *id);
        let mut participants = self
            .data
            .inline_boxes
            .iter()
            .map(|item| {
                (
                    item.id,
                    matches!(
                        item.line_break_participation(),
                        crate::inline_box::InlineBoxLineBreakParticipation::Atomic(_)
                    ),
                )
            })
            .collect::<Vec<_>>();
        participants.sort_unstable_by_key(|participant| participant.0);
        if groups.windows(2).any(|pair| pair[0].0 == pair[1].0)
            || groups.iter().any(|(id, _)| {
                let Ok(index) = participants.binary_search_by_key(id, |participant| participant.0)
                else {
                    return true;
                };
                !participants[index].1
                    || index
                        .checked_sub(1)
                        .is_some_and(|previous| participants[previous].0 == *id)
                    || participants
                        .get(index + 1)
                        .is_some_and(|next| next.0 == *id)
            })
        {
            return Err(ParallelInlineError::InvalidParticipant);
        }
        self.data.parallel_inline_groups = groups;
        Ok(())
    }
}
