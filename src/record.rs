use std::{
    error::Error,
    fmt,
    io::{self, Write},
    path::{Path, PathBuf},
};

use cap_std::fs::OpenOptions;
use fs2::FileExt;

use crate::{
    DisplayState, EvaluationError, evaluate,
    load::{LoadError, load_project_files, merge_manual_evidence},
    load_project,
    model::{Claim, EvidenceFact, Freshness, ManualEvidenceRecord, ManualEvidenceSet},
};

const MANUAL_EVIDENCE_FILE: &str = "manual-evidence.json";

#[derive(Debug)]
pub enum RecordError {
    Load(LoadError),
    Evaluation(EvaluationError),
    UnknownCapability(String),
    BlankSummary,
    Conflict(String),
    Serialize(serde_json::Error),
    Lock { path: String, source: io::Error },
    Write { path: String, source: io::Error },
}

impl fmt::Display for RecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(source) => write!(formatter, "{source}"),
            Self::Evaluation(source) => write!(formatter, "{source}"),
            Self::UnknownCapability(id) => write!(formatter, "unknown capability {id}"),
            Self::BlankSummary => write!(formatter, "evidence summary must not be blank"),
            Self::Conflict(id) => write!(
                formatter,
                "recording this evidence would leave {id} CONFLICTING; reconcile current evidence in project.yml or observations.json first"
            ),
            Self::Serialize(source) => {
                write!(formatter, "could not serialize manual evidence: {source}")
            }
            Self::Lock { path, source } => {
                write!(
                    formatter,
                    "could not lock manual evidence for {path}: {source}"
                )
            }
            Self::Write { path, source } => {
                write!(
                    formatter,
                    "could not write manual evidence at {path}: {source}"
                )
            }
        }
    }
}

impl Error for RecordError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(source) => Some(source),
            Self::Evaluation(source) => Some(source),
            Self::Serialize(source) => Some(source),
            Self::Lock { source, .. } | Self::Write { source, .. } => Some(source),
            Self::UnknownCapability(_) | Self::BlankSummary | Self::Conflict(_) => None,
        }
    }
}

impl From<LoadError> for RecordError {
    fn from(source: LoadError) -> Self {
        Self::Load(source)
    }
}

impl From<EvaluationError> for RecordError {
    fn from(source: EvaluationError) -> Self {
        Self::Evaluation(source)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedEvidence {
    pub capability_id: String,
    pub capability_label: String,
    pub display: DisplayState,
    pub evidence_file: PathBuf,
    pub superseded_records: usize,
}

pub fn record_manual_evidence(
    root: impl AsRef<Path>,
    capability_id: &str,
    claim: Claim,
    summary: &str,
) -> Result<RecordedEvidence, RecordError> {
    if summary.trim().is_empty() {
        return Err(RecordError::BlankSummary);
    }

    let _lock = EvidenceLock::acquire(root.as_ref())?;
    pause_after_lock_for_test()?;

    let (loaded_project, loaded_observations) = load_project(root.as_ref())?;
    evaluate(loaded_project, loaded_observations)?;

    let (mut project, observations, mut manual, project_dir) = load_project_files(root.as_ref())?;
    let Some(capability) = project
        .capabilities
        .iter()
        .find(|capability| capability.id == capability_id)
    else {
        return Err(RecordError::UnknownCapability(capability_id.to_owned()));
    };
    let capability_label = capability.label.clone();

    let mut superseded_records = 0;
    for record in &mut manual.records {
        if record.capability_id == capability_id
            && record.fact.freshness == Freshness::Current
            && same_dimension(record.fact.claim, claim)
        {
            record.fact.freshness = Freshness::Stale;
            superseded_records += 1;
        }
    }
    manual.records.push(ManualEvidenceRecord {
        capability_id: capability_id.to_owned(),
        fact: EvidenceFact {
            claim,
            freshness: Freshness::Current,
            summary: summary.trim().to_owned(),
            location: None,
        },
    });

    merge_manual_evidence(&mut project, &manual)?;
    let evaluated = evaluate(project, observations)?;
    let display = evaluated
        .capability(capability_id)
        .expect("recorded capability was validated before evaluation")
        .display;
    if display == DisplayState::Conflicting {
        return Err(RecordError::Conflict(capability_id.to_owned()));
    }

    let evidence_file = project_dir.config_path().join(MANUAL_EVIDENCE_FILE);
    write_manual_evidence(
        &project_dir
            .open_config()
            .map_err(|source| RecordError::Write {
                path: project_dir.config_path().display().to_string(),
                source,
            })?,
        &evidence_file,
        &manual,
    )?;

    Ok(RecordedEvidence {
        capability_id: capability_id.to_owned(),
        capability_label,
        display,
        evidence_file,
        superseded_records,
    })
}

struct EvidenceLock(std::fs::File);

impl EvidenceLock {
    fn acquire(root: &Path) -> Result<Self, RecordError> {
        let project_dir = crate::project_fs::ProjectDirectory::open(root).map_err(|source| {
            RecordError::Lock {
                path: root.display().to_string(),
                source,
            }
        })?;
        let lock_path = std::env::temp_dir().join(format!(
            "proof-lantern-record-{:016x}.lock",
            stable_path_hash(&project_dir.canonical_root)
        ));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| RecordError::Lock {
                path: lock_path.display().to_string(),
                source,
            })?;
        FileExt::lock_exclusive(&file).map_err(|source| RecordError::Lock {
            path: lock_path.display().to_string(),
            source,
        })?;
        Ok(Self(file))
    }
}

impl Drop for EvidenceLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

fn stable_path_hash(path: &Path) -> u64 {
    path.as_os_str()
        .to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[cfg(feature = "terminal-test-hooks")]
fn pause_after_lock_for_test() -> Result<(), RecordError> {
    const ENV: &str = "PROOF_LANTERN_TEST_RECORD_PAUSE_MS";
    let Some(value) = std::env::var_os(ENV) else {
        return Ok(());
    };
    let milliseconds = value
        .to_str()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value <= 2_000)
        .ok_or_else(|| RecordError::Lock {
            path: ENV.into(),
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                "test pause must be an integer from 0 through 2000",
            ),
        })?;
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    Ok(())
}

