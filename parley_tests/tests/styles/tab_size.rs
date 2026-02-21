// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tests for tab-size style property.

use crate::test_name;
use crate::util::{ColorBrush, TestEnv, samples};
use parley::layout::Alignment;
use parley::style::{StyleProperty, TabSize};
use parley::{AlignmentOptions, Layout};

/// Helper to build a layout with a specific tab size
fn build_with_tab_size(env: &mut TestEnv, text: &str, tab_size: TabSize) -> Layout<ColorBrush> {
    let mut builder = env.ranged_builder(text);
    builder.push_default(StyleProperty::TabSize(tab_size));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(None, Alignment::Start, AlignmentOptions::default());
    layout
}

// ============================================================================
// TabSize Tests
// ============================================================================

#[test]
fn tab_size_default() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::TAB_START;

    // Default tab size: 8 space widths
    let layout = build_with_tab_size(&mut env, text, TabSize::default());

    // Tab should have non-zero advance
    let first_line = layout.get(0).expect("should have at least one line");
    let first_run = first_line
        .runs()
        .next()
        .expect("should have at least one run");
    let first_cluster = first_run.clusters().next().expect("should have a cluster");
    assert!(
        first_cluster.advance() > 0.0,
        "tab cluster should have positive advance with default tab-size"
    );

    env.with_name("default").check_layout_snapshot(&layout);
}

#[test]
fn tab_size_custom_spaces() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::TAB_START;

    for n in [2.0, 4.0, 8.0] {
        let layout = build_with_tab_size(&mut env, text, TabSize::Spaces(n));

        let name = format!("spaces_{}", n as i32);
        env.with_name(&name).check_layout_snapshot(&layout);
    }
}

#[test]
fn tab_size_length() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::TAB_START;

    for length in [24.0, 48.0, 96.0] {
        let layout = build_with_tab_size(&mut env, text, TabSize::Length(length));

        let name = format!("length_{}", length as i32);
        env.with_name(&name).check_layout_snapshot(&layout);
    }
}

#[test]
fn tab_size_position_dependent() {
    let mut env = TestEnv::new(test_name!(), None);
    // Tab after text — advance should be to next stop, not full interval
    let text = "AB\tC";

    let layout = build_with_tab_size(&mut env, text, TabSize::Spaces(4.0));

    // The tab should advance to the next 4-space-width tab stop, not a full 4-space-width interval
    assert!(
        layout.width() > 0.0,
        "layout should have positive width with tab"
    );

    env.with_name("after_text").check_layout_snapshot(&layout);
}

#[test]
fn tab_size_zero() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::TABBED;

    // Zero tab-size should produce zero-width tabs
    let layout = build_with_tab_size(&mut env, text, TabSize::Spaces(0.0));

    env.with_name("zero").check_layout_snapshot(&layout);
}

#[test]
fn tab_size_multiple_tabs() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = samples::TABBED;

    // Each tab should advance to the next tab stop
    let layout = build_with_tab_size(&mut env, text, TabSize::Spaces(4.0));

    env.with_name("multiple").check_layout_snapshot(&layout);
}

#[test]
fn tab_size_ranged_span() {
    let mut env = TestEnv::new(test_name!(), None);
    // Text with tabs in different style spans
    let text = "A\tB\tC";
    let mut builder = env.ranged_builder(text);
    // First tab gets TabSize::Spaces(4.0), second gets TabSize::Spaces(8.0)
    builder.push_default(StyleProperty::TabSize(TabSize::Spaces(4.0)));
    builder.push(StyleProperty::TabSize(TabSize::Spaces(8.0)), 3..text.len());
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(None, Alignment::Start, AlignmentOptions::default());

    env.with_name("ranged").check_layout_snapshot(&layout);
}

#[test]
fn tab_size_cross_run_position() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = "AB\tCD";

    // Force a shaping run break before the tab by applying a tiny
    // letter-spacing to the second half: runs become ["AB"] ["\tCD"].
    // The tab cluster's advance must be position-dependent (accounting for
    // preceding "AB"), not a full tab-interval (as if x started at 0).
    let mut builder = env.ranged_builder(text);
    builder.push_default(StyleProperty::TabSize(TabSize::Spaces(4.0)));
    builder.push(StyleProperty::LetterSpacing(0.001), 2..text.len());
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(None, Alignment::Start, AlignmentOptions::default());

    let line = layout.get(0).expect("should have a line");
    let runs: Vec<_> = line.runs().collect();
    assert!(
        runs.len() >= 2,
        "letter-spacing change should cause a shaping run break, got {} run(s)",
        runs.len(),
    );

    // Compute "AB" advance from run 0
    let preceding_advance: f32 = runs[0].clusters().map(|c| c.advance()).sum();
    assert!(
        preceding_advance > 0.0,
        "preceding text should have positive advance"
    );

    // The tab is the first cluster of run 1
    let tab_cluster = runs[1]
        .clusters()
        .next()
        .expect("run 1 should have clusters");
    let tab_advance = tab_cluster.advance();

    // A tab after non-zero preceding text must advance less than a tab at x=0
    // (which gets the full interval). Build a reference layout to get that value.
    let ref_layout = build_with_tab_size(&mut env, "\tX", TabSize::Spaces(4.0));
    let ref_line = ref_layout.get(0).unwrap();
    let ref_tab_advance = ref_line
        .runs()
        .next()
        .unwrap()
        .clusters()
        .next()
        .unwrap()
        .advance();

    // The tab after "AB" must advance less than a tab at x=0 because "AB"
    // partially fills the first tab interval.  If finish_line() incorrectly
    // resets x to 0 per-run, the tab advance would equal ref_tab_advance.
    assert!(
        tab_advance < ref_tab_advance,
        "tab after preceding text should advance less than a tab at line start: \
         tab_advance={tab_advance}, full_interval={ref_tab_advance}, \
         preceding_advance={preceding_advance}",
    );
}
