#[cfg(test)]
mod analyzer_tests {
    use crate::{
        analyzer::Analyzer,
        lexer::Lexer,
        parser::Parser,
        types::rules::Rule,
    };

    /// Helper: lex + parse + analyze, return analyzer errors
    fn analyze(input: &str) -> Vec<(Rule, String)> {
        let bytes = input.as_bytes().to_vec();
        let mut lexer = Lexer::new(&bytes, "test");
        let toks = lexer.run();
        assert!(
            lexer.errors.is_empty(),
            "lexer errors: {:?}",
            lexer.errors
        );

        let mut parser = Parser::new(toks, "test");
        let ast = parser.parse();
        assert!(
            parser.errors.is_empty(),
            "parser errors: {:?}",
            parser.errors
        );

        let mut analyzer = Analyzer::new("test");
        analyzer.analyze(&ast);
        analyzer
            .errors
            .iter()
            .map(|e| (e.rule.clone(), e.note.clone()))
            .collect()
    }

    /// Helper: run analysis and assert no errors
    fn analyze_ok(input: &str) {
        let errs = analyze(input);
        assert!(errs.is_empty(), "expected no errors, got: {:?}", errs);
    }

    /// Helper: run analysis and assert exactly one error with the given rule
    fn analyze_has_error(input: &str, expected_rule: Rule) {
        let errs = analyze(input);
        assert!(
            errs.iter().any(|(r, _)| *r == expected_rule),
            "expected error {:?} but got: {:?}",
            expected_rule,
            errs
        );
    }

    // ── Symbol table construction ────────────────────────────────────────────

