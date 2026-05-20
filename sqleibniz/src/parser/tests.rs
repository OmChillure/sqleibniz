#[allow(unused_macros)]
macro_rules! test_group_pass_assert {
    ($group_name:ident,$($ident:ident:$input:literal=$expected:expr),*) => {
    mod $group_name {
        #[allow(unused_imports)]
        use crate::{lexer, parser::Parser, parser::nodes::*, types::*, types::storage::*};

        $(
            #[test]
            fn $ident() {
                let input = $input.as_bytes().to_vec();
                let mut l = lexer::Lexer::new(&input, "parser_test_pass");
                let toks = l.run();
                assert_eq!(l.errors.len(), 0);

                let mut parser = Parser::new(toks, "parser_test_pass");
                let ast = parser.parse();
                assert_eq!(parser.errors.len(), 0);

                let serialized_ast = serde_json::to_string(
                    &ast.into_iter()
                        .map(|n| n.as_serializable())
                        .collect::<Vec<_>>(),
                ).unwrap();
                let serialized_expected = serde_json::to_string(
                    &$expected.into_iter()
                        .map(|n| n.as_serializable())
                        .collect::<Vec<_>>(),
                    )
                .unwrap();
                pretty_assertions::assert_eq!(serialized_expected, serialized_ast);
            }
        )*
        }
    };
}

#[cfg(test)]
mod should_pass {

    test_group_pass_assert! {
        sqleibniz_instructions,
        expect: r"
    -- @sqleibniz::expect
    VACUUM 25;
    -- the above is skipped
    EXPLAIN VACUUM;
        "=vec![Explain::new(Box::new(Vacuum::new(None, None)))],

        expect_with_semicolons_in_comment: r"
    -- @sqleibniz::expect lets skip this error;;;;;;;;
    VACUUM 25;
    EXPLAIN VACUUM;
        "=vec![Explain::new(Box::new(Vacuum::new(None, None)))]
    }

    test_group_pass_assert! {
        sql_stmt_prefix,
        explain: r#"EXPLAIN VACUUM;"#=vec![Explain::new(Box::new(Vacuum::new(None, None)))],
        explain_query_plan: r#"EXPLAIN QUERY PLAN VACUUM;"#=vec![Explain::new(Box::new(Vacuum::new(None, None)))]
    }

