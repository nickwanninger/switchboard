# 🔌 Switchboard

A GUI application for managing and executing bash scripts on remote servers via SSH.

## ⚠️ Important Disclaimer

**This app was entirely vibe-coded as an AI pair programming experiment.** It works surprisingly well, but here's what you should know:

- 🎲 **Experimental**: This is a proof-of-concept, not production-ready software
- 🐛 **Bug hunting encouraged**: If you find issues (you probably will), they're features awaiting discovery
- 💾 **Backup your data**: The database is reliable, but this is experimental software
- 🔒 **Security**: It uses your SSH keys and runs bash scripts on remote machines - use at your own risk
- 🏗️ **Actively evolving**: Built in a single session, so expect rough edges

**TL;DR**: Fun for personal use and experimentation. Maybe don't deploy nuclear launch codes with it.

---

## What is Switchboard?

Switchboard is a desktop app that lets you:

- ✍️ Write and edit bash scripts in a clean UI
- 🖥️ Execute them on remote servers via SSH
- 👀 Watch output in real-time
- 📦 Organize commands in a database
- 🔪 Kill running executions
- 📋 Duplicate commands
- 🎯 Set working directories per command

Think of it as a persistent SSH script manager with a friendly interface.

## Features

### Script Management

- **Editor**: Full-height script editor with syntax highlighting
- **Metadata**: Name, description, host, user, working directory
- **Auto-save**: Changes save immediately to database
- **Duplicate**: Clone commands for variations

### Remote Execution

- **SSH Integration**: Uses your existing SSH keys and agent
- **Real-time Output**: See stdout and stderr as scripts run
- **Process Control**: Kill long-running or stuck commands
- **Environment Loading**: Sources profile files for proper PATH and env vars
- **PTY Support**: Runs with a pseudo-terminal for better compatibility

### Data Storage

- **Sled Database**: Embedded database for reliability
- **Platform-native paths**: Stores data in OS-appropriate locations
  - macOS: `~/Library/Application Support/com.switchboard.app/`
  - Linux: `~/.local/share/switchboard/`
  - Windows: `%APPDATA%\switchboard\`

## Installation

### Prerequisites

- Rust toolchain (1.70+)
- SSH keys set up for your remote hosts

### Build from Source

```bash
# Clone the repo
git clone <your-repo-url>
cd switchboard

# Build and run
cargo run --release

# Or build a macOS app bundle
./build-macos-app.sh
```

## Usage

### Creating Commands

1. Click **➕** in the sidebar
2. Fill in:
   - **Name**: What you're doing (e.g., "Deploy API")
   - **Description**: Optional details
   - **Host**: Remote hostname or IP (leave empty for localhost)
   - **User**: SSH username (defaults to current user)
   - **Working Dir**: Directory to run from (defaults to `/`)
3. Write your bash script
4. Changes auto-save

### Running Commands

1. Select a command from the sidebar
2. Click **▶ Run** in the action bar
3. Watch real-time output
4. Click **⏹ Kill** if needed

### Managing Commands

- **📋 Duplicate**: Create a copy to modify
- **🗑 Delete**: Remove a command (with confirmation)
- **Auto-save**: All changes save immediately

## Technical Details

### Architecture

```
switchboard/
├── crates/
│   ├── switchboard-core/    # Business logic
│   │   ├── executor.rs      # SSH execution
│   │   ├── models.rs        # Data structures
│   │   └── store.rs         # Database layer
│   └── switchboard-ui/      # GUI application
│       └── app.rs           # egui interface
```

### Tech Stack

- **GUI**: [egui](https://github.com/emilk/egui) / eframe
- **Database**: [sled](https://github.com/spacejam/sled) (embedded key-value store)
- **SSH**: [ssh2-rs](https://github.com/alexcrichton/ssh2-rs)
- **Serialization**: bincode

### How It Works

1. **Script Upload**: Scripts are uploaded to `/tmp/switchboard_<uuid>.sh` via SFTP
2. **Environment Setup**: Sources common profile files (`.bash_profile`, `.bashrc`, etc.)
3. **Execution**: Runs with `bash` in explicit PTY mode
4. **Output Streaming**: Both stdout and stderr are streamed back
5. **Cleanup**: Temp file is removed after execution

### SSH Authentication

Switchboard tries authentication methods in order:

1. SSH agent (most common)
2. Common key types in `~/.ssh/` (`id_ed25519`, `id_ecdsa`, `id_rsa`, `id_dsa`)

No passwords - key-based auth only.

## Troubleshooting

### "Authentication failed"

- Ensure your SSH keys are loaded: `ssh-add -l`
- Try connecting manually: `ssh user@host`
- Check that your public key is in `~/.ssh/authorized_keys` on the remote host

### "App is damaged" (macOS)

```bash
xattr -cr target/macos/Switchboard.app
```

### Scripts don't see environment variables

The app now sources profile files, but if you need custom setup:

- Add to `~/.bash_profile` on the remote host
- Or explicitly source files in your script

### Database location

Check the console output when starting - it prints:

```
Using database at: <path>
```

## Development

```bash
# Run in dev mode
cargo run

# Build release
cargo build --release

# Build macOS app
./build-macos-app.sh

# The app will be at target/macos/Switchboard.app
```

## License

MIT (probably - this was vibe-coded, we didn't get to licensing)

## Credits

Built entirely through AI pair programming as an experiment in rapid prototyping. It's a testament to:

- The power of modern Rust tooling
- egui's simplicity
- The questionable decision-making that leads to coding at 2 AM

## Contributing

This was a vibe session, but PRs welcome! Just know what you're getting into. 😅

---

**Remember**: Working software > perfect software. This definitely falls into the "working" category. Your mileage may vary! 🚀
