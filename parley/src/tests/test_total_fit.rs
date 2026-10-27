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

#[test]
fn consecutive_hyphen_limits_preserve_fitting_lines() {
    use crate::layout::{DiscretionaryBreak, DiscretionaryBreakCondition};

    let text = ["hy\u{ad}phenation"; 6].join(" ");
    let mut hyphen = layout("-", LineBreakStrategy::FirstFit);
    hyphen.break_all_lines(None);
    let mut measure = layout("hyphenation hy-", LineBreakStrategy::FirstFit);
    measure.break_all_lines(None);
    let width = measure.full_width();
    for limit in [1, 2] {
        for strategy in [
            LineBreakStrategy::FirstFit,
            LineBreakStrategy::TotalFit(PRINCE),
        ] {
            let mut paragraph = layout(&text, strategy);
            paragraph.set_discretionary_breaks(
                text.match_indices('\u{ad}')
                    .map(|(offset, _)| DiscretionaryBreak {
                        byte_index: offset + '\u{ad}'.len_utf8(),
                        advance: hyphen.full_width(),
                        max_consecutive_lines: Some(limit),
                        condition: DiscretionaryBreakCondition::WordOverflow,
                    })
                    .collect(),
            );
            paragraph.break_all_lines(Some(width));
            let mut consecutive = 0;
            let mut hyphenated = 0;
            for line in paragraph.lines() {
                consecutive = if line.ends_at_discretionary_break() {
                    hyphenated += 1;
                    consecutive + 1
                } else {
                    0
                };
                assert!(consecutive <= limit);
                let advance = line.metrics().advance - line.metrics().trailing_whitespace;
                assert!(
                    advance <= width + 0.001,
                    "{strategy:?}: {advance} > {width}"
                );
            }
            assert!(hyphenated > 0);
            assert_eq!(
                paragraph.lines().last().unwrap().text_range().end,
                text.len()
            );
        }
    }
}

