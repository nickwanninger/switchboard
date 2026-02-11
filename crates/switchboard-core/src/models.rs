use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
    pub ask_user: bool, // If true, prompt user at runtime
}

// --- Legacy Types for Migration ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandV0 {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub script: String,
    pub working_directory: Option<String>,
    pub environment: HashMap<String, String>,
    pub host: Option<String>,
    pub user: Option<String>,
    pub target_hosts: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowV0 {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub commands: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}
// ----------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    Password(String),
    KeyFile(String),
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: Uuid,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}

use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub script: String,
    pub working_directory: Option<String>,
    pub env_vars: Vec<EnvVar>,
    pub host: Option<String>,
    pub user: Option<String>,
    pub target_hosts: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub background: bool,
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub commands: Vec<Uuid>,
    pub env_vars: Vec<EnvVar>,
    pub created_at: DateTime<Utc>,
}

impl From<CommandV0> for Command {
    fn from(old: CommandV0) -> Self {
        let env_vars = old
            .environment
            .into_iter()
            .map(|(k, v)| EnvVar {
                key: k,
                value: v,
                ask_user: false,
            })
            .collect();

        Command {
            id: old.id,
            name: old.name,
            description: old.description,
            script: old.script,
            working_directory: old.working_directory,
            env_vars,
            host: old.host,
            user: old.user,
            target_hosts: old.target_hosts,
            created_at: old.created_at,
            background: false,
            source_path: old.source_path,
        }
    }
}

impl From<WorkflowV0> for Workflow {
    fn from(old: WorkflowV0) -> Self {
        Workflow {
            id: old.id,
            name: old.name,
            description: old.description,
            commands: old.commands,
            env_vars: Vec::new(),
            created_at: old.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionUpdate {
    Started(Uuid),
    Stdout(String),
    Stderr(String),
    Exit(i32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub id: Uuid,
    pub command_id: Uuid,
    pub host_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub stdout: String,
    pub stderr: String,
    pub status: ExecutionStatus,
}
