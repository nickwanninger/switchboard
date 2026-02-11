use crate::models::{Command, ExecutionResult, Host, Workflow};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Default, Serialize, Deserialize, Clone)]
struct StoreData {
    commands: Vec<Command>,
    workflows: Vec<Workflow>,
    hosts: Vec<Host>,
    #[serde(default)]
    executions: Vec<ExecutionResult>,
}

#[derive(Clone)]
pub struct CommandStore {
    path: PathBuf,
    data: Arc<RwLock<StoreData>>,
}

impl CommandStore {
    pub fn new() -> Self {
        use directories::ProjectDirs;

        // Get platform-specific data directory
        let db_path = if let Some(proj_dirs) = ProjectDirs::from("io", "nickw", "switchboard") {
            let data_dir = proj_dirs.data_dir();

            // Create directory if it doesn't exist
            if let Err(e) = std::fs::create_dir_all(data_dir) {
                eprintln!("Warning: Failed to create data directory: {}", e);
                eprintln!("Falling back to current directory");
                std::path::PathBuf::from("store.json")
            } else {
                data_dir.join("store.json")
            }
        } else {
            eprintln!("Warning: Could not determine data directory");
            eprintln!("Falling back to current directory");
            std::path::PathBuf::from("store.json")
        };

        println!("Using database at: {}", db_path.display());

        let store = Self {
            path: db_path,
            data: Arc::new(RwLock::new(StoreData::default())),
        };

        store.load();
        store
    }

    pub fn new_test() -> Self {
        // Use a temporary file
        let mut path = std::env::temp_dir();
        path.push(format!("switchboard_test_{}.json", Uuid::new_v4()));

        Self {
            path,
            data: Arc::new(RwLock::new(StoreData::default())),
        }
    }

