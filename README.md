# Threadbox

[![Checks](https://github.com/vespy-pg/threadbox/actions/workflows/checks.yml/badge.svg)](https://github.com/vespy-pg/threadbox/actions/workflows/checks.yml)
[![MIT License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

Threadbox is a local-first task inbox for work that arrives through conversations. Capture a Slack message, email, WhatsApp message, screenshot, voice note, or thought, then keep the original context beside the task.

Release 1 targets Linux and Firefox. The architecture keeps the desktop core, browser integration, and website adapters separate so Windows, macOS, and Chromium browsers can be added without rewriting the product.

Linux release packages are built on Ubuntu 22.04 to remain compatible with Ubuntu 22.04 and newer distributions.

## Install Threadbox on Ubuntu or Debian

No developer tools are required.

To download and install Threadbox with one command, paste this into a terminal. The downloaded package is removed automatically after installation:

```bash
(package_file=$(mktemp --suffix=.deb) && trap 'rm -f "$package_file"' EXIT && wget -qO "$package_file" https://github.com/vespy-pg/threadbox/releases/download/v0.1.16/Threadbox_0.1.16_amd64.deb && sudo apt install "$package_file")
```

Alternatively, install it through the graphical interface:

1. Open the [latest Threadbox release](https://github.com/vespy-pg/threadbox/releases/latest).
2. Under **Assets**, download `Threadbox_0.1.16_amd64.deb`.
3. Open the downloaded file and select **Install** in the system software window.
4. Open **Threadbox** from the applications menu. It can start automatically after future logins if that option remains enabled in Settings.

If double-clicking the package does not open an installer, open a terminal in the Downloads folder and run:

```bash
sudo apt install ./Threadbox_0.1.16_amd64.deb
```

The Firefox extension is optional. The desktop app works on its own. To capture Slack, Gmail and WhatsApp messages from Firefox, download the signed `.xpi` file from the same release and open it with Firefox after Threadbox has been started once.

### Other Linux distributions

Download `Threadbox_0.1.16_amd64.AppImage`, make it executable in the file Properties window, then open it. From a terminal, the equivalent commands are:

```bash
chmod +x Threadbox_0.1.16_amd64.AppImage
./Threadbox_0.1.16_amd64.AppImage
```

## Features

- Fast task capture with a configurable global shortcut, defaulting to `Ctrl+Shift+Space`
- High, mid, and low priorities with a combined priority-sorted Inbox
- Completed and Deleted views with configurable retention, task restoration, and automatic media cleanup
- Calendar and clock-based due date selection
- Configurable native audible overdue reminders, enabled every 15 minutes by default
- Multiple screenshot attachments through paste or the interactive system screenshot capture
- Retained local voice recordings with playback on each task
- In-app reminder center with open, snooze and complete actions
- Local Polish speech recognition for task titles and notes with whisper.cpp through `whisper-rs`
- Firefox capture buttons for Slack, Gmail, and WhatsApp Web
- Universal Firefox context-menu capture
- Local SQLite metadata with file-based media storage and complete ZIP backup export
- Single-instance desktop behavior that restores the existing window when launched again
- Configurable login autostart that stays hidden in the system tray
- No account, analytics, or cloud service

## Repository layout

- `src/` - React desktop interface
- `src-tauri/` - Rust application core, SQLite database, speech recognition, and native messaging host
- `extension/` - Firefox WebExtension and website adapters
- `scripts/` - build and development utilities

## Development prerequisites

- Node.js 20 or newer
- Rust stable
- Tauri 2 Linux system dependencies

Install JavaScript dependencies and run the desktop app:

```bash
npm install
npm run tauri dev
```

Run checks:

```bash
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run extension:build
npm run extension:lint
```

## Firefox extension development

Build the extension:

```bash
npm run extension:build
```

Open `about:debugging#/runtime/this-firefox`, select **Load Temporary Add-on**, and choose `dist/extension/firefox/manifest.json`.

Start Threadbox once before testing the extension. The desktop application registers its native messaging host for the current Firefox user on Linux.

## Local speech model

Open **Settings** in Threadbox and select **Download model**. Threadbox downloads the multilingual `ggml-small.bin` model once and stores it in the application data directory. The desktop app records from the system audio input through CPAL, converts it to a 16 kHz mono WAV, processes it locally and retains it as a playable task attachment.

## Desktop integration

On X11, the screenshot button immediately starts area selection. Other Linux sessions use the standard desktop screenshot portal. Threadbox hides while capture is active, then returns to the foreground and attaches the image. Clipboard image paste is available anywhere in the new-task dialog.

The global capture shortcut, login autostart, 12/24-hour clock and overdue reminder interval can be changed in **Settings**. Autostart is enabled by default and automatic launches stay hidden in the system tray. The reminder sound can be tested there. Sticky reminders can bring Threadbox above other applications and require the user to complete, snooze or open an overdue task. They can be disabled independently in Settings. Overdue reminders stop after the task is completed or when reminders are disabled.

## Build Linux packages

```bash
npm run release:linux
```

The release script limits the build to 4 CPU cores, 6 GB of memory, 3 concurrent Rust jobs, and 5 GB of retained Docker build cache. Override these defaults with `THREADBOX_BUILD_MEMORY`, `THREADBOX_BUILD_CPU_QUOTA`, `THREADBOX_BUILD_JOBS`, and `THREADBOX_BUILD_CACHE_LIMIT`. Packages are written under `release/linux/`, and the Firefox extension is written under `release/extension/`.

## Privacy

Threadbox reads browser content only after the user selects a capture action. Tasks and message excerpts stay in local SQLite storage. Screenshots, recordings, and file attachments are stored as regular files in the application data directory and open with their system-associated applications. The extension requests access only to Slack, Gmail, and WhatsApp Web.

## Project status

Threadbox is pre-release software. Website adapters depend on page structure and require maintenance when Slack, Gmail, or WhatsApp changes its interface.

## License

Threadbox is available under the MIT License. See `LICENSE`.
