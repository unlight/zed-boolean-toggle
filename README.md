# Boolean Toggle — Zed Extension

A Zed editor extension that toggles boolean-like values in **any language**
using a single keyboard shortcut.

---

## Supported toggles

| A side  | B side  | Variants (all automatically detected & preserved) |
|---------|---------|---------------------------------------------------|
| `true`  | `false` | `true` / `True` / `TRUE`                          |
| `yes`   | `no`    | `yes` / `Yes` / `YES`                             |
| `on`    | `off`   | `on`  / `On`  / `ON`                              |
| `1`     | `0`     | (numeric — no case to preserve)                   |

### Case preservation

The extension detects the case style of the token under your cursor and
applies the same style to the replacement:

```
true   →  false       (lowercase)
True   →  False       (Title case)
TRUE   →  FALSE       (UPPER CASE)
tRuE   →  false       (Mixed → falls back to lowercase)
```

The same logic applies to `YES/NO`, `ON/OFF`, etc.

---

## Demo

```ts
// TypeScript / JavaScript
const debug = true;       // → false
if (enabled === FALSE) …  // → TRUE

// Python / YAML config
verbose: yes              // → no
mode: ON                  // → OFF

// Any config / markdown
enabled = 1               // → 0
active: Yes               // → No
```

---

## Language support

The extension is active in 40+ languages out of the box:

**Web:** JavaScript, TypeScript, JSX, TSX, HTML, CSS, SCSS, Vue, Svelte, Astro  
**Systems:** Rust, C, C++, Go, Swift, Zig  
**JVM / CLR:** Java, Kotlin, Scala, C#, Dart  
**Scripting:** Python, Ruby, PHP, Lua, Elixir, Erlang, Haskell, OCaml, F#, Clojure, Julia, R  
**Shell:** Bash, Fish  
**Config / Data:** JSON, YAML, TOML, XML  
**Docs / Text:** Markdown, Plain Text  
**Other:** SQL, Dockerfile, Nix, Terraform, GDScript, GLSL

To add more, append Zed's language name to the `language = [...]` array in
`extension.toml`.  The canonical names are listed in Zed's default settings
under `"languages"`.

---

## Architecture

```
zed-boolean-toggle/
├── extension.toml             ← Zed extension manifest (language list lives here)
├── crates/
│   ├── extension/             ← WASM adapter  (target: wasm32-wasip1)
│   │   └── src/lib.rs         ← tells Zed which binary to launch
│   └── lsp/                   ← native LSP server binary
│       └── src/main.rs        ← all toggle logic lives here
└── Makefile
```

### Why two crates?

Zed extensions run as **WebAssembly** inside a sandbox — they can't run
arbitrary native code.  Language servers, however, must run as **native OS
processes**.  The split is therefore:

| Crate | Target | Role |
|-------|--------|------|
| `extension` | `wasm32-wasip1` | Zed plugin adapter — finds and spawns the LSP |
| `lsp` | native OS binary | Language server — all toggle detection logic |

### Data flow

```
Zed editor
  │  1. Opens any supported file → spawns boolean-toggle-lsp via stdin/stdout
  │
  │  2. User presses Ctrl+Alt+X
  │  3. Zed sends textDocument/codeAction with cursor line + column
  │
  LSP server
  │  4. Finds which keyword the cursor is on (case-insensitive scan)
  │  5. Detects case style: Lower / Title / Upper / Mixed
  │  6. Builds replacement with matching case style
  │  7. Returns CodeAction { WorkspaceEdit { newText: "<replacement>" } }
  │
  Zed editor
     8. User selects the action → edit applied, undoable with Ctrl+Z
```

### Token detection (`find_token_ci`)

```
Line:    "const DEBUG = TRUE && isOn"
Cols:     0     6       14    19    25
                             ^^^^  (TRUE → cols 14–17)
                                   ^^^   (ON  ← rejected: part of "isOn")
```

For each keyword in `TOGGLE_PAIRS` the function:

1. Slides a case-insensitive window of `keyword.len()` bytes across the line.
2. Verifies **word boundaries**: the byte before and after must not match
   `[A-Za-z0-9_$]` — this rejects `trueValue`, `isFalse`, `$true`, `10`, etc.
3. Checks `cursor_col ∈ [token_start, token_end]` (inclusive on both ends —
   a cursor right after the last character still counts).

All keywords are ASCII, so byte offsets equal UTF-16 code-unit offsets (what
LSP uses for column positions).

### Case detection

```rust
detect_case_style("TRUE")  → Upper   (all alpha chars uppercase)
detect_case_style("True")  → Title   (first upper, rest lower)
detect_case_style("true")  → Lower   (all lowercase)
detect_case_style("tRuE")  → Mixed   (fallback → lowercase replacement)
detect_case_style("1")     → Lower   (no alpha chars → treat as lower)
```

---

## Requirements

