// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Paragraph-at-a-time line breaking.

use super::{test_builders::create_font_context, utils::ColorBrush};
use crate::{FontFamily, Layout, LayoutContext, LineBreakStrategy, StyleProperty, TotalFit};
use alloc::vec::Vec;

/// Words that Prince 16.2 sets at 200 pt in 12 pt Roboto, justified, as 6, 6 and 5 words; first fit takes 7 words
/// on the first line by shrinking its separators.
const TEXT: &str = "xxxxxx xx x xxxxxxxx xxx xxxxxxxxx xx xxxxxx xxxx xxxxxxxx xxxxxxx xxxx xxxxxxxx xx x xxxx xxx";
const WIDTH: f32 = 200.0;
const PRINCE: TotalFit = TotalFit {
    stretch_em: 0.125,
    tolerance: 800.0,
    line_penalty: 10.0,
    hyphen_penalty: 50.0,
    double_hyphen_demerits: 10_000.0,
    final_hyphen_demerits: 5_000.0,
};

fn layout(text: &str, strategy: LineBreakStrategy) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
    let mut layout = builder.build(text);
    layout.set_line_break_strategy(strategy);
    layout
}

fn words_per_line(text: &str, layout: &Layout<ColorBrush>) -> Vec<usize> {
    layout
        .lines()
        .map(|line| text[line.text_range()].split_whitespace().count())
        .collect()
}

fn broken(text: &str, strategy: LineBreakStrategy) -> Vec<usize> {
    let mut layout = layout(text, strategy);
    layout.break_all_lines(Some(WIDTH));
    words_per_line(text, &layout)
}

#[test]
fn total_fit_evens_the_separators_of_a_paragraph() {
    assert_eq!(broken(TEXT, LineBreakStrategy::FirstFit), [7, 5, 5]);
    assert_eq!(broken(TEXT, LineBreakStrategy::TotalFit(PRINCE)), [6, 6, 5]);
}

#[test]
fn total_fit_stretch_uses_font_size_without_added_word_spacing() {
    use crate::{InlineBox, PositionedLayoutItem};
    let mut spaced = layout("x x", LineBreakStrategy::FirstFit);
    spaced.break_all_lines(None);
    let mut letter = layout("x", LineBreakStrategy::FirstFit);
    letter.break_all_lines(None);
    let space_advance = (spaced.full_width() - 2.0 * letter.full_width()) * 10.0 / 12.0;
    let text = " ".repeat(21);
    for word_spacing in [0.0, 1.0] {
        for (delta, expected) in [(-0.01, 9), (0.01, 8)] {
            let mut widths = [
                13.0, 7.0, 15.0, 9.0, 9.0, 43.0, 18.0, 19.0, 13.0, 36.0, 41.0, 27.0, 7.0, 49.0,
                21.0, 42.0, 42.0, 53.0, 20.0, 38.0, 18.0, 31.0,
            ];
            widths[0] =
                165.07874 - 120.0 - 7.0 * (space_advance + word_spacing) - 7.0 * 10.0 / 4.0 + delta;
            let mut font_context = create_font_context();
            let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
            let mut builder = layout_context.ranged_builder(&mut font_context, &text, 1.0, false);
            builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
            builder.push_default(StyleProperty::FontSize(10.0));
            builder.push_default(StyleProperty::WordSpacing(word_spacing));
            builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
            for (index, width) in widths.into_iter().enumerate() {
                builder.push_inline_box(InlineBox::new(index as u64, index, width, 10.0));
            }
            let mut paragraph = builder.build(&text);
            paragraph.set_line_break_strategy(LineBreakStrategy::TotalFit(PRINCE));
            paragraph.break_all_lines(Some(165.07874));
            let count = paragraph
                .lines()
                .next()
                .unwrap()
                .items()
                .filter(|item| matches!(item, PositionedLayoutItem::InlineBox(_)))
                .count();
            assert_eq!(
                count, expected,
                "word spacing {word_spacing}, delta {delta}"
            );
        }
    }
}

#[test]
fn transparent_inline_edges_do_not_include_trailing_space_in_total_fit() {
    use crate::{InlineBox, InlineBoxBreakAffinity, InlineOwnerEdgeAdvance};
    let words = "x x x x x x";
    let text = alloc::format!("{words} yyyyyyyyyyyy");
    let mut natural = layout(words, LineBreakStrategy::FirstFit);
    natural.break_all_lines(None);
    let width = natural.full_width() - 1.0;
    for inline_box in [
        InlineBox::transparent_anchor(1, words.len() + 1),
        InlineBox::inline_start_edge(
            1,
            words.len() + 1,
            InlineOwnerEdgeAdvance::uniform(0.0),
            0.0,
            InlineBoxBreakAffinity::ToNext,
        ),
    ] {
        let mut font_context = create_font_context();
        let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
        let mut builder = layout_context.ranged_builder(&mut font_context, &text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(12.0));
        builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
        builder.push_inline_box(inline_box);
        let mut paragraph = builder.build(&text);
        paragraph.set_line_break_strategy(LineBreakStrategy::TotalFit(PRINCE));
        paragraph.break_all_lines(Some(width));
        assert_eq!(words_per_line(&text, &paragraph), [6, 1]);
    }
}

