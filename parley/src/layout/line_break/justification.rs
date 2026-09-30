// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

use super::{BreakLines, BreakerState, LineTabOrigin};
use crate::layout::data::{ClusterData, LayoutData, LineData, LineItemData};
use crate::{Brush, Glyph, InlineBox};

#[derive(Clone)]
enum CandidateMutation {
    Cluster {
        index: usize,
        before: ClusterData,
        after: ClusterData,
    },
    Glyph {
        index: usize,
        before: Glyph,
        after: Glyph,
    },
    InlineBox {
        index: usize,
        before: InlineBox,
        after: InlineBox,
    },
}

#[derive(Default)]
pub(super) struct CandidateMutations(Vec<CandidateMutation>);

impl CandidateMutations {
    pub(super) fn cluster(
        journal: &mut Option<Self>,
        index: usize,
        value: &mut ClusterData,
        update: impl FnOnce(&mut ClusterData),
    ) {
        let before = *value;
        update(value);
        if let Some(journal) = journal {
            journal.0.push(CandidateMutation::Cluster {
                index,
                before,
                after: *value,
            });
        }
    }

    pub(super) fn glyph(
        journal: &mut Option<Self>,
        index: usize,
        value: &mut Glyph,
        update: impl FnOnce(&mut Glyph),
    ) {
        let before = *value;
        update(value);
        if let Some(journal) = journal {
            journal.0.push(CandidateMutation::Glyph {
                index,
                before,
                after: *value,
            });
        }
    }

    pub(super) fn inline_box(
        journal: &mut Option<Self>,
        index: usize,
        value: &mut InlineBox,
        update: impl FnOnce(&mut InlineBox),
    ) {
        let before = journal.as_ref().map(|_| value.clone());
        update(value);
        if let (Some(journal), Some(before)) = (journal, before) {
            journal.0.push(CandidateMutation::InlineBox {
                index,
                before,
                after: value.clone(),
            });
        }
    }

    fn restore<B: Brush>(&self, data: &mut LayoutData<B>) {
        for mutation in self.0.iter().rev() {
            match mutation {
                CandidateMutation::Cluster { index, before, .. } => data.clusters[*index] = *before,
                CandidateMutation::Glyph { index, before, .. } => data.glyphs[*index] = *before,
                CandidateMutation::InlineBox { index, before, .. } => {
                    data.inline_boxes[*index] = before.clone();
                }
            }
        }
    }

    fn apply<B: Brush>(&self, data: &mut LayoutData<B>) {
        for mutation in &self.0 {
            match mutation {
                CandidateMutation::Cluster { index, after, .. } => data.clusters[*index] = *after,
                CandidateMutation::Glyph { index, after, .. } => data.glyphs[*index] = *after,
                CandidateMutation::InlineBox { index, after, .. } => {
                    data.inline_boxes[*index] = after.clone();
                }
            }
        }
    }
}

pub(crate) struct LineShapeCandidate {
    pub(super) state: BreakerState,
    pub(super) line: LineData,
    pub(super) items: Vec<LineItemData>,
    mutations: CandidateMutations,
    done: bool,
    pub(super) physical_shape: Option<crate::shape::physical::PhysicalShape>,
}

impl LineShapeCandidate {
    pub(super) fn source_end<B: Brush>(
        &self,
        data: &LayoutData<B>,
    ) -> super::source_probe::SourceCursor {
        super::source_probe::SourceCursor::at(data, self.state.item_idx, self.state.cluster_idx)
    }
}

impl<B: Brush> BreakLines<'_, B> {
    pub(crate) fn preview_shape_candidate(
        &mut self,
        max_advance: f32,
        tab_origin: LineTabOrigin,
    ) -> Option<LineShapeCandidate> {
        if self.physical_shaper.is_some() && self.candidate_mutations.is_none() {
            return self.preview_physical_candidate(max_advance, tab_origin);
        }
        self.preview_natural_candidate(max_advance, tab_origin)
    }

    pub(super) fn preview_natural_candidate(
        &mut self,
        max_advance: f32,
        tab_origin: LineTabOrigin,
    ) -> Option<LineShapeCandidate> {
        self.preview_shape_candidate_with(|breaker| {
            breaker.break_next(max_advance, tab_origin).map(|_| ())
        })
    }

    pub(super) fn preview_shape_candidate_with(
        &mut self,
        measure: impl FnOnce(&mut Self) -> Option<()>,
    ) -> Option<LineShapeCandidate> {
        let state = self.state.clone();
        let item_start = self.lines.line_items.len();
        let line_start = self.lines.lines.len();
        let done = self.done;
        let previous = self.prev_state.clone();
        self.candidate_mutations = Some(CandidateMutations::default());
        let measured = measure(self);
        let mutations = self.candidate_mutations.take().expect("active candidate");
        mutations.restore(&mut self.layout.data);
        let candidate = measured.map(|_| LineShapeCandidate {
            state: self.state.clone(),
            line: self.lines.lines.pop().expect("measured line"),
            items: self.lines.line_items.split_off(item_start),
            mutations,
            done: self.done,
            physical_shape: None,
        });
        self.lines.lines.truncate(line_start);
        self.lines.line_items.truncate(item_start);
        self.state = state;
        self.prev_state = previous;
        self.done = done;
        self.letter_spacing_edges.clear();
        candidate
    }

    pub(crate) fn commit_shape_candidate(
        &mut self,
        mut candidate: LineShapeCandidate,
    ) -> (f32, f32) {
        self.prev_state = Some(self.state.clone());
        if let Some(shape) = candidate.physical_shape.take() {
            self.physical_shaper
                .as_mut()
                .expect("physical source shaper")
                .keep(&mut self.layout.data, &shape);
        }
        candidate.mutations.apply(&mut self.layout.data);
        if let Some(candidates) = &mut self.shape_candidates {
            candidates.remember(&self.layout.data, &candidate.items);
            self.previous_shape_commit = Some(CommittedShape {
                expanded: candidates.expanded,
                mutations: candidate.mutations,
            });
        }
        if let Some(shaper) = &mut self.physical_shaper {
            shaper.release_line_segments();
        }
        self.state = candidate.state;
        self.done = candidate.done;
        let measured = (candidate.line.metrics.advance, candidate.line.size());
        self.lines.lines.push(candidate.line);
        self.lines.line_items.extend(candidate.items);
        measured
    }
}

