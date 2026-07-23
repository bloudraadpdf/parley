// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Line breaking around inline boxes separated by collapsible spaces.
//!
//! CSS Text 3 §3.1.2: collapsible spaces at the end of a line are
//! removed (they hang past the edge); a wrapped line never begins with
//! removable whitespace. A sequence of full-width inline boxes divided
//! by single spaces must therefore stack one box per line — the space
//! after each box hangs on that box's line instead of wrapping into a
//! line of its own.

use alloc::{vec, vec::Vec};

use super::test_builders::create_font_context;
use crate::{FontFamily, InlineBox, LayoutContext, StyleProperty, layout::DiscretionaryBreak};

use super::utils::ColorBrush;

#[test]
fn spaces_between_full_width_inline_boxes_hang_instead_of_wrapping() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    // Three boxes at 199px in a 200px line: each fills its line, and
    // the single space between consecutive boxes cannot fit after one.
    let text = "  ";
    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    for (id, index) in [(0_u64, 0_usize), (1, 1), (2, 2)] {
        builder.push_inline_box(InlineBox {
            id,
            index,
            width: 199.0,
            height: 8.0,
            glue: false,
        });
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(200.0));

    assert_eq!(
        layout.len(),
        3,
        "three full-width boxes with collapsible spaces between them \
         must produce exactly three lines (the spaces hang at line \
         ends); lines: {:?}",
        layout
            .lines()
            .map(|line| (line.text_range(), line.metrics().advance))
            .collect::<Vec<_>>(),
    );
}

/// PDFreactor-parity trailing-space reclaim: `[text][space][box]` where
/// text + space + box overflows but text + box fits. Browsers wrap the
/// box; with `set_reclaim_space_before_inline_box(true)` the space's
/// advance is removed (exactly as a wrap would remove it) and the box
/// stays on the text's line.
#[test]
fn reclaimed_trailing_space_keeps_the_following_box_on_the_line() {
    let mut fcx = create_font_context();

    // "word " ~= 5 glyphs; box 170px; max 200px. The text alone is well
    // under 30px at 10px Roboto, text + space + 170px box overflows only
    // by less than the space's advance.
    let text = "word ";
    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.push_inline_box(InlineBox {
            id: 0,
            index: text.len(),
            width: 170.0,
            height: 8.0,
            glue: false,
        });
        builder.build(text)
    };

    // Compute the tight max_advance from a text-only probe so the
    // assertion is metric-independent: text+box fits, text+space+box
    // does not.
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut probe_builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    probe_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    probe_builder.push_default(StyleProperty::FontSize(10.0));
    let mut probe = probe_builder.build(text);
    probe.break_all_lines(None);
    let metrics = probe.lines().next().unwrap().metrics().clone();
    let word = metrics.advance - metrics.trailing_whitespace;
    let space = metrics.trailing_whitespace;
    assert!(space > 0.0, "probe geometry: the text must end in a space");
    let max_advance = word + 170.0 + space * 0.5;

    let mut wrapping = build(&mut lcx, &mut fcx);
    wrapping.break_all_lines(Some(max_advance));
    assert_eq!(
        wrapping.len(),
        2,
        "without the reclaim flag the box wraps to its own line"
    );

    let mut reclaiming = build(&mut lcx, &mut fcx);
    reclaiming.set_reclaim_space_before_inline_box(true);
    reclaiming.break_all_lines(Some(max_advance));
    assert_eq!(
        reclaiming.len(),
        1,
        "with the reclaim flag the space is removed and the box stays; lines: {:?}",
        reclaiming
            .lines()
            .map(|line| (line.text_range(), line.metrics().advance))
            .collect::<Vec<_>>(),
    );
}

