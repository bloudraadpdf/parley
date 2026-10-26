// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Paragraph-at-a-time line breaking: Knuth and Plass, "Breaking Paragraphs into Lines" (1981), with the
//! final pass of TeX (§ 863 of "TeX: The Program").

use crate::layout::DiscretionaryBreakCondition;
use alloc::vec::Vec;

#[cfg(feature = "libm")]
#[allow(unused_imports)]
use core_maths::CoreFloat;

/// How a paragraph chooses the ends of its lines.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LineBreakStrategy {
    /// Each line takes the most content that fits.
    #[default]
    FirstFit,
    /// The lines of each segment between forced breaks minimise their total demerits.
    TotalFit(TotalFit),
}

/// The parameters of total-fit line breaking.
///
/// A line that its separators stretch by `r` of their stretch, or shrink by `r` of their shrink, has the badness
/// `100 r³` and the demerits `(line_penalty + badness)²`, plus `hyphen_penalty²` when it ends at a hyphenated
/// break. The line that ends a segment has no stretch badness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TotalFit {
    /// The stretch of a word separator, as a fraction of its font size.
    pub stretch_em: f32,
    /// The largest badness of a line that does not end its segment.
    pub tolerance: f32,
    /// The demerits that each line adds.
    pub line_penalty: f32,
    /// The penalty of a line that ends at a written dash or discretionary break.
    pub hyphen_penalty: f32,
    /// The demerits of consecutive hyphenated breaks.
    pub double_hyphen_demerits: f32,
    /// The demerits of a last line that follows a hyphenated break.
    pub final_hyphen_demerits: f32,
}

/// The progress of a line from the start of a probe line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Measure {
    /// The fit advance.
    pub(super) advance: f32,
    /// The sum of the font sizes of the word separators.
    pub(super) separator_font_sizes: f32,
    /// The advance by which the word separators may shrink.
    pub(super) shrink: f32,
}

/// The position of a soft wrap opportunity among the source items: its item and its cluster.
pub(super) type BreakCursor = (usize, usize);

/// A soft wrap opportunity of a probe line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Opportunity {
    pub(super) cursor: BreakCursor,
    pub(super) projected_boundary: Option<usize>,
    /// The end of a line that breaks here: its content without the trailing separators, with any break material.
    pub(super) end: Measure,
    /// The start of the line that follows the break.
    pub(super) start: Measure,
    /// Whether the break is discretionary.
    pub(super) discretionary: Option<DiscretionaryBreakCondition>,
    pub(super) authored_dash: bool,
}

/// The measures of a segment between forced breaks.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Segment {
    pub(super) start: Measure,
    pub(super) opportunities: Vec<Opportunity>,
    /// The end of the content of the segment.
    pub(super) end: Option<Measure>,
}

/// A line end of the plan: an opportunity, or the end of the segment.
#[derive(Clone, Copy)]
struct Node {
    /// 0 for the start of the segment, `k + 1` for the opportunity `k`.
    point: usize,
    demerits: f32,
    previous: Option<usize>,
    hyphenated: bool,
}

enum LineFit {
    Overfull,
    Badness(f32),
}

impl Segment {
    /// Records an opportunity once, including a source boundary shared by nested inline edges.
    pub(super) fn record(&mut self, opportunity: Opportunity) {
        match self.opportunities.last_mut() {
            Some(last)
                if last.cursor == opportunity.cursor
                    || (opportunity.projected_boundary.is_some()
                        && last.projected_boundary == opportunity.projected_boundary) =>
            {
                *last = opportunity;
            }
            _ => self.opportunities.push(opportunity),
        }
    }

    fn start_of(&self, point: usize) -> Measure {
        point
            .checked_sub(1)
            .map_or(self.start, |k| self.opportunities[k].start)
    }

    fn end_of(&self, point: usize, end: Measure) -> Measure {
        point
            .checked_sub(1)
            .and_then(|k| self.opportunities.get(k))
            .map_or(end, |opportunity| opportunity.end)
    }

