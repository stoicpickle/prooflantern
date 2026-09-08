//! A small, cooked-terminal intent interview. Nothing is written before Save.
use std::{
    error::Error,
    io::{self, BufRead, Read, Write},
    path::Path,
};

use crate::{
    CapabilityIntent, CapabilityRole, ObservationSet, ProjectIntent, ProjectSpec, evaluate,
    init::{InitTarget, InitializedProject},
};

const MAX_ANSWER_BYTES: usize = 1024;

/// The caller owns terminal detection. Accepting streams keeps the interview
/// testable without terminal control sequences, raw mode, or a second UI loop.
pub fn initialize_guided(
    root: impl AsRef<Path>,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<Option<InitializedProject>, Box<dyn Error>> {
    let target = InitTarget::open(root.as_ref())?;
    target.check_available(true)?;
    writeln!(output, "PROOF LANTERN / GUIDED SETUP")?;
    writeln!(
        output,
        "Describe the user experience, not your files or implementation tasks."
    )?;
    writeln!(
        output,
        "Nothing is saved until you choose save. Type :cancel at any prompt to stop."
    )?;
    let default_name = target.project_name();
    let default_name = if valid_text(default_name, 80) {
        default_name
    } else {
        "My Project"
    };
    let mut draft = ProjectSpec {
        schema_version: 1,
        project: ProjectIntent {
            name: default_name.into(),
            promise: String::new(),
            pinned_keystone: None,
        },
        capabilities: Vec::new(),
    };
    loop {
        if !interview(&mut draft, input, output)? {
            return cancelled(output);
        }
        evaluate(draft.clone(), ObservationSet::default())?;
        preview(&draft, output)?;
        loop {
            let Some(answer) = ask(input, output, "Choose save, edit, or cancel", "cancel", 16)?
            else {
                return cancelled(output);
            };
            match answer.to_ascii_lowercase().as_str() {
                "save" => {
                    let yaml = serde_saphyr::to_string(&draft)?;
                    let content = format!(
                        "# Human-owned journey, confirmed in guided setup.\n# UNKNOWN means no evidence yet, not broken.\n# Record observations with `proof-lantern record`; keep intent here.\n\n{yaml}"
                    );
                    return Ok(Some(target.save(&content, true)?));
                }
                "edit" => {
                    writeln!(
                        output,
                        "Edit your answers below; Enter keeps each current answer."
                    )?;
                    break;
                }
                "cancel" => return cancelled(output),
                _ => writeln!(
                    output,
                    "Type save, edit, or cancel. Enter cancels without saving."
                )?,
            }
        }
    }
}

fn cancelled(output: &mut impl Write) -> Result<Option<InitializedProject>, Box<dyn Error>> {
    writeln!(output, "Setup cancelled. Nothing was saved.")?;
    Ok(None)
}

fn interview(
    draft: &mut ProjectSpec,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<bool> {
    let Some(name) = ask(input, output, "Project name", &draft.project.name, 80)? else {
        return Ok(false);
    };
    draft.project.name = name;
    writeln!(
        output,
        "Example result: Save a recipe and find it again tomorrow."
    )?;
    let Some(promise) = ask(
        input,
        output,
        "What useful result should a user reach?",
        &draft.project.promise,
        240,
    )?
    else {
        return Ok(false);
    };
    draft.project.promise = promise;
    let default_count = draft.capabilities.len().max(3).to_string();
    let count = loop {
        let Some(answer) = ask(
            input,
            output,
            "How many core steps (3-5)?",
            &default_count,
            1,
        )?
        else {
            return Ok(false);
        };
        match answer.parse::<usize>() {
            Ok(count @ 3..=5) => break count,
            _ => writeln!(
                output,
                "Choose 3, 4, or 5. Keep only the shortest useful journey."
            )?,
        }
    };
    writeln!(
        output,
        "Each step follows the previous one. Example: Add -> Save -> Reopen."
    )?;
    writeln!(
        output,
        "For each check, describe a visible result, e.g. reopen and see the saved recipe."
    )?;
    for index in 0..count {
        let old = draft.capabilities.get(index);
        let Some(label) = ask(
            input,
            output,
            &format!("Step {} of {count}: what does the user do?", index + 1),
            old.map_or("", |c| &c.label),
            80,
        )?
        else {
            return Ok(false);
        };
        let Some(proof_needed) = ask(
            input,
            output,
            "What visible check would show this works?",
            old.map_or("", |c| &c.proof_needed),
            240,
        )?
        else {
            return Ok(false);
        };
        let capability = CapabilityIntent {
            id: format!("step-{}", index + 1),
            label,
            map_label: None,
            role: CapabilityRole::Core {
                order: (index + 1) as u16,
            },
            depends_on: if index == 0 {
                vec![]
            } else {
                vec![format!("step-{index}")]
            },
            proof_needed,
            notes: None,
            manual_evidence: vec![],
        };
        if index < draft.capabilities.len() {
            draft.capabilities[index] = capability;
        } else {
            draft.capabilities.push(capability);
        }
    }
    draft.capabilities.truncate(count);
    Ok(true)
}

fn preview(draft: &ProjectSpec, output: &mut impl Write) -> io::Result<()> {
    writeln!(output, "\nYOUR JOURNEY / PREVIEW (not saved)")?;
    writeln!(output, "{}", draft.project.name)?;
    writeln!(output, "Promise: {}\n", draft.project.promise)?;
    for (index, capability) in draft.capabilities.iter().enumerate() {
        if index > 0 {
            writeln!(output, "    |\n    v")?;
        }
        writeln!(
            output,
            "[?] {}  ({} / UNKNOWN)",
            capability.label, capability.id
        )?;
        writeln!(output, "    Check: {}", capability.proof_needed)?;
    }
    writeln!(
        output,
        "\nUNKNOWN means no evidence yet, not broken. These checks have not been run."
    )?;
    writeln!(
        output,
        "Save creates only .proof-lantern/project.yml; it does not scan or run your code."
    )
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(|c| {
            c.is_control() || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}

fn ask(
    input: &mut impl BufRead,
    output: &mut impl Write,
    label: &str,
    default: &str,
    max_chars: usize,
) -> io::Result<Option<String>> {
    loop {
        write!(output, "{label}")?;
        if !default.is_empty() {
            write!(output, " [{default}]")?;
        }
        write!(
            output,
            "{}",
            if default.is_empty() && label.ends_with('?') {
                " "
            } else {
                ": "
            }
        )?;
        output.flush()?;
        let mut bytes = Vec::new();
        let read = input
            .take((MAX_ANSWER_BYTES + 2) as u64)
            .read_until(b'\n', &mut bytes)?;
        if read == 0 {
            return Ok(None);
        }
        if !bytes.ends_with(b"\n") {
            if bytes.len() > MAX_ANSWER_BYTES {
                input.skip_until(b'\n')?;
                writeln!(
                    output,
                    "Answer too long. Use at most {max_chars} characters."
                )?;
                continue;
            }
            // EOF with an unfinished answer is cancellation, never confirmation.
            return Ok(None);
        }
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
        let Ok(line) = String::from_utf8(bytes) else {
            writeln!(output, "Use ordinary UTF-8 text for this answer.")?;
            continue;
        };
        if line.trim().eq_ignore_ascii_case(":cancel") {
            return Ok(None);
        }
        if line.trim().is_empty() && !default.is_empty() {
            return Ok(Some(default.into()));
        }
        if !valid_text(&line, max_chars) {
            writeln!(
                output,
                "Enter 1-{max_chars} characters of plain text, without control characters."
            )?;
            continue;
        }
        return Ok(Some(line.trim().to_owned()));
    }
}
