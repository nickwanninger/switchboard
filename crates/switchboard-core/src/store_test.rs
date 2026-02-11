#[cfg(test)]
mod tests {
    use crate::models::{Command, ExecutionResult, ExecutionStatus, Host, Workflow};
    use crate::store::CommandStore;
    use uuid::Uuid;

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

        let exec = ExecutionResult {
            id: Uuid::new_v4(),
            command_id: cmd.id,
            host_id: host.id,
            started_at: chrono::Utc::now(),
            finished_at: Some(chrono::Utc::now()),
            exit_code: Some(0),
            duration_ms: Some(100),
            stdout: "STDOUT_CONTENT".into(),
            stderr: "STDERR_CONTENT".into(),
            status: ExecutionStatus::Completed,
        };
        store.add_execution(&exec);

        // 2. Export
        let json = store.export_json().expect("Export failed");

        // Setup fresh store (or clear existing one, import_json clears it)
        let store2 = CommandStore::new_test();
        store2.import_json(&json).expect("Import failed");

        // 3. Verify
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

        // Verify Logs
        let log = store2.get_execution_log(&exec.id).expect("Log missing");
        assert!(log.contains("STDOUT_CONTENT"));
        assert!(log.contains("STDERR_CONTENT"));
    }
}
