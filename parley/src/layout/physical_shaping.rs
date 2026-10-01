// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{vec, vec::Vec};
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

/// A position among the source items: a layout item, with a byte of a text item.
pub(crate) type SourceKey = (usize, Option<usize>);

#[derive(Clone, Copy)]
struct SourceEndpoint {
    item: usize,
    byte: Option<usize>,
}

impl SourceEndpoint {
    fn key(self) -> SourceKey {
        (self.item, self.byte)
    }

    fn occurs_in(self, items: &[LineItemData], members: &[usize]) -> bool {
        members.iter().any(|position| {
            let item = &items[*position];
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

/// A line item that ends the visual fragment of each owner that does not own it.
fn separates_fragments<B: Brush>(data: &LayoutData<B>, item: &LineItemData) -> bool {
    match item.kind {
        LayoutItemKind::TextRun => !item.text_range.is_empty(),
        LayoutItemKind::InlineBox => data.inline_boxes[item.index].letter_spacing().is_some(),
    }
}

impl InlineOwnerShaping {
    /// The first and last owned source items. `text_items` are the text runs with source, in source order, and
    /// `box_items` the inline boxes by identity.
    fn source_endpoints<B: Brush>(
        &self,
        data: &LayoutData<B>,
        text_items: &[usize],
        box_items: &[(u64, usize)],
    ) -> Option<[SourceEndpoint; 2]> {
        let source = |index: &usize| &data.items[*index].text_range;
        let texts = &text_items[text_items
            .partition_point(|index| source(index).end <= self.text.start)
            ..text_items.partition_point(|index| source(index).start < self.text.end)];
        let owned_text = [texts.first(), texts.last()]
            .into_iter()
            .flatten()
            .filter_map(|index| {
                let owned = intersection(&self.text, source(index))?;
                Some([owned.start, owned.end - 1].map(|byte| SourceEndpoint {
                    item: *index,
                    byte: Some(byte),
                }))
            });
        let owned_boxes = self.inline_boxes.iter().flat_map(|id| {
            box_items[box_items.partition_point(|(other, _)| other < id)..]
                .iter()
                .take_while(move |(other, _)| other == id)
                .map(|(_, index)| {
                    [SourceEndpoint {
                        item: *index,
                        byte: None,
                    }; 2]
                })
        });
        let owned = owned_text.chain(owned_boxes);
        let first = owned.clone().min_by_key(|[first, _]| first.item)?;
        let last = owned.max_by_key(|[_, last]| last.item)?;
        Some([first[0], last[1]])
    }
}

/// An owner with edges and source items.
struct IndexedOwner {
    text: Range<usize>,
    edges: Vec<InlineShapingEdge>,
    endpoints: [SourceEndpoint; 2],
    /// A last-line edge on the start side of the first source item.
    last_line_start: bool,
}

impl IndexedOwner {
    /// The visual fragments of the owner on a line, from the `members` of the line: the positions of the items
    /// between the owner's endpoints and of the cloned edges that it owns, in order. `separators` counts the items
    /// before each position that separate fragments.
    fn visual_fragments<B: Brush>(
        &self,
        data: &LayoutData<B>,
        items: &[LineItemData],
        members: &[usize],
        separators: &[usize],
        owns: impl Fn(u64) -> bool,
    ) -> Vec<OwnerFragment> {
        let mut fragments = Vec::<OwnerFragment>::new();
        let mut open = None;
        for position in members.iter().copied() {
            let item = &items[position];
            let (piece, before, after) = match item.kind {
                LayoutItemKind::InlineBox => {
                    let inline = &data.inline_boxes[item.index];
                    if inline.letter_spacing().is_none() || !owns(inline.id) {
                        continue;
                    }
                    let piece = OwnerFragment {
                        left: inline.index,
                        right: inline.index,
                    };
                    (piece, false, false)
                }
                LayoutItemKind::TextRun => {
                    let Some(owned) = intersection(&self.text, &item.text_range) else {
                        continue;
                    };
                    let leading = item.text_range.start < owned.start;
                    let trailing = owned.end < item.text_range.end;
                    if item.bidi_level & 1 == 0 {
                        let piece = OwnerFragment {
                            left: owned.start,
                            right: owned.end,
                        };
                        (piece, leading, trailing)
                    } else {
                        let piece = OwnerFragment {
                            left: owned.end,
                            right: owned.start,
                        };
                        (piece, trailing, leading)
                    }
                }
            };
            if !before && open.is_some_and(|open| separators[open] == separators[position]) {
                fragments.last_mut().expect("preceding owner piece").right = piece.right;
            } else {
                fragments.push(piece);
            }
            open = (!after).then_some(position + 1);
        }
        fragments
    }

    /// Pushes the boundaries of the owner on a line to `boundaries`.
    fn boundaries<B: Brush>(
        &self,
        data: &LayoutData<B>,
        items: &[LineItemData],
        members: &[usize],
        separators: &[usize],
        owns: impl Fn(u64) -> bool,
        boundaries: &mut Vec<usize>,
    ) {
        let [first, last] = self.endpoints;
        let first_line = first.occurs_in(items, members);
        let last_line = last.occurs_in(items, members);
        let fragments = self.visual_fragments(data, items, members, separators, owns);
        for edge in &self.edges {
            let active = match edge.placement {
                ShapingEdgePlacement::FirstLine => first_line,
                ShapingEdgePlacement::LastLine => last_line,
                ShapingEdgePlacement::EveryFragment => true,
            };
            if !active {
                continue;
            }
            if fragments.is_empty() {
                if self.text.is_empty() && (first_line || last_line) {
                    boundaries.push(self.text.start);
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
}

/// The owners with edges and source items, in the order of their first endpoints. An owner gives boundaries on a
/// line only when a line item is between its endpoints or is a cloned edge that it owns.
pub(crate) struct OwnerIndex {
    owners: Vec<IndexedOwner>,
    /// All runs have one bidi level, which is their paragraph level.
    uniform_level: bool,
    /// A segment tree over `owners`: the last endpoint that the owners of each node reach.
    reach: Vec<SourceKey>,
    /// The owned inline-box identities, with their owners.
    boxes: Vec<(u64, usize)>,
    #[cfg(test)]
    visits: core::cell::Cell<usize>,
}

impl OwnerIndex {
    pub(crate) fn new<B: Brush>(data: &LayoutData<B>, owners: &[InlineOwnerShaping]) -> Self {
        let text_items = (0..data.items.len())
            .filter(|index| {
                let item = &data.items[*index];
                item.kind == LayoutItemKind::TextRun && !item.text_range.is_empty()
            })
            .collect::<Vec<_>>();
        let mut box_items = data
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind == LayoutItemKind::InlineBox)
            .map(|(index, item)| (data.inline_boxes[item.index].id, index))
            .collect::<Vec<_>>();
        box_items.sort_unstable();
        let mut indexed = owners
            .iter()
            .filter(|owner| !owner.edges.is_empty())
            .filter_map(|owner| {
                let endpoints = owner.source_endpoints(data, &text_items, &box_items)?;
                let start = if data.items[endpoints[0].item].bidi_level & 1 == 0 {
                    PhysicalLineEdge::Left
                } else {
                    PhysicalLineEdge::Right
                };
                let indexed = IndexedOwner {
                    text: owner.text.clone(),
                    edges: owner.edges.clone(),
                    endpoints,
                    last_line_start: owner.edges.iter().any(|edge| {
                        edge.placement == ShapingEdgePlacement::LastLine && edge.side == start
                    }),
                };
                Some((indexed, &owner.inline_boxes))
            })
            .collect::<Vec<_>>();
        indexed.sort_unstable_by_key(|(owner, _)| owner.endpoints[0].key());
        let mut boxes = indexed
            .iter()
            .enumerate()
            .flat_map(|(owner, (_, ids))| ids.iter().map(move |id| (*id, owner)))
            .collect::<Vec<_>>();
        boxes.sort_unstable();
        boxes.dedup();
        let owners = indexed
            .into_iter()
            .map(|(owner, _)| owner)
            .collect::<Vec<_>>();
        let size = owners.len().next_power_of_two();
        let mut reach = vec![(0, None); 2 * size];
        for (leaf, owner) in owners.iter().enumerate() {
            reach[size + leaf] = owner.endpoints[1].key();
        }
        for node in (1..size).rev() {
            reach[node] = reach[2 * node].max(reach[2 * node + 1]);
        }
        Self {
            owners,
            uniform_level: data.runs.first().is_some_and(|first| {
                data.runs.iter().all(|run| {
                    run.bidi_level == first.paragraph_level
                        && run.paragraph_level == first.paragraph_level
                })
            }),
            reach,
            boxes,
            #[cfg(test)]
            visits: core::cell::Cell::new(0),
        }
    }

    /// Whether a line that ends at `cut`, the source position `offset`, has the boundaries of each longer line before
    /// `offset`. The items of a longer line keep their visual order before `cut` when all runs have one bidi level.
    /// An owner across `cut` changes only its fragment at `cut`, when its text reaches `offset` and it has no
    /// last-line edge on the start side.
    pub(crate) fn keeps_boundaries_before(&self, cut: SourceKey, offset: usize) -> bool {
        if !self.uniform_level {
            return false;
        }
        let mut across = Vec::new();
        self.reaching([cut, cut], 1, 0..self.reach.len() / 2, &mut across);
        across.into_iter().all(|index| {
            let owner = &self.owners[index];
            owner.endpoints[0].key() == cut || (owner.text.end >= offset && !owner.last_line_start)
        })
    }

    #[cfg(test)]
    pub(crate) fn visits(&self) -> usize {
        self.visits.get()
    }

    /// Pushes the owners in `node` of the segment tree, which covers `range` of `owners`, that reach into `span`.
    fn reaching(
        &self,
        span: [SourceKey; 2],
        node: usize,
        range: Range<usize>,
        owners: &mut Vec<usize>,
    ) {
        if range.start >= self.owners.len()
            || self.reach[node] < span[0]
            || self.owners[range.start].endpoints[0].key() > span[1]
        {
            return;
        }
        if range.len() == 1 {
            owners.push(range.start);
            return;
        }
        let middle = range.start + range.len() / 2;
        self.reaching(span, 2 * node, range.start..middle, owners);
        self.reaching(span, 2 * node + 1, middle..range.end, owners);
    }

    fn owns(&self, owner: usize, id: u64) -> bool {
        self.boxes.binary_search(&(id, owner)).is_ok()
    }

    fn owners_of(&self, id: u64) -> impl Iterator<Item = usize> + '_ {
        self.boxes[self.boxes.partition_point(|(other, _)| *other < id)..]
            .iter()
            .take_while(move |(other, _)| *other == id)
            .map(|(_, owner)| *owner)
    }

    /// The physical shaping boundaries of the line with `items`.
    pub(crate) fn boundaries<B: Brush>(
        &self,
        data: &LayoutData<B>,
        items: &[LineItemData],
    ) -> Vec<usize> {
        let mut sources = items
            .iter()
            .enumerate()
            .filter_map(|(position, item)| Some((item.layout_item_index?, position)))
            .collect::<Vec<_>>();
        sources.sort_unstable();
        let clones = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.layout_item_index.is_none() && item.kind == LayoutItemKind::InlineBox
            })
            .map(|(position, item)| (position, data.inline_boxes[item.index].id))
            .collect::<Vec<_>>();
        let separators = core::iter::once(0)
            .chain(items.iter().scan(0, |count, item| {
                *count += usize::from(separates_fragments(data, item));
                Some(*count)
            }))
            .collect::<Vec<_>>();
        let span = items
            .iter()
            .filter_map(|item| {
                let index = item.layout_item_index?;
                match item.kind {
                    LayoutItemKind::InlineBox => Some([(index, None); 2]),
                    LayoutItemKind::TextRun => (!item.text_range.is_empty()).then(|| {
                        [
                            (index, Some(item.text_range.start)),
                            (index, Some(item.text_range.end - 1)),
                        ]
                    }),
                }
            })
            .reduce(|[start, end], [first, last]| [start.min(first), end.max(last)]);
        let mut candidates = Vec::new();
        if let Some(span) = span {
            self.reaching(span, 1, 0..self.reach.len() / 2, &mut candidates);
        }
        for (_, id) in &clones {
            candidates.extend(self.owners_of(*id));
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut boundaries = Vec::new();
        for index in candidates {
            let owner = &self.owners[index];
            let [first, last] = owner.endpoints;
            let mut members = sources[sources.partition_point(|(item, _)| *item < first.item)
                ..sources.partition_point(|(item, _)| *item <= last.item)]
                .iter()
                .map(|(_, position)| *position)
                .chain(
                    clones
                        .iter()
                        .filter(|(_, id)| self.owns(index, *id))
                        .map(|(position, _)| *position),
                )
                .collect::<Vec<_>>();
            members.sort_unstable();
            #[cfg(test)]
            self.visits.set(self.visits.get() + 1 + members.len());
            owner.boundaries(
                data,
                items,
                &members,
                &separators,
                |id| self.owns(index, id),
                &mut boundaries,
            );
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        boundaries
    }
}

pub(super) fn physical_shaping_boundaries<B: Brush>(
    data: &LayoutData<B>,
    items: &[LineItemData],
    owners: &[InlineOwnerShaping],
) -> Vec<usize> {
    OwnerIndex::new(data, owners).boundaries(data, items)
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
