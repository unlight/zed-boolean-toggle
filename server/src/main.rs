//! `boolean-toggle-lsp` – a minimal Language Server Protocol server.
//!
//! ## What it does
//!
//! Detects "boolean-like" tokens at the cursor position in **any language**
//! and offers a single code action to toggle them:
//!
//! | A side          | B side          |
//! |-----------------|-----------------|
//! | `true` / `True` / `TRUE` | `false` / `False` / `FALSE` |
//! | `yes`  / `Yes`  / `YES`  | `no`    / `No`    / `NO`    |
//! | `on`   / `On`   / `ON`   | `off`   / `Off`   / `OFF`   |
//! | `1`                      | `0`                         |
//!
//! **Case is detected and preserved**: `True` becomes `False`, `YES` becomes `NO`.
//! Mixed-case strings fall back to lowercase.
//!
//! ## Protocol overview
//!
//! ```
//! Editor                           boolean-toggle-lsp
//!   │──── initialize ────────────────────────────────▶│
//!   │◀─── serverCapabilities ────────────────────────│
//!   │──── initialized ───────────────────────────────▶│  (notification)
//!   │──── textDocument/didOpen ──────────────────────▶│  (notification)
//!   │──── textDocument/didChange ────────────────────▶│  (notification, on edit)
//!   │──── textDocument/codeAction ───────────────────▶│  (on Ctrl+Alt+X)
//!   │◀─── [{ title, kind, isPreferred, edit }] ──────│
//!   │──── textDocument/didClose ─────────────────────▶│  (notification)
//!   │──── shutdown ──────────────────────────────────▶│
//!   │◀─── null ──────────────────────────────────────│
//!   │──── exit ──────────────────────────────────────▶│  (notification)
//! ```
//!
//! ## Transport
//!
//! JSON-RPC 2.0 messages framed with `Content-Length` over **stdin / stdout**.
//! Stderr is safe for debug logging and never read by the client.

use std::{
    collections::HashMap,
    io::{self, BufRead, Read, Write},
};

use serde_json::{json, Value};

// ─── Toggle pairs ─────────────────────────────────────────────────────────────

/// Every pair is expressed in **lowercase** canonical form.
/// Matching is case-insensitive; case is detected on the source token and
/// preserved in the replacement.
///
/// Pairs are tried in order — the first one whose A or B side matches
/// the cursor wins.
const TOGGLE_PAIRS: &[(&str, &str)] =
    &[("true", "false"), ("yes", "no"), ("on", "off"), ("1", "0")];

// ─── Server ───────────────────────────────────────────────────────────────────

/// All mutable state required across LSP requests.
struct Server {
    /// Full UTF-8 text of every open document, keyed by `file://…` URI.
    ///
    /// We use full sync (`TextDocumentSyncKind::Full`): the client sends the
    /// entire document on every change.  Correct and simple for this server.
    documents: HashMap<String, String>,
}

impl Server {
    fn new() -> Self {
        Server {
            documents: HashMap::new(),
        }
    }

    /// Dispatch one JSON-RPC message and return an optional response.
    ///
    /// - **Requests** (have an `id`) → return `Some(response)`.
    /// - **Notifications** (no `id`) → return `None`.
    fn dispatch(&mut self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("");

        match method {
            // ── Lifecycle ──────────────────────────────────────────────────
            "initialize" => Some(ok(id?, server_capabilities())),

            // Notification — no reply expected.
            "initialized" => None,

            "shutdown" => Some(ok(id?, Value::Null)),

            "exit" => std::process::exit(0),

            // ── Document sync ──────────────────────────────────────────────
            "textDocument/didOpen" => {
                let uri = pluck_str(&msg, &["params", "textDocument", "uri"])?;
                let text = pluck_str(&msg, &["params", "textDocument", "text"])?;
                self.documents.insert(uri, text);
                None
            }

            "textDocument/didChange" => {
                let uri = pluck_str(&msg, &["params", "textDocument", "uri"])?;
                // Full-sync: the last change object contains the full text.
                let text = msg["params"]["contentChanges"].as_array()?.last()?["text"]
                    .as_str()?
                    .to_owned();
                self.documents.insert(uri, text);
                None
            }

            "textDocument/didClose" => {
                let uri = pluck_str(&msg, &["params", "textDocument", "uri"])?;
                self.documents.remove(&uri);
                None
            }

            // ── Core feature: code actions ─────────────────────────────────
            "textDocument/codeAction" => {
                let uri = pluck_str(&msg, &["params", "textDocument", "uri"])?;
                let line = msg["params"]["range"]["start"]["line"].as_u64()? as usize;
                let col = msg["params"]["range"]["start"]["character"].as_u64()? as usize;

                let doc = self.documents.get(&uri)?.clone();
                Some(ok(id?, code_actions(&uri, &doc, line, col)))
            }

            // ── Catch-all ──────────────────────────────────────────────────
            _ => id.map(|id| {
                json!({
                    "jsonrpc": "2.0",
                    "id":      id,
                    "error":   { "code": -32601, "message": "Method not found" }
                })
            }),
        }
    }
}

