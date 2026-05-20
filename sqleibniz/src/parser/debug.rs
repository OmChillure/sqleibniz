use crate::{
    parser::nodes::*,
    types::{Keyword, Token, Type, storage::SqliteStorageClass},
};

/// impl FieldSerializable for $tt via serde_json::to_value(self).unwrap()
macro_rules! impl_field_serializable_with_serde_to_value {
    ($($tt:tt),*) => {
        $(
            impl FieldSerializable for $tt {
                fn field_as_serializable(&self) -> serde_json::Value {
                    serde_json::to_value(self).unwrap()
                }
            }
        )*
    };
}

pub trait FieldSerializable {
    fn field_as_serializable(&self) -> serde_json::Value;
}

impl_field_serializable_with_serde_to_value!(
    String,
    bool,
    Keyword,
    SqliteStorageClass,
    SchemaTableContainer,
    Type,
    PragmaInvocation,
    CompoundOp,
    JoinType,
    IndexedBy,
    FrameExclude
);

impl FieldSerializable for ColumnConstraint {
    fn field_as_serializable(&self) -> serde_json::Value {
        let name = match self {
            ColumnConstraint::PrimaryKey { .. } => "primary_key",
            ColumnConstraint::NotNull { .. } => "not_null",
            ColumnConstraint::Unique { .. } => "unique",
            ColumnConstraint::Check(_) => "check",
            ColumnConstraint::Default { .. } => "default",
            ColumnConstraint::Collate(_) => "collate",
            ColumnConstraint::Generated { .. } => "generated",
            ColumnConstraint::As { .. } => "as",
            ColumnConstraint::ForeignKey(_) => "foreign_key",
        };
        let inner = match self {
            ColumnConstraint::PrimaryKey {
                asc_desc,
                on_conflict,
                autoincrement,
            } => {
                serde_json::json!( {
                    "asc_desc": asc_desc,
                    "on_conflict": on_conflict,
                    "autoincrement": autoincrement
                })
            }
            ColumnConstraint::Unique { on_conflict }
            | ColumnConstraint::NotNull { on_conflict } => {
                serde_json::json!({
                   "on_conflict": on_conflict
                })
            }

            ColumnConstraint::ForeignKey(foreign_key_clause) => {
                serde_json::json!({
                   "foreign_key_clause": foreign_key_clause
                })
            }
            ColumnConstraint::Collate(str) => serde_json::json!(str),
            ColumnConstraint::Check(expr) => serde_json::json!({
                "expr": expr.as_serializable(),
            }),
            ColumnConstraint::Default { expr, literal } => {
                serde_json::json!({
                    "expr": match expr {
                        Some(e) => e.as_serializable(),
                        None => serde_json::Value::Null,
                    },
                    "literal": match literal {
                        Some(e) => e.as_serializable(),
                        None => serde_json::Value::Null,
                    },
                })
            }
            ColumnConstraint::Generated {
                expr,
                stored_virtual,
            }
            | ColumnConstraint::As {
                expr,
                stored_virtual,
            } => serde_json::json!({
                "expr": expr.as_serializable(),
                "stored_virtual": stored_virtual,
            }),
        };
        serde_json::json!({
            name: inner
        })
    }
}

// ── FieldSerializable for SELECT-related types ──────────────────────────────