    test_group_pass_assert! {
        vacuum,
        vacuum_first_path: r#"VACUUM;"#=vec![Vacuum::new(None, None)],
        vacuum_second_path: r#"VACUUM schema_name;"#=vec![
            Vacuum::new(
                Some(Token::new(Type::Ident("schema_name".into()))),
                None,
            )
        ],
        vacuum_third_path: r#"VACUUM INTO 'filename';"#=vec![
            Vacuum::new(
                None,
                Some(Token::new(Type::String("filename".into()))),
            )
        ],
        vacuum_full_path: r#"VACUUM schema_name INTO 'filename';"#=vec![
            Vacuum::new(
                Some(Token::new(Type::Ident("schema_name".into()))),
                Some(Token::new(Type::String("filename".into()))),
            )
        ]
    }

    test_group_pass_assert! {
        begin_stmt,
        begin: r#"BEGIN;"#=vec![Begin::new(None)],
        begin_transaction: r#"BEGIN TRANSACTION;"#=vec![Begin::new(None)],
        begin_deferred: r#"BEGIN DEFERRED;"#=vec![Begin::new(Some(Keyword::DEFERRED))],
        begin_immediate: r#"BEGIN IMMEDIATE;"#=vec![Begin::new(Some(Keyword::IMMEDIATE))],
        begin_exclusive: r#"BEGIN EXCLUSIVE;"#=vec![Begin::new(Some(Keyword::EXCLUSIVE))],

        begin_deferred_transaction: r"BEGIN DEFERRED TRANSACTION;"=vec![Begin::new(Some(Keyword::DEFERRED))],
        begin_immediate_transaction: r"BEGIN IMMEDIATE TRANSACTION;"=vec![Begin::new(Some(Keyword::IMMEDIATE))],
        begin_exclusive_transaction: r"BEGIN EXCLUSIVE TRANSACTION;"=vec![Begin::new(Some(Keyword::EXCLUSIVE))]
    }

    test_group_pass_assert! {
        commit_stmt,
        commit:            r"COMMIT;"=vec![Commit::new()],
        end:               r"END;"=vec![Commit::new()],
        commit_transaction:r"COMMIT TRANSACTION;"=vec![Commit::new()],
        end_transaction:   r"END TRANSACTION;"=vec![Commit::new()]
    }

    test_group_pass_assert! {
        rollback_stmt,

        rollback:r"ROLLBACK;"=vec![Rollback::new(None)],
        rollback_to_save_point:r"ROLLBACK TO save_point;"=vec![Rollback::new(Some("save_point".into()))],
        rollback_to_savepoint_save_point:r"ROLLBACK TO SAVEPOINT save_point;"=vec![Rollback::new(Some("save_point".into()))],
        rollback_transaction:r"ROLLBACK TRANSACTION;"=vec![Rollback::new(None)],
        rollback_transaction_to_save_point:r"ROLLBACK TRANSACTION TO save_point;"=vec![Rollback::new(Some("save_point".into()))],
        rollback_transaction_to_savepoint_save_point:r"ROLLBACK TRANSACTION TO SAVEPOINT save_point;"=vec![Rollback::new(Some("save_point".into()))]
    }

    test_group_pass_assert! {
        detach_stmt,

        detach_schema_name:r"DETACH schema_name;"=vec![Detach::new("schema_name".into())],
        detach_database_schema_name:r"DETACH DATABASE schema_name;"=vec![Detach::new("schema_name".into())]
    }

    test_group_pass_assert! {
        analyze_stmt,

        analyze:r"ANALYZE;"=vec![Analyze::new(None)],
        analyze_schema_name:r"ANALYZE schema_name;"=vec![
            Analyze::new(
                Some(SchemaTableContainer::Table("schema_name".into())),
            ),
        ],
        analyze_index_or_table_name:r"ANALYZE index_or_table_name;"=vec![
            Analyze::new(
                Some(SchemaTableContainer::Table("index_or_table_name".into()))
            )
        ],
        analyze_schema_name_with_subtable:r"ANALYZE schema_name.index_or_table_name;"=vec![
            Analyze::new(
                Some(SchemaTableContainer::SchemaAndTable {
                    schema: "schema_name".into(),
                    table: "index_or_table_name".into(),
                })
            )
        ]
    }

    test_group_pass_assert! {
        drop_stmt,

        drop_index_index_name:r"DROP INDEX index_name;"=vec![Drop::new(false, Keyword::INDEX, SchemaTableContainer::Table("index_name".into()))],
        drop_index_if_exists_schema_name_index_name:r"DROP INDEX IF EXISTS schema_name.index_name;"=vec![
            Drop::new(true, Keyword::INDEX, SchemaTableContainer::SchemaAndTable{ schema: "schema_name".into(), table: "index_name".into(), })
        ],
        drop_table_table_name:r"DROP TABLE table_name;"=vec![Drop::new(false, Keyword::TABLE, SchemaTableContainer::Table("table_name".into()))],
        drop_table_if_exists_schema_name_table_name:r"DROP TABLE IF EXISTS schema_name.table_name;"=vec![
            Drop::new(true, Keyword::TABLE, SchemaTableContainer::SchemaAndTable{ schema: "schema_name".into(), table: "table_name".into(), })
        ],
        drop_trigger_trigger_name:r"DROP TRIGGER trigger_name;"=vec![Drop::new(false, Keyword::TRIGGER, SchemaTableContainer::Table("trigger_name".into()))],
        drop_trigger_if_exists_schema_name_trigger_name:r"DROP TRIGGER IF EXISTS schema_name.trigger_name;"=vec![
            Drop::new(true, Keyword::TRIGGER, SchemaTableContainer::SchemaAndTable{ schema: "schema_name".into(), table: "trigger_name".into(), })
        ],
        drop_view_view_name:r"DROP VIEW view_name;"=vec![
            Drop::new(false, Keyword::VIEW, SchemaTableContainer::Table("view_name".into()))
        ],
        drop_view_if_exists_schema_name_view_name:r"DROP VIEW IF EXISTS schema_name.view_name;"=vec![
            Drop::new(true, Keyword::VIEW, SchemaTableContainer::SchemaAndTable{ schema: "schema_name".into(), table: "view_name".into(), })
        ]
    }

    test_group_pass_assert! {
        savepoint_stmt,

        savepoint_savepoint_name:r"SAVEPOINT savepoint_name;"=vec![Savepoint::new("savepoint_name".into())]
    }

    test_group_pass_assert! {
        release_stmt,

        release_savepoint_savepoint_name:r"RELEASE SAVEPOINT savepoint_name;"=vec![Release::new("savepoint_name".into())],
        release_savepoint_name:r"RELEASE savepoint_name;"=vec![Release::new("savepoint_name".into())]
    }

    test_group_pass_assert! {
        attach_stmt,

        attach:r"ATTACH 'database.db' AS db;"=vec![
            Attach::new(
                "db".into(),
                Expr::new(
                    Some(Token::new(Type::String("database.db".into()))),
                    None,
                    None,
                    None,
                    None,
                )
            ),
        ],
        attach_database:r"ATTACH DATABASE 'database.db' AS db;"=vec![
            Attach::new(
                "db".into(),
                Expr::new(
                    Some(Token::new(Type::String("database.db".into()))),
                    None,
                    None,
                    None,
                    None,
                )
            ),
        ]
    }

    test_group_pass_assert! {
        reindex_stmt,

        reindex:r"REINDEX;"=vec![Reindex::new(None)],
        reindex_collation_name:r"REINDEX collation_name;"=vec![Reindex::new(Some(SchemaTableContainer::Table("collation_name".into())))],
        reindex_schema_name_table_name:r"REINDEX schema_name.table_name;"=vec![Reindex::new(Some(SchemaTableContainer::SchemaAndTable { schema: "schema_name".into(), table: "table_name".into() }))]
    }

    test_group_pass_assert! {
        alter_stmt,

        alter_rename_to: r"ALTER TABLE schema.table_name RENAME TO new_table;"=vec![
            Alter::new(
                SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
                Some("new_table".into()),
                None,
                None,
                None,
                None,
            ),
        ],

        alter_rename_column_to: r"ALTER TABLE schema.table_name RENAME COLUMN old_column_name TO new_column_name;"=vec![
            Alter::new(
                SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
                None,
                Some("old_column_name".into()),
                Some("new_column_name".into()),
                None,
                None,
            ),
        ],
        alter_rename_column_to_without_column_keyword: r"ALTER TABLE schema.table_name RENAME old_column_name TO new_column_name;"=vec![
            Alter::new(
                SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
                None,
                Some("old_column_name".into()),
                Some("new_column_name".into()),
                None,
                None,
            ),
        ],

        alter_add: r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT;"=vec![
            Alter::new(
                SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
                None,
                None,
                None,
                Some(ColumnDef::new("column_name".into(), Some(SqliteStorageClass::Text), vec![])),
                None,
            ),
        ],
        alter_add_without_column_keyword: r"ALTER TABLE schema.table_name ADD column_name TEXT;"=vec![
            Alter::new(
                SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
                None,
                None,
                None,
                Some(ColumnDef::new("column_name".into(), Some(SqliteStorageClass::Text), vec![])),
                None,
            ),
        ],

        alter_drop_column: r"ALTER TABLE schema.table_name DROP COLUMN column_name;"=vec![
            Alter::new(
                SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
                None,
                None,
                None,
                None,
                Some("column_name".into()),
            ),
        ],
        alter_drop_column_without_column_keyword: r"ALTER TABLE schema.table_name DROP column_name;"=vec![
            Alter::new(
                SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
                None,
                None,
                None,
                None,
                Some("column_name".into()),
            ),
        ]
    }

    test_group_pass_assert! {
        column_constraint_primary_key,

        primary_key_no_order:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT PRIMARY KEY;"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::PrimaryKey {
                    asc_desc: None,
                    on_conflict: None,
                    autoincrement: false,
                }],
            )),
            None,
        )],

        primary_key_asc:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT PRIMARY KEY ASC;"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::PrimaryKey {
                    asc_desc: Some(Keyword::ASC),
                    on_conflict: None,
                    autoincrement: false,
                }],
            )),
            None,
        )],

        primary_key_desc_conflict_replace_autoincrement:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT PRIMARY KEY DESC ON CONFLICT REPLACE AUTOINCREMENT;"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::PrimaryKey {
                    asc_desc: Some(Keyword::DESC),
                    on_conflict: Some(Keyword::REPLACE),
                    autoincrement: true,
                }],
            )),
            None,
        )]
    }

    test_group_pass_assert! {
        column_constraint_not_null_unique,

        not_null_no_conflict:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT NOT NULL;"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::NotNull { on_conflict: None }],
            )),
            None,
        )],

        unique_conflict_replace:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT UNIQUE ON CONFLICT REPLACE;"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::Unique {
                    on_conflict: Some(Keyword::REPLACE),
                }],
            )),
            None,
        )]
    }

    test_group_pass_assert! {
        column_constraint_misc,

        check_expr:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT CHECK ('literal string lol');"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::Check(
                    Expr::new(
                        Some(Token::new(Type::String("literal string lol".into()))),
                        None, None, None, None
                    )
                )],
            )),
            None,
        )],

        default_literal:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT DEFAULT 'literal';"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::Default {
                    expr: None,
                    literal: Some(Literal {
                        t: Token::new(Type::String("literal".into()))
                    }),
                }],
            )),
            None,
        )],

        collate:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT COLLATE collation_name;"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::Collate("collation_name".into())],
            )),
            None,
        )]
    }

    test_group_pass_assert! {
        column_constraint_generated,

        generated_stored:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT GENERATED ALWAYS AS ('literal') STORED;"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::Generated {
                    expr: Expr::new(
                        Some(Token::new(Type::String("literal".into()))),
                        None, None, None, None
                    ),
                    stored_virtual: Some(Keyword::STORED),
                }],
            )),
            None,
        )],

        as_expr:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT AS ('literal');"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::As{
                    stored_virtual: None,
                    expr: Expr::new(
                        Some(Token::new(Type::String("literal".into()))),
                        None, None, None, None
                    )
                }],
            )),
            None,
        )]
    }

    test_group_pass_assert! {
        foreign_key_clause,

        references_on_delete_set_null:
        r"ALTER TABLE schema.table_name ADD COLUMN column_name TEXT REFERENCES foreign_table ON DELETE SET NULL;"=
        vec![Alter::new(
            SchemaTableContainer::SchemaAndTable { schema: "schema".into(), table: "table_name".into() },
            None, None, None,
            Some(ColumnDef::new(
                "column_name".into(),
                Some(SqliteStorageClass::Text),
                vec![ColumnConstraint::ForeignKey(ForeignKeyClause {
                    foreign_table: "foreign_table".into(),
                    references_columns: vec![],
                    on_delete: Some(ForeignKeyAction::SetNull),
                    on_update: None,
                    match_type: None,
                    deferrable: false,
                    initially_deferred: false,
                })],
            )),
            None,
        )]
    }

    test_group_pass_assert! {
        pragma,

        query:"PRAGMA database_list;"=vec![Pragma::new(SchemaTableContainer::Table("database_list".into()), PragmaInvocation::Query)],
        assignment:"PRAGMA schema.cache_size = 5;"=vec![
            Pragma::new(
                SchemaTableContainer::SchemaAndTable{
                    schema: "schema".into(),
                    table: "cache_size".into(),
                },
                PragmaInvocation::Assign { value: Token::new(Type::Number(5.0)) }
            )],
        assign_keyword:"PRAGMA schema.locking_mode = EXCLUSIVE;"=vec![
            Pragma::new(
                SchemaTableContainer::SchemaAndTable{
                    schema: "schema".into(),
                    table: "locking_mode".into(),
                },
                PragmaInvocation::Assign { value: Token::new(Type::Keyword(Keyword::EXCLUSIVE)) }
            )],
        call:"PRAGMA schema.optimize(0xfffe);"=vec![
            Pragma::new(
            SchemaTableContainer::SchemaAndTable{
                schema: "schema".into(),
                table: "optimize".into(),
            },
            PragmaInvocation::Call { value: Token::new(Type::Number(0xfffe as f64)) }
            )]
    }
}

