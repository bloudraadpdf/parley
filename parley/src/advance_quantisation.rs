// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A consumer grid for the horizontal advance of each glyph.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};

use skrifa::raw::TableProvider;

/// Quantises the horizontal advance of a glyph in the default instance, in font units, given the
/// units per em of its font. Shaping and the font metrics read each advance through it, as if the
/// font carried the quantised advances in its `hmtx` table: a variation, a positioning adjustment
/// and spacing keep their full value.
#[derive(Clone, Copy, Debug)]
pub struct AdvanceQuantisation(pub fn(advance: u16, units_per_em: u16) -> u16);

impl PartialEq for AdvanceQuantisation {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::fn_addr_eq(self.0, other.0)
    }
}

/// The quantised advances of 1 font.
#[derive(Clone)]
pub(crate) struct QuantisedAdvances<'a> {
    hmtx: Option<skrifa::raw::tables::hmtx::Hmtx<'a>>,
    units_per_em: u16,
    quantise: fn(u16, u16) -> u16,
}

impl<'a> QuantisedAdvances<'a> {
    pub(crate) fn new(font: &skrifa::FontRef<'a>, quantisation: AdvanceQuantisation) -> Self {
        Self {
            hmtx: font.hmtx().ok(),
            units_per_em: font.head().map_or(0, |head| head.units_per_em()),
            quantise: quantisation.0,
        }
    }

    /// The change of the advance of `glyph`, in font units: its quantised advance less its advance
    /// in the default instance.
    fn change(&self, glyph: skrifa::GlyphId) -> i32 {
        let Some(advance) = self.hmtx.as_ref().and_then(|hmtx| hmtx.advance(glyph)) else {
            return 0;
        };
        i32::from((self.quantise)(advance, self.units_per_em)) - i32::from(advance)
    }

    /// The quantised advance of `glyph` from its `advance` in font units.
    pub(crate) fn advance(&self, glyph: skrifa::GlyphId, advance: f32) -> f32 {
        advance + self.change(glyph) as f32
    }
}

/// The advance widths of 1 font instance in font units, quantised when the consumer asks.
#[derive(Clone)]
pub(crate) struct GlyphAdvances<'a> {
    metrics: skrifa::metrics::GlyphMetrics<'a>,
    quantised: Option<QuantisedAdvances<'a>>,
}

impl<'a> GlyphAdvances<'a> {
    pub(crate) fn new(
        font: &skrifa::FontRef<'a>,
        coords: &'a [skrifa::prelude::NormalizedCoord],
        quantisation: Option<AdvanceQuantisation>,
    ) -> Self {
        Self {
            metrics: skrifa::metrics::GlyphMetrics::new(
                font,
                skrifa::prelude::Size::unscaled(),
                coords,
            ),
            quantised: quantisation.map(|quantisation| QuantisedAdvances::new(font, quantisation)),
        }
    }

    pub(crate) fn advance_width(&self, glyph: skrifa::GlyphId) -> Option<f32> {
        let advance = self.metrics.advance_width(glyph)?;
        Some(
            self.quantised
                .as_ref()
                .map_or(advance, |quantised| quantised.advance(glyph, advance)),
        )
    }
}

/// The quantised advances of 1 font for shaping, kept with its shaper data: the quantised
/// default-instance advance of each long metric of `hmtx`, and a cache of its nominal glyphs,
/// since harfrust reads its own cache only through its builtin font functions.
pub(crate) struct QuantisedFont {
    advances: Box<[u16]>,
    nominal_glyphs: [AtomicU32; NOMINAL_GLYPH_SLOTS],
}

/// The slots of the nominal glyph cache, selected by the low bits of a code point; a slot holds
/// the high bits of the code point above the glyph id.
const NOMINAL_GLYPH_SLOTS: usize = 256;
const NOMINAL_GLYPH_BITS: u32 = 19;
const EMPTY_SLOT: u32 = u32::MAX;

