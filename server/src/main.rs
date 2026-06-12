use dashmap::DashMap;
use regex::Regex;
use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Document {
    text: Rope,
    language_id: String,
}

struct Backend {
    client: Client,
    documents: DashMap<Url, Document>,
}

// Helper: Convert UTF-8 byte offset to LSP Position (Line/UTF-16 character)
fn offset_to_position(text: &Rope, offset: usize) -> Position {
    let line = text.byte_to_line(offset);
    let line_start_char = text.line_to_char(line);
    let char_offset = text.byte_to_char(offset);
    let char_in_line = char_offset - line_start_char;

    let utf16_offset = text
        .line(line)
        .chars()
        .take(char_in_line)
        .fold(0, |acc, c| acc + c.len_utf16());
    Position::new(line as u32, utf16_offset as u32)
}

// Helper: Convert LSP Position to UTF-8 byte offset
fn position_to_offset(text: &Rope, position: Position) -> usize {
    let line = position.line as usize;
    if line >= text.len_lines() {
        return text.len_bytes();
    }

    let line_start_char = text.line_to_char(line);
    let mut utf16_seen = 0;
    let mut char_offset_in_line = 0;

    for c in text.line(line).chars() {
        if utf16_seen >= position.character as usize {
            break;
        }
        utf16_seen += c.len_utf16();
        char_offset_in_line += 1;
    }
    text.char_to_byte(line_start_char + char_offset_in_line)
}

// Finds a target keyword near the cursor using regex
fn find_toggle_target(
    text: &Rope,
    offset: usize,
    is_markdown: bool,
) -> Option<(usize, usize, String)> {
    let text_str = text.to_string();
    if text_str.is_empty() {
        return None;
    }

    let keywords = if is_markdown {
        vec!["true", "false", "yes", "no", "on", "off", "1", "0"]
    } else {
        vec!["true", "false"]
    };

    // (?i) makes it case-insensitive. \b ensures word boundaries (e.g., doesn't match "false" in "falsehood")
    let pattern = format!(r"(?i)\b({})\b", keywords.join("|"));
    let re = Regex::new(&pattern).unwrap();

    let mut best_match: Option<(usize, usize, String)> = None;
    let mut min_dist = usize::MAX;

    for cap in re.captures_iter(&text_str) {
        let m = cap.get(1).unwrap();
        let start = m.start();
        let end = m.end();

        // Calculate distance from cursor to the match
        let dist = if offset >= start && offset <= end {
            0 // Cursor is inside the word
        } else if offset < start {
            start - offset
        } else {
            offset - end
        };

        // Only match if the cursor is on the word or immediately adjacent to it
        if dist <= 1 && dist < min_dist {
            min_dist = dist;
            best_match = Some((start, end, text_str[start..end].to_string()));
        }
    }
    best_match
}

// Calculates the replacement string while preserving original case style (e.g., TRUE -> FALSE, True -> False)
fn get_replacement(word: &str, is_markdown: bool) -> Option<(String, String)> {
    let lower = word.to_lowercase();

    let is_all_upper = word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic())
        && word.chars().any(|c| c.is_alphabetic());
    let is_title = word.chars().next().map_or(false, |c| c.is_uppercase())
        && word
            .chars()
            .skip(1)
            .all(|c| c.is_lowercase() || !c.is_alphabetic());

    let apply_case = |replacement: &str| -> String {
        if is_all_upper {
            replacement.to_uppercase()
        } else if is_title {
            let mut c = replacement.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        } else {
            replacement.to_lowercase()
        }
    };

    match lower.as_str() {
        "true" => {
            let n = apply_case("false");
            Some((n.clone(), format!("Toggle to {}", n)))
        }
        "false" => {
            let n = apply_case("true");
            Some((n.clone(), format!("Toggle to {}", n)))
        }
        "yes" if is_markdown => {
            let n = apply_case("no");
            Some((n.clone(), format!("Toggle to {}", n)))
        }
        "no" if is_markdown => {
            let n = apply_case("yes");
            Some((n.clone(), format!("Toggle to {}", n)))
        }
        "on" if is_markdown => {
            let n = apply_case("off");
            Some((n.clone(), format!("Toggle to {}", n)))
        }
        "off" if is_markdown => {
            let n = apply_case("on");
            Some((n.clone(), format!("Toggle to {}", n)))
        }
        "1" if is_markdown => Some(("0".to_string(), "Toggle to 0".to_string())),
        "0" if is_markdown => Some(("1".to_string(), "Toggle to 1".to_string())),
        _ => None,
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // We need incremental sync to efficiently update our rope
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                // Advertise that we provide code actions
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Boolean Toggle LSP initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = Document {
            text: Rope::from_str(&params.text_document.text),
            language_id: params.text_document.language_id,
        };
        self.documents.insert(params.text_document.uri, doc);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(mut doc) = self.documents.get_mut(&params.text_document.uri) {
            for change in params.content_changes {
                if let Some(range) = change.range {
                    let start = position_to_offset(&doc.text, range.start);
                    let end = position_to_offset(&doc.text, range.end);
                    doc.text.remove(start..end);
                    doc.text.insert(start, &change.text);
                } else {
                    doc.text = Rope::from_str(&change.text);
                }
            }
        }
    }

    // This is where the magic happens: providing the "Toggle" action to Zed
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let mut actions = Vec::new();
        let uri = params.text_document.uri.clone();

        if let Some(doc) = self.documents.get(&uri) {
            let is_markdown = doc.language_id == "markdown";
            let start_offset = position_to_offset(&doc.text, params.range.start);

            if let Some((w_start, w_end, word)) =
                find_toggle_target(&doc.text, start_offset, is_markdown)
            {
                if let Some((new_word, title)) = get_replacement(&word, is_markdown) {
                    let edit = TextEdit {
                        range: Range {
                            start: offset_to_position(&doc.text, w_start),
                            end: offset_to_position(&doc.text, w_end),
                        },
                        new_text: new_word,
                    };
                    let mut changes = std::collections::HashMap::new();
                    changes.insert(uri.clone(), vec![edit]);

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title,
                        kind: Some(CodeActionKind::REFACTOR_REWRITE),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }
            }
        }
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: DashMap::new(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
