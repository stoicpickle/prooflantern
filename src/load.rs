use std::{error::Error, fmt, fs, path::Path};

use crate::model::{EvidenceFact, Freshness, ObservationSet, ProjectSpec};

const DEMO_PROJECT_YAML: &str = include_str!("../fixtures/recipe_box/.proof-lantern/project.yml");
const DEMO_OBSERVATIONS_JSON: &str =
    include_str!("../fixtures/recipe_box/.proof-lantern/observations.json");

#[derive(Debug)]
pub enum LoadError {
    MissingProject {
        path: String,
        project_root: String,
    },
    Read {
        path: String,
        source: std::io::Error,
    },
    ProjectYaml(serde_saphyr::Error),
    ObservationsJson(serde_json::Error),
    EvidenceLocation {
        capability_id: String,
        path: String,
        reason: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProject { path, project_root } => write!(
                formatter,
                "no Proof Lantern map found at {path}\nFrom the project directory ({project_root}), run `proof-lantern init .`; or try `proof-lantern demo` to explore the built-in example."
            ),
            Self::Read { path, source } => write!(formatter, "could not read {path}: {source}"),
            Self::ProjectYaml(source) => write!(formatter, "invalid project YAML: {source}"),
            Self::ObservationsJson(source) => {
                write!(formatter, "invalid observations JSON: {source}")
            }
            Self::EvidenceLocation {
                capability_id,
                path,
                reason,
            } => write!(
                formatter,
                "evidence for {capability_id} is not inspectable at {path}: {reason}"
            ),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MissingProject { .. } => None,
            Self::Read { source, .. } => Some(source),
            Self::ProjectYaml(source) => Some(source),
            Self::ObservationsJson(source) => Some(source),
            Self::EvidenceLocation { .. } => None,
        }
    }
}

pub fn load_project(root: impl AsRef<Path>) -> Result<(ProjectSpec, ObservationSet), LoadError> {
    let root = root.as_ref();
    let config = root.join(".proof-lantern");
    let project_path = config.join("project.yml");
    let observations_path = config.join("observations.json");
    let project_text = read_project(&project_path, root)?;
    let project = parse_project(&project_text)?;
    let observations = match fs::read_to_string(&observations_path) {
        Ok(text) => parse_observations(&text)?,
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
    validate_current_evidence_locations(root, &project, &observations)?;
    Ok((project, observations))
}

pub fn load_demo() -> Result<(ProjectSpec, ObservationSet), LoadError> {
    let mut project = parse_project(DEMO_PROJECT_YAML)?;
    project.project.name.push_str(" — Synthetic Demo");
    Ok((project, parse_observations(DEMO_OBSERVATIONS_JSON)?))
}

fn parse_project(text: &str) -> Result<ProjectSpec, LoadError> {
    serde_saphyr::from_str(text).map_err(LoadError::ProjectYaml)
}

fn parse_observations(text: &str) -> Result<ObservationSet, LoadError> {
    serde_json::from_str(text).map_err(LoadError::ObservationsJson)
}

fn validate_current_evidence_locations(
    root: &Path,
    project: &ProjectSpec,
    observations: &ObservationSet,
) -> Result<(), LoadError> {
    let canonical_root = fs::canonicalize(root).map_err(|source| LoadError::Read {
        path: root.display().to_string(),
        source,
    })?;

    for capability in &project.capabilities {
        for fact in capability
            .manual_evidence
            .iter()
            .filter(|fact| fact.freshness == Freshness::Current)
        {
            validate_fact_location(&canonical_root, &capability.id, fact)?;
        }
    }
    for observation in observations
        .observations
        .iter()
        .filter(|observation| observation.fact.freshness == Freshness::Current)
    {
        validate_fact_location(
            &canonical_root,
            &observation.capability_id,
            &observation.fact,
        )?;
    }
    Ok(())
}

fn validate_fact_location(
    canonical_root: &Path,
    capability_id: &str,
    fact: &EvidenceFact,
) -> Result<(), LoadError> {
    let Some(location) = &fact.location else {
        return Ok(());
    };
    let unresolved = canonical_root.join(&location.path);
    let resolved = fs::canonicalize(&unresolved).map_err(|source| LoadError::EvidenceLocation {
        capability_id: capability_id.to_owned(),
        path: location.path.clone(),
        reason: source.to_string(),
    })?;
    if !resolved.starts_with(canonical_root) {
        return Err(LoadError::EvidenceLocation {
            capability_id: capability_id.to_owned(),
            path: location.path.clone(),
            reason: "resolved path escapes the project root".into(),
        });
    }
    if !resolved.is_file() {
        return Err(LoadError::EvidenceLocation {
            capability_id: capability_id.to_owned(),
            path: location.path.clone(),
            reason: "resolved path is not a file".into(),
        });
    }
    if let Some(line_end) = location.line_end {
        let text = fs::read_to_string(&resolved).map_err(|source| LoadError::EvidenceLocation {
            capability_id: capability_id.to_owned(),
            path: location.path.clone(),
            reason: format!("line citation requires readable UTF-8 text: {source}"),
        })?;
        let line_count = text.lines().count();
        if usize::try_from(line_end).map_or(true, |end| end > line_count) {
            return Err(LoadError::EvidenceLocation {
                capability_id: capability_id.to_owned(),
                path: location.path.clone(),
                reason: format!("line {line_end} exceeds the file's {line_count} lines"),
            });
        }
    }
    Ok(())
}

fn read_project(path: &Path, project_root: &Path) -> Result<String, LoadError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            Err(LoadError::MissingProject {
                path: path.display().to_string(),
                project_root: project_root.display().to_string(),
            })
        }
        Err(source) => Err(LoadError::Read {
            path: path.display().to_string(),
            source,
        }),
    }
}
