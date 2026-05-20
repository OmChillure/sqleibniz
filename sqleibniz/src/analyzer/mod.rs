mod tests;

use std::collections::HashMap;

use crate::{
    error::Error,
    lev,
    parser::nodes::{
        self, ColumnConstraint, DeleteStmt, FromClause, InsertStmt, JoinConstraint, Node,
        ResultColumn, SelectCore, SelectStmt, SqlExpr, TableRef, TableSource, UpdateStmt,
    },
    types::{
        Token, Type,
        rules::Rule,
        storage::SqliteStorageClass,
    },
};

/// Information about a single column in a table.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub type_name: Option<SqliteStorageClass>,
    pub is_primary_key: bool,
    pub is_not_null: bool,
    pub is_unique: bool,
}

/// Information about a table gathered from a CREATE TABLE statement.
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

/// Semantic analyzer that performs two-pass analysis:
/// 1. Build a symbol table from CREATE TABLE statements
/// 2. Check DML statements (SELECT, INSERT, UPDATE, DELETE) against the symbol table
pub struct Analyzer {
    /// Lowercase table name -> table info
    tables: HashMap<String, TableInfo>,
    file: String,
    pub errors: Vec<Error>,
}

impl Analyzer {
    pub fn new(file: &str) -> Self {
        Self {
            tables: HashMap::new(),
            file: file.to_string(),
            errors: vec![],
        }
    }

    /// Perform two-pass semantic analysis on the AST.
    pub fn analyze(&mut self, ast: &[Box<dyn Node>]) {
        // First pass: collect all CREATE TABLE definitions into the symbol table
        for node in ast {
            self.first_pass(node.as_ref());
        }

        // Second pass: check DML statements against the symbol table
        for node in ast {
            self.second_pass(node.as_ref());
        }
    }

    // ── First pass: symbol table construction ────────────────────────────────

    fn first_pass(&mut self, node: &dyn Node) {
        if let Some(explain) = node.as_any().downcast_ref::<nodes::Explain>() {
            self.first_pass(explain.child.as_ref());
            return;
        }
        if let Some(ct) = node.as_any().downcast_ref::<nodes::CreateTable>() {
            self.register_table(ct);
        }
    }

    fn register_table(&mut self, ct: &nodes::CreateTable) {
        let mut columns = Vec::new();
        for col in &ct.columns {
            let mut is_pk = false;
            let mut is_nn = false;
            let mut is_uq = false;
            for constraint in &col.constraints {
                match constraint {
                    ColumnConstraint::PrimaryKey { .. } => {
                        is_pk = true;
                        is_nn = true;
                    }
                    ColumnConstraint::NotNull { .. } => is_nn = true,
                    ColumnConstraint::Unique { .. } => is_uq = true,
                    _ => {}
                }
            }
            columns.push(ColumnInfo {
                name: col.name.clone(),
                type_name: col.type_name.clone(),
                is_primary_key: is_pk,
                is_not_null: is_nn,
                is_unique: is_uq,
            });
        }
        let key = ct.name.to_lowercase();
        self.tables.insert(
            key,
            TableInfo {
                name: ct.name.clone(),
                columns,
            },
        );
    }

    // ── Second pass: DML checking ────────────────────────────────────────────

    fn second_pass(&mut self, node: &dyn Node) {
        if let Some(explain) = node.as_any().downcast_ref::<nodes::Explain>() {
            self.second_pass(explain.child.as_ref());
            return;
        }
        if let Some(select) = node.as_any().downcast_ref::<SelectStmt>() {
            self.check_select(select);
        } else if let Some(insert) = node.as_any().downcast_ref::<InsertStmt>() {
            self.check_insert(insert);
        } else if let Some(update) = node.as_any().downcast_ref::<UpdateStmt>() {
            self.check_update(update);
        } else if let Some(delete) = node.as_any().downcast_ref::<DeleteStmt>() {
            self.check_delete(delete);
        }
    }

    // ── SELECT checking ──────────────────────────────────────────────────────

