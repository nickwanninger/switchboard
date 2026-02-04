# Building Switchboard for macOS

## Quick Start

To build a standalone macOS application:

```bash
./build-macos-app.sh
```

This will:

1. Build the release binary with `cargo build --release`
2. Create the `.app` bundle structure
3. Generate an `Info.plist` with proper metadata
4. Copy the binary into the bundle

The resulting app will be located at: `target/macos/Switchboard.app`

## Running the App

### From the build directory:

```bash
open target/macos/Switchboard.app
```

### Install to Applications folder:

```bash
cp -r target/macos/Switchboard.app /Applications/
```

Then you can launch it from Spotlight or the Applications folder.

## App Structure

The `.app` bundle follows the standard macOS structure:

```
Switchboard.app/
├── Contents/
│   ├── Info.plist          # App metadata
│   ├── MacOS/
│   │   └── switchboard-ui  # Your binary
│   └── Resources/          # (empty for now, can add icons later)
```

## Adding an Icon (Optional)

To add a custom icon:

1. Create an `.icns` file (macOS icon format)
2. Place it in `target/macos/Switchboard.app/Contents/Resources/AppIcon.icns`
3. Add this to `Info.plist` in the `<dict>` section:
   ```xml
   <key>CFBundleIconFile</key>
   <string>AppIcon</string>
   ```

## Notes

- The app uses your system's SSH keys and configuration
- The database (`switchboard.db`) will be created in the directory where you run the app
- For a more portable version, you might want to use `~/Library/Application Support/Switchboard/` for the database

## Troubleshooting

**"App is damaged" error:**

```bash
xattr -cr target/macos/Switchboard.app
```

**App won't open (security):**
Right-click the app and select "Open", then confirm you want to open it.
