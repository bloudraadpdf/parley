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
/// `100 r³` and the demerits `(line_penalty + badness)²`, plus `hyphen_penalty²` when it ends at a discretionary
/// break. The line that ends a segment has no badness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TotalFit {
    /// The stretch of a word separator, as a fraction of its advance.
    pub stretch: f32,
    /// The largest badness of a line that does not end its segment.
    pub tolerance: f32,
    /// The demerits that each line adds.
    pub line_penalty: f32,
    /// The penalty of a line that ends at a discretionary break.
    pub hyphen_penalty: f32,
    /// The demerits of a discretionary break that follows a discretionary break.
    pub double_hyphen_demerits: f32,
    /// The demerits of a last line that follows a discretionary break.
    pub final_hyphen_demerits: f32,
}

/// The progress of a line from the start of a probe line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Measure {
    /// The fit advance.
    pub(super) advance: f32,
    /// The fit advance of the word separators.
    pub(super) separators: f32,
    /// The advance by which the word separators may shrink.
    pub(super) shrink: f32,
}

/// The position of a soft wrap opportunity among the source items: its item and its cluster.
pub(super) type BreakCursor = (usize, usize);

/// A soft wrap opportunity of a probe line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Opportunity {
    pub(super) cursor: BreakCursor,
    /// The end of a line that breaks here: its content without the trailing separators, with any break material.
    pub(super) end: Measure,
    /// The start of the line that follows the break.
    pub(super) start: Measure,
    /// Whether the break is discretionary.
    pub(super) discretionary: Option<DiscretionaryBreakCondition>,
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
    discretionary: bool,
}

enum LineFit {
    Overfull,
    Badness(f32),
}

impl Segment {
    /// Records an opportunity; a later record at the same cursor replaces an earlier one.
    pub(super) fn record(&mut self, opportunity: Opportunity) {
        match self.opportunities.last_mut() {
            Some(last) if last.cursor == opportunity.cursor => *last = opportunity,
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
    ) -> Option<Vec<BreakCursor>> {
        let end = self.end?;
        let last = self.opportunities.len() + 1;
        let fit = |from: usize, to: usize| -> LineFit {
            let start = self.start_of(from);
            let stop = self.end_of(to, end);
            let available = if from == 0 { first_width } else { width };
            let slack = available - (stop.advance - start.advance);
            let shrink = stop.shrink - start.shrink;
            if slack < -shrink - SLACK_EPSILON {
                return if to == from + 1 {
                    LineFit::Badness(0.0)
                } else {
                    LineFit::Overfull
                };
            }
            if to == last {
                return LineFit::Badness(0.0);
            }
            let ratio = if slack < 0.0 {
                -slack / shrink
            } else {
                let stretch = params.stretch * (stop.separators - start.separators);
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
        let discretionary = |point: usize| {
            point
                .checked_sub(1)
                .and_then(|k| self.opportunities.get(k))
                .is_some_and(|opportunity| opportunity.discretionary.is_some())
        };
        let mut nodes = alloc::vec![Node {
            point: 0,
            demerits: 0.0,
            previous: None,
            discretionary: false,
        }];
        let mut active = alloc::vec![0_usize];
        let mut point = 1;
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
            let mut best: Option<Node> = None;
            let mut index = 0;
            let mut forced = false;
            while index < active.len() {
                let node_index = active[index];
                let node = nodes[node_index];
                if word_end.is_some_and(|end| !matches!(fit(node.point, end), LineFit::Overfull)) {
                    index += 1;
                    continue;
                }
                let badness = match fit(node.point, point) {
                    LineFit::Overfull => {
                        active.remove(index);
                        if active.is_empty() && best.is_none() {
                            let underfull = (point - 1).max(node.point + 1);
                            nodes.push(Node {
                                point: underfull,
                                demerits: node.demerits,
                                previous: Some(node_index),
                                discretionary: discretionary(underfull),
                            });
                            active.push(nodes.len() - 1);
                            forced = true;
                            break;
                        }
                        continue;
                    }
                    LineFit::Badness(badness) => badness,
                };
                if point == last || badness <= params.tolerance {
                    let ends_discretionary = discretionary(point);
                    let hyphenation = match (node.discretionary, ends_discretionary, point == last)
                    {
                        (_, true, _) if node.discretionary => {
                            params.hyphen_penalty.powi(2) + params.double_hyphen_demerits
                        }
                        (_, true, _) => params.hyphen_penalty.powi(2),
                        (true, false, true) => params.final_hyphen_demerits,
                        _ => 0.0,
                    };
                    let demerits =
                        node.demerits + (params.line_penalty + badness).powi(2) + hyphenation;
                    if best.is_none_or(|best| demerits < best.demerits) {
                        best = Some(Node {
                            point,
                            demerits,
                            previous: Some(node_index),
                            discretionary: ends_discretionary,
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
        let mut opportunities = Vec::new();
        let mut advance = 0.0;
        let mut separators = 0.0;
        for (index, word) in words.iter().enumerate() {
            advance += word;
            if index + 1 == words.len() {
                break;
            }
            let end = Measure {
                advance,
                separators,
                shrink: separators / 6.0,
            };
            advance += SPACE;
            separators += SPACE;
            let start = Measure {
                advance,
                separators,
                shrink: separators / 6.0,
            };
            opportunities.push(Opportunity {
                cursor: (0, index + 1),
                end,
                start,
                discretionary: None,
            });
        }
        Segment {
            start: Measure::default(),
            opportunities,
            end: Some(Measure {
                advance,
                separators,
                shrink: separators / 6.0,
            }),
        }
    }

    const PRINCE: TotalFit = TotalFit {
        stretch: 0.5,
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
            .plan(params, width, width)
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

    /// The penalty of a discretionary break weighs against the evenness it buys.
    #[test]
    fn a_discretionary_break_pays_its_penalty() {
        let mut segment = segment(&two_lines(6.0 * SPACE));
        segment.opportunities[7].discretionary = Some(DiscretionaryBreakCondition::Normal);
        assert_eq!(plan_with(segment.clone(), PRINCE, 200.0), [8, 18]);
        let costly = TotalFit {
            hyphen_penalty: 300.0,
            ..PRINCE
        };
        assert_eq!(plan_with(segment, costly, 200.0), [9, 18]);
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
    fn the_last_line_has_no_badness() {
        assert_eq!(plan(&[27.0; 7], 120.0), [4]);
    }
}
