use std::collections::HashMap;
use switchboard_core::CommandStore;
use switchboard_core::models::{Command, Workflow};
use uuid::Uuid;

#[test]
fn test_workflow_crud() {
    let store = CommandStore::new_test();

    // Create Workflow
    let wf_id = Uuid::new_v4();
    let wf = Workflow {
        id: wf_id,
        name: "Test Workflow".to_string(),
        description: Some("Description".into()),
        commands: vec![],
        created_at: chrono::Utc::now(),
    };

    store.add_workflow(wf.clone());

    // Get Workflow
    let fetched = store.get_workflow(&wf_id).unwrap();
    assert_eq!(fetched.id, wf_id);
    assert_eq!(fetched.name, "Test Workflow");

    // List Workflows
    let list = store.list_workflows();
    assert!(list.iter().any(|w| w.id == wf_id));

    // Remove Workflow
    store.remove_workflow(&wf_id);
    assert!(store.get_workflow(&wf_id).is_none());
}

#[test]
fn test_workflow_integrity() {
    let store = CommandStore::new_test();

    // Create Command
    let cmd_id = Uuid::new_v4();
    let cmd = Command {
        id: cmd_id,
        name: "Test Cmd".to_string(),
        description: None,
        script: "echo hi".to_string(),
        working_directory: None,
        environment: HashMap::new(),
        host: None,
        user: None,
        target_hosts: vec![],
        created_at: chrono::Utc::now(),
        source_path: None,
    };
    store.add_command(cmd);

    // Create Workflow with Command
    let wf_id = Uuid::new_v4();
    let wf = Workflow {
        id: wf_id,
        name: "Integrity Flow".to_string(),
        description: None,
        commands: vec![cmd_id],
        created_at: chrono::Utc::now(),
    };
    store.add_workflow(wf);

    // Check Integrity
    assert!(store.is_command_in_workflow(&cmd_id));

    // Remove Workflow
    store.remove_workflow(&wf_id);
    assert!(!store.is_command_in_workflow(&cmd_id));
}
