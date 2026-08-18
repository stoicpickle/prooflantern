use std::{error::Error, fmt, io, path::Path};

use cap_std::fs::Dir;

use crate::{
    model::{EvidenceFact, Freshness, ManualEvidenceSet, ObservationSet, ProjectSpec},
    project_fs::ProjectDirectory,
};

const DEMO_PROJECT_YAML: &str = include_str!("../fixtures/recipe_box/.proof-lantern/project.yml");
const DEMO_OBSERVATIONS_JSON: &str =
    include_str!("../fixtures/recipe_box/.proof-lantern/observations.json");
const MANUAL_EVIDENCE_FILE: &str = "manual-evidence.json";

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
    ManualEvidenceJson(serde_json::Error),
    ManualEvidence {
        capability_id: String,
        reason: String,
    },
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
            Self::ManualEvidenceJson(source) => {
                write!(formatter, "invalid manual evidence JSON: {source}")
            }
            Self::ManualEvidence {
                capability_id,
                reason,
            } => write!(
                formatter,
                "manual evidence for {capability_id} is invalid: {reason}"
            ),
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
            Self::ManualEvidenceJson(source) => Some(source),
            Self::ManualEvidence { .. } | Self::EvidenceLocation { .. } => None,
        }
    }
}

pub fn load_project(root: impl AsRef<Path>) -> Result<(ProjectSpec, ObservationSet), LoadError> {
    let (mut project, observations, manual, project_dir) = load_project_files(root)?;
    merge_manual_evidence(&mut project, &manual)?;
    validate_current_evidence_locations(&project_dir.root, &project, &observations)?;
    Ok((project, observations))
}

pub(crate) fn load_project_files(
    root: impl AsRef<Path>,
) -> Result<
    (
        ProjectSpec,
        ObservationSet,
        ManualEvidenceSet,
        ProjectDirectory,
    ),
    LoadError,
> {
    let root = root.as_ref();
    let project_dir = ProjectDirectory::open(root).map_err(|source| LoadError::Read {
        path: root.display().to_string(),
        source,
    })?;
    let config_path = project_dir.config_path();
    let requested_project_path = root.join(".proof-lantern/project.yml");
    let config = match project_dir.open_config() {
        Ok(config) => config,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(LoadError::MissingProject {
                path: requested_project_path.display().to_string(),
                project_root: root.display().to_string(),
            });
        }
        Err(source) => {
            return Err(LoadError::Read {
                path: config_path.display().to_string(),
                source,
            });
        }
    };
    let project_text = read_project(&config, &requested_project_path, root)?;
    let project = parse_project(&project_text)?;
    let observations = read_optional_json(
        &config,
        "observations.json",
        &config_path.join("observations.json"),
        parse_observations,
    )?
    .unwrap_or_default();
    let manual = read_optional_json(
        &config,
        MANUAL_EVIDENCE_FILE,
        &config_path.join(MANUAL_EVIDENCE_FILE),
        parse_manual_evidence,
    )?
    .unwrap_or_default();
    Ok((project, observations, manual, project_dir))
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

fn parse_manual_evidence(text: &str) -> Result<ManualEvidenceSet, LoadError> {
    serde_json::from_str(text).map_err(LoadError::ManualEvidenceJson)
}

fn read_optional_json<T>(
    config: &Dir,
    name: &str,
    display_path: &Path,
    parse: impl FnOnce(&str) -> Result<T, LoadError>,
) -> Result<Option<T>, LoadError> {
    match config.read_to_string(name) {
        Ok(text) => parse(&text).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(LoadError::Read {
            path: display_path.display().to_string(),
            source,
        }),
    }
}

pub(crate) fn merge_manual_evidence(
    project: &mut ProjectSpec,
    manual: &ManualEvidenceSet,
) -> Result<(), LoadError> {
    if manual.schema_version != 1 {
        return Err(LoadError::ManualEvidence {
            capability_id: "<file>".into(),
            reason: format!(
                "unsupported manual evidence schema version {}",
                manual.schema_version
            ),
        });
    }
    for record in &manual.records {
        let Some(capability) = project
            .capabilities
            .iter_mut()
            .find(|capability| capability.id == record.capability_id)
        else {
            return Err(LoadError::ManualEvidence {
                capability_id: record.capability_id.clone(),
                reason: "capability does not exist in project.yml".into(),
            });
        };
        capability.manual_evidence.push(record.fact.clone());
    }
    Ok(())
}

fn validate_current_evidence_locations(
    root: &Dir,
    project: &ProjectSpec,
    observations: &ObservationSet,
) -> Result<(), LoadError> {
    for capability in &project.capabilities {
        for fact in capability
            .manual_evidence
            .iter()
            .filter(|fact| fact.freshness == Freshness::Current)
        {
            validate_fact_location(root, &capability.id, fact)?;
        }
    }
    for observation in observations
        .observations
        .iter()
        .filter(|observation| observation.fact.freshness == Freshness::Current)
    {
        validate_fact_location(root, &observation.capability_id, &observation.fact)?;
    }
    Ok(())
}

fn validate_fact_location(
    root: &Dir,
    capability_id: &str,
    fact: &EvidenceFact,
) -> Result<(), LoadError> {
    let Some(location) = &fact.location else {
        return Ok(());
    };
    let metadata = root
        .metadata(&location.path)
        .map_err(|source| LoadError::EvidenceLocation {
            capability_id: capability_id.to_owned(),
            path: location.path.clone(),
            reason: format!("path must exist and resolve inside the project root: {source}"),
        })?;
    if !metadata.is_file() {
        return Err(LoadError::EvidenceLocation {
            capability_id: capability_id.to_owned(),
            path: location.path.clone(),
            reason: "resolved path is not a file".into(),
        });
    }
    if let Some(line_end) = location.line_end {
        let text =
            root.read_to_string(&location.path)
                .map_err(|source| LoadError::EvidenceLocation {
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

fn read_project(config: &Dir, path: &Path, project_root: &Path) -> Result<String, LoadError> {
    match config.read_to_string("project.yml") {
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
