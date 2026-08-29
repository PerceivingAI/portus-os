use std::{ffi::OsString, path::PathBuf, process::Command};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub current_dir: PathBuf,
}

impl CommandSpec {
    #[must_use]
    pub fn new(program: impl Into<OsString>, current_dir: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            current_dir: current_dir.into(),
        }
    }

    #[must_use]
    pub fn with_args<I, T>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn program_display(&self) -> String {
        self.program.to_string_lossy().into_owned()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandOutcome {
    pub success: bool,
    pub code: Option<i32>,
}

pub trait CommandRunner {
    fn run(&mut self, spec: &CommandSpec) -> std::io::Result<CommandOutcome>;
}

#[derive(Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&mut self, spec: &CommandSpec) -> std::io::Result<CommandOutcome> {
        let status = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.current_dir)
            .status()?;
        Ok(CommandOutcome {
            success: status.success(),
            code: status.code(),
        })
    }
}
