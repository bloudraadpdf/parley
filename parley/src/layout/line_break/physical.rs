// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{BreakLines, LineTabOrigin, justification::LineShapeCandidate};
use crate::{Brush, layout::physical_shaping::physical_shaping_boundaries};

impl<B: Brush> BreakLines<'_, B> {
    pub(super) fn shape_physical_prefix(
        &mut self,
        prefix: &LineShapeCandidate,
        expanded: bool,
    ) -> alloc::vec::Vec<usize> {
        let boundaries = physical_shaping_boundaries(
            &self.layout.data,
            &prefix.items,
            &self.layout.data.inline_owner_shaping,
        );
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
                expanded,
            );
        shape.install(&mut self.layout.data);
        self.letter_spacing_edges.clear();
        boundaries
    }

    pub(super) fn preview_physical_candidate(
        &mut self,
        measure: f32,
        tab_origin: LineTabOrigin,
    ) -> Option<LineShapeCandidate> {
        let terminal = self.preview_natural_candidate(f32::MAX, tab_origin)?;
        let endpoints = self.source_endpoints(terminal.source_end(&self.layout.data));
        let original = self.layout.data.clusters.clone();
        let mut retained_glyphs = self.layout.data.glyphs.len();
        let expanded = self
            .shape_candidates
            .as_ref()
            .is_some_and(|shapes| shapes.expanded);
        let mut fallback = None;
        for end in endpoints.into_iter().rev() {
            self.layout.data.clusters.clone_from(&original);
            self.layout.data.glyphs.truncate(retained_glyphs);
            let Some(prefix) = self.preview_source_prefix(end, tab_origin) else {
                continue;
            };
            let boundaries = self.shape_physical_prefix(&prefix, expanded);
            let normal = self
                .preview_natural_candidate(measure, tab_origin)
                .expect("retained source prefix");
            let normal_end = normal.source_end(&self.layout.data);
            let candidate = match normal_end.key().cmp(&end.key()) {
                core::cmp::Ordering::Less => None,
                core::cmp::Ordering::Equal => Some(normal),
                core::cmp::Ordering::Greater => self.preview_source_break(end, measure, tab_origin),
            };
            let Some(mut candidate) = candidate else {
                continue;
            };
            if physical_shaping_boundaries(
                &self.layout.data,
                &candidate.items,
                &self.layout.data.inline_owner_shaping,
            ) != boundaries
            {
                continue;
            }
            candidate.physical_clusters = Some(self.layout.data.clusters.clone());
            let fits = super::line_advance_fits(
                candidate.line.fitting_advance(),
                candidate.line.max_advance,
                candidate
                    .items
                    .iter()
                    .map(|item| item.cluster_range.len() + 1)
                    .sum(),
            );
            retained_glyphs = self.layout.data.glyphs.len();
            fallback = Some(candidate);
            if fits {
                break;
            }
        }
        self.layout.data.clusters = original;
        self.layout.data.glyphs.truncate(retained_glyphs);
        self.letter_spacing_edges.clear();
        Some(fallback.expect("retained source has a terminal or unavoidable break"))
    }
}
