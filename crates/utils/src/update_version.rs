use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use toml_edit::{DocumentMut, Formatted, value};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    hotfix_version: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let crates = get_crate_dirs()?;
    update_workspace(&args.hotfix_version)?;

    for c in crates {
        update_crate(c, &args.hotfix_version)?;
    }

    Ok(())
}

fn update_workspace(version: &str) -> Result<()> {
    let path = PathBuf::from("Cargo.toml");
    let mut doc = parse_cargo_toml(path.as_path())?;
    doc["workspace"]["package"]["version"] = value(version);

    fs::write(path, doc.to_string())?;
    Ok(())
}

fn update_crate(crate_path: PathBuf, version: &str) -> Result<()> {
    let path = crate_path.join("Cargo.toml");
    let mut doc = parse_cargo_toml(path.as_path())?;

    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(deps) = doc.get_mut(section) {
            let table = deps.as_table_mut().unwrap();
            for (name, dep) in table.iter_mut() {
                if name.starts_with("hotfix") {
                    println!("updating {name} in {path:?} to {version}");
                    if let Some(dep_table) = dep.as_inline_table_mut() {
                        if let Some(v) = dep_table.get_mut("version") {
                            *v = toml_edit::Value::String(Formatted::new(version.to_string()));
                        }
                    } else {
                        *dep = value(version);
                    }
                }
            }
        }
    }

    fs::write(path, doc.to_string())?;
    Ok(())
}

fn parse_cargo_toml(cargo_toml_path: &Path) -> Result<DocumentMut> {
    let contents = fs::read_to_string(cargo_toml_path)?;
    Ok(contents.parse()?)
}

fn get_crate_dirs() -> Result<Vec<PathBuf>> {
    let crate_dir = "./crates";
    let mut crates = Vec::new();
    for entry in fs::read_dir(crate_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Check if it's a directory and the name starts with "hotfix"
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("hotfix") {
                    crates.push(path);
                }
            }
        }
    }

    Ok(crates)
}