#[test]
fn nested_quote_edges_preserve_paragraph_fitting() {
    use crate::layout::{DiscretionaryBreak, DiscretionaryBreakCondition};
    use crate::{InlineBox, InlineBoxBreakAffinity, InlineOwnerEdgeAdvance};
    let text = "‘Mrs. Cor\u{ad}ney,’ said Mr. Bum\u{ad}ble, stoop\u{ad}ing over the ma\u{ad}tron, ‘what is this, ma’am? Has any\u{ad}thing hap\u{ad}pened, ma’am? Pray an\u{ad}swer me: I’m on—on—’ Mr. Bum\u{ad}ble, in his alarm, could not im\u{ad}me\u{ad}di\u{ad}ate\u{ad}ly think of the word ‘ten\u{ad}ter\u{ad}hooks,’ so he said ‘bro\u{ad}ken bot\u{ad}tles.’";
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
    let closings = text.match_indices('’').filter(|(offset, quote)| {
        text[offset + quote.len()..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    });
    let ranges =
        text.match_indices('‘')
            .zip(closings)
            .flat_map(|((start, opening), (end, closing))| {
                [
                    start..end + closing.len(),
                    start..start + opening.len(),
                    end..end + closing.len(),
                ]
            });
    for (id, range) in ranges.enumerate() {
        builder.push_inline_box(InlineBox::inline_start_edge(
            2 * id as u64,
            range.start,
            InlineOwnerEdgeAdvance::uniform(0.0),
            0.0,
            InlineBoxBreakAffinity::ToNext,
        ));
        builder.push_inline_box(InlineBox::inline_end_edge_with_following_source_space(
            2 * id as u64 + 1,
            range.end,
            InlineOwnerEdgeAdvance::uniform(0.0),
            0.0,
            crate::FollowingSourceSpace::CollapsedAfterProjectedBreak,
            InlineBoxBreakAffinity::ToPrevious,
        ));
    }
    let mut paragraph = builder.build(text);
    paragraph.set_line_break_strategy(LineBreakStrategy::TotalFit(TotalFit {
        line_penalty: 10.5,
        double_hyphen_demerits: 3000.0,
        final_hyphen_demerits: 0.0,
        ..PRINCE
    }));
    paragraph.set_text_indent(15.6, crate::IndentOptions::default());
    let dash = text.find("—’").unwrap();
    paragraph.set_line_break_overrides(alloc::vec![
        crate::LineBreakOverride::suppress(dash),
        crate::LineBreakOverride::suppress(dash + '—'.len_utf8()),
    ]);
    let mut hyphen = layout("-", LineBreakStrategy::FirstFit);
    hyphen.break_all_lines(None);
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
    paragraph.break_all_lines(Some(379.0));
    let lines: Vec<_> = paragraph
        .lines()
        .map(|line| text[line.text_range()].trim().replace('\u{ad}', ""))
        .collect();
    assert_eq!(
        lines,
        [
            "‘Mrs. Corney,’ said Mr. Bumble, stooping over the matron, ‘what is this,",
            "ma’am? Has anything happened, ma’am? Pray answer me: I’m on—on—’",
            "Mr. Bumble, in his alarm, could not immediately think of the word ‘ten",
            "terhooks,’ so he said ‘broken bottles.’",
        ]
    );
}

#[test]
fn total_fit_penalizes_written_hyphens_but_not_em_dashes() {
    for (dash, width, unpenalized, penalized) in [
        (
            '—',
            130.0,
            "alpha beta gamma delta",
            "alpha beta gamma delta",
        ),
        (
            '—',
            140.0,
            "alpha beta gamma delta—",
            "alpha beta gamma delta—",
        ),
        ('-', 140.0, "alpha beta gamma delta-", "alpha beta gamma"),
    ] {
        let text = alloc::format!(
            "alpha beta gamma delta{dash}epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau"
        );
        for (penalty, expected) in [(0.0, unpenalized), (1_000_000.0, penalized)] {
            let mut paragraph = layout(
                &text,
                LineBreakStrategy::TotalFit(TotalFit {
                    tolerance: f32::INFINITY,
                    hyphen_penalty: penalty,
                    ..PRINCE
                }),
            );
            paragraph.break_all_lines(Some(width));
            assert_eq!(
                text[paragraph.lines().next().unwrap().text_range()].trim(),
                expected
            );
        }
    }
}

#[test]
fn total_fit_preserves_native_dash_paragraph_breaks() {
    let expected = [
        "What an excellent example of the power of dress,",
        "young Oliver Twist was! Wrapped in the blanket",
        "which had hitherto formed his only covering, he",
        "might have been the child of a nobleman or a beggar;",
        "it would have been hard for the haughtiest stranger to",
        "have assigned him his proper station in society. But",
        "now that he was enveloped in the old calico robes",
        "which had grown yellow in the same service, he was",
        "badged and ticketed, and fell into his place at",
        "once—a parish child—the orphan of a workhouse—",
        "the humble, half-starved drudge—to be cuffed and",
        "buffeted through the world—despised by all, and",
        "pitied by none.",
    ];
    for (expected, indent) in [(expected.as_slice(), 15.6), (&expected[8..], 0.0)] {
        let text = expected
            .join(" ")
            .replace("workhouse— the", "workhouse—the");
        let mut paragraph = layout(
            &text,
            LineBreakStrategy::TotalFit(TotalFit {
                line_penalty: 10.5,
                double_hyphen_demerits: 3000.0,
                final_hyphen_demerits: 0.0,
                ..PRINCE
            }),
        );
        paragraph.set_text_indent(indent, crate::IndentOptions::default());
        let short_suffix = text.find("—a ").unwrap();
        paragraph.set_line_break_overrides(alloc::vec![
            crate::LineBreakOverride::suppress(short_suffix),
            crate::LineBreakOverride::suppress(short_suffix + '—'.len_utf8()),
        ]);
        paragraph.break_all_lines(Some(280.0));
        let lines: Vec<_> = paragraph
            .lines()
            .map(|line| text[line.text_range()].trim())
            .collect();
        assert_eq!(lines, expected);
    }
}

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
fn inline_word_edges_preserve_total_fit_line_choices() {
    use crate::{InlineBox, InlineBoxBreakAffinity, InlineOwnerEdgeAdvance};
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
    for space in [
        None,
        Some(crate::FollowingSourceSpace::CollapsedAfterProjectedBreak),
        Some(crate::FollowingSourceSpace::RetainedAdvance),
    ] {
        let mut fonts = create_font_context();
        let mut context: LayoutContext<ColorBrush> = LayoutContext::new();
        let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(12.0));
        builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
        if let Some(space) = space {
            let mut start = 0;
            for (id, word) in text.split_whitespace().enumerate() {
                builder.push_inline_box(InlineBox::inline_start_edge(
                    2 * id as u64,
                    start,
                    InlineOwnerEdgeAdvance::uniform(0.0),
                    0.0,
                    InlineBoxBreakAffinity::ToNext,
                ));
                builder.push_inline_box(InlineBox::inline_end_edge_with_following_source_space(
                    2 * id as u64 + 1,
                    start + word.len(),
                    InlineOwnerEdgeAdvance::uniform(0.0),
                    0.0,
                    space,
                    InlineBoxBreakAffinity::ToPrevious,
                ));
                start += word.len() + 1;
            }
        }
        let mut paragraph = builder.build(text);
        paragraph.set_line_break_strategy(LineBreakStrategy::TotalFit(PRINCE));
        paragraph.set_text_indent(5.0, crate::IndentOptions::default());
        paragraph.break_all_lines(Some(180.0));
        let expected = if space == Some(crate::FollowingSourceSpace::RetainedAdvance) {
            [5, 6, 7, 6]
        } else {
            [5, 6, 8, 5]
        };
        assert_eq!(
            words_per_line(text, &paragraph),
            expected,
            "space={space:?}"
        );
    }
}

