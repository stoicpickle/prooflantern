use std::{
    error::Error,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use cap_std::fs::OpenOptions;

use crate::project_fs::{CONFIG_DIR, ProjectDirectory};

#[derive(Debug)]
pub enum InitError {
    Root { path: String, source: io::Error },
    NotDirectory { path: String },
    ConfigOutsideRoot { path: String },
    AlreadyExists { path: String },
    Write { path: String, source: io::Error },
}

impl fmt::Display for InitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root { path, source } => {
                write!(
                    formatter,
                    "could not open project directory {path}: {source}"
                )
            }
            Self::NotDirectory { path } => {
                write!(formatter, "project path is not a directory: {path}")
            }
            Self::ConfigOutsideRoot { path } => write!(
                formatter,
                "refusing to initialize because the .proof-lantern directory resolves outside the project root: {path}"
            ),
            Self::AlreadyExists { path } => write!(
                formatter,
                "a Proof Lantern map already exists at {path}\nNothing was changed; edit that file or move it before initializing again."
            ),
            Self::Write { path, source } => {
                write!(
                    formatter,
                    "could not create starter map at {path}: {source}"
                )
            }
        }
    }
}

impl Error for InitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Root { source, .. } | Self::Write { source, .. } => Some(source),
            Self::NotDirectory { .. }
            | Self::ConfigOutsideRoot { .. }
            | Self::AlreadyExists { .. } => None,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct InitializedProject {
    pub project_file: PathBuf,
}

pub fn initialize_project(root: impl AsRef<Path>) -> Result<InitializedProject, InitError> {
    let root = root.as_ref();
    let project = ProjectDirectory::open(root).map_err(|source| InitError::Root {
        path: root.display().to_string(),
        source,
    })?;
    let unresolved_config_dir = project.config_path();
    match project.root.create_dir(CONFIG_DIR) {
        Ok(()) => {}
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
        Err(source) => {
            return Err(InitError::Write {
                path: unresolved_config_dir.display().to_string(),
                source,
            });
        }
    }
    let config_dir = project.open_config().map_err(|source| {
        if fs::canonicalize(&unresolved_config_dir)
            .is_ok_and(|resolved| !resolved.starts_with(&project.canonical_root))
        {
            InitError::ConfigOutsideRoot {
                path: unresolved_config_dir.display().to_string(),
            }
        } else if source.kind() == io::ErrorKind::NotADirectory {
            InitError::NotDirectory {
                path: unresolved_config_dir.display().to_string(),
            }
        } else {
            InitError::Write {
                path: unresolved_config_dir.display().to_string(),
                source,
            }
        }
    })?;

    let project_file = unresolved_config_dir.join("project.yml");

    let project_name = project
        .canonical_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("My Project");
    let project_name = serde_json::to_string(project_name)
        .expect("serializing a project directory name cannot fail");
    let template = starter_template(&project_name);

    let mut file = config_dir
        .open_with(
            "project.yml",
            OpenOptions::new().write(true).create_new(true),
        )
        .map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                InitError::AlreadyExists {
                    path: project_file.display().to_string(),
                }
            } else {
                InitError::Write {
                    path: project_file.display().to_string(),
                    source,
                }
            }
        })?;
    if let Err(source) = file.write_all(template.as_bytes()) {
        drop(file);
        let _ = config_dir.remove_file("project.yml");
        return Err(InitError::Write {
            path: project_file.display().to_string(),
            source,
        });
    }

    Ok(InitializedProject { project_file })
}

fn starter_template(project_name: &str) -> String {
    format!(
        r#"# Proof Lantern starter map
#
# Start with the experience, not the files. Replace the promise and the three
# placeholder capabilities below with the shortest journey a user must finish.
# `proof_needed` should describe something a person or test can actually observe.
#
# New capabilities begin UNKNOWN. That is honest: this file records what you
# intend to build, while `proof-lantern record` writes what you actually observe
# to .proof-lantern/manual-evidence.json without rewriting this file.

schema_version: 1
project:
  name: {project_name}
  promise: Replace this with the smallest useful result a user should reach.
capabilities:
  - id: start
    label: Start the core journey
    map_label: Start
    role:
      kind: core
      order: 1
    proof_needed: Describe one visible check that proves the journey can start.

  - id: outcome
    label: Reach the core outcome
    map_label: Outcome
    role:
      kind: core
      order: 2
    depends_on: [start]
    proof_needed: Describe one visible check that proves the core outcome works.

  - id: return
    label: Return and use the result again
    map_label: Return
    role:
      kind: core
      order: 3
    depends_on: [outcome]
    proof_needed: Close or leave the project, return, and confirm the result remains usable.
"#
    )
}
