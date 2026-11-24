//! Discovering the member crates of a Cargo workspace.
//!
//! Everything here is deterministic: the facts come from `cargo metadata`, not from a model.

use std::{
    collections::HashSet,
    fmt,
    path::{Path, PathBuf},
};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package};
use serde::Serialize;

/// The directory that holds the workspace's root `Cargo.toml`.
#[derive(Debug, Clone)]
pub struct WorkspaceRoot(PathBuf);

impl WorkspaceRoot {
    /// Finds the root of the workspace that `path` belongs to.
    ///
    /// `path` may be the workspace root or any directory inside it, such as a member crate.
    pub fn locate(path: &Path) -> Result<Self, DiscoveryError> {
        // Without this check a missing directory surfaces as "failed to start `cargo metadata`",
        // which reads as if cargo were missing.
        if !path.is_dir() {
            return Err(DiscoveryError::NotADirectory(path.to_path_buf()));
        }
        let metadata = metadata_in(path)?;
        Ok(Self(metadata.workspace_root.into_std_path_buf()))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Resolves `path` against the workspace root unless it is already absolute.
    pub fn resolve(&self, path: &Path) -> PathBuf {
        self.0.join(path)
    }
}

impl fmt::Display for WorkspaceRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}

/// The workspace could not be found or described.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("workspace path {} does not exist or is not a directory", .0.display())]
    NotADirectory(PathBuf),
    #[error("`cargo metadata` failed in {}: {cause}", dir.display())]
    Metadata {
        dir: PathBuf,
        cause: cargo_metadata::Error,
    },
}

fn metadata_in(dir: &Path) -> Result<Metadata, DiscoveryError> {
    MetadataCommand::new()
        .current_dir(dir)
        .no_deps()
        .exec()
        .map_err(|cause| DiscoveryError::Metadata {
            dir: dir.to_path_buf(),
            cause,
        })
}

/// Which workspace members should be documented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberSelection {
    /// Only crates that can be published (skips examples, benchmarks and `publish = false`).
    Published,
    /// Every workspace member.
    All,
}

/// A workspace dependency declared in a member's `Cargo.toml`.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceDependency {
    pub name: String,
    pub optional: bool,
}

impl fmt::Display for WorkspaceDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.optional {
            write!(f, "{} (optional)", self.name)
        } else {
            f.write_str(&self.name)
        }
    }
}

/// A member crate, as `cargo metadata` describes it.
#[derive(Debug, Clone)]
pub struct WorkspaceCrate {
    pub name: String,
    pub dir: PathBuf,
    pub description: Option<String>,
    pub workspace_deps: Vec<WorkspaceDependency>,
    pub features: Vec<String>,
}

/// The crates selected for documentation, foundations first.
#[derive(Debug, Clone)]
pub struct WorkspaceMembers {
    pub crates: Vec<WorkspaceCrate>,
    pub skipped: usize,
}

impl fmt::Display for WorkspaceMembers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} crates ({} other members skipped)",
            self.crates.len(),
            self.skipped
        )
    }
}

/// Runs `cargo metadata` in `root` and returns the selected members in dependency order.
pub fn discover(
    root: &WorkspaceRoot,
    selection: MemberSelection,
) -> Result<WorkspaceMembers, DiscoveryError> {
    let metadata = metadata_in(root.as_path())?;

    let members = metadata.workspace_packages();
    let member_names: HashSet<&str> = members
        .iter()
        .map(|package| package.name.as_str())
        .collect();

    let selected: Vec<WorkspaceCrate> = members
        .iter()
        .filter(|package| selection == MemberSelection::All || is_publishable(package))
        .map(|package| workspace_crate(package, &member_names))
        .collect();

    Ok(WorkspaceMembers {
        skipped: members.len() - selected.len(),
        crates: in_dependency_order(selected),
    })
}

fn is_publishable(package: &Package) -> bool {
    package
        .publish
        .as_ref()
        .is_none_or(|registries| !registries.is_empty())
}

fn workspace_crate(package: &Package, member_names: &HashSet<&str>) -> WorkspaceCrate {
    let workspace_deps = package
        .dependencies
        .iter()
        .filter(|dep| {
            dep.kind == DependencyKind::Normal && member_names.contains(dep.name.as_str())
        })
        .map(|dep| WorkspaceDependency {
            name: dep.name.clone(),
            optional: dep.optional,
        })
        .collect();

    let dir = package
        .manifest_path
        .parent()
        .map_or_else(PathBuf::new, |dir| dir.as_std_path().to_path_buf());

    WorkspaceCrate {
        name: package.name.to_string(),
        dir,
        description: package.description.clone(),
        workspace_deps,
        features: package
            .features
            .keys()
            .filter(|feature| feature.as_str() != "default")
            .cloned()
            .collect(),
    }
}

/// Orders crates so that every crate comes after the workspace crates it depends on.
///
/// Ties are broken alphabetically, which keeps the generated document stable between runs.
fn in_dependency_order(mut pending: Vec<WorkspaceCrate>) -> Vec<WorkspaceCrate> {
    pending.sort_by(|a, b| a.name.cmp(&b.name));
    let mut ordered: Vec<WorkspaceCrate> = Vec::with_capacity(pending.len());

    while !pending.is_empty() {
        let is_waiting = |name: &str| pending.iter().any(|other| other.name == name);
        let next = pending
            .iter()
            .position(|candidate| {
                candidate
                    .workspace_deps
                    .iter()
                    .all(|dep| dep.name == candidate.name || !is_waiting(&dep.name))
            })
            // A dependency cycle cannot happen in Cargo, but never loop forever on bad input.
            .unwrap_or(0);
        ordered.push(pending.remove(next));
    }

    ordered
}