    fn load(&self) {
        if self.path.exists() {
            match std::fs::read_to_string(&self.path) {
                Ok(content) => {
                    match serde_json::from_str::<StoreData>(&content) {
                        Ok(data) => {
                            *self.data.write().unwrap() = data;
                        }
                        Err(e) => {
                            eprintln!("Failed to parse store.json: {}", e);
                            // Backup corrupted file?
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read store.json: {}", e);
                }
            }
        }
    }

    fn save(&self) {
        println!("Saving store to: {}", self.path.display());
        let data = self.data.read().unwrap();
        match serde_json::to_string_pretty(&*data) {
            Ok(json) => {
                let mut temp_path = self.path.clone();
                temp_path.set_extension("json.tmp");

                let result = (|| -> std::io::Result<()> {
                    // 1. Write to temporary file
                    std::fs::write(&temp_path, json)?;

                    // 2. Open for syncing
                    let file = std::fs::File::open(&temp_path)?;
                    file.sync_all()?;

                    // 3. Atomic rename
                    std::fs::rename(&temp_path, &self.path)?;

                    // 4. Sync parent directory to ensure entry is persisted
                    if let Some(parent) = self.path.parent() {
                        if let Ok(dir) = std::fs::File::open(parent) {
                            let _ = dir.sync_all();
                        }
                    }
                    Ok(())
                })();

                if let Err(e) = result {
                    eprintln!("Critical Error: Failed to save store reliably: {}", e);
                    // attempt cleanup of temp file if it exists
                    let _ = std::fs::remove_file(temp_path);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize store data: {}", e);
            }
        }
    }

    // --- Command Methods ---

    pub fn add_command(&self, cmd: Command) -> Uuid {
        let id = cmd.id;
        {
            let mut data = self.data.write().unwrap();
            // Upsert: Remove existing if present
            data.commands.retain(|c| c.id != id);
            data.commands.push(cmd);
        }
        self.save();
        id
    }

    pub fn get_command(&self, id: &Uuid) -> Option<Command> {
        let data = self.data.read().unwrap();
        data.commands.iter().find(|c| c.id == *id).cloned()
    }

    pub fn list_commands(&self) -> Vec<Command> {
        let data = self.data.read().unwrap();
        data.commands.clone()
    }

    pub fn remove_command(&self, id: &Uuid) {
        {
            let mut data = self.data.write().unwrap();
            data.commands.retain(|c| c.id != *id);
        }
        self.save();
    }

    // --- Host Methods ---

    pub fn add_host(&self, host: Host) -> Uuid {
        let id = host.id;
        {
            let mut data = self.data.write().unwrap();
            data.hosts.retain(|h| h.id != id);
            data.hosts.push(host);
        }
        self.save();
        id
    }

    pub fn get_host(&self, id: &Uuid) -> Option<Host> {
        let data = self.data.read().unwrap();
        data.hosts.iter().find(|h| h.id == *id).cloned()
    }

    pub fn list_hosts(&self) -> Vec<Host> {
        let data = self.data.read().unwrap();
        data.hosts.clone()
    }

    // --- Workflow Methods ---

    pub fn add_workflow(&self, workflow: Workflow) -> Uuid {
        let id = workflow.id;
        {
            let mut data = self.data.write().unwrap();
            data.workflows.retain(|w| w.id != id);
            data.workflows.push(workflow);
        }
        self.save();
        id
    }

    pub fn get_workflow(&self, id: &Uuid) -> Option<Workflow> {
        let data = self.data.read().unwrap();
        data.workflows.iter().find(|w| w.id == *id).cloned()
    }

    pub fn list_workflows(&self) -> Vec<Workflow> {
        let data = self.data.read().unwrap();
        data.workflows.clone()
    }

    pub fn remove_workflow(&self, id: &Uuid) {
        {
            let mut data = self.data.write().unwrap();
            data.workflows.retain(|w| w.id != *id);
        }
        self.save();
    }

    pub fn is_command_in_workflow(&self, cmd_id: &Uuid) -> bool {
        let data = self.data.read().unwrap();
        data.workflows.iter().any(|w| w.commands.contains(cmd_id))
    }

    fn executions_dir(&self) -> PathBuf {
        self.path.parent().expect("store path has no parent").join("executions")
    }

    // --- Execution Methods ---

    pub fn add_execution(&self, result: &ExecutionResult, output: &str) {
        let exec_dir = self.executions_dir();
        if let Err(e) = std::fs::create_dir_all(&exec_dir) {
            eprintln!("Warning: Failed to create executions directory: {}", e);
        } else {
            let gz_path = exec_dir.join(&result.log_file);
            let write_result = (|| -> std::io::Result<()> {
                let file = std::fs::File::create(&gz_path)?;
                let mut encoder = GzEncoder::new(file, Compression::default());
                encoder.write_all(output.as_bytes())?;
                encoder.finish()?;
                Ok(())
            })();
            if let Err(e) = write_result {
                eprintln!("Warning: Failed to write execution log {}: {}", gz_path.display(), e);
            }
        }

        {
            let mut data = self.data.write().unwrap();
            data.executions.retain(|e| e.id != result.id);
            data.executions.push(result.clone());
        }
        self.save();
    }

    pub fn get_execution_history(&self, cmd_id: &Uuid) -> Vec<ExecutionResult> {
        let data = self.data.read().unwrap();
        data.executions
            .iter()
            .filter(|e| e.command_id == *cmd_id)
            .cloned()
            .collect()
    }

    pub fn get_execution_log(&self, exec_id: &Uuid) -> Option<String> {
        let log_file = {
            let data = self.data.read().unwrap();
            data.executions.iter().find(|e| e.id == *exec_id)?.log_file.clone()
        };

        let gz_path = self.executions_dir().join(&log_file);
        let read_result = (|| -> std::io::Result<String> {
            let file = std::fs::File::open(&gz_path)?;
            let mut decoder = GzDecoder::new(file);
            let mut content = String::new();
            decoder.read_to_string(&mut content)?;
            Ok(content)
        })();

        match read_result {
            Ok(content) => Some(content),
            Err(e) => {
                eprintln!("Warning: Failed to read execution log {}: {}", gz_path.display(), e);
                None
            }
        }
    }

    // --- Export/Import ---

    pub fn export_json(&self) -> anyhow::Result<String> {
        let data = self.data.read().unwrap();
        let json = serde_json::to_string_pretty(&*data)?;
        Ok(json)
    }

    pub fn import_json(&self, json: &str) -> anyhow::Result<()> {
        let new_data: StoreData = serde_json::from_str(json)?;
        {
            let mut data = self.data.write().unwrap();
            *data = new_data;
        }
        self.save();
        Ok(())
    }

    pub fn snapshot_state(&self) -> anyhow::Result<String> {
        let data = self.data.read().unwrap();
        let json = serde_json::to_string_pretty(&*data)?;

        // 1. Compute SHA-256 hash
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let hash_result = hasher.finalize();
        let hash_hex = hex::encode(hash_result);

        // 2. Determine snapshot directory and path
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("No parent directory for store path"))?;
        let snapshots_dir = parent.join("snapshots");
        std::fs::create_dir_all(&snapshots_dir)?;

        let snapshot_path = snapshots_dir.join(format!("{}.json.gz", hash_hex));

        // 3. Gzip the content
        let file = std::fs::File::create(&snapshot_path)?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(json.as_bytes())?;
        encoder.finish()?;

        Ok(hash_hex)
    }
}
