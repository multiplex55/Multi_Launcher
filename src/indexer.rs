use crate::actions::Action;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use walkdir::{IntoIter as WalkDirIter, WalkDir};

const DEFAULT_BATCH_SIZE: usize = 512;
const DEFAULT_MAX_ITEMS: usize = 100_000;

#[derive(Debug, Clone, Copy)]
pub struct IndexOptions {
    pub batch_size: usize,
    pub max_items: usize,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            max_items: DEFAULT_MAX_ITEMS,
        }
    }
}

impl IndexOptions {
    pub fn with_max_items(max_items: Option<usize>) -> Self {
        Self {
            max_items: max_items.unwrap_or(DEFAULT_MAX_ITEMS),
            ..Self::default()
        }
    }
}

/// Lazily indexes files from one or more roots and yields actions in batches.
///
/// Duplicate files are skipped by canonical path. Traversal errors stop
/// iteration and are returned to the caller.
pub struct IndexBatchIter {
    roots: Vec<String>,
    root_idx: usize,
    current: Option<WalkDirIter>,
    seen: HashSet<PathBuf>,
    options: IndexOptions,
    produced: usize,
}

impl IndexBatchIter {
    fn new(paths: &[String], options: IndexOptions) -> Self {
        let options = IndexOptions {
            batch_size: options.batch_size.max(1),
            max_items: options.max_items.max(1),
        };
        Self {
            roots: paths.to_vec(),
            root_idx: 0,
            current: None,
            seen: HashSet::new(),
            options,
            produced: 0,
        }
    }

    fn next_root(&mut self) -> Option<String> {
        let root = self.roots.get(self.root_idx).cloned();
        if root.is_some() {
            self.root_idx += 1;
        }
        root
    }
}

impl Iterator for IndexBatchIter {
    type Item = anyhow::Result<Vec<Action>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.produced >= self.options.max_items {
            return None;
        }

        let mut batch = Vec::with_capacity(self.options.batch_size);
        while self.produced < self.options.max_items && batch.len() < self.options.batch_size {
            if self.current.is_none() {
                if let Some(root) = self.next_root() {
                    self.current = Some(WalkDir::new(root).into_iter());
                } else {
                    break;
                }
            }

            let Some(iter) = self.current.as_mut() else {
                continue;
            };

            match iter.next() {
                Some(Ok(entry)) => {
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    let canonical = match fs::canonicalize(entry.path()) {
                        Ok(path) => path,
                        Err(err) => {
                            tracing::error!(
                                path = %entry.path().display(),
                                error = %err,
                                "failed to canonicalize indexed path"
                            );
                            return Some(Err(err.into()));
                        }
                    };
                    if !self.seen.insert(canonical.clone()) {
                        continue;
                    }
                    let Some(name) = canonical.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    let display = canonical.display().to_string();
                    batch.push(Action {
                        label: name.to_string(),
                        desc: display.clone(),
                        action: display,
                        args: None,
                    });
                    self.produced += 1;
                }
                Some(Err(err)) => {
                    tracing::error!(error = %err, "failed to read directory entry");
                    return Some(Err(err.into()));
                }
                None => {
                    self.current = None;
                }
            }
        }

        if batch.is_empty() {
            None
        } else {
            Some(Ok(batch))
        }
    }
}

pub fn index_paths_batched(paths: &[String], options: IndexOptions) -> IndexBatchIter {
    IndexBatchIter::new(paths, options)
}

/// Index the provided filesystem paths and return a list of [`Action`]s.
///
/// This compatibility helper exhausts the batched iterator into a single
/// vector; prefer [`index_paths_batched`] when possible.
pub fn index_paths(paths: &[String]) -> anyhow::Result<Vec<Action>> {
    let mut results = Vec::new();
    for batch in index_paths_batched(paths, IndexOptions::default()) {
        results.extend(batch?);
    }
    Ok(results)
}
