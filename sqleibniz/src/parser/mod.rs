use nodes::{BindParameter, SchemaTableContainer};

#[cfg(feature = "trace")]
use sqleibniz_proc::trace;

use crate::{
    error::{Error, ImprovedLine, Suggestion},
    parser::nodes::{
        ColumnConstraint, CommonTableExpr, CompoundOp, CompoundSelect, ForeignKeyAction,
        ForeignKeyClause, ForeignKeyMatch, FrameBound, FrameExclude, FrameSpec, FromClause,
        IndexedBy, JoinConstraint, JoinItem, JoinType, NamedWindowDef, OrderingTerm, Pragma,
        ResultColumn, SelectCore, SqlExpr, TableRef, TableSource, WindowOver, WindowSpec,
    },
    types::{Keyword, Token, Type, rules::Rule, storage::SqliteStorageClass},
};

/// implement serialisation manually for all nodes and contained types
pub mod debug;
/// nodes holds all abstract syntax tree nodes, the node! macro, the lua preparation for the plugin execution and the sqleibniz analysis
pub mod nodes;
mod tests;

// this sucks but is necessary to track the call depth for indentation when printing the parser
// stack
#[cfg(feature = "trace")]
thread_local! {
    static CALL_DEPTH: std::cell::Cell<usize> = std::cell::Cell::new(0);
}

pub struct Parser<'a> {
    pos: usize,
    tokens: Vec<Token>,
    name: &'a str,
    pub errors: Vec<Error>,
}

/// wrap argument in Some(Box::new(_))
macro_rules! some_box {
    ($expr:expr) => {
        Some(Box::new($expr) as Box<dyn nodes::Node>)
    };
}

