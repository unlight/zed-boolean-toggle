# Boolean Toggle for Zed

A Zed editor extension that toggles boolean and binary values. 
It intelligently preserves your original casing (e.g., `true` ↔ `false`, `TRUE` ↔ `FALSE`, `True` ↔ `False`) and supports extended toggles for configuration and markup files.

## Features

- **Smart Case Preservation**: Toggles `true` ↔ `false`, `TRUE` ↔ `FALSE`, and `True` ↔ `False` seamlessly
- **Extended Toggles**: Automatically supports `yes` ↔ `no`, `on` ↔ `off`, and `1` ↔ `0` in **Markdown**, **YAML**, and **TOML** files
- **Language Agnostic**: Works in any language via the Language Server Protocol (LSP) Code Actions

## Installation

### Via Zed Extension Registry (Not yet)
1. Open Zed
2. Open the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`)
3. Run `zed: install extension`
4. Search for **Boolean Toggle** and click Install

### Manual Installation 
1. Clone this repository
2. Open Zed and run `zed: install dev extension`
3. Select the root directory of this cloned repository
4. *(For local development only)* Build the LSP server and place it in your system `PATH`:
   ```bash
   cd server
   cargo build --release
   cp target/release/boolean-toggle-lsp ~/.cargo/bin/ # Linux/macOS
   # or copy target\release\boolean-toggle-lsp.exe %USERPROFILE%\.cargo\bin\ # Windows

## License
[MIT License](https://opensource.org/licenses/MIT) (c) 2026
