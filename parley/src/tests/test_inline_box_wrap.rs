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

use alloc::vec::Vec;

use super::test_builders::create_font_context;
use crate::{FontFamily, InlineBox, LayoutContext, StyleProperty};

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

/// The dash-balancing heuristic is useful for browser-like narrow prose, but
/// callers must be able to retain the normal greedy result when the complete
/// word fits and only its trailing collapsible space overflows. In that mode
/// the space hangs and the earlier intra-word dash opportunity is not taken.
#[test]
fn intra_word_break_preference_can_be_disabled_when_trailing_space_hangs() {
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

    let mut balanced = build(&mut lcx, &mut fcx);
    balanced.break_all_lines(Some(max_advance));
    let balanced_first = balanced.lines().next().map(|line| &text[line.text_range()]);
    assert_eq!(
        balanced_first,
        Some("alpha-"),
        "the default heuristic must retain the earlier dash break"
    );

    let mut greedy = build(&mut lcx, &mut fcx);
    greedy.set_prefer_intra_word_break_over_hanging_space(false);
    greedy.break_all_lines(Some(max_advance));
    let greedy_first = greedy.lines().next().map(|line| &text[line.text_range()]);
    assert_eq!(
        greedy_first,
        Some(text),
        "when the preference is disabled, the fitting word stays whole and its trailing space hangs"
    );
}

/// Discretionary soft hyphens remain eligible even when the caller disables
/// the authored-dash preference. A soft hyphen is inserted specifically to
/// fill a line tail; hanging the following space and keeping the whole word
/// defeats auto hyphenation and stretches the preceding justified line.
#[test]
fn disabled_dash_preference_still_takes_discretionary_soft_hyphen() {
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
    layout.set_prefer_intra_word_break_over_hanging_space(false);
    layout.break_all_lines(Some(max_advance));

    assert_eq!(
        layout.lines().next().map(|line| &text[line.text_range()]),
        Some("alpha\u{00AD}"),
        "an inserted discretionary hyphen must still fill the line tail"
    );
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
