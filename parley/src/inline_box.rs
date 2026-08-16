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

/// The advance participation of a collapsible source space after an inline-end
/// edge when the edge projects that source break.
#[derive(PartialEq, Eq, Debug, Clone, Copy, Default)]
pub enum FollowingSourceSpace {
    /// The source space keeps its shaped advance.
    #[default]
    RetainedAdvance,
    /// The source space collapses when the projected break is selected.
    CollapsedAfterProjectedBreak,
    /// Unicode line breaking retains authority over the source space.
    UnicodeBoundary,
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
    /// A logical owner edge which contributes geometry but no soft-wrap
    /// opportunity of its own.
    LogicalOwnerStart { width: f32, height: f32 },
    /// A logical inline-end edge with explicit following source-space
    /// participation.
    LogicalOwnerEnd {
        width: f32,
        height: f32,
        following_source_space: FollowingSourceSpace,
    },
    /// A positioned anchor which is transparent to line breaking and sizing.
    TransparentAnchor,
}

/// Closed line-breaking participation for inline boxes.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum InlineBoxLineBreakParticipation {
    Atomic(InlineBoxBreakAffinity),
    LogicalOwnerEdge(LogicalInlineEdge),
    TransparentAnchor,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum InlineBoxShapingParticipation {
    InterveningInlineAdvance,
    TransparentBoundary,
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub(crate) enum InlineBoxLineMetricParticipation {
    AtomicBlockExtent(f32),
    BoundaryOnly,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum LogicalInlineEdge {
    Start,
    End(FollowingSourceSpace),
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum LogicalInlineEdgeSourceProjection {
    Absent,
    BeforeGeometry,
    AfterGeometry,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum InlineBoxBidiAttachment {
    Independent,
    ToPrevious,
    ToNext,
}

impl InlineBoxParticipation {
    pub(crate) const fn width(self) -> f32 {
        match self {
            Self::Atomic { width, .. }
            | Self::LogicalOwnerStart { width, .. }
            | Self::LogicalOwnerEnd { width, .. } => width,
            Self::TransparentAnchor => 0.0,
        }
    }

    pub(crate) const fn height(self) -> f32 {
        match self {
            Self::Atomic { height, .. }
            | Self::LogicalOwnerStart { height, .. }
            | Self::LogicalOwnerEnd { height, .. } => height,
            Self::TransparentAnchor => 0.0,
        }
    }

    pub(crate) const fn line_break_participation(self) -> InlineBoxLineBreakParticipation {
        match self {
            Self::Atomic { break_affinity, .. } => {
                InlineBoxLineBreakParticipation::Atomic(break_affinity)
            }
            Self::LogicalOwnerStart { .. } => {
                InlineBoxLineBreakParticipation::LogicalOwnerEdge(LogicalInlineEdge::Start)
            }
            Self::LogicalOwnerEnd {
                following_source_space,
                ..
            } => InlineBoxLineBreakParticipation::LogicalOwnerEdge(LogicalInlineEdge::End(
                following_source_space,
            )),
            Self::TransparentAnchor => InlineBoxLineBreakParticipation::TransparentAnchor,
        }
    }

    pub(crate) const fn shaping_participation(self) -> InlineBoxShapingParticipation {
        match self {
            Self::Atomic { .. } => InlineBoxShapingParticipation::InterveningInlineAdvance,
            Self::LogicalOwnerStart { width, .. } | Self::LogicalOwnerEnd { width, .. } => {
                if width == 0.0 {
                    InlineBoxShapingParticipation::TransparentBoundary
                } else {
                    InlineBoxShapingParticipation::InterveningInlineAdvance
                }
            }
            Self::TransparentAnchor => InlineBoxShapingParticipation::TransparentBoundary,
        }
    }

    pub(crate) const fn line_metric_participation(self) -> InlineBoxLineMetricParticipation {
        match self {
            Self::Atomic { height, .. } => {
                InlineBoxLineMetricParticipation::AtomicBlockExtent(height)
            }
            Self::LogicalOwnerStart { .. }
            | Self::LogicalOwnerEnd { .. }
            | Self::TransparentAnchor => InlineBoxLineMetricParticipation::BoundaryOnly,
        }
    }
}

impl LogicalInlineEdge {
    pub(crate) const fn is_end(self) -> bool {
        matches!(self, Self::End(_))
    }

    pub(crate) const fn following_source_space(self) -> FollowingSourceSpace {
        match self {
            Self::Start => FollowingSourceSpace::RetainedAdvance,
            Self::End(participation) => participation,
        }
    }

    pub(crate) fn source_projection(self, width: f32) -> LogicalInlineEdgeSourceProjection {
        match self {
            Self::Start if width == 0.0 => LogicalInlineEdgeSourceProjection::Absent,
            Self::Start => LogicalInlineEdgeSourceProjection::BeforeGeometry,
            Self::End(_) => LogicalInlineEdgeSourceProjection::AfterGeometry,
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
    bidi_attachment: InlineBoxBidiAttachment,
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
            bidi_attachment: InlineBoxBidiAttachment::Independent,
        }
    }

    /// A positioned anchor which does not participate in line breaking or sizing.
    pub fn transparent_anchor(id: u64, index: usize) -> Self {
        Self {
            id,
            index,
            participation: InlineBoxParticipation::TransparentAnchor,
            bidi_attachment: InlineBoxBidiAttachment::Independent,
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
            bidi_attachment: InlineBoxBidiAttachment::Independent,
        }
    }

    /// A logical inline-start edge which reorders with the following content.
    pub fn inline_start_edge(
        id: u64,
        index: usize,
        width: f32,
        height: f32,
        _break_affinity: InlineBoxBreakAffinity,
    ) -> Self {
        Self {
            id,
            index,
            participation: InlineBoxParticipation::LogicalOwnerStart { width, height },
            bidi_attachment: InlineBoxBidiAttachment::ToNext,
        }
    }

    /// A logical inline-end edge which reorders with the preceding content.
    pub fn inline_end_edge(
        id: u64,
        index: usize,
        width: f32,
        height: f32,
        _break_affinity: InlineBoxBreakAffinity,
    ) -> Self {
        Self::inline_end_edge_with_following_source_space(
            id,
            index,
            width,
            height,
            FollowingSourceSpace::RetainedAdvance,
            _break_affinity,
        )
    }

    /// A logical inline-end edge with explicit following source-space
    /// participation.
    pub fn inline_end_edge_with_following_source_space(
        id: u64,
        index: usize,
        width: f32,
        height: f32,
        following_source_space: FollowingSourceSpace,
        _break_affinity: InlineBoxBreakAffinity,
    ) -> Self {
        Self {
            id,
            index,
            participation: InlineBoxParticipation::LogicalOwnerEnd {
                width,
                height,
                following_source_space,
            },
            bidi_attachment: InlineBoxBidiAttachment::ToPrevious,
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

    pub(crate) const fn line_break_participation(&self) -> InlineBoxLineBreakParticipation {
        self.participation.line_break_participation()
    }

    pub(crate) const fn shaping_participation(&self) -> InlineBoxShapingParticipation {
        self.participation.shaping_participation()
    }

    pub(crate) const fn line_metric_participation(&self) -> InlineBoxLineMetricParticipation {
        self.participation.line_metric_participation()
    }

    pub(crate) const fn bidi_attachment(&self) -> InlineBoxBidiAttachment {
        self.bidi_attachment
    }

    pub(crate) const fn is_transparent_anchor(&self) -> bool {
        matches!(
            self.line_break_participation(),
            InlineBoxLineBreakParticipation::TransparentAnchor
        )
    }
}
