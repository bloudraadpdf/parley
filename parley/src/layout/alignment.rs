// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{
    BreakReason,
    data::{ClusterData, LineItemData},
};
use crate::data::LayoutData;
use crate::style::Brush;

/// Alignment of a layout.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Alignment {
    /// This is [`Alignment::Left`] for LTR text and [`Alignment::Right`] for RTL text.
    #[default]
    Start,
    /// This is [`Alignment::Right`] for LTR text and [`Alignment::Left`] for RTL text.
    End,
    /// Align content to the left edge.
    ///
    /// For alignment that should be aware of text direction, use [`Alignment::Start`] or
    /// [`Alignment::End`] instead.
    Left,
    /// Align each line centered within the container.
    Center,
    /// Align content to the right edge.
    ///
    /// For alignment that should be aware of text direction, use [`Alignment::Start`] or
    /// [`Alignment::End`] instead.
    Right,
    /// Justify each line by spacing out content, except for the last line.
    Justify,
}

/// Policy used when [`Alignment::Justify`] expands line advances.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum JustificationMode {
    /// Expand inter-word separators only.
    #[default]
    InterWord,
    /// Expand advances between eligible non-whitespace clusters.
    InterCharacter,
    /// Do not expand cluster advances; fall back to start alignment.
    None,
}

/// Additional options to fine tune alignment
#[derive(Debug, Clone, Copy)]
pub struct AlignmentOptions {
    /// If set to `true`, "end" and "center" alignment will apply even if the line contents are
    /// wider than the alignment width. If it is set to `false`, all overflowing lines will be
    /// [`Alignment::Start`] aligned.
    pub align_when_overflowing: bool,
    /// Which justification strategy to apply when [`Alignment::Justify`] is requested.
    pub justification_mode: JustificationMode,
}

#[expect(
    clippy::derivable_impls,
    reason = "Make default values explicit rather than relying on the implicit default value of bool"
)]
impl Default for AlignmentOptions {
    fn default() -> Self {
        Self {
            align_when_overflowing: false,
            justification_mode: JustificationMode::InterWord,
        }
    }
}

/// Align the layout.
///
/// If [`Alignment::Justify`] is requested, clusters' [`ClusterData::advance`] will be adjusted.
/// Prior to re-line-breaking or re-aligning, [`unjustify`] has to be called.
pub(crate) fn align<B: Brush>(
    layout: &mut LayoutData<B>,
    alignment_width: Option<f32>,
    alignment: Alignment,
    options: AlignmentOptions,
) {
    #[cfg(feature = "accesskit")]
    {
        layout.alignment = Some(alignment);
    }
    layout.alignment_width = alignment_width.unwrap_or(layout.width);
    layout.per_line_alignment_widths.clear();
    layout.aligned_justification_mode = if alignment == Alignment::Justify {
        match options.justification_mode {
            JustificationMode::None => None,
            mode => Some(mode),
        }
    } else {
        None
    };
    layout.is_aligned_justified = layout.aligned_justification_mode.is_some();

    align_impl::<_, false>(layout, alignment, options);
}

/// Align the layout with per-line alignment widths.
///
/// Each line in `alignment_widths` overrides the layout's single-width
/// [`crate::Layout::align`] alignment_width for that line. Lines beyond
/// `alignment_widths.len()` fall back to the LAST per-line width (which is
/// also stored as `LayoutData::alignment_width` for downstream readers).
/// Pass an empty slice to fall back fully to single-width behaviour using
/// `layout.width` as the alignment width.
///
/// Used by peedeeef's CSS 2.1 §9.5 Rule 9 line-box shortening: lines
/// adjacent to floats are broken at narrow band-widths and must justify
/// against THAT narrow width, not the paragraph's full max-advance.
pub(crate) fn align_per_line<B: Brush>(
    layout: &mut LayoutData<B>,
    alignment_widths: &[f32],
    alignment: Alignment,
    options: AlignmentOptions,
) {
    let canonical_width = alignment_widths.last().copied().unwrap_or(layout.width);
    layout.alignment_width = canonical_width;
    layout.per_line_alignment_widths.clear();
    layout
        .per_line_alignment_widths
        .extend_from_slice(alignment_widths);
    layout.aligned_justification_mode = if alignment == Alignment::Justify {
        match options.justification_mode {
            JustificationMode::None => None,
            mode => Some(mode),
        }
    } else {
        None
    };
    layout.is_aligned_justified = layout.aligned_justification_mode.is_some();

    align_impl::<_, false>(layout, alignment, options);
}

/// Removes previous justification applied to clusters.
///
/// This is part of resetting state in preparation for re-line-breaking or re-aligning the same
/// layout.
pub(crate) fn unjustify<B: Brush>(layout: &mut LayoutData<B>) {
    if let Some(mode) = layout.aligned_justification_mode {
        align_impl::<_, true>(
            layout,
            Alignment::Justify,
            AlignmentOptions {
                justification_mode: mode,
                ..AlignmentOptions::default()
            },
        );
        layout.is_aligned_justified = false;
        layout.aligned_justification_mode = None;
    }
}

