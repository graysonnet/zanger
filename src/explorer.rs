use ignore::WalkBuilder;
use std::path::PathBuf;

pub struct FileExplorer {
    pub files: Vec<PathBuf>,
}

impl FileExplorer {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn refresh(&mut self, search_query: &str) {
        self.files.clear();

        // Using ignore crate to walk directory tree avoiding .git and .gitignore files
        let walker = WalkBuilder::new("./").hidden(false).build();

        for result in walker.filter_map(Result::ok) {
            if result.file_type().map_or(false, |ft| ft.is_file()) {
                let path = result.into_path();
                let path_str = path.display().to_string();

                if search_query.is_empty() || path_str.to_lowercase().contains(&search_query.to_lowercase()) {
                    self.files.push(path);
                }
            }
        }

        self.files.sort();
    }
}
