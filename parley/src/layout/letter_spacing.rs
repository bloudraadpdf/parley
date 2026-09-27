// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// A typographic unit's computed tracking value in layout units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LetterSpacingUnit {
    pub(super) spacing: f32,
    pub(super) atomic: bool,
}

impl LetterSpacingUnit {
    pub const fn text(spacing: f32) -> Self {
        Self {
            spacing,
            atomic: false,
        }
    }

    pub const fn atomic(spacing: f32) -> Self {
        Self {
            spacing,
            atomic: true,
        }
    }

    pub const fn spacing(self) -> f32 {
        self.spacing
    }

    pub const fn is_atomic(self) -> bool {
        self.atomic
    }

    /// The interval between adjacent visual units. Consecutive atomic boxes
    /// belong to one unit and have no internal tracking interval.
    pub fn interval_to(self, next: Self) -> f32 {
        if self.atomic && next.atomic {
            0.0
        } else {
            (self.spacing + next.spacing) * 0.5
        }
    }
}

/// A retained source unit and its resolved outgoing tracking advance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LetterSpacingUnitRecord {
    pub(super) unit: LetterSpacingUnit,
    pub(super) source_start: usize,
    pub(super) source_end: usize,
    pub(super) applied_advance: f32,
}

impl LetterSpacingUnitRecord {
    pub const fn unit(self) -> LetterSpacingUnit {
        self.unit
    }

    pub fn source_range(self) -> core::ops::Range<usize> {
        self.source_start..self.source_end
    }

    /// Tracking already included after this visual unit. For a final unit,
    /// this preserves trimmed versus continuing-fragment spacing.
    pub const fn applied_advance(self) -> f32 {
        self.applied_advance
    }
}
