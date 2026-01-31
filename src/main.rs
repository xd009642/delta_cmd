use clap::Parser;
use radix_trie::TrieCommon;
use std::collections::BTreeSet;
use std::env;
use std::path::PathBuf;

mod cargo;
mod repository;

#[derive(Debug, Parser)]
pub struct Args {
    /// Get the project to run on, runs in current directory otherwise.
    #[clap(short, long)]
    input: Option<PathBuf>,
}

impl Args {
    pub fn path(&self) -> PathBuf {
        match self.input.as_ref() {
            Some(s) => s.clone(),
            None => env::current_dir().unwrap(),
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    // Open the repository (current directory)

    let root = args.path();

    let considered_files = repository::get_changed_source_files(&root)?;

    // Now from these files we want to create a list of projects in the workspace we should run
    // tests on. This is done via two easy checks:
    //
    // 1. If a project has a file that changed. Rerun it
    // 2. If a project has a dependency in the workspace that changed. Run it.
    //
    // We can skip dependency tree creation if 1. covers all projects. Once we get all the ones in
    // 1. we can also do some early exiting of the dependency tree resolution to save a bit of
    // effort!

    let packages = cargo::find_packages(&root)?;

    let mut changed_packages = BTreeSet::new();

    let mut end_package_names = BTreeSet::new();

    for file in &considered_files {
        if let Some(package) = packages.get_ancestor_value(&root.join(file)) {
            changed_packages.insert(root.join(file));
            end_package_names.insert(package.name.as_str());
        }
    }

    let mut changed_packages_previous = 0;

    while changed_packages_previous != changed_packages.len() {
        changed_packages_previous = changed_packages.len();

        for (key, val) in packages.iter() {
            if val
                .dependencies
                .iter()
                .any(|x| changed_packages.contains(x))
            {
                if let Some(package) = packages.get_ancestor_value(&root.join(key)) {
                    changed_packages.insert(root.join(key));
                    end_package_names.insert(package.name.as_str());
                }
            }
        }
    }

    if !changed_packages.is_empty() {
        println!(
            "Changed packages end: `-p {}`",
            end_package_names
                .iter()
                .map(|x| *x)
                .collect::<Vec<_>>()
                .join(" -p ")
        );
    } else {
        println!("No packages have changed");
    }

    Ok(())
}