fn server_capabilities() -> Value {
    json!({
        "capabilities": {
            // 1 = Full sync.
            "textDocumentSync": 1,
            "codeActionProvider": {
                "codeActionKinds": ["refactor.rewrite"]
            }
        },
        "serverInfo": {
            "name":    "boolean-toggle-lsp",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

// ─── Toggle logic ─────────────────────────────────────────────────────────────

/// Build the `CodeAction` array for a given cursor position.
/// Returns an empty JSON array when the cursor is not on a known token.
fn code_actions(uri: &str, content: &str, line: usize, col: usize) -> Value {
    match build_toggle_action(uri, content, line, col) {
        Some(action) => json!([action]),
        None => json!([]),
    }
}

/// Iterate over `TOGGLE_PAIRS` and return the first `CodeAction` whose A or B
/// side matches the token under the cursor.
///
/// The case style of the source token is detected (lower / Title / UPPER) and
/// applied to the replacement string before building the edit.
fn build_toggle_action(uri: &str, content: &str, line: usize, col: usize) -> Option<Value> {
    let line_text = content.lines().nth(line)?;

    for &(a, b) in TOGGLE_PAIRS {
        // Try the A side first, then the B side.
        for (keyword, replacement) in [(a, b), (b, a)] {
            if let Some((start, end, matched)) = find_token_ci(line_text, keyword, col) {
                let style = detect_case_style(matched);
                let replacement = apply_case_style(replacement, style);

                return Some(json!({
                    // Title shows the actual matched text and where it goes.
                    "title": format!("Toggle:  {} → {}", matched, replacement),
                    "kind":  "refactor.rewrite",
                    // Marks this as the preferred action so editors can
                    // auto-apply it without showing a picker.
                    "isPreferred": true,
                    // A WorkspaceEdit replaces the token directly — no
                    // executeCommand round-trip needed.
                    "edit": {
                        "changes": {
                            uri: [{
                                "range": {
                                    "start": { "line": line, "character": start },
                                    "end":   { "line": line, "character": end   }
                                },
                                "newText": replacement
                            }]
                        }
                    }
                }));
            }
        }
    }

    None
}

// ─── Case detection and preservation ─────────────────────────────────────────

/// The case pattern of a source token.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CaseStyle {
    /// `true`, `false`, `yes`, `no`, `on`, `off`, `1`, `0`
    Lower,
    /// `True`, `False`, `Yes`, `No`, `On`, `Off`
    Title,
    /// `TRUE`, `FALSE`, `YES`, `NO`, `ON`, `OFF`
    Upper,
    /// Anything else (e.g. `tRuE`) — replacement will be lowercase.
    Mixed,
}

/// Infer the case style from the alphabetic characters in `s`.
///
/// Numeric characters (e.g. `1`, `0`) carry no case, so pure-numeric
/// strings return `Lower` (the replacement is unchanged by `apply_case_style`).
fn detect_case_style(s: &str) -> CaseStyle {
    let alpha: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();

    if alpha.is_empty() {
        // Numbers: no case to preserve — keep the replacement as-is.
        return CaseStyle::Lower;
    }

    if alpha.iter().all(|c| c.is_uppercase()) {
        CaseStyle::Upper
    } else if alpha.iter().all(|c| c.is_lowercase()) {
        CaseStyle::Lower
    } else if alpha[0].is_uppercase() && alpha[1..].iter().all(|c| c.is_lowercase()) {
        CaseStyle::Title
    } else {
        CaseStyle::Mixed
    }
}

/// Apply `style` to `s` and return the result as a new `String`.
///
/// - `Lower` → `to_lowercase()`
/// - `Upper` → `to_uppercase()`
/// - `Title` → first char uppercased, rest lowercased
/// - `Mixed` → falls back to `to_lowercase()`
fn apply_case_style(s: &str, style: CaseStyle) -> String {
    match style {
        CaseStyle::Lower | CaseStyle::Mixed => s.to_lowercase(),
        CaseStyle::Upper => s.to_uppercase(),
        CaseStyle::Title => {
            let mut chars = s.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    // chars.as_str() is the unconsumed remainder of the
                    // original string slice — no extra allocation needed.
                    let rest_lower = chars.as_str().to_lowercase();
                    first.to_uppercase().collect::<String>() + &rest_lower
                }
            }
        }
    }
}

// ─── Token finding ────────────────────────────────────────────────────────────

/// Scan `line` for a case-insensitive occurrence of `keyword` that:
///
/// 1. Is a **standalone token** — not adjacent to `[A-Za-z0-9_$]`.
/// 2. Is **under the cursor** — `col ∈ [start, end]` (inclusive on both ends;
///    a cursor sitting one position after the last char still counts).
///
/// Returns `(start_col, end_col, matched_slice)` on success.
///
/// # Column semantics
///
/// LSP columns are UTF-16 code-unit offsets.  All keywords in `TOGGLE_PAIRS`
/// are ASCII, so bytes == chars == UTF-16 code units — we can work on bytes.
fn find_token_ci<'a>(line: &'a str, keyword: &str, col: usize) -> Option<(usize, usize, &'a str)> {
    let src = line.as_bytes();
    let kw = keyword.as_bytes(); // already lowercase (from TOGGLE_PAIRS)
    let kw_len = kw.len();

    let mut i = 0;
    while i + kw_len <= src.len() {
        // Case-insensitive byte comparison.  Keywords are ASCII, so
        // `to_ascii_lowercase` is correct and allocation-free.
        let matches = src[i..i + kw_len]
            .iter()
            .zip(kw.iter())
            .all(|(src_byte, kw_byte)| src_byte.to_ascii_lowercase() == *kw_byte);

        if matches {
            let (start, end) = (i, i + kw_len);

            // Word-boundary guard: reject `trueValue`, `isFalse`, `10`, etc.
            let ok_before = start == 0 || !is_word_char(src[start - 1]);
            let ok_after = end == src.len() || !is_word_char(src[end]);

            if ok_before && ok_after && col >= start && col <= end {
                // SAFETY: `start..end` was found by scanning ASCII bytes inside
                // a valid UTF-8 string, so the slice is always valid UTF-8.
                return Some((start, end, &line[start..end]));
            }
        }

        i += 1;
    }

    None
}

