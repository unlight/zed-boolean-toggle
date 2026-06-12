# ── Boolean Toggle – build targets ───────────────────────────────────────────
#
# Usage:
#   make dev        → full development setup (build + install LSP)
#   make build      → build both artefacts without installing
#   make test       → run all unit tests
#   make clean      → remove build artefacts

.PHONY: build build-lsp build-ext test install-lsp dev clean

# ─── Composite targets ────────────────────────────────────────────────────────

## Full development setup: install the LSP binary and build the WASM extension.
dev: install-lsp build-ext
	@echo ""
	@echo "✅  Development build complete."
	@echo ""
	@echo "Load the extension in Zed:"
	@echo "  Extensions (⌘⇧X) → 'Install Dev Extension' → select this directory"
	@echo ""
	@echo "Add the keybinding to ~/.config/zed/keymap.json — see README.md."

## Build both the WASM extension and the native LSP binary (no install).
build: build-lsp build-ext

# ─── Individual targets ───────────────────────────────────────────────────────

## Build the native LSP server binary (release mode).
build-lsp:
	cargo build --release -p boolean-toggle-lsp

## Ensure the wasm32-wasip1 target is installed, then build the WASM extension.
build-ext:
	rustup target add wasm32-wasip1 2>/dev/null || true
	cargo build --target wasm32-wasip1 --release -p boolean-toggle

## Install the LSP binary to Cargo's bin directory (puts it on PATH).
install-lsp: build-lsp
	cargo install --path crates/lsp --force

## Run all unit tests (LSP crate; extension crate is WASM-only).
test:
	cargo test -p boolean-toggle-lsp -- --color always 2>&1

## Remove build artefacts.
clean:
	cargo clean
