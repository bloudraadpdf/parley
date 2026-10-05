// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

use super::{ShapeContext, cache, variations_iter};
use crate::{
    AdvanceQuantisation, FontData, FontVariation,
    advance_quantisation::{QuantisedFont, QuantisedFontFuncs},
};

/// The shaper data of 1 font, with its quantised advances when the layout quantises them.
pub(super) struct FontShapeData {
    shaper: harfrust::ShaperData,
    quantised: Option<QuantisedFont>,
}

pub(super) struct SegmentShape<'a> {
    pub(super) font: FontData,
    pub(super) synthesis: fontique::Synthesis,
    pub(super) variations: Option<&'a [FontVariation]>,
    pub(super) features: &'a [harfrust::Feature],
    pub(super) size: f32,
    pub(super) direction: harfrust::Direction,
    pub(super) script: harfrust::Script,
    pub(super) language: Option<harfrust::Language>,
    pub(super) text: &'a str,
    pub(super) before: &'a str,
    pub(super) after: &'a str,
    pub(super) produce_concat_boundaries: bool,
    pub(super) advance_quantisation: Option<AdvanceQuantisation>,
}

pub(super) struct ShapedSegment {
    pub(super) glyphs: harfrust::GlyphBuffer,
    pub(super) coords: Vec<harfrust::NormalizedCoord>,
}

impl ShapeContext {
    pub(super) fn shape_segment(&mut self, input: SegmentShape<'_>) -> ShapedSegment {
        let font = harfrust::FontRef::from_index(input.font.data.as_ref(), input.font.index)
            .expect("selected shaping font");
        let blob_id = input.font.data.id();
        let FontShapeData {
            shaper: shaper_data,
            quantised,
        } = self.shape_data_cache.entry(
            cache::ShapeDataKey::new(blob_id, input.font.index, input.advance_quantisation),
            || FontShapeData {
                shaper: harfrust::ShaperData::new(&font),
                quantised: input
                    .advance_quantisation
                    .and_then(|quantisation| QuantisedFont::new(&font, quantisation)),
            },
        );
        let instance = self.shape_instance_cache.entry(
            cache::ShapeInstanceKey::new(
                blob_id,
                input.font.index,
                &input.synthesis,
                input.variations,
            ),
            || {
                harfrust::ShaperInstance::from_variations(
                    &font,
                    variations_iter(&input.synthesis, input.variations),
                )
            },
        );
        let shaper = shaper_data.shaper(&font).instance(Some(instance)).build();
        let plan = self.shape_plan_cache.entry(
            cache::ShapePlanKey::new(
                blob_id,
                input.font.index,
                &input.synthesis,
                input.direction,
                input.script,
                input.language.clone(),
                input.features,
                input.variations,
            ),
            || {
                harfrust::ShapePlan::new(
                    &shaper,
                    input.direction,
                    Some(input.script),
                    input.language.as_ref(),
                    input.features,
                )
            },
        );
        let mut buffer = self
            .unicode_buffer
            .take()
            .expect("available shaping buffer");
        buffer.set_flags(if input.produce_concat_boundaries {
            harfrust::BufferFlags::PRODUCE_UNSAFE_TO_CONCAT
        } else {
            harfrust::BufferFlags::empty()
        });
        buffer.reserve(input.text.len());
        for (index, character) in input.text.chars().enumerate() {
            buffer.add(character, index as u32);
        }
        buffer.set_pre_context(input.before);
        buffer.set_post_context(input.after);
        buffer.set_direction(input.direction);
        buffer.set_script(input.script);
        if let Some(language) = input.language {
            buffer.set_language(language);
        }
        let mut funcs = quantised.as_ref().map(|quantised| {
            QuantisedFontFuncs::new(quantised, &font, !shaper.coords().is_empty())
        });
        ShapedSegment {
            glyphs: shaper.shape(
                buffer,
                harfrust::ShapeOptions::new()
                    .plan(Some(plan))
                    .features(input.features)
                    .point_size(Some(input.size))
                    .font_funcs(
                        funcs
                            .as_mut()
                            .map(|funcs| funcs as &mut dyn harfrust::font::FontFuncs),
                    ),
            ),
            coords: shaper.coords().to_vec(),
        }
    }
}
