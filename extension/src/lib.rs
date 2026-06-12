use zed_extension_api as zed;

// The core extension struct
struct BooleanToggleExtension;

impl zed::Extension for BooleanToggleExtension {
    fn new() -> Self {
        Self
    }

    // This method is called by Zed when it needs to start the language server.
    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        // In a production extension, you would use zed::download_file to fetch the
        // pre-compiled binary for the user's platform from GitHub releases.
        // For this example, we assume `boolean-toggle-lsp` is in the PATH or worktree.
        let lsp_path = worktree
            .which("boolean-toggle-lsp")
            .unwrap_or_else(|| "boolean-toggle-lsp".to_string());

        Ok(zed::Command {
            command: lsp_path,
            args: vec![], // The LSP server runs over stdio
            env: Default::default(),
        })
    }
}

// Registers the extension with Zed
zed::register_extension!(BooleanToggleExtension);
