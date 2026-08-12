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

/// The layout participation of an inline box.
#[derive(PartialEq, Debug, Clone, Copy)]
enum InlineBoxParticipation {
    /// An ordinary atomic inline with geometry and soft-wrap affinity.
    Atomic {
        width: f32,
        height: f32,
        break_affinity: InlineBoxBreakAffinity,
    },
    /// A positioned anchor which is transparent to line breaking and sizing.
    TransparentAnchor,
}

impl InlineBoxParticipation {
    pub(crate) const fn width(self) -> f32 {
        match self {
            Self::Atomic { width, .. } => width,
            Self::TransparentAnchor => 0.0,
        }
    }

    pub(crate) const fn height(self) -> f32 {
        match self {
            Self::Atomic { height, .. } => height,
            Self::TransparentAnchor => 0.0,
        }
    }

    pub(crate) const fn break_affinity(self) -> Option<InlineBoxBreakAffinity> {
        match self {
            Self::Atomic { break_affinity, .. } => Some(break_affinity),
            Self::TransparentAnchor => None,
        }
    }
}

/// A box to be laid out inline with text.
#[derive(PartialEq, Debug, Clone)]
pub struct InlineBox {
    /// User-specified identifier for the box, which can be used by the user to determine which box in
    /// parley's output corresponds to which box in its input.
    pub id: u64,
    /// The byte offset into the underlying text string at which the box should be placed.
    /// This must not be within a Unicode code point.
    pub index: usize,
    /// The box's closed layout participation.
    participation: InlineBoxParticipation,
}

impl InlineBox {
    /// A regular (non-glued) inline box.
    pub fn new(id: u64, index: usize, width: f32, height: f32) -> Self {
        Self {
            id,
            index,
            participation: InlineBoxParticipation::Atomic {
                width,
                height,
                break_affinity: InlineBoxBreakAffinity::Independent,
            },
        }
    }

    /// A positioned anchor which does not participate in line breaking or sizing.
    pub fn transparent_anchor(id: u64, index: usize) -> Self {
        Self {
            id,
            index,
            participation: InlineBoxParticipation::TransparentAnchor,
        }
    }

    /// An atomic inline box with an explicit soft-wrap affinity.
    pub fn atomic_with_break_affinity(
        id: u64,
        index: usize,
        width: f32,
        height: f32,
        break_affinity: InlineBoxBreakAffinity,
    ) -> Self {
        Self {
            id,
            index,
            participation: InlineBoxParticipation::Atomic {
                width,
                height,
                break_affinity,
            },
        }
    }

    /// Returns the inline advance in pixels.
    pub const fn width(&self) -> f32 {
        self.participation.width()
    }

    /// Returns the block extent in pixels.
    pub const fn height(&self) -> f32 {
        self.participation.height()
    }

    pub(crate) const fn break_affinity(&self) -> Option<InlineBoxBreakAffinity> {
        self.participation.break_affinity()
    }

    pub(crate) const fn is_transparent_anchor(&self) -> bool {
        matches!(
            self.participation,
            InlineBoxParticipation::TransparentAnchor
        )
    }
}