#[allow(unused_macros)]
macro_rules! test_group_pass {
    ($group_name:ident,$($ident:ident:$input:literal),*) => {
    mod $group_name {
        use crate::{lexer, parser::Parser};

        $(
            #[test]
            fn $ident() {
                let input = $input.as_bytes().to_vec();
                let mut l = lexer::Lexer::new(&input, "parser_test_pass");
                let toks = l.run();
                assert_eq!(l.errors.len(), 0, "lexer errors: {:?}", l.errors);

                let mut parser = Parser::new(toks, "parser_test_pass");
                let _ = parser.parse();
                assert_eq!(parser.errors.len(), 0, "parser errors: {:?}", parser.errors);
            }
        )*
        }
    };
}

#[allow(unused_macros)]
macro_rules! test_group_fail {
    ($group_name:ident,$($ident:ident:$input:literal),*) => {
    mod $group_name {
        use crate::{lexer, parser::Parser};

        $(
            #[test]
            fn $ident() {
                let input = $input.as_bytes().to_vec();
                let mut l = lexer::Lexer::new(&input, "parser_test_fail");
                let toks = l.run();
                assert_eq!(l.errors.len(), 0);

                let mut parser = Parser::new(toks, "parser_test_fail");
                let _ = parser.parse();
                assert_ne!(parser.errors.len(), 0);
            }
        )*
        }
    };
}