/// The actual alignment implementation.
///
/// This is const-generic over `UNDO_JUSTIFICATION`: justified alignment adjusts clusters'
/// [`ClusterData::advance`], and this mutation has to be undone for re-line-breaking or
/// re-aligning. `UNDO_JUSTIFICATION` indicates whether the adjustment has to be applied, or
/// undone.
///
/// Writing a separate function for undoing justification would be faster, but not by much, and
/// doing it this way we are sure the calculations performed are equivalent.
fn align_impl<B: Brush, const UNDO_JUSTIFICATION: bool>(
    layout: &mut LayoutData<B>,
    alignment: Alignment,
    options: AlignmentOptions,
) {
    // Whether the text base direction is right-to-left.
    let is_rtl = layout.base_level & 1 == 1;

    // Apply alignment to line items
    for line_index in 0..layout.lines.len() {
        let (indent, line_advance, trailing_whitespace, break_reason, num_spaces, item_range) = {
            let line = &layout.lines[line_index];
            (
                line.indent,
                line.metrics.advance,
                line.metrics.trailing_whitespace,
                line.break_reason,
                line.num_spaces,
                line.item_range.clone(),
            )
        };

        if is_rtl {
            // In RTL text, trailing whitespace is on the left. As we hang that whitespace, offset
            // the line to the left. Note: indent is not subtracted here because `free_space` below
            // already accounts for it.
            layout.lines[line_index].metrics.offset = -trailing_whitespace;
        } else {
            layout.lines[line_index].metrics.offset = indent;
        }

        // Per-line alignment width override (peedeeef CSS 2.1 §9.5 Rule 9).
        // Falls back to `layout.alignment_width` when no override is set
        // for this line.
        let alignment_width = layout
            .per_line_alignment_widths
            .get(line_index)
            .copied()
            .unwrap_or(layout.alignment_width);

        // Compute free space.
        let free_space = alignment_width - indent - line_advance + trailing_whitespace;

        if !options.align_when_overflowing && free_space <= 0.0 {
            if is_rtl {
                // In RTL text, right-align on overflow.
                layout.lines[line_index].metrics.offset += free_space;
            }
            continue;
        }

        match (alignment, is_rtl) {
            (Alignment::Left, _) | (Alignment::Start, false) | (Alignment::End, true) => {
                // Do nothing
            }
            (Alignment::Right, _) | (Alignment::Start, true) | (Alignment::End, false) => {
                layout.lines[line_index].metrics.offset += free_space;
            }
            (Alignment::Center, _) => {
                layout.lines[line_index].metrics.offset += free_space * 0.5;
            }
            (Alignment::Justify, _) => {
                // Justified alignment doesn't have any effect if free_space is negative or zero
                if free_space <= 0.0 {
                    continue;
                }

                let justification_mode = options.justification_mode;
                if justification_mode == JustificationMode::None {
                    if is_rtl {
                        layout.lines[line_index].metrics.offset += free_space;
                    }
                    continue;
                }

                let opportunities = match justification_mode {
                    JustificationMode::InterWord => num_spaces,
                    JustificationMode::InterCharacter => count_inter_character_opportunities(
                        &layout.line_items[item_range.clone()],
                        &layout.clusters,
                        is_rtl,
                    ),
                    JustificationMode::None => 0,
                };

                // Justified alignment doesn't apply to the last line of a paragraph
                // (`BreakReason::None`), (`BreakReason::Explicit`) or if there are no whitespace
                // gaps to adjust. In that case, start-align, i.e., left-align for LTR text and
                // right-align for RTL text.
                if matches!(break_reason, BreakReason::None | BreakReason::Explicit)
                    || opportunities == 0
                {
                    if is_rtl {
                        layout.lines[line_index].metrics.offset += free_space;
                    }
                    continue;
                }

                let adjustment =
                    free_space / opportunities as f32 * if UNDO_JUSTIFICATION { -1. } else { 1. };
                let mut applied = 0;
                let line_items = &layout.line_items[item_range];
                let line_items: &mut dyn Iterator<Item = &LineItemData> = if is_rtl {
                    &mut line_items.iter().rev()
                } else {
                    &mut line_items.iter()
                };
                for line_item in line_items.filter(|item| item.is_text_run()) {
                    let line_item_is_rtl = line_item.bidi_level & 1 != 0;
                    let clusters = &mut layout.clusters[line_item.cluster_range.clone()];
                    let clusters: &mut dyn Iterator<Item = &mut ClusterData> = if line_item_is_rtl {
                        &mut clusters.iter_mut().rev()
                    } else {
                        &mut clusters.iter_mut()
                    };
                    for cluster in clusters {
                        if applied == opportunities {
                            break;
                        }
                        if cluster_is_justification_opportunity(cluster, justification_mode) {
                            cluster.advance += adjustment;
                            applied += 1;
                        }
                    }
                }
            }
        }
    }
}

