# OpenAuto Compilation & Cross-Platform Distribution

This document outlines how to compile OpenAuto for macOS and Windows, and explains how cross-platform paths are handled within the codebase.

## Path Management (macOS vs Windows)

You do not need to create separate `/mac`, `/windows_x64`, or `/windows_x86` folders for path variables! The Rust codebase already elegantly handles this using conditional compilation in a single source of truth: `crates/empire-core/src/paths.rs`.

At compile time, Rust automatically selects the correct semantic path for the target OS:
- **macOS** (`#[cfg(target_os = "macos")]`): Uses `~/Library/Application Support/com.openauto.desktop`
- **Windows** (`#[cfg(target_os = "windows")]`): Uses the native `%APPDATA%\com.openauto.desktop` directory (e.g., `C:\Users\Username\AppData\Roaming\com.openauto.desktop`)
- **Linux**: Uses `~/.local/share/com.openauto.desktop`

This guarantees that the SQLite databases and account states stay completely independent of the binaries, and developers never have to write OS-specific wrappers manually. The Rust compiler magically handles it for whichever platform you compile to!

## Compilation Commands

To build the distributables for different platforms, use the Tauri CLI (which automatically drives `cargo` for the Rust backend and `vite` for the JS frontend).

### macOS
The fastest way to build for macOS (since you are on a Mac) is to use the included project shortcut which handles bumping the version, building the `.dmg`, and installing it to `/Applications/OpenAuto.app`:

```bash
oa bump && oa build
```

Alternatively, to build manually without the release script:
```bash
cd desktop
npm run tauri build
```

### Windows (x64 and x86)

Because Tauri tightly integrates with the OS windowing system (WebView2 on Windows, WebKit on Mac), **the officially recommended way to compile Windows `.exe` installers is to run the build command directly on a Windows machine** (or via a CI/CD pipeline like GitHub Actions). 

On a Windows machine with Node.js and Rust installed, open PowerShell/Command Prompt in the project root:

**Windows 64-bit (x64) - Standard**
```powershell
cd desktop
npm run tauri build -- --target x86_64-pc-windows-msvc
```
*Outputs an installer: `desktop\src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\OpenAuto_x64_setup.exe`*

**Windows 32-bit (x86)**
```powershell
cd desktop
npm run tauri build -- --target i686-pc-windows-msvc
```
*Outputs an installer: `desktop\src-tauri\target\i686-pc-windows-msvc\release\bundle\nsis\OpenAuto_x86_setup.exe`*

*(Note: You will need the standard Rust MSVC toolchains installed via `rustup target add x86_64-pc-windows-msvc` and `rustup target add i686-pc-windows-msvc` on your Windows build machine).*
