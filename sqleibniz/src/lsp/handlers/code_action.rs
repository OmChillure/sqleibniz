use std::collections::HashMap;

use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionParams, Position, Range, TextEdit, WorkspaceEdit,
};

use crate::{error::Error, lsp::error::LspError};

pub fn handle(
    connection: &Connection,
    errors: &[Error],
    id: RequestId,
    params: CodeActionParams,
) -> Result<(), LspError> {
    eprintln!("got code action request #{id}");

    let request_range = params.range;
    let uri = params.text_document.uri;

    let mut actions: Vec<CodeAction> = Vec::new();

    for error in errors {
        let diag_range = Range::new(
            Position {
                line: error.line as u32,
                character: error.start as u32,
            },
            Position {
                line: error.line as u32,
                character: error.end as u32,
            },
        );

        // Only include actions whose diagnostic range overlaps the request range
        if !ranges_overlap(&diag_range, &request_range) {
            continue;
        }

        if let Some(ref suggestion) = error.suggestion {
            if suggestion.replacement.is_empty() && suggestion.message.starts_with("Add a WHERE") {
                // informational suggestion — no text edit
                continue;
            }
            let edit_range = Range::new(
                Position {
                    line: suggestion.start_line as u32,
                    character: suggestion.start_col as u32,
                },
                Position {
                    line: suggestion.end_line as u32,
                    character: suggestion.end_col as u32,
                },
            );
            let text_edit = TextEdit {
                range: edit_range,
                new_text: suggestion.replacement.clone(),
            };

            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![text_edit]);

            actions.push(CodeAction {
                title: suggestion.message.clone(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![error.clone().into()]),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                is_preferred: Some(true),
                ..Default::default()
            });
        }
    }

    let result = serde_json::to_value(&actions).unwrap();
    let resp = Response {
        id,
        result: Some(result),
        error: None,
    };
    connection
        .sender
        .send(Message::Response(resp))
        .map_err(|_| "failed to send code actions")?;
    Ok(())
}

fn ranges_overlap(a: &Range, b: &Range) -> bool {
    !(a.end.line < b.start.line
        || (a.end.line == b.start.line && a.end.character < b.start.character)
        || b.end.line < a.start.line
        || (b.end.line == a.start.line && b.end.character < a.start.character))
}