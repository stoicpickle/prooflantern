use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::Claim;

#[derive(Debug, Parser)]
#[command(
    name = "proof-lantern",
    version,
    about = "See what a project's core journey has decided, built, and proven",
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    /// Project directory containing .proof-lantern/project.yml.
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open the built-in Recipe Box prototype.
    Demo,
    /// Create a starter map, optionally with a guided interview; never overwrite.
    Init {
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,
        /// Ask about the user journey and preview it before saving (interactive terminal only).
        #[arg(long)]
        guided: bool,
    },
    /// Print the deterministic current focus without opening the TUI.
    Next {
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
    /// Explain one capability without opening the TUI.
    #[command(
        after_help = "Examples:\n  proof-lantern explain reopen .\n  proof-lantern explain save fixtures/recipe_box"
    )]
    Explain {
        /// Capability ID from .proof-lantern/project.yml (for example, reopen).
        #[arg(value_name = "NODE")]
        node: String,
        /// Project directory containing .proof-lantern/project.yml.
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
    /// Record one human-observed fact without rewriting project.yml.
    #[command(
        after_help = "Examples:\n  proof-lantern record reopen passed --summary \"The recipe appeared after reopening.\" .\n  proof-lantern record save failed --summary \"The saved recipe disappeared after restart.\" .\n  proof-lantern record reopen passed --summary \"I checked reopening.\" --input src/storage.rs --input tests/reopen.rs .\n\nReceipts capture recording-time context, not test execution. Without --input, code changes are not detected."
    )]
    Record {
        /// Capability ID from .proof-lantern/project.yml (for example, reopen).
        #[arg(value_name = "NODE")]
        node: String,
        /// What you observed: built, missing, passed, or failed.
        #[arg(value_name = "CLAIM")]
        claim: RecordClaim,
        /// A short description of what you observed.
        #[arg(long, value_name = "TEXT")]
        summary: String,
        /// File to fingerprint, relative to PATH; repeat for each input (no automatic scanning).
        #[arg(long = "input", value_name = "FILE")]
        inputs: Vec<String>,
        /// Project directory containing .proof-lantern/project.yml.
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum RecordClaim {
    /// Relevant implementation is present, but behavior is not yet proven.
    Built,
    /// Required implementation is explicitly absent.
    Missing,
    /// The observable proof passed.
    Passed,
    /// The observable proof failed.
    Failed,
}

impl From<RecordClaim> for Claim {
    fn from(value: RecordClaim) -> Self {
        match value {
            RecordClaim::Built => Self::ImplementationPresent,
            RecordClaim::Missing => Self::ImplementationAbsent,
            RecordClaim::Passed => Self::VerificationPassed,
            RecordClaim::Failed => Self::VerificationFailed,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Invocation {
    Project(PathBuf),
    Demo,
    Init(PathBuf),
    GuidedInit(PathBuf),
    Next(PathBuf),
    Explain {
        node: String,
        path: PathBuf,
    },
    Record {
        node: String,
        claim: RecordClaim,
        summary: String,
        inputs: Vec<String>,
        path: PathBuf,
    },
}

impl Cli {
    pub fn invocation(self) -> Invocation {
        match (self.command, self.path) {
            (Some(Command::Demo), _) => Invocation::Demo,
            (Some(Command::Init { path, guided: true }), _) => Invocation::GuidedInit(path),
            (
                Some(Command::Init {
                    path,
                    guided: false,
                }),
                _,
            ) => Invocation::Init(path),
            (Some(Command::Next { path }), _) => Invocation::Next(path),
            (Some(Command::Explain { node, path }), _) => Invocation::Explain { node, path },
            (
                Some(Command::Record {
                    node,
                    claim,
                    summary,
                    inputs,
                    path,
                }),
                _,
            ) => Invocation::Record {
                node,
                claim,
                summary,
                inputs,
                path,
            },
            (None, path) => Invocation::Project(path.unwrap_or_else(|| ".".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_surface_keeps_tui_and_plain_output_distinct() {
        assert_eq!(
            Cli::try_parse_from(["proof-lantern"]).unwrap().invocation(),
            Invocation::Project(".".into())
        );
        assert_eq!(
            Cli::try_parse_from(["proof-lantern", "demo"])
                .unwrap()
                .invocation(),
            Invocation::Demo
        );
        assert_eq!(
            Cli::try_parse_from(["proof-lantern", "init", "my-project"])
                .unwrap()
                .invocation(),
            Invocation::Init("my-project".into())
        );
        assert_eq!(
            Cli::try_parse_from(["proof-lantern", "next", "fixtures/recipe_box"])
                .unwrap()
                .invocation(),
            Invocation::Next("fixtures/recipe_box".into())
        );
        assert_eq!(
            Cli::try_parse_from([
                "proof-lantern",
                "record",
                "save",
                "passed",
                "--summary",
                "Save and reopen worked",
                "fixtures/recipe_box",
            ])
            .unwrap()
            .invocation(),
            Invocation::Record {
                node: "save".into(),
                claim: RecordClaim::Passed,
                summary: "Save and reopen worked".into(),
                inputs: Vec::new(),
                path: "fixtures/recipe_box".into(),
            }
        );
    }

    #[test]
    fn guided_setup_is_opt_in_and_accepts_either_flag_position() {
        for arguments in [
            vec!["proof-lantern", "init", "--guided", "my-project"],
            vec!["proof-lantern", "init", "my-project", "--guided"],
        ] {
            assert_eq!(
                Cli::try_parse_from(arguments).unwrap().invocation(),
                Invocation::GuidedInit("my-project".into())
            );
        }
        assert_eq!(
            Cli::try_parse_from(["proof-lantern", "init", "--guided"])
                .unwrap()
                .invocation(),
            Invocation::GuidedInit(".".into())
        );
        assert_eq!(
            Cli::try_parse_from(["proof-lantern", "init"])
                .unwrap()
                .invocation(),
            Invocation::Init(".".into())
        );
    }
}
