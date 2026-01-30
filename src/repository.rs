use std::path::{Path, PathBuf};

use git2::{DiffOptions, Repository};

pub fn is_considered(path: &Path) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => return false,
    };
    matches!(
        ext.as_str(),
        "rs" | "c" | "cpp" | "h" | "hpp" | "cc" | "cxx" | "toml"
    )
}

pub fn get_changed_source_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let repo = Repository::open(root)?;

    // Get HEAD commit
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    // Last commit should have at least one parent
    let parent = commit.parent(0)?;

    // Get trees
    let commit_tree = commit.tree()?;
    let parent_tree = parent.tree()?;

    let mut diff_opt = DiffOptions::new();

    // Diff parent -> commit
    let diff =
        repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), Some(&mut diff_opt))?;

    println!("Files changed in last commit:");

    let mut considered_files = vec![];
    diff.foreach(
        &mut |delta, _| {
            if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                if is_considered(path) {
                    considered_files.push(path.to_path_buf());
                }
            }
            true
        },
        None,
        None,
        None,
    )?;

    Ok(considered_files)
}
