// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;
use core::ops::Range;

use super::{Layout, LayoutItemKind};
use crate::style::Brush;

/// An exact resolved Unicode bidi level.
///
/// Values are minted by Parley's bidi resolver. Consumers can compare levels
/// and inspect directionality without constructing raw, unvalidated levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BidiLevel(u8);

impl BidiLevel {
    /// Returns whether this level has right-to-left directionality.
    pub const fn is_rtl(self) -> bool {
        self.0 & 1 != 0
    }
}

/// Stable identity of one item in the pre-line-break paragraph topology.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BidiAtomId(usize);

/// Source identity owned by a visual bidi atom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BidiVisualAtomKind {
    /// A source text range.
    Text(Range<usize>),
    /// An inline box at a source boundary.
    InlineBox {
        /// Caller-supplied inline-box identity.
        id: u64,
        /// UTF-8 byte boundary at which the box participates.
        source_boundary: usize,
    },
}

/// One atom in paragraph-wide visual order before line breaking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BidiVisualAtom {
    id: BidiAtomId,
    level: BidiLevel,
    kind: BidiVisualAtomKind,
}

impl BidiVisualAtom {
    pub const fn id(&self) -> BidiAtomId {
        self.id
    }

    pub const fn level(&self) -> BidiLevel {
        self.level
    }

    pub const fn kind(&self) -> &BidiVisualAtomKind {
        &self.kind
    }
}

/// Paragraph-wide visual item topology resolved before line breaking.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BidiTopology(Vec<BidiVisualAtom>);

impl BidiTopology {
    pub fn atoms(&self) -> &[BidiVisualAtom] {
        &self.0
    }
}

impl<B: Brush> Layout<B> {
    /// Returns the stable infinitely-long-line visual topology.
    ///
    /// The result is independent of line breaking and alignment. Atom IDs map
    /// positioned runs back to this topology through [`crate::layout::Run::bidi_atom_id`].
    pub fn bidi_topology(&self) -> BidiTopology {
        let mut visual_indices: Vec<usize> = (0..self.data.items.len()).collect();
        reorder_by_level(&mut visual_indices, |index| {
            self.data.items[index].bidi_level
        });
        BidiTopology(
            visual_indices
                .into_iter()
                .map(|index| {
                    let item = &self.data.items[index];
                    let kind = match item.kind {
                        LayoutItemKind::TextRun => {
                            BidiVisualAtomKind::Text(self.data.runs[item.index].text_range.clone())
                        }
                        LayoutItemKind::InlineBox => {
                            let inline_box = &self.data.inline_boxes[item.index];
                            BidiVisualAtomKind::InlineBox {
                                id: inline_box.id,
                                source_boundary: inline_box.index,
                            }
                        }
                    };
                    BidiVisualAtom {
                        id: BidiAtomId(index),
                        level: BidiLevel(item.bidi_level),
                        kind,
                    }
                })
                .collect(),
        )
    }

    pub(crate) fn bidi_atom_id_for_run(&self, run_index: usize) -> Option<BidiAtomId> {
        self.data
            .items
            .iter()
            .enumerate()
            .find_map(|(index, item)| {
                (item.kind == LayoutItemKind::TextRun && item.index == run_index)
                    .then_some(BidiAtomId(index))
            })
    }
}

fn reorder_by_level(indices: &mut [usize], mut level_at: impl FnMut(usize) -> u8) {
    let Some(max_level) = indices.iter().map(|&index| level_at(index)).max() else {
        return;
    };
    let Some(lowest_odd) = indices
        .iter()
        .map(|&index| level_at(index))
        .filter(|level| level & 1 != 0)
        .min()
    else {
        return;
    };
    for level in (lowest_odd..=max_level).rev() {
        let mut start = 0;
        while start < indices.len() {
            if level_at(indices[start]) < level {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < indices.len() && level_at(indices[end]) >= level {
                end += 1;
            }
            indices[start..end].reverse();
            start = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::reorder_by_level;

    #[test]
    fn exact_levels_reorder_even_nested_runs() {
        let levels = [0, 1, 2];
        let mut indices = [0, 1, 2];
        reorder_by_level(&mut indices, |index| levels[index]);
        assert_eq!(indices, [0, 2, 1]);
    }
}
