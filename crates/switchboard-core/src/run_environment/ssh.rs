use super::{BackgroundHandle, OutputChunk, RunEnvironment, RunEnvironmentError};
use crate::models::Host;
use ssh2::Session;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;

pub struct SshRunEnvironment {
    sess: Session,
    host: Host,
}

impl SshRunEnvironment {
    pub fn connect(host: &Host) -> Result<Self, RunEnvironmentError> {
        let tcp = TcpStream::connect(format!("{}:{}", host.hostname, host.port))
            .map_err(|e| RunEnvironmentError::ConnectionFailed(e.to_string()))?;

        let mut sess = Session::new().map_err(|e| RunEnvironmentError::Ssh(e.to_string()))?;
        sess.set_tcp_stream(tcp);
        sess.handshake()
            .map_err(|e| RunEnvironmentError::ConnectionFailed(e.to_string()))?;

        let mut auth_success = false;

        if sess.userauth_agent(&host.username).is_ok() && sess.authenticated() {
            auth_success = true;
        }

        if !auth_success {
            let key_types = ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"];
            if let Ok(home) = std::env::var("HOME") {
                for key_name in key_types {
                    let key_path = Path::new(&home).join(".ssh").join(key_name);
                    if key_path.exists() {
                        if sess
                            .userauth_pubkey_file(&host.username, None, &key_path, None)
                            .is_ok()
                            && sess.authenticated()
                        {
                            auth_success = true;
                            break;
                        }
                    }
                }
            }
        }

        if !auth_success {
            return Err(RunEnvironmentError::AuthFailed(format!(
                "Authentication failed for user '{}' on {}\n\nTroubleshooting:\n\
                1. Verify your SSH keys are set up for {}\n\
                2. Run 'ssh-add -l' to check if your key is loaded in the agent\n\
                3. Try 'ssh {}@{}' manually to test the connection\n\
                4. Check that your public key is in ~/.ssh/authorized_keys on the remote host",
                host.username, host.hostname, host.hostname, host.username, host.hostname
            )));
        }

        Ok(SshRunEnvironment {
            sess,
            host: host.clone(),
        })
    }
}

impl RunEnvironment for SshRunEnvironment {
    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), RunEnvironmentError> {
        let sftp = self
            .sess
            .sftp()
            .map_err(|e| RunEnvironmentError::UploadFailed(e.to_string()))?;

        let mut remote_file = sftp
            .create(Path::new(path))
            .map_err(|e| RunEnvironmentError::UploadFailed(e.to_string()))?;

        remote_file
            .write_all(contents)
            .map_err(|e| RunEnvironmentError::UploadFailed(e.to_string()))?;

        Ok(())
    }

    fn run(
        &self,
        command: &str,
        on_output: &dyn Fn(OutputChunk),
        kill_rx: &std::sync::mpsc::Receiver<()>,
    ) -> Result<i32, RunEnvironmentError> {
        let mut channel = self
            .sess
            .channel_session()
            .map_err(|e| RunEnvironmentError::Ssh(e.to_string()))?;

        channel
            .exec(command)
            .map_err(|e| RunEnvironmentError::Ssh(e.to_string()))?;

        let mut stdout_buffer = [0u8; 1024];
        let mut stderr_buffer = [0u8; 1024];

        loop {
            if kill_rx.try_recv().is_ok() {
                on_output(OutputChunk::Stderr(
                    "\n[Killing execution...]\n".to_string(),
                ));
                let _ = channel.write_all(&[0x03]);
                let _ = channel.flush();
                std::thread::sleep(std::time::Duration::from_millis(200));
                let _ = channel.send_eof();
                let _ = channel.close();
                on_output(OutputChunk::Stderr("[Execution terminated]\n".to_string()));
                return Ok(-1);
            }

            match channel.read(&mut stdout_buffer) {
                Ok(n) if n > 0 => {
                    let s = String::from_utf8_lossy(&stdout_buffer[0..n]).to_string();
                    on_output(OutputChunk::Stdout(s));
                }
                _ => {}
            }

            match channel.stderr().read(&mut stderr_buffer) {
                Ok(n) if n > 0 => {
                    let s = String::from_utf8_lossy(&stderr_buffer[0..n]).to_string();
                    on_output(OutputChunk::Stderr(s));
                }
                _ => {}
            }

            if channel.eof() {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let _ = channel.wait_close();
        Ok(channel.exit_status().unwrap_or(-1))
    }

    fn run_background(&self, command: &str) -> Result<BackgroundHandle, RunEnvironmentError> {
        let mut channel = self
            .sess
            .channel_session()
            .map_err(|e| RunEnvironmentError::Ssh(e.to_string()))?;

        channel
            .exec(command)
            .map_err(|e| RunEnvironmentError::Ssh(e.to_string()))?;

        let _ = channel.send_eof();
        let _ = channel.close();

        Ok(BackgroundHandle {
            pid_or_hint: "remote background process".to_string(),
        })
    }

    fn emit_preamble(&self, on_output: &dyn Fn(OutputChunk), log_file: &str) {
        on_output(OutputChunk::Stdout(
            "Logging to /tmp. Tail it with the following command:\n".to_string(),
        ));
        on_output(OutputChunk::Stdout(format!(
            "\n\nssh {}@{} -- tail -f {}\n\n----------------\n",
            self.host.username, self.host.hostname, log_file
        )));
    }
}
