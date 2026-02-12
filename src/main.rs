use anyhow::Context;
use cargo::Package;
use clap::Parser;
use minijinja::{Environment, Value};
use radix_trie::{Trie, TrieCommon};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

mod cargo;
mod repository;

const CARGO_TEST_TEMPLATE: &'static str = "cargo test {% for pkg in packages %} -p {{ pkg }} {% endfor %} {% for arg in args %} {{ arg }} {% endfor %}";
const CARGO_NEXTEST_TEMPLATE: &'static str = "cargo nextest {% for pkg in packages %} -p {{ pkg }} {% endfor %} {% for arg in args %} {{ arg }} {% endfor %}";
const CARGO_BUILD_TEMPLATE: &'static str = "cargo build {% for pkg in packages %} -p {{ pkg }} {% endfor %} {% for arg in args %} {{ arg }} {% endfor %}";
const CARGO_BENCH_TEMPLATE: &'static str = "cargo build {% for pkg in packages %} -p {{ pkg }} {% endfor %} {% for arg in args %} {{ arg }} {% endfor %}";

#[derive(Debug, Parser)]
pub enum RunCommand {
    Test(RequiredArgs),
    Nextest(RequiredArgs),
    Build(RequiredArgs),
    Bench(RequiredArgs),
    Run(Args),
}

impl RunCommand {
    pub fn required_args(&self) -> &RequiredArgs {
        match self {
            Self::Test(a) | Self::Nextest(a) | Self::Build(a) | Self::Bench(a) => a,
            Self::Run(a) => &a.required,
        }
    }

    pub fn command(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Test(_) => Some(CARGO_TEST_TEMPLATE.into()),
            Self::Nextest(_) => Some(CARGO_NEXTEST_TEMPLATE.into()),
            Self::Build(_) => Some(CARGO_BUILD_TEMPLATE.into()),
            Self::Bench(_) => Some(CARGO_BENCH_TEMPLATE.into()),
            Self::Run(a) => a.command.as_ref().map(|x| x.into()),
        }
    }
}

#[derive(Debug, Parser)]
pub struct RequiredArgs {
    /// Get the project to run on, runs in current directory otherwise.
    #[arg(short, long)]
    input: Option<PathBuf>,
    /// Generate command but don't run it
    #[arg(long)]
    no_run: bool,
    /// These will be passed to the minijinja template as the args variable
    #[arg(last = true)]
    args: Vec<String>,
}

impl RequiredArgs {
    fn path(&self) -> PathBuf {
        match self.input.as_ref() {
            Some(s) => s.clone(),
            None => env::current_dir().unwrap(),
        }
    }
}

#[derive(Debug, Parser)]
pub struct Args {
    /// Run the following command. This accepts a minijinja template where `packages` is a list of
    /// packages that can be included and `excludes` is a list of packages that can be excluded.
    /// For a cargo test you can write the template `cargo test {% for pkg in packages %} -p {{ pkg
    /// }}{% endfor %}`
    #[arg(short, long)]
    command: Option<String>,
    #[command(flatten)]
    required: RequiredArgs,
}

fn generate_exclude_list<'a>(
    packages: impl Iterator<Item = &'a Package>,
    included_packages: &BTreeSet<&str>,
) -> BTreeSet<&'a str> {
    packages
        .filter(|x| !included_packages.contains(x.name.as_str()))
        .map(|x| x.name.as_str())
        .collect::<BTreeSet<_>>()
}

fn generate_command(
    template: &str,
    packages: &Trie<PathBuf, Package>,
    included_packages: &BTreeSet<&str>,
    args: &[String],
) -> anyhow::Result<Command> {
    let mut env = Environment::new();
    env.add_template("cmd", template)?;
    let expr = env.get_template("cmd")?;

    let variable_names = expr.undeclared_variables(true);
    let mut variables = HashMap::new();
    for var in variable_names.iter() {
        match var.as_str() {
            "packages" => {
                variables.insert("packages", Value::from_serialize(included_packages));
            }
            "excludes" => {
                variables.insert(
                    "excludes",
                    Value::from_serialize(generate_exclude_list(
                        packages.values(),
                        included_packages,
                    )),
                );
            }
            "args" => {
                variables.insert("args", Value::from_serialize(args));
            }
            s => anyhow::bail!("Unsupported variable `{}`", s),
        }
    }
    let result = expr.render(&variables)?;

    let parts = shell_words::split(result.as_str())?;
    let mut part_iter = parts.into_iter();
    let exe = part_iter.next().context("No program name")?;
    let mut cmd = Command::new(exe);

    cmd.args(part_iter)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    Ok(cmd)
}

fn main() -> anyhow::Result<()> {
    let args = RunCommand::parse();

    let root = args.required_args().path();

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

    //let exclude = generate_exclude_list(packages.values(), &end_package_names);

    if let Some(cmd) = args.command() {
        let mut cmd = generate_command(
            &cmd,
            &packages,
            &end_package_names,
            &args.required_args().args,
        )?;
        if args.required_args().no_run {
            println!("{:?}", cmd);
        } else {
            cmd.status()?;
        }
    } else if !changed_packages.is_empty() {
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
