// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{collections::BTreeMap, vec::Vec};

use crate::{
    InlineBox,
    inline_box::{ClonedInlineOwner, InlineBoxShapingParticipation},
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

    pub(super) fn start_advance(&self) -> f32 {
        self.owners.iter().map(|owner| owner.start_width).sum()
    }

    pub(super) fn end_advance(&self) -> f32 {
        self.owners.iter().map(|owner| owner.end_width).sum()
    }

    pub(super) fn finish_intrinsic_fragment(
        &self,
        maximum: &mut f32,
        advance: &mut f32,
        trailing: f32,
    ) {
        *maximum = maximum.max(*advance - trailing + self.end_advance());
        *advance = self.start_advance();
    }

    pub(super) fn start_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.owners.iter().map(|owner| owner.start_id)
    }

    pub(super) fn end_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.owners.iter().rev().map(|owner| owner.end_id)
    }
}

pub(super) struct ClonedInlineEdgeMap(BTreeMap<u64, LineItemData>);

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
                    Some((edge.id, LineItemData::inline_box(item, None, edge.width())))
                })
                .collect(),
        )
    }

    pub(super) fn append(&self, ids: impl IntoIterator<Item = u64>, items: &mut Vec<LineItemData>) {
        items.extend(ids.into_iter().map(|id| {
            self.0
                .get(&id)
                .expect("a cloned owner retains both source edges")
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
    (item.layout_item_index.is_some() || inline_box.cloned_owner().is_none())
        && inline_box.shaping_participation() != InlineBoxShapingParticipation::TransparentBoundary
}
