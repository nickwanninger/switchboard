use crate::models::Command;
use crate::store::CommandStore;

/// Save a command to the database
/// This function now simply ensures the command is in the store
pub fn save_command(store: &CommandStore, command: &Command) {
    store.add_command(command.clone());
}
