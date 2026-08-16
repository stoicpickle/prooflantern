use std::{
    error::Error,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

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
    let canonical_root = fs::canonicalize(root).map_err(|source| InitError::Root {
        path: root.display().to_string(),
        source,
    })?;
    if !canonical_root.is_dir() {
        return Err(InitError::NotDirectory {
            path: root.display().to_string(),
        });
    }

    let unresolved_config_dir = canonical_root.join(".proof-lantern");
    fs::create_dir_all(&unresolved_config_dir).map_err(|source| InitError::Write {
        path: unresolved_config_dir.display().to_string(),
        source,
    })?;
    let config_dir =
        fs::canonicalize(&unresolved_config_dir).map_err(|source| InitError::Write {
            path: unresolved_config_dir.display().to_string(),
            source,
        })?;
    if !config_dir.starts_with(&canonical_root) {
        return Err(InitError::ConfigOutsideRoot {
            path: unresolved_config_dir.display().to_string(),
        });
    }
    if !config_dir.is_dir() {
        return Err(InitError::NotDirectory {
            path: unresolved_config_dir.display().to_string(),
        });
    }

    let project_file = config_dir.join("project.yml");
    if project_file.exists() {
        return Err(InitError::AlreadyExists {
            path: project_file.display().to_string(),
        });
    }

    let project_name = canonical_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("My Project");
    let project_name = serde_json::to_string(project_name)
        .expect("serializing a project directory name cannot fail");
    let template = starter_template(&project_name);

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&project_file)
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
        let _ = fs::remove_file(&project_file);
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
# intend to build, while evidence of what exists or works is recorded separately.

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
