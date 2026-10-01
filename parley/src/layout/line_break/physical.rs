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
        crate::shape::physical::PhysicalWork {
            visited_clusters: self.visited_clusters,
            ..self
                .physical_shaper
                .as_ref()
                .expect("physical source shaper")
                .work()
        }
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

    /// The fitting candidate of the line with physical glyphs. The first terminal line is the natural line at four
    /// times the measure, and each next one doubles that width, until a terminal line gives the result of the
    /// terminal line to the next forced break. The last one is that terminal line.
    pub(super) fn preview_physical_candidate(
        &mut self,
        measure: f32,
        tab_origin: LineTabOrigin,
    ) -> Option<LineShapeCandidate> {
        let fit = PhysicalFit {
            measure,
            tab_origin,
            expanded: self
                .shape_candidates
                .as_ref()
                .is_some_and(|shapes| shapes.expanded),
            line_start: SourceCursor::at(
                &self.layout.data,
                self.state.item_idx,
                self.state.cluster_idx,
            )
            .source_offset(&self.layout.data),
            limit: self
                .preview_natural_candidate(measure, tab_origin)
                .map(|natural| natural.line.text_range.end + natural.line.text_range.len()),
        };
        let mut reach = Some(4.0 * measure).filter(|reach| reach.is_finite());
        let mut previous = None;
        loop {
            let terminal = self.preview_natural_candidate(reach.unwrap_or(f32::MAX), tab_origin)?;
            let end = terminal.source_end(&self.layout.data);
            if let Some(candidate) = self.fit_physical_line(fit, end, reach.is_none()) {
                return Some(candidate);
            }
            reach = reach
                .map(|reach| 2.0 * reach.max(terminal.line.metrics.advance))
                .filter(|wider| wider.is_finite() && previous != Some(end));
            previous = Some(end);
        }
    }

    /// The fitting candidate of the line from the terminal line that ends at `terminal`. Without `complete`, the
    /// terminal line can end before the forced break: it gives `None` when its glyphs, natural break or bound can
    /// differ from those of the terminal line to the forced break.
    fn fit_physical_line(
        &mut self,
        fit: PhysicalFit,
        terminal: SourceCursor,
        complete: bool,
    ) -> Option<LineShapeCandidate> {
        let cut = terminal.source_offset(&self.layout.data);
        if !complete
            && !self
                .physical_shaper
                .as_ref()
                .expect("physical source shaper")
                .keeps_boundaries_before(terminal.source_key(&self.layout.data), cut)
        {
            return None;
        }
        let endpoints = self.source_endpoints(terminal);
        let mut limit = fit.limit;
        let mut fallback = None;
        let mut bound = None;
        for end in endpoints.iter().copied().rev() {
            if bound.is_some_and(|bound: SourceCursor| end.key() > bound.key()) {
                continue;
            }
            let Some(prefix) = self.preview_source_prefix(end, fit.tab_origin) else {
                if bound.is_none() && !complete {
                    return None;
                }
                continue;
            };
            let (shape, candidate) = loop {
                let (shape, boundaries) = self.physical_prefix_shape(
                    &prefix,
                    limit.filter(|_| bound.is_none()),
                    fit.expanded,
                );
                if bound.is_none() && !complete && !shape.ends_before(cut) {
                    return None;
                }
                let installed = self.install_physical_shape(&shape);
                let normal = self
                    .preview_natural_candidate(fit.measure, fit.tab_origin)
                    .expect("retained source prefix");
                let normal_end = normal.source_end(&self.layout.data);
                let line_bound = match bound {
                    Some(bound) => Some(bound),
                    None if complete => Some(
                        self.first_regular_break_after(&endpoints, normal_end, fit.tab_origin)
                            .unwrap_or(terminal),
                    ),
                    None if normal_end.key() < terminal.key() => {
                        self.first_regular_break_after(&endpoints, normal_end, fit.tab_origin)
                    }
                    None => None,
                };
                let Some(line_bound) = line_bound else {
                    self.remove_physical_shape(installed);
                    return None;
                };
                if !shape.covers(line_bound.source_offset(&self.layout.data)) {
                    self.remove_physical_shape(installed);
                    limit = limit.map(|limit| {
                        limit.max(fit.line_start) + limit.saturating_sub(fit.line_start).max(1)
                    });
                    continue;
                }
                bound = Some(line_bound);
                let candidate = match normal_end.key().cmp(&end.key()) {
                    core::cmp::Ordering::Less => None,
                    core::cmp::Ordering::Equal => Some(normal),
                    core::cmp::Ordering::Greater => {
                        self.preview_source_break(end, fit.measure, fit.tab_origin)
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

    /// The first endpoint after `natural_end` where a line can end with a regular break.
    fn first_regular_break_after(
        &mut self,
        endpoints: &[SourceCursor],
        natural_end: SourceCursor,
        tab_origin: LineTabOrigin,
    ) -> Option<SourceCursor> {
        endpoints
            .iter()
            .copied()
            .filter(|end| end.key() > natural_end.key())
            .find(|end| {
                self.preview_source_break(*end, f32::MAX, tab_origin)
                    .is_some_and(|candidate| candidate.line.break_reason == BreakReason::Regular)
            })
    }
}

/// The inputs of the physical line fit that do not depend on the terminal line.
#[derive(Clone, Copy)]
struct PhysicalFit {
    measure: f32,
    tab_origin: LineTabOrigin,
    expanded: bool,
    line_start: usize,
    limit: Option<usize>,
}
