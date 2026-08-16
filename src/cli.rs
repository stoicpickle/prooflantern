use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
    /// Create a commented starter map without overwriting existing work.
    Init {
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
    /// Print the deterministic current focus without opening the TUI.
    Next {
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
    /// Explain one capability without opening the TUI.
    Explain {
        #[arg(value_name = "NODE")]
        node: String,
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub enum Invocation {
    Project(PathBuf),
    Demo,
    Init(PathBuf),
    Next(PathBuf),
    Explain { node: String, path: PathBuf },
}

impl Cli {
    pub fn invocation(self) -> Invocation {
        match (self.command, self.path) {
            (Some(Command::Demo), _) => Invocation::Demo,
            (Some(Command::Init { path }), _) => Invocation::Init(path),
            (Some(Command::Next { path }), _) => Invocation::Next(path),
            (Some(Command::Explain { node, path }), _) => Invocation::Explain { node, path },
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
    }
}