#[cfg(test)]
mod should_pass_select {
    // ── Basic SELECT ─────────────────────────────────────────────────────
    test_group_pass! {
        select_basic,
        select_star:                "SELECT *;",
        select_literal_number:      "SELECT 1;",
        select_literal_string:      "SELECT 'hello';",
        select_literal_null:        "SELECT NULL;",
        select_literal_true:        "SELECT true;",
        select_literal_false:       "SELECT false;",
        select_literal_blob:        "SELECT X'AABB';",
        select_current_time:        "SELECT CURRENT_TIME;",
        select_current_date:        "SELECT CURRENT_DATE;",
        select_current_timestamp:   "SELECT CURRENT_TIMESTAMP;",
        select_multiple_cols:       "SELECT 1, 2, 3;",
        select_column_name:         "SELECT name;",
        select_table_column:        "SELECT t.name;",
        select_schema_table_column: "SELECT s.t.name;",
        select_table_star:          "SELECT t.*;",
        select_alias_as:            "SELECT 1 AS one;",
        select_alias_implicit:      "SELECT 1 one;",
        select_distinct:            "SELECT DISTINCT name;",
        select_all:                 "SELECT ALL name;",
        select_mixed_cols:          "SELECT id, name AS n, t.*, *;",
        select_negative_number:     "SELECT -1;",
        select_positive_number:     "SELECT +42;",
        select_bitwise_not:         "SELECT ~0;"
    }

    // ── FROM clause ──────────────────────────────────────────────────────
    test_group_pass! {
        select_from,
        from_table:                 "SELECT * FROM users;",
        from_table_alias:           "SELECT * FROM users u;",
        from_table_alias_as:        "SELECT * FROM users AS u;",
        from_schema_table:          "SELECT * FROM main.users;",
        from_multiple_tables:       "SELECT * FROM users, orders;",
        from_subquery:              "SELECT * FROM (SELECT 1);",
        from_subquery_alias:        "SELECT * FROM (SELECT 1) AS t;",
        from_paren_tables:          "SELECT * FROM (users, orders);",
        from_indexed_by:            "SELECT * FROM users INDEXED BY idx_name;",
        from_not_indexed:           "SELECT * FROM users NOT INDEXED;",
        from_table_function:        "SELECT * FROM json_each('[]');",
        from_schema_table_func:     "SELECT * FROM main.json_each('[]');"
    }

    // ── JOINs ────────────────────────────────────────────────────────────
    test_group_pass! {
        select_join,
        inner_join:                 "SELECT * FROM a JOIN b ON a.id = b.id;",
        inner_join_explicit:        "SELECT * FROM a INNER JOIN b ON a.id = b.id;",
        left_join:                  "SELECT * FROM a LEFT JOIN b ON a.id = b.id;",
        left_outer_join:            "SELECT * FROM a LEFT OUTER JOIN b ON a.id = b.id;",
        right_join:                 "SELECT * FROM a RIGHT JOIN b ON a.id = b.id;",
        full_join:                  "SELECT * FROM a FULL JOIN b ON a.id = b.id;",
        full_outer_join:            "SELECT * FROM a FULL OUTER JOIN b ON a.id = b.id;",
        cross_join:                 "SELECT * FROM a CROSS JOIN b;",
        natural_join:               "SELECT * FROM a NATURAL JOIN b;",
        natural_left_join:          "SELECT * FROM a NATURAL LEFT JOIN b;",
        join_using:                 "SELECT * FROM a JOIN b USING (id);",
        join_using_multi:           "SELECT * FROM a JOIN b USING (id, name);",
        multiple_joins:             "SELECT * FROM a JOIN b ON a.id = b.a_id JOIN c ON b.id = c.b_id;",
        mixed_comma_and_join:       "SELECT * FROM a, b JOIN c ON b.id = c.b_id;"
    }

