// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

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
    let mut layout = build_layout(
        " AAABBBCCC\u{202e}IIIHHHGGGFFFEEEDDD\u{202c}JJJKKKLLL",
        [InlineBox::new(49, 20, 19.0, 0.0)],
    );
    layout.break_all_lines(None);
    let mut box_x = None;
    for item in layout.lines().flat_map(|line| line.items()) {
        match item {
            PositionedLayoutItem::InlineBox(inline_box) if inline_box.id == 49 => {
                box_x = Some(inline_box.x);
            }
            _ => {}
        }
    }
    assert_eq!(box_x, Some(127.177_734));
}
