// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{string::String, vec::Vec};

use super::test_builders::create_font_context;
use super::utils::ColorBrush;
use crate::{
    BidiAtomId, BidiVisualAtomKind, FontFamily, InlineBox, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};

fn build_layout(
    text: &str,
    inline_boxes: impl IntoIterator<Item = InlineBox>,
) -> Layout<ColorBrush> {
    let mut font_context = create_font_context();
    let mut layout_context: LayoutContext<ColorBrush> = LayoutContext::new();
    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named("Roboto")));
    builder.push_default(StyleProperty::FontSize(10.0));
    for inline_box in inline_boxes {
        builder.push_inline_box(inline_box);
    }
    builder.build(text)
}

fn text_atom_ids(layout: &Layout<ColorBrush>) -> Vec<BidiAtomId> {
    layout
        .bidi_topology()
        .atoms()
        .iter()
        .filter_map(|atom| matches!(atom.kind(), BidiVisualAtomKind::Text(_)).then_some(atom.id()))
        .collect()
}

fn visible_topology_text(layout: &Layout<ColorBrush>, text: &str) -> String {
    layout
        .bidi_topology()
        .atoms()
        .iter()
        .filter_map(|atom| match atom.kind() {
            BidiVisualAtomKind::Text(range) => Some((atom.level(), &text[range.clone()])),
            BidiVisualAtomKind::InlineBox { .. } => None,
        })
        .flat_map(|(level, text)| {
            let mut visible = text
                .chars()
                .filter(char::is_ascii_alphabetic)
                .collect::<Vec<_>>();
            if level.is_rtl() {
                visible.reverse();
            }
            visible
        })
        .collect()
}

#[test]
fn topology_uses_exact_levels_for_paragraph_visual_order() {
    let layout = build_layout("abc אבג 123", []);
    let topology = layout.bidi_topology();
    let atoms = topology.atoms();

    assert_eq!(atoms.len(), 3);
    assert_eq!(
        atoms.iter().map(|atom| atom.kind()).collect::<Vec<_>>(),
        [
            &BidiVisualAtomKind::Text(0..4),
            &BidiVisualAtomKind::Text(11..14),
            &BidiVisualAtomKind::Text(4..11),
        ],
    );
    assert!(!atoms[0].level().is_rtl());
    assert!(!atoms[1].level().is_rtl());
    assert!(atoms[2].level().is_rtl());
    assert_ne!(
        atoms[0].level(),
        atoms[1].level(),
        "level zero and nested level two must remain distinguishable",
    );
}

#[test]
fn nested_directional_overrides_preserve_the_css2_visible_sequence() {
    let text = "a\u{202e}l\u{202d}c\u{202e}j\u{202d}e\u{202e}h\u{202d}g\u{202c}f\u{202c}i\u{202c}d\u{202c}k\u{202c}b\u{202c}m";
    let layout = build_layout(text, []);

    assert_eq!(visible_topology_text(&layout, text), "abcdefghijklm");
}

#[test]
fn zero_width_inline_boundaries_preserve_the_css2_visible_sequence() {
    let prefix = "a\u{202e}l\u{202d}";
    let first_owner = "c\u{202e}j\u{202d}e\u{202e}";
    let between = "h\u{202d}g\u{202c}f";
    let second_owner = "\u{202c}i\u{202c}d\u{202c}k\u{202c}b";
    let suffix = "\u{202c}m";
    let text = [prefix, first_owner, between, second_owner, suffix].concat();
    let first_start = prefix.len();
    let first_end = first_start + first_owner.len();
    let second_start = first_end + between.len();
    let second_end = second_start + second_owner.len();
    let layout = build_layout(
        &text,
        [
            InlineBox::inline_start_edge(
                1,
                first_start,
                0.0,
                0.0,
                crate::InlineBoxBreakAffinity::ToNext,
            ),
            InlineBox::inline_end_edge(
                2,
                first_end,
                0.0,
                0.0,
                crate::InlineBoxBreakAffinity::ToPrevious,
            ),
            InlineBox::inline_start_edge(
                3,
                second_start,
                0.0,
                0.0,
                crate::InlineBoxBreakAffinity::ToNext,
            ),
            InlineBox::inline_end_edge(
                4,
                second_end,
                0.0,
                0.0,
                crate::InlineBoxBreakAffinity::ToPrevious,
            ),
        ],
    );

    assert_eq!(visible_topology_text(&layout, &text), "abcdefghijklm");
}

#[test]
fn topology_is_stable_across_repeated_line_breaking() {
    let mut layout = build_layout("abc אבג 123", []);
    let before = layout.bidi_topology();

    layout.break_all_lines(Some(24.0));
    assert!(layout.len() > 1);
    assert_eq!(layout.bidi_topology(), before);

    layout.break_all_lines(None);
    assert_eq!(layout.len(), 1);
    assert_eq!(layout.bidi_topology(), before);
}