    // ── WHERE clause ─────────────────────────────────────────────────────
    test_group_pass! {
        select_where,
        where_simple:               "SELECT * FROM t WHERE id = 1;",
        where_and:                  "SELECT * FROM t WHERE a = 1 AND b = 2;",
        where_or:                   "SELECT * FROM t WHERE a = 1 OR b = 2;",
        where_not:                  "SELECT * FROM t WHERE NOT active;",
        where_is_null:              "SELECT * FROM t WHERE name IS NULL;",
        where_is_not_null:          "SELECT * FROM t WHERE name IS NOT NULL;",
        where_isnull:               "SELECT * FROM t WHERE name ISNULL;",
        where_notnull:              "SELECT * FROM t WHERE name NOTNULL;",
        where_in_list:              "SELECT * FROM t WHERE id IN (1, 2, 3);",
        where_not_in_list:          "SELECT * FROM t WHERE id NOT IN (1, 2, 3);",
        where_in_subquery:          "SELECT * FROM t WHERE id IN (SELECT id FROM other);",
        where_like:                 "SELECT * FROM t WHERE name LIKE '%test%';",
        where_like_escape:          "SELECT * FROM t WHERE name LIKE '%\\%%' ESCAPE '\\';",
        where_not_like:             "SELECT * FROM t WHERE name NOT LIKE 'test';",
        where_glob:                 "SELECT * FROM t WHERE name GLOB '*test*';",
        where_between:              "SELECT * FROM t WHERE id BETWEEN 1 AND 10;",
        where_not_between:          "SELECT * FROM t WHERE id NOT BETWEEN 1 AND 10;",
        where_exists:               "SELECT * FROM t WHERE EXISTS (SELECT 1 FROM other);",
        where_complex:              "SELECT * FROM t WHERE (a = 1 OR b = 2) AND NOT (c = 3);"
    }

    // ── Expressions & operators ──────────────────────────────────────────
    test_group_pass! {
        select_expressions,
        expr_addition:              "SELECT 1 + 2;",
        expr_subtraction:           "SELECT 5 - 3;",
        expr_multiplication:        "SELECT 2 * 3;",
        expr_division:              "SELECT 10 / 2;",
        expr_modulo:                "SELECT 10 % 3;",
        expr_concat:                "SELECT 'a' || 'b';",
        expr_comparison_lt:         "SELECT 1 < 2;",
        expr_comparison_gt:         "SELECT 2 > 1;",
        expr_comparison_le:         "SELECT 1 <= 2;",
        expr_comparison_ge:         "SELECT 2 >= 1;",
        expr_comparison_ne:         "SELECT 1 != 2;",
        expr_comparison_ne2:        "SELECT 1 <> 2;",
        expr_comparison_eq2:        "SELECT 1 == 1;",
        expr_bitwise_and:           "SELECT 5 & 3;",
        expr_bitwise_or:            "SELECT 5 | 3;",
        expr_shift_left:            "SELECT 1 << 3;",
        expr_shift_right:           "SELECT 8 >> 2;",
        expr_precedence:            "SELECT 1 + 2 * 3;",
        expr_paren:                 "SELECT (1 + 2) * 3;",
        expr_nested_paren:          "SELECT ((1 + 2) * (3 - 1));",
        expr_unary_minus:           "SELECT -x FROM t;",
        expr_unary_plus:            "SELECT +x FROM t;",
        expr_complex:               "SELECT a + b * c - d / e FROM t;"
    }

    // ── CAST ─────────────────────────────────────────────────────────────
    test_group_pass! {
        select_cast,
        cast_basic:                 "SELECT CAST(1 AS TEXT);",
        cast_in_where:              "SELECT * FROM t WHERE CAST(val AS INTEGER) > 5;"
    }

    // ── CASE ─────────────────────────────────────────────────────────────
    test_group_pass! {
        select_case,
        case_simple:                "SELECT CASE WHEN 1 THEN 'one' END;",
        case_multiple_when:         "SELECT CASE WHEN 1 THEN 'one' WHEN 2 THEN 'two' END;",
        case_else:                  "SELECT CASE WHEN 1 THEN 'one' ELSE 'other' END;",
        case_with_operand:          "SELECT CASE x WHEN 1 THEN 'one' WHEN 2 THEN 'two' ELSE 'other' END FROM t;",
        case_nested:                "SELECT CASE WHEN a > 0 THEN CASE WHEN a > 10 THEN 'big' ELSE 'small' END ELSE 'neg' END FROM t;"
    }

    // ── Function calls ───────────────────────────────────────────────────
    test_group_pass! {
        select_functions,
        func_count_star:            "SELECT COUNT(*) FROM t;",
        func_count_col:             "SELECT COUNT(id) FROM t;",
        func_count_distinct:        "SELECT COUNT(DISTINCT name) FROM t;",
        func_sum:                   "SELECT SUM(amount) FROM t;",
        func_avg:                   "SELECT AVG(price) FROM t;",
        func_min:                   "SELECT MIN(id) FROM t;",
        func_max:                   "SELECT MAX(id) FROM t;",
        func_no_args:               "SELECT random();",
        func_multi_args:            "SELECT COALESCE(a, b, c) FROM t;",
        func_nested:                "SELECT MAX(ABS(x)) FROM t;",
        func_upper:                 "SELECT UPPER(name) FROM t;",
        func_substr:                "SELECT SUBSTR(name, 1, 3) FROM t;",
        func_ifnull:                "SELECT IFNULL(a, 0) FROM t;",
        func_in_where:              "SELECT * FROM t WHERE LENGTH(name) > 5;"
    }

