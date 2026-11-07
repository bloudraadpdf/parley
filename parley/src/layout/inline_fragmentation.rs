// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{collections::BTreeMap, vec::Vec};

use crate::{
    InlineBox,
    inline_box::{
        ClonedInlineOwner, InlineBoxLineBreakParticipation, InlineBoxShapingParticipation,
    },
    style::Brush,
};

use super::{LayoutData, LayoutItemKind, LineItemData};

#[derive(Clone, Default)]
pub(super) struct ClonedInlineFlow {
    owners: Vec<ClonedInlineOwner>,
}

impl ClonedInlineFlow {
    pub(super) fn before_edge(&mut self, edge: &InlineBox) {
        if let Some(owner) = edge.cloned_owner() {
            if edge.id == owner.end_id {
                assert_eq!(
                    self.owners.pop(),
                    Some(owner),
                    "cloned inline owners are nested"
                );
            }
        }
    }

    pub(super) fn after_edge(&mut self, edge: &InlineBox) {
        if let Some(owner) = edge.cloned_owner() {
            if edge.id == owner.start_id {
                self.owners.push(owner);
            }
        }
    }

    pub(super) fn start_advance(&self, line_index: usize) -> f32 {
        self.owners
            .iter()
            .map(|owner| owner.start_width.for_line(line_index))
            .sum()
    }

    pub(super) fn end_advance(&self, line_index: usize) -> f32 {
        self.owners
            .iter()
            .map(|owner| owner.end_width.for_line(line_index))
            .sum()
    }

    pub(super) fn finish_intrinsic_fragment(
        &self,
        maximum: &mut f64,
        advance: &mut f64,
        trailing: f64,
        line_index: usize,
    ) {
        *maximum = maximum.max(*advance - trailing + f64::from(self.end_advance(line_index)));
        *advance = f64::from(self.start_advance(line_index + 1));
    }

    pub(super) fn start_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.owners.iter().map(|owner| owner.start_id)
    }

    pub(super) fn end_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.owners.iter().rev().map(|owner| owner.end_id)
    }
}

pub(super) struct ClonedInlineEdgeMap(BTreeMap<u64, [LineItemData; 2]>);

impl ClonedInlineEdgeMap {
    pub(super) fn new<B: Brush>(layout: &LayoutData<B>) -> Self {
        Self(
            layout
                .items
                .iter()
                .filter_map(|item| {
                    if item.kind != LayoutItemKind::InlineBox {
                        return None;
                    }
                    let edge = &layout.inline_boxes[item.index];
                    edge.cloned_owner()?;
                    Some((
                        edge.id,
                        [
                            LineItemData::inline_box(item, None, edge.width_for_line(0)),
                            LineItemData::inline_box(item, None, edge.width_for_line(1)),
                        ],
                    ))
                })
                .collect(),
        )
    }

    pub(super) fn append(
        &self,
        ids: impl IntoIterator<Item = u64>,
        items: &mut Vec<LineItemData>,
        line_index: usize,
    ) {
        items.extend(ids.into_iter().map(|id| {
            self.0
                .get(&id)
                .expect("a cloned owner retains both source edges")[usize::from(line_index != 0)]
            .clone()
        }));
    }
}

pub(super) fn blocks_source_whitespace<B: Brush>(
    item: &LineItemData,
    layout: &LayoutData<B>,
) -> bool {
    if item.kind != LayoutItemKind::InlineBox {
        return false;
    }
    let inline_box = &layout.inline_boxes[item.index];
    // Inline decorations delimit an owner, not source text: terminal spaces
    // still collapse before an original border/padding edge (CSS Text 3 §4.1.2).
    if matches!(
        inline_box.line_break_participation(),
        InlineBoxLineBreakParticipation::LogicalOwnerEdge(_)
    ) {
        return false;
    }
    (item.layout_item_index.is_some() || inline_box.cloned_owner().is_none())
        && inline_box.shaping_participation() != InlineBoxShapingParticipation::TransparentBoundary
}