/// Function naming directly corresponds to the sqlite3 documentation of sql syntax.
///
/// ## See:
///
/// - https://www.sqlite.org/lang.html
/// - https://www.sqlite.org/lang_expr.html
impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token>, name: &'a str) -> Parser<'a> {
        Parser {
            pos: 0,
            name,
            tokens,
            errors: vec![],
        }
    }

    fn cur(&self) -> &Token {
        if let Some(tok) = self.tokens.get(self.pos) {
            tok
        } else {
            &Token {
                ttype: Type::Eof,
                start: 0,
                end: 0,
                line: 0,
            }
        }
    }

    fn err(&self, msg: impl Into<String>, note: &str, start: &Token, rule: Rule) -> Error {
        Error {
            improved_line: None,
            file: self.name.to_string(),
            line: start.line,
            rule,
            note: note.into(),
            msg: msg.into(),
            start: start.start,
            end: start.end,
            doc_url: None,
            suggestion: None,
        }
    }

    fn push_err(&mut self, msg: impl Into<String>, note: &str, start: &Token, rule: Rule) {
        let err = self.err(msg, note, start, rule);
        self.errors.push(err);
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn advance(&mut self) {
        if !self.is_eof() {
            self.pos += 1
        }
    }

    fn is(&mut self, t: Type) -> bool {
        self.cur().ttype == t
    }

    fn is_keyword(&mut self, keyword: Keyword) -> bool {
        self.cur().ttype == Type::Keyword(keyword)
    }

    fn skip_until_semicolon_or_eof(&mut self) {
        while !self.is_eof() && !self.is(Type::Semicolon) {
            self.advance();
        }
    }

    /// checks if type of current token is equal to t, otherwise pushs an error, advances either way
    fn consume(&mut self, t: Type) {
        let tt = t.clone();
        if !self.is(tt) {
            let cur = self.cur().clone();
            let mut err = self.err(
                match cur.ttype {
                    Type::Eof => "Unexpected End of input",
                    _ => "Unexpected Token",
                },
                &format!("Wanted {:?}, got {:?}", t, cur.ttype),
                &cur,
                Rule::Syntax,
            );
            if t == Type::Semicolon {
                err.msg = "Missing semicolon".into();
                err.note.push_str(", terminate statements with ';'");
                err.rule = Rule::Semicolon;
                err.improved_line = Some(ImprovedLine {
                    snippet: ";",
                    start: self.cur().end,
                });
                err.suggestion = Some(Suggestion {
                    message: "Add `;` to terminate the statement".into(),
                    replacement: ";".into(),
                    start_line: cur.line,
                    start_col: cur.start,
                    end_line: cur.line,
                    end_col: cur.start,
                });
            }
            err.doc_url = Some("https://www.sqlite.org/syntax/sql-stmt.html");
            self.errors.push(err);
        }
        self.advance(); // we advance either way to keep the parser error resistant
    }

    fn consume_keyword(&mut self, keyword: Keyword) {
        self.consume(Type::Keyword(keyword));
    }

    fn next_is(&self, t: Type) -> bool {
        self.tokens
            .get(self.pos + 1)
            .is_some_and(|tok| tok.ttype == t)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    /// checks if current token is semicolon, if not pushes Rule::Syntax
    fn expect_end(&mut self, doc: &'static str) -> Option<()> {
        if !self.is(Type::Semicolon) {
            let cur = self.cur().clone();
            let mut err = self.err(
                "Unexpected Statement Continuation",
                &format!("Expected statement end via Semicolon, got {:?}", cur.ttype),
                &cur,
                Rule::Syntax,
            );
            if !doc.is_empty() {
                err.doc_url = Some(doc);
            }
            self.errors.push(err);
            self.advance();
        }
        None
    }

    fn consume_ident(
        &mut self,
        doc: &'static str,
        expected_ident_name: &'static str,
    ) -> Option<String> {
        if let Type::Ident(ident) = &self.cur().ttype {
            let i = ident.to_string();
            self.advance();
            Some(i)
        } else {
            let cur = self.cur().clone();
            let mut err = self.err(
                "Unexpected Token",
                &format!(
                    "Expected Ident(<{}>), got {:?}",
                    expected_ident_name, cur.ttype
                ),
                &cur,
                Rule::Syntax,
            );
            err.doc_url = Some(doc);
            self.errors.push(err);
            self.advance();
            None
        }
    }

    #[cfg_attr(feature = "trace", trace)]
    pub fn parse(&mut self) -> Vec<Box<dyn nodes::Node>> {
        self.sql_stmt_list()
    }

    /// see: https://www.sqlite.org/syntax/sql-stmt-list.html
    #[cfg_attr(feature = "trace", trace)]
    fn sql_stmt_list(&mut self) -> Vec<Box<dyn nodes::Node>> {
        let mut r = vec![];
        while !self.is_eof() {
            if let Token {
                ttype: Type::InstructionExpect,
                ..
            } = self.cur()
            {
                // skip all token until the statement ends
                self.skip_until_semicolon_or_eof();
                // only consume ; if we arent at an eof, otherwise we want the last comment of a
                // file to end with a ; which doesnt make sense
                if !self.is_eof() {
                    // skip ';'
                    self.consume(Type::Semicolon);
                    continue;
                }
            }
            if let Some(stmt) = self.sql_stmt_prefix() {
                r.push(stmt);
            }
            self.consume(Type::Semicolon);
        }
        r
    }

    #[cfg_attr(feature = "trace", trace)]
    fn sql_stmt_prefix(&mut self) -> Option<Box<dyn nodes::Node>> {
        let r: Option<Box<dyn nodes::Node>> = match self.cur().ttype {
            Type::Keyword(Keyword::EXPLAIN) => {
                let t = self.cur().clone();
                // skip EXPLAIN
                self.advance();

                // path for EXPLAIN->QUERY->PLAN
                if self.is(Type::Keyword(Keyword::QUERY)) {
                    self.advance();
                    self.consume(Type::Keyword(Keyword::PLAN));
                }

                // else path is EXPLAIN->*_stmt
                some_box!(nodes::Explain {
                    t,
                    child: self.sql_stmt()?,
                })
            }
            _ => self.sql_stmt(),
        };

        r
    }

    /// see: https://www.sqlite.org/syntax/sql-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn sql_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        match self.cur().ttype {
            Type::Keyword(Keyword::SELECT)
            | Type::Keyword(Keyword::WITH)
            | Type::Keyword(Keyword::VALUES) => self.select_stmt(),
            Type::Keyword(Keyword::PRAGMA) => self.pragma_stmt(),
            Type::Keyword(Keyword::ALTER) => self.alter_stmt(),
            Type::Keyword(Keyword::ATTACH) => self.attach_stmt(),
            Type::Keyword(Keyword::REINDEX) => self.reindex_stmt(),
            Type::Keyword(Keyword::RELEASE) => self.release_stmt(),
            Type::Keyword(Keyword::SAVEPOINT) => self.savepoint_stmt(),
            Type::Keyword(Keyword::DROP) => self.drop_stmt(),
            Type::Keyword(Keyword::ANALYZE) => self.analyse_stmt(),
            Type::Keyword(Keyword::DETACH) => self.detach_stmt(),
            Type::Keyword(Keyword::ROLLBACK) => self.rollback_stmt(),
            Type::Keyword(Keyword::COMMIT) | Type::Keyword(Keyword::END) => self.commit_stmt(),
            Type::Keyword(Keyword::BEGIN) => self.begin_stmt(),
            Type::Keyword(Keyword::VACUUM) => self.vacuum_stmt(),
            Type::Keyword(Keyword::CREATE) => self.create_stmt(),
            Type::Keyword(Keyword::INSERT) | Type::Keyword(Keyword::REPLACE) => {
                self.insert_stmt()
            }
            Type::Keyword(Keyword::UPDATE) => self.update_stmt(),
            Type::Keyword(Keyword::DELETE) => self.delete_stmt(),

            // statement should not start with a semicolon 󰚌
            Type::Semicolon => {
                self.push_err(
                    "Unexpected Token",
                    "Semicolon makes no sense at this point, Semicolons are used to terminate statements",
                    &self.cur().clone(),
                    Rule::Syntax,
                );
                self.advance();
                None
            }

            // explicitly disallowing literals at this point: results in clearer and more
            // understandable error messages
            Type::String(_)
            | Type::Number(_)
            | Type::Blob(_)
            | Type::Keyword(Keyword::NULL)
            | Type::Boolean(_)
            | Type::Keyword(Keyword::CURRENT_TIME)
            | Type::Keyword(Keyword::CURRENT_DATE)
            | Type::Keyword(Keyword::CURRENT_TIMESTAMP) => {
                let mut err = self.err(
                    "Unexpected Literal",
                    &format!("Literal {:?} can not start a statement", self.cur().ttype),
                    self.cur(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/syntax/sql-stmt.html");
                self.errors.push(err);
                self.advance();
                None
            }
            Type::Ident(ref name) => {
                let suggestions = Keyword::suggestions(name);
                if !suggestions.is_empty() {
                    let cur = self.cur().clone();
                    let mut err = self.err(
                        "Unknown Keyword",
                        &format!(
                            "'{}' is not an SQL keyword, did you mean one of: {}",
                            name,
                            suggestions.join(", ").as_str()
                        ),
                        &cur,
                        Rule::UnknownKeyword,
                    );
                    err.doc_url = Some("https://sqlite.org/lang_keywords.html");
                    err.suggestion = Some(Suggestion {
                        message: format!("Replace `{}` with `{}`", name, suggestions[0]),
                        replacement: suggestions[0].to_string(),
                        start_line: cur.line,
                        start_col: cur.start,
                        end_line: cur.line,
                        end_col: cur.end,
                    });
                    self.errors.push(err);
                } else {
                    self.push_err(
                        "Unknown Keyword",
                        &format!("'{name}' is not a keyword"),
                        &self.cur().clone(),
                        Rule::UnknownKeyword,
                    );
                };
                self.advance();
                None
            }
            Type::Keyword(_) => {
                let cur = self.cur().clone();
                self.push_err(
                    "Unimplemented",
                    &format!("sqleibniz can not yet analyse the token {:?}", cur.ttype,),
                    &cur,
                    Rule::Unimplemented,
                );
                self.advance();
                None
            }
            _ => {
                let cur = self.cur().clone();
                self.push_err(
                    "Unknown Token",
                    &format!(
                        "sqleibniz does not understand the token {:?}, skipping ahead to next statement",
                        cur.ttype
                    ),
                    &cur,
                    Rule::Unimplemented,
                );
                self.skip_until_semicolon_or_eof();
                None
            }
        }
    }

    // ── SELECT statement ─────────────────────────────────────────────────────

    /// https://www.sqlite.org/lang_select.html
    fn select_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let select = self.select_stmt_inner()?;
        self.expect_end("https://www.sqlite.org/lang_select.html");
        some_box!(select)
    }

    fn select_stmt_inner(&mut self) -> Option<nodes::SelectStmt> {
        let t = self.cur().clone();

        // ── WITH clause (CTEs) ──
        let mut ctes = vec![];
        let mut recursive = false;
        if self.is_keyword(Keyword::WITH) {
            self.advance();
            if self.is_keyword(Keyword::RECURSIVE) {
                recursive = true;
                self.advance();
            }
            loop {
                ctes.push(self.common_table_expr()?);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
        }

        // ── First select core ──
        let first = self.select_core()?;

        // ── Compound selects (UNION / INTERSECT / EXCEPT) ──
        let mut rest = vec![];
        loop {
            let op = if self.is_keyword(Keyword::UNION) {
                self.advance();
                if self.is_keyword(Keyword::ALL) {
                    self.advance();
                    CompoundOp::UnionAll
                } else {
                    CompoundOp::Union
                }
            } else if self.is_keyword(Keyword::INTERSECT) {
                self.advance();
                CompoundOp::Intersect
            } else if self.is_keyword(Keyword::EXCEPT) {
                self.advance();
                CompoundOp::Except
            } else {
                break;
            };
            let core = self.select_core()?;
            rest.push((op, core));
        }

        // ── ORDER BY ──
        let mut order_by = vec![];
        if self.is_keyword(Keyword::ORDER) {
            self.advance();
            self.consume_keyword(Keyword::BY);
            loop {
                order_by.push(self.ordering_term()?);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
        }

        // ── LIMIT / OFFSET ──
        let mut limit = None;
        let mut offset = None;
        if self.is_keyword(Keyword::LIMIT) {
            self.advance();
            limit = Some(self.sql_expr()?);
            if self.is_keyword(Keyword::OFFSET) {
                self.advance();
                offset = Some(self.sql_expr()?);
            } else if self.is(Type::Comma) {
                self.advance();
                // LIMIT x,y  ⟹  LIMIT y OFFSET x  (SQLite quirk)
                offset = limit;
                limit = Some(self.sql_expr()?);
            }
        }

        Some(nodes::SelectStmt {
            t,
            ctes,
            recursive,
            body: CompoundSelect { first, rest },
            order_by,
            limit,
            offset,
        })
    }

    fn select_core(&mut self) -> Option<SelectCore> {
        // ── VALUES clause ──
        if self.is_keyword(Keyword::VALUES) {
            self.advance();
            let mut rows = vec![];
            loop {
                self.consume(Type::BraceLeft);
                let mut row = vec![];
                if !self.is(Type::BraceRight) {
                    row.push(self.sql_expr()?);
                    while self.is(Type::Comma) {
                        self.advance();
                        row.push(self.sql_expr()?);
                    }
                }
                self.consume(Type::BraceRight);
                rows.push(row);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
            return Some(SelectCore {
                distinct: None,
                columns: vec![],
                from: None,
                where_clause: None,
                group_by: vec![],
                having: None,
                windows: vec![],
                is_values: true,
                values_rows: rows,
            });
        }

        // ── SELECT ──
        self.consume_keyword(Keyword::SELECT);

        let distinct = if self.is_keyword(Keyword::DISTINCT) {
            self.advance();
            Some(Keyword::DISTINCT)
        } else if self.is_keyword(Keyword::ALL) {
            self.advance();
            Some(Keyword::ALL)
        } else {
            None
        };

        // ── Result columns ──
        let mut columns = vec![];
        loop {
            // detect trailing comma: comma followed by FROM / WHERE / GROUP / etc.
            if !columns.is_empty()
                && matches!(
                    self.cur().ttype,
                    Type::Keyword(Keyword::FROM)
                        | Type::Keyword(Keyword::WHERE)
                        | Type::Keyword(Keyword::GROUP)
                        | Type::Keyword(Keyword::ORDER)
                        | Type::Keyword(Keyword::LIMIT)
                        | Type::Keyword(Keyword::UNION)
                        | Type::Keyword(Keyword::INTERSECT)
                        | Type::Keyword(Keyword::EXCEPT)
                        | Type::Keyword(Keyword::WINDOW)
                        | Type::Semicolon
                )
            {
                let cur = self.cur().clone();
                let mut err = self.err(
                    "Trailing comma",
                    "Remove the trailing comma before this keyword",
                    &cur,
                    Rule::TrailingComma,
                );
                err.suggestion = Some(Suggestion {
                    message: "Remove the trailing comma".into(),
                    replacement: String::new(),
                    start_line: cur.line,
                    start_col: cur.start.saturating_sub(2),
                    end_line: cur.line,
                    end_col: cur.start,
                });
                self.errors.push(err);
                break;
            }
            columns.push(self.result_column()?);
            if !self.is(Type::Comma) {
                break;
            }
            self.advance();
        }

        // ── FROM ──
        let from = if self.is_keyword(Keyword::FROM) {
            self.advance();
            Some(self.from_clause()?)
        } else {
            None
        };

        // ── WHERE ──
        let where_clause = if self.is_keyword(Keyword::WHERE) {
            self.advance();
            Some(self.sql_expr()?)
        } else {
            None
        };

        // ── GROUP BY / HAVING ──
        let mut group_by = vec![];
        let mut having = None;
        if self.is_keyword(Keyword::GROUP) {
            self.advance();
            self.consume_keyword(Keyword::BY);
            loop {
                group_by.push(self.sql_expr()?);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
            if self.is_keyword(Keyword::HAVING) {
                self.advance();
                having = Some(self.sql_expr()?);
            }
        }

        // ── WINDOW ──
        let mut windows = vec![];
        if self.is_keyword(Keyword::WINDOW) {
            self.advance();
            loop {
                let wname = self.consume_ident(
                    "https://www.sqlite.org/lang_select.html",
                    "window_name",
                )?;
                self.consume_keyword(Keyword::AS);
                self.consume(Type::BraceLeft);
                let spec = self.window_spec()?;
                self.consume(Type::BraceRight);
                windows.push(NamedWindowDef {
                    name: wname,
                    def: spec,
                });
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
        }

        Some(SelectCore {
            distinct,
            columns,
            from,
            where_clause,
            group_by,
            having,
            windows,
            is_values: false,
            values_rows: vec![],
        })
    }

    fn result_column(&mut self) -> Option<ResultColumn> {
        // *
        if self.is(Type::Asterisk) {
            self.advance();
            return Some(ResultColumn::Star);
        }

        // table.* — check for ident.dot.star
        if let Type::Ident(_) = &self.cur().ttype {
            if self.next_is(Type::Dot) {
                if self
                    .peek_at(2)
                    .is_some_and(|t| t.ttype == Type::Asterisk)
                {
                    let name = match &self.cur().ttype {
                        Type::Ident(n) => n.clone(),
                        _ => unreachable!(),
                    };
                    self.advance(); // ident
                    self.advance(); // dot
                    self.advance(); // *
                    return Some(ResultColumn::TableStar(name));
                }
            }
        }

        let expr = self.sql_expr()?;

        // optional alias
        let alias = if self.is_keyword(Keyword::AS) {
            self.advance();
            if let Type::Ident(n) = &self.cur().ttype {
                let n = n.clone();
                self.advance();
                Some(n)
            } else if let Type::String(n) = &self.cur().ttype {
                let n = n.clone();
                self.advance();
                Some(n)
            } else {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected alias name",
                    &format!("expected identifier after AS, got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                None
            }
        } else if let Type::Ident(n) = &self.cur().ttype {
            // implicit alias (no AS keyword) — only bare identifiers
            let n = n.clone();
            self.advance();
            Some(n)
        } else {
            None
        };

        Some(ResultColumn::Expr { expr, alias })
    }

    // ── FROM clause parsing ──────────────────────────────────────────────────

    fn from_clause(&mut self) -> Option<FromClause> {
        let first = self.table_ref()?;
        let mut joins = vec![];

        loop {
            if self.is(Type::Comma) {
                self.advance();
                let table = self.table_ref()?;
                joins.push(JoinItem {
                    natural: false,
                    join_type: JoinType::Comma,
                    table,
                    constraint: None,
                });
            } else if self.is_join_start() {
                joins.push(self.join_item()?);
            } else {
                break;
            }
        }

        Some(FromClause { first, joins })
    }

    fn is_join_start(&self) -> bool {
        matches!(
            self.cur().ttype,
            Type::Keyword(Keyword::JOIN)
                | Type::Keyword(Keyword::INNER)
                | Type::Keyword(Keyword::LEFT)
                | Type::Keyword(Keyword::RIGHT)
                | Type::Keyword(Keyword::FULL)
                | Type::Keyword(Keyword::CROSS)
                | Type::Keyword(Keyword::NATURAL)
        )
    }

    fn join_item(&mut self) -> Option<JoinItem> {
        let natural = if self.is_keyword(Keyword::NATURAL) {
            self.advance();
            true
        } else {
            false
        };

        let join_type = if self.is_keyword(Keyword::LEFT) {
            self.advance();
            if self.is_keyword(Keyword::OUTER) {
                self.advance();
            }
            self.consume_keyword(Keyword::JOIN);
            JoinType::Left
        } else if self.is_keyword(Keyword::RIGHT) {
            self.advance();
            if self.is_keyword(Keyword::OUTER) {
                self.advance();
            }
            self.consume_keyword(Keyword::JOIN);
            JoinType::Right
        } else if self.is_keyword(Keyword::FULL) {
            self.advance();
            if self.is_keyword(Keyword::OUTER) {
                self.advance();
            }
            self.consume_keyword(Keyword::JOIN);
            JoinType::Full
        } else if self.is_keyword(Keyword::CROSS) {
            self.advance();
            self.consume_keyword(Keyword::JOIN);
            JoinType::Cross
        } else if self.is_keyword(Keyword::INNER) {
            self.advance();
            self.consume_keyword(Keyword::JOIN);
            JoinType::Inner
        } else if self.is_keyword(Keyword::JOIN) {
            self.advance();
            JoinType::Inner
        } else {
            let cur = self.cur().clone();
            self.push_err(
                "Expected JOIN keyword",
                &format!("got {:?}", cur.ttype),
                &cur,
                Rule::Syntax,
            );
            self.advance();
            JoinType::Inner
        };

        let table = self.table_ref()?;

        let constraint = if self.is_keyword(Keyword::ON) {
            self.advance();
            Some(JoinConstraint::On(self.sql_expr()?))
        } else if self.is_keyword(Keyword::USING) {
            self.advance();
            self.consume(Type::BraceLeft);
            let mut cols = vec![];
            loop {
                cols.push(self.consume_ident(
                    "https://www.sqlite.org/lang_select.html",
                    "column_name",
                )?);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
            self.consume(Type::BraceRight);
            Some(JoinConstraint::Using(cols))
        } else {
            None
        };

        Some(JoinItem {
            natural,
            join_type,
            table,
            constraint,
        })
    }

    fn table_ref(&mut self) -> Option<TableRef> {
        let ref_token = self.cur().clone();
        let source;
        if self.is(Type::BraceLeft) {
            self.advance();
            if self.is_keyword(Keyword::SELECT)
                || self.is_keyword(Keyword::WITH)
                || self.is_keyword(Keyword::VALUES)
            {
                // subquery
                let sel = self.select_stmt_inner()?;
                self.consume(Type::BraceRight);
                source = TableSource::Subquery(Box::new(sel));
            } else {
                // parenthesized FROM clause
                let inner = self.from_clause()?;
                self.consume(Type::BraceRight);
                source = TableSource::ParenFrom(Box::new(inner));
            }
        } else {
            // [schema.]table_name or table_function(args)
            let name1 = match &self.cur().ttype {
                Type::Ident(n) => n.clone(),
                Type::String(n) => n.clone(),
                _ => {
                    let cur = self.cur().clone();
                    self.push_err(
                        "Expected table name",
                        &format!("expected table name, got {:?}", cur.ttype),
                        &cur,
                        Rule::Syntax,
                    );
                    self.advance();
                    return None;
                }
            };
            self.advance();

            if self.is(Type::Dot) {
                // schema.table or schema.func()
                self.advance();
                let name2 = match &self.cur().ttype {
                    Type::Ident(n) | Type::String(n) => n.clone(),
                    _ => {
                        let cur = self.cur().clone();
                        self.push_err(
                            "Expected table name after schema",
                            &format!("got {:?}", cur.ttype),
                            &cur,
                            Rule::Syntax,
                        );
                        self.advance();
                        return None;
                    }
                };
                self.advance();

                if self.is(Type::BraceLeft) {
                    // schema.func(args)
                    self.advance();
                    let mut args = vec![];
                    if !self.is(Type::BraceRight) {
                        args.push(self.sql_expr()?);
                        while self.is(Type::Comma) {
                            self.advance();
                            args.push(self.sql_expr()?);
                        }
                    }
                    self.consume(Type::BraceRight);
                    source = TableSource::TableFunction {
                        schema: Some(name1),
                        name: name2,
                        args,
                    };
                } else {
                    source = TableSource::Table {
                        schema: Some(name1),
                        name: name2,
                    };
                }
            } else if self.is(Type::BraceLeft) {
                // table_function(args) without schema
                self.advance();
                let mut args = vec![];
                if !self.is(Type::BraceRight) {
                    args.push(self.sql_expr()?);
                    while self.is(Type::Comma) {
                        self.advance();
                        args.push(self.sql_expr()?);
                    }
                }
                self.consume(Type::BraceRight);
                source = TableSource::TableFunction {
                    schema: None,
                    name: name1,
                    args,
                };
            } else {
                source = TableSource::Table {
                    schema: None,
                    name: name1,
                };
            }
        }

        // optional alias
        let alias = if self.is_keyword(Keyword::AS) {
            self.advance();
            if let Type::Ident(n) = &self.cur().ttype {
                let n = n.clone();
                self.advance();
                Some(n)
            } else if let Type::String(n) = &self.cur().ttype {
                let n = n.clone();
                self.advance();
                Some(n)
            } else {
                None
            }
        } else if let Type::Ident(n) = &self.cur().ttype {
            // implicit alias — but only if the ident is not a keyword-ish
            // that starts a clause (ON, JOIN, WHERE, etc.)
            if !self.is_join_start()
                && !matches!(
                    self.cur().ttype,
                    Type::Keyword(Keyword::WHERE)
                        | Type::Keyword(Keyword::GROUP)
                        | Type::Keyword(Keyword::HAVING)
                        | Type::Keyword(Keyword::ORDER)
                        | Type::Keyword(Keyword::LIMIT)
                        | Type::Keyword(Keyword::UNION)
                        | Type::Keyword(Keyword::INTERSECT)
                        | Type::Keyword(Keyword::EXCEPT)
                        | Type::Keyword(Keyword::ON)
                        | Type::Keyword(Keyword::USING)
                        | Type::Keyword(Keyword::WINDOW)
                )
            {
                let n = n.clone();
                self.advance();
                Some(n)
            } else {
                None
            }
        } else {
            None
        };

        // optional INDEXED BY / NOT INDEXED
        let indexed = if self.is_keyword(Keyword::INDEXED) {
            self.advance();
            self.consume_keyword(Keyword::BY);
            let idx_name = self.consume_ident(
                "https://www.sqlite.org/lang_select.html",
                "index_name",
            )?;
            Some(IndexedBy::Indexed(idx_name))
        } else if self.is_keyword(Keyword::NOT) && self.next_is(Type::Keyword(Keyword::INDEXED)) {
            self.advance();
            self.advance();
            Some(IndexedBy::NotIndexed)
        } else {
            None
        };

        Some(TableRef {
            token: ref_token,
            source,
            alias,
            indexed,
        })
    }

    // ── Ordering term ────────────────────────────────────────────────────────

    fn ordering_term(&mut self) -> Option<OrderingTerm> {
        let expr = self.sql_expr()?;

        let collation = if self.is_keyword(Keyword::COLLATE) {
            self.advance();
            Some(self.consume_ident(
                "https://www.sqlite.org/lang_select.html",
                "collation_name",
            )?)
        } else {
            None
        };

        let asc_desc =
            if let Type::Keyword(k @ (Keyword::ASC | Keyword::DESC)) = &self.cur().ttype {
                let k = *k;
                self.advance();
                Some(k)
            } else {
                None
            };

        let nulls = if self.is_keyword(Keyword::NULLS) {
            self.advance();
            if let Type::Keyword(k @ (Keyword::FIRST | Keyword::LAST)) = &self.cur().ttype {
                let k = *k;
                self.advance();
                Some(k)
            } else {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected FIRST or LAST",
                    &format!("got {:?} after NULLS", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                None
            }
        } else {
            None
        };

        Some(OrderingTerm {
            expr,
            collation,
            asc_desc,
            nulls,
        })
    }

    // ── CTE parsing ──────────────────────────────────────────────────────────

    fn common_table_expr(&mut self) -> Option<CommonTableExpr> {
        let name = self.consume_ident(
            "https://www.sqlite.org/lang_select.html",
            "cte_name",
        )?;

        let mut columns = vec![];
        if self.is(Type::BraceLeft) {
            self.advance();
            loop {
                columns.push(self.consume_ident(
                    "https://www.sqlite.org/lang_select.html",
                    "column_name",
                )?);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
            self.consume(Type::BraceRight);
        }

        self.consume_keyword(Keyword::AS);

        let materialized = if self.is_keyword(Keyword::NOT) {
            self.advance();
            self.consume_keyword(Keyword::MATERIALIZED);
            Some(false)
        } else if self.is_keyword(Keyword::MATERIALIZED) {
            self.advance();
            Some(true)
        } else {
            None
        };

        self.consume(Type::BraceLeft);
        let select = self.select_stmt_inner()?;
        self.consume(Type::BraceRight);

        Some(CommonTableExpr {
            name,
            columns,
            materialized,
            select: Box::new(select),
        })
    }

    // ── Window parsing ───────────────────────────────────────────────────────

    fn window_over(&mut self) -> Option<WindowOver> {
        if self.is(Type::BraceLeft) {
            self.advance();
            let spec = self.window_spec()?;
            self.consume(Type::BraceRight);
            Some(WindowOver::Spec(spec))
        } else if let Type::Ident(name) = &self.cur().ttype {
            let name = name.clone();
            self.advance();
            Some(WindowOver::Name(name))
        } else {
            let cur = self.cur().clone();
            self.push_err(
                "Expected window specification",
                &format!("got {:?}", cur.ttype),
                &cur,
                Rule::Syntax,
            );
            None
        }
    }

    fn window_spec(&mut self) -> Option<WindowSpec> {
        let base_window = if let Type::Ident(n) = &self.cur().ttype {
            if !matches!(
                self.cur().ttype,
                Type::Keyword(Keyword::PARTITION)
                    | Type::Keyword(Keyword::ORDER)
                    | Type::Keyword(Keyword::RANGE)
                    | Type::Keyword(Keyword::ROWS)
                    | Type::Keyword(Keyword::GROUPS)
            ) {
                let n = n.clone();
                self.advance();
                Some(n)
            } else {
                None
            }
        } else {
            None
        };

        let mut partition_by = vec![];
        if self.is_keyword(Keyword::PARTITION) {
            self.advance();
            self.consume_keyword(Keyword::BY);
            loop {
                partition_by.push(self.sql_expr()?);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
        }

        let mut order_by = vec![];
        if self.is_keyword(Keyword::ORDER) {
            self.advance();
            self.consume_keyword(Keyword::BY);
            loop {
                order_by.push(self.ordering_term()?);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
        }

        let frame = if matches!(
            self.cur().ttype,
            Type::Keyword(Keyword::RANGE)
                | Type::Keyword(Keyword::ROWS)
                | Type::Keyword(Keyword::GROUPS)
        ) {
            Some(self.frame_spec()?)
        } else {
            None
        };

        Some(WindowSpec {
            base_window,
            partition_by,
            order_by,
            frame,
        })
    }

    fn frame_spec(&mut self) -> Option<FrameSpec> {
        let mode = match &self.cur().ttype {
            Type::Keyword(k @ (Keyword::RANGE | Keyword::ROWS | Keyword::GROUPS)) => {
                let k = *k;
                self.advance();
                k
            }
            _ => {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected RANGE, ROWS, or GROUPS",
                    &format!("got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                return None;
            }
        };

        let (start, end) = if self.is_keyword(Keyword::BETWEEN) {
            self.advance();
            let s = Box::new(self.frame_bound()?);
            self.consume_keyword(Keyword::AND);
            let e = Box::new(self.frame_bound()?);
            (s, Some(e))
        } else {
            (Box::new(self.frame_bound()?), None)
        };

        let exclude = if self.is_keyword(Keyword::EXCLUDE) {
            self.advance();
            if self.is_keyword(Keyword::NO) {
                self.advance();
                self.consume_keyword(Keyword::OTHERS);
                Some(FrameExclude::NoOthers)
            } else if self.is_keyword(Keyword::CURRENT) {
                self.advance();
                self.consume_keyword(Keyword::ROW);
                Some(FrameExclude::CurrentRow)
            } else if self.is_keyword(Keyword::GROUP) {
                self.advance();
                Some(FrameExclude::Group)
            } else if self.is_keyword(Keyword::TIES) {
                self.advance();
                Some(FrameExclude::Ties)
            } else {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected EXCLUDE option",
                    &format!("got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                None
            }
        } else {
            None
        };

        Some(FrameSpec {
            mode,
            start,
            end,
            exclude,
        })
    }

    fn frame_bound(&mut self) -> Option<FrameBound> {
        if self.is_keyword(Keyword::UNBOUNDED) {
            self.advance();
            if self.is_keyword(Keyword::PRECEDING) {
                self.advance();
                Some(FrameBound::UnboundedPreceding)
            } else if self.is_keyword(Keyword::FOLLOWING) {
                self.advance();
                Some(FrameBound::UnboundedFollowing)
            } else {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected PRECEDING or FOLLOWING",
                    &format!("got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                None
            }
        } else if self.is_keyword(Keyword::CURRENT) {
            self.advance();
            self.consume_keyword(Keyword::ROW);
            Some(FrameBound::CurrentRow)
        } else {
            let expr = self.sql_expr()?;
            if self.is_keyword(Keyword::PRECEDING) {
                self.advance();
                Some(FrameBound::Preceding(Box::new(expr)))
            } else if self.is_keyword(Keyword::FOLLOWING) {
                self.advance();
                Some(FrameBound::Following(Box::new(expr)))
            } else {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected PRECEDING or FOLLOWING",
                    &format!("got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                None
            }
        }
    }

    // ── Expression parser (full SQLite expression grammar) ───────────────────

    fn sql_expr(&mut self) -> Option<SqlExpr> {
        self.sql_expr_or()
    }

    fn sql_expr_or(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_and()?;
        while self.is_keyword(Keyword::OR) {
            let op = self.cur().clone();
            self.advance();
            let right = self.sql_expr_and()?;
            left = SqlExpr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn sql_expr_and(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_not()?;
        while self.is_keyword(Keyword::AND) {
            let op = self.cur().clone();
            self.advance();
            let right = self.sql_expr_not()?;
            left = SqlExpr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn sql_expr_not(&mut self) -> Option<SqlExpr> {
        if self.is_keyword(Keyword::NOT) {
            let op = self.cur().clone();
            self.advance();
            let operand = self.sql_expr_not()?;
            return Some(SqlExpr::UnaryOp {
                op,
                operand: Box::new(operand),
            });
        }
        self.sql_expr_equality()
    }

    fn sql_expr_equality(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_comparison()?;

        loop {
            // IS [NOT] NULL / IS [NOT] expr
            if self.is_keyword(Keyword::IS) {
                self.advance();
                let negated = if self.is_keyword(Keyword::NOT) {
                    self.advance();
                    true
                } else {
                    false
                };
                if self.is_keyword(Keyword::NULL) {
                    self.advance();
                    left = SqlExpr::IsNull {
                        expr: Box::new(left),
                        negated,
                    };
                } else {
                    // IS [NOT] expr — treat as binary comparison
                    let right = self.sql_expr_comparison()?;
                    let op_tok = Token {
                        ttype: Type::Keyword(if negated {
                            Keyword::ISNULL
                        } else {
                            Keyword::IS
                        }),
                        ..self.cur().clone()
                    };
                    left = SqlExpr::BinaryOp {
                        left: Box::new(left),
                        op: op_tok,
                        right: Box::new(right),
                    };
                }
                continue;
            }

            // ISNULL / NOTNULL
            if self.is_keyword(Keyword::ISNULL) {
                self.advance();
                left = SqlExpr::IsNull {
                    expr: Box::new(left),
                    negated: false,
                };
                continue;
            }
            if self.is_keyword(Keyword::NOTNULL) {
                self.advance();
                left = SqlExpr::IsNull {
                    expr: Box::new(left),
                    negated: true,
                };
                continue;
            }

            // [NOT] IN / LIKE / GLOB / REGEXP / MATCH / BETWEEN
            let negated = if self.is_keyword(Keyword::NOT) {
                if let Some(tok) = self.peek_at(1) {
                    if matches!(
                        tok.ttype,
                        Type::Keyword(
                            Keyword::IN
                                | Keyword::LIKE
                                | Keyword::GLOB
                                | Keyword::REGEXP
                                | Keyword::MATCH
                                | Keyword::BETWEEN
                        )
                    ) {
                        self.advance();
                        true
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                false
            };

            if self.is_keyword(Keyword::IN) {
                self.advance();
                self.consume(Type::BraceLeft);
                if self.is_keyword(Keyword::SELECT)
                    || self.is_keyword(Keyword::WITH)
                    || self.is_keyword(Keyword::VALUES)
                {
                    let sel = self.select_stmt_inner()?;
                    self.consume(Type::BraceRight);
                    left = SqlExpr::InSelect {
                        expr: Box::new(left),
                        negated,
                        subquery: Box::new(sel),
                    };
                } else {
                    let mut values = vec![];
                    if !self.is(Type::BraceRight) {
                        values.push(self.sql_expr()?);
                        while self.is(Type::Comma) {
                            self.advance();
                            values.push(self.sql_expr()?);
                        }
                    }
                    self.consume(Type::BraceRight);
                    left = SqlExpr::InList {
                        expr: Box::new(left),
                        negated,
                        values,
                    };
                }
                continue;
            }

            if matches!(
                self.cur().ttype,
                Type::Keyword(
                    Keyword::LIKE | Keyword::GLOB | Keyword::REGEXP | Keyword::MATCH
                )
            ) {
                let op = match &self.cur().ttype {
                    Type::Keyword(k) => *k,
                    _ => unreachable!(),
                };
                self.advance();
                let pattern = self.sql_expr_comparison()?;
                let escape = if self.is_keyword(Keyword::ESCAPE) {
                    self.advance();
                    Some(Box::new(self.sql_expr_comparison()?))
                } else {
                    None
                };
                left = SqlExpr::Like {
                    expr: Box::new(left),
                    negated,
                    op,
                    pattern: Box::new(pattern),
                    escape,
                };
                continue;
            }

            if self.is_keyword(Keyword::BETWEEN) {
                self.advance();
                let low = self.sql_expr_comparison()?;
                self.consume_keyword(Keyword::AND);
                let high = self.sql_expr_comparison()?;
                left = SqlExpr::Between {
                    expr: Box::new(left),
                    negated,
                    low: Box::new(low),
                    high: Box::new(high),
                };
                continue;
            }

            // = == != <>
            if self.is(Type::Equal)
                || self.is(Type::BangEqual)
                || self.is(Type::LessGreater)
            {
                let op = self.cur().clone();
                self.advance();
                let right = self.sql_expr_comparison()?;
                left = SqlExpr::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
                continue;
            }

            break;
        }

        Some(left)
    }

    fn sql_expr_comparison(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_bitwise()?;
        while self.is(Type::Less)
            || self.is(Type::LessEqual)
            || self.is(Type::Greater)
            || self.is(Type::GreaterEqual)
        {
            let op = self.cur().clone();
            self.advance();
            let right = self.sql_expr_bitwise()?;
            left = SqlExpr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn sql_expr_bitwise(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_addition()?;
        while self.is(Type::ShiftLeft)
            || self.is(Type::ShiftRight)
            || self.is(Type::Ampersand)
            || self.is(Type::Pipe)
        {
            let op = self.cur().clone();
            self.advance();
            let right = self.sql_expr_addition()?;
            left = SqlExpr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn sql_expr_addition(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_multiplication()?;
        while self.is(Type::Plus) || self.is(Type::Minus) {
            let op = self.cur().clone();
            self.advance();
            let right = self.sql_expr_multiplication()?;
            left = SqlExpr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn sql_expr_multiplication(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_concat()?;
        while self.is(Type::Asterisk) || self.is(Type::Slash) || self.is(Type::Percent) {
            let op = self.cur().clone();
            self.advance();
            let right = self.sql_expr_concat()?;
            left = SqlExpr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn sql_expr_concat(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_unary()?;
        while self.is(Type::PipePipe) {
            let op = self.cur().clone();
            self.advance();
            let right = self.sql_expr_unary()?;
            left = SqlExpr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn sql_expr_unary(&mut self) -> Option<SqlExpr> {
        if self.is(Type::Minus) || self.is(Type::Plus) || self.is(Type::Tilde) {
            let op = self.cur().clone();
            self.advance();
            let operand = self.sql_expr_unary()?;
            return Some(SqlExpr::UnaryOp {
                op,
                operand: Box::new(operand),
            });
        }
        self.sql_expr_postfix()
    }

    fn sql_expr_postfix(&mut self) -> Option<SqlExpr> {
        let mut left = self.sql_expr_atom()?;
        while self.is_keyword(Keyword::COLLATE) {
            self.advance();
            let collation = self.consume_ident(
                "https://www.sqlite.org/lang_expr.html",
                "collation_name",
            )?;
            left = SqlExpr::Collate {
                expr: Box::new(left),
                collation,
            };
        }
        Some(left)
    }

    fn sql_expr_atom(&mut self) -> Option<SqlExpr> {
        match self.cur().ttype.clone() {
            // Literals
            Type::String(_)
            | Type::Number(_)
            | Type::Blob(_)
            | Type::Boolean(_)
            | Type::Keyword(Keyword::NULL)
            | Type::Keyword(Keyword::CURRENT_TIME)
            | Type::Keyword(Keyword::CURRENT_DATE)
            | Type::Keyword(Keyword::CURRENT_TIMESTAMP) => {
                let tok = self.cur().clone();
                self.advance();
                Some(SqlExpr::Literal(tok))
            }

            // Identifiers: column ref, table.column, schema.table.column, or function call
            Type::Ident(name) => {
                let ident_token = self.cur().clone();
                self.advance();

                // function call: name(...)
                if self.is(Type::BraceLeft) {
                    return self.sql_expr_function_call(name);
                }

                // dotted: table.column or schema.table.column
                if self.is(Type::Dot) {
                    self.advance();
                    if let Type::Ident(name2) = self.cur().ttype.clone() {
                        let col_token = self.cur().clone();
                        self.advance();
                        if self.is(Type::Dot) {
                            self.advance();
                            if let Type::Ident(name3) = self.cur().ttype.clone() {
                                let col3_token = self.cur().clone();
                                self.advance();
                                return Some(SqlExpr::ColumnRef {
                                    token: col3_token,
                                    schema: Some(name),
                                    table: Some(name2),
                                    column: name3,
                                });
                            }
                        }
                        return Some(SqlExpr::ColumnRef {
                            token: col_token,
                            schema: None,
                            table: Some(name),
                            column: name2,
                        });
                    } else if let Type::Asterisk = &self.cur().ttype {
                        // table.* inside expression context (e.g. EXISTS check)
                        self.advance();
                        return Some(SqlExpr::ColumnRef {
                            token: ident_token,
                            schema: None,
                            table: Some(name),
                            column: "*".into(),
                        });
                    }
                }

                Some(SqlExpr::ColumnRef {
                    token: ident_token,
                    schema: None,
                    table: None,
                    column: name,
                })
            }

            // Parenthesized expression or subquery
            Type::BraceLeft => {
                self.advance();
                if self.is_keyword(Keyword::SELECT)
                    || self.is_keyword(Keyword::WITH)
                    || self.is_keyword(Keyword::VALUES)
                {
                    let sel = self.select_stmt_inner()?;
                    self.consume(Type::BraceRight);
                    return Some(SqlExpr::Subquery(Box::new(sel)));
                }
                let expr = self.sql_expr()?;
                self.consume(Type::BraceRight);
                Some(SqlExpr::Paren(Box::new(expr)))
            }

            // CAST(expr AS type)
            Type::Keyword(Keyword::CAST) => {
                self.advance();
                self.consume(Type::BraceLeft);
                let expr = self.sql_expr()?;
                self.consume_keyword(Keyword::AS);
                let type_name = self.consume_ident(
                    "https://www.sqlite.org/lang_expr.html",
                    "type_name",
                )?;
                self.consume(Type::BraceRight);
                Some(SqlExpr::Cast {
                    expr: Box::new(expr),
                    type_name,
                })
            }

            // CASE
            Type::Keyword(Keyword::CASE) => {
                self.advance();
                let operand = if !self.is_keyword(Keyword::WHEN) {
                    Some(Box::new(self.sql_expr()?))
                } else {
                    None
                };
                let mut when_clauses = vec![];
                while self.is_keyword(Keyword::WHEN) {
                    self.advance();
                    let w = self.sql_expr()?;
                    self.consume_keyword(Keyword::THEN);
                    let t = self.sql_expr()?;
                    when_clauses.push((w, t));
                }
                let else_clause = if self.is_keyword(Keyword::ELSE) {
                    self.advance();
                    Some(Box::new(self.sql_expr()?))
                } else {
                    None
                };
                self.consume_keyword(Keyword::END);
                Some(SqlExpr::Case {
                    operand,
                    when_clauses,
                    else_clause,
                })
            }

            // EXISTS (subquery)
            Type::Keyword(Keyword::EXISTS) => {
                self.advance();
                self.consume(Type::BraceLeft);
                let sel = self.select_stmt_inner()?;
                self.consume(Type::BraceRight);
                Some(SqlExpr::Exists {
                    negated: false,
                    subquery: Box::new(sel),
                })
            }

            // Bind parameters
            Type::Question => {
                let tok = self.cur().clone();
                self.advance();
                let counter = if let Type::Number(_) = &self.cur().ttype {
                    let c = self.cur().clone();
                    self.advance();
                    Some(c)
                } else {
                    None
                };
                Some(SqlExpr::BindParam {
                    token: tok,
                    name: None,
                    counter,
                })
            }
            Type::Colon | Type::At | Type::Dollar => {
                let tok = self.cur().clone();
                self.advance();
                if let Type::Ident(n) = &self.cur().ttype {
                    let n = n.clone();
                    self.advance();
                    Some(SqlExpr::BindParam {
                        token: tok,
                        name: Some(n),
                        counter: None,
                    })
                } else {
                    let cur = self.cur().clone();
                    self.push_err(
                        "Invalid bind parameter",
                        &format!("expected identifier after bind prefix, got {:?}", cur.ttype),
                        &cur,
                        Rule::Syntax,
                    );
                    None
                }
            }

            // Star — used inside COUNT(*) etc.
            Type::Asterisk => {
                self.advance();
                Some(SqlExpr::Star)
            }

            _ => {
                let cur = self.cur().clone();
                self.push_err(
                    "Unexpected Token in expression",
                    &format!("expected expression, got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                None
            }
        }
    }

    fn sql_expr_function_call(&mut self, name: String) -> Option<SqlExpr> {
        self.advance(); // skip '('

        let distinct = if self.is_keyword(Keyword::DISTINCT) {
            self.advance();
            true
        } else {
            false
        };

        let mut args = vec![];
        if self.is(Type::Asterisk) {
            args.push(SqlExpr::Star);
            self.advance();
        } else if !self.is(Type::BraceRight) {
            args.push(self.sql_expr()?);
            while self.is(Type::Comma) {
                self.advance();
                args.push(self.sql_expr()?);
            }
        }
        self.consume(Type::BraceRight);

        // FILTER / OVER for window functions
        let filter = if self.is_keyword(Keyword::FILTER) {
            self.advance();
            self.consume(Type::BraceLeft);
            self.consume_keyword(Keyword::WHERE);
            let f = self.sql_expr()?;
            self.consume(Type::BraceRight);
            Some(Box::new(f))
        } else {
            None
        };

        if self.is_keyword(Keyword::OVER) {
            self.advance();
            let over = self.window_over()?;
            return Some(SqlExpr::WindowFunctionCall {
                name,
                distinct,
                args,
                filter,
                over: Box::new(over),
            });
        }

        Some(SqlExpr::FunctionCall {
            name,
            distinct,
            args,
        })
    }

    /// https://www.sqlite.org/lang_createtable.html
    #[cfg_attr(feature = "trace", trace)]
    fn create_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let t = self.cur().clone();
        // skip CREATE
        self.advance();

        let temporary = if self.is_keyword(Keyword::TEMP) || self.is_keyword(Keyword::TEMPORARY) {
            self.advance();
            true
        } else {
            false
        };

        // Only TABLE is supported right now
        if !self.is_keyword(Keyword::TABLE) {
            let cur = self.cur().clone();
            self.push_err(
                "Unimplemented",
                &format!(
                    "CREATE {:?} is not yet supported by sqleibniz, only CREATE TABLE",
                    cur.ttype
                ),
                &cur,
                Rule::Unimplemented,
            );
            self.skip_until_semicolon_or_eof();
            return None;
        }
        self.advance(); // skip TABLE

        let if_not_exists = if self.is_keyword(Keyword::IF) {
            self.advance();
            self.consume_keyword(Keyword::NOT);
            self.consume_keyword(Keyword::EXISTS);
            true
        } else {
            false
        };

        // [schema.]table_name
        let (schema, name) = match self.cur().ttype.clone() {
            Type::Ident(first) if self.next_is(Type::Dot) => {
                self.advance(); // skip schema
                self.advance(); // skip dot
                if let Type::Ident(tbl) = &self.cur().ttype {
                    let tbl = tbl.clone();
                    self.advance();
                    (Some(first), tbl)
                } else {
                    let cur = self.cur().clone();
                    self.push_err(
                        "Expected table name",
                        &format!("expected table name after schema., got {:?}", cur.ttype),
                        &cur,
                        Rule::Syntax,
                    );
                    self.skip_until_semicolon_or_eof();
                    return None;
                }
            }
            Type::Ident(tbl) => {
                self.advance();
                (None, tbl)
            }
            _ => {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected table name",
                    &format!("expected table name, got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                self.skip_until_semicolon_or_eof();
                return None;
            }
        };

        // AS select_stmt  (CREATE TABLE ... AS SELECT ...)
        if self.is_keyword(Keyword::AS) {
            self.advance();
            let _sel = self.select_stmt_inner();
            self.expect_end("https://www.sqlite.org/lang_createtable.html");
            return some_box!(nodes::CreateTable {
                t,
                if_not_exists,
                schema,
                name,
                columns: vec![],
                temporary,
            });
        }

        self.consume(Type::BraceLeft);

        let mut columns = vec![];
        if !self.is(Type::BraceRight) {
            if let Some(col) = self.column_def() {
                columns.push(col);
            }
            while self.is(Type::Comma) {
                self.advance();
                // skip table constraints for now (PRIMARY KEY(...), UNIQUE(...), etc.)
                if matches!(
                    self.cur().ttype,
                    Type::Keyword(Keyword::PRIMARY)
                        | Type::Keyword(Keyword::UNIQUE)
                        | Type::Keyword(Keyword::CHECK)
                        | Type::Keyword(Keyword::FOREIGN)
                        | Type::Keyword(Keyword::CONSTRAINT)
                ) {
                    // skip to matching ')' by counting braces
                    let mut depth = 0i32;
                    loop {
                        if self.is_eof() {
                            break;
                        }
                        if self.is(Type::BraceLeft) {
                            depth += 1;
                        }
                        if self.is(Type::BraceRight) {
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                        }
                        if self.is(Type::Comma) && depth == 0 {
                            // another table constraint or column
                            break;
                        }
                        self.advance();
                    }
                    continue;
                }
                if self.is(Type::BraceRight) {
                    break;
                }
                if let Some(col) = self.column_def() {
                    columns.push(col);
                }
            }
        }

        self.consume(Type::BraceRight);

        // optional WITHOUT ROWID
        if self.is_keyword(Keyword::WITHOUT) {
            self.advance();
            // skip ROWID (it's an ident, not a keyword)
            if let Type::Ident(ref s) = self.cur().ttype {
                if s.to_uppercase() == "ROWID" {
                    self.advance();
                }
            }
        }

        // optional STRICT
        if let Type::Ident(ref s) = self.cur().ttype {
            if s.to_uppercase() == "STRICT" {
                self.advance();
            }
        }

        self.expect_end("https://www.sqlite.org/lang_createtable.html");

        some_box!(nodes::CreateTable {
            t,
            if_not_exists,
            schema,
            name,
            columns,
            temporary,
        })
    }

    /// https://www.sqlite.org/lang_insert.html
    #[cfg_attr(feature = "trace", trace)]
    fn insert_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let t = self.cur().clone();

        // INSERT OR action | REPLACE
        let or_action = if self.is_keyword(Keyword::REPLACE) {
            self.advance();
            Some(Keyword::REPLACE)
        } else {
            self.advance(); // skip INSERT
            if self.is_keyword(Keyword::OR) {
                self.advance();
                let action = match &self.cur().ttype {
                    Type::Keyword(
                        k @ (Keyword::REPLACE
                        | Keyword::ABORT
                        | Keyword::ROLLBACK
                        | Keyword::FAIL
                        | Keyword::IGNORE),
                    ) => {
                        let k = *k;
                        self.advance();
                        Some(k)
                    }
                    _ => {
                        let cur = self.cur().clone();
                        self.push_err(
                            "Expected conflict action",
                            &format!(
                                "expected REPLACE, ABORT, ROLLBACK, FAIL, or IGNORE after INSERT OR, got {:?}",
                                cur.ttype
                            ),
                            &cur,
                            Rule::Syntax,
                        );
                        None
                    }
                };
                action
            } else {
                None
            }
        };

        self.consume_keyword(Keyword::INTO);

        // [schema.]table_name
        let table_token = self.cur().clone();
        let (schema, table) = match self.cur().ttype.clone() {
            Type::Ident(first) if self.next_is(Type::Dot) => {
                self.advance();
                self.advance();
                if let Type::Ident(tbl) = &self.cur().ttype {
                    let tbl = tbl.clone();
                    self.advance();
                    (Some(first), tbl)
                } else {
                    let cur = self.cur().clone();
                    self.push_err(
                        "Expected table name",
                        &format!("expected table name, got {:?}", cur.ttype),
                        &cur,
                        Rule::Syntax,
                    );
                    self.skip_until_semicolon_or_eof();
                    return None;
                }
            }
            Type::Ident(tbl) => {
                self.advance();
                (None, tbl)
            }
            _ => {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected table name",
                    &format!("expected table name after INTO, got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                self.skip_until_semicolon_or_eof();
                return None;
            }
        };

        // optional column list
        let mut columns = vec![];
        if self.is(Type::BraceLeft) && !self.next_is(Type::Keyword(Keyword::SELECT)) {
            // peek ahead to distinguish (col_list) from (SELECT ...)
            // simple heuristic: if next token after '(' is an ident and the one after that
            // is ',' or ')', it's a column list
            let looks_like_col_list = if let Some(tok) = self.peek_at(1) {
                matches!(tok.ttype, Type::Ident(_))
            } else {
                false
            };

            if looks_like_col_list {
                self.advance(); // skip '('
                loop {
                    columns.push(self.consume_ident(
                        "https://www.sqlite.org/lang_insert.html",
                        "column_name",
                    )?);
                    if !self.is(Type::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.consume(Type::BraceRight);
            }
        }

        // DEFAULT VALUES | VALUES (...) | select_stmt
        let mut values = vec![];
        let mut select = None;
        let mut default_values = false;

        if self.is_keyword(Keyword::DEFAULT) {
            self.advance();
            self.consume_keyword(Keyword::VALUES);
            default_values = true;
        } else if self.is_keyword(Keyword::VALUES) {
            self.advance();
            loop {
                self.consume(Type::BraceLeft);
                let mut row = vec![];
                if !self.is(Type::BraceRight) {
                    row.push(self.sql_expr()?);
                    while self.is(Type::Comma) {
                        self.advance();
                        row.push(self.sql_expr()?);
                    }
                }
                self.consume(Type::BraceRight);
                values.push(row);
                if !self.is(Type::Comma) {
                    break;
                }
                self.advance();
            }
        } else if self.is_keyword(Keyword::SELECT)
            || self.is_keyword(Keyword::WITH)
            || self.is_keyword(Keyword::VALUES)
        {
            select = Some(Box::new(self.select_stmt_inner()?));
        } else {
            let cur = self.cur().clone();
            self.push_err(
                "Expected VALUES, DEFAULT VALUES, or SELECT",
                &format!("got {:?}", cur.ttype),
                &cur,
                Rule::Syntax,
            );
            self.skip_until_semicolon_or_eof();
            return None;
        }

        self.expect_end("https://www.sqlite.org/lang_insert.html");

        some_box!(nodes::InsertStmt {
            t,
            or_action,
            schema,
            table,
            table_token,
            columns,
            values,
            select,
            default_values,
        })
    }

    /// https://www.sqlite.org/lang_update.html
    #[cfg_attr(feature = "trace", trace)]
    fn update_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let t = self.cur().clone();
        self.advance(); // skip UPDATE

        let or_action = if self.is_keyword(Keyword::OR) {
            self.advance();
            match &self.cur().ttype {
                Type::Keyword(
                    k @ (Keyword::REPLACE
                    | Keyword::ABORT
                    | Keyword::ROLLBACK
                    | Keyword::FAIL
                    | Keyword::IGNORE),
                ) => {
                    let k = *k;
                    self.advance();
                    Some(k)
                }
                _ => {
                    let cur = self.cur().clone();
                    self.push_err(
                        "Expected conflict action",
                        &format!("expected conflict action after UPDATE OR, got {:?}", cur.ttype),
                        &cur,
                        Rule::Syntax,
                    );
                    None
                }
            }
        } else {
            None
        };

        // [schema.]table_name
        let table_token = self.cur().clone();
        let (schema, table_name) = match self.cur().ttype.clone() {
            Type::Ident(first) if self.next_is(Type::Dot) => {
                self.advance();
                self.advance();
                if let Type::Ident(tbl) = &self.cur().ttype {
                    let tbl = tbl.clone();
                    self.advance();
                    (Some(first), tbl)
                } else {
                    let cur = self.cur().clone();
                    self.push_err(
                        "Expected table name",
                        &format!("expected table name, got {:?}", cur.ttype),
                        &cur,
                        Rule::Syntax,
                    );
                    self.skip_until_semicolon_or_eof();
                    return None;
                }
            }
            Type::Ident(tbl) => {
                self.advance();
                (None, tbl)
            }
            _ => {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected table name",
                    &format!("expected table name after UPDATE, got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                self.skip_until_semicolon_or_eof();
                return None;
            }
        };

        // optional alias
        let alias = if self.is_keyword(Keyword::AS) {
            self.advance();
            self.consume_ident("https://www.sqlite.org/lang_update.html", "alias")
        } else if !self.is_keyword(Keyword::SET) {
            if let Type::Ident(n) = &self.cur().ttype {
                let n = n.clone();
                self.advance();
                Some(n)
            } else {
                None
            }
        } else {
            None
        };

        self.consume_keyword(Keyword::SET);

        // SET column = expr [, ...]
        let mut set_clauses = vec![];
        loop {
            let column_token = self.cur().clone();
            let col = self.consume_ident(
                "https://www.sqlite.org/lang_update.html",
                "column_name",
            )?;
            self.consume(Type::Equal);
            let expr = self.sql_expr()?;
            set_clauses.push(nodes::SetClause {
                column: col,
                column_token,
                expr,
            });
            if !self.is(Type::Comma) {
                break;
            }
            self.advance();
        }

        // optional FROM
        let from = if self.is_keyword(Keyword::FROM) {
            self.advance();
            Some(self.from_clause()?)
        } else {
            None
        };

        // optional WHERE
        let where_clause = if self.is_keyword(Keyword::WHERE) {
            self.advance();
            Some(self.sql_expr()?)
        } else {
            None
        };

        self.expect_end("https://www.sqlite.org/lang_update.html");

        some_box!(nodes::UpdateStmt {
            t,
            or_action,
            schema,
            table: table_name,
            table_token,
            alias,
            set_clauses,
            from,
            where_clause,
        })
    }

    /// https://www.sqlite.org/lang_delete.html
    #[cfg_attr(feature = "trace", trace)]
    fn delete_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let t = self.cur().clone();
        self.advance(); // skip DELETE
        self.consume_keyword(Keyword::FROM);

        // [schema.]table_name
        let table_token = self.cur().clone();
        let (schema, table_name) = match self.cur().ttype.clone() {
            Type::Ident(first) if self.next_is(Type::Dot) => {
                self.advance();
                self.advance();
                if let Type::Ident(tbl) = &self.cur().ttype {
                    let tbl = tbl.clone();
                    self.advance();
                    (Some(first), tbl)
                } else {
                    let cur = self.cur().clone();
                    self.push_err(
                        "Expected table name",
                        &format!("expected table name, got {:?}", cur.ttype),
                        &cur,
                        Rule::Syntax,
                    );
                    self.skip_until_semicolon_or_eof();
                    return None;
                }
            }
            Type::Ident(tbl) => {
                self.advance();
                (None, tbl)
            }
            _ => {
                let cur = self.cur().clone();
                self.push_err(
                    "Expected table name",
                    &format!("expected table name after DELETE FROM, got {:?}", cur.ttype),
                    &cur,
                    Rule::Syntax,
                );
                self.skip_until_semicolon_or_eof();
                return None;
            }
        };

        // optional alias
        let alias = if self.is_keyword(Keyword::AS) {
            self.advance();
            self.consume_ident("https://www.sqlite.org/lang_delete.html", "alias")
        } else if !self.is_keyword(Keyword::WHERE) {
            if let Type::Ident(n) = &self.cur().ttype {
                let n = n.clone();
                self.advance();
                Some(n)
            } else {
                None
            }
        } else {
            None
        };

        // optional WHERE
        let where_clause = if self.is_keyword(Keyword::WHERE) {
            self.advance();
            Some(self.sql_expr()?)
        } else {
            None
        };

        self.expect_end("https://www.sqlite.org/lang_delete.html");

        some_box!(nodes::DeleteStmt {
            t,
            schema,
            table: table_name,
            table_token,
            alias,
            where_clause,
        })
    }

    /// https://www.sqlite.org/pragma.html
    #[cfg_attr(feature = "trace", trace)]
    fn pragma_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let t = self.cur().clone();

        // skip PRAGMA
        self.advance();

        // PRAGMA needs a target name
        let Some(schema_and_pragma) = self.schema_table_container(Some("pragma")) else {
            return None;
        };

        let pragma = if self.is(Type::Semicolon) {
            Pragma {
                t,
                name: schema_and_pragma,
                invocation: nodes::PragmaInvocation::Query,
            }
        } else if self.is(Type::Equal) {
            self.advance();
            match self.cur().ttype {
                Type::String(_) | Type::Number(_) | Type::Ident(_) | Type::Keyword(_) => {}
                _ => {
                    let cur = self.cur().clone();
                    self.push_err("Bad pragma value", &format!("A pragmas assignment value has to be either String, Number, Ident or a Keyword, got {:?} instead", cur.ttype), &cur, Rule::Syntax,);
                    self.advance();
                }
            }
            let p = Pragma {
                t,
                name: schema_and_pragma,
                invocation: nodes::PragmaInvocation::Assign {
                    value: self.cur().clone(),
                },
            };
            self.advance();
            p
        } else if self.is(Type::BraceLeft) {
            self.advance();
            match self.cur().ttype {
                Type::String(_) | Type::Number(_) | Type::Ident(_) | Type::Keyword(_) => {}
                _ => {
                    let cur = self.cur().clone();
                    self.push_err("Bad pragma value", &format!("A pragmas call value has to be either String, Number, Ident or a Keyword, got {:?} instead", cur.ttype), &cur, Rule::Syntax,);
                    self.advance();
                }
            }
            let p = Pragma {
                t,
                name: schema_and_pragma,
                invocation: nodes::PragmaInvocation::Call {
                    value: self.cur().clone(),
                },
            };
            self.advance();
            self.consume(Type::BraceRight);
            p
        } else {
            let cur = self.cur().clone();
            self.push_err(
                "Bad pragma value",
                &format!(
                    "A pragmas rhs value has to be either an assignment via '=', a call via '(<arg>)' or simply be a query, got {:?} instead",
                    cur.ttype
                ),
                &cur,
                Rule::Syntax,
            );
            self.advance();
            return None;
        };

        self.expect_end("https://www.sqlite.org/pragma.html");

        some_box!(pragma)
    }

    /// https://www.sqlite.org/lang_altertable.html
    #[cfg_attr(feature = "trace", trace)]
    fn alter_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let mut a = nodes::Alter {
            t: self.cur().clone(),
            target: SchemaTableContainer::Table(String::new()),
            rename_to: None,
            rename_column_target: None,
            new_column_name: None,
            add_column: None,
            drop_column: None,
        };

        self.advance();
        self.consume(Type::Keyword(Keyword::TABLE));

        a.target = self.schema_table_container(None)?;

        match self.cur().ttype {
            Type::Keyword(Keyword::RENAME) => {
                self.advance();
                if self.is(Type::Keyword(Keyword::TO)) {
                    // RENAME TO <new_table_name>
                    self.advance();
                    let new_table_name = self.consume_ident(
                        "https://www.sqlite.org/lang_altertable.html",
                        "new_table_name",
                    )?;
                    a.rename_to = Some(new_table_name);
                } else {
                    if self.is(Type::Keyword(Keyword::COLUMN)) {
                        self.advance();
                    }

                    a.rename_column_target = self.consume_ident(
                        "https://www.sqlite.org/lang_altertable.html",
                        "column_name",
                    );
                    self.consume(Type::Keyword(Keyword::TO));
                    a.new_column_name = self.consume_ident(
                        "https://www.sqlite.org/lang_altertable.html",
                        "column_name",
                    );
                }
            }
            Type::Keyword(Keyword::ADD) => {
                self.advance();
                if self.is(Type::Keyword(Keyword::COLUMN)) {
                    self.advance();
                }

                a.add_column = self.column_def();
            }
            Type::Keyword(Keyword::DROP) => {
                self.advance();
                if self.is(Type::Keyword(Keyword::COLUMN)) {
                    self.advance();
                }
                a.drop_column = self
                    .consume_ident("https://www.sqlite.org/lang_altertable.html", "column_name");
            }
            _ => {
                let mut err = self.err(
                    "Unexpected Token",
                    &format!(
                        "ALTER requires either RENAME, ADD or DROP at this point, got {:?}",
                        self.cur().ttype
                    ),
                    self.cur(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/lang_altertable.html");
                self.errors.push(err);
                self.advance();
                return None;
            }
        }

        self.expect_end("https://www.sqlite.org/lang_altertable.html");

        some_box!(a)
    }

    /// https://www.sqlite.org/syntax/reindex-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn reindex_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let mut r = nodes::Reindex {
            t: self.cur().clone(),
            target: None,
        };
        self.advance();

        // REINDEX has a path with no further nodes
        if self.is(Type::Semicolon) {
            return some_box!(r);
        }

        r.target = self.schema_table_container(None);

        self.expect_end("https://www.sqlite.org/syntax/reindex-stmt.html");

        some_box!(r)
    }

    /// https://www.sqlite.org/syntax/attach-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn attach_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let t = self.cur().clone();
        // skipping ATTACH
        self.advance();
        // skipping optional DATABASE
        if self.is(Type::Keyword(Keyword::DATABASE)) {
            self.advance();
        }

        let mut a = nodes::Attach {
            t,
            schema_name: String::new(),
            expr: self.expr()?,
        };

        self.consume(Type::Keyword(Keyword::AS));

        a.schema_name =
            self.consume_ident("https://www.sqlite.org/lang_attach.html", "schema_name")?;

        self.expect_end("https://www.sqlite.org/lang_attach.html");

        some_box!(a)
    }

    /// https://www.sqlite.org/syntax/release-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn release_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let mut r = nodes::Release {
            t: self.cur().clone(),
            savepoint_name: String::new(),
        };
        self.advance();

        if self.is(Type::Keyword(Keyword::SAVEPOINT)) {
            self.advance();
        }

        r.savepoint_name = self.consume_ident(
            "https://www.sqlite.org/syntax/release-stmt.html",
            "savepoint_name",
        )?;

        self.expect_end("https://www.sqlite.org/syntax/release-stmt.html");

        some_box!(r)
    }

    /// https://www.sqlite.org/syntax/savepoint-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn savepoint_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let mut s = nodes::Savepoint {
            t: self.cur().clone(),
            savepoint_name: String::new(),
        };
        self.advance();
        s.savepoint_name = self.consume_ident(
            "https://www.sqlite.org/syntax/savepoint-stmt.html",
            "savepoint_name",
        )?;
        self.expect_end("https://www.sqlite.org/lang_savepoint.html");

        some_box!(s)
    }

    /// https://www.sqlite.org/lang_dropindex.html
    /// https://www.sqlite.org/lang_droptable.html
    /// https://www.sqlite.org/lang_droptrigger.html
    /// https://www.sqlite.org/lang_dropview.html
    #[cfg_attr(feature = "trace", trace)]
    fn drop_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let t = self.cur().clone();
        self.advance();

        match self.cur().ttype {
            Type::Keyword(Keyword::INDEX) => (),
            Type::Keyword(Keyword::TABLE) => (),
            Type::Keyword(Keyword::TRIGGER) => (),
            Type::Keyword(Keyword::VIEW) => (),
            _ => {
                let mut err = self.err(
                        "Unexpected Token",
                        &format!(
                            "DROP requires either TRIGGER, TABLE, TRIGGER or VIEW at this point, got {:?}",
                            self.cur().ttype
                        ),
                        self.cur(),
                        Rule::Syntax,
                    );
                err.doc_url = Some("https://www.sqlite.org/lang.html");
                self.errors.push(err);
                self.advance();
                return None;
            }
        }

        let ttype = {
            let Type::Keyword(keyword) = &self.cur().ttype else {
                unreachable!("self.cur() in (in the set theory kind) {{INDEX,TABLE,TRIGGER,VIEW}}")
            };
            *keyword
        };

        // skip either INDEX;TABLE;TRIGGER or VIEW
        self.advance();

        let if_exists = if self.is(Type::Keyword(Keyword::IF)) {
            self.advance();
            self.consume(Type::Keyword(Keyword::EXISTS));
            true
        } else {
            false
        };

        let argument = self.schema_table_container(None)?;

        some_box!(nodes::Drop {
            t,
            ttype,
            if_exists,
            argument,
        })
    }

    /// https://www.sqlite.org/syntax/analyze-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn analyse_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let mut a = nodes::Analyze {
            t: self.cur().clone(),
            target: None,
        };

        self.advance();

        // inlined Parser::schema_table_container
        a.target = match self.cur().ttype.clone() {
            Type::Ident(schema) if self.next_is(Type::Dot) => {
                self.advance();
                self.advance();
                if let Type::Ident(table) = &self.cur().ttype {
                    let table = table.clone();
                    self.advance();
                    Some(SchemaTableContainer::SchemaAndTable { schema, table })
                } else if let Type::String(table) = &self.cur().ttype {
                    let table = table.clone();
                    self.advance();
                    Some(SchemaTableContainer::SchemaAndTable { schema, table })
                } else {
                    let cur = self.cur().clone();
                    self.errors.push(match cur.ttype {
                        Type::Keyword(keyword) => {
                            let as_str: &str = keyword.into();
                            self.err(
                            "Malformed table name",
                            &format!("`{as_str}` is a keyword, if you want to use it as a table or column name, quote it: '{as_str}'"),
                            &cur, Rule::Syntax)
                        }
                        _ => self.err(
                            "Malformed table name",
                            &format!(
                                "expected a table name after <schema_name>. - got {:?}",
                                cur.ttype
                            ),
                            &cur,
                            Rule::Syntax,
                        ),
                    });

                    // skip wrong token, should I skip to the next statement via
                    // self.skip_until_semicolon_or_eof?
                    self.advance();
                    None
                }
            }
            Type::Ident(table_name) | Type::String(table_name) => {
                // skip table_name
                self.advance();
                Some(SchemaTableContainer::Table(table_name))
            }
            _ => None,
        };

        self.expect_end("https://www.sqlite.org/lang_analyze.html");

        some_box!(a)
    }

    /// https://www.sqlite.org/syntax/detach-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn detach_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let t = self.cur().clone();
        self.advance();

        // skip optional DATABASE path
        if self.is(Type::Keyword(Keyword::DATABASE)) {
            self.advance();
        }

        let schema_name =
            self.consume_ident("https://www.sqlite.org/lang_detach.html", "schema_name")?;

        let d = nodes::Detach { t, schema_name };

        self.expect_end("https://www.sqlite.org/lang_detach.html");

        some_box!(d)
    }

    /// https://www.sqlite.org/syntax/rollback-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn rollback_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let mut rollback = nodes::Rollback {
            t: self.cur().clone(),
            save_point: None,
        };
        self.advance();

        match self.cur().ttype {
            Type::Keyword(Keyword::TRANSACTION) | Type::Keyword(Keyword::TO) | Type::Semicolon => {}
            _ => {
                let mut err = self.err(
                    "Unexpected Token",
                    &format!(
                        "ROLLBACK requires TRANSACTION, TO or to end at this point, got {:?}",
                        self.cur().ttype
                    ),
                    self.cur(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/lang_transaction.html");
                self.errors.push(err);
            }
        }

        // optional TRANSACTION
        if self.is(Type::Keyword(Keyword::TRANSACTION)) {
            self.advance();
        }

        // optional TO
        if self.is(Type::Keyword(Keyword::TO)) {
            self.advance();

            // optional SAVEPOINT
            if self.is(Type::Keyword(Keyword::SAVEPOINT)) {
                self.advance();
            }

            match self.cur().ttype {
                Type::Keyword(Keyword::SAVEPOINT) | Type::Ident(_) | Type::Semicolon => {}
                _ => {
                    let mut err = self.err(
                        "Unexpected Token",
                        &format!(
                            "ROLLBACK requires SAVEPOINT, Ident or to end at this point, got {:?}",
                            self.cur().ttype
                        ),
                        self.cur(),
                        Rule::Syntax,
                    );
                    err.doc_url = Some("https://www.sqlite.org/lang_transaction.html");
                    self.errors.push(err);
                    self.advance();
                }
            }

            if let Type::Ident(str) = &self.cur().ttype {
                rollback.save_point = Some(String::from(str));
            } else {
                let mut err = self.err(
                    "Unexpected Token",
                    &format!(
                        "ROLLBACK wants Ident as <savepoint-name>, got {:?}",
                        self.cur().ttype
                    ),
                    self.cur(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/lang_transaction.html");
                self.errors.push(err);
            }
            self.advance();
        }

        self.expect_end("https://www.sqlite.org/lang_transaction.html");

        some_box!(rollback)
    }

    /// https://www.sqlite.org/syntax/commit-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn commit_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let commit: Option<Box<dyn nodes::Node>> = some_box!(nodes::Commit {
            t: self.cur().clone(),
        });

        // skip either COMMIT or END
        self.advance();

        match self.cur().ttype {
            // expected end 1
            Type::Semicolon => (),
            // expected end 2, optional
            Type::Keyword(Keyword::TRANSACTION) => self.advance(),
            _ => {
                let mut err = self.err(
                    "Unexpected Token",
                    &format!(
                        "Wanted Keyword(TRANSACTION) or Semicolon, got {:?}",
                        self.cur().ttype
                    ),
                    self.cur(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/lang_transaction.html");
                self.errors.push(err);
                self.advance();
            }
        }

        self.expect_end("https://www.sqlite.org/lang_transaction.html");

        commit
    }

    /// https://www.sqlite.org/syntax/begin-stmt.html
    #[cfg_attr(feature = "trace", trace)]
    fn begin_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let mut begin: nodes::Begin = nodes::Begin {
            t: self.cur().clone(),
            transaction_kind: None,
        };

        // skip BEGIN
        self.advance();

        // skip modifiers
        match self.cur().ttype {
            // BEGIN;
            Type::Semicolon => return some_box!(begin),
            Type::Keyword(Keyword::DEFERRED)
            | Type::Keyword(Keyword::IMMEDIATE)
            | Type::Keyword(Keyword::EXCLUSIVE) => {
                begin.transaction_kind = if let Type::Keyword(word) = &self.cur().ttype {
                    Some(*word)
                } else {
                    None
                };
                self.advance()
            }
            _ => {}
        }

        match self.cur().ttype {
            Type::Semicolon => return some_box!(begin),
            // ending
            Type::Keyword(Keyword::TRANSACTION) => self.advance(),
            Type::Keyword(Keyword::DEFERRED)
            | Type::Keyword(Keyword::IMMEDIATE)
            | Type::Keyword(Keyword::EXCLUSIVE) => {
                let mut err = self.err(
                    "Unexpected Token",
                    "BEGIN does not allow multiple transaction behaviour modifiers",
                    self.cur(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/lang_transaction.html");
                self.errors.push(err);
                // TODO: think about if this is smart at this point, skipping to the next ; could
                // be skipping too many tokens
                self.skip_until_semicolon_or_eof();
            }
            _ => {
                let mut err = self.err(
                    "Unexpected Token",
                    &format!(
                        "Wanted any of TRANSACTION, DEFERRED, IMMEDIATE or EXCLUSIVE before this point, got {:?}",
                        self.cur().ttype
                    ),
                    self.cur(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/lang_transaction.html");
                self.errors.push(err);
            }
        }

        self.expect_end("https://www.sqlite.org/lang_transaction.html");

        some_box!(begin)
    }

    /// https://www.sqlite.org/lang_vacuum.html
    #[cfg_attr(feature = "trace", trace)]
    fn vacuum_stmt(&mut self) -> Option<Box<dyn nodes::Node>> {
        let mut v = nodes::Vacuum {
            t: self.cur().clone(),
            schema_name: None,
            filename: None,
        };
        self.consume(Type::Keyword(Keyword::VACUUM));

        match self.cur().ttype {
            Type::Semicolon | Type::Ident(_) | Type::Keyword(Keyword::INTO) => {}
            _ => {
                let mut err = self.err(
                    "Unexpected Token",
                    &format!(
                        "Wanted {:?} with {:?} or {:?} for VACUUM stmt, got {:?}",
                        Type::Keyword(Keyword::INTO),
                        Type::String("<filename>".to_string()),
                        Type::Ident("<schema_name>".to_string()),
                        self.cur().ttype.clone()
                    ),
                    &self.cur().clone(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/lang_vacuum.html");
                self.errors.push(err);
                self.advance(); // skip error_token
            }
        }

        // first path
        if let Type::Semicolon = self.cur().ttype {
            return some_box!(v);
        }

        // if schema_name is specified
        if let Type::Ident(_) = self.cur().ttype {
            v.schema_name = Some(self.cur().clone());
            self.advance(); // skip schema_name
        }

        // if INTO keyword is given is specified
        if let Type::Keyword(Keyword::INTO) = self.cur().ttype {
            self.advance(); // skip INTO
            if let Type::String(_) = self.cur().ttype {
                v.filename = Some(self.cur().clone());
            } else {
                let mut err = self.err(
                    "Unexpected Token",
                    &format!(
                        "Wanted {:?} for VACUUM stmt with {:?}, got {:?}",
                        Type::String("<filename>".to_string()),
                        Type::Keyword(Keyword::INTO),
                        self.cur().ttype.clone()
                    ),
                    &self.cur().clone(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/lang_vacuum.html");
                self.errors.push(err);
            }
            self.advance(); // skip filename or error token
        }

        self.expect_end("https://www.sqlite.org/lang_vacuum.html");

        some_box!(v)
    }

    /// see: https://www.sqlite.org/syntax/literal-value.html
    #[cfg_attr(feature = "trace", trace)]
    fn literal_value(&mut self) -> Option<Box<dyn nodes::Node>> {
        let cur = self.cur();
        match cur.ttype {
            Type::String(_)
            | Type::Number(_)
            | Type::Blob(_)
            | Type::Keyword(Keyword::NULL)
            | Type::Boolean(_)
            | Type::Keyword(Keyword::CURRENT_TIME)
            | Type::Keyword(Keyword::CURRENT_DATE)
            | Type::Keyword(Keyword::CURRENT_TIMESTAMP) => {
                let s: Option<Box<dyn nodes::Node>> = some_box!(nodes::Literal { t: cur.clone() });
                // skipping over the current character
                self.advance();
                s
            }
            _ => {
                let mut err = self.err("Unexpected Token", &format!("Wanted a literal (any of number,string,blob,null,true,false,CURRENT_TIME,CURRENT_DATE,CURRENT_DATE), got {:?}", cur.ttype),cur, Rule::Syntax);
                err.doc_url = Some("https://www.sqlite.org/syntax/literal-value.html");
                self.errors.push(err);
                self.advance();
                None
            }
        }
    }

    /// parses an sql expression: https://www.sqlite.org/syntax/expr.html
    fn expr(&mut self) -> Option<nodes::Expr> {
        let mut e = nodes::Expr {
            t: self.cur().clone(),
            literal: None,
            bind: None,
            schema: None,
            table: None,
            column: None,
        };
        match self.cur().ttype {
            // literal value
            Type::String(_)
            | Type::Number(_)
            | Type::Blob(_)
            | Type::Keyword(Keyword::NULL)
            | Type::Boolean(_)
            | Type::Keyword(Keyword::CURRENT_TIME)
            | Type::Keyword(Keyword::CURRENT_DATE)
            | Type::Keyword(Keyword::CURRENT_TIMESTAMP) => {
                e.literal = self.literal_value().map(|e| e.token().clone())
            }
            // bind parameter with optional ident: ?[ident]
            Type::Question => {
                // sqlite documentation says: But because it is easy to miscount the question marks, the
                // use of this parameter format is discouraged. Programmers are encouraged to use
                // one of the symbolic formats [...] or the ?NNN format [...] instead.
                let mut param = BindParameter {
                    t: self.cur().clone(),

                    counter: None,
                    name: None,
                };
                self.advance();

                // question mark can have a number after them, but they are optional
                if let Token {
                    ttype: Type::Number(_),
                    ..
                } = self.cur()
                {
                    param.counter = self.literal_value();
                }
                e.bind = Some(param)
            }
            // bind parameter with required ident: [:@$]<ident>
            Type::Colon | Type::At | Type::Dollar => {
                let mut bind = BindParameter {
                    t: self.cur().clone(),
                    counter: None,
                    name: None,
                };
                self.advance();

                // all bind params need an identifier, because they need to be named
                if let Token {
                    ttype: Type::Ident(ident),
                    ..
                } = self.cur()
                {
                    bind.name = Some(ident.clone());
                    self.advance();
                } else {
                    self.push_err(
                        "Invalid bind parameter",
                        &format!(
                            "Bind parameter with {:?} requires an identifier as a postfix",
                            bind.t.ttype
                        ),
                        &bind.t,
                        Rule::Syntax,
                    );
                    // skip invalid token
                    self.advance();
                    return None;
                }
                e.bind = Some(bind);
            }
            Type::Ident(_) => {
                // this is the start of a function
                if self.next_is(Type::BraceLeft) {
                    todo!("function-name(function-arguments) [filter-clause] [over-clause]")
                }

                // this sets either the schema, the table or the column
                todo!("[schema-name.][table-name.]<column-name>");
            }
            _ => {
                let cur = self.cur().clone();
                self.push_err(
                    "Invalid construct",
                    &format!(
                        "At this point in an expression, {:?} is not a valid construct",
                        cur.ttype
                    ),
                    &cur,
                    Rule::Syntax,
                );
                self.advance();
                return None;
            }
        }
        Some(e)
    }

    /// parses schema_name.table_name and table_name
    #[cfg_attr(feature = "trace", trace)]
    fn schema_table_container(
        &mut self,
        target_name: Option<&str>,
    ) -> Option<SchemaTableContainer> {
        match self.cur().ttype.clone() {
            Type::Ident(schema) if self.next_is(Type::Dot) => {
                // skip schema_name
                self.advance();
                // skip Type::Dot
                self.advance();
                if let Type::Ident(table) = &self.cur().ttype {
                    let table = table.clone();
                    // skip table_name
                    self.advance();
                    Some(SchemaTableContainer::SchemaAndTable { schema, table })
                } else if let Type::String(table) = &self.cur().ttype {
                    let table = table.clone();
                    // skip table_name
                    self.advance();
                    Some(SchemaTableContainer::SchemaAndTable { schema, table })
                } else {
                    // we got schema_name. but not Ident|String following? this is a syntax error
                    let cur = self.cur().clone();
                    self.errors.push(match cur.ttype {
                        Type::Keyword(keyword) => {
                let target_name = target_name.unwrap_or("table");
                            let as_str: &str = keyword.into();
                            self.err(
                            format!("Malformed {target_name} name"),
                            &format!("`{as_str}` is a keyword, if you want to use it as a {target_name} or column name, quote it: '{as_str}'"),
                            &cur, Rule::Syntax)
                        }
                        _ => {
                let target_name = target_name.unwrap_or("table");
                            self.err(
                                                    format!("Malformed {target_name} name"),
                                                    &format!(
                                                        "expected a {target_name} name after <schema_name>. - got {:?}",
                                                        cur.ttype
                                                    ),
                                                    &cur,
                                                    Rule::Syntax,
                                                )
                        },
                    });

                    // skip wrong token, should I skip to the next statement via
                    // self.skip_until_semicolon_or_eof?
                    self.advance();
                    None
                }
            }
            Type::Ident(table_name) | Type::String(table_name) => {
                // skip table_name
                self.advance();
                Some(SchemaTableContainer::Table(table_name))
            }
            _ => {
                let cur = self.cur().clone();
                let target_name = target_name.unwrap_or("table");
                self.push_err(
                    format!("Malformed {} name", target_name),
                    &format!(
                        "expected either schema_name.{} or {}, got {:?}",
                        target_name, target_name, cur.ttype
                    ),
                    &cur,
                    Rule::Syntax,
                );
                self.advance();
                None
            }
        }
    }

    /// https://www.sqlite.org/syntax/conflict-clause.html
    #[cfg_attr(feature = "trace", trace)]
    fn conflict_clause(&mut self) -> Option<Keyword> {
        if self.is_keyword(Keyword::ON) {
            self.advance();
            self.consume_keyword(Keyword::CONFLICT);
            if let Type::Keyword(keyword) = &self.cur().ttype {
                match keyword {
                    Keyword::ROLLBACK
                    | Keyword::ABORT
                    | Keyword::FAIL
                    | Keyword::IGNORE
                    | Keyword::REPLACE => {
                        let keyword = *keyword;
                        self.advance();
                        return Some(keyword);
                    }
                    _ => {
                        let mut err = self.err(
                            "Unexpected Keyword",
                            &format!(
                                "Wanted either ROLLBACK, ABORT, FAIL, IGNORE or REPLACE after ON CONFLICT, got {:?}.",
                                self.cur().ttype
                            ),
                            self.cur(),
                            Rule::Syntax,
                        );
                        err.doc_url = Some("https://www.sqlite.org/syntax/conflict-clause.html");
                        self.errors.push(err);
                    }
                }
            } else {
                let mut err = self.err(
                    "Unexpected Keyword",
                    &format!(
                        "Wanted either ROLLBACK, ABORT, FAIL, IGNORE or REPLACE after ON CONFLICT, got {:?}.",
                        self.cur().ttype
                    ),
                    self.cur(),
                    Rule::Syntax,
                );
                err.doc_url = Some("https://www.sqlite.org/syntax/conflict-clause.html");
                self.errors.push(err);
            }
            self.advance();
        }
        None
    }

    /// https://www.sqlite.org/syntax/foreign-key-clause.html but specifically the ON and MATCH
    /// paths, necessary because the end of the block moves back to the state machine states ON and
    /// MATCH
    #[cfg_attr(feature = "trace", trace)]
    fn foreign_key_clause_on_and_match(&mut self, fk: &mut ForeignKeyClause) -> Option<()> {
        let mut is_delete = false;
        if self.is_keyword(Keyword::ON) {
            self.advance();

            match &self.cur().ttype {
                Type::Keyword(Keyword::DELETE) => is_delete = true,
                Type::Keyword(Keyword::UPDATE) => (),
                _ => {
                    let mut err = self.err(
                        "Unexpected Token",
                        &format!("Wanted DELETE or UPDATE, got {:?}.", self.cur().ttype),
                        self.cur(),
                        Rule::Syntax,
                    );
                    err.doc_url = Some("https://www.sqlite.org/syntax/foreign-key-clause.html");
                    self.errors.push(err);
                }
            };

            self.advance();

            let action = match self.cur().ttype {
                Type::Keyword(Keyword::CASCADE) => {
                    self.advance();
                    Some(ForeignKeyAction::Cascade)
                }
                Type::Keyword(Keyword::RESTRICT) => {
                    self.advance();
                    Some(ForeignKeyAction::Restrict)
                }
                Type::Keyword(Keyword::NO) => {
                    self.advance();
                    self.consume_keyword(Keyword::ACTION);
                    Some(ForeignKeyAction::NoAction)
                }
                Type::Keyword(Keyword::SET) => {
                    self.advance();
                    let a = Some(if self.is_keyword(Keyword::NULL) {
                        ForeignKeyAction::SetNull
                    } else {
                        self.consume_keyword(Keyword::DEFAULT);
                        ForeignKeyAction::SetDefault
                    });
                    self.advance();
                    a
                }
                _ => {
                    let mut err = self.err(
                        "Unexpected Token",
                        &format!(
                            "Wanted SET, CASCADE, RESTRICT or NO after ON DELETE/UPDATE, got {:?}.",
                            self.cur().ttype
                        ),
                        self.cur(),
                        Rule::Syntax,
                    );
                    err.doc_url = Some("https://www.sqlite.org/syntax/foreign-key-clause.html");
                    self.errors.push(err);
                    self.advance();
                    None
                }
            };

            if is_delete {
                fk.on_delete = action;
            } else {
                fk.on_update = action;
            }

            self.foreign_key_clause_on_and_match(fk)
        } else if self.is_keyword(Keyword::MATCH) {
            self.advance();
            fk.match_type = match self.cur().ttype {
                Type::Keyword(Keyword::FULL) => Some(ForeignKeyMatch::Full),
                Type::Keyword(Keyword::PARTIAL) => Some(ForeignKeyMatch::Partial),
                Type::Keyword(Keyword::SIMPLE) => Some(ForeignKeyMatch::Simple),
                _ => todo!("error handling MATCH <kind>"),
            };
            self.advance();
            self.foreign_key_clause_on_and_match(fk)
        } else {
            None
        }
    }

    /// https://www.sqlite.org/syntax/foreign-key-clause.html and https://sqlite.org/foreignkeys.html
    #[cfg_attr(feature = "trace", trace)]
    fn foreign_key_clause(&mut self) -> Option<ForeignKeyClause> {
        let mut fk = ForeignKeyClause {
            foreign_table: String::new(),
            references_columns: vec![],
            on_delete: None,
            on_update: None,
            match_type: None,
            deferrable: false,
            initially_deferred: false,
        };

        self.consume_keyword(Keyword::REFERENCES);
        fk.foreign_table = self.consume_ident(
            "https://www.sqlite.org/syntax/foreign-key-clause.html",
            "foreign_table",
        )?;

        if self.is(Type::BraceLeft) {
            self.advance();
            loop {
                fk.references_columns.push(self.consume_ident(
                    "https://www.sqlite.org/syntax/foreign-key-clause.html",
                    "column_name",
                )?);

                // if we have a comma, the next token is an identifier
                if self.is(Type::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }

            self.consume(Type::BraceRight);
        }

        self.foreign_key_clause_on_and_match(&mut fk);

        if self.is_keyword(Keyword::NOT) || self.is_keyword(Keyword::DEFERRABLE) {
            fk.deferrable = true;
            if self.is_keyword(Keyword::NOT) {
                fk.deferrable = false;
                self.advance();
            }
            self.consume_keyword(Keyword::DEFERRABLE);
            if self.is_keyword(Keyword::INITIALLY) {
                self.advance();
                match &self.cur().ttype {
                    Type::Keyword(Keyword::DEFERRED) => fk.initially_deferred = true,
                    Type::Keyword(Keyword::IMMEDIATE) => (),
                    _ => {
                        let mut err = self.err(
                        "Unexpected Keyword",
                        &format!(
                            "Wanted DEFERRED or IMMEDIATE after DEFERRABLE INITIALLY, got {:?}.",
                            self.cur().ttype
                        ),
                        self.cur(),
                        Rule::Syntax,
                    );
                        err.doc_url = Some("https://www.sqlite.org/syntax/foreign-key-clause.html");
                        self.errors.push(err);
                    }
                };

                self.advance();
            }

            if !fk.deferrable {
                fk.initially_deferred = false;
            }
        }

        Some(fk)
    }

    /// https://www.sqlite.org/syntax/column-def.html
    #[cfg_attr(feature = "trace", trace)]
    fn column_def(&mut self) -> Option<nodes::ColumnDef> {
        let mut def = nodes::ColumnDef {
            t: self.cur().clone(),
            name: String::new(),
            type_name: None,
            constraints: vec![],
        };

        def.name = self.consume_ident("https://www.sqlite.org/syntax/column-def.html", "name")?;

        // we got a type_name: https://www.sqlite.org/syntax/type-name.html
        if let Type::Ident(name) = &self.cur().ttype {
            def.type_name = Some(SqliteStorageClass::from_str(name));

            if SqliteStorageClass::from_str_strict(name.as_str()).is_none() {
                let mut e = self.err(
                    format!("non-canonical SQLite type name `{name}`",),
                    &format!("SQLite will assign {} affinity to this column based on it being declared as type {name}. Consider using a canonical sqlite type: TEXT, BLOB, REAL or INTEGER instead.",
                        SqliteStorageClass::from_str(name.as_str())),
                    self.cur(),
                    Rule::Quirk,
                );
                e.doc_url = Some("https://www.sqlite.org/datatype3.html");
                self.errors.push(e);
            }

            // skip type name
            self.advance();

            if self.is(Type::BraceLeft) {
                // skip Type::BraceLeft
                self.advance();
                if let Type::Number(_) = self.cur().ttype {
                    self.advance();
                } else {
                    let mut err = self.err(
                        "Unexpected Token",
                        &format!(
                            "Wanted a Number after Type::BraceLeft, got {:?}.",
                            self.cur().ttype
                        ),
                        self.cur(),
                        Rule::Syntax,
                    );
                    err.doc_url = Some("https://www.sqlite.org/syntax/type-name.html");
                    self.errors.push(err);
                    self.advance();
                }

                if self.is(Type::Comma) {
                    self.advance();
                    if let Type::Number(_) = self.cur().ttype {
                        self.advance();
                    } else {
                        let mut err = self.err(
                            "Unexpected Token",
                            &format!(
                                "Wanted a Number after Type::BraceLeft, Type::Number and Type::Comma, got {:?}.",
                                self.cur().ttype
                            ),
                            self.cur(),
                            Rule::Syntax,
                        );
                        err.doc_url = Some("https://www.sqlite.org/syntax/type-name.html");
                        self.errors.push(err);
                        self.advance();
                    }
                }
                self.consume(Type::BraceRight);
            }
        } else {
            let tok = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .unwrap_or_else(|| self.cur());

            let err = Error {
                improved_line: None,
                file: self.name.to_string(),
                line: tok.line,
                rule: Rule::Quirk,
                note: "SQLite allows columns without a declared type. Such columns use dynamic typing and type affinity is not enforced. Consider adding TEXT, BLOB, REAL, or INTEGER if this is unintended.".into(),
                msg: "Possibly unintended flexible typed column".into(),
                start: tok.start,
                end: tok.end,
                doc_url: Some("https://www.sqlite.org/quirks.html#the_datatype_is_optional"),
                suggestion: None,
            };
            self.errors.push(err);
        }

        // column_constraint: https://www.sqlite.org/syntax/column-constraint.html
        while !self.is_eof()
            && matches!(
                self.cur().ttype,
                Type::Keyword(Keyword::CONSTRAINT)
                    | Type::Keyword(Keyword::PRIMARY)
                    | Type::Keyword(Keyword::NOT)
                    | Type::Keyword(Keyword::UNIQUE)
                    | Type::Keyword(Keyword::CHECK)
                    | Type::Keyword(Keyword::DEFAULT)
                    | Type::Keyword(Keyword::COLLATE)
                    | Type::Keyword(Keyword::REFERENCES)
                    | Type::Keyword(Keyword::GENERATED)
                    | Type::Keyword(Keyword::AS)
            )
        {
            if self.is_keyword(Keyword::CONSTRAINT) {
                self.advance();
                self.consume_ident(
                    "https://www.sqlite.org/syntax/column-constraint.html",
                    "name",
                );
            }

            let constraint = if self.is_keyword(Keyword::PRIMARY) {
                self.advance();
                self.consume_keyword(Keyword::KEY);
                let asc_desc =
                    if let Type::Keyword(k @ (Keyword::ASC | Keyword::DESC)) = &self.cur().ttype {
                        let k = *k;
                        self.advance();
                        Some(k)
                    } else {
                        None
                    };

                let on_conflict = self.conflict_clause();
                let autoincrement = if self.is_keyword(Keyword::AUTOINCREMENT) {
                    self.advance();
                    true
                } else {
                    false
                };

                Some(ColumnConstraint::PrimaryKey {
                    asc_desc,
                    on_conflict,
                    autoincrement,
                })
            } else if self.is_keyword(Keyword::NOT) {
                self.advance();
                self.consume_keyword(Keyword::NULL);
                Some(ColumnConstraint::NotNull {
                    on_conflict: self.conflict_clause(),
                })
            } else if self.is_keyword(Keyword::UNIQUE) {
                self.advance();
                Some(ColumnConstraint::Unique {
                    on_conflict: self.conflict_clause(),
                })
            } else if self.is_keyword(Keyword::CHECK) {
                self.advance();
                self.consume(Type::BraceLeft);
                let e = self.expr()?;
                self.consume(Type::BraceRight);
                Some(ColumnConstraint::Check(e))
            } else if self.is_keyword(Keyword::DEFAULT) {
                self.advance();
                if self.is(Type::BraceLeft) {
                    self.advance();
                    let expr = self.expr();
                    self.consume(Type::BraceRight);
                    Some(ColumnConstraint::Default {
                        literal: None,
                        expr,
                    })
                } else {
                    // this aint so pretty, but sometimes i do need literals as Option<Box<dyn
                    // Box>> and sometimes as Option<Literal>, it is what it is, Nodes sadly dont
                    // care about my feelings :(
                    let lit = self.literal_value();
                    Some(ColumnConstraint::Default {
                        literal: lit.map(|n| nodes::Literal {
                            t: n.token().clone(),
                        }),
                        expr: None,
                    })
                }
            } else if self.is_keyword(Keyword::COLLATE) {
                self.advance();
                Some(ColumnConstraint::Collate(self.consume_ident(
                    "https://www.sqlite.org/syntax/column-constraint.html",
                    "collation_name",
                )?))
            } else if self.is_keyword(Keyword::REFERENCES) {
                Some(ColumnConstraint::ForeignKey(self.foreign_key_clause()?))
            } else if self.is_keyword(Keyword::GENERATED) || self.is_keyword(Keyword::AS) {
                let mut is_generated = false;
                if self.is_keyword(Keyword::GENERATED) {
                    is_generated = true;
                    self.advance();
                    self.consume_keyword(Keyword::ALWAYS);
                }

                self.consume_keyword(Keyword::AS);
                self.consume(Type::BraceLeft);
                let expr = self.expr().unwrap();
                self.consume(Type::BraceRight);

                let stored_virtual =
                    if let Type::Keyword(k @ (Keyword::STORED | Keyword::VIRTUAL)) =
                        &self.cur().ttype
                    {
                        let k = *k;
                        self.advance();
                        Some(k)
                    } else {
                        None
                    };

                if is_generated {
                    Some(ColumnConstraint::Generated {
                        expr,
                        stored_virtual,
                    })
                } else {
                    Some(ColumnConstraint::As {
                        expr,
                        stored_virtual,
                    })
                }
            } else {
                None
            };

            if let Some(constraint) = constraint {
                def.constraints.push(constraint);
            }
        }

        Some(def)
    }
}