impl QuantisedFont {
    /// The quantised advances of `font`, or `None` when the quantisation changes no advance.
    pub(crate) fn new(
        font: &skrifa::FontRef<'_>,
        quantisation: AdvanceQuantisation,
    ) -> Option<Self> {
        let metrics = font.hmtx().ok()?.h_metrics();
        let units_per_em = font.head().ok()?.units_per_em();
        // Runs of equal advances are common (an ideographic font has few distinct ones), so
        // each run quantises its advance once.
        let quantised = || {
            let mut last: Option<(u16, u16)> = None;
            metrics.iter().map(move |metric| {
                let advance = metric.advance();
                match last {
                    Some((previous, quantised)) if previous == advance => quantised,
                    _ => {
                        let quantised = (quantisation.0)(advance, units_per_em);
                        last = Some((advance, quantised));
                        quantised
                    }
                }
            })
        };
        metrics
            .iter()
            .zip(quantised())
            .any(|(metric, quantised)| metric.advance() != quantised)
            .then(|| Self {
                advances: quantised().collect(),
                nominal_glyphs: [const { AtomicU32::new(EMPTY_SLOT) }; NOMINAL_GLYPH_SLOTS],
            })
    }

    /// The quantised default-instance advance of `glyph`: a glyph past the long metrics shares
    /// the advance of the last one.
    fn advance(&self, glyph: harfrust::GlyphId) -> i32 {
        usize::try_from(glyph.to_u32())
            .ok()
            .and_then(|glyph| self.advances.get(glyph))
            .or(self.advances.last())
            .map_or(0, |&advance| i32::from(advance))
    }

    fn nominal_glyph(
        &self,
        builtin: &harfrust::font::BuiltinFontFuncs<'_>,
        c: u32,
    ) -> Option<harfrust::GlyphId> {
        let slot = &self.nominal_glyphs[c as usize % NOMINAL_GLYPH_SLOTS];
        let tag = c / NOMINAL_GLYPH_SLOTS as u32;
        let cached = slot.load(Ordering::Relaxed);
        if cached != EMPTY_SLOT && cached >> NOMINAL_GLYPH_BITS == tag {
            return Some(harfrust::GlyphId::new(
                cached & ((1 << NOMINAL_GLYPH_BITS) - 1),
            ));
        }
        let glyph = builtin.nominal_glyph(c)?;
        if glyph.to_u32() < 1 << NOMINAL_GLYPH_BITS {
            slot.store(
                tag << NOMINAL_GLYPH_BITS | glyph.to_u32(),
                Ordering::Relaxed,
            );
        }
        Some(glyph)
    }
}

/// The font functions of 1 shaping call with a quantised font. A variation instance keeps the
/// change of its variation: its advance is the builtin advance plus the change that the
/// quantisation makes to the default-instance advance.
pub(crate) struct QuantisedFontFuncs<'a> {
    font: &'a QuantisedFont,
    variation: Option<skrifa::raw::tables::hmtx::Hmtx<'a>>,
}

impl<'a> QuantisedFontFuncs<'a> {
    pub(crate) fn new(font: &'a QuantisedFont, shaped: &skrifa::FontRef<'a>, varied: bool) -> Self {
        Self {
            font,
            variation: varied.then(|| shaped.hmtx().ok()).flatten(),
        }
    }
}

impl harfrust::font::FontFuncs for QuantisedFontFuncs<'_> {
    fn nominal_glyph(
        &mut self,
        builtin: &harfrust::font::BuiltinFontFuncs<'_>,
        c: u32,
    ) -> Option<harfrust::GlyphId> {
        self.font.nominal_glyph(builtin, c)
    }

    fn populate_nominal_glyphs(
        &mut self,
        builtin: &harfrust::font::BuiltinFontFuncs<'_>,
        batch: harfrust::font::NominalGlyphBatch<'_>,
    ) -> usize {
        let mut done = 0;
        for (c, glyph) in batch {
            let Some(nominal) = self.font.nominal_glyph(builtin, c) else {
                break;
            };
            *glyph = nominal;
            done += 1;
        }
        done
    }

    fn advance_width(
        &mut self,
        builtin: &harfrust::font::BuiltinFontFuncs<'_>,
        glyph: harfrust::GlyphId,
    ) -> i32 {
        let quantised = self.font.advance(glyph);
        match &self.variation {
            None => quantised,
            Some(hmtx) => {
                let default = hmtx.advance(glyph).map_or(0, i32::from);
                builtin.advance_width(glyph) + quantised - default
            }
        }
    }

    fn vertical_origin(
        &mut self,
        builtin: &harfrust::font::BuiltinFontFuncs<'_>,
        glyph: harfrust::GlyphId,
    ) -> (i32, i32) {
        (
            self.advance_width(builtin, glyph) / 2,
            builtin.vertical_origin(glyph).1,
        )
    }
}
