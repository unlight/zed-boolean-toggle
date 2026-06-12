use zed_extension_api::{self as zed, Editor, Result};

struct BooleanToggle;

impl zed::Extension for BooleanToggle {
    fn new() -> Self {
        Self
    }
}

impl BooleanToggle {
    /// Toggle the boolean word under each cursor between `true` and `false`.
    #[zed_extension_api::command] // ← correct attribute path
    fn toggle(&self, editor: &mut Editor) -> Result<()> {
        let language = editor.language();
        if !matches!(
            language.as_str(),
            "JavaScript" | "TypeScript" | "JSX" | "TSX"
        ) {
            return Ok(());
        }

        let selections = editor.selections()?;
        let mut edits = Vec::new();

        for sel in &selections {
            let cursor = sel.head;
            let Some(word_range) = editor.word_range_at(cursor) else {
                continue;
            };

            let word = editor.text_for_range(word_range)?;
            let replacement = match word.as_str() {
                "true" => "false",
                "false" => "true",
                _ => continue,
            };

            // `Edit` is directly under `zed` (which is `zed_extension_api`)
            edits.push(zed::Edit::replace(word_range, replacement.to_string()));
        }

        if !edits.is_empty() {
            editor.edit(edits)?;
        }

        Ok(())
    }
}

zed::register_extension!(BooleanToggle);
