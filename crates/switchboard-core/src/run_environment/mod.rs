pub mod local;
pub mod ssh;

pub use local::LocalRunEnvironment;
pub use ssh::SshRunEnvironment;

use thiserror::Error;

pub enum OutputChunk {
    Stdout(String),
    Stderr(String),
}

pub struct BackgroundHandle {
    pub pid_or_hint: String,
}

#[derive(Error, Debug)]
pub enum RunEnvironmentError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SSH error: {0}")]
    Ssh(String),
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("Upload failed: {0}")]
    UploadFailed(String),
}

pub trait RunEnvironment: Send {
    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), RunEnvironmentError>;

    fn run(
        &self,
        command: &str,
        on_output: &dyn Fn(OutputChunk),
        kill_rx: &std::sync::mpsc::Receiver<()>,
    ) -> Result<i32, RunEnvironmentError>;

    fn run_background(&self, command: &str) -> Result<BackgroundHandle, RunEnvironmentError>;

    fn emit_preamble(&self, on_output: &dyn Fn(OutputChunk), log_file: &str);
}
