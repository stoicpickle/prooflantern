use std::{error::Error, fmt, fs, path::Path};

use crate::model::{ObservationSet, ProjectSpec};

#[derive(Debug)]
pub enum LoadError {
    Read {
        path: String,
        source: std::io::Error,
    },
    ProjectYaml(serde_saphyr::Error),
    ObservationsJson(serde_json::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(formatter, "could not read {path}: {source}"),
            Self::ProjectYaml(source) => write!(formatter, "invalid project YAML: {source}"),
            Self::ObservationsJson(source) => {
                write!(formatter, "invalid observations JSON: {source}")
            }
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::ProjectYaml(source) => Some(source),
            Self::ObservationsJson(source) => Some(source),
        }
    }
}

pub fn load_project(
    project_path: impl AsRef<Path>,
    observations_path: impl AsRef<Path>,
) -> Result<(ProjectSpec, ObservationSet), LoadError> {
    let project_path = project_path.as_ref();
    let observations_path = observations_path.as_ref();
    let project_text = read(project_path)?;
    let project = serde_saphyr::from_str(&project_text).map_err(LoadError::ProjectYaml)?;
    let observations = match fs::read_to_string(observations_path) {
        Ok(text) => serde_json::from_str(&text).map_err(LoadError::ObservationsJson)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ObservationSet {
            schema_version: 1,
            observations: Vec::new(),
        },
        Err(source) => {
            return Err(LoadError::Read {
                path: observations_path.display().to_string(),
                source,
            });
        }
    };
    Ok((project, observations))
}

fn read(path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.display().to_string(),
        source,
    })
}