    #[test]
    fn create_table_registers_in_symbol_table() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             SELECT * FROM users;",
        );
    }

    #[test]
    fn create_table_if_not_exists() {
        analyze_ok(
            "CREATE TABLE IF NOT EXISTS users (id INTEGER, name TEXT);
             SELECT * FROM users;",
        );
    }

    #[test]
    fn create_temp_table() {
        analyze_ok(
            "CREATE TEMP TABLE tmp (val TEXT);
             SELECT * FROM tmp;",
        );
    }

    #[test]
    fn multiple_create_tables() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE orders (id INTEGER, user_id INTEGER, total REAL);
             SELECT * FROM users;
             SELECT * FROM orders;",
        );
    }

    // ── Unknown table ────────────────────────────────────────────────────────

    #[test]
    fn select_unknown_table() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER);
             SELECT * FROM nonexistent;",
            Rule::UnknownTable,
        );
    }

    #[test]
    fn select_unknown_table_with_suggestion() {
        let errs = analyze(
            "CREATE TABLE users (id INTEGER);
             SELECT * FROM usres;",
        );
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].0, Rule::UnknownTable);
        assert!(
            errs[0].1.contains("did you mean"),
            "expected suggestion, got: {}",
            errs[0].1
        );
        assert!(
            errs[0].1.contains("users"),
            "expected 'users' suggestion, got: {}",
            errs[0].1
        );
    }

    #[test]
    fn insert_unknown_table() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER);
             INSERT INTO nonexistent VALUES (1);",
            Rule::UnknownTable,
        );
    }

    #[test]
    fn update_unknown_table() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER);
             UPDATE nonexistent SET id = 1;",
            Rule::UnknownTable,
        );
    }

    #[test]
    fn delete_unknown_table() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER);
             DELETE FROM nonexistent;",
            Rule::UnknownTable,
        );
    }

    #[test]
    fn no_create_tables_means_no_semantic_errors() {
        // If there are no CREATE TABLE statements, the analyzer has no symbol table
        // and should not report unknown table errors
        analyze_ok("SELECT * FROM users;");
    }

    // ── Unknown column ───────────────────────────────────────────────────────

    #[test]
    fn select_unknown_column() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT nonexistent FROM users;",
            Rule::UnknownColumn,
        );
    }

    #[test]
    fn select_unknown_column_with_suggestion() {
        let errs = analyze(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT naem FROM users;",
        );
        assert!(
            errs.iter().any(|(r, _)| *r == Rule::UnknownColumn),
            "expected UnknownColumn error, got: {:?}",
            errs
        );
        let col_err = errs
            .iter()
            .find(|(r, _)| *r == Rule::UnknownColumn)
            .unwrap();
        assert!(
            col_err.1.contains("did you mean"),
            "expected suggestion, got: {}",
            col_err.1
        );
        assert!(
            col_err.1.contains("name"),
            "expected 'name' suggestion, got: {}",
            col_err.1
        );
    }

    #[test]
    fn select_valid_column() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT id FROM users;",
        );
    }

    #[test]
    fn select_qualified_column() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT users.id FROM users;",
        );
    }

    #[test]
    fn select_qualified_unknown_column() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT users.nonexistent FROM users;",
            Rule::UnknownColumn,
        );
    }

    #[test]
    fn update_unknown_column() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             UPDATE users SET nonexistent = 'val';",
            Rule::UnknownColumn,
        );
    }

    #[test]
    fn update_valid_column() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             UPDATE users SET name = 'Bob';",
        );
    }

    #[test]
    fn select_column_case_insensitive() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT ID FROM users;",
        );
    }

    // ── INSERT value count mismatch ──────────────────────────────────────────

    #[test]
    fn insert_correct_value_count() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             INSERT INTO users VALUES (1, 'Alice');",
        );
    }

    #[test]
    fn insert_too_few_values() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             INSERT INTO users VALUES (1);",
            Rule::InsertValueCountMismatch,
        );
    }

    #[test]
    fn insert_too_many_values() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             INSERT INTO users VALUES (1, 'Alice', 'extra');",
            Rule::InsertValueCountMismatch,
        );
    }

    #[test]
    fn insert_with_column_list_correct() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT, email TEXT);
             INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com');",
        );
    }

    #[test]
    fn insert_with_column_list_mismatch() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT, email TEXT);
             INSERT INTO users (name, email) VALUES ('Alice');",
            Rule::InsertValueCountMismatch,
        );
    }

    #[test]
    fn insert_with_unknown_column_in_list() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             INSERT INTO users (id, nonexistent) VALUES (1, 'val');",
            Rule::UnknownColumn,
        );
    }

    #[test]
    fn insert_multiple_rows_one_bad() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             INSERT INTO users VALUES (1, 'Alice'), (2);",
            Rule::InsertValueCountMismatch,
        );
    }

    // ── Type mismatch ────────────────────────────────────────────────────────

    #[test]
    fn type_mismatch_text_vs_integer() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT * FROM users WHERE id = 'text_value';",
            Rule::TypeMismatch,
        );
    }

    #[test]
    fn type_mismatch_text_vs_blob() {
        analyze_has_error(
            "CREATE TABLE data (id INTEGER, payload BLOB);
             SELECT * FROM data WHERE payload = 'text_value';",
            Rule::TypeMismatch,
        );
    }

    #[test]
    fn type_compatible_integer_real() {
        analyze_ok(
            "CREATE TABLE data (id INTEGER, score REAL);
             SELECT * FROM data WHERE id = 42;",
        );
    }

    #[test]
    fn type_compatible_same_types() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT * FROM users WHERE name = 'Alice';",
        );
    }

    #[test]
    fn no_type_mismatch_for_null() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT * FROM users WHERE name = NULL;",
        );
    }

    // ── JOIN checking ────────────────────────────────────────────────────────

    #[test]
    fn join_valid_tables() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             CREATE TABLE orders (id INTEGER, user_id INTEGER, total REAL);
             SELECT * FROM users JOIN orders ON users.id = orders.user_id;",
        );
    }

    #[test]
    fn join_unknown_table() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER);
             SELECT * FROM users JOIN nonexistent ON users.id = nonexistent.id;",
            Rule::UnknownTable,
        );
    }

    #[test]
    fn join_unknown_column_in_condition() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             CREATE TABLE orders (id INTEGER, user_id INTEGER);
             SELECT * FROM users JOIN orders ON users.id = orders.nonexistent;",
            Rule::UnknownColumn,
        );
    }

    // ── Alias handling ───────────────────────────────────────────────────────

    #[test]
    fn select_with_alias() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT u.id FROM users u;",
        );
    }

    #[test]
    fn select_with_alias_unknown_column() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT u.nonexistent FROM users u;",
            Rule::UnknownColumn,
        );
    }

    // ── Subqueries ───────────────────────────────────────────────────────────

    #[test]
    fn subquery_in_from() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT * FROM (SELECT id FROM users) AS sub;",
        );
    }

    #[test]
    fn subquery_checks_inner_tables() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER);
             SELECT * FROM (SELECT * FROM nonexistent) AS sub;",
            Rule::UnknownTable,
        );
    }

    // ── WHERE clause checking ────────────────────────────────────────────────

    #[test]
    fn where_unknown_column() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT * FROM users WHERE nonexistent = 1;",
            Rule::UnknownColumn,
        );
    }

    #[test]
    fn where_valid_column() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT * FROM users WHERE id = 1;",
        );
    }

    #[test]
    fn delete_where_valid_column() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             DELETE FROM users WHERE id = 1;",
        );
    }

    #[test]
    fn delete_where_unknown_column() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER, name TEXT);
             DELETE FROM users WHERE nonexistent = 1;",
            Rule::UnknownColumn,
        );
    }

    // ── Case insensitive table names ─────────────────────────────────────────

    #[test]
    fn table_name_case_insensitive() {
        analyze_ok(
            "CREATE TABLE Users (id INTEGER);
             SELECT * FROM users;",
        );
    }

    // ── SELECT * (star) should not trigger column errors ─────────────────────

    #[test]
    fn select_star_ok() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT * FROM users;",
        );
    }

    // ── Explain wrapping ─────────────────────────────────────────────────────

    #[test]
    fn explain_select_checks_inner() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER);
             EXPLAIN SELECT * FROM nonexistent;",
            Rule::UnknownTable,
        );
    }

    // ── Default values insert ────────────────────────────────────────────────

    #[test]
    fn insert_default_values_ok() {
        analyze_ok(
            "CREATE TABLE users (id INTEGER, name TEXT);
             INSERT INTO users DEFAULT VALUES;",
        );
    }

    // ── INSERT with SELECT ───────────────────────────────────────────────────

    #[test]
    fn insert_with_select_checks_subquery() {
        analyze_has_error(
            "CREATE TABLE users (id INTEGER);
             CREATE TABLE archive (id INTEGER);
             INSERT INTO archive SELECT * FROM nonexistent;",
            Rule::UnknownTable,
        );
    }

    // ── Multiple errors in one file ──────────────────────────────────────────

    #[test]
    fn multiple_errors() {
        let errs = analyze(
            "CREATE TABLE users (id INTEGER, name TEXT);
             SELECT * FROM nonexistent;
             INSERT INTO users VALUES (1, 'Alice', 'extra');",
        );
        assert!(errs.len() >= 2, "expected at least 2 errors, got: {:?}", errs);
    }
}