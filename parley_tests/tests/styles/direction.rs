// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tests for paragraph base direction.

use crate::test_name;
use crate::util::{ColorBrush, TestEnv, samples};
use parley::layout::Alignment;
use parley::style::BaseDirection;
use parley::{AlignmentOptions, Layout};

/// Helper to build a layout with a specific base direction
fn build_with_direction(
    env: &mut TestEnv,
    text: &str,
    direction: BaseDirection,
) -> Layout<ColorBrush> {
    let mut builder = env.ranged_builder(text);
    builder.set_direction(direction);
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(None, Alignment::Start, AlignmentOptions::default());
    layout
}

// ============================================================================
// Direction Tests
// ============================================================================

#[test]
fn direction_rtl_english() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::LATIN;

    let layout = build_with_direction(&mut env, text, BaseDirection::Rtl);

    assert!(
        layout.is_rtl(),
        "English text with RTL direction should report is_rtl"
    );

    env.with_name("rtl_english").check_layout_snapshot(&layout);
}

#[test]
fn direction_ltr_arabic() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::ARABIC;

    let layout = build_with_direction(&mut env, text, BaseDirection::Ltr);

    assert!(
        !layout.is_rtl(),
        "Arabic text with LTR direction should not report is_rtl"
    );

    env.with_name("ltr_arabic").check_layout_snapshot(&layout);
}

#[test]
fn direction_auto_ltr() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::LATIN;

    let layout = build_with_direction(&mut env, text, BaseDirection::Auto);

    assert!(
        !layout.is_rtl(),
        "English text with Auto direction should detect LTR"
    );

    env.with_name("auto_ltr").check_layout_snapshot(&layout);
}

#[test]
fn direction_auto_rtl() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::ARABIC;

    let layout = build_with_direction(&mut env, text, BaseDirection::Auto);

    assert!(
        layout.is_rtl(),
        "Arabic text with Auto direction should detect RTL"
    );

    env.with_name("auto_rtl").check_layout_snapshot(&layout);
}

#[test]
fn direction_rtl_affects_alignment() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::MIXED_BIDI;

    let layout_ltr = build_with_direction(&mut env, text, BaseDirection::Ltr);
    env.with_name("mixed_bidi_ltr")
        .check_layout_snapshot(&layout_ltr);

    let layout_rtl = build_with_direction(&mut env, text, BaseDirection::Rtl);
    env.with_name("mixed_bidi_rtl")
        .check_layout_snapshot(&layout_rtl);

    // RTL and LTR should produce different visual orderings
    assert_ne!(
        layout_ltr.is_rtl(),
        layout_rtl.is_rtl(),
        "LTR and RTL should produce different base levels"
    );
}
