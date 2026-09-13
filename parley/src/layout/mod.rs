// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Layout types.

#[cfg(feature = "accesskit")]
mod accessibility;
mod alignment;
mod bidi;
mod cluster;
mod glyph;
mod inline_fragmentation;
mod line;
mod line_break;
mod run;

// TODO - Add to allowed lint set?
#[expect(
    clippy::module_inception,
    reason = "Private inner module for code organisation"
)]
mod layout;

pub(crate) mod data;

#[cfg(feature = "accesskit")]
pub use accessibility::LayoutAccessibility;
pub use alignment::{Alignment, AlignmentOptions, JustificationMode};
pub use bidi::{BidiAtomId, BidiLevel, BidiTopology, BidiVisualAtom, BidiVisualAtomKind};
pub use cluster::{Affinity, Cluster, ClusterPath, ClusterSide};
pub use data::{
    BreakReason, DiscretionaryBreak, LineBreakOverride, LineStartFitAdvance,
    NormalSoftWrapSelection,
};
pub use glyph::Glyph;
pub use layout::Layout;
pub use line::{
    GlyphRun, HangingWhitespace, Line, LineMetrics, PositionedInlineBox, PositionedLayoutItem,
};
pub use line_break::{BreakLines, CommittedLine, LineTabOrigin};
pub use run::{Run, RunMetrics};

pub(crate) use data::{LayoutData, LayoutItem, LayoutItemKind, LineData, LineItemData};
pub(crate) use line::LineItem;

// TODO - Deprecation not yet active to ease internal code migration.
#[deprecated(since = "TBD", note = "Access from the `editing` module instead.")]
pub use crate::editing::{Cursor, Selection};

// TODO - Move the following to `style` module and submodules.

use crate::style::Brush;
use crate::style::SoftBreakPolicy;
use crate::{
    HyphenateCharacter, LineHeight, OverflowWrap, TabSize, TextWrapMode, WhiteSpaceCollapse,
};

#[allow(clippy::partial_pub_fields)]
/// Style properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Style<B: Brush> {
    /// Brush for drawing glyphs.
    pub brush: B,
    /// Underline decoration.
    pub underline: Option<Decoration<B>>,
    /// Strikethrough decoration.
    pub strikethrough: Option<Decoration<B>>,
    /// Overline decoration.
    pub overline: Option<Decoration<B>>,
    /// Partially resolved line height, either in in layout units or dependent on metrics
    pub(crate) line_height: LineHeight,
    /// Resolved normal line-breaking behavior.
    pub(crate) soft_break_policy: SoftBreakPolicy,
    /// Per-cluster overflow-wrap setting
    pub(crate) overflow_wrap: OverflowWrap,
    /// Per-cluster text-wrap-mode setting
    pub(crate) text_wrap_mode: TextWrapMode,
    /// Per-cluster white-space collapsing and hanging setting.
    pub(crate) white_space_collapse: WhiteSpaceCollapse,
    /// Tab size.
    pub(crate) tab_size: TabSize,
    /// Hyphenate character.
    pub(crate) hyphenate_character: HyphenateCharacter,
    /// Whether the consumer-selected fixed font-metric grid applies to this
    /// style run.
    pub(crate) font_metric_advance_quantization: bool,
    #[cfg(feature = "accesskit")]
    /// Locale if any, so we can set the corresponding AccessKit property
    pub(crate) locale: Option<fontique::Language>,
}

/// Underline, strikethrough, or overline decoration.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoration<B: Brush> {
    /// Brush used to draw the decoration.
    pub brush: B,
    /// Offset of the decoration from the baseline. If `None`, use the metrics
    /// of the containing run.
    pub offset: Option<f32>,
    /// Thickness of the decoration. If `None`, use the metrics of the
    /// containing run.
    pub size: Option<f32>,
}

/// Lower and upper bounds on layout width based on its contents.
#[derive(Copy, Clone, Debug)]
pub struct ContentWidths {
    /// The minimum content width. This is the width of the layout if _all_ soft line-breaking
    /// opportunities are taken.
    pub min: f32,
    /// The maximum content width. This is the width of the layout if _no_ soft line-breaking
    /// opportunities are taken.
    pub max: f32,
}

/// Options controlling text-indent behavior, corresponding to CSS `text-indent` keywords.
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub struct IndentOptions {
    /// If `true`, indent also applies after every hard line break, not just to the first line.
    /// Corresponds to the CSS `each-line` keyword. Defaults to `false`.
    pub each_line: bool,
    /// If `true`, inverts which lines are indented: continuation lines are indented
    /// instead of the first line(s). Corresponds to the CSS `hanging` keyword. Defaults to `false`.
    pub hanging: bool,
}

/// Relationship between the first line in this layout and its CSS
/// `text-indent` formatting scope.
///
/// A layout of an element starts at [`Self::ElementStart`]. Fragmentation can
/// instead re-layout a suffix whose preceding line ended in either a soft wrap
/// or an explicit break. Keeping that predecessor state nominal lets the line
/// breaker apply `each-line` and `hanging` without mistaking every fragment
/// for the first formatted line of the element.
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub enum IndentStart {
    /// The layout starts with the element's first formatted line.
    #[default]
    ElementStart,
    /// The layout resumes after a soft wrap break.
    ContinuationAfterSoftWrap,
    /// The layout resumes after an explicit line break.
    ContinuationAfterExplicitBreak,
}
