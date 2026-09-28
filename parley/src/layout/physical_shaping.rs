// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;
use core::ops::Range;

use super::{LayoutData, LayoutItemKind, Line, LineItemData, PhysicalLineEdge};
use crate::Brush;

/// The fragments on which a nonzero physical edge interrupts shaping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapingEdgePlacement {
    /// The outermost fragment on the owner's first line.
    FirstLine,
    /// The outermost fragment on the owner's last line.
    LastLine,
    /// Each visual fragment, including fragments induced by bidi reordering.
    EveryFragment,
}

/// A physical edge with at least one nonzero margin, border or padding component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InlineShapingEdge {
    /// Side of the visual fragment in the native inline axis.
    pub side: PhysicalLineEdge,
    /// Fragmentation policy for this edge.
    pub placement: ShapingEdgePlacement,
}

/// Source membership and physical shaping edges of a non-atomic inline owner.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InlineOwnerShaping {
    /// UTF-8 source range belonging to the owner and its descendants.
    pub text: Range<usize>,
    /// Owned inline-box IDs, including owner edges and descendant atomics.
    pub inline_boxes: Vec<u64>,
    /// Breaking physical edges. A zero total advance does not remove an edge.
    pub edges: Vec<InlineShapingEdge>,
}

#[derive(Clone, Copy)]
struct SourceEndpoint {
    item: usize,
    byte: Option<usize>,
}

impl SourceEndpoint {
    fn occurs_in(self, items: &[LineItemData]) -> bool {
        items.iter().any(|item| {
            item.layout_item_index == Some(self.item)
                && self.byte.is_none_or(|byte| item.text_range.contains(&byte))
        })
    }
}

#[derive(Clone, Copy)]
struct OwnerFragment {
    left: usize,
    right: usize,
}

impl OwnerFragment {
    fn boundary(self, side: PhysicalLineEdge) -> usize {
        match side {
            PhysicalLineEdge::Left => self.left,
            PhysicalLineEdge::Right => self.right,
        }
    }
}

fn intersection(left: &Range<usize>, right: &Range<usize>) -> Option<Range<usize>> {
    let range = left.start.max(right.start)..left.end.min(right.end);
    (range.start < range.end).then_some(range)
}

impl InlineOwnerShaping {
    fn source_endpoints<B: Brush>(&self, data: &LayoutData<B>) -> Option<[SourceEndpoint; 2]> {
        let mut endpoints = data.items.iter().enumerate().filter_map(|(index, item)| {
            let bytes = match item.kind {
                LayoutItemKind::TextRun => {
                    let owned = intersection(&self.text, &item.text_range)?;
                    [Some(owned.start), Some(owned.end - 1)]
                }
                LayoutItemKind::InlineBox => {
                    self.inline_boxes
                        .contains(&data.inline_boxes[item.index].id)
                        .then_some(())?;
                    [None, None]
                }
            };
            Some(bytes.map(|byte| SourceEndpoint { item: index, byte }))
        });
        let first = endpoints.next()?;
        let last = endpoints.next_back().unwrap_or(first);
        Some([first[0], last[1]])
    }

    fn visual_fragments<B: Brush>(
        &self,
        data: &LayoutData<B>,
        items: &[LineItemData],
    ) -> Vec<OwnerFragment> {
        let mut fragments = Vec::<OwnerFragment>::new();
        let mut continues = false;
        let mut append = |owned: Option<OwnerFragment>| match owned {
            Some(fragment) if continues => {
                fragments.last_mut().expect("preceding owner piece").right = fragment.right;
            }
            Some(fragment) => {
                fragments.push(fragment);
                continues = true;
            }
            None => continues = false,
        };
        for item in items {
            if item.kind == LayoutItemKind::InlineBox {
                let inline = &data.inline_boxes[item.index];
                if inline.letter_spacing().is_some() {
                    append(
                        self.inline_boxes
                            .contains(&inline.id)
                            .then_some(OwnerFragment {
                                left: inline.index,
                                right: inline.index,
                            }),
                    );
                }
                continue;
            }
            if item.text_range.is_empty() {
                continue;
            }
            let Some(owned) = intersection(&self.text, &item.text_range) else {
                append(None);
                continue;
            };
            let (before, piece, after) = if item.bidi_level & 1 == 0 {
                (
                    item.text_range.start < owned.start,
                    OwnerFragment {
                        left: owned.start,
                        right: owned.end,
                    },
                    owned.end < item.text_range.end,
                )
            } else {
                (
                    owned.end < item.text_range.end,
                    OwnerFragment {
                        left: owned.end,
                        right: owned.start,
                    },
                    item.text_range.start < owned.start,
                )
            };
            if before {
                append(None);
            }
            append(Some(piece));
            if after {
                append(None);
            }
        }
        fragments
    }
}

pub(super) fn physical_shaping_boundaries<B: Brush>(
    data: &LayoutData<B>,
    items: &[LineItemData],
    owners: &[InlineOwnerShaping],
) -> Vec<usize> {
    let mut boundaries = Vec::new();
    for owner in owners {
        let Some([first, last]) = owner.source_endpoints(data) else {
            continue;
        };
        let first_line = first.occurs_in(items);
        let last_line = last.occurs_in(items);
        let fragments = owner.visual_fragments(data, items);
        for edge in &owner.edges {
            let active = match edge.placement {
                ShapingEdgePlacement::FirstLine => first_line,
                ShapingEdgePlacement::LastLine => last_line,
                ShapingEdgePlacement::EveryFragment => true,
            };
            if !active {
                continue;
            }
            if fragments.is_empty() {
                if owner.text.is_empty() && (first_line || last_line) {
                    boundaries.push(owner.text.start);
                }
            } else if edge.placement == ShapingEdgePlacement::EveryFragment {
                boundaries.extend(
                    fragments
                        .iter()
                        .map(|fragment| fragment.boundary(edge.side)),
                );
            } else {
                let fragment = match edge.side {
                    PhysicalLineEdge::Left => fragments.first(),
                    PhysicalLineEdge::Right => fragments.last(),
                };
                boundaries.push(
                    fragment
                        .expect("nonempty owner fragments")
                        .boundary(edge.side),
                );
            }
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

impl<B: Brush> Line<'_, B> {
    /// Resolves physical owner edges to source shaping boundaries on this line.
    /// This query does not change the line's selected glyphs or geometry.
    pub fn physical_shaping_boundaries(&self, owners: &[InlineOwnerShaping]) -> Vec<usize> {
        physical_shaping_boundaries(
            &self.layout.data,
            &self.layout.data.line_items[self.data.item_range.clone()],
            owners,
        )
    }
}