    /// The line ends of the segment that minimise its demerits, the ends at opportunities only; `None` without a
    /// recorded segment end.
    pub(super) fn plan(
        &self,
        params: TotalFit,
        first_width: f32,
        width: f32,
        mut shape_adjustment: impl FnMut(Option<BreakCursor>, Option<BreakCursor>) -> f32,
    ) -> Option<Vec<BreakCursor>> {
        let end = self.end?;
        let last = self.opportunities.len() + 1;
        let mut fit = |from: usize, to: usize| -> LineFit {
            let start = self.start_of(from);
            let stop = self.end_of(to, end);
            let available = if from == 0 { first_width } else { width };
            let cursor = |point: usize| {
                point
                    .checked_sub(1)
                    .and_then(|index| self.opportunities.get(index))
                    .map(|opportunity| opportunity.cursor)
            };
            let slack = available
                - (stop.advance - start.advance + shape_adjustment(cursor(from), cursor(to)));
            let shrink = stop.shrink - start.shrink;
            if slack < -shrink - SLACK_EPSILON {
                return if to == from + 1 {
                    LineFit::Badness(0.0)
                } else {
                    LineFit::Overfull
                };
            }
            if to == last && slack >= 0.0 {
                return LineFit::Badness(0.0);
            }
            let ratio = if slack < 0.0 {
                -slack / shrink
            } else {
                let stretch =
                    params.stretch_em * (stop.separator_font_sizes - start.separator_font_sizes);
                if stretch > 0.0 {
                    slack / stretch
                } else if slack > 0.0 {
                    f32::INFINITY
                } else {
                    0.0
                }
            };
            LineFit::Badness(100.0 * ratio * ratio * ratio)
        };
        let hyphenated = |point: usize| {
            point
                .checked_sub(1)
                .and_then(|k| self.opportunities.get(k))
                .is_some_and(|opportunity| {
                    opportunity.discretionary.is_some() || opportunity.authored_dash
                })
        };
        let line_demerits = |node: Node, point: usize, badness: f32| {
            let hyphenation = match (node.hyphenated, hyphenated(point), point == last) {
                (true, true, _) => params.hyphen_penalty.powi(2) + params.double_hyphen_demerits,
                (_, true, _) => params.hyphen_penalty.powi(2),
                (true, false, true) => params.final_hyphen_demerits,
                _ => 0.0,
            };
            node.demerits + (params.line_penalty + badness).powi(2) + hyphenation
        };
        let mut nodes = alloc::vec![Node {
            point: 0,
            demerits: 0.0,
            previous: None,
            hyphenated: false,
        }];
        let mut active = alloc::vec![0_usize];
        let mut point = 1;
        let mut enabled_word = None;
        while point <= last {
            let word_end = self.opportunities.get(point - 1).and_then(|opportunity| {
                (opportunity.discretionary == Some(DiscretionaryBreakCondition::WordOverflow)).then(
                    || {
                        (point..self.opportunities.len())
                            .find(|&next| self.opportunities[next].discretionary.is_none())
                            .map_or(last, |next| next + 1)
                    },
                )
            });
            if let Some(end) = word_end {
                if enabled_word != Some(end)
                    && active
                        .iter()
                        .any(|&index| matches!(fit(nodes[index].point, end), LineFit::Overfull))
                {
                    enabled_word = Some(end);
                }
                if enabled_word != Some(end) {
                    point += 1;
                    continue;
                }
            }
            let mut best: Option<Node> = None;
            let mut fallback: Option<Node> = None;
            let mut index = 0;
            let mut forced = false;
            while index < active.len() {
                let node_index = active[index];
                let node = nodes[node_index];
                let badness = match fit(node.point, point) {
                    LineFit::Overfull => {
                        active.remove(index);
                        let underfull = (point - 1).max(node.point + 1);
                        let badness = match fit(node.point, underfull) {
                            LineFit::Badness(value) => value,
                            LineFit::Overfull => f32::INFINITY,
                        };
                        let candidate = Node {
                            point: underfull,
                            demerits: line_demerits(node, underfull, badness),
                            previous: Some(node_index),
                            hyphenated: hyphenated(underfull),
                        };
                        let candidate = fallback
                            .filter(|best| best.demerits <= candidate.demerits)
                            .unwrap_or(candidate);
                        fallback = Some(candidate);
                        if active.is_empty() && best.is_none() {
                            nodes.push(candidate);
                            active.push(nodes.len() - 1);
                            forced = true;
                            break;
                        }
                        continue;
                    }
                    LineFit::Badness(badness) => badness,
                };
                if point == last || badness <= params.tolerance {
                    let ends_hyphenated = hyphenated(point);
                    let demerits = line_demerits(node, point, badness);
                    if best.is_none_or(|best| demerits < best.demerits) {
                        best = Some(Node {
                            point,
                            demerits,
                            previous: Some(node_index),
                            hyphenated: ends_hyphenated,
                        });
                    }
                }
                index += 1;
            }
            if forced {
                continue;
            }
            if let Some(best) = best {
                nodes.push(best);
                active.push(nodes.len() - 1);
            }
            point += 1;
        }
        let mut node = nodes.iter().rposition(|node| node.point == last)?;
        let mut cursors = Vec::new();
        while let Some(previous) = nodes[node].previous {
            if nodes[node].point != last {
                cursors.push(self.opportunities[nodes[node].point - 1].cursor);
            }
            node = previous;
        }
        cursors.reverse();
        Some(cursors)
    }
}

/// The slack below which a line overflows its shrink: the rounding of a sum of advances.
const SLACK_EPSILON: f32 = 1e-3;

#[cfg(test)]
mod tests {
    use super::*;

    const SPACE: f32 = 3.0;

    /// A segment of words with one space between each two.
    fn segment(words: &[f32]) -> Segment {
        segment_with_metrics(words, SPACE, 12.0)
    }

