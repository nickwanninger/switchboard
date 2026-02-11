#[cfg(test)]
mod tests {
    use crate::models::{Command, ExecutionResult, ExecutionStatus, Host, Workflow};
    use crate::store::CommandStore;
    use uuid::Uuid;

    fn make_exec(cmd_id: Uuid, host_id: Uuid) -> (Uuid, ExecutionResult) {
        let exec_id = Uuid::new_v4();
        let exec = ExecutionResult {
            id: exec_id,
            command_id: cmd_id,
            host_id,
            started_at: chrono::Utc::now(),
            finished_at: Some(chrono::Utc::now()),
            exit_code: Some(0),
            duration_ms: Some(100),
            status: ExecutionStatus::Completed,
            log_file: format!("{}.log.gz", exec_id),
        };
        (exec_id, exec)
    }

    #[test]
    fn test_execution_log_write_read() {
        let store = CommandStore::new_test();

        let cmd_id = Uuid::new_v4();
        let host_id = Uuid::new_v4();
        let (exec_id, exec) = make_exec(cmd_id, host_id);

        store.add_execution(&exec, "STDOUT_CONTENT\nSTDERR_CONTENT");

        let log = store.get_execution_log(&exec_id).expect("Log missing");
        assert!(log.contains("STDOUT_CONTENT"));
        assert!(log.contains("STDERR_CONTENT"));
    }

    #[test]
    fn test_export_import_cycle() {
        let store = CommandStore::new_test();

        // 1. Create Data
        let cmd = Command {
            id: Uuid::new_v4(),
            name: "Test Command".into(),
            description: None,
            script: "echo hello".into(),
            working_directory: None,
            env_vars: vec![],
            host: None,
            user: None,
            target_hosts: vec![],
            created_at: chrono::Utc::now(),
            background: false,
            source_path: None,
        };
        store.add_command(cmd.clone());

        let host = Host {
            id: Uuid::new_v4(),
            name: "Test Host".into(),
            hostname: "localhost".into(),
            port: 22,
            username: "user".into(),
            auth: crate::models::AuthMethod::Agent,
        };
        store.add_host(host.clone());

        let wf = Workflow {
            id: Uuid::new_v4(),
            name: "Test Workflow".into(),
            description: None,
            commands: vec![cmd.id],
            env_vars: vec![],
            created_at: chrono::Utc::now(),
        };
        store.add_workflow(wf.clone());

        let (_, exec) = make_exec(cmd.id, host.id);
        store.add_execution(&exec, "STDOUT_CONTENT\nSTDERR_CONTENT");

        // 2. Export
        let json = store.export_json().expect("Export failed");

        // Setup fresh store and import
        let store2 = CommandStore::new_test();
        store2.import_json(&json).expect("Import failed");

        // 3. Verify metadata round-trips
        let cmds = store2.list_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].id, cmd.id);

        let hosts = store2.list_hosts();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].id, host.id);

        let wfs = store2.list_workflows();
        assert_eq!(wfs.len(), 1);
        assert_eq!(wfs[0].id, wf.id);

        let history = store2.get_execution_history(&cmd.id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, exec.id);
        // Log file is in store's executions dir, not store2's, so we only check metadata here.
    }
}