/// First-fit composition keeps a complete fitting word intact when only its
/// following collapsible space exceeds the measure.
#[test]
fn fitting_word_wins_over_earlier_dash_break_when_trailing_space_hangs() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha-beta ";

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.build(text)
    };

    let mut probe = build(&mut lcx, &mut fcx);
    probe.break_all_lines(None);
    let metrics = probe.lines().next().unwrap().metrics().clone();
    let word_advance = metrics.advance - metrics.trailing_whitespace;
    let max_advance = word_advance + metrics.trailing_whitespace * 0.5;

    let mut greedy = build(&mut lcx, &mut fcx);
    greedy.break_all_lines(Some(max_advance));
    let greedy_first = greedy.lines().next().map(|line| &text[line.text_range()]);
    assert_eq!(
        greedy_first,
        Some(text),
        "the fitting word stays whole and its trailing space hangs"
    );
}

/// A discretionary boundary does not displace a complete fitting word merely
/// because its following collapsible space overflows. The space hangs at the
/// line edge; the hyphen is considered only when the word itself does not fit.
#[test]
fn fitting_word_wins_over_earlier_discretionary_break_when_trailing_space_hangs() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha\u{00AD}beta ";

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let metrics = layout.lines().next().unwrap().metrics().clone();
    let word_advance = metrics.advance - metrics.trailing_whitespace;
    let max_advance = word_advance + metrics.trailing_whitespace * 0.5;

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(max_advance));

    assert_eq!(
        layout.lines().next().map(|line| &text[line.text_range()]),
        Some(text),
        "the complete fitting word must remain intact while its trailing space hangs"
    );
}

#[test]
fn discretionary_material_participates_in_line_fit_and_metrics_only_when_taken() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "alpha\u{00AD}beta";

    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        builder.build(text)
    };

    let mut prefix_builder = lcx.ranged_builder(&mut fcx, "alpha", 1.0, false);
    prefix_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    prefix_builder.push_default(StyleProperty::FontSize(10.0));
    let mut prefix = prefix_builder.build("alpha");
    prefix.break_all_lines(None);
    let prefix_advance = prefix.lines().next().unwrap().metrics().advance;

    let inserted_advance = 4.0;
    let mut layout = build(&mut lcx, &mut fcx);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "alpha\u{00AD}".len(),
        advance: inserted_advance,
        max_consecutive_lines: None,
    }]);
    layout.break_all_lines(Some(prefix_advance + inserted_advance + 0.25));

    let first = layout.lines().next().expect("the prefix must form a line");
    assert_eq!(&text[first.text_range()], "alpha\u{00AD}");
    assert!((first.discretionary_advance() - inserted_advance).abs() < 0.001);
    assert!(
        (first.metrics().advance - (prefix_advance + inserted_advance)).abs() < 0.01,
        "the selected line must reserve the visible discretionary material: {:?}",
        first.metrics(),
    );

    let mut unbroken = build(&mut lcx, &mut fcx);
    unbroken.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "alpha\u{00AD}".len(),
        advance: inserted_advance,
        max_consecutive_lines: None,
    }]);
    unbroken.break_all_lines(None);
    assert_eq!(unbroken.len(), 1);
    assert_eq!(
        unbroken.lines().next().unwrap().discretionary_advance(),
        0.0
    );

    let mut too_narrow = build(&mut lcx, &mut fcx);
    too_narrow.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "alpha\u{00AD}".len(),
        advance: inserted_advance,
        max_consecutive_lines: None,
    }]);
    too_narrow.break_all_lines(Some(prefix_advance + inserted_advance - 0.25));
    assert_eq!(
        too_narrow.len(),
        1,
        "a discretionary boundary whose inserted material does not fit is not a valid line ending"
    );
    assert_eq!(
        too_narrow.lines().next().unwrap().discretionary_advance(),
        0.0
    );
}

#[test]
fn discretionary_break_respects_consecutive_line_limit() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "aa\u{00AD}aa\u{00AD}aa\u{00AD}aa";

    let mut segment_builder = lcx.ranged_builder(&mut fcx, "aa", 1.0, false);
    segment_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    segment_builder.push_default(StyleProperty::FontSize(10.0));
    let mut segment = segment_builder.build("aa");
    segment.break_all_lines(None);
    let measure = segment.lines().next().unwrap().metrics().advance + 2.0;

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let mut layout = builder.build(text);
    layout.set_discretionary_breaks(
        text.match_indices('\u{00AD}')
            .map(|(index, _)| DiscretionaryBreak {
                byte_index: index + '\u{00AD}'.len_utf8(),
                advance: 2.0,
                max_consecutive_lines: Some(1),
            })
            .collect(),
    );
    layout.break_all_lines(Some(measure));

    assert_eq!(
        layout
            .lines()
            .filter(|line| line.discretionary_advance() > 0.0)
            .count(),
        1,
        "the configured cap must suppress a second consecutive discretionary line ending",
    );
}