#[test]
fn automatic_and_authored_hyphens_follow_native_prince_line_choices() {
    use crate::layout::{DiscretionaryBreak, DiscretionaryBreakCondition};
    let text = "centre; -haf, n. the high sea; -henda, f. = alhenda; -hending, f. full or per\u{ad}fect rhyme, as góð: blóð (opposed to ‘skothending’);";
    let unbroken = [
        "centre; -haf, n. the high sea; -henda, f.",
        "= alhenda; -hending, f. full or perfect",
        "rhyme, as góð: blóð (opposed to",
        "‘skothending’);",
    ];
    let hyphenated = [
        "centre; -haf, n. the high sea; -henda,",
        "f. = alhenda; -hending, f. full or per-",
        "fect rhyme, as góð: blóð (opposed to",
        "‘skothending’);",
    ];
    let mut hyphen = layout("-", LineBreakStrategy::FirstFit);
    hyphen.break_all_lines(None);
    for (condition, width, expected) in [
        (DiscretionaryBreakCondition::WordOverflow, 200.0, unbroken),
        (DiscretionaryBreakCondition::WordOverflow, 197.0, hyphenated),
        (DiscretionaryBreakCondition::Normal, 200.0, hyphenated),
    ] {
        let mut paragraph = layout(text, LineBreakStrategy::TotalFit(PRINCE));
        paragraph.set_discretionary_breaks(alloc::vec![DiscretionaryBreak {
            byte_index: text.find('\u{ad}').unwrap() + '\u{ad}'.len_utf8(),
            advance: hyphen.full_width(),
            max_consecutive_lines: None,
            condition,
        }]);
        paragraph.break_all_lines(Some(width));
        let lines = paragraph
            .lines()
            .map(|line| {
                text[line.text_range()].trim().replace(
                    '\u{ad}',
                    if line.ends_at_discretionary_break() {
                        "-"
                    } else {
                        ""
                    },
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(lines, expected, "{condition:?} {width}");
    }
}

#[test]
fn an_automatic_hyphen_stays_available_to_alternative_line_starts() {
    use crate::layout::{DiscretionaryBreak, DiscretionaryBreakCondition, LineBreakOverride};
    let text = "care; -ligr, a. sad; -ljóð, re\u{ad}fl. to be in low spir\u{ad}its; -sam\u{ad}li\u{ad}ga, adv. sor\u{ad}row\u{ad}ful\u{ad}ly, sad\u{ad}ly; (1) trou\u{ad}ble\u{ad}some (-söm flu\u{ad}ga); (2) de\u{ad}pressed,";
    let mut hyphen = layout("-", LineBreakStrategy::FirstFit);
    hyphen.break_all_lines(None);
    let mut paragraph = layout(text, LineBreakStrategy::TotalFit(PRINCE));
    paragraph.set_text_indent(5.0, crate::IndentOptions::default());
    paragraph.set_line_break_overrides(
        text.match_indices(" -")
            .map(|(offset, _)| LineBreakOverride::suppress(offset + 2))
            .collect(),
    );
    paragraph.set_discretionary_breaks(
        text.match_indices('\u{ad}')
            .map(|(offset, _)| DiscretionaryBreak {
                byte_index: offset + '\u{ad}'.len_utf8(),
                advance: hyphen.full_width(),
                max_consecutive_lines: None,
                condition: DiscretionaryBreakCondition::WordOverflow,
            })
            .collect(),
    );
    paragraph.break_all_lines(Some(209.0));
    let lines = paragraph
        .lines()
        .map(|line| {
            let mut text = text[line.text_range()].trim().replace('\u{ad}', "");
            if line.ends_at_discretionary_break() {
                text.push('-');
            }
            text
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        [
            "care; -ligr, a. sad; -ljóð, refl. to be in low",
            "spirits; -samliga, adv. sorrowfully, sad-",
            "ly; (1) troublesome (-söm fluga); (2) de-",
            "pressed,",
        ]
    );
}

#[test]
fn each_segment_between_forced_breaks_has_its_own_plan() {
    let text = alloc::format!("{TEXT}\n{TEXT}");
    assert_eq!(
        broken(&text, LineBreakStrategy::TotalFit(PRINCE)),
        [6, 6, 5, 6, 6, 5]
    );
}

#[test]
fn a_line_of_another_measure_breaks_the_rest_of_the_segment_first_fit() {
    let mut layout = layout(TEXT, LineBreakStrategy::TotalFit(PRINCE));
    let mut breaker = layout.break_lines();
    let mut widths = [WIDTH, WIDTH + 50.0, WIDTH + 50.0].into_iter();
    while breaker
        .break_next(widths.next().unwrap_or(WIDTH), crate::LineTabOrigin::ZERO)
        .is_some()
    {}
    breaker.finish();
    assert_eq!(words_per_line(TEXT, &layout), [6, 7, 4]);
}
