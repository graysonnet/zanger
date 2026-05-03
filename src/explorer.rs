use ignore::WalkBuilder;
use rayon::prelude::*;
use bstr::ByteSlice;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct FileExplorer {
    pub all_items: Vec<FileItem>,
    pub visible_items: Vec<FileItem>,
    pub collapsed_dirs: HashSet<PathBuf>,
}

impl FileExplorer {
    pub fn new() -> Self {
        Self {
            all_items: Vec::new(),
            visible_items: Vec::new(),
            collapsed_dirs: HashSet::new(),
        }
    }

    pub fn refresh(&mut self) {
        self.all_items.clear();
        // Use "." instead of "./" for better OS compatibility
        let walker = WalkBuilder::new(".").hidden(false).build();

        for result in walker.filter_map(Result::ok) {
            let is_dir = result.file_type().map_or(false, |ft| ft.is_dir());
            let path = result.into_path();

            // OS-agnostic way to skip the root directory "." itself
            if path.components().count() <= 1 && is_dir {
                continue;
            }

            if is_dir {
                self.collapsed_dirs.insert(path.clone());
            }

            self.all_items.push(FileItem { path, is_dir });
        }
        self.all_items.sort_by(|a, b| a.path.cmp(&b.path));
    }

    pub fn update_visible(&mut self, file_query: &str, content_query: &str) {
        self.visible_items.clear();
        let is_search = !file_query.is_empty() || !content_query.is_empty();

        if is_search {
            let file_query_lower = file_query.to_lowercase();
            let content_query_lower = content_query.to_lowercase();
            let content_query_bytes = content_query_lower.as_bytes();

            let matched: Vec<FileItem> = self.all_items.par_iter().filter(|item| {
                if item.is_dir {
                    return false;
                }

                let path_str = item.path.display().to_string().to_lowercase();

                // 1. File constraint
                if !file_query.is_empty() && !path_str.contains(&file_query_lower) {
                    return false;
                }

                // 2. Content constraint
                if !content_query.is_empty() {
                    // Check file isn't massive
                    if let Ok(metadata) = std::fs::metadata(&item.path) {
                        if metadata.len() > 10 * 1024 * 1024 {
                            return false;
                        }
                    } else {
                        return false;
                    }

                    if let Ok(content) = fs::read(&item.path) {
                        // Priority 1: match raw exact
                        if content.find(content_query_bytes).is_none() {
                            // Priority 2: match ascii lowercase
                            if content.to_ascii_lowercase().find(content_query_bytes).is_none() {
                                return false; // Doesn't match content constraint
                            }
                        }
                    } else {
                        return false; // Can't read, fail content match
                    }
                }

                true // Survived all filters!
            }).cloned().collect();

            self.visible_items.extend(matched);

        } else {
            let mut skip_prefix: Option<PathBuf> = None;

            for item in &self.all_items {
                if let Some(ref prefix) = skip_prefix {
                    if item.path.starts_with(prefix) {
                        continue;
                    } else {
                        skip_prefix = None;
                    }
                }

                self.visible_items.push(item.clone());
                if item.is_dir && self.collapsed_dirs.contains(&item.path) {
                    skip_prefix = Some(item.path.clone());
                }
            }
        }
    }

    pub fn toggle_dir(&mut self, path: &Path) {
        if self.collapsed_dirs.contains(path) {
            self.collapsed_dirs.remove(path);
        } else {
            self.collapsed_dirs.insert(path.to_path_buf());
        }
    }

    pub fn toggle_all_dirs(&mut self) {
        if self.collapsed_dirs.is_empty() {
            // Collapse all
            for item in &self.all_items {
                if item.is_dir {
                    self.collapsed_dirs.insert(item.path.clone());
                }
            }
        } else {
            // Expand all
            self.collapsed_dirs.clear();
        }
    }
}
