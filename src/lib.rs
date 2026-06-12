//! Zed extension entry point.
//!
//! This extension exposes a single slash command, `/toggle-bool`, that
//! flips the boolean-like token passed as its argument. See `src/toggle.rs`
//! for the language-agnostic toggle rules and `README.md` for how to bind
//! it to `ctrl-alt-x` via Zed's keymap.

mod toggle;

use zed_extension_api::{
    register_extension, Extension, SlashCommand, SlashCommandArgumentCompletion,
    SlashCommandOutput, SlashCommandOutputSection, Worktree,
};

struct ToggleBoolExtension;

impl Extension for ToggleBoolExtension {
    fn new() -> Self {
        Self
    }

    /// Called by Zed when the user invokes a slash command declared in
    /// `extension.toml`. We only own one: `toggle-bool`.
    fn run_slash_command(
        &self,
        command: SlashCommand,
        args: Vec<String>,
        _worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        if command.name != "toggle-bool" {
            return Err(format!("unknown slash command: {}", command.name));
        }

        // Join arguments back together so the user can pass either
        // `/toggle-bool true` or `/toggle-bool   True  ` etc.
        let raw = args.join(" ");

        // The language hint is optional. Zed currently does not pass the
        // active buffer's language to slash commands, so we fall back to
        // an explicit `--lang=<name>` flag when the user wants Markdown
        // extras (e.g. `/toggle-bool on --lang=Markdown`).
        let (token, language) = parse_args(&raw);

        let toggled = toggle::toggle_token(token, language.as_deref())
            .ok_or_else(|| format!("`{}` is not a recognised boolean token", token))?;

        let section = SlashCommandOutputSection {
            range: (0..toggled.len()).into(),
            label: format!("toggle-bool: {} → {}", token, toggled),
        };

        Ok(SlashCommandOutput {
            text: toggled,
            sections: vec![section],
        })
    }

    /// Provide argument completions for the slash command. Zed will show
    /// these in the assistant panel as the user types.
    fn complete_slash_command_argument(
        &self,
        command: SlashCommand,
        _args: Vec<String>,
    ) -> Result<Vec<SlashCommandArgumentCompletion>, String> {
        if command.name != "toggle-bool" {
            return Ok(Vec::new());
        }
        Ok(["true", "false", "True", "TRUE", "on", "yes", "1"]
            .iter()
            .map(|label| completion(label))
            .collect())
    }
}

fn completion(label: &str) -> SlashCommandArgumentCompletion {
    SlashCommandArgumentCompletion {
        label: label.to_string(),
        new_text: label.to_string(),
        run_command: true,
    }
}

/// Split the raw argument string into `(token, optional_language)`.
///
/// Recognises a trailing `--lang=<name>` flag, since Zed does not (yet)
/// pass buffer language metadata into slash commands.
fn parse_args(raw: &str) -> (&str, Option<String>) {
    let mut token = raw.trim();
    let mut language: Option<String> = None;

    if let Some(idx) = token.rfind("--lang=") {
        let (left, right) = token.split_at(idx);
        let lang = right.trim_start_matches("--lang=").trim();
        if !lang.is_empty() {
            language = Some(lang.to_string());
        }
        token = left.trim();
    }

    (token, language)
}

register_extension!(ToggleBoolExtension);

// ---------------------------------------------------------------------------
// Tests for the thin argument-parsing layer.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::parse_args;

    #[test]
    fn parses_bare_token() {
        assert_eq!(parse_args("true"), ("true", None));
    }

    #[test]
    fn parses_token_with_language() {
        let (token, lang) = parse_args("on --lang=Markdown");
        assert_eq!(token, "on");
        assert_eq!(lang.as_deref(), Some("Markdown"));
    }

    #[test]
    fn trims_whitespace() {
        let (token, lang) = parse_args("  YES   --lang=md  ");
        assert_eq!(token, "YES");
        assert_eq!(lang.as_deref(), Some("md"));
    }
}
