use crate::models::{Command, Host, Workflow};
use uuid::Uuid;

pub struct CommandStore {
    db: sled::Db,
}

impl CommandStore {
    pub fn new() -> Self {
        use directories::ProjectDirs;

        // Get platform-specific data directory
        // macOS: ~/Library/Application Support/com.switchboard.app
        // Linux: ~/.local/share/switchboard
        // Windows: %APPDATA%\switchboard
        let db_path = if let Some(proj_dirs) = ProjectDirs::from("io", "nickw", "switchboard") {
            let data_dir = proj_dirs.data_dir();

            // Create directory if it doesn't exist
            if let Err(e) = std::fs::create_dir_all(data_dir) {
                eprintln!("Warning: Failed to create data directory: {}", e);
                eprintln!("Falling back to current directory");
                std::path::PathBuf::from("switchboard.db")
            } else {
                data_dir.join("switchboard.db")
            }
        } else {
            eprintln!("Warning: Could not determine data directory");
            eprintln!("Falling back to current directory");
            std::path::PathBuf::from("switchboard.db")
        };

        println!("Using database at: {}", db_path.display());

        let db = sled::open(&db_path).expect("Failed to open database");
        Self { db }
    }

    pub fn new_test() -> Self {
        let db = sled::Config::new().temporary(true).open().unwrap();
        Self { db }
    }

    pub fn add_command(&self, cmd: Command) -> Uuid {
        let id = cmd.id;
        let key = format!("cmd:{}", id);
        let value = bincode::serialize(&cmd).expect("Failed to serialize command");
        self.db
            .insert(key.as_bytes(), value)
            .expect("Failed to insert command");
        id
    }

    pub fn get_command(&self, id: &Uuid) -> Option<Command> {
        let key = format!("cmd:{}", id);
        self.db
            .get(key.as_bytes())
            .ok()
            .flatten()
            .and_then(|bytes| bincode::deserialize(&bytes).ok())
    }

    pub fn list_commands(&self) -> Vec<Command> {
        let prefix = b"cmd:";
        self.db
            .scan_prefix(prefix)
            .filter_map(|result| result.ok())
            .filter_map(|(_, value)| bincode::deserialize(&value).ok())
            .collect()
    }

    pub fn remove_command(&self, id: &Uuid) {
        let key = format!("cmd:{}", id);
        let _ = self.db.remove(key.as_bytes());
    }

    pub fn add_host(&self, host: Host) -> Uuid {
        let id = host.id;
        let key = format!("host:{}", id);
        let value = bincode::serialize(&host).expect("Failed to serialize host");
        self.db
            .insert(key.as_bytes(), value)
            .expect("Failed to insert host");
        id
    }

    pub fn get_host(&self, id: &Uuid) -> Option<Host> {
        let key = format!("host:{}", id);
        self.db
            .get(key.as_bytes())
            .ok()
            .flatten()
            .and_then(|bytes| bincode::deserialize(&bytes).ok())
    }

    pub fn list_hosts(&self) -> Vec<Host> {
        let prefix = b"host:";
        self.db
            .scan_prefix(prefix)
            .filter_map(|result| result.ok())
            .filter_map(|(_, value)| bincode::deserialize(&value).ok())
            .collect()
    }

    pub fn add_workflow(&self, workflow: Workflow) -> Uuid {
        let id = workflow.id;
        let key = format!("workflow:{}", id);
        let value = bincode::serialize(&workflow).expect("Failed to serialize workflow");
        self.db
            .insert(key.as_bytes(), value)
            .expect("Failed to insert workflow");
        id
    }

    pub fn get_workflow(&self, id: &Uuid) -> Option<Workflow> {
        let key = format!("workflow:{}", id);
        self.db
            .get(key.as_bytes())
            .ok()
            .flatten()
            .and_then(|bytes| bincode::deserialize(&bytes).ok())
    }

    pub fn list_workflows(&self) -> Vec<Workflow> {
        let prefix = b"workflow:";
        self.db
            .scan_prefix(prefix)
            .filter_map(|result| result.ok())
            .filter_map(|(_, value)| bincode::deserialize(&value).ok())
            .collect()
    }

    pub fn remove_workflow(&self, id: &Uuid) {
        let key = format!("workflow:{}", id);
        let _ = self.db.remove(key.as_bytes());
    }

    pub fn is_command_in_workflow(&self, cmd_id: &Uuid) -> bool {
        self.list_workflows()
            .iter()
            .any(|w| w.commands.contains(cmd_id))
    }

    pub fn add_execution(&self, result: &crate::models::ExecutionResult) {
        // 1. Store Metadata (Lite)
        let meta_key = format!("exec_meta:{}:{}", result.command_id, result.id);

        let mut meta = result.clone();
        // Clear heavy fields from metadata
        meta.stdout = String::new();
        meta.stderr = String::new();

        let meta_value = bincode::serialize(&meta).expect("Failed to serialize execution metadata");
        self.db
            .insert(meta_key.as_bytes(), meta_value)
            .expect("Failed to insert execution metadata");

        // 2. Store Logs (Compressed)
        let log_key = format!("exec_log:{}", result.id);
        let output = format!("STDOUT:\n{}\n\nSTDERR:\n{}", result.stdout, result.stderr);

        // Zstd compress
        let compressed =
            zstd::stream::encode_all(output.as_bytes(), 0).expect("Failed to compress logs");
        self.db
            .insert(log_key.as_bytes(), compressed)
            .expect("Failed to insert logs");
    }

    pub fn get_execution_history(&self, cmd_id: &Uuid) -> Vec<crate::models::ExecutionResult> {
        let prefix = format!("exec_meta:{}:", cmd_id);
        self.db
            .scan_prefix(prefix.as_bytes())
            .filter_map(|result| result.ok())
            .filter_map(|(_, value)| bincode::deserialize(&value).ok())
            .collect()
    }

    pub fn get_execution_log(&self, exec_id: &Uuid) -> Option<String> {
        let key = format!("exec_log:{}", exec_id);
        self.db.get(key.as_bytes()).ok().flatten().map(|bytes| {
            let decoded = zstd::stream::decode_all(&bytes[..])
                .unwrap_or_else(|_| b"<failed to decompress>".to_vec());
            String::from_utf8_lossy(&decoded).to_string()
        })
    }
}
