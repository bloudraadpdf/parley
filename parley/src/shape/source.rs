// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{Brush, FontVariation, analysis::CharInfo};
use alloc::vec::Vec;
use core::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeferredSourceShape {
    pub(crate) run_index: usize,
    pub(crate) context: Range<usize>,
    pub(crate) script: icu_properties::props::Script,
    pub(crate) language: Option<harfrust::Language>,
    pub(crate) variations: Option<Vec<FontVariation>>,
    pub(crate) features: Vec<harfrust::Feature>,
    pub(crate) character_infos: Vec<(CharInfo, u16)>,
    pub(crate) character_offsets: Vec<usize>,
    pub(crate) safe_concat_boundaries: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedSourceShape {
    pub(crate) clusters: Vec<crate::layout::data::ClusterData>,
    pub(crate) glyphs: Vec<crate::Glyph>,
    pub(crate) source: Range<usize>,
    pub(crate) safe_concat_boundaries: Vec<usize>,
}

/// Safe source boundaries are valid only with `PRODUCE_UNSAFE_TO_CONCAT` enabled.
pub(super) fn safe_concat_boundaries(
    glyphs: &harfrust::GlyphBuffer,
    text: &str,
    source_start: usize,
) -> Vec<usize> {
    let offsets = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut boundaries = glyphs
        .glyph_infos()
        .chunk_by(|left, right| left.cluster == right.cluster)
        .filter(|group| {
            group
                .iter()
                .all(|glyph| !glyph.flags().is_unsafe_to_concat())
        })
        .map(|group| source_start + offsets[group[0].cluster as usize])
        .collect::<Vec<_>>();
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

impl DeferredSourceShape {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shape_source_range<B: Brush>(
        &self,
        context: &mut super::ShapeContext,
        scratch: &mut crate::layout::data::LayoutData<B>,
        data: &crate::layout::data::LayoutData<B>,
        source: &str,
        range: Range<usize>,
        source_context: Range<usize>,
        features: &[harfrust::Feature],
    ) -> PreparedSourceShape {
        let run = &data.runs[self.run_index];
        let text = &source[range.clone()];
        let first = self
            .character_offsets
            .binary_search(&(range.start - run.text_range.start))
            .expect("source character");
        let last = self
            .character_offsets
            .binary_search(&(range.end - run.text_range.start))
            .expect("source character");
        let infos = &self.character_infos[first..last];
        let analysis = crate::analysis::AnalysisDataSources::new();
        let script = crate::convert::script_to_fontique(self.script, &analysis);
        let shaped = context.shape_segment(super::segment::SegmentShape {
            font: data.fonts[run.font_index].clone(),
            synthesis: run.synthesis,
            variations: self.variations.as_deref(),
            features,
            size: run.font_size,
            direction: if run.bidi_level & 1 == 0 {
                harfrust::Direction::LeftToRight
            } else {
                harfrust::Direction::RightToLeft
            },
            script: crate::convert::script_to_harfrust(script),
            language: self.language.clone(),
            text,
            before: &source[source_context.start..range.start],
            after: &source[range.end..source_context.end],
            produce_concat_boundaries: true,
        });
        scratch.push_run(
            data.fonts[run.font_index].clone(),
            run.font_size,
            run.font_attrs,
            run.synthesis,
            &shaped.glyphs,
            self.script,
            run.bidi_level,
            run.paragraph_level,
            run.paragraph_has_strong_direction,
            infos[0].1,
            run.word_spacing,
            run.letter_spacing,
            text,
            infos,
            range.clone(),
            &shaped.coords,
            &[],
        );
        scratch.finish(
            analysis
                .grapheme_boundaries(text)
                .map(|boundary| range.start + boundary),
        );
        let mut clusters = core::mem::take(&mut scratch.clusters);
        for cluster in &mut clusters {
            cluster.text_offset += range.start - run.text_range.start;
        }
        let result = PreparedSourceShape {
            clusters,
            glyphs: core::mem::take(&mut scratch.glyphs),
            source: range.clone(),
            safe_concat_boundaries: safe_concat_boundaries(&shaped.glyphs, text, range.start),
        };
        scratch.runs.clear();
        scratch.items.clear();
        scratch.coords.clear();
        context.unicode_buffer = Some(shaped.glyphs.clear());
        result
    }
}