    // ── GROUP BY / HAVING ────────────────────────────────────────────────
    test_group_pass! {
        select_group_by,
        group_by_single:            "SELECT name, COUNT(*) FROM t GROUP BY name;",
        group_by_multiple:          "SELECT a, b, COUNT(*) FROM t GROUP BY a, b;",
        group_by_having:            "SELECT name, COUNT(*) FROM t GROUP BY name HAVING COUNT(*) > 1;",
        group_by_expr:              "SELECT SUBSTR(name, 1, 1), COUNT(*) FROM t GROUP BY SUBSTR(name, 1, 1);"
    }

    // ── ORDER BY ─────────────────────────────────────────────────────────
    test_group_pass! {
        select_order_by,
        order_by_single:            "SELECT * FROM t ORDER BY id;",
        order_by_asc:               "SELECT * FROM t ORDER BY id ASC;",
        order_by_desc:              "SELECT * FROM t ORDER BY id DESC;",
        order_by_multiple:          "SELECT * FROM t ORDER BY a ASC, b DESC;",
        order_by_collate:           "SELECT * FROM t ORDER BY name COLLATE NOCASE;",
        order_by_nulls_first:       "SELECT * FROM t ORDER BY name NULLS FIRST;",
        order_by_nulls_last:        "SELECT * FROM t ORDER BY name ASC NULLS LAST;",
        order_by_expr:              "SELECT * FROM t ORDER BY a + b;"
    }

    // ── LIMIT / OFFSET ──────────────────────────────────────────────────
    test_group_pass! {
        select_limit,
        limit_only:                 "SELECT * FROM t LIMIT 10;",
        limit_offset:               "SELECT * FROM t LIMIT 10 OFFSET 20;",
        limit_comma_syntax:         "SELECT * FROM t LIMIT 20, 10;",
        limit_expr:                 "SELECT * FROM t LIMIT 5 + 5;"
    }

    // ── Subqueries ───────────────────────────────────────────────────────
    test_group_pass! {
        select_subqueries,
        subquery_in_from:           "SELECT * FROM (SELECT id, name FROM users) AS t;",
        subquery_in_where:          "SELECT * FROM t WHERE id = (SELECT MAX(id) FROM t);",
        subquery_in_select:         "SELECT (SELECT COUNT(*) FROM t) AS cnt;",
        subquery_exists:            "SELECT * FROM t WHERE EXISTS (SELECT 1 FROM other WHERE other.tid = t.id);",
        subquery_in_select_list:    "SELECT id, (SELECT name FROM users WHERE users.id = orders.uid) FROM orders;",
        correlated_subquery:        "SELECT * FROM t1 WHERE t1.a > (SELECT AVG(t2.a) FROM t2 WHERE t2.b = t1.b);"
    }

    // ── CTEs (WITH) ──────────────────────────────────────────────────────
    test_group_pass! {
        select_cte,
        cte_simple:                 "WITH t AS (SELECT 1 AS x) SELECT * FROM t;",
        cte_columns:                "WITH t(a, b) AS (SELECT 1, 2) SELECT * FROM t;",
        cte_multiple:               "WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a, b;",
        cte_recursive:              "WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM cnt WHERE x < 10) SELECT x FROM cnt;",
        cte_materialized:           "WITH t AS MATERIALIZED (SELECT 1) SELECT * FROM t;",
        cte_not_materialized:       "WITH t AS NOT MATERIALIZED (SELECT 1) SELECT * FROM t;"
    }

    // ── Compound SELECT (UNION / INTERSECT / EXCEPT) ─────────────────────
    test_group_pass! {
        select_compound,
        union_basic:                "SELECT 1 UNION SELECT 2;",
        union_all:                  "SELECT 1 UNION ALL SELECT 2;",
        intersect_basic:            "SELECT 1 INTERSECT SELECT 1;",
        except_basic:               "SELECT 1 EXCEPT SELECT 2;",
        union_multiple:             "SELECT 1 UNION SELECT 2 UNION SELECT 3;",
        compound_with_order:        "SELECT a FROM t1 UNION SELECT b FROM t2 ORDER BY 1;",
        compound_with_limit:        "SELECT a FROM t1 UNION ALL SELECT b FROM t2 LIMIT 10;"
    }

    // ── VALUES ───────────────────────────────────────────────────────────
    test_group_pass! {
        select_values,
        values_single:              "VALUES (1, 'a');",
        values_multiple:            "VALUES (1, 'a'), (2, 'b'), (3, 'c');",
        values_in_cte:              "WITH t(a, b) AS (VALUES (1, 'x'), (2, 'y')) SELECT * FROM t;"
    }