#[cfg(not(feature = "terminal-test-hooks"))]
fn pause_after_lock_for_test() -> Result<(), RecordError> {
    Ok(())
}

fn write_manual_evidence(
    config: &cap_std::fs::Dir,
    display_path: &Path,
    manual: &ManualEvidenceSet,
) -> Result<(), RecordError> {
    let mut bytes = serde_json::to_vec_pretty(manual).map_err(RecordError::Serialize)?;
    bytes.push(b'\n');

    let mut last_error = None;
    for attempt in 0..100_u8 {
        let temporary = format!(".manual-evidence.json.tmp-{}-{attempt}", std::process::id());
        let file = config.open_with(&temporary, OpenOptions::new().write(true).create_new(true));
        let mut file = match file {
            Ok(file) => file,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                last_error = Some(source);
                continue;
            }
            Err(source) => {
                return Err(RecordError::Write {
                    path: display_path.display().to_string(),
                    source,
                });
            }
        };
        if let Err(source) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = config.remove_file(&temporary);
            return Err(RecordError::Write {
                path: display_path.display().to_string(),
                source,
            });
        }
        drop(file);
        if let Err(source) = config.rename(&temporary, config, MANUAL_EVIDENCE_FILE) {
            let _ = config.remove_file(&temporary);
            return Err(RecordError::Write {
                path: display_path.display().to_string(),
                source,
            });
        }
        sync_directory(config).map_err(|source| RecordError::Write {
            path: display_path.display().to_string(),
            source,
        })?;
        return Ok(());
    }

    Err(RecordError::Write {
        path: display_path.display().to_string(),
        source: last_error
            .unwrap_or_else(|| io::Error::other("could not allocate a temporary file")),
    })
}

#[cfg(unix)]
fn sync_directory(config: &cap_std::fs::Dir) -> io::Result<()> {
    config.try_clone()?.into_std_file().sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_config: &cap_std::fs::Dir) -> io::Result<()> {
    Ok(())
}

const fn same_dimension(left: Claim, right: Claim) -> bool {
    matches!(
        (left, right),
        (
            Claim::ImplementationPresent | Claim::ImplementationAbsent,
            Claim::ImplementationPresent | Claim::ImplementationAbsent
        ) | (
            Claim::VerificationPassed | Claim::VerificationFailed,
            Claim::VerificationPassed | Claim::VerificationFailed
        )
    )
}