/// Returns `true` if byte `b` can appear inside a word token.
///
/// Used for boundary checks — a keyword is only matched when neither the byte
/// immediately before nor the byte immediately after is a word character.
///
/// The set intentionally includes `$` to avoid false matches inside
/// PHP/shell/template variables like `$true` or `${ON}`.
#[inline]
fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

// ─── JSON-RPC helpers ─────────────────────────────────────────────────────────

/// Build a successful JSON-RPC response.
fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Walk a nested `Value` by field-path and return the leaf as an owned `String`.
///
/// Example: `pluck_str(&msg, &["params", "textDocument", "uri"])`.
fn pluck_str(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_str().map(str::to_owned)
}

// ─── stdio transport ──────────────────────────────────────────────────────────

/// Read one LSP message from `reader`.
///
/// LSP framing:
/// ```text
/// Content-Length: <n>\r\n
/// \r\n
/// <n bytes of UTF-8 JSON>
/// ```
fn read_message<R: BufRead>(reader: &mut R) -> Option<Value> {
    let mut content_length: Option<usize> = None;

    // Header section — read until the blank separator line.
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(n) = trimmed.strip_prefix("Content-Length: ") {
            content_length = n.trim().parse().ok();
        }
        // Content-Type and other headers are intentionally ignored.
    }

    let mut body = vec![0u8; content_length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// Write one LSP message to `writer` and flush.
fn write_message<W: Write>(writer: &mut W, msg: &Value) {
    let body =
        serde_json::to_string(msg).expect("serde_json::to_string is infallible for json!() values");
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)
        .expect("write to stdout failed");
    writer.flush().expect("flush stdout failed");
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut server = Server::new();

    loop {
        match read_message(&mut reader) {
            Some(msg) => {
                if let Some(resp) = server.dispatch(msg) {
                    write_message(&mut writer, &resp);
                }
            }
            None => break, // EOF or malformed frame
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ─── Helpers ─────────────────────────────────────────────────────────────

    const URI: &str = "file:///test.ts";

    /// Run code-action detection against a single-line document (line 0).
    fn actions(line_text: &str, col: usize) -> Value {
        code_actions(URI, line_text, 0, col)
    }

    /// Extract `newText` from the first edit in the first action.
    fn new_text(a: &Value) -> &str {
        a[0]["edit"]["changes"][URI][0]["newText"]
            .as_str()
            .unwrap_or("<none>")
    }

    fn is_empty(a: &Value) -> bool {
        a.as_array().map_or(true, |v| v.is_empty())
    }

    // ═══════════════════════════════════════════════════════════════════════
    // true / false
    // ═══════════════════════════════════════════════════════════════════════

    // ─── lowercase ───────────────────────────────────────────────────────────

    #[test]
    fn true_to_false_lowercase() {
        assert_eq!(new_text(&actions("x = true;", 4)), "false");
    }

    #[test]
    fn false_to_true_lowercase() {
        assert_eq!(new_text(&actions("x = false;", 4)), "true");
    }

    // ─── UPPER CASE ──────────────────────────────────────────────────────────

    #[test]
    fn true_to_false_upper() {
        assert_eq!(new_text(&actions("x = TRUE;", 4)), "FALSE");
    }

    #[test]
    fn false_to_true_upper() {
        assert_eq!(new_text(&actions("x = FALSE;", 4)), "TRUE");
    }

    // ─── Title Case ──────────────────────────────────────────────────────────

    #[test]
    fn true_to_false_title() {
        assert_eq!(new_text(&actions("x = True;", 4)), "False");
    }

    #[test]
    fn false_to_true_title() {
        assert_eq!(new_text(&actions("x = False;", 4)), "True");
    }

    // ─── Mixed case → lowercase ───────────────────────────────────────────────

    #[test]
    fn mixed_case_falls_back_to_lower() {
        // "tRuE" is detected as Mixed → replacement is lowercase "false"
        assert_eq!(new_text(&actions("x = tRuE;", 4)), "false");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // yes / no
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn yes_to_no_lowercase() {
        assert_eq!(new_text(&actions("enabled: yes", 9)), "no");
    }

    #[test]
    fn no_to_yes_lowercase() {
        assert_eq!(new_text(&actions("active: no", 8)), "yes");
    }

    #[test]
    fn yes_to_no_upper() {
        assert_eq!(new_text(&actions("flag: YES", 6)), "NO");
    }

    #[test]
    fn no_to_yes_upper() {
        assert_eq!(new_text(&actions("flag: NO", 6)), "YES");
    }

    #[test]
    fn yes_to_no_title() {
        assert_eq!(new_text(&actions("confirm: Yes", 9)), "No");
    }

    #[test]
    fn no_to_yes_title() {
        assert_eq!(new_text(&actions("confirm: No", 9)), "Yes");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // on / off
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn on_to_off_lowercase() {
        assert_eq!(new_text(&actions("state = on", 8)), "off");
    }

    #[test]
    fn off_to_on_lowercase() {
        assert_eq!(new_text(&actions("state = off", 8)), "on");
    }

    #[test]
    fn on_to_off_upper() {
        assert_eq!(new_text(&actions("MODE: ON", 6)), "OFF");
    }

    #[test]
    fn off_to_on_upper() {
        assert_eq!(new_text(&actions("MODE: OFF", 6)), "ON");
    }

    #[test]
    fn on_to_off_title() {
        assert_eq!(new_text(&actions("power: On", 7)), "Off");
    }

    #[test]
    fn off_to_on_title() {
        assert_eq!(new_text(&actions("power: Off", 7)), "On");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 1 / 0
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn one_to_zero() {
        // "enabled = 1" — cursor on '1' (col 10)
        assert_eq!(new_text(&actions("enabled = 1", 10)), "0");
    }

    #[test]
    fn zero_to_one() {
        assert_eq!(new_text(&actions("debug = 0", 8)), "1");
    }

    #[test]
    fn no_match_for_multi_digit_number() {
        // `10` — the `1` at col 0 is followed by `0` (a word char) → no match
        assert!(is_empty(&actions("10", 0)));
    }

    #[test]
    fn no_match_for_zero_in_100() {
        // `0` inside `100` — preceded by digit → no match
        assert!(is_empty(&actions("100", 2)));
    }

    #[test]
    fn standalone_1_in_expression() {
        // "result = 1 + x" — cursor at col 9 ('1')
        assert_eq!(new_text(&actions("result = 1 + x", 9)), "0");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Cursor position edge cases
    // ═══════════════════════════════════════════════════════════════════════

    //  col:  0123456789 (true starts at 4, ends at 7, exclusive end = 8)
    // text:  "x = true;"

    #[test]
    fn cursor_at_token_start() {
        assert_eq!(new_text(&actions("x = true;", 4)), "false");
    }

    #[test]
    fn cursor_at_token_middle() {
        assert_eq!(new_text(&actions("x = true;", 6)), "false");
    }

    #[test]
    fn cursor_at_last_char_of_token() {
        // 'e' of "true" is at col 7
        assert_eq!(new_text(&actions("x = true;", 7)), "false");
    }

    #[test]
    fn cursor_one_past_token_end() {
        // col == end is inclusive — cursor right after the token still matches
        assert_eq!(new_text(&actions("x = true;", 8)), "false");
    }

    #[test]
    fn cursor_two_past_token_end_no_match() {
        // col = 9 → on the ';', two positions after "true" ends → no match
        assert!(is_empty(&actions("x = true;", 9)));
    }

    #[test]
    fn cursor_way_past_end_no_match() {
        assert!(is_empty(&actions("x = true;", 99)));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Word-boundary checks
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn no_match_inside_identifier_prefix() {
        // "trueValue" — 't' at col 0, but 'V' after is alphanumeric → rejected
        assert!(is_empty(&actions("let trueValue = 1;", 4)));
    }

    #[test]
    fn no_match_inside_identifier_suffix() {
        // "isFalse" — 'f' at col 2, but 'i','s' before are alpha → rejected
        assert!(is_empty(&actions("const isFalse = () => {};", 9)));
    }

    #[test]
    fn no_match_no_as_suffix_in_word() {
        // "notable" contains "no" but it's followed by 't' → rejected
        assert!(is_empty(&actions("let notable = 1;", 4)));
    }

    #[test]
    fn no_match_yes_as_part_of_word() {
        // "yesterday" starts with "yes" — followed by 't' → rejected
        assert!(is_empty(&actions("let yesterday = true;", 4)));
    }

    #[test]
    fn no_match_on_as_part_of_identifier() {
        // "online" starts with "on" — followed by 'l' → rejected
        assert!(is_empty(&actions("const online = true;", 6)));
    }

    #[test]
    fn dollar_sign_blocks_match() {
        // "$true" — '$' is a word char, so "true" preceded by it is rejected
        assert!(is_empty(&actions("echo $true", 6)));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Multiple toggleable tokens on one line
    // ═══════════════════════════════════════════════════════════════════════

    //  text:  "if (true && false)"
    //  col:    0123456789012345678
    //                ^^^^  col 4-7   (true)
    //                         ^^^^^  col 12-16 (false)

    #[test]
    fn picks_first_token_when_cursor_on_it() {
        assert_eq!(new_text(&actions("if (true && false)", 5)), "false");
    }

    #[test]
    fn picks_second_token_when_cursor_on_it() {
        assert_eq!(new_text(&actions("if (true && false)", 14)), "true");
    }

    #[test]
    fn true_false_same_line_gap_gives_no_match() {
        // Cursor in the gap "&&" (cols 9-10) — no boolean there
        assert!(is_empty(&actions("if (true && false)", 9)));
    }

    // Different pair types on the same line.
    #[test]
    fn mixed_pairs_on_same_line_first() {
        // "yes and true" — cursor on "yes" (col 0)
        assert_eq!(new_text(&actions("yes and true", 0)), "no");
    }

    #[test]
    fn mixed_pairs_on_same_line_second() {
        // "yes and true" — cursor on "true" (col 8)
        assert_eq!(new_text(&actions("yes and true", 8)), "false");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // WorkspaceEdit range correctness
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn range_for_true_is_exact() {
        // "x = true;" — 'true' at cols 4–7, end = 4+4 = 8
        let a = actions("x = true;", 4);
        let e = &a[0]["edit"]["changes"][URI][0];
        assert_eq!(e["range"]["start"]["character"], 4);
        assert_eq!(e["range"]["end"]["character"], 8);
        assert_eq!(e["newText"], "false");
    }

    #[test]
    fn range_for_false_is_exact() {
        // "x = false;" — 'false' at cols 4–8, end = 4+5 = 9
        let a = actions("x = false;", 4);
        let e = &a[0]["edit"]["changes"][URI][0];
        assert_eq!(e["range"]["start"]["character"], 4);
        assert_eq!(e["range"]["end"]["character"], 9);
        assert_eq!(e["newText"], "true");
    }

    #[test]
    fn range_for_on_to_off() {
        // "state = on" — 'on' at cols 8–9, end = 8+2 = 10
        let a = actions("state = on", 8);
        let e = &a[0]["edit"]["changes"][URI][0];
        assert_eq!(e["range"]["start"]["character"], 8);
        assert_eq!(e["range"]["end"]["character"], 10);
        assert_eq!(e["newText"], "off");
    }

    #[test]
    fn range_for_off_to_on() {
        // "state = off" — 'off' at cols 8–10, end = 8+3 = 11
        let a = actions("state = off", 8);
        let e = &a[0]["edit"]["changes"][URI][0];
        assert_eq!(e["range"]["start"]["character"], 8);
        assert_eq!(e["range"]["end"]["character"], 11);
        assert_eq!(e["newText"], "on");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Code-action metadata
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn action_is_preferred() {
        assert_eq!(actions("x = true;", 4)[0]["isPreferred"], true);
    }

    #[test]
    fn action_kind_is_refactor_rewrite() {
        assert_eq!(actions("x = true;", 4)[0]["kind"], "refactor.rewrite");
    }

    #[test]
    fn action_title_shows_matched_text_and_replacement() {
        // Title must show what's there and what it becomes.
        let title = actions("x = TRUE;", 4)[0]["title"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(title.contains("TRUE"), "title should contain matched token");
        assert!(title.contains("FALSE"), "title should contain replacement");
        assert!(title.contains("→"), "title should have arrow");
    }

    #[test]
    fn action_title_reflects_case_for_yes_no() {
        let title = actions("flag: YES", 6)[0]["title"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(title.contains("YES") && title.contains("NO"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Case-style detection
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn detect_lower() {
        assert_eq!(detect_case_style("true"), CaseStyle::Lower);
        assert_eq!(detect_case_style("false"), CaseStyle::Lower);
        assert_eq!(detect_case_style("yes"), CaseStyle::Lower);
        assert_eq!(detect_case_style("on"), CaseStyle::Lower);
    }

    #[test]
    fn detect_upper() {
        assert_eq!(detect_case_style("TRUE"), CaseStyle::Upper);
        assert_eq!(detect_case_style("FALSE"), CaseStyle::Upper);
        assert_eq!(detect_case_style("YES"), CaseStyle::Upper);
        assert_eq!(detect_case_style("OFF"), CaseStyle::Upper);
    }

    #[test]
    fn detect_title() {
        assert_eq!(detect_case_style("True"), CaseStyle::Title);
        assert_eq!(detect_case_style("False"), CaseStyle::Title);
        assert_eq!(detect_case_style("Yes"), CaseStyle::Title);
        assert_eq!(detect_case_style("On"), CaseStyle::Title);
        assert_eq!(detect_case_style("Off"), CaseStyle::Title);
    }

    #[test]
    fn detect_mixed() {
        assert_eq!(detect_case_style("tRuE"), CaseStyle::Mixed);
        assert_eq!(detect_case_style("fAlSe"), CaseStyle::Mixed);
    }

    #[test]
    fn detect_numeric_is_lower() {
        // Numerics have no case; Lower signals "apply as-is"
        assert_eq!(detect_case_style("1"), CaseStyle::Lower);
        assert_eq!(detect_case_style("0"), CaseStyle::Lower);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Case-style application
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn apply_lower() {
        assert_eq!(apply_case_style("false", CaseStyle::Lower), "false");
        assert_eq!(apply_case_style("off", CaseStyle::Lower), "off");
        assert_eq!(apply_case_style("no", CaseStyle::Lower), "no");
        assert_eq!(apply_case_style("0", CaseStyle::Lower), "0");
    }

    #[test]
    fn apply_upper() {
        assert_eq!(apply_case_style("false", CaseStyle::Upper), "FALSE");
        assert_eq!(apply_case_style("off", CaseStyle::Upper), "OFF");
        assert_eq!(apply_case_style("no", CaseStyle::Upper), "NO");
        assert_eq!(apply_case_style("0", CaseStyle::Upper), "0");
    }

    #[test]
    fn apply_title() {
        assert_eq!(apply_case_style("false", CaseStyle::Title), "False");
        assert_eq!(apply_case_style("off", CaseStyle::Title), "Off");
        assert_eq!(apply_case_style("true", CaseStyle::Title), "True");
        assert_eq!(apply_case_style("yes", CaseStyle::Title), "Yes");
        assert_eq!(apply_case_style("no", CaseStyle::Title), "No");
    }

    #[test]
    fn apply_mixed_falls_back_to_lower() {
        assert_eq!(apply_case_style("false", CaseStyle::Mixed), "false");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Server dispatch
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn initialize_returns_capabilities() {
        let mut s = Server::new();
        let resp = s
            .dispatch(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
            .unwrap();
        assert_eq!(resp["id"], 1);
        assert!(resp["result"]["capabilities"]["codeActionProvider"].is_object());
    }

    #[test]
    fn initialized_is_a_notification() {
        let mut s = Server::new();
        assert!(s
            .dispatch(json!({"jsonrpc":"2.0","method":"initialized","params":{}}))
            .is_none());
    }

    #[test]
    fn unknown_request_returns_method_not_found() {
        let mut s = Server::new();
        let resp = s
            .dispatch(json!({"jsonrpc":"2.0","id":9,"method":"foo/bar","params":{}}))
            .unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn unknown_notification_is_silently_ignored() {
        let mut s = Server::new();
        assert!(s
            .dispatch(json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{}}))
            .is_none());
    }

    #[test]
    fn did_open_stores_document() {
        let mut s = Server::new();
        s.dispatch(json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen",
            "params":{"textDocument":{"uri":"file:///a.ts","text":"let x = true;"}}
        }));
        assert!(s.documents.contains_key("file:///a.ts"));
    }

    #[test]
    fn did_change_updates_document() {
        let mut s = Server::new();
        s.dispatch(json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen",
            "params":{"textDocument":{"uri":"file:///b.ts","text":"old"}}
        }));
        s.dispatch(json!({
            "jsonrpc":"2.0","method":"textDocument/didChange",
            "params":{
                "textDocument":{"uri":"file:///b.ts","version":2},
                "contentChanges":[{"text":"new content"}]
            }
        }));
        assert_eq!(s.documents["file:///b.ts"], "new content");
    }

    #[test]
    fn did_close_removes_document() {
        let mut s = Server::new();
        s.documents.insert("file:///c.ts".into(), "x".into());
        s.dispatch(json!({
            "jsonrpc":"2.0","method":"textDocument/didClose",
            "params":{"textDocument":{"uri":"file:///c.ts"}}
        }));
        assert!(!s.documents.contains_key("file:///c.ts"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Transport
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn round_trip_message() {
        let msg = json!({"jsonrpc":"2.0","method":"initialized","params":{}});
        let mut buf: Vec<u8> = Vec::new();
        write_message(&mut buf, &msg);
        let parsed = read_message(&mut Cursor::new(buf)).unwrap();
        assert_eq!(parsed["method"], "initialized");
    }

    #[test]
    fn read_ignores_content_type_header() {
        let body = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        let frame = format!(
            "Content-Length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}",
            body.len(), body
        );
        let parsed = read_message(&mut Cursor::new(frame.as_bytes())).unwrap();
        assert_eq!(parsed["method"], "initialized");
    }
}
