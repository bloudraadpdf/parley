// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rich styling support.

mod brush;
mod font;
mod styleset;

use alloc::borrow::Cow;

pub use brush::*;
pub use font::{
    FontFamily, FontFamilyName, FontFeature, FontFeatures, FontStyle, FontVariation,
    FontVariations, FontWeight, FontWidth, GenericFamily,
};
pub use fontique::Language;
pub use parlance::{
    BaseDirection, HyphenateCharacter, OverflowWrap, TabSize, TextWrapMode, WordBreak,
};
pub use styleset::StyleSet;

use crate::util::nearly_eq;

#[derive(Debug, Clone, Copy)]
pub enum WhiteSpaceCollapse {
    Collapse,
    Preserve,
}

/// How normal soft-wrap opportunities are constructed and prioritized.
///
/// CSS `line-break: anywhere` is not an emergency overflow policy: it creates
/// a normal opportunity around every typographic character unit and forbids
/// prioritizing those opportunities. Keeping that mode distinct from
/// [`OverflowWrap`] prevents callers from representing it as an emergency
/// break policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineBreakMode {
    /// Use Unicode line-breaking opportunities and normal UA prioritization.
    #[default]
    Normal,
    /// Create equal-priority opportunities around every character unit.
    Anywhere,
}

/// Fully-resolved normal line-breaking behavior.
///
/// This private carrier is constructed only from the two independent authored
/// controls. Once text reaches analysis/layout, an `Anywhere` paragraph cannot
/// accidentally retain a normal Unicode priority policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SoftBreakPolicy {
    Unicode(WordBreak),
    Anywhere,
}

impl Default for SoftBreakPolicy {
    fn default() -> Self {
        Self::Unicode(WordBreak::Normal)
    }
}

impl SoftBreakPolicy {
    pub(crate) const fn resolve(word_break: WordBreak, line_break_mode: LineBreakMode) -> Self {
        match line_break_mode {
            LineBreakMode::Normal => Self::Unicode(word_break),
            LineBreakMode::Anywhere => Self::Anywhere,
        }
    }

