//! File system tools (read, write, list, patch).
//!
//! Provides the agent with safe file system access.
//! Read and list are auto-approved; write and patch require approval.

use anyhow::Result;
use async_trait::async_trait;

use super::{Tool, ToolCategory, ToolContext, ToolOutput};

/// Maximum file size we'll read (10MB)
const MAX_READ_BYTES: u64 = 10 * 1024 * 1024;

/// Maximum number of directory entries to list
const MAX_LIST_ENTRIES: usize = 500;

// ============================================================================
// ReadFileTool
// ============================================================================

pub struct ReadFileTool;

impl ReadFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Returns the file content as text. \
         For binary files, returns a description of the file type and size."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (absolute or relative to working directory)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-indexed, default: 1)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (default: all)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path_str = match params["path"].as_str() {
            Some(p) => p,
            None => return Ok(ToolOutput::Error("Missing 'path' parameter".to_string())),
        };

        let path = resolve_path(path_str, &ctx.working_directory);

        // Check file exists and size
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) => {
                return Ok(ToolOutput::Error(format!(
                    "Cannot access '{}': {}",
                    path_str, e
                )))
            }
        };

        if metadata.is_dir() {
            return Ok(ToolOutput::Error(format!(
                "'{}' is a directory, use list_directory instead",
                path_str
            )));
        }

        if metadata.len() > MAX_READ_BYTES {
            return Ok(ToolOutput::Error(format!(
                "File too large ({} bytes, max {}). Use offset/limit to read portions.",
                metadata.len(),
                MAX_READ_BYTES
            )));
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(_) => {
                // Probably binary — report size and type
                return Ok(ToolOutput::Text(format!(
                    "[Binary file: {} bytes]",
                    metadata.len()
                )));
            }
        };

        // Apply offset/limit
        let offset = params["offset"].as_u64().unwrap_or(1).max(1) as usize;
        let limit = params["limit"].as_u64().map(|l| l as usize);

        let lines: Vec<&str> = content.lines().collect();
        let start = (offset - 1).min(lines.len());
        let end = match limit {
            Some(l) => (start + l).min(lines.len()),
            None => lines.len(),
        };

        let selected: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>4} | {}", start + i + 1, line))
            .collect();

        let mut result = selected.join("\n");
        if end < lines.len() {
            result.push_str(&format!(
                "\n\n[Showing lines {}-{} of {}]",
                start + 1,
                end,
                lines.len()
            ));
        }

        Ok(ToolOutput::Text(result))
    }

    fn requires_approval(&self) -> bool {
        false // Reading is safe
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
}

// ============================================================================
// WriteFileTool
// ============================================================================

pub struct WriteFileTool;

impl WriteFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, \
         or overwrites it if it does. Creates parent directories as needed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (absolute or relative to working directory)"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path_str = match params["path"].as_str() {
            Some(p) => p,
            None => return Ok(ToolOutput::Error("Missing 'path' parameter".to_string())),
        };

        let content = match params["content"].as_str() {
            Some(c) => c,
            None => return Ok(ToolOutput::Error("Missing 'content' parameter".to_string())),
        };

        let path = resolve_path(path_str, &ctx.working_directory);

        // Create parent directories
        if let Some(parent) = std::path::Path::new(&path).parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(ToolOutput::Error(format!(
                    "Failed to create directories: {}",
                    e
                )));
            }
        }

        match tokio::fs::write(&path, content).await {
            Ok(()) => {
                tracing::info!("WriteFileTool: wrote {} bytes to {}", content.len(), path);
                Ok(ToolOutput::Text(format!(
                    "Wrote {} bytes to {}",
                    content.len(),
                    path_str
                )))
            }
            Err(e) => Ok(ToolOutput::Error(format!(
                "Failed to write '{}': {}",
                path_str, e
            ))),
        }
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
}

// ============================================================================
// ListDirectoryTool
// ============================================================================

pub struct ListDirectoryTool;

