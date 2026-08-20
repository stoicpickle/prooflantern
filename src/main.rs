use std::{
    error::Error,
    io::{self, Write},
    path::Path,
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use clap::Parser;
use proof_lantern::{
    App, CurrentFocus, DisplayState, EvaluatedProject, EvidenceSource, Freshness,
    cli::{Cli, Invocation},
    evaluate, initialize_project, load_demo, load_project, record_manual_evidence,
};

#[cfg(feature = "terminal-test-hooks")]
const TERMINAL_EXIT_PROBE_ENV: &str = "PROOF_LANTERN_TEST_TERMINAL_EXIT";

#[cfg(feature = "terminal-test-hooks")]
#[derive(Clone, Copy)]
enum TerminalExitProbe {
    Ok,
    Error,
    Panic,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("proof-lantern: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let invocation = Cli::parse().invocation();
    match invocation {
        Invocation::Init(path) => {
            let initialized = initialize_project(&path)?;
            println!("Created {}", initialized.project_file.display());
            println!();
            println!("Next:");
            println!("  1. Open that file and replace the starter promise and capabilities.");
            println!("  2. From that project folder, run `proof-lantern .` to view the map.");
            println!(
                "  3. After a real check, record what you saw with `proof-lantern record start passed --summary \"what worked\" .`."
            );
            println!("  4. Run `proof-lantern demo` to compare it with an evidence-rich example.");
            println!();
            println!(
                "Seeing UNKNOWN at first is expected: intent exists, but evidence has not been recorded yet."
            );
            println!(
                "Guide: https://github.com/stoicpickle/prooflantern/blob/main/docs/PROJECT_FORMAT.md"
            );
            return Ok(());
        }
        Invocation::Next(path) => {
            print_next(&load_root(&path)?, &path)?;
            return Ok(());
        }
        Invocation::Explain { node, path } => {
            print_explanation(&load_root(path)?, &node)?;
            return Ok(());
        }
        Invocation::Record {
            node,
            claim,
            summary,
            path,
        } => {
            ensure_known_capability(&load_root(&path)?, &node)?;
            let recorded = record_manual_evidence(&path, &node, claim.into(), &summary)?;
            println!(
                "Recorded {} for {} in {}",
                recorded.display.label(),
                recorded.capability_label,
                recorded.evidence_file.display()
            );
            if recorded.superseded_records > 0 {
                println!(
                    "Kept {} older manual record(s) as STALE history.",
                    recorded.superseded_records
                );
            }
            println!("Next: run `proof-lantern next .` from this map root.");
            return Ok(());
        }
        Invocation::Demo | Invocation::Project(_) => {}
    }

    let (project, project_command_hints) = match invocation {
        Invocation::Demo => {
            let (spec, observations) = load_demo()?;
            (evaluate(spec, observations)?, false)
        }
        Invocation::Project(path) => (load_root(path)?, true),
        Invocation::Init(_)
        | Invocation::Next(_)
        | Invocation::Explain { .. }
        | Invocation::Record { .. } => unreachable!(),
    };
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal_flag = Arc::clone(&interrupted);
    ctrlc::set_handler(move || signal_flag.store(true, Ordering::Relaxed))?;
    let mut app = App::new(project).with_interrupt_flag(interrupted);
    if project_command_hints {
        app = app.with_project_command_hints();
    }

    #[cfg(feature = "terminal-test-hooks")]
    {
        let exit_probe = terminal_exit_probe()?;
        ratatui::run(|terminal| match exit_probe {
            Some(TerminalExitProbe::Ok) => {
                terminal.draw(|frame| proof_lantern::ui::render(frame, &app))?;
                Ok(())
            }
            Some(TerminalExitProbe::Error) => {
                terminal.draw(|frame| proof_lantern::ui::render(frame, &app))?;
                Err(io::Error::other("injected terminal failure"))
            }
            Some(TerminalExitProbe::Panic) => {
                terminal.draw(|frame| proof_lantern::ui::render(frame, &app))?;
                panic!("injected terminal panic");
            }
            None => app.run(terminal),
        })?;
    }
    #[cfg(not(feature = "terminal-test-hooks"))]
    ratatui::run(|terminal| app.run(terminal))?;
    Ok(())
}

fn load_root(root: impl AsRef<Path>) -> Result<EvaluatedProject, Box<dyn Error>> {
    let (spec, observations) = load_project(root)?;
    Ok(evaluate(spec, observations)?)
}

fn print_next(project: &EvaluatedProject, path: &Path) -> io::Result<()> {
    let mut output = io::stdout().lock();
    match project.current_focus() {
        CurrentFocus::Complete { heading, summary } => {
            writeln!(output, "{heading}")?;
            writeln!(output, "{summary}")?;
        }
        CurrentFocus::Capability {
            capability,
            kind,
            summary,
            action,
            ..
        } => {
            writeln!(output, "{}", kind.heading())?;
            writeln!(
                output,
                "{} {} — {}",
                capability.display.glyph(),
                capability.intent.label,
                capability.display.label()
            )?;
            writeln!(output, "Capability ID: {}", capability.intent.id)?;
            writeln!(output, "Run commands from map root: {}", path.display())?;
            writeln!(output, "{summary}")?;
            writeln!(output, "{}: {}", action.heading, action.instruction)?;
            writeln!(output, "Inspect this capability and its evidence:")?;
            writeln!(
                output,
                "  proof-lantern explain -- {}",
                capability.intent.id
            )?;
            match capability.display {
                DisplayState::Conflicting => {
                    writeln!(
                        output,
                        "Reconcile the conflicting current evidence before recording another result."
                    )?;
                }
                DisplayState::Missing => {
                    writeln!(
                        output,
                        "Review the current MISSING evidence first. Project-authored or imported evidence must be updated or marked STALE at its source."
                    )?;
                    print_record_template(
                        &mut output,
                        capability.intent.id.as_str(),
                        "After reconciling that evidence, edit this record template:",
                    )?;
                }
                DisplayState::ProofFailed => {
                    writeln!(
                        output,
                        "Review the current failed evidence first. Project-authored or imported evidence must be updated or marked STALE at its source."
                    )?;
                    print_record_template(
                        &mut output,
                        capability.intent.id.as_str(),
                        "After reconciling that evidence, edit this record template:",
                    )?;
                }
                DisplayState::Proven | DisplayState::BuiltUnproven | DisplayState::Unknown => {
                    print_record_template(
                        &mut output,
                        capability.intent.id.as_str(),
                        "Record template (edit CLAIM and the summary first):",
                    )?;
                }
            }
        }
    }
    print_warnings(&mut output, project)
}

fn print_record_template(
    output: &mut impl Write,
    capability_id: &str,
    heading: &str,
) -> io::Result<()> {
    writeln!(output, "{heading}")?;
    writeln!(
        output,
        "  proof-lantern record --summary \"REPLACE WITH WHAT YOU OBSERVED\" -- {capability_id} CLAIM"
    )?;
    writeln!(
        output,
        "CLAIM can be built, missing, passed, or failed; unresolved conflicts are rejected."
    )
}

fn print_explanation(project: &EvaluatedProject, node: &str) -> io::Result<()> {
    let capability = ensure_known_capability(project, node)?;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "{} {} — {}",
        capability.display.glyph(),
        capability.intent.label,
        capability.display.label()
    )?;
    writeln!(output, "Why: {}", capability.why())?;
    writeln!(output, "Evidence:")?;
    if capability.reasons.is_empty() {
        writeln!(output, "  No current evidence recorded.")?;
    } else {
        for reason in &capability.reasons {
            let source = match reason.source {
                EvidenceSource::Human => "HUMAN",
                EvidenceSource::StaticScan => "STATIC SCAN",
                EvidenceSource::ImportedTestResult => "IMPORTED TEST RESULT",
            };
            let freshness = match reason.fact.freshness {
                Freshness::Current => "CURRENT",
                Freshness::Stale => "STALE",
            };
            let location = match &reason.fact.location {
                Some(location) => match (location.line_start, location.line_end) {
                    (Some(start), Some(end)) => format!("{}:{start}-{end}", location.path),
                    _ => location.path.clone(),
                },
                None => "not recorded".to_owned(),
            };
            writeln!(output, "  - Source: {source}")?;
            writeln!(output, "    Freshness: {freshness}")?;
            writeln!(output, "    Summary: {}", reason.fact.summary)?;
            writeln!(output, "    Location: {location}")?;
        }
    }
    writeln!(output, "Proof needed: {}", capability.intent.proof_needed)?;
    print_warnings(&mut output, project)
}

fn ensure_known_capability<'a>(
    project: &'a EvaluatedProject,
    node: &str,
) -> io::Result<&'a proof_lantern::CapabilityAssessment> {
    project.capability(node).ok_or_else(|| {
        let valid_ids = project
            .capabilities
            .iter()
            .map(|capability| capability.intent.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unknown capability {node}\nValid capability IDs: {valid_ids}\nUse an ID from .proof-lantern/project.yml."
            ),
        )
    })
}

fn print_warnings(output: &mut impl Write, project: &EvaluatedProject) -> io::Result<()> {
    if project.warnings.is_empty() {
        return Ok(());
    }

    writeln!(output, "Warnings:")?;
    for warning in project.warning_messages() {
        writeln!(output, "  - {warning}")?;
    }
    Ok(())
}

#[cfg(feature = "terminal-test-hooks")]
fn terminal_exit_probe() -> Result<Option<TerminalExitProbe>, Box<dyn Error>> {
    let Some(value) = std::env::var_os(TERMINAL_EXIT_PROBE_ENV) else {
        return Ok(None);
    };
    match value.to_str() {
        Some("ok") => Ok(Some(TerminalExitProbe::Ok)),
        Some("error") => Ok(Some(TerminalExitProbe::Error)),
        Some("panic") => Ok(Some(TerminalExitProbe::Panic)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{TERMINAL_EXIT_PROBE_ENV} accepts only `ok`, `error`, or `panic`"),
        )
        .into()),
    }
}
