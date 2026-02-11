use crate::models::{Command, ExecutionUpdate, Host};
use crate::run_environment::{OutputChunk, RunEnvironment, RunEnvironmentError};
use std::collections::HashMap;

pub(crate) fn orchestrate_execution(
    exec_id: uuid::Uuid,
    env: &dyn RunEnvironment,
    command: &Command,
    _host: &Host,
    mut env_vars: HashMap<String, String>,
    on_update: &dyn Fn(ExecutionUpdate),
    kill_rx: std::sync::mpsc::Receiver<()>,
) -> Result<(), RunEnvironmentError> {
    let log_file = format!("/tmp/switchboard_{}.log", exec_id);
    let script_path = format!("/tmp/switchboard_{}.sh", exec_id);

    env_vars.insert("SWITCHBOARD_RUN".to_string(), exec_id.to_string());
    env_vars.insert("SWITCHBOARD_LOG".to_string(), log_file.clone());

    env.write_file(&script_path, command.script.as_bytes())?;

    let mut env_exports = String::new();
    for (key, val) in &env_vars {
        let escaped_val = val.replace('\'', "'\\''");
        env_exports.push_str(&format!("export {}='{}'; ", key, escaped_val));
    }

    let work_dir = command.working_directory.as_deref().unwrap_or("/");
    let inner_cmd = format!(
        "{}chmod +x {} && cd {} && bash -l {}; rm -f {}",
        env_exports, script_path, work_dir, script_path, script_path
    );

    let map_chunk = |chunk: OutputChunk| match chunk {
        OutputChunk::Stdout(s) => on_update(ExecutionUpdate::Stdout(s)),
        OutputChunk::Stderr(s) => on_update(ExecutionUpdate::Stderr(s)),
    };

    env.emit_preamble(&map_chunk, &log_file);

    if command.background {
        let exec_cmd = format!("nohup bash -c '{}' > {} 2>&1 &", inner_cmd, log_file);
        let handle = env.run_background(&exec_cmd)?;
        on_update(ExecutionUpdate::Stdout(format!(
            "Background process started: {}\n",
            handle.pid_or_hint
        )));
        on_update(ExecutionUpdate::Exit(0));
    } else {
        let escaped_inner = inner_cmd.replace('\'', "'\\''");
        let exec_cmd = format!("/bin/bash -c '{}' | tee {}", escaped_inner, log_file);
        let code = env.run(&exec_cmd, &map_chunk, &kill_rx)?;
        on_update(ExecutionUpdate::Exit(code));
    }

    Ok(())
}