impl ListDirectoryTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files and directories in a given path. Shows names, sizes, and types."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path (defaults to working directory)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List recursively (default: false, max depth: 3)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path_str = params["path"].as_str().unwrap_or(&ctx.working_directory);
        let recursive = params["recursive"].as_bool().unwrap_or(false);

        let path = resolve_path(path_str, &ctx.working_directory);

        let metadata = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) => {
                return Ok(ToolOutput::Error(format!(
                    "Cannot access '{}': {}",
                    path_str, e
                )))
            }
        };

        if !metadata.is_dir() {
            return Ok(ToolOutput::Error(format!(
                "'{}' is not a directory",
                path_str
            )));
        }

        let max_depth = if recursive { 3 } else { 1 };
        let mut entries = Vec::new();
        list_dir_recursive(&path, &path, max_depth, 0, &mut entries).await;

        if entries.len() > MAX_LIST_ENTRIES {
            entries.truncate(MAX_LIST_ENTRIES);
            entries.push(format!("[Truncated at {} entries]", MAX_LIST_ENTRIES));
        }

        Ok(ToolOutput::Text(entries.join("\n")))
    }

    fn requires_approval(&self) -> bool {
        false // Listing is safe
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
}

// ============================================================================
// PatchFileTool
// ============================================================================

pub struct PatchFileTool;

impl PatchFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for PatchFileTool {
    fn name(&self) -> &str {
        "patch_file"
    }

    fn description(&self) -> &str {
        "Apply a targeted edit to a file by replacing a specific string with new content. \
         The old_string must match exactly (including whitespace). \
         Fails if old_string is not found or matches multiple locations."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact string to find and replace (must be unique in the file)"
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement string"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path_str = match params["path"].as_str() {
            Some(p) => p,
            None => return Ok(ToolOutput::Error("Missing 'path' parameter".to_string())),
        };
        let old_string = match params["old_string"].as_str() {
            Some(s) => s,
            None => {
                return Ok(ToolOutput::Error(
                    "Missing 'old_string' parameter".to_string(),
                ))
            }
        };
        let new_string = match params["new_string"].as_str() {
            Some(s) => s,
            None => {
                return Ok(ToolOutput::Error(
                    "Missing 'new_string' parameter".to_string(),
                ))
            }
        };

        let path = resolve_path(path_str, &ctx.working_directory);

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolOutput::Error(format!(
                    "Cannot read '{}': {}",
                    path_str, e
                )))
            }
        };

        let match_count = content.matches(old_string).count();

        if match_count == 0 {
            return Ok(ToolOutput::Error(format!(
                "old_string not found in '{}'. Make sure it matches exactly (including whitespace).",
                path_str
            )));
        }

        if match_count > 1 {
            return Ok(ToolOutput::Error(format!(
                "old_string matches {} locations in '{}'. Provide more context to make it unique.",
                match_count, path_str
            )));
        }

        let new_content = content.replacen(old_string, new_string, 1);

        match tokio::fs::write(&path, &new_content).await {
            Ok(()) => {
                tracing::info!("PatchFileTool: patched {}", path);
                Ok(ToolOutput::Text(format!("Patched '{}'", path_str)))
            }
            Err(e) => Ok(ToolOutput::Error(format!(
                "Failed to write '{}': {}",
                path_str, e
            ))),
        }
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Resolve a path, making it absolute relative to the working directory if needed.
fn resolve_path(path: &str, working_dir: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        path.to_string()
    } else {
        std::path::Path::new(working_dir)
            .join(path)
            .to_string_lossy()
            .to_string()
    }
}

