use std::{
    error::Error,
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use clap::Parser;
use proof_lantern::{
    App, EvaluatedProject,
    cli::{Cli, Invocation},
    evaluate, load_project,
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
        Invocation::Demo => load_root(demo_root())?,
        Invocation::Project(path) => load_root(path)?,
        Invocation::Next(_) | Invocation::Explain { .. } => unreachable!(),
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
    let config = root.as_ref().join(".proof-lantern");
    let (spec, observations) =
        load_project(config.join("project.yml"), config.join("observations.json"))?;
    Ok(evaluate(spec, observations)?)
}

fn demo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box")
}

fn print_next(project: &EvaluatedProject) -> io::Result<()> {
    let mut output = io::stdout().lock();
    let Some(gap) = &project.keystone else {
        return writeln!(output, "No unresolved core capability.");
    };
    let capability = project
        .capability(&gap.capability_id)
        .expect("evaluated gap must reference a capability");
    writeln!(output, "KEYSTONE GAP")?;
    writeln!(
        output,
        "{} {} — {}",
        capability.display.glyph(),
        capability.intent.label,
        capability.display.label()
    )?;
    writeln!(output, "{}", project.gap_impact(gap))?;
    writeln!(output, "Proof needed: {}", capability.intent.proof_needed)
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
            let freshness = if reason.fact.freshness == proof_lantern::Freshness::Stale {
                "[STALE] "
            } else {
                ""
            };
            writeln!(output, "  - {freshness}{}", reason.fact.summary)?;
        }
    }
    writeln!(output, "Proof needed: {}", capability.intent.proof_needed)
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
