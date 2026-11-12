// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{BreakLines, LineTabOrigin};
use crate::layout::{
    Layout, LineBreakPurpose, ParallelInlineCut, ParallelInlineFragment, PreparedParallelRow,
};
use crate::{Brush, InlineBoxBreakAffinity};
use alloc::{sync::Arc, vec::Vec};

/// Reuse the total-fit probe's cumulative source measures for progress only;
/// selected interval fitting continues to use the exact native row breaker.
pub(in crate::layout) fn boundary_advances<B: Brush>(
    layout: &mut Layout<B>,
    boundaries: &[usize],
    inline_boundaries: Option<&[usize]>,
) -> Option<Vec<f32>> {
    let mut breaker = layout.break_lines();
    if let Some(items) = inline_boundaries {
        breaker.parallel_row_cuts = Some(
            items[1..items.len() - 1]
                .iter()
                .map(|item| (*item, crate::OverflowWrap::Normal))
                .collect(),
        );
    }
    breaker.state.probe = Some(super::total_fit::Segment::default());
    let candidate = breaker.preview_shape_candidate_with(|probe| {
        probe
            .break_next_first_fit(f32::MAX, LineTabOrigin::ZERO)
            .map(|_| ())
    })?;
    let segment = candidate.state.probe.as_ref()?;
    let mut measured = Vec::with_capacity(segment.opportunities.len() + 2);
    measured.push((0, 0.0));
    for opportunity in &segment.opportunities {
        let byte = if inline_boundaries.is_some() {
            opportunity.cursor.0
        } else {
            opportunity.projected_boundary.unwrap_or_else(|| {
                super::SourceCursor::at(
                    &breaker.layout.data,
                    opportunity.cursor.0,
                    opportunity.cursor.1,
                )
                .source_offset(&breaker.layout.data)
            })
        };
        measured.push((byte, opportunity.start.advance));
    }
    measured.push((
        if inline_boundaries.is_some() {
            breaker.layout.data.items.len()
        } else {
            breaker.layout.data.text_len
        },
        segment.end?.advance,
    ));
    measured.sort_unstable_by_key(|entry| entry.0);
    let advances = boundaries
        .iter()
        .map(|byte| {
            let index = measured.binary_search_by_key(byte, |entry| entry.0).ok()?;
            let advance = measured[index].1;
            advance.is_finite().then_some(advance)
        })
        .collect::<Option<Vec<_>>>()?;
    breaker.commit_shape_candidate(candidate);
    Some(advances)
}

#[derive(Clone)]
pub(super) struct ParallelContinuation {
    pub(super) item: usize,
    starts: Vec<usize>,
}

#[derive(Clone)]
pub(super) struct SelectedParallelFragment {
    pub(super) item: usize,
    pub(super) advance: f32,
    pub(super) fragment: ParallelInlineFragment,
    pub(super) previous: Option<Arc<Self>>,
}

impl Drop for SelectedParallelFragment {
    fn drop(&mut self) {
        let mut previous = self.previous.take();
        while let Some(link) = previous {
            match Arc::try_unwrap(link) {
                Ok(mut fragment) => previous = fragment.previous.take(),
                Err(_) => break,
            }
        }
    }
}

pub(super) struct ParallelFit {
    pub(super) fragment: ParallelInlineFragment,
    pub(super) advance: f32,
    pub(super) complete: bool,
}

