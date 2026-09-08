use std::{
    collections::BTreeSet,
    io::Read,
    path::{Component, Path, PathBuf},
};

use cap_std::fs::Dir;
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::model::{EvidenceFact, EvidenceReceipt, Freshness, ReceiptCheck, ReceiptInput};

const MAX_INPUTS: usize = 32;
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const HASH_BUFFER_BYTES: usize = 8 * 1024;

/// Captures an evidence receipt rooted exclusively in the supplied project directory.
pub(crate) fn capture_receipt(
    root: &Dir,
    proof_needed: &str,
    paths: &[String],
) -> Result<EvidenceReceipt, String> {
    if proof_needed.trim().is_empty() {
        return Err("proof requirement must not be blank".into());
    }

    let paths = canonical_input_paths(paths)?;
    let inputs = paths
        .iter()
        .map(|path| {
            hash_regular_file(root, path).map(|sha256| ReceiptInput {
                path: path.clone(),
                sha256,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let recorded_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| format!("could not format receipt timestamp: {error}"))?;

    Ok(EvidenceReceipt {
        recorded_at,
        proof_needed: proof_needed.to_owned(),
        inputs,
    })
}

/// Ensures the serialized fields are safe and canonical before any project files are read.
pub(crate) fn validate_receipt(receipt: &EvidenceReceipt) -> Result<(), String> {
    OffsetDateTime::parse(&receipt.recorded_at, &Rfc3339)
        .map_err(|error| format!("recorded_at must be RFC3339: {error}"))?;
    if receipt.proof_needed.trim().is_empty() {
        return Err("proof_needed must not be blank".into());
    }
    if receipt.inputs.len() > MAX_INPUTS {
        return Err(format!("receipt may bind at most {MAX_INPUTS} input files"));
    }

    let mut previous: Option<&str> = None;
    for input in &receipt.inputs {
        validate_input_path(&input.path)?;
        if !is_lowercase_sha256(&input.sha256) {
            return Err(format!(
                "receipt digest for {} must be 64 lowercase hexadecimal characters",
                input.path
            ));
        }
        if let Some(previous) = previous
            && input.path.as_str() <= previous
        {
            return Err(format!(
                "receipt input paths must be unique and sorted canonically (problem at {})",
                input.path
            ));
        }
        previous = Some(input.path.as_str());
    }
    Ok(())
}

/// Updates only the derived receipt check. Structural errors remain load errors; changed or
/// unavailable current inputs become stale evidence that can be recorded again later.
pub(crate) fn check_receipt(
    root: &Dir,
    proof_needed: &str,
    fact: &mut EvidenceFact,
) -> Result<(), String> {
    let Some(receipt) = &fact.receipt else {
        fact.receipt_check = ReceiptCheck::Unchecked;
        return Ok(());
    };
    validate_receipt(receipt)?;

    // Historical evidence must remain readable even after its source is deliberately retired.
    if fact.freshness == Freshness::Stale {
        fact.receipt_check = ReceiptCheck::Unchecked;
        return Ok(());
    }
    if receipt.proof_needed != proof_needed {
        fact.receipt_check = ReceiptCheck::NeedsRecheck(
            "Proof requirement changed since this evidence receipt was recorded.".into(),
        );
        return Ok(());
    }

    for input in &receipt.inputs {
        match hash_regular_file(root, &input.path) {
            Ok(actual) if actual == input.sha256 => {}
            Ok(_) => {
                fact.receipt_check =
                    ReceiptCheck::NeedsRecheck(format!("Receipt input changed: {}.", input.path));
                return Ok(());
            }
            Err(reason) => {
                fact.receipt_check = ReceiptCheck::NeedsRecheck(format!(
                    "Receipt input {} needs recheck: {reason}",
                    input.path
                ));
                return Ok(());
            }
        }
    }
    fact.receipt_check = ReceiptCheck::Matched;
    Ok(())
}

fn canonical_input_paths(paths: &[String]) -> Result<Vec<String>, String> {
    let mut canonical = BTreeSet::new();
    for path in paths {
        validate_input_path(path)?;
        canonical.insert(path.clone());
    }
    if canonical.len() > MAX_INPUTS {
        return Err(format!(
            "a receipt may bind at most {MAX_INPUTS} input files"
        ));
    }
    Ok(canonical.into_iter().collect())
}

/// Paths use slash-separated, relative lexical form with no aliases. This lets serialized
/// receipts be compared directly and keeps receipt metadata out of the evidence boundary.
fn validate_input_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("receipt input path must not be empty".into());
    }
    if path.contains('\\') || path.contains(':') || path.chars().any(char::is_control) {
        return Err(format!(
            "receipt input path is not portable or safe: {path:?}"
        ));
    }
    let parsed = Path::new(path);
    if parsed.is_absolute()
        || parsed.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "receipt input path must stay below the project root: {path:?}"
        ));
    }
    let segments = path.split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || segment.ends_with('.') || segment.ends_with(' '))
    {
        return Err(format!(
            "receipt input path must use canonical slash-separated form: {path:?}"
        ));
    }
    if segments.iter().any(|segment| {
        segment.eq_ignore_ascii_case(".git") || segment.eq_ignore_ascii_case(".proof-lantern")
    }) {
        return Err(format!(
            "receipt input path may not bind project metadata: {path:?}"
        ));
    }
    Ok(())
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hash_regular_file(root: &Dir, path: &str) -> Result<String, String> {
    validate_input_path(path)?;
    reject_symlink_components(root, path)?;
    let metadata = root
        .metadata(path)
        .map_err(|error| format!("cannot inspect path inside the project root: {error}"))?;
    if !metadata.is_file() {
        return Err("path is not a regular file".into());
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "file exceeds the {MAX_INPUT_BYTES}-byte receipt limit"
        ));
    }

    let mut file = root
        .open(path)
        .map_err(|error| format!("cannot open file inside the project root: {error}"))?;
    let opened_metadata = file
        .metadata()
        .map_err(|error| format!("cannot inspect opened file: {error}"))?;
    if !opened_metadata.is_file() {
        return Err("opened path is not a regular file".into());
    }

    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read file inside the project root: {error}"))?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(u64::try_from(count).expect("buffer length fits u64"));
        if total > MAX_INPUT_BYTES {
            return Err(format!(
                "file exceeds the {MAX_INPUT_BYTES}-byte receipt limit"
            ));
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn reject_symlink_components(root: &Dir, path: &str) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in Path::new(path).components() {
        let Component::Normal(segment) = component else {
            return Err(format!("receipt input path is not canonical: {path:?}"));
        };
        current.push(segment);
        let metadata = root
            .symlink_metadata(&current)
            .map_err(|error| format!("cannot inspect path inside the project root: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("path or a parent directory is a symlink".into());
        }
    }
    Ok(())
}