pub(super) struct ShapeCandidateBuffers<B: Brush> {
    other: Vec<ClusterData>,
    selected: Vec<ClusterData>,
    pub(super) expanded: bool,
    active_suffix: Option<core::ops::Range<usize>>,
    suffixes: alloc::collections::BTreeMap<usize, Vec<ClusterData>>,
    suffix_shaper: Option<crate::shape::justification::SourceSuffixShaper<B>>,
}

impl<B: Brush> ShapeCandidateBuffers<B> {
    pub(super) fn new(data: &mut LayoutData<B>) -> Option<Self> {
        if data.line_shape_variants.is_none() && !data.inline_owner_shaping.is_empty() {
            data.line_shape_variants = Some(crate::shape::justification::LineShapeVariants {
                original: data.clusters.clone(),
                expanded: data.clusters.clone(),
                original_glyph_len: data.glyphs.len(),
                variant_glyph_len: data.glyphs.len(),
                policy: crate::JustificationShapePolicy::default(),
            });
        }
        let variants = data.line_shape_variants.as_ref()?;
        data.clusters.clone_from(&variants.original);
        data.glyphs.truncate(variants.variant_glyph_len);
        Some(Self {
            other: variants.expanded.clone(),
            selected: variants.original.clone(),
            expanded: false,
            active_suffix: None,
            suffixes: alloc::collections::BTreeMap::new(),
            suffix_shaper: None,
        })
    }

    fn select(&mut self, data: &mut LayoutData<B>, expanded: bool) {
        if self.expanded != expanded {
            core::mem::swap(&mut self.other, &mut data.clusters);
            self.expanded = expanded;
        }
    }

    fn prepare_source_start(
        &mut self,
        data: &mut LayoutData<B>,
        item_index: usize,
        cluster_index: usize,
    ) {
        let variants = data
            .line_shape_variants
            .as_ref()
            .expect("retained variants");
        if let Some(range) = self.active_suffix.take() {
            data.clusters[range.clone()].copy_from_slice(&variants.original[range]);
        }
        let Some(item) = data.items[item_index..].iter().find(|item| {
            item.kind == crate::layout::data::LayoutItemKind::TextRun
                && item.cluster_range.end > cluster_index
        }) else {
            return;
        };
        let run_index = item.index;
        let run = &data.runs[run_index];
        let start = item.cluster_range.start.max(cluster_index);
        if start <= run.cluster_range.start || start >= run.cluster_range.end {
            return;
        }
        let is_internal = if run.bidi_level & 1 == 0 {
            variants.original[start].is_ligature_component()
        } else {
            variants.original[start - 1].is_ligature_component()
        };
        if !is_internal {
            return;
        }
        if !self.suffixes.contains_key(&start) {
            let source_start = variants.original[start].text_range(run).start;
            let shaper = self
                .suffix_shaper
                .get_or_insert_with(|| crate::shape::justification::SourceSuffixShaper::new(data));
            let mut shape = shaper.shape(data, run_index, source_start);
            let run = &data.runs[run_index];
            let glyph_offset = data.glyphs.len() - run.glyph_start;
            for cluster in &mut shape.clusters {
                if cluster.glyph_len != 0xFF {
                    cluster.glyph_offset += glyph_offset as u32;
                }
                let source = cluster.text_range(run).start;
                if let Ok(index) = data
                    .source_cluster_fit_advances
                    .binary_search_by_key(&source, |entry| entry.byte_index)
                {
                    cluster.line_break_advance = data.source_cluster_fit_advances[index].advance;
                }
            }
            data.glyphs.extend_from_slice(&shape.glyphs);
            self.suffixes.insert(start, shape.clusters);
        }
        let clusters = &self.suffixes[&start];
        let range = start..start + clusters.len();
        data.clusters[range.clone()].copy_from_slice(clusters);
        self.active_suffix = Some(range);
    }

