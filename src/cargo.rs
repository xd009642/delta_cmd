use cargo_metadata::MetadataCommand;
use radix_trie::Trie;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Package {
    pub name: String,
    pub manifest: PathBuf,
    pub dependencies: Vec<PathBuf>,
}

fn check_path(root: &Path, path: Option<&Path>) -> bool {
    match path {
        Some(p) => p.starts_with(root),
        None => false,
    }
}

pub fn find_packages(root: &Path) -> anyhow::Result<Trie<PathBuf, Package>> {
    let metadata = MetadataCommand::new().current_dir(root).exec()?;

    let mut packages = Trie::new();

    for package in &metadata.workspace_members {
        let package = &metadata[package];

        let dependencies = package
            .dependencies
            .iter()
            .filter(|x| check_path(root, x.path.as_ref().map(|x| x.as_std_path())))
            .map(|x| x.path.clone().unwrap().into_std_path_buf())
            .collect();

        let pack = Package {
            name: package.name.to_string(),
            manifest: package.manifest_path.clone().into_std_path_buf(),
            dependencies,
        };
        packages.insert(
            package
                .manifest_path
                .parent()
                .unwrap()
                .as_std_path()
                .to_path_buf(),
            pack,
        );
    }

    Ok(packages)
}
