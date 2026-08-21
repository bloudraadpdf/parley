// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::WhiteSpaceCollapse;
use crate::inline_box::{
    FollowingSourceSpace, InlineBox, InlineBoxLineBreakParticipation, LogicalInlineEdge,
    LogicalInlineEdgeSourceProjection,
};
use crate::layout::{ContentWidths, Glyph, JustificationMode, LineMetrics, RunMetrics, Style};
use crate::style::Brush;

/// Selection policy among normal soft-wrap opportunities.
///
/// This does not create or suppress opportunities. It only decides whether
/// normal punctuation priority classes may displace a later fitting boundary,
/// or whether composition always keeps the latest fitting normal boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NormalSoftWrapSelection {
    /// Apply the normal UA punctuation priority classes.
    #[default]
    PriorityClasses,
    /// Keep the latest fitting normal boundary.
    GreedyLatest,
}

/// A caller-supplied soft line-break decision at one UTF-8 byte boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineBreakOverride {
    byte_index: usize,
    disposition: LineBreakOverrideDisposition,
}

/// Provenance-preserving override disposition.
///
/// This is deliberately private: callers choose one of the semantic
/// constructors on [`LineBreakOverride`] and cannot fabricate a boolean
/// opportunity that silently loses its priority contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineBreakOverrideDisposition {
    Suppress,
    NormalOpportunity,
    UnprioritizedOpportunity,
    ResolvedCollapsedSourceOpportunity,
    ResolvedRetainedSourceOpportunity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceSoftWrapBoundary {
    Absent,
    Opportunity {
        byte_index: usize,
        authority: SourceSoftWrapAuthority,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceSoftWrapAuthority {
    AdjoiningStyles { following_wrap_mode: TextWrapMode },
    CallerResolvedCollapsedSpace,
    CallerResolvedRetainedSpace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectedSourceBoundary {
    Exact {
        byte_index: usize,
    },
    AcrossCollapsibleSpace {
        edge_byte_index: usize,
        source_byte_index: usize,
        following_source_space: FollowingSourceSpace,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectedSourceClusterParticipation {
    Normal,
    CollapsedSourceSpace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectedSourceLineFill {
    AvailableMeasure,
    FilledMeasure,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum SelectedSourceClusterAdvance {
    #[default]
    Natural,
    Collapsed {
        source_range: Range<usize>,
    },
}

impl SelectedSourceClusterAdvance {
    pub(crate) fn resolve(&self, byte_index: usize, natural_advance: f32) -> f32 {
        match self {
            Self::Collapsed { source_range } if source_range.contains(&byte_index) => 0.0,
            Self::Natural | Self::Collapsed { .. } => natural_advance,
        }
    }
}

impl ProjectedSourceBoundary {
    pub(crate) const fn resolve_line_fill(self, fill: ProjectedSourceLineFill) -> Self {
        match (self, fill) {
            (
                Self::AcrossCollapsibleSpace {
                    edge_byte_index,
                    source_byte_index,
                    following_source_space: FollowingSourceSpace::CollapseAfterFilledOwnerFragment,
                },
                ProjectedSourceLineFill::FilledMeasure,
            ) => Self::AcrossCollapsibleSpace {
                edge_byte_index,
                source_byte_index,
                following_source_space: FollowingSourceSpace::CollapsedAfterProjectedBreak,
            },
            (
                Self::AcrossCollapsibleSpace {
                    edge_byte_index,
                    source_byte_index,
                    following_source_space: FollowingSourceSpace::CollapseAfterFilledOwnerFragment,
                },
                ProjectedSourceLineFill::AvailableMeasure,
            ) => Self::AcrossCollapsibleSpace {
                edge_byte_index,
                source_byte_index,
                following_source_space: FollowingSourceSpace::RetainedAdvance,
            },
            (boundary, _) => boundary,
        }
    }

    pub(crate) const fn target(self) -> usize {
        match self {
            Self::Exact { byte_index } => byte_index,
            Self::AcrossCollapsibleSpace {
                source_byte_index, ..
            } => source_byte_index,
        }
    }

    pub(crate) const fn suppresses(self, byte_index: usize) -> bool {
        match self {
            Self::Exact {
                byte_index: projected_byte_index,
            } => byte_index == projected_byte_index,
            Self::AcrossCollapsibleSpace {
                edge_byte_index,
                source_byte_index,
                ..
            } => byte_index >= edge_byte_index && byte_index <= source_byte_index,
        }
    }

    pub(crate) const fn cluster_participation(
        self,
        byte_index: usize,
    ) -> ProjectedSourceClusterParticipation {
        match self {
            Self::AcrossCollapsibleSpace {
                edge_byte_index,
                source_byte_index,
                following_source_space: FollowingSourceSpace::CollapsedAfterProjectedBreak,
            } if byte_index >= edge_byte_index && byte_index < source_byte_index => {
                ProjectedSourceClusterParticipation::CollapsedSourceSpace
            }
            Self::Exact { .. } | Self::AcrossCollapsibleSpace { .. } => {
                ProjectedSourceClusterParticipation::Normal
            }
        }
    }

    pub(crate) fn selected_source_cluster_advance(self) -> SelectedSourceClusterAdvance {
        match self {
            Self::AcrossCollapsibleSpace {
                edge_byte_index,
                source_byte_index,
                following_source_space: FollowingSourceSpace::CollapsedAfterProjectedBreak,
            } => SelectedSourceClusterAdvance::Collapsed {
                source_range: edge_byte_index..source_byte_index,
            },
            Self::Exact { .. } | Self::AcrossCollapsibleSpace { .. } => {
                SelectedSourceClusterAdvance::Natural
            }
        }
    }
}

impl SourceSoftWrapBoundary {
    pub(crate) const fn is_available_from(self, preceding_wrap_mode: TextWrapMode) -> bool {
        match self {
            Self::Absent => false,
            Self::Opportunity {
                authority:
                    SourceSoftWrapAuthority::AdjoiningStyles {
                        following_wrap_mode,
                    },
                ..
            } => {
                matches!(preceding_wrap_mode, TextWrapMode::Wrap)
                    || matches!(following_wrap_mode, TextWrapMode::Wrap)
            }
            Self::Opportunity {
                authority:
                    SourceSoftWrapAuthority::CallerResolvedCollapsedSpace
                    | SourceSoftWrapAuthority::CallerResolvedRetainedSpace,
                ..
            } => true,
        }
    }

    pub(crate) const fn projection_from(
        self,
        edge_byte_index: usize,
        following_source_space: FollowingSourceSpace,
    ) -> Option<ProjectedSourceBoundary> {
        match self {
            Self::Absent => None,
            Self::Opportunity { byte_index, .. } if byte_index == edge_byte_index => {
                Some(ProjectedSourceBoundary::Exact { byte_index })
            }
            Self::Opportunity {
                byte_index,
                authority: SourceSoftWrapAuthority::CallerResolvedRetainedSpace,
            } if byte_index > edge_byte_index => None,
            Self::Opportunity { byte_index, .. }
                if byte_index > edge_byte_index
                    && matches!(
                        following_source_space,
                        FollowingSourceSpace::UnicodeBoundary
                    ) =>
            {
                None
            }
            Self::Opportunity { byte_index, .. } if byte_index > edge_byte_index => {
                Some(ProjectedSourceBoundary::AcrossCollapsibleSpace {
                    edge_byte_index,
                    source_byte_index: byte_index,
                    following_source_space,
                })
            }
            Self::Opportunity { .. } => None,
        }
    }
}

impl LineBreakOverride {
    /// Suppress a Unicode soft-wrap opportunity at `byte_index`.
    pub const fn suppress(byte_index: usize) -> Self {
        Self {
            byte_index,
            disposition: LineBreakOverrideDisposition::Suppress,
        }
    }

    /// Add a normal opportunity whose priority is derived from its authored
    /// source unit and the resolved line-breaking policy.
    pub const fn opportunity(byte_index: usize) -> Self {
        Self {
            byte_index,
            disposition: LineBreakOverrideDisposition::NormalOpportunity,
        }
    }

    /// Add an explicitly equal-priority opportunity, as required for
    /// `line-break: anywhere`-style caller policies.
    pub const fn unprioritized_opportunity(byte_index: usize) -> Self {
        Self {
            byte_index,
            disposition: LineBreakOverrideDisposition::UnprioritizedOpportunity,
        }
    }

    /// Add a source opportunity whose wrapping styles were resolved before
    /// text normalisation removed its owning run.
    pub const fn resolved_collapsed_source_opportunity(byte_index: usize) -> Self {
        Self {
            byte_index,
            disposition: LineBreakOverrideDisposition::ResolvedCollapsedSourceOpportunity,
        }
    }

    /// Add a resolved source opportunity after a retained source space.
    ///
    /// Logical owner edges must not project this boundary before the visible
    /// space that owns it.
    pub const fn resolved_retained_source_opportunity(byte_index: usize) -> Self {
        Self {
            byte_index,
            disposition: LineBreakOverrideDisposition::ResolvedRetainedSourceOpportunity,
        }
    }

    /// UTF-8 byte index of the affected boundary.
    pub const fn byte_index(self) -> usize {
        self.byte_index
    }

    /// Whether the override adds rather than suppresses an opportunity.
    pub const fn allows_break(self) -> bool {
        !matches!(self.disposition, LineBreakOverrideDisposition::Suppress)
    }

    pub(crate) const fn disposition(self) -> LineBreakOverrideDisposition {
        self.disposition
    }
}

/// Material inserted only when a discretionary line break is taken.
///
/// The advance participates in line fitting and alignment, but not in the
/// unbroken text flow. This is the standard penalty-node model used for soft
/// hyphens: the visible hyphen has a real width only on the line where the
/// break is selected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiscretionaryBreak {
    /// UTF-8 byte index of the boundary after the discretionary character.
    pub byte_index: usize,
    /// Advance of the material displayed when the break is taken.
    pub advance: f32,
    /// Maximum number of consecutive lines that may end at this class of
    /// discretionary break. `None` imposes no limit.
    pub max_consecutive_lines: Option<u32>,
}
use crate::util::nearly_zero;
use crate::{
    FontData, FontMetricAdvanceQuantization, IndentOptions, IndentStart, LineHeight, OverflowWrap,
    TextWrapMode,
};
use core::num::NonZeroU16;
use core::ops::Range;
use skrifa::MetadataProvider as _;

use alloc::vec::Vec;

use crate::analysis::cluster::Whitespace;
use crate::analysis::{AuthoredBreakUnit, Boundary, CharInfo};

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct ClusterData {
    pub(crate) info: ClusterInfo,
    /// Cluster flags (see impl methods for details).
    pub(crate) flags: u16,
    /// Style index for this cluster.
    pub(crate) style_index: u16,
    /// Number of glyphs in this cluster (0xFF = single glyph stored inline)
    pub(crate) glyph_len: u8,
    /// Number of text bytes in this cluster
    pub(crate) text_len: u8,
    /// If `glyph_len == 0xFF`, then `glyph_offset` is a glyph identifier,
    /// otherwise, it's an offset into the glyph array with the base
    /// taken from the owning run.
    pub(crate) glyph_offset: u32,
    /// Offset into the text for this cluster
    pub(crate) text_offset: u16,
    /// Advance width for this cluster
    pub(crate) advance: f32,
    /// Advance used only for greedy line-fit decisions. This normally equals
    /// `advance`; nominal-metric line breaking excludes shaping adjustments
    /// such as kerning while the rendered cluster retains them.
    pub(crate) line_break_advance: f32,
}

impl ClusterData {
    pub(crate) const LIGATURE_START: u16 = 1;
    pub(crate) const LIGATURE_COMPONENT: u16 = 2;

    #[inline(always)]
    pub(crate) fn is_ligature_start(self) -> bool {
        self.flags & Self::LIGATURE_START != 0
    }

    #[inline(always)]
    pub(crate) fn is_ligature_component(self) -> bool {
        self.flags & Self::LIGATURE_COMPONENT != 0
    }

    #[inline(always)]
    pub(crate) fn text_range(self, run: &RunData) -> Range<usize> {
        let start = run.text_range.start + self.text_offset as usize;
        start..start + self.text_len as usize
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct ClusterInfo {
    boundary: Boundary,
    source_char: char,
    authored_break_unit: AuthoredBreakUnit,
}

impl ClusterInfo {
    pub(crate) fn new(
        boundary: Boundary,
        source_char: char,
        authored_break_unit: AuthoredBreakUnit,
    ) -> Self {
        Self {
            boundary,
            source_char,
            authored_break_unit,
        }
    }

    // Returns the boundary type of the cluster.
    pub(crate) fn boundary(self) -> Boundary {
        self.boundary
    }

    // Returns the whitespace type of the cluster.
    pub(crate) fn whitespace(self) -> Whitespace {
        to_whitespace(self.source_char)
    }

    /// Returns if the cluster is a line boundary.
    pub(crate) fn is_boundary(self) -> bool {
        self.boundary != Boundary::None
    }

    /// Returns if the cluster is an emoji.
    pub(crate) fn is_emoji(self) -> bool {
        // TODO: Defer to ICU4X properties (see: https://docs.rs/icu/latest/icu/properties/props/struct.Emoji.html).
        matches!(self.source_char as u32, 0x1F600..=0x1F64F | 0x1F300..=0x1F5FF | 0x1F680..=0x1F6FF | 0x2600..=0x26FF | 0x2700..=0x27BF)
    }

    /// Returns if the cluster is any whitespace.
    pub(crate) fn is_whitespace(self) -> bool {
        self.source_char.is_whitespace()
    }

    /// Returns the cluster's original character.
    pub(crate) fn source_char(self) -> char {
        self.source_char
    }

    /// Returns the semantic source-unit class used for wrap-candidate
    /// provenance.
    pub(crate) fn authored_break_unit(self) -> AuthoredBreakUnit {
        self.authored_break_unit
    }

    /// Returns whether this cluster is absent from the default visual
    /// rendering and therefore is not a typographic character unit for
    /// letter spacing.
    ///
    /// CSS Text applies tracking to typographic character units, not to
    /// default-ignorable format controls. This includes an unselected soft
    /// hyphen: its visible replacement is discretionary material and is
    /// measured separately only when the line actually breaks there.
    pub(crate) fn is_default_ignorable(self) -> bool {
        icu_properties::CodePointSetData::new::<icu_properties::props::DefaultIgnorableCodePoint>()
            .contains(self.source_char)
    }
}

const fn to_whitespace(c: char) -> Whitespace {
    const LINE_SEPARATOR: char = '\u{2028}';
    const PARAGRAPH_SEPARATOR: char = '\u{2029}';

    match c {
        ' ' => Whitespace::Space,
        '\t' => Whitespace::Tab,
        '\n' | '\r' | LINE_SEPARATOR | PARAGRAPH_SEPARATOR => Whitespace::Newline,
        '\u{00A0}' => Whitespace::NoBreakSpace,
        _ => Whitespace::None,
    }
}

/// `HarfRust`-based run data
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunData {
    /// Index of the font for the run.
    pub(crate) font_index: usize,
    /// Font size.
    pub(crate) font_size: f32,
    /// Font attributes, needed for accessibility.
    pub(crate) font_attrs: fontique::Attributes,
    /// Synthesis for rendering (contains variation settings)
    pub(crate) synthesis: fontique::Synthesis,
    /// Range of normalized coordinates in the layout data.
    pub(crate) coords_range: Range<usize>,
    /// Range of the source text.
    pub(crate) text_range: Range<usize>,
    /// Bidi level for the run.
    pub(crate) bidi_level: u8,
    /// Range of clusters.
    pub(crate) cluster_range: Range<usize>,
    /// Base for glyph indices.
    pub(crate) glyph_start: usize,
    /// Metrics for the run.
    pub(crate) metrics: RunMetrics,
    /// Additional word spacing.
    pub(crate) word_spacing: f32,
    /// Additional letter spacing.
    pub(crate) letter_spacing: f32,
    /// Total advance of the run.
    pub(crate) advance: f32,
}

#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub enum BreakReason {
    #[default]
    None,
    Regular,
    Explicit,
    Emergency,
}

#[derive(Clone, Default, Debug, PartialEq)]
pub(crate) struct LineData {
    /// Range of the source text.
    pub(crate) text_range: Range<usize>,
    /// Range of line items.
    pub(crate) item_range: Range<usize>,
    /// Metrics for the line.
    pub(crate) metrics: LineMetrics,
    /// The cause of the line break.
    pub(crate) break_reason: BreakReason,
    /// Maximum advance for the line.
    pub(crate) max_advance: f32,
    /// Number of justified clusters on the line.
    pub(crate) num_spaces: usize,
    /// Source-cluster advance selected for this materialised line.
    pub(crate) selected_source_cluster_advance: SelectedSourceClusterAdvance,
    /// Text indent applied to this line.
    pub(crate) indent: f32,
    /// Advance inserted only because this line ended at a discretionary
    /// break. Zero for ordinary and mandatory breaks.
    pub(crate) discretionary_advance: f32,
    /// Whether this line selected a registered discretionary boundary.
    /// Kept separately from the advance because valid inserted material may
    /// have zero width.
    pub(crate) ends_at_discretionary_break: bool,
}

impl LineData {
    pub(crate) fn size(&self) -> f32 {
        self.metrics.ascent + self.metrics.descent + self.metrics.leading
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineItemData {
    /// Whether the item is a run or an inline box
    pub(crate) kind: LayoutItemKind,
    /// The index of the run or inline box in the runs or `inline_boxes` vec
    pub(crate) index: usize,
    /// Bidi level for the item (used for reordering)
    pub(crate) bidi_level: u8,
    /// Stable item identity in the paragraph topology.
    pub(crate) layout_item_index: Option<usize>,
    /// Advance (size in direction of text flow) for the run.
    pub(crate) advance: f32,

    // Fields that only apply to text runs (Ignored for boxes)
    // TODO: factor this out?
    /// True if the run is composed entirely of whitespace.
    pub(crate) is_whitespace: bool,
    /// True if the run ends in whitespace.
    pub(crate) has_trailing_whitespace: bool,
    /// Range of the source text.
    pub(crate) text_range: Range<usize>,
    /// Range of clusters.
    pub(crate) cluster_range: Range<usize>,
}

impl LineItemData {
    pub(crate) fn is_text_run(&self) -> bool {
        self.kind == LayoutItemKind::TextRun
    }

    #[inline(always)]
    pub(crate) fn is_rtl(&self) -> bool {
        self.bidi_level & 1 != 0
    }

    /// If the item is a text run
    ///   - Determine if it consists entirely of whitespace (`is_whitespace` property)
    ///   - Determine if it has trailing whitespace (`has_trailing_whitespace` property)
    pub(crate) fn compute_whitespace_properties<B: Brush>(&mut self, layout_data: &LayoutData<B>) {
        // Skip items which are not text runs
        if self.kind != LayoutItemKind::TextRun {
            return;
        }

        self.is_whitespace = true;
        if self.is_rtl() {
            // RTL runs check for "trailing" whitespace at the front.
            for cluster in layout_data.clusters[self.cluster_range.clone()].iter() {
                if cluster.info.is_default_ignorable() {
                    continue;
                } else if cluster.info.is_whitespace() {
                    self.has_trailing_whitespace = true;
                } else {
                    self.is_whitespace = false;
                    break;
                }
            }
        } else {
            for cluster in layout_data.clusters[self.cluster_range.clone()]
                .iter()
                .rev()
            {
                if cluster.info.is_default_ignorable() {
                    continue;
                } else if cluster.info.is_whitespace() {
                    self.has_trailing_whitespace = true;
                } else {
                    self.is_whitespace = false;
                    break;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutItemKind {
    TextRun,
    InlineBox,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayoutItem {
    /// Whether the item is a run or an inline box
    pub(crate) kind: LayoutItemKind,
    /// The index of the run or inline box in the runs or `inline_boxes` vec
    pub(crate) index: usize,
    /// Bidi level for the item (used for reordering)
    pub(crate) bidi_level: u8,
    /// Source text owned by this item before line breaking.
    pub(crate) text_range: Range<usize>,
    /// Shaped clusters owned by this item before line breaking.
    pub(crate) cluster_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutData<B: Brush> {
    pub(crate) scale: f32,
    pub(crate) quantize: bool,
    pub(crate) font_metric_advance_quantization: Option<FontMetricAdvanceQuantization>,
    pub(crate) nominal_font_metric_line_breaks: bool,
    pub(crate) normal_soft_wrap_selection: NormalSoftWrapSelection,
    /// When `true`, the line breaker reclaims the advance of collapsible
    /// trailing whitespace when doing so lets the following inline box fit
    /// on the current line (PDFreactor's model) instead of wrapping the
    /// box (the browser model). See `BreakLines`.
    pub(crate) reclaim_space_before_inline_box: bool,
    /// Caller-supplied decisions for specific UTF-8 byte boundaries. Entries
    /// are sorted by `byte_index`; `opportunity = true` adds a soft break and
    /// `false` suppresses the Unicode soft break at that boundary.
    pub(crate) line_break_overrides: Vec<LineBreakOverride>,
    /// Sorted discretionary break material, keyed by UTF-8 boundary.
    pub(crate) discretionary_breaks: Vec<DiscretionaryBreak>,
    pub(crate) base_level: u8,
    pub(crate) text_len: usize,
    pub(crate) width: f32,
    pub(crate) full_width: f32,
    pub(crate) height: f32,
    pub(crate) fonts: Vec<FontData>,
    pub(crate) coords: Vec<i16>,

    // Input (/ output of style resolution)
    pub(crate) styles: Vec<Style<B>>,
    pub(crate) inline_boxes: Vec<InlineBox>,

    // Output of shaping
    pub(crate) runs: Vec<RunData>,
    pub(crate) items: Vec<LayoutItem>,
    pub(crate) clusters: Vec<ClusterData>,
    pub(crate) glyphs: Vec<Glyph>,

    // Output of line breaking
    pub(crate) lines: Vec<LineData>,
    pub(crate) line_items: Vec<LineItemData>,

    // Output of alignment
    #[cfg(feature = "accesskit")]
    /// Directly store the alignment if accessibility is enabled so we can
    /// set the corresponding AccessKit property.
    pub(crate) alignment: Option<super::Alignment>,
    /// Whether the layout is aligned with [`crate::Alignment::Justify`].
    pub(crate) is_aligned_justified: bool,
    /// The justification mode used for the current aligned state, if any.
    pub(crate) aligned_justification_mode: Option<JustificationMode>,
    /// The width the layout was aligned to.
    pub(crate) alignment_width: f32,
    /// Per-line override widths supplied by [`crate::Layout::align_per_line`].
    /// When non-empty, the entry at `line_index` overrides
    /// [`Self::alignment_width`] for that line; lines without an entry fall
    /// back to [`Self::alignment_width`]. Empty when single-width
    /// [`crate::Layout::align`] was used.
    pub(crate) per_line_alignment_widths: Vec<f32>,
    /// The text-indent amount in layout units.
    pub(crate) indent_amount: f32,
    /// Options controlling text-indent behavior (each-line, hanging).
    pub(crate) indent_options: IndentOptions,
    /// Formatting-scope relationship of this layout's first line.
    pub(crate) indent_start: IndentStart,
}

impl<B: Brush> Default for LayoutData<B> {
    fn default() -> Self {
        Self {
            scale: 1.,
            quantize: true,
            font_metric_advance_quantization: None,
            nominal_font_metric_line_breaks: false,
            normal_soft_wrap_selection: NormalSoftWrapSelection::default(),
            reclaim_space_before_inline_box: false,
            line_break_overrides: Vec::new(),
            discretionary_breaks: Vec::new(),
            base_level: 0,
            text_len: 0,
            width: 0.,
            full_width: 0.,
            height: 0.,
            fonts: Vec::new(),
            coords: Vec::new(),
            styles: Vec::new(),
            inline_boxes: Vec::new(),
            runs: Vec::new(),
            items: Vec::new(),
            clusters: Vec::new(),
            glyphs: Vec::new(),
            lines: Vec::new(),
            line_items: Vec::new(),
            #[cfg(feature = "accesskit")]
            alignment: None,
            is_aligned_justified: false,
            aligned_justification_mode: None,
            alignment_width: 0.0,
            per_line_alignment_widths: Vec::new(),
            indent_amount: 0.0,
            indent_options: IndentOptions::default(),
            indent_start: IndentStart::default(),
        }
    }
}

impl<B: Brush> LayoutData<B> {
    pub(crate) fn source_soft_wrap_boundary_after(
        &self,
        item_index: usize,
        byte_index: usize,
        edge: LogicalInlineEdge,
    ) -> SourceSoftWrapBoundary {
        let boundary_override = |index| {
            self.line_break_overrides
                .binary_search_by_key(&index, |entry| entry.byte_index())
                .ok()
                .map(|entry| self.line_break_overrides[entry].disposition())
        };
        for item in &self.items[item_index + 1..] {
            match item.kind {
                LayoutItemKind::InlineBox => {
                    let inline_box = &self.inline_boxes[item.index];
                    if inline_box.index < byte_index
                        || !matches!(
                            inline_box.line_break_participation(),
                            InlineBoxLineBreakParticipation::LogicalOwnerEdge(_)
                                | InlineBoxLineBreakParticipation::TransparentAnchor
                        )
                    {
                        return SourceSoftWrapBoundary::Absent;
                    }
                }
                LayoutItemKind::TextRun => {
                    let run = &self.runs[item.index];
                    for cluster in &self.clusters[item.cluster_range.clone()] {
                        let cluster_index = cluster.text_range(run).start;
                        if cluster_index < byte_index {
                            continue;
                        }
                        let disposition = boundary_override(cluster_index);
                        if edge.is_end() && cluster.info.whitespace() == Whitespace::Space {
                            continue;
                        }
                        let authority = match disposition {
                            Some(LineBreakOverrideDisposition::Suppress) => {
                                return SourceSoftWrapBoundary::Absent;
                            }
                            Some(
                                LineBreakOverrideDisposition::ResolvedCollapsedSourceOpportunity,
                            ) => Some(SourceSoftWrapAuthority::CallerResolvedCollapsedSpace),
                            Some(
                                LineBreakOverrideDisposition::ResolvedRetainedSourceOpportunity,
                            ) => Some(SourceSoftWrapAuthority::CallerResolvedRetainedSpace),
                            Some(
                                LineBreakOverrideDisposition::NormalOpportunity
                                | LineBreakOverrideDisposition::UnprioritizedOpportunity,
                            ) => Some(SourceSoftWrapAuthority::AdjoiningStyles {
                                following_wrap_mode: self.styles[cluster.style_index as usize]
                                    .text_wrap_mode,
                            }),
                            None if cluster.info.boundary() == Boundary::Line => {
                                Some(SourceSoftWrapAuthority::AdjoiningStyles {
                                    following_wrap_mode: self.styles[cluster.style_index as usize]
                                        .text_wrap_mode,
                                })
                            }
                            None => None,
                        };
                        if let Some(authority) = authority {
                            return SourceSoftWrapBoundary::Opportunity {
                                byte_index: cluster_index,
                                authority,
                            };
                        }
                        return SourceSoftWrapBoundary::Absent;
                    }
                }
            }
        }
        SourceSoftWrapBoundary::Absent
    }

    pub(crate) fn clear(&mut self) {
        self.scale = 1.;
        self.quantize = true;
        self.font_metric_advance_quantization = None;
        self.nominal_font_metric_line_breaks = false;
        self.normal_soft_wrap_selection = NormalSoftWrapSelection::default();
        self.reclaim_space_before_inline_box = false;
        self.line_break_overrides.clear();
        self.base_level = 0;
        self.text_len = 0;
        self.width = 0.;
        self.full_width = 0.;
        self.height = 0.;
        self.fonts.clear();
        self.coords.clear();
        self.styles.clear();
        self.inline_boxes.clear();
        self.runs.clear();
        self.items.clear();
        self.clusters.clear();
        self.glyphs.clear();
        self.lines.clear();
        self.line_items.clear();
        self.is_aligned_justified = false;
        self.aligned_justification_mode = None;
        self.alignment_width = 0.0;
        self.per_line_alignment_widths.clear();
        self.indent_amount = 0.0;
        self.indent_options = IndentOptions::default();
        self.indent_start = IndentStart::default();
    }

    /// Push an inline box to the list of items.
    ///
    /// UAX #9 §3.3.4 (rule N2) and CSS Writing Modes 4 §2.4 specify that an
    /// atomic inline (treated as Object Replacement Character U+FFFC) takes
    /// the embedding direction when no same-direction strong character
    /// surrounds it. Concretely: inside a `direction: rtl` paragraph an
    /// inline box at the logical paragraph start must shape at the
    /// paragraph base level (1, RTL) so that L2 reordering moves it to
    /// the visual inline-end (right) edge, not the visual inline-start
    /// (left) edge.
    ///
    /// We inherit the previous run's level when one exists so a box
    /// embedded mid-run keeps the level of its surrounding strong text;
    /// otherwise we fall back to the paragraph base level instead of
    /// the LTR default. The previous LTR default caused an inline box
    /// at the start of an RTL paragraph to be visually placed at the
    /// left of the line — observably mismatching Chromium, PDFreactor,
    /// Prince and AHF on the `ui/accent-color/checked-checkbox-rtl`
    /// scenario family.
    pub(crate) fn push_inline_box(&mut self, index: usize, surrounding_level: u8) {
        self.items.push(LayoutItem {
            kind: LayoutItemKind::InlineBox,
            index,
            bidi_level: surrounding_level,
            text_range: 0..0,
            cluster_range: 0..0,
        });
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push_run(
        &mut self,
        font: FontData,
        font_size: f32,
        font_attrs: fontique::Attributes,
        synthesis: fontique::Synthesis,
        glyph_buffer: &harfrust::GlyphBuffer,
        script: icu_properties::props::Script,
        bidi_level: u8,
        style_index: u16,
        word_spacing: f32,
        letter_spacing: f32,
        source_text: &str,
        char_infos: &[(CharInfo, u16)], // From text analysis
        text_range: Range<usize>,       // The text range this run covers
        coords: &[harfrust::NormalizedCoord],
        transparent_inline_boxes: &[(usize, u8, usize)],
    ) {
        let coords_start = self.coords.len();
        self.coords.extend(coords.iter().map(|c| c.to_bits()));
        let coords_end = self.coords.len();

        let font_index = self
            .fonts
            .iter()
            .position(|f| *f == font)
            .unwrap_or_else(|| {
                let index = self.fonts.len();
                self.fonts.push(font);
                index
            });

        let font = self.fonts[font_index].clone();
        let font_ref = skrifa::FontRef::from_index(font.data.as_ref(), font.index).unwrap();
        let size = skrifa::prelude::Size::new(font_size);
        let metrics = skrifa::metrics::Metrics::new(&font_ref, size, coords);
        let scale_factor = font_size / metrics.units_per_em as f32;
        let glyph_metrics = skrifa::metrics::GlyphMetrics::new(
            &font_ref,
            skrifa::prelude::Size::unscaled(),
            coords,
        );
        let advance_projection = self
            .font_metric_advance_quantization
            .filter(|quantization| {
                self.styles[style_index as usize].font_metric_advance_quantization
                    && quantization.applies_to_script(script)
            })
            .map(|quantization| {
                FontMetricAdvanceProjection::new(
                    &font_ref,
                    coords,
                    metrics.units_per_em,
                    font_size,
                    quantization.denominator(),
                )
            });
        let space_gid = font_ref.charmap().map(' ');
        let space_advance = space_gid
            .and_then(|gid| {
                glyph_metrics.advance_width(gid).map(|advance| {
                    let advance = advance * scale_factor;
                    advance_projection
                        .as_ref()
                        .map_or(advance, |projection| projection.project(gid, advance))
                })
            })
            .unwrap_or(font_size / 4.0);
        let space_glyph_id = space_gid.map_or(0_u32, |gid| gid.to_u32());
        let units_per_em = metrics.units_per_em as f32;

        let metrics = {
            let (underline_offset, underline_size) = if let Some(underline) = metrics.underline {
                (underline.offset, underline.thickness)
            } else {
                // Default values from Harfbuzz: https://github.com/harfbuzz/harfbuzz/blob/00492ec7df0038f41f78d43d477c183e4e4c506e/src/hb-ot-metrics.cc#L334
                let default = units_per_em / 18.0;
                (default, default)
            };
            let (strikethrough_offset, strikethrough_size) =
                if let Some(strikeout) = metrics.strikeout {
                    (strikeout.offset, strikeout.thickness)
                } else {
                    // Default values from HarfBuzz: https://github.com/harfbuzz/harfbuzz/blob/00492ec7df0038f41f78d43d477c183e4e4c506e/src/hb-ot-metrics.cc#L334-L347
                    (metrics.ascent / 2.0, units_per_em / 18.0)
                };

            // Overline: no dedicated OpenType metric; use ascent for offset
            // and underline thickness for size (matching browser behaviour).
            let overline_size = underline_size;
            let overline_offset = metrics.ascent;

            // Compute line height
            let style = &self.styles[style_index as usize];
            let line_height = match style.line_height {
                LineHeight::Absolute(value) => value,
                LineHeight::FontSizeRelative(value) => value * font_size,
                LineHeight::MetricsRelative(value) => {
                    (metrics.ascent - metrics.descent + metrics.leading) * value
                }
            };

            RunMetrics {
                ascent: metrics.ascent,
                descent: -metrics.descent,
                leading: metrics.leading,
                underline_offset,
                underline_size,
                strikethrough_offset,
                strikethrough_size,
                overline_offset,
                overline_size,
                line_height,
                x_height: metrics.x_height,
                cap_height: metrics.cap_height,
                space_advance,
                space_glyph_id,
            }
        };

        let cluster_range = self.clusters.len()..self.clusters.len();

        let mut run = RunData {
            font_index,
            font_size,
            font_attrs,
            synthesis,
            coords_range: coords_start..coords_end,
            text_range,
            bidi_level,
            cluster_range,
            glyph_start: self.glyphs.len(),
            metrics,
            word_spacing,
            letter_spacing,
            advance: 0.,
        };

        // `HarfRust` returns glyphs in visual order, so we need to process them as such while
        // maintaining logical ordering of clusters.

        let glyph_infos = glyph_buffer.glyph_infos();
        if glyph_infos.is_empty() {
            for &(box_index, surrounding_level, _) in transparent_inline_boxes {
                self.push_inline_box(box_index, surrounding_level);
            }
            return;
        }
        let glyph_positions = glyph_buffer.glyph_positions();
        let cluster_range_start = self.clusters.len();
        let is_rtl = bidi_level & 1 == 1;
        if !is_rtl {
            run.advance = process_clusters(
                Direction::Ltr,
                &mut self.clusters,
                &mut self.glyphs,
                scale_factor,
                advance_projection.as_ref(),
                self.nominal_font_metric_line_breaks,
                glyph_infos,
                glyph_positions,
                char_infos,
                source_text.char_indices(),
            );
        } else {
            run.advance = process_clusters(
                Direction::Rtl,
                &mut self.clusters,
                &mut self.glyphs,
                scale_factor,
                advance_projection.as_ref(),
                self.nominal_font_metric_line_breaks,
                glyph_infos,
                glyph_positions,
                char_infos,
                source_text.char_indices().rev(),
            );
            // Reverse clusters into logical order for RTL
            let clusters_len = self.clusters.len();
            self.clusters[cluster_range_start..clusters_len].reverse();
        }

        run.cluster_range = cluster_range_start..self.clusters.len();
        if !run.cluster_range.is_empty() {
            self.push_shaped_run(run, transparent_inline_boxes);
        }
    }

    fn push_shaped_run(&mut self, run: RunData, transparent_inline_boxes: &[(usize, u8, usize)]) {
        let run_index = self.runs.len();
        let run_text_range = run.text_range.clone();
        let run_cluster_range = run.cluster_range.clone();
        let bidi_level = run.bidi_level;
        self.runs.push(run);

        let mut text_start = run_text_range.start;
        let mut cluster_start = run_cluster_range.start;
        for &(box_index, surrounding_level, boundary) in transparent_inline_boxes {
            if !(run_text_range.start..=run_text_range.end).contains(&boundary) {
                continue;
            }
            let run = &self.runs[run_index];
            let cluster_end = self.clusters[cluster_start..run_cluster_range.end]
                .iter()
                .position(|cluster| cluster.text_range(run).start >= boundary)
                .map_or(run_cluster_range.end, |offset| cluster_start + offset);
            if text_start < boundary || cluster_start < cluster_end {
                self.items.push(LayoutItem {
                    kind: LayoutItemKind::TextRun,
                    index: run_index,
                    bidi_level,
                    text_range: text_start..boundary,
                    cluster_range: cluster_start..cluster_end,
                });
            }
            self.push_inline_box(box_index, surrounding_level);
            text_start = boundary;
            cluster_start = cluster_end;
        }
        if text_start < run_text_range.end || cluster_start < run_cluster_range.end {
            self.items.push(LayoutItem {
                kind: LayoutItemKind::TextRun,
                index: run_index,
                bidi_level,
                text_range: text_start..run_text_range.end,
                cluster_range: cluster_start..run_cluster_range.end,
            });
        }
    }

    pub(crate) fn finish(&mut self) {
        for run in &self.runs {
            let word = run.word_spacing;
            let letter = run.letter_spacing;
            if nearly_zero(word) && nearly_zero(letter) {
                continue;
            }
            let clusters = &mut self.clusters[run.cluster_range.clone()];
            for cluster in clusters {
                let mut spacing = if cluster.info.is_default_ignorable() {
                    0.0
                } else {
                    letter
                };
                if !nearly_zero(word) && cluster.info.whitespace().is_space_or_nbsp() {
                    spacing += word;
                }
                if !nearly_zero(spacing) {
                    cluster.advance += spacing;
                    cluster.line_break_advance += spacing;
                    if cluster.glyph_len != 0xFF {
                        let start = run.glyph_start + cluster.glyph_offset as usize;
                        let end = start + cluster.glyph_len as usize;
                        let glyphs = &mut self.glyphs[start..end];
                        if let Some(last) = glyphs.last_mut() {
                            last.advance += spacing;
                        }
                    }
                }
            }
        }
        // Set default tab advances based on tab_size and the run's space advance.
        // This provides a reasonable default for intrinsic sizing; the line breaker
        // will override with position-dependent values during layout.
        for run in &self.runs {
            let space_advance = run.metrics.space_advance;
            let space_glyph_id = run.metrics.space_glyph_id;
            let cluster_range = run.cluster_range.clone();
            let glyph_start = run.glyph_start;

            // All clusters in a shaping run share the same style, so reading
            // tab_size from the first cluster's style is sufficient.
            let tab_size = self
                .clusters
                .get(cluster_range.start)
                .map(|c| self.styles[c.style_index as usize].tab_size)
                .unwrap_or_default();
            let default_advance = tab_size.interval(space_advance);

            for cluster in &mut self.clusters[cluster_range] {
                if cluster.info.whitespace() == Whitespace::Tab {
                    if default_advance > 0.0 {
                        let delta = default_advance - cluster.advance;
                        cluster.advance = default_advance;
                        // Update glyph advance to match.
                        if cluster.glyph_len != 0xFF && !nearly_zero(delta) {
                            let start = glyph_start + cluster.glyph_offset as usize;
                            let end = start + cluster.glyph_len as usize;
                            if let Some(last) = self.glyphs[start..end].last_mut() {
                                last.advance += delta;
                            }
                        }
                    } else {
                        cluster.advance = 0.0;
                    }
                    // Replace the .notdef glyph (ID 0) that the shaper emits
                    // for U+0009 with the space glyph.  Tab is whitespace: it
                    // contributes advance but must not render a visible glyph.
                    if cluster.glyph_len == 0xFF {
                        // Single glyph stored inline: glyph_offset IS the ID.
                        cluster.glyph_offset = space_glyph_id;
                    } else {
                        let start = glyph_start + cluster.glyph_offset as usize;
                        let end = start + cluster.glyph_len as usize;
                        for glyph in &mut self.glyphs[start..end] {
                            glyph.id = space_glyph_id;
                        }
                    }
                }
            }
        }
    }

    // TODO: this method does not handle mixed direction text at all.
    pub(crate) fn calculate_content_widths(&self) -> ContentWidths {
        fn hanging_whitespace_advance<B: Brush>(
            cluster: Option<&ClusterData>,
            styles: &[Style<B>],
        ) -> f32 {
            cluster
                .filter(|cluster| {
                    cluster.info.whitespace().is_space_or_nbsp()
                        && styles[cluster.style_index as usize].white_space_collapse
                            != WhiteSpaceCollapse::BreakSpaces
                })
                .map_or(0.0, |cluster| cluster.advance)
        }

        let mut min_width = 0.0_f32;
        let mut max_width = 0.0_f32;

        let mut running_min_width = 0.0;
        let mut running_max_width = 0.0;
        let mut text_wrap_mode = TextWrapMode::Wrap;
        let mut prev_cluster: Option<&ClusterData> = None;
        let mut projected_source_boundary: Option<ProjectedSourceBoundary> = None;
        let is_rtl = self.base_level & 1 == 1;
        for (item_index, item) in self.items.iter().enumerate() {
            match item.kind {
                LayoutItemKind::TextRun => {
                    let run = &self.runs[item.index];
                    let clusters = &self.clusters[item.cluster_range.clone()];
                    if is_rtl {
                        prev_cluster = clusters.first();
                    }
                    for cluster in clusters {
                        let boundary = cluster.info.boundary();
                        let byte_index = cluster.text_range(run).start;
                        let boundary_override = self
                            .line_break_overrides
                            .binary_search_by_key(&byte_index, |entry| entry.byte_index())
                            .ok()
                            .map(|index| self.line_break_overrides[index].disposition());
                        let style = &self.styles[cluster.style_index as usize];
                        let prev_text_wrap_mode = text_wrap_mode;
                        text_wrap_mode = style.text_wrap_mode;
                        let source_boundary_was_projected = projected_source_boundary
                            .is_some_and(|projection| projection.suppresses(byte_index));
                        if projected_source_boundary
                            .is_some_and(|projection| projection.target() <= byte_index)
                        {
                            projected_source_boundary = None;
                        }
                        let resolved_source_opportunity = matches!(
                            boundary_override,
                            Some(
                                LineBreakOverrideDisposition::ResolvedCollapsedSourceOpportunity
                                    | LineBreakOverrideDisposition::ResolvedRetainedSourceOpportunity
                            )
                        );
                        let style_resolved_opportunity = !matches!(
                            boundary_override,
                            Some(LineBreakOverrideDisposition::Suppress)
                        ) && (matches!(
                            boundary_override,
                            Some(
                                LineBreakOverrideDisposition::NormalOpportunity
                                    | LineBreakOverrideDisposition::UnprioritizedOpportunity
                            )
                        ) || boundary == Boundary::Line
                            || style.overflow_wrap == OverflowWrap::Anywhere);
                        if boundary == Boundary::Mandatory
                            || (!source_boundary_was_projected
                                && (resolved_source_opportunity
                                    || (prev_text_wrap_mode == TextWrapMode::Wrap
                                        && style_resolved_opportunity)))
                        {
                            let trailing_whitespace =
                                hanging_whitespace_advance(prev_cluster, &self.styles);
                            min_width = min_width.max(running_min_width - trailing_whitespace);
                            running_min_width = 0.0;
                            if boundary == Boundary::Mandatory {
                                max_width = max_width.max(running_max_width - trailing_whitespace);
                                running_max_width = 0.0;
                            }
                        }
                        running_min_width += cluster.advance;
                        running_max_width += cluster.advance;
                        if !is_rtl {
                            prev_cluster = Some(cluster);
                        }
                    }
                    let trailing_whitespace =
                        hanging_whitespace_advance(prev_cluster, &self.styles);
                    min_width = min_width.max(running_min_width - trailing_whitespace);
                }
                LayoutItemKind::InlineBox => {
                    let ibox = &self.inline_boxes[item.index];
                    let width = ibox.width();
                    running_max_width += width;
                    match ibox.line_break_participation() {
                        InlineBoxLineBreakParticipation::Atomic(break_affinity) => {
                            let can_wrap = text_wrap_mode == TextWrapMode::Wrap;
                            if can_wrap && break_affinity.allows_break_before() {
                                let trailing_whitespace =
                                    hanging_whitespace_advance(prev_cluster, &self.styles);
                                min_width = min_width.max(running_min_width - trailing_whitespace);
                                running_min_width = 0.0;
                            }
                            running_min_width += width;
                            if can_wrap && break_affinity.allows_break_after() {
                                min_width = min_width.max(running_min_width);
                                running_min_width = 0.0;
                            }
                            prev_cluster = None;
                        }
                        InlineBoxLineBreakParticipation::LogicalOwnerEdge(edge) => {
                            let source_projection = edge.source_projection(width);
                            let boundary =
                                self.source_soft_wrap_boundary_after(item_index, ibox.index, edge);
                            let projection =
                                boundary.projection_from(ibox.index, edge.following_source_space());
                            let project = projection.is_some()
                                && source_projection != LogicalInlineEdgeSourceProjection::Absent
                                && (source_projection
                                    == LogicalInlineEdgeSourceProjection::AfterGeometry
                                    || projected_source_boundary
                                        .map(ProjectedSourceBoundary::target)
                                        != projection.map(ProjectedSourceBoundary::target))
                                && boundary.is_available_from(text_wrap_mode);
                            if source_projection
                                == LogicalInlineEdgeSourceProjection::BeforeGeometry
                                && project
                            {
                                let trailing_whitespace =
                                    hanging_whitespace_advance(prev_cluster, &self.styles);
                                min_width = min_width.max(running_min_width - trailing_whitespace);
                                running_min_width = 0.0;
                                projected_source_boundary = projection;
                            }
                            running_min_width += width;
                            if source_projection == LogicalInlineEdgeSourceProjection::AfterGeometry
                                && project
                            {
                                min_width = min_width.max(running_min_width);
                                running_min_width = 0.0;
                                projected_source_boundary = projection;
                            }
                        }
                        InlineBoxLineBreakParticipation::TransparentAnchor => {}
                    }
                }
            }
            let trailing_whitespace = hanging_whitespace_advance(prev_cluster, &self.styles);
            max_width = max_width.max(running_max_width - trailing_whitespace);
        }

        let trailing_whitespace = hanging_whitespace_advance(prev_cluster, &self.styles);
        min_width = min_width.max(running_min_width - trailing_whitespace);

        ContentWidths {
            min: min_width,
            max: max_width,
        }
    }
}

/// Processes shaped glyphs from `HarfRust` and converts them into `ClusterData` and `Glyph`.
///
/// # Parameters
///
/// ## Output Parameters (mutated by this function):
/// * `clusters` - Vector where new `ClusterData` entries will be pushed.
/// * `glyphs` - Vector where new `Glyph` entries will be pushed. Note: single-glyph clusters
///   with zero offsets may be inlined directly into `ClusterData`.
///
/// ## Input Parameters:
/// * `direction` - Direction of the text.
/// * `scale_factor` - Scaling factor used to convert font units to the target size.
/// * `glyph_infos` - `HarfRust` glyph information in visual order.
/// * `glyph_positions` - `HarfRust` glyph positioning data in visual order.
/// * `char_infos` - Character information from text analysis, indexed by cluster ID.
/// * `char_indices_iter` - Iterator over (`byte_offset`, `char`) pairs from the source text.
///   Should be in logical order (forward for LTR, reverse for RTL).
fn process_clusters<I: Iterator<Item = (usize, char)>>(
    direction: Direction,
    clusters: &mut Vec<ClusterData>,
    glyphs: &mut Vec<Glyph>,
    scale_factor: f32,
    advance_projection: Option<&FontMetricAdvanceProjection<'_>>,
    nominal_font_metric_line_breaks: bool,
    glyph_infos: &[harfrust::GlyphInfo],
    glyph_positions: &[harfrust::GlyphPosition],
    char_infos: &[(CharInfo, u16)],
    char_indices_iter: I,
) -> f32 {
    let mut char_indices_iter = char_indices_iter.peekable();
    let mut cluster_start_char = char_indices_iter.next().unwrap();
    let mut total_glyphs: u32 = 0;
    let mut cluster_glyph_offset: u32 = 0;
    let start_cluster_id = glyph_infos.first().unwrap().cluster;
    let mut cluster_id = start_cluster_id;
    let mut char_info = char_infos[cluster_id as usize];
    let mut run_advance = 0.0;
    let mut cluster_advance = 0.0;
    let mut cluster_line_break_advance = 0.0;
    // If the current cluster might be a single-glyph, zero-offset cluster, we defer
    // pushing the first glyph to `glyphs` because it might be inlined into `ClusterData`.
    let mut pending_inline_glyph: Option<Glyph> = None;

    // The mental model for understanding this function is best grasped by first reading
    // the HarfBuzz docs on [clusters](https://harfbuzz.github.io/working-with-harfbuzz-clusters.html).
    //
    // `num_components` is the number of characters in the current cluster. Since source text's characters
    // were inserted into `HarfRust`'s buffer using their logical indices as the cluster ID, `HarfRust` will
    // assign the first character's cluster ID (in logical order) to the merged cluster because the minimum
    // ID is selected for [merging](https://github.com/harfbuzz/harfrust/blob/a38025fb336230b492366740c86021bb406bcd0d/src/hb/buffer.rs#L920-L924).
    //
    //  So, the number of components in a given cluster is dependent on `direction`.
    //   - In LTR, `num_components` is the difference between the next cluster and the current cluster.
    //   - In RTL, `num_components` is the difference between the last cluster and the current cluster.
    // This is because we must compare the current cluster to its next larger ID (in other words, the next
    // logical index, which is visually downstream in LTR and visually upstream in RTL).
    //
    // For example, consider the LTR text for "afi" where "fi" form a ligature.
    //   Initial cluster values: 0, 1, 2 (logical + visual order)
    //   `HarfRust` assignation: 0, 1, 1
    //   Cluster count:          2
    //   `num_components`:       (1 - 0 =) 1, (3 - 1 =) 2
    //
    // Now consider the RTL text for "حداً".
    //   Initial cluster values:  0, 1, 2, 3 (logical, or in-memory, order)
    //   Reversed cluster values: 3, 2, 1, 0 (visual order - the return order of `HarfRust` for RTL)
    //   `HarfRust` assignation:  3, 2, 0, 0
    //   Cluster count:           3
    //   `num_components`:        (4 - 3 =) 1, (3 - 2 =) 1, (2 - 0 =) 2
    let num_components =
        |next_cluster: u32, current_cluster: u32, last_cluster: u32| match direction {
            Direction::Ltr => next_cluster - current_cluster,
            Direction::Rtl => last_cluster - current_cluster,
        };
    let mut last_cluster_id: u32 = match direction {
        Direction::Ltr => 0,
        Direction::Rtl => char_infos.len() as u32,
    };

    for (glyph_info, glyph_pos) in glyph_infos.iter().zip(glyph_positions.iter()) {
        // Flush previous cluster if we've reached a new cluster
        if cluster_id != glyph_info.cluster {
            run_advance += cluster_advance;
            let num_components = num_components(glyph_info.cluster, cluster_id, last_cluster_id);
            cluster_advance /= num_components as f32;
            cluster_line_break_advance /= num_components as f32;
            let is_newline = to_whitespace(cluster_start_char.1) == Whitespace::Newline;
            let cluster_type = if num_components > 1 {
                debug_assert!(!is_newline);
                ClusterType::LigatureStart
            } else if is_newline {
                ClusterType::Newline
            } else {
                ClusterType::Regular
            };

            let inline_glyph_id = if matches!(cluster_type, ClusterType::Regular) {
                pending_inline_glyph.take().map(|g| g.id)
            } else {
                // This isn't a regular cluster, so we don't inline the glyph and push
                // it to `glyphs`.
                if let Some(pending) = pending_inline_glyph.take() {
                    glyphs.push(pending);
                    total_glyphs += 1;
                }
                None
            };

            push_cluster(
                clusters,
                char_info,
                cluster_start_char,
                cluster_glyph_offset,
                cluster_advance,
                cluster_line_break_advance,
                total_glyphs,
                cluster_type,
                inline_glyph_id,
            );
            cluster_glyph_offset = total_glyphs;

            if num_components > 1 {
                // Skip characters until we reach the current cluster
                for i in 1..num_components {
                    cluster_start_char = char_indices_iter.next().unwrap();
                    if to_whitespace(cluster_start_char.1) == Whitespace::Space {
                        break;
                    }
                    let char_info_ = match direction {
                        Direction::Ltr => char_infos[(cluster_id + i) as usize],
                        Direction::Rtl => char_infos[(cluster_id + num_components - i) as usize],
                    };
                    push_cluster(
                        clusters,
                        char_info_,
                        cluster_start_char,
                        cluster_glyph_offset,
                        cluster_advance,
                        cluster_line_break_advance,
                        total_glyphs,
                        ClusterType::LigatureComponent,
                        None,
                    );
                }
            }
            cluster_start_char = char_indices_iter.next().unwrap();

            cluster_advance = 0.0;
            cluster_line_break_advance = 0.0;
            last_cluster_id = cluster_id;
            cluster_id = glyph_info.cluster;
            char_info = char_infos[cluster_id as usize];
            pending_inline_glyph = None;
        }

        let shaped_advance = (glyph_pos.x_advance as f32) * scale_factor;
        let glyph_id = skrifa::GlyphId::new(glyph_info.glyph_id);
        let advance = advance_projection
            .filter(|_| !nearly_zero(shaped_advance))
            .map_or(shaped_advance, |projection| {
                projection.project(glyph_id, shaped_advance)
            });
        let line_break_advance = if nominal_font_metric_line_breaks && !nearly_zero(shaped_advance)
        {
            advance_projection.map_or(advance, |projection| projection.nominal(glyph_id))
        } else {
            advance
        };
        let glyph = Glyph {
            id: glyph_info.glyph_id,
            style_index: char_info.1,
            x: (glyph_pos.x_offset as f32) * scale_factor,
            // Convert from font space (Y-up) to layout space (Y-down)
            y: -(glyph_pos.y_offset as f32) * scale_factor,
            advance,
        };
        cluster_advance += glyph.advance;
        cluster_line_break_advance += line_break_advance;
        // Push any pending glyph. If it was a zero-offset, single glyph cluster, it would
        // have been pushed in the first `if` block.
        if let Some(pending) = pending_inline_glyph.take() {
            glyphs.push(pending);
            total_glyphs += 1;
        }
        if total_glyphs == cluster_glyph_offset && glyph.x == 0.0 && glyph.y == 0.0 {
            // Defer this potential zero-offset, single glyph cluster
            pending_inline_glyph = Some(glyph);
        } else {
            glyphs.push(glyph);
            total_glyphs += 1;
        }
    }

    // Push the last cluster
    {
        // See comment above `num_components` for why we use `char_infos.len()` for LTR and 0 for RTL.
        let next_cluster_id = match direction {
            Direction::Ltr => char_infos.len() as u32,
            Direction::Rtl => 0,
        };
        let num_components = num_components(next_cluster_id, cluster_id, last_cluster_id);
        if num_components > 1 {
            // This is a ligature - create ligature start + ligature components

            if let Some(pending) = pending_inline_glyph.take() {
                glyphs.push(pending);
                total_glyphs += 1;
            }
            let ligature_advance = cluster_advance / num_components as f32;
            let ligature_line_break_advance = cluster_line_break_advance / num_components as f32;
            push_cluster(
                clusters,
                char_info,
                cluster_start_char,
                cluster_glyph_offset,
                ligature_advance,
                ligature_line_break_advance,
                total_glyphs,
                ClusterType::LigatureStart,
                None,
            );

            cluster_glyph_offset = total_glyphs;

            // Create ligature component clusters for the remaining characters
            let mut i = 1;
            for char in char_indices_iter {
                if to_whitespace(char.1) == Whitespace::Space {
                    break;
                }
                let component_char_info = match direction {
                    Direction::Ltr => char_infos[(cluster_id + i) as usize],
                    Direction::Rtl => char_infos[(cluster_id + num_components - i) as usize],
                };
                push_cluster(
                    clusters,
                    component_char_info,
                    char,
                    cluster_glyph_offset,
                    ligature_advance,
                    ligature_line_break_advance,
                    total_glyphs,
                    ClusterType::LigatureComponent,
                    None,
                );
                i += 1;
            }
        } else {
            let is_newline = to_whitespace(cluster_start_char.1) == Whitespace::Newline;
            let cluster_type = if is_newline {
                ClusterType::Newline
            } else {
                ClusterType::Regular
            };
            let mut inline_glyph_id = None;
            match cluster_type {
                ClusterType::Regular => {
                    if total_glyphs == cluster_glyph_offset {
                        if let Some(pending) = pending_inline_glyph.take() {
                            inline_glyph_id = Some(pending.id);
                        }
                    }
                }
                _ => {
                    if let Some(pending) = pending_inline_glyph.take() {
                        glyphs.push(pending);
                        total_glyphs += 1;
                    }
                }
            }
            push_cluster(
                clusters,
                char_info,
                cluster_start_char,
                cluster_glyph_offset,
                cluster_advance,
                cluster_line_break_advance,
                total_glyphs,
                cluster_type,
                inline_glyph_id,
            );
        }
    }

    run_advance
}

/// Projects only the base font-metric contribution to a fixed em grid.
/// Shaping adjustments remain intact because the residual is removed from
/// the already-shaped advance instead of quantizing that final advance.
struct FontMetricAdvanceProjection<'a> {
    glyph_metrics: skrifa::metrics::GlyphMetrics<'a>,
    units_per_em: f64,
    font_size: f64,
    denominator: f64,
}

impl<'a> FontMetricAdvanceProjection<'a> {
    fn new(
        font: &skrifa::FontRef<'a>,
        coords: &'a [skrifa::prelude::NormalizedCoord],
        units_per_em: u16,
        font_size: f32,
        denominator: NonZeroU16,
    ) -> Self {
        Self {
            glyph_metrics: skrifa::metrics::GlyphMetrics::new(
                font,
                skrifa::prelude::Size::unscaled(),
                coords,
            ),
            units_per_em: f64::from(units_per_em),
            font_size: f64::from(font_size),
            denominator: f64::from(denominator.get()),
        }
    }

    fn project(&self, glyph_id: skrifa::GlyphId, shaped_advance: f32) -> f32 {
        let Some(metric_units) = self.glyph_metrics.advance_width(glyph_id) else {
            return shaped_advance;
        };
        let metric_em = f64::from(metric_units) / self.units_per_em;
        let projected_em = (metric_em * self.denominator).floor() / self.denominator;
        let residual = (metric_em - projected_em) * self.font_size;
        shaped_advance - residual as f32
    }

    fn nominal(&self, glyph_id: skrifa::GlyphId) -> f32 {
        let Some(metric_units) = self.glyph_metrics.advance_width(glyph_id) else {
            return 0.0;
        };
        let metric_em = f64::from(metric_units) / self.units_per_em;
        let projected_em = (metric_em * self.denominator).floor() / self.denominator;
        (projected_em * self.font_size) as f32
    }
}

#[derive(PartialEq)]
enum Direction {
    Ltr,
    Rtl,
}

enum ClusterType {
    LigatureStart,
    LigatureComponent,
    Regular,
    Newline,
}

impl From<&ClusterType> for u16 {
    fn from(cluster_type: &ClusterType) -> Self {
        match cluster_type {
            ClusterType::LigatureStart => ClusterData::LIGATURE_START,
            ClusterType::LigatureComponent => ClusterData::LIGATURE_COMPONENT,
            ClusterType::Regular | ClusterType::Newline => 0, // No special flags
        }
    }
}

fn push_cluster(
    clusters: &mut Vec<ClusterData>,
    char_info: (CharInfo, u16),
    cluster_start_char: (usize, char),
    glyph_offset: u32,
    advance: f32,
    line_break_advance: f32,
    total_glyphs: u32,
    cluster_type: ClusterType,
    inline_glyph_id: Option<u32>,
) {
    let glyph_len = (total_glyphs - glyph_offset) as u8;

    let (final_glyph_len, final_glyph_offset, final_advance, final_line_break_advance) =
        match cluster_type {
            ClusterType::LigatureComponent => {
                // Ligature components have no glyphs, only advance.
                debug_assert_eq!(glyph_len, 0);
                (0_u8, 0_u32, advance, line_break_advance)
            }
            ClusterType::Newline => {
                // Newline clusters are stripped of their glyph contribution.
                debug_assert_eq!(glyph_len, 1);
                (0_u8, 0_u32, 0.0, 0.0)
            }
            _ if inline_glyph_id.is_some() => {
                // Inline glyphs are stored inline within `ClusterData`
                debug_assert_eq!(glyph_len, 0);
                (
                    0xFF_u8,
                    inline_glyph_id.unwrap(),
                    advance,
                    line_break_advance,
                )
            }
            ClusterType::Regular | ClusterType::LigatureStart => {
                // Regular and ligature start clusters maintain their glyphs and advance.
                debug_assert_ne!(glyph_len, 0);
                (glyph_len, glyph_offset, advance, line_break_advance)
            }
        };

    clusters.push(ClusterData {
        info: ClusterInfo::new(
            char_info.0.boundary,
            cluster_start_char.1,
            char_info.0.authored_break_unit,
        ),
        flags: (&cluster_type).into(),
        style_index: char_info.1,
        glyph_len: final_glyph_len,
        text_len: cluster_start_char.1.len_utf8() as u8,
        glyph_offset: final_glyph_offset,
        text_offset: cluster_start_char.0 as u16,
        advance: final_advance,
        line_break_advance: final_line_break_advance,
    });
}