    fn check_select(&mut self, select: &SelectStmt) {
        for cte in &select.ctes {
            self.check_select(&cte.select);
        }
        self.check_select_core(&select.body.first);
        for (_, core) in &select.body.rest {
            self.check_select_core(core);
        }
    }

    fn check_select_core(&mut self, core: &SelectCore) {
        if core.is_values {
            return;
        }

        let scope = if let Some(ref from) = core.from {
            self.check_from_clause(from)
        } else {
            QueryScope::empty()
        };

        for col in &core.columns {
            if let ResultColumn::Expr { expr, .. } = col {
                self.check_expr(expr, &scope);
            }
        }

        if let Some(ref where_expr) = core.where_clause {
            self.check_expr(where_expr, &scope);
        }

        for expr in &core.group_by {
            self.check_expr(expr, &scope);
        }

        if let Some(ref having) = core.having {
            self.check_expr(having, &scope);
        }
    }

    // ── INSERT checking ──────────────────────────────────────────────────────

    fn check_insert(&mut self, insert: &InsertStmt) {
        let table_info = self.check_table_exists(&insert.table, &insert.table_token);

        if let Some(table_info) = table_info {
            for col_name in &insert.columns {
                if !table_info
                    .columns
                    .iter()
                    .any(|c| c.name.eq_ignore_ascii_case(col_name))
                {
                    let suggestions = self.suggest_column(col_name, &table_info);
                    let note = if suggestions.is_empty() {
                        format!(
                            "column '{}' does not exist in table '{}'",
                            col_name, insert.table
                        )
                    } else {
                        format!(
                            "column '{}' does not exist in table '{}', did you mean: {}",
                            col_name,
                            insert.table,
                            suggestions.join(", ")
                        )
                    };
                    self.errors.push(self.err(
                        "Unknown column",
                        &note,
                        &insert.table_token,
                        Rule::UnknownColumn,
                    ));
                }
            }

            let expected_count = if insert.columns.is_empty() {
                table_info.columns.len()
            } else {
                insert.columns.len()
            };

            for (i, row) in insert.values.iter().enumerate() {
                if row.len() != expected_count {
                    self.errors.push(self.err(
                        "INSERT value count mismatch",
                        &format!(
                            "row {} has {} value(s) but {} column(s) are expected (table '{}' has columns: {})",
                            i + 1,
                            row.len(),
                            expected_count,
                            insert.table,
                            table_info
                                .columns
                                .iter()
                                .map(|c| c.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        &insert.t,
                        Rule::InsertValueCountMismatch,
                    ));
                }
            }
        }

        if let Some(ref sel) = insert.select {
            self.check_select(sel);
        }
    }

    // ── UPDATE checking ──────────────────────────────────────────────────────

    fn check_update(&mut self, update: &UpdateStmt) {
        let table_info = self.check_table_exists(&update.table, &update.table_token);

        let mut scope = QueryScope::empty();
        let key = update.table.to_lowercase();
        if let Some(info) = self.tables.get(&key) {
            let alias = update.alias.as_deref().unwrap_or(&update.table);
            scope.add(alias.to_string(), info.clone());
        }
        if let Some(ref from) = update.from {
            let from_scope = self.check_from_clause(from);
            scope.merge(from_scope);
        }

        if let Some(ref info) = table_info {
            for set in &update.set_clauses {
                self.check_column_in_table(&set.column, &set.column_token, info);
                self.check_expr(&set.expr, &scope);
            }
        }

        if let Some(ref where_expr) = update.where_clause {
            self.check_expr(where_expr, &scope);
        }
    }

    // ── DELETE checking ──────────────────────────────────────────────────────

    fn check_delete(&mut self, delete: &DeleteStmt) {
        let _table_info = self.check_table_exists(&delete.table, &delete.table_token);

        let mut scope = QueryScope::empty();
        let key = delete.table.to_lowercase();
        if let Some(info) = self.tables.get(&key) {
            let alias = delete.alias.as_deref().unwrap_or(&delete.table);
            scope.add(alias.to_string(), info.clone());
        }

        if let Some(ref where_expr) = delete.where_clause {
            self.check_expr(where_expr, &scope);
        }
    }

    // ── FROM clause → builds query scope + checks table existence ────────────

    fn check_from_clause(&mut self, from: &FromClause) -> QueryScope {
        let mut scope = QueryScope::empty();
        self.check_table_ref_into_scope(&from.first, &mut scope);
        for join in &from.joins {
            self.check_table_ref_into_scope(&join.table, &mut scope);
            if let Some(JoinConstraint::On(ref expr)) = join.constraint {
                self.check_expr(expr, &scope);
            }
        }
        scope
    }

    fn check_table_ref_into_scope(&mut self, tref: &TableRef, scope: &mut QueryScope) {
        match &tref.source {
            TableSource::Table { schema: _, name } => {
                let key = name.to_lowercase();
                if let Some(info) = self.tables.get(&key).cloned() {
                    let visible_name = tref.alias.as_deref().unwrap_or(name);
                    scope.add(visible_name.to_string(), info);
                } else if !self.tables.is_empty() {
                    let suggestions = self.suggest_table(name);
                    let note = if suggestions.is_empty() {
                        format!("table '{}' does not exist in any CREATE TABLE", name)
                    } else {
                        format!(
                            "table '{}' does not exist, did you mean: {}",
                            name,
                            suggestions.join(", ")
                        )
                    };
                    self.errors.push(self.err(
                        "Unknown table",
                        &note,
                        &tref.token,
                        Rule::UnknownTable,
                    ));
                }
            }
            TableSource::Subquery(sel) => {
                self.check_select(sel);
                if let Some(ref alias) = tref.alias {
                    scope.add_opaque(alias.clone());
                }
            }
            TableSource::TableFunction { .. } => {
                if let Some(ref alias) = tref.alias {
                    scope.add_opaque(alias.clone());
                }
            }
            TableSource::ParenFrom(inner) => {
                let inner_scope = self.check_from_clause(inner);
                scope.merge(inner_scope);
            }
        }
    }

    // ── Expression checking ──────────────────────────────────────────────────

    fn check_expr(&mut self, expr: &SqlExpr, scope: &QueryScope) {
        match expr {
            SqlExpr::ColumnRef {
                token,
                schema: _,
                table,
                column,
            } => {
                if column == "*" {
                    return;
                }
                if let Some(tbl_name) = table {
                    if let Some(info) = scope.resolve_table(tbl_name) {
                        self.check_column_in_table(column, token, info);
                    }
                } else if !scope.is_empty() {
                    let found = scope.find_column(column);
                    if !found {
                        let all_columns = scope.all_columns();
                        let suggestions = self.suggest_from_list(column, &all_columns);
                        let note = if suggestions.is_empty() {
                            format!(
                                "column '{}' does not exist in any table in scope",
                                column
                            )
                        } else {
                            format!(
                                "column '{}' does not exist in any table in scope, did you mean: {}",
                                column,
                                suggestions.join(", ")
                            )
                        };
                        self.errors.push(
                            self.err("Unknown column", &note, token, Rule::UnknownColumn),
                        );
                    }
                }
            }
            SqlExpr::BinaryOp { left, op, right } => {
                self.check_expr(left, scope);
                self.check_expr(right, scope);
                self.check_type_compat(left, op, right, scope);
            }
            SqlExpr::UnaryOp { operand, .. } => {
                self.check_expr(operand, scope);
            }
            SqlExpr::FunctionCall { args, .. } => {
                for arg in args {
                    self.check_expr(arg, scope);
                }
            }
            SqlExpr::WindowFunctionCall {
                args, filter, ..
            } => {
                for arg in args {
                    self.check_expr(arg, scope);
                }
                if let Some(f) = filter {
                    self.check_expr(f, scope);
                }
            }
            SqlExpr::Paren(inner) => self.check_expr(inner, scope),
            SqlExpr::Subquery(sel) => self.check_select(sel),
            SqlExpr::Exists { subquery, .. } => self.check_select(subquery),
            SqlExpr::InList { expr, values, .. } => {
                self.check_expr(expr, scope);
                for v in values {
                    self.check_expr(v, scope);
                }
            }
            SqlExpr::InSelect {
                expr, subquery, ..
            } => {
                self.check_expr(expr, scope);
                self.check_select(subquery);
            }
            SqlExpr::Between {
                expr, low, high, ..
            } => {
                self.check_expr(expr, scope);
                self.check_expr(low, scope);
                self.check_expr(high, scope);
            }
            SqlExpr::IsNull { expr, .. } => self.check_expr(expr, scope),
            SqlExpr::Like {
                expr,
                pattern,
                escape,
                ..
            } => {
                self.check_expr(expr, scope);
                self.check_expr(pattern, scope);
                if let Some(e) = escape {
                    self.check_expr(e, scope);
                }
            }
            SqlExpr::Cast { expr, .. } => self.check_expr(expr, scope),
            SqlExpr::Case {
                operand,
                when_clauses,
                else_clause,
            } => {
                if let Some(op) = operand {
                    self.check_expr(op, scope);
                }
                for (w, t) in when_clauses {
                    self.check_expr(w, scope);
                    self.check_expr(t, scope);
                }
                if let Some(e) = else_clause {
                    self.check_expr(e, scope);
                }
            }
            SqlExpr::Collate { expr, .. } => self.check_expr(expr, scope),
            SqlExpr::Literal(_) | SqlExpr::Star | SqlExpr::BindParam { .. } => {}
        }
    }

    // ── Type compatibility checking ──────────────────────────────────────────

    fn infer_type(&self, expr: &SqlExpr, scope: &QueryScope) -> Option<SqliteStorageClass> {
        match expr {
            SqlExpr::Literal(tok) => match &tok.ttype {
                Type::Number(_) => Some(SqliteStorageClass::Integer),
                Type::String(_) => Some(SqliteStorageClass::Text),
                Type::Blob(_) => Some(SqliteStorageClass::Blob),
                Type::Boolean(_) => Some(SqliteStorageClass::Integer),
                Type::Keyword(crate::types::Keyword::NULL) => None,
                _ => None,
            },
            SqlExpr::ColumnRef { table, column, .. } => {
                if let Some(tbl_name) = table {
                    if let Some(info) = scope.resolve_table(tbl_name) {
                        return info
                            .columns
                            .iter()
                            .find(|c| c.name.eq_ignore_ascii_case(column))
                            .and_then(|c| c.type_name.clone());
                    }
                } else {
                    for info in scope.tables.values().flatten() {
                        if let Some(col) = info
                            .columns
                            .iter()
                            .find(|c| c.name.eq_ignore_ascii_case(column))
                        {
                            return col.type_name.clone();
                        }
                    }
                }
                None
            }
            SqlExpr::Cast { type_name, .. } => SqliteStorageClass::from_str_strict(type_name),
            _ => None,
        }
    }

    fn types_compatible(a: &SqliteStorageClass, b: &SqliteStorageClass) -> bool {
        use SqliteStorageClass::*;
        match (a, b) {
            (Null, _) | (_, Null) => true,
            (Integer, Integer) | (Real, Real) | (Text, Text) | (Blob, Blob) => true,
            (Integer, Real) | (Real, Integer) => true,
            _ => false,
        }
    }

    fn check_type_compat(
        &mut self,
        left: &SqlExpr,
        op: &Token,
        right: &SqlExpr,
        scope: &QueryScope,
    ) {
        let is_comparison = matches!(
            op.ttype,
            Type::Equal
                | Type::BangEqual
                | Type::LessGreater
                | Type::Less
                | Type::Greater
                | Type::LessEqual
                | Type::GreaterEqual
        );
        if !is_comparison {
            return;
        }

        let left_type = self.infer_type(left, scope);
        let right_type = self.infer_type(right, scope);

        if let (Some(ref lt), Some(ref rt)) = (left_type, right_type) {
            if !Self::types_compatible(lt, rt) {
                self.errors.push(self.err(
                    "Type mismatch in comparison",
                    &format!(
                        "comparing {} with {} may produce unexpected results",
                        lt, rt
                    ),
                    op,
                    Rule::TypeMismatch,
                ));
            }
        }
    }

    // ── Helper: check table existence ────────────────────────────────────────

    fn check_table_exists(&mut self, name: &str, token: &Token) -> Option<TableInfo> {
        if self.tables.is_empty() {
            return None;
        }
        let key = name.to_lowercase();
        if let Some(info) = self.tables.get(&key) {
            return Some(info.clone());
        }
        let suggestions = self.suggest_table(name);
        let note = if suggestions.is_empty() {
            format!("table '{}' does not exist in any CREATE TABLE", name)
        } else {
            format!(
                "table '{}' does not exist, did you mean: {}",
                name,
                suggestions.join(", ")
            )
        };
        self.errors
            .push(self.err("Unknown table", &note, token, Rule::UnknownTable));
        None
    }

    fn check_column_in_table(&mut self, column: &str, token: &Token, info: &TableInfo) {
        if info
            .columns
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(column))
        {
            return;
        }
        let suggestions = self.suggest_column(column, info);
        let note = if suggestions.is_empty() {
            format!(
                "column '{}' does not exist in table '{}'",
                column, info.name
            )
        } else {
            format!(
                "column '{}' does not exist in table '{}', did you mean: {}",
                column,
                info.name,
                suggestions.join(", ")
            )
        };
        self.errors
            .push(self.err("Unknown column", &note, token, Rule::UnknownColumn));
    }

    // ── Suggestions via Levenshtein distance ─────────────────────────────────

    fn suggest_table(&self, name: &str) -> Vec<String> {
        let input = name.to_lowercase();
        self.suggest_from_list(
            &input,
            &self.tables.keys().cloned().collect::<Vec<_>>(),
        )
    }

    fn suggest_column(&self, name: &str, table: &TableInfo) -> Vec<String> {
        let cols: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
        self.suggest_from_list(name, &cols)
    }

    fn suggest_from_list(&self, input: &str, candidates: &[String]) -> Vec<String> {
        let input_lower = input.to_lowercase();
        let bytes = input_lower.as_bytes();
        let mut scored: Vec<(&str, usize)> = candidates
            .iter()
            .map(|c| {
                let dist = lev::distance(bytes, c.to_lowercase().as_bytes());
                (c.as_str(), dist)
            })
            .collect();
        scored.sort_by_key(|&(_, d)| d);
        scored
            .into_iter()
            .take(3)
            .filter(|&(_, d)| d <= 4)
            .map(|(k, _)| k.to_string())
            .collect()
    }

    // ── Error construction ───────────────────────────────────────────────────

    fn err(&self, msg: &str, note: &str, token: &Token, rule: Rule) -> Error {
        Error {
            improved_line: None,
            file: self.file.clone(),
            line: token.line,
            rule,
            note: note.into(),
            msg: msg.into(),
            start: token.start,
            end: token.end,
            doc_url: None,
        }
    }
}

// ── Query scope: tracks which tables/columns are visible ─────────────────────

struct QueryScope {
    tables: HashMap<String, Option<TableInfo>>,
}

impl QueryScope {
    fn empty() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    fn add(&mut self, name: String, info: TableInfo) {
        self.tables.insert(name.to_lowercase(), Some(info));
    }

    fn add_opaque(&mut self, name: String) {
        self.tables.insert(name.to_lowercase(), None);
    }

    fn merge(&mut self, other: QueryScope) {
        for (k, v) in other.tables {
            self.tables.insert(k, v);
        }
    }

    fn resolve_table(&self, name: &str) -> Option<&TableInfo> {
        self.tables
            .get(&name.to_lowercase())
            .and_then(|v| v.as_ref())
    }

    fn find_column(&self, column: &str) -> bool {
        for entry in self.tables.values() {
            match entry {
                Some(info) => {
                    if info
                        .columns
                        .iter()
                        .any(|c| c.name.eq_ignore_ascii_case(column))
                    {
                        return true;
                    }
                }
                None => {
                    return true;
                }
            }
        }
        false
    }

    fn all_columns(&self) -> Vec<String> {
        let mut result = Vec::new();
        for entry in self.tables.values() {
            if let Some(info) = entry {
                for col in &info.columns {
                    result.push(col.name.clone());
                }
            }
        }
        result
    }
}