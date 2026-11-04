// Copyright 2025 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{vec, vec::Vec};
use icu_normalizer::properties::Decomposed;
use icu_properties::{CodePointSetData, props::DefaultIgnorableCodePoint};

use crate::analysis::AnalysisDataSources;

#[derive(Debug, Default)]
pub(crate) struct CharCluster {
    pub chars: Vec<Char>,
    pub is_emoji: bool,
    pub map_len: usize,
    pub start: u32,
    pub end: u32,
    pub force_normalize: bool,
    comp: Form,
    decomp: Form,
    form: FormKind,
    best_ratio: f32,
}

impl CharCluster {
    pub(crate) fn range(&self) -> SourceRange {
        SourceRange {
            start: self.start,
            end: self.end,
        }
    }
}

/// Source range of a cluster in code units.
#[derive(Copy, Clone)]
pub(crate) struct SourceRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct Char {
    /// The character.
    pub ch: char,
    /// How font selection requires a face to support this character.
    pub coverage: FontCoverage,
    /// Nominal glyph identifier.
    pub glyph_id: GlyphId,
    /// Indexes into the list of styles for the containing text run, to find the style applicable
    /// to this character.
    pub style_index: u16,
}

pub(crate) type GlyphId = u16;

/// How a character takes part in font selection (CSS Fonts 4 5.3).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum FontCoverage {
    /// A default-ignorable shaping control: it reaches the shaper but does not
    /// disqualify a face whose cmap omits it.
    #[default]
    Ignored,
    /// The face must map the character to a nominal glyph.
    Nominal,
    /// A variation selector after a base character: the face must map the
    /// variation sequence.
    VariationSequence,
}

/// The character map queries that font selection makes of a face.
pub(crate) struct FaceCoverage<G, S> {
    /// The nominal glyph of a character, or 0.
    pub glyph: G,
    /// Whether the face maps a variation sequence.
    pub sequence: S,
}

impl<G: Fn(char) -> GlyphId, S: Fn(char, char) -> bool> FaceCoverage<G, S> {
    /// The nominal glyph of `ch`; a space separator the font lacks takes the space glyph, as `HarfBuzz` shapes it.
    fn nominal_glyph(&self, ch: char) -> GlyphId {
        match (self.glyph)(ch) {
            0 if has_space_fallback(ch) => (self.glyph)(' '),
            glyph => glyph,
        }
    }
}

impl FontCoverage {
    pub(crate) const fn counts(self) -> bool {
        !matches!(self, Self::Ignored)
    }
}

/// Whitespace content of a cluster.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(crate) enum Whitespace {
    /// Not a space.
    None = 0,
    /// Standard space.
    Space = 1,
    /// Non-breaking space (U+00A0).
    NoBreakSpace = 2,
    /// Unicode space separator other than U+0020 and U+00A0.
    OtherSpaceSeparator = 3,
    /// Horizontal tab.
    Tab = 4,
    /// Newline (CR, LF, CRLF, LS, or PS).
    Newline = 5,
}

impl Whitespace {
    /// Returns true for space or no break space.
    pub(crate) fn is_space_or_nbsp(self) -> bool {
        matches!(self, Self::Space | Self::NoBreakSpace)
    }
}

/// Iterative status of mapping a character cluster to nominal glyph identifiers.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Status {
    /// Mapping should be skipped.
    Discard,
    /// The best mapping so far.
    Keep,
    /// Complete mapping.
    Complete,
}

impl CharCluster {
    pub(crate) fn clear(&mut self) {
        self.chars.clear();
        self.is_emoji = false;
        self.map_len = 0;
        self.start = 0;
        self.end = 0;
        self.force_normalize = false;
        self.comp.clear();
        self.decomp.clear();
        self.form = FormKind::Original;
        self.best_ratio = 0.;
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.chars.len()
    }

    /// Returns the primary style index for the cluster.
    #[inline(always)]
    pub(crate) fn style_index(&self) -> u16 {
        self.chars[0].style_index
    }

    #[inline(always)]
    fn contributes_to_shaping(ch: char, analysis_data_sources: &AnalysisDataSources) -> bool {
        let props = analysis_data_sources.properties(ch);
        crate::analysis::contributes_to_shaping(props.general_category(), props.script())
    }

