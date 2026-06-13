# Usage:
#   make dev        → full development setup (build + install LSP)
#   make build      → build both artefacts without installing
#   make test       → run all unit tests
#   make clean      → remove build artefacts

.PHONY: build build-lsp build-ext test install-lsp dev clean

## Full development setup: install the LSP binary and build the WASM extension
dev: install-lsp build-ext
	@echo ""
	@echo "✅  Development build complete."
	@echo ""
	@echo "Load the extension in Zed:"
	@echo "  Extensions (⌘⇧X) → 'Install Dev Extension' → select this directory"

## Build both the WASM extension and the native LSP binary (no install)
build: build-lsp build-ext

## Build the native LSP server binary (release mode)
build-lsp:
	cargo build --release --manifest-path server/Cargo.toml

## Ensure the wasm32-wasip1 target is installed, then build the WASM extension.
build-ext:
	rustup target add wasm32-wasip1 2>/dev/null || true
	cargo build --target wasm32-wasip1 --release -p boolean-toggle

## Install the LSP binary to Cargo's bin directory (puts it on PATH).
install-lsp: build-lsp
	cargo install --path server --force

## Run all unit tests (LSP crate; extension crate is WASM-only).
test:
	cargo test --manifest-path server/Cargo.toml -- --color always 2>&1

## Remove build artefacts.
clean:
	cargo clean
	cargo clean --manifest-path server/Cargo.toml
