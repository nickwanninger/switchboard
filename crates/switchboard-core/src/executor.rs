use crate::models::{Command, ExecutionUpdate, Host};
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
        command: &Command,
        host: &Host,
        on_update: Box<dyn Fn(ExecutionUpdate) + Send + Sync>,
        kill_rx: std::sync::mpsc::Receiver<()>,
    ) -> Result<(), ExecuteError>;
}

use ssh2::Session;
use std::io::Read;
use std::net::TcpStream;
use std::path::Path;

pub struct SshExecutor;

impl CommandExecutor for SshExecutor {
    fn execute(
        &self,
        command: &Command,
        host: &Host,
        on_update: Box<dyn Fn(ExecutionUpdate) + Send + Sync>,
        kill_rx: std::sync::mpsc::Receiver<()>,
    ) -> Result<(), ExecuteError> {
        // Check connectivity/auth briefly? No, might block.
        // Just spawn.

        let script = command.script.clone();
        let cmd_id = command.id;
        let host_clone = host.clone();
        let working_dir = command.working_directory.clone();

        std::thread::spawn(move || {
            on_update(ExecutionUpdate::Started(cmd_id));

            let host = host_clone;

            let tcp = match TcpStream::connect(format!("{}:{}", host.hostname, host.port)) {
                Ok(t) => t,
                Err(e) => {
                    on_update(ExecutionUpdate::Output(format!("Failed to connect: {}", e)));
                    on_update(ExecutionUpdate::Exit(-1));
                    return;
                }
            };

            let mut sess = Session::new().unwrap();
            sess.set_tcp_stream(tcp);
            if let Err(e) = sess.handshake() {
                on_update(ExecutionUpdate::Output(format!("Handshake failed: {}", e)));
                on_update(ExecutionUpdate::Exit(-1));
                return;
            }

            // Try authentication methods in order
            let mut auth_success = false;

            // 1. Try SSH agent first
            if let Ok(_) = sess.userauth_agent(&host.username) {
                if sess.authenticated() {
                    on_update(ExecutionUpdate::Output(format!(
                        "✓ Authenticated via SSH agent\n"
                    )));
                    auth_success = true;
                }
            }

            // 2. Try common SSH key types
            if !auth_success {
                let key_types = vec!["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"];

                if let Ok(home) = std::env::var("HOME") {
                    for key_name in key_types {
                        let key_path = Path::new(&home).join(".ssh").join(key_name);
                        if key_path.exists() {
                            if let Ok(_) =
                                sess.userauth_pubkey_file(&host.username, None, &key_path, None)
                            {
                                if sess.authenticated() {
                                    auth_success = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if !auth_success {
                on_update(ExecutionUpdate::Output(format!(
                    "❌ Authentication failed for user '{}'\n\nTroubleshooting:\n\
                    1. Verify your SSH keys are set up for {}\n\
                    2. Run 'ssh-add -l' to check if your key is loaded in the agent\n\
                    3. Try 'ssh {}@{}' manually to test the connection\n\
                    4. Check that your public key is in ~/.ssh/authorized_keys on the remote host\n",
                    host.username, host.hostname, host.username, host.hostname
                )));
                on_update(ExecutionUpdate::Exit(-1));
                return;
            }

            // Generate a unique temp file name
            let temp_script = format!("/tmp/switchboard_{}.sh", uuid::Uuid::new_v4());

            // 1. Upload script via SFTP
            let sftp = match sess.sftp() {
                Ok(s) => s,
                Err(e) => {
                    on_update(ExecutionUpdate::Output(format!(
                        "Failed to start SFTP: {}",
                        e
                    )));
                    on_update(ExecutionUpdate::Exit(-1));
                    return;
                }
            };

            let mut remote_file = match sftp.create(Path::new(&temp_script)) {
                Ok(f) => f,
                Err(e) => {
                    on_update(ExecutionUpdate::Output(format!(
                        "Failed to create temp file: {}",
                        e
                    )));
                    on_update(ExecutionUpdate::Exit(-1));
                    return;
                }
            };

            use std::io::Write;
            if let Err(e) = remote_file.write_all(script.as_bytes()) {
                on_update(ExecutionUpdate::Output(format!(
                    "Failed to write script: {}",
                    e
                )));
                on_update(ExecutionUpdate::Exit(-1));
                return;
            }

            drop(remote_file);
            drop(sftp);

            on_update(ExecutionUpdate::Output(format!(
                "Script uploaded to {}\n",
                temp_script
            )));

            // 2. Make it executable and run with bash
            // Explicitly use bash to avoid issues with non-standard default shells (e.g., fish)
            let work_dir = working_dir.as_deref().unwrap_or("/");

            // Source common profile files to load environment variables
            // This ensures things like PATH, custom env vars, etc. are available
            let source_profiles = "[ -f /etc/profile ] && . /etc/profile; [ -f ~/.bash_profile ] && . ~/.bash_profile; [ -f ~/.profile ] && . ~/.profile; [ -f ~/.bashrc ] && . ~/.bashrc";

            let inner_cmd = format!(
                "{} && chmod +x {} && cd {} && bash {} ; rm -f {}",
                source_profiles, temp_script, work_dir, temp_script, temp_script
            );
            // Wrap in explicit bash invocation to bypass user's default shell
            let exec_cmd = format!("/bin/bash -c '{}'", inner_cmd.replace("'", "'\\''"));

            let mut channel = match sess.channel_session() {
                Ok(c) => c,
                Err(e) => {
                    on_update(ExecutionUpdate::Output(format!(
                        "Failed to open channel: {}",
                        e
                    )));
                    on_update(ExecutionUpdate::Exit(-1));
                    return;
                }
            };

            // Request a pseudo-tty (equivalent to ssh -t)
            if let Err(e) = channel.request_pty("xterm", None, None) {
                on_update(ExecutionUpdate::Output(format!(
                    "Failed to request PTY: {}",
                    e
                )));
                on_update(ExecutionUpdate::Exit(-1));
                return;
            }

            if let Err(e) = channel.exec(&exec_cmd) {
                on_update(ExecutionUpdate::Output(format!("Failed to exec: {}", e)));
                on_update(ExecutionUpdate::Exit(-1));
                return;
            }

            // Stream output from both stdout and stderr
            let mut stdout_buffer = [0u8; 1024];
            let mut stderr_buffer = [0u8; 1024];
            loop {
                // Check for kill signal
                if let Ok(_) = kill_rx.try_recv() {
                    on_update(ExecutionUpdate::Output(
                        "\n[Killing execution...]\n".to_string(),
                    ));

                    // Try to interrupt the process by sending Ctrl+C (SIGINT)
                    // We write the interrupt character to stdin
                    let _ = channel.write_all(&[0x03]); // ASCII ETX (Ctrl+C)
                    let _ = channel.flush();

                    // Give it a moment to handle the signal
                    std::thread::sleep(std::time::Duration::from_millis(200));

                    // Now close the channel forcibly
                    let _ = channel.send_eof();
                    let _ = channel.close();

                    on_update(ExecutionUpdate::Output(
                        "[Execution terminated]\n".to_string(),
                    ));
                    on_update(ExecutionUpdate::Exit(-1));
                    return;
                }

                // Read stdout
                match channel.read(&mut stdout_buffer) {
                    Ok(n) if n > 0 => {
                        let s = String::from_utf8_lossy(&stdout_buffer[0..n]);
                        on_update(ExecutionUpdate::Output(s.to_string()));
                    }
                    _ => {}
                }

                // Read stderr
                match channel.stderr().read(&mut stderr_buffer) {
                    Ok(n) if n > 0 => {
                        let s = String::from_utf8_lossy(&stderr_buffer[0..n]);
                        on_update(ExecutionUpdate::Output(s.to_string()));
                    }
                    _ => {}
                }

                // Check if channel is EOF
                if channel.eof() {
                    break;
                }

                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            // Wait for exit status
            let _ = channel.wait_close();
            let code = channel.exit_status().unwrap_or(-1);
            on_update(ExecutionUpdate::Exit(code));
        });

        Ok(())
    }
}
