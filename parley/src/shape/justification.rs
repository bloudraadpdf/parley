// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;
use core::ops::Range;

use crate::{
    Brush, FontFeature,
    analysis::CharInfo,
    resolve::{ResolveContext, ResolvedStyle},
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SourceFontFeatures {
    pub(crate) source: Range<usize>,
    pub(crate) features: Vec<FontFeature>,
}

impl SourceFontFeatures {
    pub(super) fn for_segment<B: Brush>(
        context: &ResolveContext,
        styles: &[ResolvedStyle<B>],
        text: &str,
        source_start: usize,
        infos: &[(CharInfo, u16)],
    ) -> Vec<Self> {
        let mut characters = text.char_indices();
        let mut start = source_start;
        infos
            .chunk_by(|left, right| left.1 == right.1)
            .filter_map(|chunk| {
                let (offset, character) = characters.nth(chunk.len() - 1)?;
                let end = source_start + offset + character.len_utf8();
                let source = start..end;
                start = end;
                let features = context
                    .features(styles[usize::from(chunk[0].1)].font_features_for_justification)?;
                (!features.is_empty()).then(|| Self {
                    source,
                    features: features.to_vec(),
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeferredJustificationShape {
    pub(crate) source: alloc::sync::Arc<super::source::DeferredSourceShape>,
    pub(crate) prepared: Option<super::source::PreparedSourceShape>,
    pub(crate) alternatives: Vec<SourceFontFeatures>,
}

impl DeferredJustificationShape {
    fn eligible_features(
        &self,
        run: &crate::layout::data::RunData,
        clusters: &[crate::layout::data::ClusterData],
        has_opportunity: impl Fn(Range<usize>) -> bool,
    ) -> Vec<harfrust::Feature> {
        let mut features = self.source.features.clone();
        let mut cursor = 0;
        while cursor < clusters.len() {
            let start = cursor;
            let cluster = clusters[cursor];
            cursor += 1;
            if !cluster.is_ligature_start() && !cluster.is_ligature_component() {
                continue;
            }
            if run.bidi_level & 1 == 0 {
                while clusters
                    .get(cursor)
                    .is_some_and(|cluster| cluster.is_ligature_component())
                {
                    cursor += 1;
                }
            } else {
                while !clusters[cursor - 1].is_ligature_start() && cursor < clusters.len() {
                    cursor += 1;
                }
            }
            let source_start = clusters[start].text_range(run).start;
            let source_end = clusters[cursor - 1].text_range(run).end;
            if !has_opportunity(source_start..source_end) {
                continue;
            }
            let first_alternative = self
                .alternatives
                .partition_point(|alternative| alternative.source.end <= source_start);
            for alternative in self.alternatives[first_alternative..]
                .iter()
                .take_while(|alternative| alternative.source.start < source_end)
            {
                let start = source_start.max(alternative.source.start);
                let end = source_end.min(alternative.source.end);
                if start >= end {
                    continue;
                }
                let first = self
                    .source
                    .character_offsets
                    .binary_search(&(start - run.text_range.start))
                    .expect("retained character boundary");
                let last = self
                    .source
                    .character_offsets
                    .binary_search(&(end - run.text_range.start))
                    .expect("retained character boundary");
                features.extend(alternative.features.iter().map(|feature| {
                    harfrust::Feature::new(
                        harfrust::Tag::new(&feature.tag.to_bytes()),
                        feature.value as u32,
                        first..last,
                    )
                }));
            }
        }
        features
    }
}

impl<B: Brush> crate::LayoutContext<B> {
    /// Prepare optional shaping alternatives at eligible source boundaries.
    /// The original clusters and glyphs remain selected until line fitting.
    pub fn prepare_justification_shapes(&mut self, layout: &mut crate::Layout<B>) {
        layout.clear_justification_shape_selection();
        if layout.justification_opportunities().is_empty()
            || !layout.has_justification_shape_candidates()
        {
            return;
        }
        let source = layout
            .data
            .shaping_source_text
            .as_ref()
            .expect("retained shaping text");
        let mut scratch = crate::layout::data::LayoutData::<B>::default();
        scratch.styles.clone_from(&layout.data.styles);
        scratch.font_metric_advance_quantization = layout.data.font_metric_advance_quantization;
        scratch.nominal_font_metric_line_breaks = layout.data.nominal_font_metric_line_breaks;
        for index in 0..layout.data.deferred_justification_shapes.len() {
            let deferred = &layout.data.deferred_justification_shapes[index];
            let run = &layout.data.runs[deferred.source.run_index];
            let features = deferred.eligible_features(
                run,
                &layout.data.clusters[run.cluster_range.clone()],
                |source| {
                    layout
                        .data
                        .justification_opportunities
                        .text_boundaries(source)
                        .next()
                        .is_some()
                },
            );
            let prepared = (features != deferred.source.features).then(|| {
                deferred.source.shape_source_range(
                    &mut self.scx,
                    &mut scratch,
                    &layout.data,
                    source,
                    run.text_range.clone(),
                    deferred.source.context.clone(),
                    &features,
                )
            });
            layout.data.deferred_justification_shapes[index].prepared = prepared;
        }
    }
}

/// Select shaping for lines that receive character justification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JustificationShapePolicy {
    pub regular_lines: bool,
    pub terminal_lines: bool,
}

impl JustificationShapePolicy {
    pub(crate) fn expands(self, reason: crate::BreakReason) -> bool {
        if matches!(
            reason,
            crate::BreakReason::None | crate::BreakReason::Explicit
        ) {
            self.terminal_lines
        } else {
            self.regular_lines
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JustificationShapeVariants {
    pub(crate) original: Vec<crate::layout::data::ClusterData>,
    pub(crate) expanded: Vec<crate::layout::data::ClusterData>,
    pub(crate) original_glyph_len: usize,
    pub(crate) variant_glyph_len: usize,
    pub(crate) policy: JustificationShapePolicy,
}

impl<B: Brush> crate::Layout<B> {
    /// Returns the policy used by the active source shaping alternatives.
    pub fn justification_shape_policy(&self) -> Option<JustificationShapePolicy> {
        self.data
            .justification_shape_variants
            .as_ref()
            .map(|variants| variants.policy)
    }

    /// Use prepared alternatives when the selected line receives character spacing.
    pub fn set_justification_shape_policy(&mut self, policy: JustificationShapePolicy) {
        self.clear_justification_shape_selection();
        if policy == JustificationShapePolicy::default()
            || !self
                .data
                .deferred_justification_shapes
                .iter()
                .any(|candidate| candidate.prepared.is_some())
        {
            return;
        }
        let original_glyph_len = self.data.glyphs.len();
        let original = self.data.clusters.clone();
        let mut expanded = original.clone();
        for deferred in &self.data.deferred_justification_shapes {
            let Some(prepared) = &deferred.prepared else {
                continue;
            };
            let run = &self.data.runs[deferred.source.run_index];
            let glyph_offset = self.data.glyphs.len() - run.glyph_start;
            assert_eq!(
                run.cluster_range.len(),
                prepared.clusters.len(),
                "retained source cluster count"
            );
            for (target, candidate) in expanded[run.cluster_range.clone()]
                .iter_mut()
                .zip(&prepared.clusters)
            {
                assert_eq!(
                    (target.text_offset, target.text_len),
                    (candidate.text_offset, candidate.text_len),
                    "retained source cluster identity"
                );
                *target = *candidate;
                if target.glyph_len != 0xFF {
                    target.glyph_offset += glyph_offset as u32;
                }
            }
            self.data.glyphs.extend_from_slice(&prepared.glyphs);
        }
        crate::layout::apply_source_fit_projection(
            &self.data.runs,
            &mut expanded,
            &self.data.source_cluster_fit_advances,
        );
        self.data.justification_shape_variants = Some(JustificationShapeVariants {
            original,
            expanded,
            original_glyph_len,
            variant_glyph_len: self.data.glyphs.len(),
            policy,
        });
    }
}

pub(crate) struct SourceSuffixShaper<B: Brush> {
    context: super::ShapeContext,
    scratch: crate::layout::data::LayoutData<B>,
}

impl<B: Brush> SourceSuffixShaper<B> {
    pub(crate) fn new(data: &crate::layout::data::LayoutData<B>) -> Self {
        let mut scratch = crate::layout::data::LayoutData::default();
        scratch.styles.clone_from(&data.styles);
        scratch.font_metric_advance_quantization = data.font_metric_advance_quantization;
        scratch.nominal_font_metric_line_breaks = data.nominal_font_metric_line_breaks;
        Self {
            context: super::ShapeContext::default(),
            scratch,
        }
    }

    pub(crate) fn shape(
        &mut self,
        data: &crate::layout::data::LayoutData<B>,
        run_index: usize,
        source_start: usize,
    ) -> super::source::PreparedSourceShape {
        let index = data
            .deferred_justification_shapes
            .binary_search_by_key(&run_index, |shape| shape.source.run_index)
            .expect("retained optional shaping run");
        let deferred = &data.deferred_justification_shapes[index];
        let run = &data.runs[run_index];
        let source = data
            .shaping_source_text
            .as_ref()
            .expect("retained shaping text");
        let boundaries = &deferred.source.safe_concat_boundaries;
        let first = boundaries.partition_point(|boundary| *boundary <= source_start);
        for offset in first..=boundaries.len() {
            let splice = boundaries
                .get(offset)
                .copied()
                .unwrap_or(run.text_range.end);
            let end = boundaries
                .get(offset + 1)
                .copied()
                .unwrap_or(run.text_range.end);
            let mut shape = deferred.source.shape_source_range(
                &mut self.context,
                &mut self.scratch,
                data,
                source,
                source_start..end,
                deferred.source.context.clone(),
                &deferred.source.features,
            );
            if splice == run.text_range.end
                || shape.safe_concat_boundaries.binary_search(&splice).is_ok()
            {
                shape.source.end = splice;
                shape
                    .clusters
                    .retain(|cluster| cluster.text_range(run).start < splice);
                return shape;
            }
        }
        unreachable!("the shaping run end is a structural boundary")
    }
}