| Tool | Notes |
|------|-------|
| [Rust + Cargo](https://rustup.rs) | stable ≥ 1.78 |
| `wasm32-wasip1` target | `make dev` installs it automatically |
| [Zed](https://zed.dev) | 0.140+ recommended |

---

## Quick start (development)

```bash
# 1. Clone
git clone https://github.com/yourname/zed-boolean-toggle
cd zed-boolean-toggle

# 2. Build + install
make dev
```

`make dev` does three things:

1. `cargo build --release -p boolean-toggle-lsp` → builds the native binary
2. `cargo install --path crates/lsp` → copies it to `~/.cargo/bin` (on `$PATH`)
3. `cargo build --target wasm32-wasip1 --release -p boolean-toggle` → builds the WASM

### Load the extension in Zed

Open the **Extensions** panel (⌘⇧X / Ctrl⇧X) → **"Install Dev Extension"**
→ select this directory.

---

## Keybinding

Add this to `~/.config/zed/keymap.json`:

```json
[
  {
    "context": "Editor",
    "bindings": {
      "ctrl-alt-x": "editor::ToggleCodeActions"
    }
  }
]
```

The `context: "Editor"` makes it available in every file type.
If you want it only in specific languages, narrow the context:

```json
{
  "context": "Editor && (language == Python || language == YAML)",
  "bindings": { "ctrl-alt-x": "editor::ToggleCodeActions" }
}
```

### How the UX works

1. Place the cursor on any supported token (`true`, `YES`, `on`, `1`, …)
2. Press **Ctrl + Alt + X** — the code-actions panel opens
3. The panel shows exactly one action, e.g. `Toggle:  TRUE → FALSE`
4. Press **Enter** (or click) to apply

When the cursor is **not** on a toggle token, `editor::ToggleCodeActions`
falls through to other code actions from TypeScript, ESLint, etc. — no
interference.

> **Tip:** the action is marked `"isPreferred": true`.  If a future Zed
> version adds an "apply preferred action" command, you'll be able to bind
> that to get a truly single-keystroke toggle.

---

## Running tests

```bash
make test
# or:
cargo test -p boolean-toggle-lsp
```

The suite covers:

| Category | Examples |
|----------|---------|
| All four pair types | true/false, yes/no, on/off, 1/0 |
| Three case styles | lower, Title, UPPER |
| Mixed case fallback | `tRuE` → `false` |
| Cursor positions | start, middle, end, one-past-end of token |
| Word boundaries | `trueValue`, `isFalse`, `$true`, `10`, `online`, `notable`, `yesterday` |
| Multiple tokens per line | Cursor selects the right one |
| WorkspaceEdit ranges | Exact character positions verified |
| Action metadata | `isPreferred`, `kind`, title format |
| Server dispatch | `initialize`, `didOpen`, `didChange`, `didClose` |
| Transport | `Content-Length` framing round-trip |

---

## Building manually

```bash
# Native LSP binary
cargo build --release -p boolean-toggle-lsp
# → target/release/boolean-toggle-lsp

# WASM extension
rustup target add wasm32-wasip1
cargo build --target wasm32-wasip1 --release -p boolean-toggle
# → target/wasm32-wasip1/release/boolean_toggle.wasm
```

---

## Production deployment

For the Zed extension marketplace, bundle pre-built binaries instead of
relying on PATH:

```rust
// crates/extension/src/lib.rs
fn language_server_command(
    &mut self,
    _id: &LanguageServerId,
    _worktree: &zed::Worktree,
) -> Result<zed::Command> {
    let (os, arch) = zed::current_platform();
    let binary = self.download_if_needed(os, arch)?; // cache in extension dir
    Ok(zed::Command { command: binary, args: vec![], env: vec![] })
}
```

Upload platform binaries (`linux-x86_64`, `linux-aarch64`, `macos-x86_64`,
`macos-aarch64`, `windows-x86_64`) to a GitHub Release and download them
lazily on first use via Zed's `zed::download_file` API.  See the
[Zed extension docs](https://zed.dev/docs/extensions/developing-extensions)
for the full API.

---

## Adding new toggle pairs

Edit `TOGGLE_PAIRS` in `crates/lsp/src/main.rs`:

```rust
const TOGGLE_PAIRS: &[(&str, &str)] = &[
    ("true",    "false"),
    ("yes",     "no"),
    ("on",      "off"),
    ("1",       "0"),
    // Add yours:
    ("enabled", "disabled"),   // ← new
    ("allow",   "deny"),       // ← new
];
```

Rules for new pairs:
- Always lowercase canonical form (case detection/application is automatic)
- Word-boundary checking is automatic — no extra configuration needed
- Rebuild with `make build-lsp` and `cargo install --path crates/lsp`

---

## Project layout reference

```
zed-boolean-toggle/
├── Cargo.toml                  Workspace (two members)
├── Makefile                    build / test / install helpers
├── README.md                   this file
├── extension.toml              Zed manifest — language list lives here
└── crates/
    ├── extension/
    │   ├── Cargo.toml          crate-type = ["cdylib"], target = wasm32-wasip1
    │   └── src/lib.rs          zed::Extension impl + register_extension! macro
    └── lsp/
        ├── Cargo.toml          binary crate, only dep = serde_json
        └── src/main.rs         TOGGLE_PAIRS · CaseStyle · find_token_ci · tests
```

---

## Contributing

1. Fork → branch → `make test` must pass green
2. Every new toggle pair or behaviour needs a matching test in `main.rs`
3. No new runtime dependencies without discussion — `serde_json` is the only one

---

## License

MIT — see `LICENSE`.
