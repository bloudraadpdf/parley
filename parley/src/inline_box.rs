// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// The soft-wrap relationship between an inline box and its neighbours.
#[derive(PartialEq, Eq, Debug, Clone, Copy, Default)]
pub enum InlineBoxBreakAffinity {
    /// Soft-wrap opportunities on both sides remain available.
    #[default]
    Independent,
    /// The box stays with the preceding content.
    ToPrevious,
    /// The box stays with the following content.
    ToNext,
    /// The box stays with content on both sides.
    Both,
}

impl InlineBoxBreakAffinity {
    pub(crate) const fn allows_break_before(self) -> bool {
        matches!(self, Self::Independent | Self::ToNext)
    }

    pub(crate) const fn allows_break_after(self) -> bool {
        matches!(self, Self::Independent | Self::ToPrevious)
    }
}

/// A box to be laid out inline with text
#[derive(PartialEq, Debug, Clone)]
pub struct InlineBox {
    /// User-specified identifier for the box, which can be used by the user to determine which box in
    /// parley's output corresponds to which box in its input.
    pub id: u64,
    /// The byte offset into the underlying text string at which the box should be placed.
    /// This must not be within a Unicode code point.
    pub index: usize,
    /// The width of the box in pixels
    pub width: f32,
    /// The height of the box in pixels
    pub height: f32,
    /// The soft-wrap relationship with adjacent content.
    pub break_affinity: InlineBoxBreakAffinity,
}

impl InlineBox {
    /// A regular (non-glued) inline box.
    pub fn new(id: u64, index: usize, width: f32, height: f32) -> Self {
        Self {
            id,
            index,
            width,
            height,
            break_affinity: InlineBoxBreakAffinity::Independent,
        }
    }
}