#[test]
fn inline_box_atoms_keep_caller_identity_and_source_boundary() {
    let layout = build_layout(
        "a אב z",
        [
            InlineBox::new(47, 6, 5.0, 5.0),
            InlineBox::new(48, 6, 5.0, 5.0),
        ],
    );
    let boxes = layout
        .bidi_topology()
        .atoms()
        .iter()
        .filter_map(|atom| match atom.kind() {
            BidiVisualAtomKind::InlineBox {
                id,
                source_boundary,
            } => Some((atom.id(), *id, *source_boundary)),
            BidiVisualAtomKind::Text(_) => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(boxes.len(), 2);
    let mut caller_ids = boxes.iter().map(|(_, id, _)| *id).collect::<Vec<_>>();
    caller_ids.sort_unstable();
    assert_eq!(caller_ids, [47, 48]);
    assert_eq!((boxes[0].2, boxes[1].2), (6, 6));
    assert_ne!(boxes[0].0, boxes[1].0);
}

#[test]
fn inline_box_boundary_levels_use_utf8_byte_offsets() {
    let layout = build_layout("éaא", [InlineBox::new(51, "é".len(), 4.0, 4.0)]);
    let topology = layout.bidi_topology();
    let atom = topology
        .atoms()
        .iter()
        .find(|atom| matches!(atom.kind(), BidiVisualAtomKind::InlineBox { id: 51, .. }))
        .expect("the inline box remains in the topology");

    assert!(!atom.level().is_rtl());
}

#[test]
fn positioned_runs_retain_their_pre_break_atom_identity() {
    let mut layout = build_layout("abc אבג 123", []);
    let topology_ids = text_atom_ids(&layout);
    layout.break_all_lines(Some(24.0));

    let positioned = layout
        .lines()
        .flat_map(|line| line.items())
        .filter_map(|item| match item {
            PositionedLayoutItem::GlyphRun(glyph_run) => glyph_run.run().bidi_atom_id(),
            PositionedLayoutItem::InlineBox(_) => None,
        })
        .collect::<Vec<_>>();

    assert!(!positioned.is_empty());
    assert!(positioned.iter().all(|id| topology_ids.contains(id)));
    for id in topology_ids {
        assert!(
            positioned.contains(&id),
            "each pre-break text atom must survive as positioned geometry",
        );
    }
}

#[test]
fn inline_box_at_override_boundary_uses_the_current_resolved_level() {
    let layout = build_layout(
        " AAABBBCCC\u{202e}IIIHHHGGGFFFEEEDDD\u{202c}JJJKKKLLL",
        [InlineBox::new(49, 21, 19.0, 0.0)],
    );
    let topology = layout.bidi_topology();
    let atom = topology
        .atoms()
        .iter()
        .find(|atom| matches!(atom.kind(), BidiVisualAtomKind::InlineBox { id: 49, .. }))
        .expect("the edge box must remain in the bidi topology");

    assert!(atom.level().is_rtl());
}

#[test]
fn trailing_edge_box_before_pdf_stays_in_the_override_sequence() {
    let layout = build_layout(
        " AAABBBCCC\u{202e}IIIHHHGGGFFFEEEDDD\u{202c}JJJKKKLLL",
        [InlineBox::new(49, 20, 19.0, 0.0)],
    );
    let topology = layout.bidi_topology();
    let atom = topology
        .atoms()
        .iter()
        .find(|atom| matches!(atom.kind(), BidiVisualAtomKind::InlineBox { id: 49, .. }))
        .expect("the edge box must remain in the bidi topology");

    assert!(atom.level().is_rtl());
}

#[test]
fn trailing_edge_box_is_positioned_after_the_reordered_override_text() {
    const OWNER: core::ops::Range<usize> = 13..22;
    let mut layout = build_layout(
        " AAABBBCCC\u{202e}IIIHHHGGGFFFEEEDDD\u{202c}JJJKKKLLL",
        [InlineBox::inline_end_edge(
            49,
            OWNER.end,
            19.0,
            0.0,
            crate::InlineBoxBreakAffinity::ToPrevious,
        )],
    );
    layout.break_all_lines(None);
    let items = layout.lines().flat_map(|line| line.items()).collect::<Vec<_>>();
    let owner_index = items
        .iter()
        .position(|item| {
            matches!(
                item,
                PositionedLayoutItem::GlyphRun(run) if run.run().text_range() == OWNER
            )
        })
        .expect("the overridden owner text must remain positioned");
    let edge_index = items
        .iter()
        .position(|item| matches!(item, PositionedLayoutItem::InlineBox(inline_box) if inline_box.id == 49))
        .expect("the inline-end edge must remain positioned");

    assert_eq!(edge_index, owner_index + 1);
}