    #[inline(always)]
    pub(crate) fn font_coverage(
        ch: char,
        follows_base: bool,
        analysis_data_sources: &AnalysisDataSources,
    ) -> FontCoverage {
        if follows_base && analysis_data_sources.properties(ch).is_variation_selector() {
            FontCoverage::VariationSequence
        } else if Self::contributes_to_shaping(ch, analysis_data_sources)
            && !const { CodePointSetData::new::<DefaultIgnorableCodePoint>() }.contains(ch)
        {
            FontCoverage::Nominal
        } else {
            FontCoverage::Ignored
        }
    }

    fn decomposed(&mut self, analysis_data_sources: &AnalysisDataSources) -> Option<&[Char]> {
        match self.decomp.state {
            FormState::Invalid => None,
            FormState::None => {
                self.decomp.state = FormState::Invalid;

                // Only attempt pairwise normalization (1 <-> 2 characters)
                if self.chars.len() != 1 {
                    return None;
                }

                let decomposer = analysis_data_sources.decomposing_normalizer();
                let decomp = decomposer.decompose(self.chars[0].ch);
                match decomp {
                    Decomposed::Default | Decomposed::Singleton(_) => {
                        return None;
                    }
                    Decomposed::Expansion(a, b) => {
                        let mut copy = self.chars[0];
                        copy.ch = a;
                        copy.coverage = Self::font_coverage(a, false, analysis_data_sources);
                        self.decomp.chars[0] = copy;

                        copy.ch = b;
                        copy.coverage = Self::font_coverage(b, true, analysis_data_sources);
                        self.decomp.chars[1] = copy;

                        self.decomp.len = 2;
                    }
                }

                self.decomp.state = FormState::Valid;
                self.decomp.setup();
                Some(self.decomp.chars())
            }
            FormState::Valid => Some(self.decomp.chars()),
        }
    }

    fn composed(&mut self, analysis_data_sources: &AnalysisDataSources) -> Option<&[Char]> {
        match self.comp.state {
            FormState::Invalid => None,
            FormState::None => {
                self.comp.state = FormState::Invalid;

                // Only attempt pairwise normalization (1 <-> 2 characters)
                if self.chars.len() != 2 {
                    return None;
                }

                let composer = analysis_data_sources.composing_normalizer();
                let ch = composer.compose(self.chars[0].ch, self.chars[1].ch)?;
                let mut copy = self.chars[0];
                copy.ch = ch;
                copy.coverage = Self::font_coverage(ch, false, analysis_data_sources);
                self.comp.chars[0] = copy;
                self.comp.len = 1;

                self.comp.state = FormState::Valid;
                self.comp.setup();
                Some(self.comp.chars())
            }
            FormState::Valid => Some(self.comp.chars()),
        }
    }

