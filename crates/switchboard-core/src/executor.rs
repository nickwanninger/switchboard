use crate::models::{Command, ExecutionUpdate, Host};
use crate::orchestration::orchestrate_execution;
use crate::run_environment::{LocalRunEnvironment, RunEnvironment, SshRunEnvironment};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecuteError {
    #[error("SSH error: {0}")]
    SshError(String),
    #[error("Connection failed")]
    ConnectionFailed,
}

pub trait CommandExecutor: Send + Sync {
    /// Execute a command and stream updates via the provided callback.
    /// The callback may be called from a different thread.
    fn execute(
        &self,
        exec_id: uuid::Uuid,
        command: &Command,
        host: &Host,
        env_vars: std::collections::HashMap<String, String>,
        on_update: Box<dyn Fn(ExecutionUpdate) + Send + Sync>,
        kill_rx: std::sync::mpsc::Receiver<()>,
    ) -> Result<(), ExecuteError>;
}

pub struct Executor;

impl CommandExecutor for Executor {
    fn execute(
        &self,
        exec_id: uuid::Uuid,
        command: &Command,
        host: &Host,
        env_vars: std::collections::HashMap<String, String>,
        on_update: Box<dyn Fn(ExecutionUpdate) + Send + Sync>,
        kill_rx: std::sync::mpsc::Receiver<()>,
    ) -> Result<(), ExecuteError> {
        let is_local = host.name.to_lowercase() == "local"
            || host.hostname.to_lowercase() == "localhost"
            || host.hostname == "127.0.0.1";

        let command = command.clone();
        let host = host.clone();

        std::thread::spawn(move || {
            on_update(ExecutionUpdate::Started(command.id));

            let env: Box<dyn RunEnvironment> = if is_local {
                Box::new(LocalRunEnvironment::new())
            } else {
                match SshRunEnvironment::connect(&host) {
                    Ok(e) => Box::new(e),
                    Err(e) => {
                        on_update(ExecutionUpdate::Stderr(format!("{}", e)));
                        on_update(ExecutionUpdate::Exit(-1));
                        return;
                    }
                }
            };

            if let Err(e) =
                orchestrate_execution(exec_id, env.as_ref(), &command, &host, env_vars, &*on_update, kill_rx)
            {
                on_update(ExecutionUpdate::Stderr(format!("Execution error: {}", e)));
                on_update(ExecutionUpdate::Exit(-1));
            }
        });

        Ok(())
    }
}