#[test]
fn physical_inline_edges_do_not_disable_total_fit() {
    use crate::layout::{PhysicalLineEdge, ShapingEdgePlacement};

    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, TEXT, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
    let start = TEXT.find("xxxxxxxxx").unwrap();
    builder.push_inline_owner_shaping(super::test_physical_shaping::owner(
        start..start + 9,
        PhysicalLineEdge::Left,
        ShapingEdgePlacement::FirstLine,
    ));
    let mut layout = builder.build(TEXT);
    layout.set_line_break_strategy(LineBreakStrategy::TotalFit(PRINCE));
    layout.break_all_lines(Some(WIDTH));
    assert_eq!(words_per_line(TEXT, &layout), [6, 6, 5]);
}

#[test]
fn inactive_justification_features_do_not_disable_total_fit() {
    let text = TEXT.replacen("xxxxxx", "xxx\u{00ad}xxx", 1) + " fi";
    let mut fonts = create_font_context();
    let mut context = LayoutContext::<ColorBrush>::new();
    let mut builder = context.ranged_builder(&mut fonts, &text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
    let features = [crate::FontFeature::new(
        crate::setting::Tag::from_bytes(*b"liga"),
        0,
    )];
    builder.push_default(StyleProperty::FontFeaturesForJustification(
        (&features).into(),
    ));
    let mut layout = builder.build(&text);
    assert!(layout.has_justification_shape_candidates());
    layout.set_line_break_strategy(LineBreakStrategy::TotalFit(PRINCE));
    layout.break_all_lines(Some(WIDTH));
    assert_eq!(words_per_line(&text, &layout), [6, 6, 6]);
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

#[test]
fn total_fit_accounts_for_trimmed_terminal_tracking() {
    let breaks = |text: &str, width: f32, trim: bool, mixed: bool| {
        let mut fonts = create_font_context();
        let mut context = LayoutContext::<ColorBrush>::new();
        let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(12.0));
        builder.push_default(StyleProperty::LetterSpacing(0.7));
        builder.push_default(StyleProperty::JustifyShrink(1.0 / 6.0));
        if mixed {
            builder.push(StyleProperty::LetterSpacing(1.4), 0..1);
        }
        let mut layout = builder.build(text);
        layout.set_line_break_strategy(LineBreakStrategy::TotalFit(PRINCE));
        layout.set_line_end_letter_spacing_trim(trim);
        layout.break_all_lines(Some(width));
        layout
            .lines()
            .map(|line| line.text_range())
            .collect::<Vec<_>>()
    };
    for text in [TEXT, &alloc::format!("AאבB {TEXT}")] {
        for mixed in [false, true] {
            for width in 100..250 {
                assert_eq!(
                    breaks(text, width as f32, true, mixed),
                    breaks(text, width as f32 + 0.7, false, mixed),
                    "width {width}, mixed {mixed}, text {text}"
                );
            }
        }
    }
}
