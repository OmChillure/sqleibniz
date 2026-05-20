#[derive(Debug, PartialEq, Clone, serde::Serialize)]
/// Rule is attached to each error and can be supplied to sqleibniz via the Config structure serialized in ./leibniz.toml
#[derive(clap::ValueEnum)]
pub enum Rule {
    /// Source file is empty
    NoContent,
    /// Source file is not empty but holds no statements
    NoStatements,
    /// Source file contains constructs sqleibniz does not yet understand
    Unimplemented,
    /// Source file contains an unknown keyword
    UnknownKeyword,
    /// Source file contains invalid sqleibniz instruction
    BadSqleibnizInstruction,
    /// Source file uses sql features sqlite does not support
    SqliteUnsupported,
    /// Sqlite or SQL quirk: https://www.sqlite.org/quirks.html; anything where SQLite deviates
    /// from a stricter, conventional SQL model
    Quirk,
    /// Source file contains an unterminated string
    UnterminatedString,
    /// The source file contains an unknown character
    UnknownCharacter,
    /// The source file contains an invalid numeric literal, either overflow or incorrect syntax
    InvalidNumericLiteral,
    /// The source file contains an invalid blob literal, either bad hex data (a-f,A-F,0-9) or
    /// incorrect syntax
    InvalidBlob,
    /// The source file contains a structure with incorrect syntax
    Syntax,
    /// The source file is missing a semicolon
    Semicolon,
    /// A table referenced in a DML statement does not exist in any CREATE TABLE
    UnknownTable,
    /// A column referenced in a DML statement does not exist in the target table
    UnknownColumn,
    /// An INSERT statement provides a different number of values than the target table has columns
    InsertValueCountMismatch,
    /// A comparison or operation between incompatible SQLite storage classes
    TypeMismatch,
    /// Using = NULL or != NULL instead of IS NULL / IS NOT NULL
    EqualNull,
    /// LIKE pattern contains no wildcards (% or _)
    LikeNoWildcards,
    /// BETWEEN bounds are reversed (low > high)
    ReversedBetween,
    /// UPDATE or DELETE without WHERE clause
    MissingWhere,
    /// Trailing comma in a list (e.g. SELECT a, FROM t)
    TrailingComma,
}

impl mlua::FromLua for Rule {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        let value: String = lua.unpack(value)?;

        Ok(match value.as_str() {
            "NoContent" => Self::NoContent,
            "NoStatements" => Self::NoStatements,
            "Unimplemented" => Self::Unimplemented,
            "UnterminatedString" => Self::UnterminatedString,
            "UnknownCharacter" => Self::UnknownCharacter,
            "InvalidNumericLiteral" => Self::InvalidNumericLiteral,
            "InvalidBlob" => Self::InvalidBlob,
            "Syntax" => Self::Syntax,
            "Semicolon" => Self::Semicolon,
            "BadSqleibnizInstruction" => Self::BadSqleibnizInstruction,
            "UnknownKeyword" => Self::UnknownKeyword,
            "SqliteUnsupported" => Self::SqliteUnsupported,
            "Quirk" => Self::Quirk,
            "UnknownTable" => Self::UnknownTable,
            "UnknownColumn" => Self::UnknownColumn,
            "InsertValueCountMismatch" => Self::InsertValueCountMismatch,
            "TypeMismatch" => Self::TypeMismatch,
            "EqualNull" => Self::EqualNull,
            "LikeNoWildcards" => Self::LikeNoWildcards,
            "ReversedBetween" => Self::ReversedBetween,
            "MissingWhere" => Self::MissingWhere,
            "TrailingComma" => Self::TrailingComma,
            _ => {
                return Err(mlua::Error::FromLuaConversionError {
                    from: "string",
                    to: "sqleibniz::rules::Rule".into(),
                    message: Some("Unknown rule name".into()),
                });
            }
        })
    }
}

impl Rule {
    pub fn name(&self) -> &str {
        match self {
            Self::NoContent => "NoContent",
            Self::NoStatements => "NoStatements",
            Self::Unimplemented => "Unimplemented",
            Self::UnterminatedString => "UnterminatedString",
            Self::UnknownCharacter => "UnknownCharacter",
            Self::InvalidNumericLiteral => "InvalidNumericLiteral",
            Self::InvalidBlob => "InvalidBlob",
            Self::Syntax => "Syntax",
            Self::Quirk => "Quirk",
            Self::Semicolon => "Semicolon",
            Self::BadSqleibnizInstruction => "BadSqleibnizInstruction",
            Self::UnknownKeyword => "UnknownKeyword",
            Self::SqliteUnsupported => "SqliteUnsupported",
            Self::UnknownTable => "UnknownTable",
            Self::UnknownColumn => "UnknownColumn",
            Self::InsertValueCountMismatch => "InsertValueCountMismatch",
            Self::TypeMismatch => "TypeMismatch",
            Self::EqualNull => "EqualNull",
            Self::LikeNoWildcards => "LikeNoWildcards",
            Self::ReversedBetween => "ReversedBetween",
            Self::MissingWhere => "MissingWhere",
            Self::TrailingComma => "TrailingComma",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::NoContent => "Source file is empty",
            Self::NoStatements => "Source file is not empty but holds no statements",
            Self::Unimplemented => {
                "Source file contains constructs sqleibniz does not yet understand"
            }
            Self::UnterminatedString => "Source file contains an unterminated string",
            Self::UnknownCharacter => "The source file contains an unknown character",
            Self::InvalidNumericLiteral => "The source file contains an invalid numeric literal",
            Self::InvalidBlob => "The source file contains an invalid blob literal",
            Self::Syntax => "The source file contains a structure with incorrect syntax",
            Self::Semicolon => "The source file is missing a semicolon",
            Self::BadSqleibnizInstruction => {
                "The source file contains an invalid sqleibniz instruction"
            }
            Self::Quirk => "Sqlite or SQL quirk: https://www.sqlite.org/quirks.html",
            Self::UnknownKeyword => "Source file contains an unknown keyword",
            Self::SqliteUnsupported => "Source file uses sql features sqlite does not support",
            Self::UnknownTable => "A referenced table does not exist in any CREATE TABLE",
            Self::UnknownColumn => "A referenced column does not exist in the target table",
            Self::InsertValueCountMismatch => {
                "INSERT provides a different number of values than the table has columns"
            }
            Self::TypeMismatch => {
                "A comparison or operation between incompatible SQLite storage classes"
            }
            Self::EqualNull => "Using = NULL or != NULL instead of IS NULL / IS NOT NULL",
            Self::LikeNoWildcards => "LIKE pattern has no wildcards (% or _), use = instead",
            Self::ReversedBetween => "BETWEEN bounds appear reversed (low > high)",
            Self::MissingWhere => "UPDATE or DELETE without a WHERE clause affects all rows",
            Self::TrailingComma => "Trailing comma in a list",
        }
    }
}