    fn segment_with_metrics(words: &[f32], space: f32, font_size: f32) -> Segment {
        let mut opportunities = Vec::new();
        let mut advance = 0.0;
        let mut separators = 0.0;
        let mut separator_font_sizes = 0.0;
        for (index, word) in words.iter().enumerate() {
            advance += word;
            if index + 1 == words.len() {
                break;
            }
            let end = Measure {
                advance,
                separator_font_sizes,
                shrink: separators / 6.0,
            };
            advance += space;
            separators += space;
            separator_font_sizes += font_size;
            let start = Measure {
                advance,
                separator_font_sizes,
                shrink: separators / 6.0,
            };
            opportunities.push(Opportunity {
                cursor: (0, index + 1),
                projected_boundary: None,
                end,
                start,
                discretionary: None,
                authored_dash: false,
            });
        }
        Segment {
            start: Measure::default(),
            opportunities,
            end: Some(Measure {
                advance,
                separator_font_sizes,
                shrink: separators / 6.0,
            }),
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

    fn plan(words: &[f32], width: f32) -> Vec<usize> {
        plan_with(segment(words), PRINCE, width)
    }

    fn plan_with(segment: Segment, params: TotalFit, width: f32) -> Vec<usize> {
        segment
            .plan(params, width, width, |_, _| 0.0)
            .expect("a segment with an end has a plan")
            .into_iter()
            .map(|(_, cluster)| cluster)
            .collect()
    }

    /// Two lines of nine words: the first fills its measure, the second keeps `slack`; the tenth word may move.
    fn two_lines(slack: f32) -> Vec<f32> {
        let width = 200.0;
        let mover = 10.0;
        let first = (width - 8.0 * SPACE - mover) / 8.0;
        let second = (width - slack - 8.0 * SPACE) / 9.0;
        let mut words = alloc::vec![first; 8];
        words.push(mover);
        words.extend([second; 9]);
        words.push(180.0);
        words
    }

    #[test]
    fn a_word_moves_when_that_evens_the_separators_of_two_lines() {
        assert_eq!(plan(&two_lines(4.0 * SPACE), 200.0), [9, 18]);
        assert_eq!(plan(&two_lines(6.0 * SPACE), 200.0), [8, 18]);
    }

    #[test]
    fn a_hyphenated_break_pays_its_penalty() {
        for authored_dash in [false, true] {
            let mut segment = segment(&two_lines(6.0 * SPACE));
            segment.opportunities[7].authored_dash = authored_dash;
            segment.opportunities[7].discretionary =
                (!authored_dash).then_some(DiscretionaryBreakCondition::Normal);
            assert_eq!(plan_with(segment.clone(), PRINCE, 200.0), [8, 18]);
            let costly = TotalFit {
                hyphen_penalty: 300.0,
                ..PRINCE
            };
            assert_eq!(plan_with(segment, costly, 200.0), [9, 18]);
        }
    }

    #[test]
    fn an_automatic_hyphen_waits_until_the_unbroken_word_overflows() {
        let mut segment = segment(&two_lines(6.0 * SPACE));
        segment.opportunities[7].discretionary = Some(DiscretionaryBreakCondition::WordOverflow);
        assert_eq!(plan_with(segment.clone(), PRINCE, 200.0), [9, 18]);
        assert_eq!(plan_with(segment, PRINCE, 195.0).first(), Some(&8));
    }

    #[test]
    fn an_automatic_hyphen_in_the_final_word_uses_the_segment_end() {
        let mut segment = segment(&[80.0, 10.0]);
        segment.opportunities[0].discretionary = Some(DiscretionaryBreakCondition::WordOverflow);
        assert!(plan_with(segment.clone(), PRINCE, 100.0).is_empty());
        assert_eq!(plan_with(segment, PRINCE, 85.0), [1]);
    }

    /// Every line but the last stretches beyond the tolerance: each line ends where the next word overflows.
    #[test]
    fn an_underfull_line_ends_where_no_active_break_reaches_further() {
        assert_eq!(plan(&[90.0; 5], 200.0), [2, 4]);
    }

    #[test]
    fn an_underfull_line_keeps_the_lowest_cost_predecessor() {
        let segment = segment_with_metrics(
            &[
                21.0, 6.0, 14.0, 28.0, 27.0, 44.0, 40.0, 9.0, 32.0, 22.0, 19.0, 24.0, 7.0, 33.0,
                45.0, 47.0, 46.0, 17.0, 54.0, 8.0, 30.0, 34.0,
            ],
            10.0 * 229.0 / 1024.0,
            10.0,
        );
        assert_eq!(plan_with(segment, PRINCE, 165.07874), [6, 12, 16, 21]);
    }

    #[test]
    fn the_last_line_has_no_stretch_badness() {
        assert_eq!(plan(&[27.0; 7], 120.0), [4]);
    }

    #[test]
    fn the_last_line_retains_its_shrink_badness() {
        let mut words = [10.0; 36];
        words[35] = 11.0;
        assert_eq!(plan(&words, 149.5), [11, 22, 33]);
    }
}
