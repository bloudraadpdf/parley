// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

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
    /// A glued box binds to the adjacent text with no soft-wrap
    /// opportunity on either side, and contributes its width to the
    /// surrounding unbreakable run in min-content measurement. Use for
    /// inline border/padding shims (CSS forbids a break between an
    /// inline's padding and its adjacent glyph). A regular replaced
    /// box (`false`) keeps the wrap opportunities UAX #14 assigns
    /// around objects.
    pub glue: bool,
}

impl InlineBox {
    /// A regular (non-glued) inline box.
    pub fn new(id: u64, index: usize, width: f32, height: f32) -> Self {
        Self {
            id,
            index,
            width,
            height,
            glue: false,
        }
    }
}
