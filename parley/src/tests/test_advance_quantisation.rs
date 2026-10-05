// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A consumer's quantisation of the default-instance advance of each glyph.

use alloc::{sync::Arc, vec::Vec};

use fontique::{Blob, Collection, CollectionOptions, FontInfoOverride, FontWeight, SourceCache};
use skrifa::raw::{TableProvider, TopLevelTable, tables::hmtx::Hmtx};

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    AdvanceQuantisation, FontContext, FontFamily, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};

/// The units per em of Roboto: at this font size, 1 font unit is 1 layout unit.
const ROBOTO_UNITS_PER_EM: f32 = 2048.0;

/// A grid of 64 font units, far coarser than a real one, so that each glyph changes.
fn coarse(advance: u16, _units_per_em: u16) -> u16 {
    advance / 64 * 64
}

/// The glyphs of `text` in Roboto with their advance and their `hmtx` advance.
fn glyphs(text: &str, quantisation: Option<AdvanceQuantisation>) -> Vec<(u32, f32, u16)> {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    lcx.set_advance_quantisation(quantisation);
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(ROBOTO_UNITS_PER_EM));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let line = layout.lines().next().unwrap();
    line.items()
        .filter_map(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => Some(run),
            PositionedLayoutItem::InlineBox(_) => None,
        })
        .flat_map(|run| {
            let font = run.run().font();
            let font = skrifa::FontRef::from_index(font.data.as_ref(), font.index).unwrap();
            let hmtx = font.hmtx().unwrap();
            run.glyphs()
                .map(|glyph| {
                    let advance = hmtx.advance(skrifa::GlyphId::new(glyph.id)).unwrap();
                    (glyph.id, glyph.advance, advance)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_quantised_advance_replaces_the_hmtx_advance_of_each_glyph() {
    // The repeated word reads its glyphs from the nominal glyph cache.
    let text = "Hamburgefonstiv Hamburgefonstiv";
    let natural = glyphs(text, None);
    let quantised = glyphs(text, Some(AdvanceQuantisation(coarse)));
    assert_eq!(natural.len(), quantised.len());
    for (&(id, natural, hmtx), &(quantised_id, quantised, _)) in natural.iter().zip(&quantised) {
        assert_eq!(id, quantised_id);
        assert_eq!(
            quantised,
            natural - f32::from(hmtx) + f32::from(coarse(hmtx, 2048)),
            "glyph {id}"
        );
    }
    assert!(
        natural
            .iter()
            .zip(&quantised)
            .any(|(natural, quantised)| natural.1 != quantised.1),
        "the coarse grid changes an advance"
    );
}

#[test]
fn a_quantised_advance_keeps_the_kerning_adjustment() {
    let natural = glyphs("AV", None);
    let quantised = glyphs("AV", Some(AdvanceQuantisation(coarse)));
    let kerning = |glyphs: &[(u32, f32, u16)]| -> f32 {
        glyphs
            .iter()
            .map(|&(_, advance, hmtx)| advance - f32::from(hmtx))
            .sum()
    };
    let grid = |glyphs: &[(u32, f32, u16)]| -> f32 {
        glyphs
            .iter()
            .map(|&(_, _, hmtx)| f32::from(coarse(hmtx, 2048)) - f32::from(hmtx))
            .sum()
    };
    assert_ne!(kerning(&natural), 0.0, "Roboto kerns AV");
    assert_eq!(kerning(&quantised), kerning(&natural) + grid(&natural));
}

/// `font` with the coarse advance of each long metric in its `hmtx` table.
fn with_coarse_hmtx(font: &[u8]) -> Vec<u8> {
    let font_ref = skrifa::FontRef::new(font).unwrap();
    let hmtx = font_ref
        .table_directory
        .table_records()
        .iter()
        .find(|record| record.tag() == Hmtx::TAG)
        .unwrap()
        .offset() as usize;
    let mut patched = font.to_vec();
    for metric in 0..usize::from(font_ref.hhea().unwrap().number_of_h_metrics()) {
        let at = hmtx + 4 * metric;
        let advance = u16::from_be_bytes([patched[at], patched[at + 1]]);
        patched[at..at + 2].copy_from_slice(&coarse(advance, 0).to_be_bytes());
    }
    patched
}

/// The id, position and advance of each glyph of `text` in `family` at `weight`.
fn positioned(
    fcx: &mut FontContext,
    family: &str,
    weight: f32,
    text: &str,
    quantisation: Option<AdvanceQuantisation>,
) -> Vec<(u32, f32, f32, f32)> {
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    lcx.set_advance_quantisation(quantisation);
    let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    builder.push_default(StyleProperty::FontSize(ROBOTO_UNITS_PER_EM));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(weight)));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
        .lines()
        .flat_map(|line| line.items())
        .flat_map(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => run
                .positioned_glyphs()
                .map(|glyph| (glyph.id, glyph.x, glyph.y, glyph.advance))
                .collect::<Vec<_>>(),
            PositionedLayoutItem::InlineBox(_) => Vec::new(),
        })
        .collect()
}

/// Shaping with a quantisation places each glyph as shaping a font whose `hmtx` table holds the
/// quantised advances: with kerning, with mark attachment, and at a variation instance.
#[test]
fn a_quantised_advance_shapes_as_a_font_with_the_quantised_hmtx() {
    for (font, family, text, weights) in [
        (
            &include_bytes!("../../../parley_dev/assets/fonts/roboto_fonts/Roboto-Regular.ttf")[..],
            "Roboto",
            "Hamburgefonstiv AV To",
            &[400.0][..],
        ),
        (
            include_bytes!("../../../parley_dev/assets/fonts/mark_anchor/mark-anchor-test.ttf"),
            "mark-anchor-test",
            "A\u{0301}A\u{0300}\u{0301} \u{05D0}\u{0301}",
            &[400.0],
        ),
        (
            include_bytes!(
                "../../../parley_dev/assets/fonts/arimo_fonts/Arimo-VariableFont_wght.ttf"
            ),
            "Arimo",
            "Hamburgefonstiv AV To",
            &[400.0, 700.0],
        ),
    ] {
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        collection.register_fonts(Blob::new(Arc::new(font.to_vec())), None);
        collection.register_fonts(
            Blob::new(Arc::new(with_coarse_hmtx(font))),
            Some(FontInfoOverride {
                family_name: Some("Coarse"),
                ..FontInfoOverride::default()
            }),
        );
        let mut fcx = FontContext {
            collection,
            source_cache: SourceCache::default(),
        };
        let mut naturals = Vec::new();
        for &weight in weights {
            let quantised = positioned(
                &mut fcx,
                family,
                weight,
                text,
                Some(AdvanceQuantisation(coarse)),
            );
            let natural = positioned(&mut fcx, family, weight, text, None);
            assert!(!quantised.is_empty());
            assert_eq!(
                quantised,
                positioned(&mut fcx, "Coarse", weight, text, None),
                "{family} at {weight}"
            );
            assert_ne!(
                quantised, natural,
                "the coarse grid changes {family} at {weight}"
            );
            assert!(!naturals.contains(&natural), "{family} varies at {weight}");
            naturals.push(natural);
        }
    }
}
