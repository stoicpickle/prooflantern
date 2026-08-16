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
    App, CurrentFocus, EvaluatedProject, EvidenceSource, Freshness,
    cli::{Cli, Invocation},
    evaluate, initialize_project, load_demo, load_project,
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
            println!("  3. Run `proof-lantern demo` to compare it with an evidence-rich example.");
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
            print_next(&load_root(path)?)?;
            return Ok(());
        }
        Invocation::Explain { node, path } => {
            print_explanation(&load_root(path)?, &node)?;
            return Ok(());
        }
        Invocation::Demo | Invocation::Project(_) => {}
    }

    let project = match invocation {
        Invocation::Demo => {
            let (spec, observations) = load_demo()?;
            evaluate(spec, observations)?
        }
        Invocation::Project(path) => load_root(path)?,
        Invocation::Init(_) | Invocation::Next(_) | Invocation::Explain { .. } => unreachable!(),
    };
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal_flag = Arc::clone(&interrupted);
    ctrlc::set_handler(move || signal_flag.store(true, Ordering::Relaxed))?;
    let app = App::new(project).with_interrupt_flag(interrupted);

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

fn print_next(project: &EvaluatedProject) -> io::Result<()> {
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
            writeln!(output, "{summary}")?;
            writeln!(output, "{}: {}", action.heading, action.instruction)?;
        }
    }
    print_warnings(&mut output, project)
}

fn print_explanation(project: &EvaluatedProject, node: &str) -> io::Result<()> {
    let capability = project.capability(node).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown capability {node}"),
        )
    })?;
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
