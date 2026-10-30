// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{BreakLines, LineTabOrigin, justification::LineShapeCandidate};
use crate::{Brush, layout::LayoutItemKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceCursor {
    BeforeItem(usize),
    WithinText { item: usize, cluster: usize },
}

impl SourceCursor {
    pub(super) fn at<B: Brush>(
        data: &crate::layout::LayoutData<B>,
        mut item: usize,
        cluster: usize,
    ) -> Self {
        while let Some(source) = data.items.get(item) {
            if source.kind == LayoutItemKind::InlineBox {
                return Self::BeforeItem(item);
            }
            if cluster <= source.cluster_range.start {
                return Self::BeforeItem(item);
            }
            if cluster < source.cluster_range.end {
                return Self::WithinText { item, cluster };
            }
            item += 1;
        }
        Self::BeforeItem(item)
    }

    pub(super) fn source_offset<B: Brush>(self, data: &crate::layout::LayoutData<B>) -> usize {
        let (item, cluster) = match self {
            Self::BeforeItem(item) => (item, None),
            Self::WithinText { item, cluster } => (item, Some(cluster)),
        };
        let Some(source) = data.items.get(item) else {
            return data.text_len;
        };
        match (source.kind, cluster) {
            (LayoutItemKind::TextRun, Some(cluster)) => {
                data.clusters[cluster]
                    .text_range(&data.runs[source.index])
                    .start
            }
            (LayoutItemKind::TextRun, None) => source.text_range.start,
            (LayoutItemKind::InlineBox, _) => data.inline_boxes[source.index].index,
        }
    }

    /// The position of the cursor among the source items: its item, with its byte in a text item.
    pub(super) fn source_key<B: Brush>(
        self,
        data: &crate::layout::LayoutData<B>,
    ) -> crate::layout::SourceKey {
        match self {
            Self::BeforeItem(item) => (item, None),
            Self::WithinText { item, .. } => (item, Some(self.source_offset(data))),
        }
    }

    pub(super) fn key(self) -> (usize, Option<usize>) {
        match self {
            Self::BeforeItem(item) => (item, None),
            Self::WithinText { item, cluster } => (item, Some(cluster)),
        }
    }

    fn normalized<B: Brush>(self, data: &crate::layout::LayoutData<B>) -> Self {
        match self {
            Self::WithinText { item, cluster } => Self::at(data, item, cluster),
            cursor => cursor,
        }
    }

    fn matches<B: Brush>(
        self,
        data: &crate::layout::LayoutData<B>,
        item: usize,
        cluster: usize,
    ) -> bool {
        self.normalized(data) == Self::at(data, item, cluster)
    }

    fn precedes<B: Brush>(
        self,
        data: &crate::layout::LayoutData<B>,
        item: usize,
        cluster: usize,
    ) -> bool {
        self.normalized(data).key() < Self::at(data, item, cluster).key()
    }
}

#[derive(Clone, Copy)]
pub(super) enum SourceProbe {
    Prefix(SourceCursor),
    LegalBreak(SourceCursor),
}

impl SourceProbe {
    pub(super) fn is_prefix(self) -> bool {
        matches!(self, Self::Prefix(_))
    }
}

pub(super) enum ProbedBreak {
    Regular(super::RegularBreakCandidate),
    Emergency(super::BoundarySnapshot),
    PassedEnd,
}

impl<B: Brush> BreakLines<'_, B> {
    pub(super) fn source_endpoints(&self, terminal: SourceCursor) -> alloc::vec::Vec<SourceCursor> {
        let start = SourceCursor::at(
            &self.layout.data,
            self.state.item_idx,
            self.state.cluster_idx,
        );
        let mut endpoints = alloc::vec::Vec::new();
        for (index, item) in self
            .layout
            .data
            .items
            .iter()
            .enumerate()
            .skip(self.state.item_idx)
        {
            let before = SourceCursor::BeforeItem(index);
            if before.key() > terminal.key() {
                break;
            }
            if before.key() > start.key() {
                endpoints.push(before);
            }
            if before == terminal {
                break;
            }
            if item.kind == LayoutItemKind::TextRun {
                let first = (item.cluster_range.start + 1).max(if index == self.state.item_idx {
                    self.state.cluster_idx + 1
                } else {
                    0
                });
                let last = match terminal {
                    SourceCursor::WithinText {
                        item: terminal_item,
                        cluster,
                    } if terminal_item == index => cluster.min(item.cluster_range.end),
                    _ => item.cluster_range.end,
                };
                endpoints.extend((first..last).map(|cluster| {
                    #[cfg(test)]
                    self.visited_source_endpoints
                        .set(self.visited_source_endpoints.get() + 1);
                    SourceCursor::WithinText {
                        item: index,
                        cluster,
                    }
                }));
            }
        }
        if endpoints.last() != Some(&terminal) {
            endpoints.push(terminal);
        }
        endpoints
    }
    pub(super) fn prefix_ends_at(&self, cursor: SourceCursor) -> bool {
        matches!(self.source_probe, Some(SourceProbe::Prefix(end)) if end == cursor)
            || self.fitted_end_precedes_collapsible_space(cursor)
    }

    fn fitted_end_precedes_collapsible_space(&self, cursor: SourceCursor) -> bool {
        if self.state.lines != 0
            || self.layout.data.first_line_source_end
                != Some(cursor.source_offset(&self.layout.data))
        {
            return false;
        }
        let (item, cluster) = match cursor {
            SourceCursor::BeforeItem(item) => (item, None),
            SourceCursor::WithinText { item, cluster } => (item, Some(cluster)),
        };
        let Some(item) = self
            .layout
            .data
            .items
            .get(item)
            .filter(|item| item.kind == LayoutItemKind::TextRun)
        else {
            return false;
        };
        let Some(cluster) = self
            .layout
            .data
            .clusters
            .get(cluster.unwrap_or(item.cluster_range.start))
        else {
            return false;
        };
        crate::layout::data::WhiteSpaceLayoutMode::from_style(
            &self.layout.data.styles[cluster.style_index as usize],
        )
        .collapses_space(cluster.info.whitespace())
    }

    pub(super) fn probes_current_break(&self) -> bool {
        matches!(self.source_probe, Some(SourceProbe::LegalBreak(end))
            if end.matches(&self.layout.data, self.state.item_idx, self.state.cluster_idx))
    }

    pub(super) fn source_break_candidate(&self) -> Option<ProbedBreak> {
        let end = match self.source_probe {
            Some(SourceProbe::LegalBreak(end)) => Some(end),
            _ => None,
        };
        let byte_end = self
            .layout
            .data
            .first_line_source_end
            .filter(|end| self.state.lines == 0 && *end < self.layout.data.text_len);
        if end.is_none() && byte_end.is_none() {
            return None;
        }
        let matches = |item, cluster| {
            let offset =
                SourceCursor::at(&self.layout.data, item, cluster).source_offset(&self.layout.data);
            byte_end.is_none_or(|limit| offset <= limit)
                && (end.is_some_and(|end| end.matches(&self.layout.data, item, cluster))
                    || byte_end == Some(offset))
        };
        for candidate in [
            &self.state.prev_boundary,
            &self.state.restored_normal_boundary,
            &self.state.overflow_discretionary_boundary,
        ]
        .into_iter()
        .flatten()
        {
            let (snapshot, _) = candidate.clone().into_snapshot();
            if matches(snapshot.item_idx, snapshot.cluster_idx) {
                return Some(ProbedBreak::Regular(candidate.clone()));
            }
        }
        self.state
            .emergency_boundary
            .as_ref()
            .filter(|candidate| matches(candidate.0.item_idx, candidate.0.cluster_idx))
            .map(|candidate| ProbedBreak::Emergency(candidate.0.clone()))
            .or_else(|| {
                end.filter(|end| {
                    end.precedes(
                        &self.layout.data,
                        self.state.item_idx,
                        self.state.cluster_idx,
                    )
                })
                .map(|_| ProbedBreak::PassedEnd)
            })
    }

    pub(super) fn commit_source_prefix(&mut self, measure: f32, indent: f32) -> Option<(f32, f32)> {
        if self.state.line.items.is_empty() && self.state.line.clusters.is_empty() {
            return None;
        }
        let removes_leading_space = self.fitted_end_precedes_collapsible_space(SourceCursor::at(
            &self.layout.data,
            self.state.item_idx,
            self.state.cluster_idx,
        ));
        super::try_commit_line(
            self.layout,
            &self.cloned_edges,
            &mut self.lines,
            &mut self.state.line,
            measure,
            crate::layout::BreakReason::Regular,
            indent,
        )
        .then(|| {
            let result = self.start_new_line();
            if removes_leading_space {
                self.state.line.source_start_whitespace =
                    super::SourceStartWhitespace::RemoveCollapsible;
            }
            result
        })
        .flatten()
    }

    pub(super) fn preview_source_prefix(
        &mut self,
        end: SourceCursor,
        tab_origin: LineTabOrigin,
    ) -> Option<LineShapeCandidate> {
        self.preview_source(SourceProbe::Prefix(end), f32::MAX, tab_origin)
    }

    pub(super) fn preview_source_break(
        &mut self,
        end: SourceCursor,
        measure: f32,
        tab_origin: LineTabOrigin,
    ) -> Option<LineShapeCandidate> {
        self.preview_source(SourceProbe::LegalBreak(end), measure, tab_origin)
            .filter(|candidate| {
                end.matches(
                    &self.layout.data,
                    candidate.state.item_idx,
                    candidate.state.cluster_idx,
                )
            })
    }

    fn preview_source(
        &mut self,
        probe: SourceProbe,
        measure: f32,
        tab_origin: LineTabOrigin,
    ) -> Option<LineShapeCandidate> {
        let (SourceProbe::Prefix(end) | SourceProbe::LegalBreak(end)) = probe;
        match end {
            SourceCursor::BeforeItem(index) => {
                assert!(index <= self.layout.data.items.len());
            }
            SourceCursor::WithinText { item, cluster } => {
                let item = &self.layout.data.items[item];
                assert_eq!(item.kind, LayoutItemKind::TextRun);
                assert!(item.cluster_range.contains(&cluster));
            }
        }
        assert!(self.source_probe.is_none());
        self.source_probe = Some(probe);
        let candidate = self.preview_shape_candidate_with(|breaker| {
            breaker.break_next(measure, tab_origin).map(|_| ())
        });
        self.source_probe = None;
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_bidi_topology::build_layout_with_direction;
    use crate::{BaseDirection, InlineBox, layout::BreakReason};

    #[test]
    fn short_source_windows_visit_only_their_endpoints() {
        for text in [
            "abcdef ".repeat(16),
            "abc \u{05d0}\u{05d1}\u{05d2} def".into(),
        ] {
            let mut layout = build_layout_with_direction(&text, [], Some(BaseDirection::Ltr));
            let mut positions = alloc::vec::Vec::new();
            for (item, source) in layout.data.items.iter().enumerate() {
                positions.push(SourceCursor::BeforeItem(item));
                if source.kind == LayoutItemKind::TextRun {
                    positions.extend(
                        (source.cluster_range.start + 1..source.cluster_range.end)
                            .map(|cluster| SourceCursor::WithinText { item, cluster }),
                    );
                }
            }
            positions.push(SourceCursor::BeforeItem(layout.data.items.len()));
            let mut breaker = layout.break_lines();
            for window in positions.windows(4) {
                let (item, cluster) = match window[0] {
                    SourceCursor::BeforeItem(item) => (item, 0),
                    SourceCursor::WithinText { item, cluster } => (item, cluster),
                };
                breaker.state.item_idx = item;
                breaker.state.cluster_idx = cluster;
                assert_eq!(breaker.source_endpoints(window[3]), window[1..]);
            }
            assert!(
                breaker.visited_source_endpoints.get() <= 2 * positions.len(),
                "{} endpoint visits for {} positions",
                breaker.visited_source_endpoints.get(),
                positions.len()
            );
        }
    }

    fn cursor_at(layout: &crate::Layout<impl Brush>, byte: usize) -> SourceCursor {
        layout
            .data
            .items
            .iter()
            .enumerate()
            .find_map(|(item, source)| {
                if source.kind != LayoutItemKind::TextRun {
                    return None;
                }
                let run = &layout.data.runs[source.index];
                source
                    .cluster_range
                    .clone()
                    .find(|cluster| {
                        run.text_range.start + layout.data.clusters[*cluster].text_offset == byte
                    })
                    .map(|cluster| SourceCursor::WithinText { item, cluster })
            })
            .unwrap()
    }

    #[test]
    fn constrained_break_uses_a_normal_opportunity_at_the_actual_measure() {
        let mut layout = build_layout_with_direction("a b c", [], Some(BaseDirection::Ltr));
        let end = cursor_at(&layout, 4);
        let mut word = build_layout_with_direction("a", [], Some(BaseDirection::Ltr));
        word.break_all_lines(None);
        let mut breaker = layout.break_lines();
        let candidate = breaker
            .preview_source_break(end, f32::MAX, LineTabOrigin::ZERO)
            .unwrap();
        assert_eq!(candidate.line.text_range, 0..4);
        assert_eq!(candidate.line.break_reason, BreakReason::Regular);
        assert!(
            breaker
                .preview_source_break(end, word.width(), LineTabOrigin::ZERO)
                .is_none()
        );
        assert!(breaker.lines.lines.is_empty());
    }

    #[test]
    fn a_legal_break_probe_stops_at_its_endpoint() {
        let visited = |words: usize| {
            let text = "word ".repeat(words);
            let mut layout = build_layout_with_direction(&text, [], Some(BaseDirection::Ltr));
            let end = cursor_at(&layout, 2);
            let mut breaker = layout.break_lines();
            assert!(
                breaker
                    .preview_source_break(end, f32::MAX, LineTabOrigin::ZERO)
                    .is_none()
            );
            breaker.visited_clusters
        };
        assert_eq!(visited(100), visited(200));
    }

    #[test]
    fn constrained_break_cannot_promote_a_ligature_interior_to_a_break() {
        let mut layout = build_layout_with_direction("office", [], Some(BaseDirection::Ltr));
        let end = cursor_at(&layout, 2);
        let mut breaker = layout.break_lines();
        assert!(
            breaker
                .preview_source_prefix(end, LineTabOrigin::ZERO)
                .is_some()
        );
        assert!(
            breaker
                .preview_source_break(end, f32::MAX, LineTabOrigin::ZERO)
                .is_none()
        );
    }

    #[test]
    fn constrained_break_retains_discretionary_material_and_line_limits() {
        use crate::layout::{DiscretionaryBreak, DiscretionaryBreakCondition};

        let mut layout = build_layout_with_direction("ab\u{00ad}cd", [], Some(BaseDirection::Ltr));
        layout.set_discretionary_breaks(alloc::vec![DiscretionaryBreak {
            byte_index: 4,
            advance: 3.0,
            max_consecutive_lines: Some(1),
            condition: DiscretionaryBreakCondition::Normal,
        }]);
        let end = cursor_at(&layout, 4);
        let mut breaker = layout.break_lines();
        let candidate = breaker
            .preview_source_break(end, f32::MAX, LineTabOrigin::ZERO)
            .unwrap();
        assert_eq!(candidate.line.text_range, 0..4);
        assert_eq!(candidate.line.discretionary_advance, 3.0);
        assert!(candidate.line.ends_at_discretionary_break);
        breaker.state.consecutive_discretionary_lines = 1;
        assert!(
            breaker
                .preview_source_break(end, f32::MAX, LineTabOrigin::ZERO)
                .is_none()
        );
    }

    #[test]
    fn constrained_break_preserves_suppressed_source_opportunities() {
        let mut layout = build_layout_with_direction("a b", [], Some(BaseDirection::Ltr));
        layout.set_line_break_overrides(alloc::vec![crate::layout::LineBreakOverride::suppress(2)]);
        let end = cursor_at(&layout, 2);
        let mut breaker = layout.break_lines();
        assert!(
            breaker
                .preview_source_break(end, f32::MAX, LineTabOrigin::ZERO)
                .is_none()
        );
    }

    #[test]
    fn prefix_distinguishes_atomics_at_the_same_source_offset() {
        let mut layout = build_layout_with_direction(
            "ab",
            [
                InlineBox::new(11, 1, 10.0, 10.0),
                InlineBox::new(12, 1, 10.0, 10.0),
            ],
            Some(BaseDirection::Ltr),
        );
        let atomic_items = layout
            .data
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind == LayoutItemKind::InlineBox)
            .map(|(index, _)| index)
            .collect::<alloc::vec::Vec<_>>();
        let mut breaker = layout.break_lines();
        for (count, end) in atomic_items.into_iter().enumerate() {
            let candidate = breaker
                .preview_source_prefix(SourceCursor::BeforeItem(end), LineTabOrigin::default())
                .unwrap();
            assert_eq!(
                candidate
                    .items
                    .iter()
                    .filter(|item| item.kind == LayoutItemKind::InlineBox)
                    .count(),
                count
            );
            assert_eq!(candidate.line.text_range, 0..1);
            let legal = breaker
                .preview_source_break(SourceCursor::BeforeItem(end), f32::MAX, LineTabOrigin::ZERO)
                .unwrap();
            assert_eq!(legal.items, candidate.items);
            assert!(breaker.lines.lines.is_empty());
            assert_eq!(breaker.state.item_idx, 0);
        }
        assert!(breaker.source_probe.is_none());
    }

    #[test]
    fn prefix_stops_at_a_forced_break_before_its_requested_endpoint() {
        let mut layout = build_layout_with_direction("a\nb", [], Some(BaseDirection::Ltr));
        let end = SourceCursor::BeforeItem(layout.data.items.len());
        let mut breaker = layout.break_lines();
        let prefix = breaker
            .preview_source_prefix(end, LineTabOrigin::default())
            .unwrap();
        assert_eq!(prefix.line.text_range, 0..2);
        assert_eq!(prefix.line.break_reason, BreakReason::Explicit);
        let normal = breaker
            .preview_shape_candidate(f32::MAX, LineTabOrigin::default())
            .unwrap();
        assert_eq!(normal.line.text_range, prefix.line.text_range);
        assert_eq!(normal.items, prefix.items);
    }

    #[test]
    fn prefix_can_inspect_a_ligature_interior_without_accepting_a_break() {
        let mut layout = build_layout_with_direction("office", [], Some(BaseDirection::Ltr));
        let (item, cluster) = layout
            .data
            .items
            .iter()
            .enumerate()
            .find_map(|(index, item)| {
                if item.kind != LayoutItemKind::TextRun {
                    return None;
                }
                item.cluster_range
                    .clone()
                    .find(|cluster| layout.data.clusters[*cluster].text_offset == 2)
                    .map(|cluster| (index, cluster))
            })
            .unwrap();
        let clusters = layout.data.clusters.clone();
        assert!(clusters.iter().any(|cluster| cluster.is_ligature_start()));
        let glyphs = layout.data.glyphs.clone();
        let mut breaker = layout.break_lines();
        let prefix = breaker
            .preview_source_prefix(
                SourceCursor::WithinText { item, cluster },
                LineTabOrigin::default(),
            )
            .unwrap();
        assert_eq!(prefix.line.text_range, 0..2);
        assert_eq!(breaker.layout.data.clusters, clusters);
        assert_eq!(breaker.layout.data.glyphs, glyphs);
        assert!(breaker.lines.lines.is_empty());
        let normal = breaker
            .preview_shape_candidate(f32::MAX, LineTabOrigin::default())
            .unwrap();
        assert_eq!(normal.line.text_range, 0..6);
    }

    #[test]
    fn prefix_uses_visual_fragments_and_source_first_last_membership() {
        use crate::layout::{
            InlineOwnerShaping, InlineShapingEdge, PhysicalLineEdge, ShapingEdgePlacement,
        };

        let mut layout = build_layout_with_direction("ععع", [], Some(BaseDirection::Ltr));
        let (item, cluster) = layout
            .data
            .items
            .iter()
            .enumerate()
            .find_map(|(index, item)| {
                (item.kind == LayoutItemKind::TextRun && item.cluster_range.len() == 3)
                    .then_some((index, item.cluster_range.start + 2))
            })
            .unwrap();
        let end = SourceCursor::BeforeItem(layout.data.items.len());
        let owners = [InlineOwnerShaping {
            text: 0..6,
            inline_boxes: alloc::vec::Vec::new(),
            edges: alloc::vec![
                InlineShapingEdge {
                    side: PhysicalLineEdge::Left,
                    placement: ShapingEdgePlacement::FirstLine
                },
                InlineShapingEdge {
                    side: PhysicalLineEdge::Right,
                    placement: ShapingEdgePlacement::LastLine
                },
            ],
        }];
        let mut breaker = layout.break_lines();
        for (cursor, expected) in [
            (SourceCursor::WithinText { item, cluster }, alloc::vec![4]),
            (end, alloc::vec![0, 6]),
        ] {
            let prefix = breaker
                .preview_source_prefix(cursor, LineTabOrigin::ZERO)
                .unwrap();
            assert_eq!(
                crate::layout::physical_shaping::physical_shaping_boundaries(
                    &breaker.layout.data,
                    &prefix.items,
                    &owners,
                ),
                expected
            );
        }
        assert!(breaker.lines.lines.is_empty());
    }
}