    // ── Window functions ─────────────────────────────────────────────────
    test_group_pass! {
        select_window,
        window_row_number:          "SELECT row_number() OVER (ORDER BY id) FROM t;",
        window_partition:           "SELECT SUM(amt) OVER (PARTITION BY cat) FROM t;",
        window_partition_order:     "SELECT SUM(amt) OVER (PARTITION BY cat ORDER BY dt) FROM t;",
        window_named:               "SELECT SUM(amt) OVER w FROM t WINDOW w AS (PARTITION BY cat ORDER BY dt);",
        window_multiple_named:      "SELECT SUM(a) OVER w1, AVG(b) OVER w2 FROM t WINDOW w1 AS (ORDER BY a), w2 AS (ORDER BY b);",
        window_rows_frame:          "SELECT SUM(a) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t;",
        window_range_unbounded:     "SELECT SUM(a) OVER (ORDER BY id RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM t;",
        window_filter:              "SELECT COUNT(*) FILTER (WHERE a > 0) OVER (ORDER BY id) FROM t;",
        window_rows_current:        "SELECT AVG(x) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING) FROM t;",
        window_groups_frame:        "SELECT SUM(x) OVER (ORDER BY id GROUPS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t;"
    }

    // ── Bind parameters in SELECT ────────────────────────────────────────
    test_group_pass! {
        select_bind_params,
        bind_question:              "SELECT * FROM t WHERE id = ?1;",
        bind_colon:                 "SELECT * FROM t WHERE id = :id;",
        bind_at:                    "SELECT * FROM t WHERE id = @id;",
        bind_dollar:                "SELECT * FROM t WHERE id = $id;",
        bind_in_limit:              "SELECT * FROM t LIMIT ?1 OFFSET ?2;"
    }

    // ── Complex / real-world queries ─────────────────────────────────────
    test_group_pass! {
        select_complex,
        complex_kitchen_sink:
            "SELECT u.id, u.name, COUNT(o.id) AS order_count, SUM(o.total) AS total_spent FROM users u LEFT JOIN orders o ON u.id = o.user_id WHERE u.active = 1 AND o.date > '2024-01-01' GROUP BY u.id, u.name HAVING COUNT(o.id) > 0 ORDER BY total_spent DESC LIMIT 10;",

        complex_nested_subqueries:
            "SELECT * FROM (SELECT id, name FROM users WHERE id IN (SELECT user_id FROM orders WHERE total > 100)) AS big_spenders ORDER BY name;",

        complex_cte_with_join:
            "WITH active_users AS (SELECT * FROM users WHERE active = 1) SELECT au.name, COUNT(o.id) FROM active_users au JOIN orders o ON au.id = o.user_id GROUP BY au.name;",

        complex_recursive_cte:
            "WITH RECURSIVE tree(id, parent_id, depth) AS (SELECT id, parent_id, 0 FROM nodes WHERE parent_id IS NULL UNION ALL SELECT n.id, n.parent_id, t.depth + 1 FROM nodes n JOIN tree t ON n.parent_id = t.id) SELECT * FROM tree ORDER BY depth, id;",

        complex_window_analytics:
            "SELECT id, amount, SUM(amount) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS running_total, AVG(amount) OVER (ORDER BY id ROWS BETWEEN 2 PRECEDING AND 2 FOLLOWING) AS moving_avg FROM transactions;",

        complex_case_in_select:
            "SELECT id, CASE WHEN status = 1 THEN 'active' WHEN status = 2 THEN 'suspended' ELSE 'unknown' END AS status_text FROM users;",

        complex_multiple_unions:
            "SELECT 'users' AS source, COUNT(*) AS cnt FROM users UNION ALL SELECT 'orders', COUNT(*) FROM orders UNION ALL SELECT 'products', COUNT(*) FROM products ORDER BY cnt DESC;",

        complex_correlated_exists:
            "SELECT * FROM users u WHERE EXISTS (SELECT 1 FROM orders o WHERE o.user_id = u.id AND o.total > 1000) AND NOT EXISTS (SELECT 1 FROM bans b WHERE b.user_id = u.id);",

        complex_multi_join:
            "SELECT p.name, c.name AS category, SUM(oi.quantity) AS total_sold FROM products p INNER JOIN categories c ON p.category_id = c.id LEFT JOIN order_items oi ON p.id = oi.product_id GROUP BY p.name, c.name ORDER BY total_sold DESC NULLS LAST LIMIT 20 OFFSET 0;",

        explain_select:
            "EXPLAIN SELECT * FROM users;",

        explain_query_plan_select:
            "EXPLAIN QUERY PLAN SELECT * FROM users WHERE id > 5;"
    }
}

#[cfg(test)]
mod should_pass_create_table {
    test_group_pass! {
        create_table_basic,
        simple:
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
        if_not_exists:
            "CREATE TABLE IF NOT EXISTS users (id INTEGER, name TEXT);",
        temp:
            "CREATE TEMP TABLE tmp (val TEXT);",
        temporary:
            "CREATE TEMPORARY TABLE tmp (val TEXT);",
        multiple_columns:
            "CREATE TABLE t (a INTEGER, b TEXT, c REAL, d BLOB);",
        with_constraints:
            "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL);",
        with_default:
            "CREATE TABLE t (id INTEGER, status TEXT DEFAULT 'active');",
        with_foreign_key:
            "CREATE TABLE orders (id INTEGER, user_id INTEGER REFERENCES users(id));",
        schema_qualified:
            "CREATE TABLE main.users (id INTEGER);",
        without_rowid:
            "CREATE TABLE t (id INTEGER PRIMARY KEY) WITHOUT ROWID;",
        multiple_tables:
            "CREATE TABLE a (id INTEGER); CREATE TABLE b (id INTEGER);",
        create_as_select:
            "CREATE TABLE archive AS SELECT * FROM users;"
    }
}