#[test]
fn zero_advance_discretionary_break_reports_selected_boundary() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();
    let text = "aa\u{00AD}aa";

    let mut segment_builder = lcx.ranged_builder(&mut fcx, "aa", 1.0, false);
    segment_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    segment_builder.push_default(StyleProperty::FontSize(10.0));
    let mut segment = segment_builder.build("aa");
    segment.break_all_lines(None);
    let measure = segment.lines().next().unwrap().metrics().advance + 0.25;

    let mut builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    let mut layout = builder.build(text);
    layout.set_discretionary_breaks(vec![DiscretionaryBreak {
        byte_index: "aa\u{00AD}".len(),
        advance: 0.0,
        max_consecutive_lines: None,
    }]);
    layout.break_all_lines(Some(measure));

    let first = layout.lines().next().expect("the prefix must form a line");
    assert_eq!(&text[first.text_range()], "aa\u{00AD}");
    assert!(first.ends_at_discretionary_break());
    assert_eq!(first.discretionary_advance(), 0.0);
}

/// A glued inline box (a border/padding shim) binds to the adjacent
/// text: min-content measurement sums the shim widths into the text's
/// unbreakable run, and the breaker never wraps between a shim and its
/// glyphs. A replaced (non-glued) box keeps its wrap opportunities.
#[test]
fn glued_boxes_bind_to_text_in_measurement_and_breaking() {
    let mut fcx = create_font_context();
    let mut lcx: LayoutContext<ColorBrush> = LayoutContext::new();

    // "11" bracketed by an 8px and a 6.75px padding shim (the W8BEN
    // item-11 cell). The unit's min-content is the SUM of all three.
    let text = "11";
    let build = |lcx: &mut LayoutContext<ColorBrush>, fcx: &mut crate::FontContext, glue: bool| {
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
        builder.push_default(StyleProperty::FontSize(10.0));
        let mut leading = InlineBox::new(0, 0, 8.0, 0.0);
        leading.glue = glue;
        let mut trailing = InlineBox::new(1, text.len(), 6.75, 0.0);
        trailing.glue = glue;
        builder.push_inline_box(leading);
        builder.push_inline_box(trailing);
        builder.build(text)
    };

    let glued = build(&mut lcx, &mut fcx, true);
    let widths = glued.calculate_content_widths();
    let mut text_only_builder = lcx.ranged_builder(&mut fcx, text, 1.0, false);
    text_only_builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    text_only_builder.push_default(StyleProperty::FontSize(10.0));
    let text_only = text_only_builder.build(text).calculate_content_widths();
    assert!(
        (widths.min - (text_only.min + 8.0 + 6.75)).abs() < 0.01,
        "glued min-content must sum the shims into the text run: got {} \
         for text min {}",
        widths.min,
        text_only.min,
    );

    // Break at a width below the unit: the glued unit must stay on one
    // line (overflowing), never splitting between shim and glyphs.
    let mut layout = build(&mut lcx, &mut fcx, true);
    layout.break_all_lines(Some(widths.min - 2.0));
    assert_eq!(
        layout.len(),
        1,
        "a glued unit narrower than its column overflows on ONE line; \
         lines: {:?}",
        layout
            .lines()
            .map(|line| (line.text_range(), line.metrics().advance))
            .collect::<Vec<_>>(),
    );

    // The same shapes as REPLACED boxes keep their wrap opportunities.
    let unglued_widths = build(&mut lcx, &mut fcx, false).calculate_content_widths();
    assert!(
        unglued_widths.min < widths.min,
        "replaced boxes keep per-box wrap opportunities: {} vs glued {}",
        unglued_widths.min,
        widths.min,
    );
}
