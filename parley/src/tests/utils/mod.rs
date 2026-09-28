// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;
use peniko::Color;

pub(crate) mod asserts;

pub(super) fn visual_glyphs(layout: &crate::Layout<ColorBrush>) -> Vec<Vec<(u32, f32, f32, f32)>> {
    layout
        .lines()
        .map(|line| {
            line.items()
                .flat_map(|item| match item {
                    crate::PositionedLayoutItem::GlyphRun(run) => run
                        .glyphs()
                        .map(|glyph| (glyph.id, glyph.advance, glyph.x, glyph.y))
                        .collect::<Vec<_>>(),
                    crate::PositionedLayoutItem::InlineBox(_) => Vec::new(),
                })
                .collect()
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ColorBrush {
    pub(crate) color: Color,
}

impl ColorBrush {
    pub(crate) fn new(color: Color) -> Self {
        let rgba8 = color.to_rgba8();
        Self {
            color: Color::from_rgba8(rgba8.r, rgba8.g, rgba8.b, rgba8.a),
        }
    }
}

impl Default for ColorBrush {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
        }
    }
}
