use super::{BackgroundHandle, OutputChunk, RunEnvironment, RunEnvironmentError};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;

pub struct LocalRunEnvironment;

impl LocalRunEnvironment {
    pub fn new() -> Self {
        LocalRunEnvironment
    }
}

impl RunEnvironment for LocalRunEnvironment {
    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), RunEnvironmentError> {
        std::fs::write(path, contents)?;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
        Ok(())
    }

    fn run(
        &self,
        command: &str,
        on_output: &dyn Fn(OutputChunk),
        kill_rx: &std::sync::mpsc::Receiver<()>,
    ) -> Result<i32, RunEnvironmentError> {
        let mut child = std::process::Command::new("/bin/bash")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdout = child.stdout.take().expect("Failed to open stdout");
        let mut stderr = child.stderr.take().expect("Failed to open stderr");

        let (out_tx, out_rx) = std::sync::mpsc::channel::<OutputChunk>();
        let out_tx_stderr = out_tx.clone();

        std::thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            loop {
                match stdout.read(&mut buffer) {
                    Ok(n) if n > 0 => {
                        let s = String::from_utf8_lossy(&buffer[0..n]).to_string();
                        let _ = out_tx.send(OutputChunk::Stdout(s));
                    }
                    _ => break,
                }
            }
        });

        std::thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            loop {
                match stderr.read(&mut buffer) {
                    Ok(n) if n > 0 => {
                        let s = String::from_utf8_lossy(&buffer[0..n]).to_string();
                        let _ = out_tx_stderr.send(OutputChunk::Stderr(s));
                    }
                    _ => break,
                }
            }
        });

        loop {
            if kill_rx.try_recv().is_ok() {
                on_output(OutputChunk::Stderr("\n[Killing execution...]\n".to_string()));
                let _ = child.kill();
                let _ = child.wait();
                return Ok(-1);
            }

            while let Ok(chunk) = out_rx.try_recv() {
                on_output(chunk);
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    // Drain any remaining output
                    while let Ok(chunk) = out_rx.try_recv() {
                        on_output(chunk);
                    }
                    return Ok(status.code().unwrap_or(-1));
                }
                Ok(None) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => return Ok(-1),
            }
        }
    }

    fn run_background(&self, command: &str) -> Result<BackgroundHandle, RunEnvironmentError> {
        let child = std::process::Command::new("nohup")
            .arg("/bin/bash")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()?;
        Ok(BackgroundHandle {
            pid_or_hint: child.id().to_string(),
        })
    }

    fn emit_preamble(&self, _on_output: &dyn Fn(OutputChunk), _log_file: &str) {}
}
