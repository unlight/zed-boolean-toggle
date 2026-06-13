use std::path::Path;
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
        let mut lsp_path = String::new();
        if (false) {
            // 1. Detect the user's Operating System and CPU Architecture
            let (os, arch) = zed::current_platform();

            // 2. Map the platform to the exact name of the binary asset you will upload to GitHub
            let asset_name = match (os, arch) {
                (zed::Os::Mac, zed::Architecture::Aarch64) => {
                    "boolean-toggle-lsp-aarch64-apple-darwin"
                }
                (zed::Os::Mac, zed::Architecture::X8664) => {
                    "boolean-toggle-lsp-x86_64-apple-darwin"
                }
                // Not compiling
                // (zed::Os::Linux, zed::Architecture::Aarch64) => {
                //     "boolean-toggle-lsp-aarch64-unknown-linux-gnu"
                // }
                (zed::Os::Linux, zed::Architecture::X8664) => {
                    "boolean-toggle-lsp-x86_64-unknown-linux-gnu"
                }
                (zed::Os::Windows, zed::Architecture::X8664) => {
                    "boolean-toggle-lsp-x86_64-pc-windows-msvc.exe"
                }
                _ => return Err(format!("Unsupported platform: {:?} {:?}", os, arch).into()),
            };

            // 3. Define where to save the binary locally inside the extension's directory
            lsp_path = format!("bin/{}", asset_name);

            // 4. If the binary doesn't exist yet, download it!
            if !Path::new(&lsp_path).exists() {
                let release = zed::github_release_by_tag_name(
                    "unlight/zed-boolean-toggle",
                    concat!("v", env!("CARGO_PKG_VERSION")), // The exact tag name of your GitHub release
                )?;

                // Search the release assets to find the one that matches our platform's filename
                let asset = release
                    .assets
                    .iter()
                    .find(|asset| asset.name == asset_name)
                    .ok_or_else(|| format!("no asset found for {}", asset_name))?;

                // Download the file using the URL provided directly by the GitHub API
                zed::download_file(
                    &asset.download_url,
                    &lsp_path,
                    zed::DownloadedFileType::Uncompressed,
                )?;

                // Make it executable (required for macOS and Linux)
                zed::make_file_executable(&lsp_path)?;
            }
        } else {
            // In a production extension, you would use zed::download_file to fetch the
            // pre-compiled binary for the user's platform from GitHub releases.
            // For this example, we assume `boolean-toggle-lsp` is in the PATH or worktree.
            lsp_path = worktree
                .which("boolean-toggle-lsp")
                .unwrap_or_else(|| "boolean-toggle-lsp".to_string());

            println!("DEBUG: The lsp_path is {}", lsp_path);
        }

        Ok(zed::Command {
            command: lsp_path,
            args: vec![], // The LSP server runs over stdio
            env: Default::default(),
        })
    }

    // fn download_if_needed() {}
}

// Registers the extension with Zed
zed::register_extension!(BooleanToggleExtension);