impl FieldSerializable for SqlExpr {
    fn field_as_serializable(&self) -> serde_json::Value {
        match self {
            SqlExpr::Literal(tok) => serde_json::json!({
                "kind": "literal",
                "value": tok.field_as_serializable(),
            }),
            SqlExpr::ColumnRef { token: _, schema, table, column } => serde_json::json!({
                "kind": "column_ref",
                "schema": schema,
                "table": table,
                "column": column,
            }),
            SqlExpr::Star => serde_json::json!({"kind": "star"}),
            SqlExpr::BindParam { token, name, counter } => serde_json::json!({
                "kind": "bind_param",
                "token": token.field_as_serializable(),
                "name": name,
                "counter": counter.as_ref().map(|t| t.field_as_serializable()),
            }),
            SqlExpr::UnaryOp { op, operand } => serde_json::json!({
                "kind": "unary_op",
                "op": op.field_as_serializable(),
                "operand": operand.field_as_serializable(),
            }),
            SqlExpr::BinaryOp { left, op, right } => serde_json::json!({
                "kind": "binary_op",
                "left": left.field_as_serializable(),
                "op": op.field_as_serializable(),
                "right": right.field_as_serializable(),
            }),
            SqlExpr::FunctionCall { name, distinct, args } => serde_json::json!({
                "kind": "function_call",
                "name": name,
                "distinct": distinct,
                "args": args.iter().map(|a| a.field_as_serializable()).collect::<Vec<_>>(),
            }),
            SqlExpr::WindowFunctionCall { name, distinct, args, filter, over } => serde_json::json!({
                "kind": "window_function_call",
                "name": name,
                "distinct": distinct,
                "args": args.iter().map(|a| a.field_as_serializable()).collect::<Vec<_>>(),
                "filter": filter.as_ref().map(|f| f.field_as_serializable()),
                "over": over.field_as_serializable(),
            }),
            SqlExpr::Paren(expr) => serde_json::json!({
                "kind": "paren",
                "expr": expr.field_as_serializable(),
            }),
            SqlExpr::Subquery(sel) => serde_json::json!({
                "kind": "subquery",
                "select": sel.as_serializable(),
            }),
            SqlExpr::Exists { negated, subquery } => serde_json::json!({
                "kind": "exists",
                "negated": negated,
                "subquery": subquery.as_serializable(),
            }),
            SqlExpr::InList { expr, negated, values } => serde_json::json!({
                "kind": "in_list",
                "expr": expr.field_as_serializable(),
                "negated": negated,
                "values": values.iter().map(|v| v.field_as_serializable()).collect::<Vec<_>>(),
            }),
            SqlExpr::InSelect { expr, negated, subquery } => serde_json::json!({
                "kind": "in_select",
                "expr": expr.field_as_serializable(),
                "negated": negated,
                "subquery": subquery.as_serializable(),
            }),
            SqlExpr::Between { expr, negated, low, high } => serde_json::json!({
                "kind": "between",
                "expr": expr.field_as_serializable(),
                "negated": negated,
                "low": low.field_as_serializable(),
                "high": high.field_as_serializable(),
            }),
            SqlExpr::IsNull { expr, negated } => serde_json::json!({
                "kind": "is_null",
                "expr": expr.field_as_serializable(),
                "negated": negated,
            }),
            SqlExpr::Like { expr, negated, op, pattern, escape } => serde_json::json!({
                "kind": "like",
                "expr": expr.field_as_serializable(),
                "negated": negated,
                "op": op,
                "pattern": pattern.field_as_serializable(),
                "escape": escape.as_ref().map(|e| e.field_as_serializable()),
            }),
            SqlExpr::Cast { expr, type_name } => serde_json::json!({
                "kind": "cast",
                "expr": expr.field_as_serializable(),
                "type_name": type_name,
            }),
            SqlExpr::Case { operand, when_clauses, else_clause } => serde_json::json!({
                "kind": "case",
                "operand": operand.as_ref().map(|e| e.field_as_serializable()),
                "when_clauses": when_clauses.iter().map(|(w, t)| serde_json::json!({
                    "when": w.field_as_serializable(),
                    "then": t.field_as_serializable(),
                })).collect::<Vec<_>>(),
                "else": else_clause.as_ref().map(|e| e.field_as_serializable()),
            }),
            SqlExpr::Collate { expr, collation } => serde_json::json!({
                "kind": "collate",
                "expr": expr.field_as_serializable(),
                "collation": collation,
            }),
        }
    }
}

impl FieldSerializable for ResultColumn {
    fn field_as_serializable(&self) -> serde_json::Value {
        match self {
            ResultColumn::Star => serde_json::json!({"kind": "star"}),
            ResultColumn::TableStar(t) => serde_json::json!({"kind": "table_star", "table": t}),
            ResultColumn::Expr { expr, alias } => serde_json::json!({
                "kind": "expr",
                "expr": expr.field_as_serializable(),
                "alias": alias,
            }),
        }
    }
}

impl FieldSerializable for FromClause {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "first": self.first.field_as_serializable(),
            "joins": self.joins.iter().map(|j| j.field_as_serializable()).collect::<Vec<_>>(),
        })
    }
}

impl FieldSerializable for TableRef {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "source": self.source.field_as_serializable(),
            "alias": self.alias,
            "indexed": self.indexed.as_ref().map(|i| i.field_as_serializable()),
        })
    }
}

impl FieldSerializable for Box<SelectStmt> {
    fn field_as_serializable(&self) -> serde_json::Value {
        Node::as_serializable(self.as_ref())
    }
}

impl FieldSerializable for SetClause {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "column": self.column,
            "expr": self.expr.field_as_serializable(),
        })
    }
}

impl FieldSerializable for TableSource {
    fn field_as_serializable(&self) -> serde_json::Value {
        match self {
            TableSource::Table { schema, name } => serde_json::json!({
                "kind": "table", "schema": schema, "name": name,
            }),
            TableSource::TableFunction { schema, name, args } => serde_json::json!({
                "kind": "table_function", "schema": schema, "name": name,
                "args": args.iter().map(|a| a.field_as_serializable()).collect::<Vec<_>>(),
            }),
            TableSource::Subquery(sel) => serde_json::json!({
                "kind": "subquery", "select": sel.as_serializable(),
            }),
            TableSource::ParenFrom(from) => serde_json::json!({
                "kind": "paren_from", "from": from.field_as_serializable(),
            }),
        }
    }
}