    pub(crate) const fn segmentation_word_break(self) -> WordBreak {
        match self {
            Self::Unicode(word_break) => word_break,
            Self::Anywhere => WordBreak::BreakAll,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontSynthesis {
    #[default]
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontSynthesisStyle {
    #[default]
    Auto,
    None,
    ObliqueOnly,
}

/// The height that this text takes up. The default is `MetricsRelative(1.0)`, which is the given
/// font's preferred line height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    /// The line's height is a multiple of the "line height" defined by the font's metrics--the sum
    /// of the ascender height, descender height, and line gap / leading.
    MetricsRelative(f32),
    /// Line height specified as a multiple of the font size. This is how the CSS `line-height`
    /// property behaves if given a unitless number. Useful if you're using system-defined generic
    /// font families and want the line heights to be consistent across platforms.
    FontSizeRelative(f32),
    /// Line height specified in absolute units. This can be useful for ensuring all lines are
    /// spaced a whole number of pixels apart, or fitting lines into a given layout container
    /// height.
    Absolute(f32),
}

impl Default for LineHeight {
    fn default() -> Self {
        Self::MetricsRelative(1.0)
    }
}

impl LineHeight {
    pub(crate) fn nearly_eq(self, other: Self) -> bool {
        match (self, other) {
            (Self::MetricsRelative(a), Self::MetricsRelative(b))
            | (Self::FontSizeRelative(a), Self::FontSizeRelative(b))
            | (Self::Absolute(a), Self::Absolute(b)) => nearly_eq(a, b),
            _ => false,
        }
    }

    pub(crate) fn scale(self, scale: f32) -> Self {
        match self {
            Self::Absolute(value) => Self::Absolute(value * scale),
            // The other variants are relative to the font size, so scaling here needn't do anything
            value => value,
        }
    }
}

/// Properties that define a style.
#[derive(Clone, PartialEq, Debug)]
pub enum StyleProperty<'a, B: Brush> {
    /// CSS `font-family` property value.
    FontFamily(FontFamily<'a>),
    /// Font size.
    FontSize(f32),
    /// Font width.
    FontWidth(FontWidth),
    /// Font style.
    FontStyle(FontStyle),
    /// Font weight.
    FontWeight(FontWeight),
    /// Weight synthesis policy.
    FontSynthesisWeight(FontSynthesis),
    /// Style synthesis policy.
    FontSynthesisStyle(FontSynthesisStyle),
    /// Font variation settings.
    FontVariations(FontVariations<'a>),
    /// Font feature settings.
    FontFeatures(FontFeatures<'a>),
    /// Whether the consumer-selected fixed font-metric grid applies to this
    /// style run.
    FontMetricAdvanceQuantization(bool),
    /// Locale.
    Locale(Option<Language>),
    /// Brush for rendering text.
    Brush(B),
    /// Underline decoration.
    Underline(bool),
    /// Offset of the underline decoration.
    UnderlineOffset(Option<f32>),
    /// Size of the underline decoration.
    UnderlineSize(Option<f32>),
    /// Brush for rendering the underline decoration.
    UnderlineBrush(Option<B>),
    /// Strikethrough decoration.
    Strikethrough(bool),
    /// Offset of the strikethrough decoration.
    StrikethroughOffset(Option<f32>),
    /// Size of the strikethrough decoration.
    StrikethroughSize(Option<f32>),
    /// Brush for rendering the strikethrough decoration.
    StrikethroughBrush(Option<B>),
    /// Overline decoration.
    Overline(bool),
    /// Offset of the overline decoration.
    OverlineOffset(Option<f32>),
    /// Size of the overline decoration.
    OverlineSize(Option<f32>),
    /// Brush for rendering the overline decoration.
    OverlineBrush(Option<B>),
    /// Line height.
    LineHeight(LineHeight),
    /// Extra spacing between words.
    WordSpacing(f32),
    /// Extra spacing between letters.
    LetterSpacing(f32),
    /// Control over where words can wrap.
    WordBreak(WordBreak),
    /// Control over normal line-breaking opportunities and their priority.
    LineBreakMode(LineBreakMode),
    /// Control over "emergency" line-breaking.
    OverflowWrap(OverflowWrap),
    /// Control over non-"emergency" line-breaking.
    TextWrapMode(TextWrapMode),
    /// Tab size.
    TabSize(TabSize),
    /// Hyphenate character.
    HyphenateCharacter(HyphenateCharacter),
}

/// Unresolved styles.
#[derive(Clone, PartialEq, Debug)]
pub struct TextStyle<'family, 'settings, B: Brush> {
    /// CSS `font-family` property value.
    pub font_family: FontFamily<'family>,
    /// Font size.
    pub font_size: f32,
    /// Font width.
    pub font_width: FontWidth,
    /// Font style.
    pub font_style: FontStyle,
    /// Font weight.
    pub font_weight: FontWeight,
    /// Weight synthesis policy.
    pub font_synthesis_weight: FontSynthesis,
    /// Style synthesis policy.
    pub font_synthesis_style: FontSynthesisStyle,
    /// Font variation settings.
    pub font_variations: FontVariations<'settings>,
    /// Font feature settings.
    pub font_features: FontFeatures<'settings>,
    /// Whether the consumer-selected fixed font-metric grid applies to this
    /// style run.
    pub font_metric_advance_quantization: bool,
    /// Locale.
    pub locale: Option<Language>,
    /// Brush for rendering text.
    pub brush: B,
    /// Underline decoration.
    pub has_underline: bool,
    /// Offset of the underline decoration.
    pub underline_offset: Option<f32>,
    /// Size of the underline decoration.
    pub underline_size: Option<f32>,
    /// Brush for rendering the underline decoration.
    pub underline_brush: Option<B>,
    /// Strikethrough decoration.
    pub has_strikethrough: bool,
    /// Offset of the strikethrough decoration.
    pub strikethrough_offset: Option<f32>,
    /// Size of the strikethrough decoration.
    pub strikethrough_size: Option<f32>,
    /// Brush for rendering the strikethrough decoration.
    pub strikethrough_brush: Option<B>,
    /// Overline decoration.
    pub has_overline: bool,
    /// Offset of the overline decoration.
    pub overline_offset: Option<f32>,
    /// Size of the overline decoration.
    pub overline_size: Option<f32>,
    /// Brush for rendering the overline decoration.
    pub overline_brush: Option<B>,
    /// Line height.
    pub line_height: LineHeight,
    /// Extra spacing between words.
    pub word_spacing: f32,
    /// Extra spacing between letters.
    pub letter_spacing: f32,
    /// Control over where words can wrap.
    pub word_break: WordBreak,
    /// Control over normal line-breaking opportunities and their priority.
    pub line_break_mode: LineBreakMode,
    /// Control over "emergency" line-breaking.
    pub overflow_wrap: OverflowWrap,
    /// Control over non-"emergency" line-breaking.
    pub text_wrap_mode: TextWrapMode,
    /// Tab size.
    pub tab_size: TabSize,
    /// Hyphenate character.
    pub hyphenate_character: HyphenateCharacter,
}

impl<B: Brush> Default for TextStyle<'static, 'static, B> {
    fn default() -> Self {
        TextStyle {
            font_family: FontFamily::Source(Cow::Borrowed("sans-serif")),
            font_size: 16.0,
            font_width: FontWidth::default(),
            font_style: FontStyle::default(),
            font_weight: FontWeight::default(),
            font_synthesis_weight: FontSynthesis::default(),
            font_synthesis_style: FontSynthesisStyle::default(),
            font_variations: FontVariations::empty(),
            font_features: FontFeatures::empty(),
            font_metric_advance_quantization: true,
            locale: None,
            brush: B::default(),
            has_underline: false,
            underline_offset: None,
            underline_size: None,
            underline_brush: None,
            has_strikethrough: false,
            strikethrough_offset: None,
            strikethrough_size: None,
            strikethrough_brush: None,
            has_overline: false,
            overline_offset: None,
            overline_size: None,
            overline_brush: None,
            line_height: LineHeight::default(),
            word_spacing: 0.0,
            letter_spacing: 0.0,
            word_break: WordBreak::default(),
            line_break_mode: LineBreakMode::default(),
            overflow_wrap: OverflowWrap::default(),
            text_wrap_mode: TextWrapMode::default(),
            tab_size: TabSize::default(),
            hyphenate_character: HyphenateCharacter::default(),
        }
    }
}

impl<'a, B: Brush> From<FontFamily<'a>> for StyleProperty<'a, B> {
    fn from(value: FontFamily<'a>) -> Self {
        StyleProperty::FontFamily(value)
    }
}

impl<'a, B: Brush> From<&'a [FontFamilyName<'a>]> for StyleProperty<'a, B> {
    fn from(value: &'a [FontFamilyName<'a>]) -> Self {
        StyleProperty::FontFamily(value.into())
    }
}

impl<'a, B: Brush> From<FontFamilyName<'a>> for StyleProperty<'a, B> {
    fn from(value: FontFamilyName<'a>) -> Self {
        StyleProperty::FontFamily(value.into())
    }
}

impl<'a, B: Brush> From<FontVariations<'a>> for StyleProperty<'a, B> {
    fn from(value: FontVariations<'a>) -> Self {
        StyleProperty::FontVariations(value)
    }
}

impl<'a, B: Brush> From<FontFeatures<'a>> for StyleProperty<'a, B> {
    fn from(value: FontFeatures<'a>) -> Self {
        StyleProperty::FontFeatures(value)
    }
}

impl<B: Brush> From<GenericFamily> for StyleProperty<'_, B> {
    fn from(f: GenericFamily) -> Self {
        StyleProperty::FontFamily(f.into())
    }
}

impl<B: Brush> From<LineHeight> for StyleProperty<'_, B> {
    fn from(value: LineHeight) -> Self {
        StyleProperty::LineHeight(value)
    }
}

impl<B: Brush> From<TabSize> for StyleProperty<'_, B> {
    fn from(value: TabSize) -> Self {
        StyleProperty::TabSize(value)
    }
}

impl<B: Brush> From<HyphenateCharacter> for StyleProperty<'_, B> {
    fn from(value: HyphenateCharacter) -> Self {
        StyleProperty::HyphenateCharacter(value)
    }
}