    pub(super) fn remember(&mut self, data: &LayoutData<B>, items: &[LineItemData]) {
        for item in items.iter().filter(|item| item.is_text_run()) {
            self.selected[item.cluster_range.clone()]
                .copy_from_slice(&data.clusters[item.cluster_range.clone()]);
        }
    }

    pub(super) fn finish(self, data: &mut LayoutData<B>) {
        data.clusters = self.selected;
    }
}

impl<B: Brush> BreakLines<'_, B> {
    fn select_shape(&mut self, expanded: bool) {
        self.shape_candidates
            .as_mut()
            .expect("shape alternatives")
            .select(&mut self.layout.data, expanded);
        self.letter_spacing_edges.clear();
    }

    fn prepare_natural_shape(&mut self) {
        self.select_shape(false);
        if self.physical_shaper.is_none() {
            self.shape_candidates
                .as_mut()
                .expect("shape alternatives")
                .prepare_source_start(
                    &mut self.layout.data,
                    self.state.item_idx,
                    self.state.cluster_idx,
                );
        }
    }

    pub(super) fn break_length_with_original_shape(&mut self, max_chars: u32) -> Option<()> {
        self.prepare_natural_shape();
        let mut candidate =
            self.preview_shape_candidate_with(|breaker| breaker.break_next_with_length(max_chars))?;
        if self.physical_shaper.is_some() {
            let (shape, _) = self.physical_prefix_shape(&candidate, None, false);
            let installed = self.install_physical_shape(&shape);
            candidate = self
                .preview_shape_candidate_with(|breaker| breaker.break_next_with_length(max_chars))
                .expect("same retained source count");
            self.remove_physical_shape(installed);
            candidate.physical_shape = Some(shape);
        }
        self.commit_shape_candidate(candidate);
        Some(())
    }

    pub(super) fn break_next_with_shapes(
        &mut self,
        max_advance: f32,
        tab_origin: LineTabOrigin,
    ) -> Option<(f32, f32)> {
        self.prepare_natural_shape();
        let natural = self.preview_shape_candidate(max_advance, tab_origin)?;
        let policy = self
            .layout
            .data
            .line_shape_variants
            .as_ref()
            .expect("shape policy")
            .policy;
        if !line_needs_justification_shape(&self.layout.data, &natural.line, &natural.items, policy)
        {
            return Some(self.commit_shape_candidate(natural));
        }
        self.select_shape(true);
        self.reject_terminal_candidate = !self
            .layout
            .data
            .line_shape_variants
            .as_ref()
            .expect("shape policy")
            .policy
            .terminal_lines;
        let expanded = self
            .preview_shape_candidate(max_advance, tab_origin)
            .expect("same retained source");
        self.reject_terminal_candidate = false;
        if !policy.expands(expanded.line.break_reason)
            || expanded.line.fitting_advance() > expanded.line.max_advance
        {
            self.select_shape(false);
            let mut natural = natural;
            natural.line.justification_shape_fallback = true;
            return Some(self.commit_shape_candidate(natural));
        }
        Some(self.commit_shape_candidate(expanded))
    }
}

pub(super) struct CommittedShape {
    expanded: bool,
    mutations: CandidateMutations,
}

impl<B: Brush> BreakLines<'_, B> {
    pub(super) fn revert_shape_candidate(&mut self) {
        let Some(committed) = self.previous_shape_commit.take() else {
            return;
        };
        self.select_shape(committed.expanded);
        committed.mutations.restore(&mut self.layout.data);
        let line = self.lines.lines.last().expect("committed shape line");
        let variants = self
            .layout
            .data
            .line_shape_variants
            .as_ref()
            .expect("shape alternatives");
        let candidates = self.shape_candidates.as_mut().expect("shape candidates");
        for item in self.lines.line_items[line.item_range.clone()]
            .iter()
            .filter(|item| item.is_text_run())
        {
            candidates.selected[item.cluster_range.clone()]
                .copy_from_slice(&variants.original[item.cluster_range.clone()]);
        }
        self.letter_spacing_edges.clear();
    }
}

pub(crate) fn line_needs_justification_shape<B: Brush>(
    data: &LayoutData<B>,
    line: &LineData,
    items: &[LineItemData],
    policy: crate::JustificationShapePolicy,
) -> bool {
    if line.max_advance == f32::MAX
        || !policy.expands(line.break_reason)
        || crate::layout::alignment::line_contains_tab(items, &data.clusters)
    {
        return false;
    }
    line.max_advance - line.fitting_advance() > 0.0
        && data
            .justification_opportunities
            .text_boundaries(line.text_range.clone())
            .next()
            .is_some()
}
