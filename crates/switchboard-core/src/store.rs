use crate::models::{Command, Host};
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
}
