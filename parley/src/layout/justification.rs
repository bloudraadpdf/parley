// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;
use core::ops::Range;

/// A source unit at an eligible expansion boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JustificationUnit {
    /// A typographic text unit, identified by its UTF-8 byte range.
    Text(Range<usize>),
    /// An atomic inline box, identified by its caller-supplied identifier.
    InlineBox(u64),
}

/// A caller-resolved expansion opportunity in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JustificationOpportunity {
    /// Expansion within a word separator.
    WordSeparator(Range<usize>),
    /// Expansion between typographic units.
    BetweenUnits {
        /// The preceding source unit.
        before: JustificationUnit,
        /// The following source unit.
        after: JustificationUnit,
    },
}

impl JustificationOpportunity {
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
}

impl JustificationOpportunities {
    pub(crate) fn set(&mut self, entries: Vec<JustificationOpportunity>) {
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
    }

    pub(crate) fn entries(&self) -> &[JustificationOpportunity] {
        &self.entries
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
        opportunities.set(vec![
            boundary(4..5, 5..6),
            JustificationOpportunity::WordSeparator(3..4),
            boundary(0..1, 1..3),
            JustificationOpportunity::BetweenUnits {
                before: JustificationUnit::Text(6..7),
                after: JustificationUnit::InlineBox(42),
            },
        ]);
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
        opportunities.set(vec![boundary(0..1, 1..2)]);
        opportunities.set(vec![boundary(4..5, 5..6)]);
        assert_eq!(opportunities.text_boundaries(0..2).count(), 0);
        assert_eq!(opportunities.text_boundaries(4..6).count(), 1);
        opportunities.clear();
        assert!(opportunities.entries().is_empty());
        assert_eq!(opportunities.text_boundaries(0..8).count(), 0);
    }
}