fn count_inter_character_opportunities(
    line_items: &[LineItemData],
    clusters: &[ClusterData],
    is_rtl: bool,
) -> usize {
    let mut eligible_clusters = 0usize;
    let line_items: &mut dyn Iterator<Item = &LineItemData> = if is_rtl {
        &mut line_items.iter().rev()
    } else {
        &mut line_items.iter()
    };

    line_items
        .filter(|item| item.is_text_run())
        .for_each(|line_item| {
            let clusters = &clusters[line_item.cluster_range.clone()];
            let line_item_is_rtl = line_item.bidi_level & 1 != 0;
            let clusters: &mut dyn Iterator<Item = &ClusterData> = if line_item_is_rtl {
                &mut clusters.iter().rev()
            } else {
                &mut clusters.iter()
            };
            clusters.for_each(|cluster| {
                if cluster_is_justification_opportunity(cluster, JustificationMode::InterCharacter)
                {
                    eligible_clusters += 1;
                }
            });
        });

    eligible_clusters.saturating_sub(1)
}

fn cluster_is_justification_opportunity(
    cluster: &ClusterData,
    justification_mode: JustificationMode,
) -> bool {
    match justification_mode {
        JustificationMode::InterWord => cluster.info.whitespace().is_space_or_nbsp(),
        JustificationMode::InterCharacter => !cluster.info.is_whitespace(),
        JustificationMode::None => false,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::{Alignment, AlignmentOptions, JustificationMode, align};
    use crate::analysis::{AuthoredBreakUnit, Boundary};
    use crate::layout::LineMetrics;
    use crate::layout::data::{
        BreakReason, ClusterData, ClusterInfo, LayoutData, LayoutItemKind, LineData, LineItemData,
    };

    fn test_cluster(ch: char, advance: f32) -> ClusterData {
        ClusterData {
            info: ClusterInfo::new(Boundary::Word, ch, AuthoredBreakUnit::Other),
            flags: 0,
            style_index: 0,
            glyph_len: 0xFF,
            text_len: ch.len_utf8() as u8,
            glyph_offset: 1,
            text_offset: 0,
            advance,
            line_break_advance: advance,
        }
    }

    fn test_layout(
        clusters: Vec<ClusterData>,
        line_advance: f32,
        num_spaces: usize,
    ) -> LayoutData<()> {
        let mut layout = LayoutData::default();
        layout.width = line_advance;
        layout.line_items.push(LineItemData {
            kind: LayoutItemKind::TextRun,
            index: 0,
            bidi_level: 0,
            advance: line_advance,
            is_whitespace: false,
            has_trailing_whitespace: false,
            text_range: 0..0,
            cluster_range: 0..clusters.len(),
        });
        layout.clusters = clusters;
        layout.lines.push(LineData {
            item_range: 0..1,
            metrics: LineMetrics {
                advance: line_advance,
                ..Default::default()
            },
            break_reason: BreakReason::Regular,
            max_advance: line_advance,
            num_spaces,
            indent: 0.0,
            ..Default::default()
        });
        layout
    }

    #[test]
    fn justify_none_does_not_expand_clusters() {
        let mut layout = test_layout(
            vec![
                test_cluster('a', 10.0),
                test_cluster(' ', 5.0),
                test_cluster('b', 15.0),
            ],
            30.0,
            1,
        );

        align(
            &mut layout,
            Some(50.0),
            Alignment::Justify,
            AlignmentOptions {
                justification_mode: JustificationMode::None,
                ..AlignmentOptions::default()
            },
        );

        assert_eq!(layout.clusters[0].advance, 10.0);
        assert_eq!(layout.clusters[1].advance, 5.0);
        assert_eq!(layout.clusters[2].advance, 15.0);
        assert_eq!(layout.lines[0].metrics.offset, 0.0);
    }

    #[test]
    fn inter_word_only_expands_spaces() {
        let mut layout = test_layout(
            vec![
                test_cluster('a', 10.0),
                test_cluster(' ', 5.0),
                test_cluster('b', 15.0),
            ],
            30.0,
            1,
        );

        align(
            &mut layout,
            Some(50.0),
            Alignment::Justify,
            AlignmentOptions {
                justification_mode: JustificationMode::InterWord,
                ..AlignmentOptions::default()
            },
        );

        assert_eq!(layout.clusters[0].advance, 10.0);
        assert_eq!(layout.clusters[1].advance, 25.0);
        assert_eq!(layout.clusters[2].advance, 15.0);
    }

    #[test]
    fn inter_character_expands_non_whitespace_gaps() {
        let mut layout = test_layout(
            vec![
                test_cluster('漢', 10.0),
                test_cluster('字', 10.0),
                test_cluster('語', 10.0),
            ],
            30.0,
            0,
        );

        align(
            &mut layout,
            Some(50.0),
            Alignment::Justify,
            AlignmentOptions {
                justification_mode: JustificationMode::InterCharacter,
                ..AlignmentOptions::default()
            },
        );

        assert_eq!(layout.clusters[0].advance, 20.0);
        assert_eq!(layout.clusters[1].advance, 20.0);
        assert_eq!(layout.clusters[2].advance, 10.0);
    }
}