    pub(crate) fn map(
        &mut self,
        coverage: &FaceCoverage<impl Fn(char) -> GlyphId, impl Fn(char, char) -> bool>,
        analysis_data_sources: &AnalysisDataSources,
    ) -> Status {
        let len = self.len();
        if len == 0 {
            return Status::Complete;
        }
        let mut glyph_ids = vec![0; len.max(2)];
        let prev_ratio = self.best_ratio;
        let mut ratio;
        if self.force_normalize && self.composed(analysis_data_sources).is_some() {
            ratio = self
                .comp
                .mapper()
                .map(coverage, &mut glyph_ids, self.best_ratio);
            if ratio > self.best_ratio {
                self.best_ratio = ratio;
                self.form = FormKind::NFC;
                if ratio >= 1. {
                    return Status::Complete;
                }
            }
        }
        ratio = Mapper {
            chars: &mut self.chars[..len],
            map_len: self.map_len.max(1),
        }
        .map(coverage, &mut glyph_ids, self.best_ratio);
        if ratio > self.best_ratio {
            self.best_ratio = ratio;
            self.form = FormKind::Original;
            if ratio >= 1. {
                return Status::Complete;
            }
        }
        if self.decomposed(analysis_data_sources).is_some() {
            ratio = self
                .decomp
                .mapper()
                .map(coverage, &mut glyph_ids, self.best_ratio);
            if ratio > self.best_ratio {
                self.best_ratio = ratio;
                self.form = FormKind::NFD;
                if ratio >= 1. {
                    return Status::Complete;
                }
            }
            if !self.force_normalize && self.composed(analysis_data_sources).is_some() {
                ratio = self
                    .comp
                    .mapper()
                    .map(coverage, &mut glyph_ids, self.best_ratio);
                if ratio > self.best_ratio {
                    self.best_ratio = ratio;
                    self.form = FormKind::NFC;
                    if ratio >= 1. {
                        return Status::Complete;
                    }
                }
            }
        }
        if self.best_ratio > prev_ratio {
            Status::Keep
        } else {
            Status::Discard
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
enum FormKind {
    #[default]
    Original,
    NFD,
    NFC,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FormState {
    None,
    Valid,
    Invalid,
}

#[derive(Clone, Debug)]
pub(crate) struct Form {
    chars: [Char; 2],
    len: u8,
    map_len: usize,
    state: FormState,
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl Form {
    fn new() -> Self {
        Self {
            chars: [Char::default(), Char::default()],
            len: 0,
            map_len: 0,
            state: FormState::None,
        }
    }

    fn clear(&mut self) {
        self.chars = [Char::default(), Char::default()];
        self.len = 0;
        self.map_len = 0;
        self.state = FormState::None;
    }

    #[inline(always)]
    fn chars(&self) -> &[Char] {
        &self.chars[..self.len as usize]
    }

    #[inline(always)]
    fn setup(&mut self) {
        self.map_len = self.chars().iter().filter(|c| c.coverage.counts()).count();
    }

    #[inline(always)]
    fn mapper(&mut self) -> Mapper<'_> {
        Mapper {
            chars: &mut self.chars[..self.len as usize],
            map_len: self.map_len,
        }
    }
}

struct Mapper<'a> {
    chars: &'a mut [Char],
    map_len: usize,
}

impl<'a> Mapper<'a> {
    fn map(
        &mut self,
        coverage: &FaceCoverage<impl Fn(char) -> GlyphId, impl Fn(char, char) -> bool>,
        glyphs: &mut [u16],
        best_ratio: f32,
    ) -> f32 {
        if self.map_len == 0 {
            return 1.;
        }
        let mut mapped = 0;
        let mut base = None;
        for (c, g) in self.chars.iter().zip(glyphs.iter_mut()) {
            *g = coverage.nominal_glyph(c.ch);
            mapped += usize::from(match c.coverage {
                FontCoverage::Ignored => false,
                FontCoverage::Nominal => *g != 0,
                FontCoverage::VariationSequence => {
                    base.is_some_and(|base| (coverage.sequence)(base, c.ch))
                }
            });
            base = Some(c.ch);
        }
        let ratio = mapped as f32 / self.map_len as f32;
        if ratio > best_ratio {
            for (ch, glyph) in self.chars.iter_mut().zip(glyphs) {
                ch.glyph_id = *glyph;
            }
        }
        ratio
    }
}

/// Whether `ch` is a space separator that `HarfBuzz` renders with the space glyph of a font that lacks it
/// (`space_fallback`); such a font covers it when it maps the space.
pub const fn has_space_fallback(ch: char) -> bool {
    matches!(
        ch,
        '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// Tests nominal cluster coverage with the same canonical and space fallbacks used by font selection.
pub fn cluster_is_supported(
    text: &str,
    glyph_for: impl Fn(char) -> Option<u32>,
    supports_sequence: impl Fn(char, char) -> bool,
) -> bool {
    let data = AnalysisDataSources::new();
    let coverage = FaceCoverage {
        glyph: |ch| u16::from(glyph_for(ch).is_some_and(|glyph| glyph != 0)),
        sequence: supports_sequence,
    };
    cluster(text.chars(), &data).map(&coverage, &data) == Status::Complete
}

fn cluster(chars: impl IntoIterator<Item = char>, data: &AnalysisDataSources) -> CharCluster {
    let mut cluster = CharCluster::default();
    for ch in chars {
        let properties = data.properties(ch);
        cluster.force_normalize |= super::force_normalize(
            ch,
            properties.grapheme_cluster_break(),
            properties.is_variation_selector(),
        );
        let coverage = CharCluster::font_coverage(ch, !cluster.chars.is_empty(), data);
        cluster.map_len += usize::from(coverage.counts());
        cluster.chars.push(Char {
            ch,
            coverage,
            glyph_id: 0,
            style_index: 0,
        });
    }
    cluster
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_cluster_coverage_preserves_font_matching_rules() {
        let supports = |text: &str, mapped: &[char]| {
            cluster_is_supported(text, |ch| mapped.contains(&ch).then_some(1), |_, _| false)
        };
        assert!(supports("\u{00a0}\u{093c}", &[' ', '\u{093c}']));
        assert!(!supports("\u{00a0}\u{093c}", &[' ']));
        assert!(!supports("\u{00a0}\u{093c}", &['\u{093c}']));
        assert!(supports("e\u{0301}", &['é']));
        assert!(!supports("e\u{0301}", &['e']));
        assert!(supports("é", &['e', '\u{0301}']));
        assert!(supports("a\u{200d}", &['a']));
        assert!(!cluster_is_supported("a", |_| Some(0), |_, _| false));
    }

    #[test]
    fn a_variation_sequence_needs_a_face_that_maps_the_sequence() {
        let supports = |sequence: Option<(char, char)>| {
            cluster_is_supported(
                "\u{8279}\u{fe00}",
                |_| Some(1),
                |base, selector| sequence == Some((base, selector)),
            )
        };
        assert!(supports(Some(('\u{8279}', '\u{fe00}'))));
        assert!(!supports(None));
    }

    #[test]
    fn an_extended_pair_without_a_canonical_composition_needs_a_font_that_maps_it() {
        let data = AnalysisDataSources::new();
        let mut cluster = cluster("\u{0627}\u{064B}".chars(), &data);
        cluster.force_normalize = true;

        assert_eq!(
            cluster.map(
                &FaceCoverage {
                    glyph: |_| 0,
                    sequence: |_, _| false
                },
                &data
            ),
            Status::Discard
        );
    }

    #[test]
    fn default_ignorables_do_not_reduce_emoji_cluster_font_coverage() {
        let data = AnalysisDataSources::new();
        let mut cluster = cluster(
            "\u{1F469}\u{1F3FF}\u{200D}\u{2764}\u{FE0F}\u{200D}\u{1F48B}\u{200D}\u{1F468}\u{1F3FB}"
                .chars(),
            &data,
        );

        let coverage = FaceCoverage {
            glyph: |ch| u16::from(!matches!(ch, '\u{200D}' | '\u{FE0F}')),
            sequence: |base, selector| (base, selector) == ('\u{2764}', '\u{FE0F}'),
        };
        let status = cluster.map(&coverage, &data);

        assert_eq!(
            status,
            Status::Complete,
            "ZWJ is a shaping control and a variation selector needs its sequence, not a nominal glyph",
        );
    }

    /// `HarfBuzz` shapes a space separator that a font lacks with the space glyph of the font, as `WeasyPrint` and
    /// Chrome do (weasyprint-samples poster: U+202F in "25 juin" with Archivo Black).
    #[test]
    fn a_font_with_a_space_covers_the_space_separators_it_lacks() {
        let data = AnalysisDataSources::new();
        let space_only = |ch| u16::from(ch == ' ');
        for ch in ['\u{00A0}', '\u{2009}', '\u{202F}', '\u{205F}', '\u{3000}'] {
            assert_eq!(
                cluster([ch], &data).map(
                    &FaceCoverage {
                        glyph: space_only,
                        sequence: |_, _| false
                    },
                    &data
                ),
                Status::Complete,
                "{ch:?}"
            );
        }
        assert_eq!(
            cluster(['\u{1680}'], &data).map(
                &FaceCoverage {
                    glyph: space_only,
                    sequence: |_, _| false
                },
                &data
            ),
            Status::Discard
        );
        assert_eq!(
            cluster(['\u{202F}'], &data).map(
                &FaceCoverage {
                    glyph: |_| 0,
                    sequence: |_, _| false
                },
                &data
            ),
            Status::Discard
        );
    }

    #[test]
    fn font_coverage_maps_every_character_in_a_large_cluster() {
        let data = AnalysisDataSources::new();
        let mut cluster = cluster(
            core::iter::once('e').chain(core::iter::repeat_n('\u{0301}', 35_000)),
            &data,
        );

        assert_eq!(
            cluster.map(
                &FaceCoverage {
                    glyph: |_| 1,
                    sequence: |_, _| false
                },
                &data
            ),
            Status::Complete
        );
        assert!(cluster.chars.iter().all(|ch| ch.glyph_id == 1));
    }
}
