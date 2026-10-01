// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{
    BreakLines, LineTabOrigin, justification::LineShapeCandidate, source_probe::SourceCursor,
};
use crate::{Brush, layout::BreakReason, shape::physical::PhysicalShape};

impl<B: Brush> BreakLines<'_, B> {
    #[cfg(test)]
    pub(crate) fn shaped_line_boundaries(&self) -> alloc::collections::BTreeSet<usize> {
        self.physical_shaper
            .as_ref()
            .expect("physical source shaper")
            .line_boundaries()
    }

    #[cfg(test)]
    pub(crate) fn physical_work(&self) -> crate::shape::physical::PhysicalWork {
        self.physical_shaper
            .as_ref()
            .expect("physical source shaper")
            .work()
    }

    pub(super) fn physical_prefix_shape(
        &mut self,
        prefix: &LineShapeCandidate,
        limit: Option<usize>,
        expanded: bool,
    ) -> (PhysicalShape, alloc::vec::Vec<usize>) {
        let boundaries = self
            .physical_shaper
            .as_ref()
            .expect("physical source shaper")
            .boundaries(&self.layout.data, &prefix.items);
        let soft_boundaries = (!prefix.line.text_range.is_empty())
            .then_some([prefix.line.text_range.start, prefix.line.text_range.end]);
        let shape = self
            .physical_shaper
            .as_mut()
            .expect("physical source shaper")
            .shape_line(
                &self.layout.data,
                &boundaries,
                soft_boundaries
                    .as_ref()
                    .map_or(&[], |range| range.as_slice()),
                prefix.line.text_range.start,
                limit,
                expanded,
            );
        (shape, boundaries)
    }

    pub(super) fn install_physical_shape(
        &mut self,
        shape: &PhysicalShape,
    ) -> crate::shape::physical::InstalledShape {
        let installed = self
            .physical_shaper
            .as_mut()
            .expect("physical source shaper")
            .install(&mut self.layout.data, shape);
        self.letter_spacing_edges.clear();
        installed
    }

    pub(super) fn remove_physical_shape(
        &mut self,
        installed: crate::shape::physical::InstalledShape,
    ) {
        self.physical_shaper
            .as_mut()
            .expect("physical source shaper")
            .remove(&mut self.layout.data, installed);
        self.letter_spacing_edges.clear();
    }

    pub(super) fn preview_physical_candidate(
        &mut self,
        measure: f32,
        tab_origin: LineTabOrigin,
    ) -> Option<LineShapeCandidate> {
        let terminal = self.preview_natural_candidate(f32::MAX, tab_origin)?;
        let endpoints = self.source_endpoints(terminal.source_end(&self.layout.data));
        let expanded = self
            .shape_candidates
            .as_ref()
            .is_some_and(|shapes| shapes.expanded);
        let line_start = SourceCursor::at(
            &self.layout.data,
            self.state.item_idx,
            self.state.cluster_idx,
        )
        .source_offset(&self.layout.data);
        let mut limit = self
            .preview_natural_candidate(measure, tab_origin)
            .map(|natural| natural.line.text_range.end + natural.line.text_range.len());
        let mut fallback = None;
        let mut bound = None;
        for end in endpoints.iter().copied().rev() {
            if bound.is_some_and(|bound: SourceCursor| end.key() > bound.key()) {
                continue;
            }
            let Some(prefix) = self.preview_source_prefix(end, tab_origin) else {
                continue;
            };
            let (shape, candidate) = loop {
                let (shape, boundaries) = self.physical_prefix_shape(
                    &prefix,
                    limit.filter(|_| bound.is_none()),
                    expanded,
                );
                let installed = self.install_physical_shape(&shape);
                let normal = self
                    .preview_natural_candidate(measure, tab_origin)
                    .expect("retained source prefix");
                let normal_end = normal.source_end(&self.layout.data);
                let line_bound = bound.unwrap_or_else(|| {
                    self.first_regular_break_after(&endpoints, normal_end, tab_origin)
                });
                if !shape.covers(line_bound.source_offset(&self.layout.data)) {
                    self.remove_physical_shape(installed);
                    limit = limit.map(|limit| {
                        limit.max(line_start) + limit.saturating_sub(line_start).max(1)
                    });
                    continue;
                }
                bound = Some(line_bound);
                let candidate = match normal_end.key().cmp(&end.key()) {
                    core::cmp::Ordering::Less => None,
                    core::cmp::Ordering::Equal => Some(normal),
                    core::cmp::Ordering::Greater => {
                        self.preview_source_break(end, measure, tab_origin)
                    }
                }
                .filter(|candidate| {
                    self.physical_shaper
                        .as_ref()
                        .expect("physical source shaper")
                        .boundaries(&self.layout.data, &candidate.items)
                        == boundaries
                });
                self.remove_physical_shape(installed);
                break (shape, candidate);
            };
            let Some(mut candidate) = candidate else {
                continue;
            };
            let fits = super::line_advance_fits(
                candidate.line.fitting_advance(),
                candidate.line.max_advance,
                candidate
                    .items
                    .iter()
                    .map(|item| item.cluster_range.len() + 1)
                    .sum(),
            );
            candidate.physical_shape = Some(shape);
            fallback = Some(candidate);
            if fits {
                break;
            }
        }
        Some(fallback.expect("retained source has a terminal or unavoidable break"))
    }

    fn first_regular_break_after(
        &mut self,
        endpoints: &[SourceCursor],
        natural_end: SourceCursor,
        tab_origin: LineTabOrigin,
    ) -> SourceCursor {
        let terminal = *endpoints.last().expect("terminal endpoint");
        endpoints
            .iter()
            .copied()
            .filter(|end| end.key() > natural_end.key())
            .find(|end| {
                self.preview_source_break(*end, f32::MAX, tab_origin)
                    .is_some_and(|candidate| candidate.line.break_reason == BreakReason::Regular)
            })
            .unwrap_or(terminal)
    }
}
