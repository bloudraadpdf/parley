// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use core::ops::Range;

use super::{
    ShapeContext,
    source::{DeferredSourceShape, PreparedSourceShape, SourceShapePlan},
};
use crate::{
    Brush,
    layout::data::{ClusterData, LayoutData},
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct WindowKey {
    run: usize,
    start: usize,
    end: usize,
    context: (usize, usize),
    boundaries: Vec<usize>,
    segments: Vec<usize>,
    expanded: bool,
}

pub(crate) struct PhysicalShaper<B: Brush> {
    context: ShapeContext,
    scratch: LayoutData<B>,
    original: Arc<Vec<ClusterData>>,
    expanded: Arc<Vec<ClusterData>>,
    windows: BTreeMap<WindowKey, Arc<PreparedSourceShape>>,
}

struct PhysicalWindow {
    run: usize,
    clusters: Range<usize>,
    shape: Arc<PreparedSourceShape>,
}

pub(crate) struct PhysicalShape {
    baseline: Arc<Vec<ClusterData>>,
    windows: Vec<PhysicalWindow>,
}

impl PhysicalShape {
    pub(crate) fn install<B: Brush>(&self, data: &mut LayoutData<B>) {
        data.clusters.clone_from(&self.baseline);
        for window in &self.windows {
            window.shape.install_for_run(
                &data.runs[window.run],
                &mut data.clusters[window.clusters.clone()],
                &mut data.glyphs,
            );
        }
        crate::layout::apply_source_fit_projection(
            &data.runs,
            &mut data.clusters,
            &data.source_cluster_fit_advances,
        );
    }
}

fn affected_windows(
    range: &Range<usize>,
    safe: &[usize],
    mut points: Vec<usize>,
) -> Vec<Range<usize>> {
    points.sort_unstable();
    points.dedup();
    let mut windows: Vec<Range<usize>> = Vec::new();
    for point in points {
        let before = safe.partition_point(|boundary| *boundary <= point);
        let start = before
            .checked_sub(1)
            .map_or(range.start, |index| safe[index]);
        if before != 0 && start == point {
            continue;
        }
        let end = safe.get(before).copied().unwrap_or(range.end);
        if start >= end {
            continue;
        }
        if let Some(previous) = windows.last_mut().filter(|previous| previous.end >= start) {
            previous.end = previous.end.max(end);
        } else {
            windows.push(start..end);
        }
    }
    windows
}

impl<B: Brush> PhysicalShaper<B> {
    pub(crate) fn new(data: &LayoutData<B>) -> Self {
        let original = Arc::new(data.line_shape_variants.as_ref().map_or_else(
            || data.clusters.clone(),
            |variants| variants.original.clone(),
        ));
        let expanded = data
            .line_shape_variants
            .as_ref()
            .filter(|variants| variants.policy != crate::JustificationShapePolicy::default())
            .map_or_else(
                || Arc::clone(&original),
                |variants| Arc::new(variants.expanded.clone()),
            );
        Self {
            context: ShapeContext::default(),
            scratch: super::source::source_scratch(data),
            original,
            expanded,
            windows: BTreeMap::new(),
        }
    }

    pub(crate) fn shape_line(
        &mut self,
        data: &LayoutData<B>,
        boundaries: &[usize],
        soft_boundaries: &[usize],
        expanded: bool,
    ) -> PhysicalShape {
        let source = data
            .shaping_source_text
            .as_deref()
            .expect("retained physical shaping source");
        assert!(
            boundaries
                .iter()
                .chain(soft_boundaries)
                .all(|boundary| source.is_char_boundary(*boundary)),
            "physical shaping boundaries must be UTF-8 positions"
        );
        let mut boundaries = boundaries.to_vec();
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut windows = Vec::new();
        for deferred in &data.deferred_physical_shapes {
            let run = &data.runs[deferred.run_index];
            let alternative = expanded
                .then(|| {
                    data.deferred_justification_shapes
                        .iter()
                        .find(|candidate| candidate.source.run_index == deferred.run_index)
                })
                .flatten();
            let safe = deferred
                .safe_concat_boundaries
                .iter()
                .copied()
                .filter(|boundary| {
                    alternative
                        .and_then(|shape| shape.prepared.as_ref())
                        .is_none_or(|shape| {
                            shape.safe_concat_boundaries.binary_search(boundary).is_ok()
                        })
                })
                .collect::<Vec<_>>();
            let points = boundaries
                .iter()
                .copied()
                .filter(|boundary| {
                    *boundary > deferred.context.start && *boundary < deferred.context.end
                })
                .map(|boundary| boundary.clamp(run.text_range.start, run.text_range.end))
                .chain(soft_boundaries.iter().copied().filter(|boundary| {
                    *boundary > run.text_range.start && *boundary < run.text_range.end
                }))
                .collect();
            for range in affected_windows(&run.text_range, &safe, points) {
                let context = (
                    if safe.binary_search(&range.start).is_ok() {
                        range.start
                    } else {
                        deferred.context.start
                    },
                    if safe.binary_search(&range.end).is_ok() {
                        range.end
                    } else {
                        deferred.context.end
                    },
                );
                let relevant = boundaries
                    .iter()
                    .copied()
                    .filter(|boundary| *boundary > context.0 && *boundary < context.1)
                    .collect::<Vec<_>>();
                let mut segments = relevant.clone();
                segments.extend(
                    soft_boundaries
                        .iter()
                        .copied()
                        .filter(|boundary| range.contains(boundary)),
                );
                segments.sort_unstable();
                segments.dedup();
                let key = WindowKey {
                    run: deferred.run_index,
                    start: range.start,
                    end: range.end,
                    context,
                    boundaries: relevant,
                    segments,
                    expanded,
                };
                if !self.windows.contains_key(&key) {
                    let natural =
                        self.prepare_window(data, source, deferred, &key, &deferred.features);
                    let prepared = if let Some(alternative) = alternative {
                        let features =
                            alternative.eligible_features(run, &natural.clusters, |range| {
                                data.justification_opportunities
                                    .text_boundaries(range)
                                    .next()
                                    .is_some()
                            });
                        if features == deferred.features {
                            natural
                        } else {
                            self.prepare_window(data, source, deferred, &key, &features)
                        }
                    } else {
                        natural
                    };
                    self.windows.insert(key.clone(), Arc::new(prepared));
                }
                let original = &self.original[run.cluster_range.clone()];
                let first =
                    original.partition_point(|cluster| cluster.text_range(run).start < range.start);
                let last =
                    original.partition_point(|cluster| cluster.text_range(run).start < range.end);
                windows.push(PhysicalWindow {
                    run: deferred.run_index,
                    clusters: run.cluster_range.start + first..run.cluster_range.start + last,
                    shape: Arc::clone(&self.windows[&key]),
                });
            }
        }
        PhysicalShape {
            baseline: Arc::clone(if expanded {
                &self.expanded
            } else {
                &self.original
            }),
            windows,
        }
    }

    fn prepare_window(
        &mut self,
        data: &LayoutData<B>,
        source: &str,
        deferred: &DeferredSourceShape,
        key: &WindowKey,
        features: &[harfrust::Feature],
    ) -> PreparedSourceShape {
        deferred.shape_with_boundaries(
            &mut self.context,
            &mut self.scratch,
            data,
            source,
            SourceShapePlan {
                source: key.start..key.end,
                context: key.context.0..key.context.1,
                context_boundaries: &key.boundaries,
                segment_boundaries: &key.segments,
                features,
            },
        )
    }
}
