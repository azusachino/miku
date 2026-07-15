use grep::{
    regex::RegexMatcherBuilder,
    searcher::{sinks::UTF8, SearcherBuilder},
};
use ignore::WalkBuilder;
use serde::Serialize;
use std::path::Path;

const MAX_MATCHES_PER_FILE: usize = 5;

#[derive(Debug, Serialize)]
pub struct ContentMatch {
    pub line: u64,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct ContentFile {
    pub path: String,
    pub title: String,
    pub matches: Vec<ContentMatch>,
}

#[derive(Debug, Serialize)]
pub struct ContentSearchPage {
    pub files: Vec<ContentFile>,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
    pub next_offset: usize,
}

pub fn search(
    root: &Path,
    query: &str,
    offset: usize,
    requested_limit: usize,
    regex: bool,
) -> Result<ContentSearchPage, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(ContentSearchPage {
            files: Vec::new(),
            offset: 0,
            limit: requested_limit.clamp(1, 50),
            has_more: false,
            next_offset: 0,
        });
    }

    let mut matcher_builder = RegexMatcherBuilder::new();
    matcher_builder.case_smart(true).multi_line(false);
    if !regex {
        matcher_builder.fixed_strings(true);
    }
    let pattern_matcher = matcher_builder
        .build(query)
        .map_err(|error| format!("invalid search pattern: {error}"))?;
    let limit = requested_limit.clamp(1, 50);
    let mut searcher = SearcherBuilder::new().line_number(true).build();
    let mut matched_files = Vec::new();
    let mut matching_file_count = 0usize;
    let mut has_more = false;

    for entry in WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .ignore(false)
        .parents(false)
        .build()
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !is_markdown_file(root, path)
            || path.components().any(|part| part.as_os_str() == ".trash")
        {
            continue;
        }

        let mut file_matches = Vec::new();
        let sink = UTF8(|line, text: &str| {
            if file_matches.len() >= MAX_MATCHES_PER_FILE {
                return Ok(false);
            }
            file_matches.push(ContentMatch {
                line,
                text: text.trim_end_matches(['\r', '\n']).to_string(),
            });
            Ok(true)
        });
        searcher
            .search_path(&pattern_matcher, path, sink)
            .map_err(|error| error.to_string())?;
        if file_matches.is_empty() {
            continue;
        }

        if matching_file_count >= offset && matched_files.len() < limit {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            let display_path = relative
                .strip_suffix(".md")
                .unwrap_or(&relative)
                .to_string();
            let title = Path::new(&display_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&display_path)
                .to_string();
            matched_files.push(ContentFile {
                path: display_path,
                title,
                matches: file_matches,
            });
        }
        matching_file_count += 1;
        if matched_files.len() >= limit && matching_file_count > offset + limit {
            has_more = true;
            break;
        }
    }

    let next_offset = offset + matched_files.len();
    Ok(ContentSearchPage {
        files: matched_files,
        offset,
        limit,
        has_more,
        next_offset,
    })
}

fn is_markdown_file(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
        && path.is_file()
        && path.extension().and_then(|extension| extension.to_str()) == Some("md")
}

#[cfg(test)]
mod tests {
    use super::search;
    use std::fs;

    #[test]
    fn searches_markdown_files_and_returns_extensionless_paths() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::write(root.path().join("Today.md"), "# Today\nJVM notes\n").expect("write");
        fs::write(root.path().join("ignored.txt"), "JVM notes\n").expect("write");
        let result = search(root.path(), "jvm", 0, 10, false).expect("search");
        assert_eq!(result.files[0].path, "Today");
        assert_eq!(result.files[0].matches[0].line, 2);
    }

    #[test]
    fn rejects_invalid_regex() {
        let root = tempfile::tempdir().expect("tempdir");
        let error = search(root.path(), "[", 0, 10, true).expect_err("invalid regex");
        assert!(error.contains("invalid search pattern"));
    }
}
