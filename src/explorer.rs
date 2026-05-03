use ignore::WalkBuilder;
use std::collections::HashSet;
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

            self.all_items.push(FileItem { path, is_dir });
        }
        self.all_items.sort_by(|a, b| a.path.cmp(&b.path));
    }

    pub fn update_visible(&mut self, search_query: &str) {
        self.visible_items.clear();
        let is_search = !search_query.is_empty();
        let query_lower = search_query.to_lowercase();

        let mut skip_prefix: Option<PathBuf> = None;

        for item in &self.all_items {
            let path_str = item.path.display().to_string();

            if is_search {
                if path_str.to_lowercase().contains(&query_lower) && !item.is_dir {
                    self.visible_items.push(item.clone());
                }
            } else {
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
}