// shortcut: candidate resets scan the full row; reuse a persistent breaker for long paired rows.
fn row_fit<B: Brush>(
    layout: &mut Layout<B>,
    row: &PreparedParallelRow<B>,
    start: usize,
    limit: usize,
    measure: f32,
    purpose: LineBreakPurpose,
    emergency: bool,
) -> (usize, f32) {
    layout.set_line_break_purpose(purpose);
    layout.set_line_break_overrides(if emergency {
        row.emergency_overrides.to_vec()
    } else {
        row.normal_overrides.to_vec()
    });
    let byte_limit = row.boundaries[limit];
    layout.data.first_line_source_end =
        (row.inline_boundaries.is_none() && limit + 1 < row.boundaries.len()).then_some(byte_limit);
    let (item_index, cluster_index) = row.cursors[start];
    let mut breaker = layout.break_lines();
    if let Some(items) = &row.inline_boundaries {
        breaker.source_probe = Some(super::SourceProbe::Prefix(super::SourceCursor::BeforeItem(
            items[limit],
        )));
        breaker.parallel_row_cuts = Some(
            row.cuts
                .iter()
                .enumerate()
                .filter_map(|(index, cut)| {
                    let ParallelInlineCut::AfterInlineBox { overflow, .. } = cut else {
                        return None;
                    };
                    (*overflow == crate::OverflowWrap::Normal
                        || emergency && purpose.allows_emergency_wrap(*overflow))
                    .then_some((items[index + 1], *overflow))
                })
                .collect(),
        );
    }
    breaker.state.item_idx = item_index;
    breaker.state.cluster_idx = cluster_index;
    breaker.state.line.items = item_index..item_index;
    breaker.state.line.clusters = cluster_index..cluster_index;
    breaker.break_next(measure, LineTabOrigin::ZERO);
    let line = breaker
        .lines
        .lines
        .first()
        .expect("nonempty prepared row must emit a line");
    let end = if let Some(items) = &row.inline_boundaries {
        items
            .binary_search(&breaker.state.item_idx)
            .expect("whole-child row ends at a validated native item cursor")
    } else {
        let source_end = breaker
            .state
            .taken_projected_source_boundary
            .map_or(line.text_range.end, |boundary| boundary.target());
        row.boundaries
            .binary_search(&source_end)
            .expect("row line must end at a validated source boundary")
    };
    assert!(
        end > start && end <= limit,
        "parallel rows must make bounded source progress"
    );
    (end, line.measured_advance())
}