async fn list_dir_recursive(
    base: &str,
    current: &str,
    max_depth: usize,
    depth: usize,
    entries: &mut Vec<String>,
) {
    if depth >= max_depth || entries.len() >= MAX_LIST_ENTRIES {
        return;
    }

    let mut read_dir = match tokio::fs::read_dir(current).await {
        Ok(rd) => rd,
        Err(e) => {
            entries.push(format!("  [error reading {}: {}]", current, e));
            return;
        }
    };

    let mut items = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        items.push(entry);
    }

    // Sort by name
    items.sort_by_key(|e| e.file_name());

    for entry in items {
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files at depth 0 unless specifically requested
        if name.starts_with('.') && depth == 0 {
            continue;
        }

        let indent = "  ".repeat(depth);
        let rel_path = entry.path();
        let rel = rel_path
            .strip_prefix(base)
            .unwrap_or(&rel_path)
            .to_string_lossy();

        if let Ok(meta) = entry.metadata().await {
            if meta.is_dir() {
                entries.push(format!("{}{}/", indent, rel));
                // Recurse into subdirectories (box the future to avoid deep recursion)
                Box::pin(list_dir_recursive(
                    base,
                    &entry.path().to_string_lossy(),
                    max_depth,
                    depth + 1,
                    entries,
                ))
                .await;
            } else {
                let size = format_size(meta.len());
                entries.push(format!("{}{}  ({})", indent, rel, size));
            }
        } else {
            entries.push(format!("{}{}", indent, rel));
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn test_ctx() -> ToolContext {
        ToolContext {
            working_directory: "/tmp".to_string(),
            username: "test".to_string(),
            autonomous: false,
        }
    }

    #[tokio::test]
    async fn test_read_file() {
        // Create a temp file
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "line 1\nline 2\nline 3\n").unwrap();

        let tool = ReadFileTool::new();
        let params = serde_json::json!({"path": file_path.to_string_lossy()});
        let result = tool.execute(params, &test_ctx()).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("line 1"));
                assert!(text.contains("line 2"));
                assert!(text.contains("line 3"));
            }
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_read_file_with_offset_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "a\nb\nc\nd\ne\n").unwrap();

        let tool = ReadFileTool::new();
        let params =
            serde_json::json!({"path": file_path.to_string_lossy(), "offset": 2, "limit": 2});
        let result = tool.execute(params, &test_ctx()).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("b"));
                assert!(text.contains("c"));
                assert!(!text.contains("   1 |")); // shouldn't show line 1
            }
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_write_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("output.txt");

        let tool = WriteFileTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": "hello world"
        });

        let result = tool.execute(params, &test_ctx()).await.unwrap();
        assert!(result.is_success());

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_write_file_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("sub/dir/output.txt");

        let tool = WriteFileTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": "nested"
        });

        let result = tool.execute(params, &test_ctx()).await.unwrap();
        assert!(result.is_success());
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_list_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let tool = ListDirectoryTool::new();
        let params = serde_json::json!({"path": dir.path().to_string_lossy()});
        let result = tool.execute(params, &test_ctx()).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("a.txt"));
                assert!(text.contains("b.rs"));
                assert!(text.contains("subdir/"));
            }
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_patch_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("code.rs");
        std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let tool = PatchFileTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "old_string": "println!(\"hello\")",
            "new_string": "println!(\"goodbye\")"
        });

        let result = tool.execute(params, &test_ctx()).await.unwrap();
        assert!(result.is_success());

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("goodbye"));
        assert!(!content.contains("hello"));
    }

    #[tokio::test]
    async fn test_patch_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("code.rs");
        std::fs::write(&file_path, "some content").unwrap();

        let tool = PatchFileTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "old_string": "nonexistent text",
            "new_string": "replacement"
        });

        let result = tool.execute(params, &test_ctx()).await.unwrap();
        assert!(matches!(result, ToolOutput::Error(_)));
    }

    #[tokio::test]
    async fn test_patch_file_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("code.rs");
        std::fs::write(&file_path, "foo bar foo").unwrap();

        let tool = PatchFileTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "old_string": "foo",
            "new_string": "baz"
        });

        let result = tool.execute(params, &test_ctx()).await.unwrap();
        assert!(matches!(result, ToolOutput::Error(_)));
    }

    #[test]
    fn test_resolve_path_absolute() {
        assert_eq!(resolve_path("/usr/bin/ls", "/tmp"), "/usr/bin/ls");
    }

    #[test]
    fn test_resolve_path_relative() {
        assert_eq!(
            resolve_path("src/main.rs", "/home/user/project"),
            "/home/user/project/src/main.rs"
        );
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(1_500_000), "1.4 MB");
    }

    #[test]
    fn test_approval_requirements() {
        assert!(!ReadFileTool::new().requires_approval());
        assert!(!ListDirectoryTool::new().requires_approval());
        assert!(WriteFileTool::new().requires_approval());
        assert!(PatchFileTool::new().requires_approval());
    }
}
