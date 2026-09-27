// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;
use core::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum JustificationUnitAddress {
    TextCluster(usize),
    InlineBox(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct JustificationSideSpacing {
    pub(crate) leading: f32,
    pub(crate) trailing: f32,
    pub(crate) applied_after: f32,
}

/// A source unit at an eligible expansion boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JustificationUnit {
    /// A typographic text unit, identified by its UTF-8 byte range.
    Text(Range<usize>),
    /// An atomic inline box, identified by its caller-supplied identifier.
    InlineBox(u64),
}

/// A caller-resolved expansion opportunity identified by source units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JustificationOpportunity {
    /// Expansion within a word separator.
    WordSeparator(Range<usize>),
    /// Expansion between adjacent visual typographic units.
    BetweenUnits {
        /// The unit on the physical left side of the opportunity.
        before: JustificationUnit,
        /// The unit on the physical right side of the opportunity.
        after: JustificationUnit,
    },
}

impl JustificationOpportunity {
    fn source_anchor(&self, boxes: &alloc::collections::BTreeMap<u64, usize>) -> Option<usize> {
        let anchor = |unit: &JustificationUnit| match unit {
            JustificationUnit::Text(range) => Some(range.start),
            JustificationUnit::InlineBox(id) => boxes.get(id).copied(),
        };
        match self {
            Self::WordSeparator(range) => Some(range.start),
            Self::BetweenUnits { before, after } => Some(anchor(before)?.min(anchor(after)?)),
        }
    }

    fn text_boundary(&self) -> Option<(&Range<usize>, &Range<usize>)> {
        match self {
            Self::BetweenUnits {
                before: JustificationUnit::Text(before),
                after: JustificationUnit::Text(after),
            } => Some((before, after)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct JustificationOpportunities {
    entries: Vec<JustificationOpportunity>,
    text_boundaries: Vec<usize>,
    source_order: Vec<(usize, usize)>,
}

impl JustificationOpportunities {
    pub(crate) fn set(
        &mut self,
        entries: Vec<JustificationOpportunity>,
        inline_boxes: &[crate::InlineBox],
    ) {
        let boxes = inline_boxes
            .iter()
            .map(|inline| (inline.id, inline.index))
            .collect();
        self.entries = entries;
        self.text_boundaries.clear();
        self.text_boundaries.extend(
            self.entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| entry.text_boundary().map(|_| index)),
        );
        self.text_boundaries.sort_by_key(|&index| {
            let (before, after) = self.entries[index].text_boundary().unwrap();
            (before.start.min(after.start), before.end.max(after.end))
        });
        self.source_order.clear();
        self.source_order.extend(
            self.entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    entry.source_anchor(&boxes).map(|anchor| (anchor, index))
                }),
        );
        self.source_order.sort_unstable();
    }

    pub(crate) fn entries(&self) -> &[JustificationOpportunity] {
        &self.entries
    }

    pub(crate) fn line_candidates(
        &self,
        source: Range<usize>,
    ) -> impl Iterator<Item = &JustificationOpportunity> {
        let start = self
            .source_order
            .partition_point(|&(anchor, _)| anchor < source.start);
        self.source_order[start..]
            .iter()
            .take_while(move |&&(anchor, _)| anchor <= source.end)
            .map(|&(_, index)| &self.entries[index])
    }

    pub(crate) fn text_boundaries(
        &self,
        source: Range<usize>,
    ) -> impl Iterator<Item = (&Range<usize>, &Range<usize>)> {
        let start = self.text_boundaries.partition_point(|&index| {
            let (before, after) = self.entries[index].text_boundary().unwrap();
            before.start.min(after.start) < source.start
        });
        self.text_boundaries[start..]
            .iter()
            .map(|&index| self.entries[index].text_boundary().unwrap())
            .take_while(move |(before, after)| before.start.min(after.start) < source.end)
            .filter(move |(before, after)| before.end.max(after.end) <= source.end)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.text_boundaries.clear();
        self.source_order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn boundary(before: Range<usize>, after: Range<usize>) -> JustificationOpportunity {
        JustificationOpportunity::BetweenUnits {
            before: JustificationUnit::Text(before),
            after: JustificationUnit::Text(after),
        }
    }

    #[test]
    fn candidate_contains_both_endpoints_of_an_eligible_boundary() {
        let mut opportunities = JustificationOpportunities::default();
        opportunities.set(
            vec![
                boundary(4..5, 5..6),
                JustificationOpportunity::WordSeparator(3..4),
                boundary(0..1, 1..3),
                JustificationOpportunity::BetweenUnits {
                    before: JustificationUnit::Text(6..7),
                    after: JustificationUnit::InlineBox(42),
                },
            ],
            &[],
        );
        assert_eq!(
            opportunities.text_boundaries(0..6).collect::<Vec<_>>(),
            vec![(&(0..1), &(1..3)), (&(4..5), &(5..6))]
        );
        assert_eq!(opportunities.text_boundaries(1..5).count(), 0);
        assert_eq!(opportunities.text_boundaries(0..3).count(), 1);
        assert_eq!(opportunities.text_boundaries(4..6).count(), 1);
        assert_eq!(opportunities.text_boundaries(0..2).count(), 0);
        assert_eq!(opportunities.entries().len(), 4);
    }

    #[test]
    fn replacing_and_clearing_policy_removes_previous_boundaries() {
        let mut opportunities = JustificationOpportunities::default();
        opportunities.set(vec![boundary(0..1, 1..2)], &[]);
        opportunities.set(vec![boundary(4..5, 5..6)], &[]);
        assert_eq!(opportunities.text_boundaries(0..2).count(), 0);
        assert_eq!(opportunities.text_boundaries(4..6).count(), 1);
        opportunities.clear();
        assert!(opportunities.entries().is_empty());
        assert_eq!(opportunities.text_boundaries(0..8).count(), 0);
    }

    #[test]
    fn many_line_candidate_queries_visit_each_source_opportunity_once() {
        let mut opportunities = JustificationOpportunities::default();
        opportunities.set(
            (0..4096)
                .map(|index| JustificationOpportunity::WordSeparator(index * 4 + 1..index * 4 + 2))
                .collect(),
            &[],
        );
        let mut visited = 0;
        for index in 0..4096 {
            let entries = opportunities
                .line_candidates(index * 4..index * 4 + 4)
                .collect::<Vec<_>>();
            assert_eq!(
                entries,
                vec![&JustificationOpportunity::WordSeparator(
                    index * 4 + 1..index * 4 + 2
                )]
            );
            visited += entries.len();
        }
        assert_eq!(visited, opportunities.entries().len());
    }

    #[test]
    fn atomic_endpoints_are_indexed_at_their_registered_source_boundary() {
        let entry = JustificationOpportunity::BetweenUnits {
            before: JustificationUnit::InlineBox(1),
            after: JustificationUnit::InlineBox(2),
        };
        let mut opportunities = JustificationOpportunities::default();
        opportunities.set(
            vec![entry.clone()],
            &[
                crate::InlineBox::new(1, 0, 10.0, 10.0),
                crate::InlineBox::new(2, 0, 10.0, 10.0),
            ],
        );
        assert_eq!(
            opportunities.line_candidates(0..0).collect::<Vec<_>>(),
            vec![&entry]
        );
    }
}