#[cfg(test)]
mod should_pass_insert {
    test_group_pass! {
        insert_basic,
        simple_values:
            "INSERT INTO users VALUES (1, 'Alice');",
        with_columns:
            "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com');",
        multiple_rows:
            "INSERT INTO users VALUES (1, 'Alice'), (2, 'Bob');",
        or_replace:
            "INSERT OR REPLACE INTO users VALUES (1, 'Alice');",
        or_ignore:
            "INSERT OR IGNORE INTO users VALUES (1, 'Alice');",
        or_abort:
            "INSERT OR ABORT INTO users VALUES (1, 'Alice');",
        or_rollback:
            "INSERT OR ROLLBACK INTO users VALUES (1, 'Alice');",
        or_fail:
            "INSERT OR FAIL INTO users VALUES (1, 'Alice');",
        replace_shorthand:
            "REPLACE INTO users VALUES (1, 'Alice');",
        default_values:
            "INSERT INTO users DEFAULT VALUES;",
        insert_select:
            "INSERT INTO archive SELECT * FROM users;",
        schema_qualified:
            "INSERT INTO main.users VALUES (1, 'Alice');",
        with_expressions:
            "INSERT INTO t VALUES (1 + 2, 'hello' || ' world');"
    }
}

#[cfg(test)]
mod should_pass_update {
    test_group_pass! {
        update_basic,
        simple:
            "UPDATE users SET name = 'Bob';",
        with_where:
            "UPDATE users SET name = 'Bob' WHERE id = 1;",
        multiple_set:
            "UPDATE users SET name = 'Bob', email = 'bob@example.com' WHERE id = 1;",
        or_replace:
            "UPDATE OR REPLACE users SET name = 'Bob';",
        or_ignore:
            "UPDATE OR IGNORE users SET name = 'Bob';",
        schema_qualified:
            "UPDATE main.users SET name = 'Bob';",
        with_alias:
            "UPDATE users AS u SET name = 'Bob' WHERE u.id = 1;",
        with_expression:
            "UPDATE inventory SET qty = qty - 1 WHERE product_id = 42;"
    }
}

#[cfg(test)]
mod should_pass_delete {
    test_group_pass! {
        delete_basic,
        simple:
            "DELETE FROM users;",
        with_where:
            "DELETE FROM users WHERE id = 1;",
        schema_qualified:
            "DELETE FROM main.users WHERE id = 1;",
        complex_where:
            "DELETE FROM log WHERE created_at < '2024-01-01' AND level = 'debug';"
    }
}

#[cfg(test)]
mod should_fail_create_insert_update_delete {
    test_group_fail! {
        create_table_negative,
        create_no_table_name:       "CREATE TABLE;",
        create_no_columns:          "CREATE TABLE t;",
        create_no_paren:            "CREATE TABLE t id INTEGER;"
    }

    test_group_fail! {
        insert_negative,
        insert_no_into:             "INSERT VALUES (1);",
        insert_no_table:            "INSERT INTO;",
        insert_no_values:           "INSERT INTO t;"
    }

    test_group_fail! {
        update_negative,
        update_no_table:            "UPDATE;",
        update_no_set:              "UPDATE t;",
        update_set_no_col:          "UPDATE t SET;"
    }

    test_group_fail! {
        delete_negative,
        delete_no_from:             "DELETE;",
        delete_no_table:            "DELETE FROM;"
    }
}

#[cfg(test)]
mod should_fail {
    test_group_fail! {
        negative_tests,
        eof_semi: ";",
        eof_literal: "'str'",
        alter_no_table: "ALTER;",
        alter_no_name: "ALTER TABLE;",
        commit_no_semicolon: "COMMIT",
        end_no_semicolon: "END",
        rollback_no_semicolon: "ROLLBACK",
        rollback_to_savepoint_no_name: "ROLLBACK TO SAVEPOINT",
        begin_no_semicolon: "BEGIN",
        begin_invalid_modifiers: "BEGIN DEFERRED IMMEDIATE EXCLUSIVE EXCLUSIVE;",
        detach_no_name: "DETACH;",
        detach_invalid_literal: "DETACH 'bad';",
        drop_no_object: "DROP TABLE;",
        drop_invalid_object: "DROP INDEX 5;",
        savepoint_no_name: "SAVEPOINT;",
        release_no_name: "RELEASE;",
        reindex_no_name: "REINDEX",
        reindex_invalid_literal: "REINDEX 25;",
        vacuum_no_semicolon: "VACUUM",
        vacuum_invalid_combined: "VACUUM 5 INTO 5;"
    }

    test_group_fail! {
        select_negative,
        select_no_columns:          "SELECT;",
        select_no_semicolon:        "SELECT 1",
        select_where_no_expr:       "SELECT * FROM t WHERE;",
        select_from_no_table:       "SELECT * FROM;",
        select_group_by_no_expr:    "SELECT * FROM t GROUP BY;",
        select_order_by_no_expr:    "SELECT * FROM t ORDER BY;",
        select_limit_no_expr:       "SELECT * FROM t LIMIT;",
        select_join_no_table:       "SELECT * FROM a JOIN;",
        select_join_on_no_expr:     "SELECT * FROM a JOIN b ON;",
        select_bad_cte:             "WITH AS (SELECT 1) SELECT 1;",
        select_unclosed_paren:      "SELECT (1 + 2;",
        select_bad_between:         "SELECT * FROM t WHERE x BETWEEN;",
        select_bad_in:              "SELECT * FROM t WHERE x IN;",
        select_bad_cast:            "SELECT CAST(1;",
        select_bad_case:            "SELECT CASE WHEN THEN 1 END;"
    }
}
