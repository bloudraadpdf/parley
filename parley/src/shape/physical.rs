// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use core::ops::Range;

use super::{
    ShapeContext,
    source::{DeferredSourceShape, PreparedSourceShape},
};
use crate::{
    Brush,
    layout::{
        LineItemData, OwnerIndex,
        data::{ClusterData, LayoutData},
    },
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SegmentKey {
    run: usize,
    source: (usize, usize),
    context: (usize, usize),
    features: Vec<(harfrust::Tag, u32, u32, u32)>,
}

/// The justification alternative of a run, with the safe concatenation boundaries of both shapes when it is prepared.
struct Alternative {
    shape: usize,
    safe_concat_boundaries: Option<Vec<usize>>,
}

pub(crate) struct PhysicalShaper<B: Brush> {
    context: ShapeContext,
    scratch: LayoutData<B>,
    owner_edges: Vec<usize>,
    owners: OwnerIndex,
    alternatives: Vec<Option<Alternative>>,
    segments: BTreeMap<SegmentKey, Arc<PreparedSourceShape>>,
    line_segments: BTreeMap<SegmentKey, Arc<PreparedSourceShape>>,
    #[cfg(test)]
    line_boundaries: alloc::collections::BTreeSet<usize>,
    #[cfg(test)]
    work: PhysicalWork,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PhysicalWork {
    pub(crate) shaped_source: usize,
    pub(crate) installed_clusters: usize,
    pub(crate) released_segments: usize,
    pub(crate) safe_boundaries: usize,
    pub(crate) owner_visits: usize,
}

struct PhysicalSegment {
    run: usize,
    clusters: Range<usize>,
    shape: Arc<PreparedSourceShape>,
}

pub(crate) struct PhysicalShape {
    segments: Vec<PhysicalSegment>,
    end: Option<usize>,
}

impl PhysicalShape {
    /// The shape gives the glyphs of the line up to `offset`.
    pub(crate) fn covers(&self, offset: usize) -> bool {
        self.end.is_none_or(|end| offset <= end)
    }
}

/// The clusters that an installed shape replaced.
pub(crate) struct InstalledShape {
    clusters: Vec<(usize, Vec<ClusterData>)>,
    glyphs: usize,
}

/// The source ranges that need new glyphs for `points`, each from the safe boundary before a point to the safe
/// boundary after it, with the last point in each range and its glyph context: an end at a safe boundary limits the
/// context, else the end of `context` applies.
fn affected_windows(
    range: &Range<usize>,
    context: &Range<usize>,
    safe: &[usize],
    mut points: Vec<usize>,
) -> Vec<(Range<usize>, usize, Range<usize>)> {
    points.sort_unstable();
    points.dedup();
    let mut windows: Vec<(Range<usize>, usize)> = Vec::new();
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
        if let Some((previous, last)) = windows
            .last_mut()
            .filter(|(previous, _)| previous.end >= start)
        {
            previous.end = previous.end.max(end);
            *last = point;
        } else {
            windows.push((start..end, point));
        }
    }
    let safe_or = |position: usize, outer: usize| {
        if safe.binary_search(&position).is_ok() {
            position
        } else {
            outer
        }
    };
    windows
        .into_iter()
        .map(|(window, last)| {
            let context = safe_or(window.start, context.start)..safe_or(window.end, context.end);
            (window, last, context)
        })
        .collect()
}

impl<B: Brush> PhysicalShaper<B> {
    pub(crate) fn new(data: &LayoutData<B>) -> Self {
        let mut owner_edges = data
            .inline_owner_shaping
            .iter()
            .filter(|owner| !owner.edges.is_empty())
            .flat_map(|owner| [owner.text.start, owner.text.end])
            .collect::<Vec<_>>();
        owner_edges.sort_unstable();
        owner_edges.dedup();
        let alternatives = data
            .deferred_physical_shapes
            .iter()
            .map(|deferred| {
                let shape = data
                    .deferred_justification_shapes
                    .binary_search_by_key(&deferred.run_index, |candidate| {
                        candidate.source.run_index
                    })
                    .ok()?;
                let safe_concat_boundaries = data.deferred_justification_shapes[shape]
                    .prepared
                    .as_ref()
                    .map(|prepared| {
                        deferred
                            .safe_concat_boundaries
                            .iter()
                            .copied()
                            .filter(|boundary| {
                                prepared
                                    .safe_concat_boundaries
                                    .binary_search(boundary)
                                    .is_ok()
                            })
                            .collect()
                    });
                Some(Alternative {
                    shape,
                    safe_concat_boundaries,
                })
            })
            .collect::<Vec<_>>();
        Self {
            context: ShapeContext::default(),
            scratch: super::source::source_scratch(data),
            owner_edges,
            owners: OwnerIndex::new(data, &data.inline_owner_shaping),
            #[cfg(test)]
            work: PhysicalWork {
                safe_boundaries: alternatives
                    .iter()
                    .flatten()
                    .filter_map(|alternative| alternative.safe_concat_boundaries.as_ref())
                    .map(Vec::len)
                    .sum(),
                ..PhysicalWork::default()
            },
            alternatives,
            segments: BTreeMap::new(),
            line_segments: BTreeMap::new(),
            #[cfg(test)]
            line_boundaries: alloc::collections::BTreeSet::new(),
        }
    }

    pub(crate) fn release_line_segments(&mut self) {
        #[cfg(test)]
        {
            self.work.released_segments += self.line_segments.len();
            self.line_boundaries.clear();
        }
        self.line_segments.clear();
    }

    #[cfg(test)]
    pub(crate) fn line_boundaries(&self) -> alloc::collections::BTreeSet<usize> {
        self.line_boundaries.clone()
    }

    #[cfg(test)]
    pub(crate) fn work(&self) -> PhysicalWork {
        PhysicalWork {
            owner_visits: self.owners.visits(),
            ..self.work
        }
    }

    /// The physical shaping boundaries of the line with `items`.
    pub(crate) fn boundaries(&self, data: &LayoutData<B>, items: &[LineItemData]) -> Vec<usize> {
        self.owners.boundaries(data, items)
    }

    /// The segments of a line from `line_start`: each window of `affected_windows` from the line start to the next
    /// owner edge after its last point, and not after the first boundary at or after `limit`, split at the boundaries
    /// and the soft line boundaries.
    pub(crate) fn shape_line(
        &mut self,
        data: &LayoutData<B>,
        boundaries: &[usize],
        soft_boundaries: &[usize],
        line_start: usize,
        limit: Option<usize>,
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
        let end = limit.and_then(|limit| {
            boundaries[boundaries.partition_point(|boundary| *boundary < limit)..]
                .first()
                .copied()
        });
        let mut segments = Vec::new();
        let first = data
            .deferred_physical_shapes
            .partition_point(|deferred| data.runs[deferred.run_index].text_range.end <= line_start);
        for index in first..data.deferred_physical_shapes.len() {
            let deferred = &data.deferred_physical_shapes[index];
            let run = &data.runs[deferred.run_index];
            if end.is_some_and(|end| run.text_range.start >= end) {
                break;
            }
            let alternative = self.alternatives[index].as_ref().filter(|_| expanded);
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
            let safe = alternative
                .and_then(|alternative| alternative.safe_concat_boundaries.as_deref())
                .unwrap_or(&deferred.safe_concat_boundaries);
            let windows = affected_windows(&run.text_range, &deferred.context, safe, points);
            let alternative = alternative
                .map(|alternative| &data.deferred_justification_shapes[alternative.shape]);
            for (range, last, context) in windows {
                let next_edge = self.owner_edges
                    [self.owner_edges.partition_point(|edge| *edge <= last)..]
                    .first()
                    .copied()
                    .unwrap_or(range.end);
                let install = range.start.max(line_start)
                    ..range.end.min(next_edge).min(end.unwrap_or(range.end));
                if install.is_empty() {
                    continue;
                }
                let relevant = boundaries
                    .iter()
                    .copied()
                    .filter(|boundary| *boundary > context.start && *boundary < context.end)
                    .collect::<Vec<_>>();
                let mut cuts = relevant.clone();
                cuts.extend(
                    soft_boundaries
                        .iter()
                        .copied()
                        .filter(|boundary| range.contains(boundary)),
                );
                cuts.sort_unstable();
                cuts.dedup();
                let mut start = install.start;
                for end in cuts
                    .iter()
                    .copied()
                    .filter(|cut| *cut > install.start && *cut < install.end)
                    .chain(core::iter::once(install.end))
                {
                    let before = relevant.partition_point(|boundary| *boundary <= start);
                    let after = relevant.partition_point(|boundary| *boundary < end);
                    let segment_context = relevant[..before]
                        .last()
                        .copied()
                        .unwrap_or(context.start)
                        .max(context.start)
                        ..relevant
                            .get(after)
                            .copied()
                            .unwrap_or(context.end)
                            .min(context.end);
                    let line_boundaries = [start, end]
                        .into_iter()
                        .filter(|edge| {
                            range.contains(edge)
                                && soft_boundaries.contains(edge)
                                && boundaries.binary_search(edge).is_err()
                        })
                        .collect::<Vec<_>>();
                    let natural = self.segment(
                        data,
                        deferred,
                        start..end,
                        segment_context.clone(),
                        &deferred.features,
                        &line_boundaries,
                    );
                    let shape = match alternative {
                        Some(alternative) => {
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
                                self.segment(
                                    data,
                                    deferred,
                                    start..end,
                                    segment_context,
                                    &features,
                                    &line_boundaries,
                                )
                            }
                        }
                        None => natural,
                    };
                    let clusters = &data.clusters[run.cluster_range.clone()];
                    let first =
                        clusters.partition_point(|cluster| cluster.text_range(run).start < start);
                    let last =
                        clusters.partition_point(|cluster| cluster.text_range(run).start < end);
                    segments.push(PhysicalSegment {
                        run: deferred.run_index,
                        clusters: run.cluster_range.start + first..run.cluster_range.start + last,
                        shape,
                    });
                    start = end;
                }
            }
        }
        PhysicalShape { segments, end }
    }

    fn segment(
        &mut self,
        data: &LayoutData<B>,
        deferred: &DeferredSourceShape,
        range: Range<usize>,
        context: Range<usize>,
        features: &[harfrust::Feature],
        line_boundaries: &[usize],
    ) -> Arc<PreparedSourceShape> {
        let key = SegmentKey {
            run: deferred.run_index,
            source: (range.start, range.end),
            context: (context.start, context.end),
            features: deferred
                .range_features(&data.runs[deferred.run_index], &range, features)
                .into_iter()
                .map(|feature| (feature.tag, feature.value, feature.start, feature.end))
                .collect(),
        };
        if let Some(shape) = self
            .segments
            .get(&key)
            .or_else(|| self.line_segments.get(&key))
        {
            return Arc::clone(shape);
        }
        #[cfg(test)]
        {
            self.work.shaped_source += range.len();
        }
        let shape = Arc::new(
            deferred.shape_source_range(
                &mut self.context,
                &mut self.scratch,
                data,
                data.shaping_source_text
                    .as_deref()
                    .expect("retained physical shaping source"),
                range,
                context,
                features,
            ),
        );
        let store = if line_boundaries.is_empty() {
            &mut self.segments
        } else {
            #[cfg(test)]
            self.line_boundaries.extend(line_boundaries);
            &mut self.line_segments
        };
        store.insert(key, Arc::clone(&shape));
        shape
    }

    /// Keeps the clusters that `shape` replaces, then installs it.
    pub(crate) fn install(
        &mut self,
        data: &mut LayoutData<B>,
        shape: &PhysicalShape,
    ) -> InstalledShape {
        let installed = InstalledShape {
            clusters: shape
                .segments
                .iter()
                .map(|segment| {
                    (
                        segment.clusters.start,
                        data.clusters[segment.clusters.clone()].to_vec(),
                    )
                })
                .collect(),
            glyphs: data.glyphs.len(),
        };
        self.keep(data, shape);
        installed
    }

    /// Replaces the clusters of each segment of `shape` and appends its glyphs.
    pub(crate) fn keep(&mut self, data: &mut LayoutData<B>, shape: &PhysicalShape) {
        for segment in &shape.segments {
            let run = &data.runs[segment.run];
            let target = &mut data.clusters[segment.clusters.clone()];
            segment.shape.install_for_run(run, target, &mut data.glyphs);
            crate::layout::project_source_fit(run, target, &data.source_cluster_fit_advances);
            #[cfg(test)]
            {
                self.work.installed_clusters += target.len();
            }
        }
    }

    /// Gives back the clusters that `installed` replaced and removes its glyphs.
    pub(crate) fn remove(&mut self, data: &mut LayoutData<B>, installed: InstalledShape) {
        for (start, clusters) in installed.clusters.into_iter().rev() {
            #[cfg(test)]
            {
                self.work.installed_clusters += clusters.len();
            }
            data.clusters[start..start + clusters.len()].copy_from_slice(&clusters);
        }
        data.glyphs.truncate(installed.glyphs);
    }
}