impl<B: Brush> BreakLines<'_, B> {
    pub(super) fn parallel_group_index(&self, id: u64) -> Option<usize> {
        self.layout
            .data
            .parallel_inline_groups
            .binary_search_by_key(&id, |(id, _)| *id)
            .ok()
    }

    pub(super) fn fit_parallel_group(
        &mut self,
        group_index: usize,
        measure: f32,
        partial: bool,
        emergency: bool,
    ) -> Option<ParallelFit> {
        let purpose = self.layout.data.line_break_purpose;
        let group = &self.layout.data.parallel_inline_groups[group_index].1;
        if emergency && !group.rows.iter().any(|row| row.cuts.iter().any(|cut| {
            matches!(cut, ParallelInlineCut::Emergency { overflow, .. } | ParallelInlineCut::AfterInlineBox { overflow, .. } if purpose.allows_emergency_wrap(*overflow))
        })) { return None; }
        let starts = self
            .state
            .line
            .parallel_continuation
            .as_ref()
            .filter(|continuation| continuation.item == self.state.item_idx)
            .map_or_else(
                || alloc::vec![0; group.rows.len()],
                |continuation| continuation.starts.clone(),
            );
        let limits = group
            .rows
            .iter()
            .zip(&starts)
            .map(|(row, &start)| {
                if !partial {
                    return Some(row.boundaries.len() - 1);
                }
                row.cuts.iter().enumerate().rev().find_map(|(index, cut)| {
                    let ordinal = index + 1;
                    let allowed = match cut {
                        ParallelInlineCut::Normal(_) => true,
                        ParallelInlineCut::AfterInlineBox { overflow, .. }
                            if *overflow == crate::OverflowWrap::Normal =>
                        {
                            true
                        }
                        ParallelInlineCut::Emergency { overflow, .. }
                        | ParallelInlineCut::AfterInlineBox { overflow, .. } => {
                            emergency && purpose.allows_emergency_wrap(*overflow)
                        }
                    };
                    (ordinal > start && allowed).then_some(ordinal)
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let mut rows = Vec::with_capacity(group.rows.len());
        let mut widths = Vec::with_capacity(group.rows.len());
        for (index, (row, &start)) in group.rows.iter().zip(&starts).enumerate() {
            let limit = limits[index];
            let (end, width) = row_fit(
                &mut self.parallel_rows[group_index][index],
                row,
                start,
                limit,
                measure,
                purpose,
                emergency,
            );
            widths.push(width);
            rows.push(start..end);
        }
        if partial {
            let minimum = widths.iter().copied().fold(0.0_f32, f32::max);
            let overflowing = minimum > measure;
            let mut progress = if overflowing { 0.0_f32 } else { 1.0_f32 };
            let mut weighted = false;
            for (row, range) in group.rows.iter().zip(&rows) {
                let start = row.boundary_advances[range.start];
                let remaining = row.boundary_advances.last().copied().unwrap_or(start) - start;
                let consumed = row.boundary_advances[range.end] - start;
                if remaining > 0.0 && remaining.is_finite() && consumed.is_finite() {
                    let fraction = (consumed / remaining).clamp(0.0, 1.0);
                    progress = if overflowing {
                        progress.max(fraction)
                    } else {
                        progress.min(fraction)
                    };
                    weighted = true;
                }
            }
            // Balance actual native source progress, not absolute row lengths.
            // At zero measure, the coarsest legal prefix establishes the minimum
            // feasible progress and denser rows consume their matching prefix.
            for (index, ((row, &start), width)) in
                group.rows.iter().zip(&starts).zip(&mut widths).enumerate()
            {
                let remaining = row.boundary_advances.last().copied().unwrap_or(0.0)
                    - row.boundary_advances[start];
                let target = if weighted && remaining > 0.0 && remaining.is_finite() {
                    let target = remaining * progress;
                    if overflowing {
                        target
                    } else {
                        target.min(measure)
                    }
                } else if overflowing {
                    minimum
                } else {
                    measure
                };
                if *width != target {
                    let limit = limits[index];
                    let (end, fitted) = row_fit(
                        &mut self.parallel_rows[group_index][index],
                        row,
                        start,
                        limit,
                        target,
                        purpose,
                        emergency,
                    );
                    rows[index] = start..end;
                    *width = fitted;
                }
            }
        }
        let advance = widths.into_iter().fold(0.0_f32, f32::max);
        let complete = rows
            .iter()
            .zip(group.rows.iter())
            .all(|(range, row)| range.end + 1 == row.boundaries.len());
        Some(ParallelFit {
            fragment: ParallelInlineFragment {
                rows: rows.into(),
                inline_advance: advance,
            },
            advance,
            complete,
        })
    }

    pub(super) fn append_parallel_fragment(
        &mut self,
        fit: ParallelFit,
        height: f32,
        affinity: InlineBoxBreakAffinity,
    ) {
        let item = self.state.item_idx;
        let inline_box = &self.layout.data.inline_boxes[self.layout.data.items[item].index];
        let advance = fit.advance + inline_box.advance_for_line(self.state.lines)
            - inline_box.width_for_line(self.state.lines);
        self.state.append_inline_box_to_line(
            self.state.line.x + advance,
            self.state.line.fit_x + advance,
            height,
            inline_box.shaping_participation(),
            inline_box.source_kind(),
            None,
        );
        self.state.line.parallel_continuation = (!fit.complete).then(|| ParallelContinuation {
            item,
            starts: fit.fragment.rows.iter().map(|range| range.end).collect(),
        });
        self.state.line.parallel_fragments = Some(Arc::new(SelectedParallelFragment {
            item,
            advance,
            fragment: fit.fragment,
            previous: self.state.line.parallel_fragments.clone(),
        }));
        if fit.complete {
            self.state.item_idx += 1;
            self.state.mark_inline_box_break_after(affinity);
        }
    }
}
