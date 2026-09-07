//! Variable font selection must remain independent of faux synthesis.

use std::sync::Arc;

use fontique::{Blob, FontInfo, FontStyle, FontStyleSynthesis, SourceId, SourceInfo, SourceKind};

fn variable_face() -> FontInfo {
    let bytes =
        include_bytes!("../../parley_dev/assets/fonts/roboto_fonts/RobotoFlex-VariableFont.ttf");
    FontInfo::from_source(
        SourceInfo::new(
            SourceId::new(),
            SourceKind::Memory(Blob::new(Arc::new(bytes.to_vec()))),
        ),
        0,
    )
    .expect("the variable fixture must load")
}

#[test]
fn matching_weight_still_selects_the_variable_axis() {
    let font = variable_face();
    let selected = font.synthesis(
        font.width(),
        FontStyle::Normal,
        font.weight(),
        FontStyleSynthesis::Allowed,
    );
    assert!(
        selected.variation_settings().iter().any(|(tag, value)| {
            tag.to_be_bytes() == *b"wght" && *value == font.weight().value()
        })
    );
}

#[test]
fn css_oblique_angles_use_the_opposite_opentype_slant_sign() {
    let font = variable_face();
    for angle in [-10.0, 10.0] {
        let selected = font.synthesis(
            font.width(),
            FontStyle::Oblique(Some(angle)),
            font.weight(),
            FontStyleSynthesis::Allowed,
        );
        assert!(
            selected
                .variation_settings()
                .iter()
                .any(|(tag, value)| { tag.to_be_bytes() == *b"slnt" && *value == -angle })
        );
    }
}

#[test]
fn disabling_faux_style_preserves_real_variable_style() {
    let font = variable_face();
    let selected = font.synthesis(
        font.width(),
        FontStyle::Oblique(Some(10.0)),
        font.weight(),
        FontStyleSynthesis::Forbidden,
    );
    assert!(
        selected
            .variation_settings()
            .iter()
            .any(|(tag, value)| { tag.to_be_bytes() == *b"slnt" && *value == -10.0 })
    );
    assert_eq!(selected.skew(), None);
}

#[test]
fn disabling_faux_synthesis_keeps_selected_axes() {
    let font = variable_face();
    let selected = font.synthesis(
        font.width(),
        FontStyle::Oblique(Some(10.0)),
        font.weight(),
        FontStyleSynthesis::Allowed,
    );
    let disabled = selected
        .without_weight_synthesis()
        .without_style_synthesis();
    assert_eq!(selected.variation_settings(), disabled.variation_settings());
    assert!(!disabled.embolden());
    assert_eq!(disabled.skew(), None);
    assert!(
        disabled.variation_settings().iter().any(
            |(tag, value)| tag.to_be_bytes() == *b"wdth" && *value == font.width().percentage()
        )
    );
}