impl FieldSerializable for JoinItem {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "natural": self.natural,
            "join_type": self.join_type.field_as_serializable(),
            "table": self.table.field_as_serializable(),
            "constraint": self.constraint.as_ref().map(|c| c.field_as_serializable()),
        })
    }
}

impl FieldSerializable for JoinConstraint {
    fn field_as_serializable(&self) -> serde_json::Value {
        match self {
            JoinConstraint::On(e) => serde_json::json!({"kind": "on", "expr": e.field_as_serializable()}),
            JoinConstraint::Using(cols) => serde_json::json!({"kind": "using", "columns": cols}),
        }
    }
}

impl FieldSerializable for OrderingTerm {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "expr": self.expr.field_as_serializable(),
            "collation": self.collation,
            "asc_desc": self.asc_desc,
            "nulls": self.nulls,
        })
    }
}

impl FieldSerializable for CompoundSelect {
    fn field_as_serializable(&self) -> serde_json::Value {
        let mut rest = vec![];
        for (op, core) in &self.rest {
            rest.push(serde_json::json!({
                "op": op.field_as_serializable(),
                "core": core.field_as_serializable(),
            }));
        }
        serde_json::json!({
            "first": self.first.field_as_serializable(),
            "rest": rest,
        })
    }
}

impl FieldSerializable for SelectCore {
    fn field_as_serializable(&self) -> serde_json::Value {
        if self.is_values {
            return serde_json::json!({
                "kind": "values",
                "rows": self.values_rows.iter().map(|row|
                    row.iter().map(|e| e.field_as_serializable()).collect::<Vec<_>>()
                ).collect::<Vec<_>>(),
            });
        }
        serde_json::json!({
            "kind": "select",
            "distinct": self.distinct,
            "columns": self.columns.iter().map(|c| c.field_as_serializable()).collect::<Vec<_>>(),
            "from": self.from.as_ref().map(|f| f.field_as_serializable()),
            "where": self.where_clause.as_ref().map(|e| e.field_as_serializable()),
            "group_by": self.group_by.iter().map(|e| e.field_as_serializable()).collect::<Vec<_>>(),
            "having": self.having.as_ref().map(|e| e.field_as_serializable()),
            "windows": self.windows.iter().map(|w| w.field_as_serializable()).collect::<Vec<_>>(),
        })
    }
}

impl FieldSerializable for CommonTableExpr {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "columns": self.columns,
            "materialized": self.materialized,
            "select": self.select.as_serializable(),
        })
    }
}

impl FieldSerializable for NamedWindowDef {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "def": self.def.field_as_serializable(),
        })
    }
}

impl FieldSerializable for WindowOver {
    fn field_as_serializable(&self) -> serde_json::Value {
        match self {
            WindowOver::Name(n) => serde_json::json!({"kind": "name", "name": n}),
            WindowOver::Spec(s) => serde_json::json!({"kind": "spec", "spec": s.field_as_serializable()}),
        }
    }
}

impl FieldSerializable for WindowSpec {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "base_window": self.base_window,
            "partition_by": self.partition_by.iter().map(|e| e.field_as_serializable()).collect::<Vec<_>>(),
            "order_by": self.order_by.iter().map(|o| o.field_as_serializable()).collect::<Vec<_>>(),
            "frame": self.frame.as_ref().map(|f| f.field_as_serializable()),
        })
    }
}

impl FieldSerializable for FrameSpec {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::json!({
            "mode": self.mode,
            "start": self.start.as_ref().field_as_serializable(),
            "end": self.end.as_ref().map(|e| e.as_ref().field_as_serializable()),
            "exclude": self.exclude.as_ref().map(|e| e.field_as_serializable()),
        })
    }
}

impl FieldSerializable for FrameBound {
    fn field_as_serializable(&self) -> serde_json::Value {
        match self {
            FrameBound::UnboundedPreceding => serde_json::json!("unbounded_preceding"),
            FrameBound::Preceding(e) => serde_json::json!({"preceding": e.field_as_serializable()}),
            FrameBound::CurrentRow => serde_json::json!("current_row"),
            FrameBound::Following(e) => serde_json::json!({"following": e.field_as_serializable()}),
            FrameBound::UnboundedFollowing => serde_json::json!("unbounded_following"),
        }
    }
}

impl FieldSerializable for Token {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::to_value(&self.ttype).unwrap()
    }
}

impl FieldSerializable for Box<dyn Node> {
    fn field_as_serializable(&self) -> serde_json::Value {
        self.as_serializable()
    }
}

impl<T: FieldSerializable> FieldSerializable for Option<T> {
    fn field_as_serializable(&self) -> serde_json::Value {
        match self {
            Some(n) => n.field_as_serializable(),
            None => serde_json::Value::Null,
        }
    }
}

impl<T: FieldSerializable> FieldSerializable for Vec<T> {
    fn field_as_serializable(&self) -> serde_json::Value {
        serde_json::Value::Array(self.iter().map(|n| n.field_as_serializable()).collect())
    }
}
