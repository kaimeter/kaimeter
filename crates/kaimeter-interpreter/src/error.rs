//! Errors produced while applying a rule bundle.

use alloc::string::String;
use core::fmt;

/// Errors produced while applying a rule bundle or evaluating its rules.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RuleError {
    /// A fixed-point operation left the 128-bit scaled range.
    Overflow,
    /// A fixed-point division had a zero divisor.
    DivisionByZero,
    /// A decimal string was not a plain decimal number.
    InvalidDecimal,
    /// A date was not a real calendar date or not `YYYY-MM-DD`.
    InvalidDate,
    /// `rules.json` was not valid schema data.
    Schema {
        /// Line of the first parse error.
        line: usize,
        /// Column of the first parse error.
        column: usize,
    },
    /// The bundle carries no `rules.json`.
    MissingRulesFile,
    /// `rules.json` was not valid UTF-8.
    RulesNotUtf8,
    /// `rules.json` declares a schema the interpreter does not implement.
    UnsupportedSchema {
        /// Schema tag found in `rules.json`.
        found: String,
    },
    /// `rules.json` declares no rules.
    NoRules,
    /// A rule declares no outputs.
    NoOutputs {
        /// Rule identifier.
        rule: String,
    },
    /// A rule with several outputs does not say which one it proves.
    MissingProves {
        /// Rule identifier.
        rule: String,
    },
    /// The invocation names a rule the bundle does not define.
    UnknownRule {
        /// Requested rule identifier.
        rule: String,
    },
    /// A declared rule input was not provided.
    MissingInput {
        /// Declared input name.
        name: String,
    },
    /// The invocation provides an input the rule does not declare.
    UnknownInput {
        /// Provided input name.
        name: String,
    },
    /// A record carries no value for a declared field.
    MissingField {
        /// Declared field name.
        name: String,
    },
    /// The `proves` field does not name an output of the rule.
    UnknownOutput {
        /// Requested output name.
        name: String,
    },
    /// A value had a different type than the operation requires.
    TypeMismatch {
        /// Type the operation requires.
        expected: &'static str,
        /// Type the value actually has.
        found: &'static str,
    },
    /// A record access names a field the record does not carry.
    UnknownField {
        /// Requested field name.
        name: String,
    },
    /// A record access was made outside a list aggregate element.
    NoElement,
    /// A rule references a parameter table the bundle does not carry.
    UnknownTable {
        /// Logical table name, for example `gwp`.
        table: String,
    },
    /// A parameter table has no `rows` array.
    MissingRows {
        /// Logical table name.
        table: String,
    },
    /// A parameter table file was not valid JSON.
    TableJson {
        /// Logical table name.
        table: String,
        /// Line of the first parse error.
        line: usize,
        /// Column of the first parse error.
        column: usize,
    },
    /// No row of a parameter table matched the predicates.
    MissingRow {
        /// Logical table name.
        table: String,
    },
    /// The matched row carries no value in the requested column.
    MissingColumn {
        /// Logical table name.
        table: String,
        /// Requested column.
        column: String,
    },
    /// A parameter table cell was not a UTF-8 string.
    NonStringCell {
        /// Logical table name.
        table: String,
        /// Offending column.
        column: String,
    },
    /// A parameter table cell could not be parsed as a scalar.
    InvalidTableScalar {
        /// Logical table name.
        table: String,
        /// Offending column.
        column: String,
        /// Offending value.
        value: String,
    },
    /// An operand of an arithmetic or logical operation was malformed.
    InvalidOperands {
        /// Operation, for example `add`.
        op: &'static str,
        /// Shape the operation requires.
        expected: &'static str,
    },
    /// A constant expression did not carry exactly one literal.
    InvalidConstant,
    /// A table predicate did not carry exactly one matcher.
    InvalidPredicate {
        /// Logical table name.
        table: String,
    },
    /// An input declaration references an undeclared record type.
    UnknownType {
        /// Record type name.
        name: String,
    },
    /// A `const` scalar literal is not a plain decimal string.
    InvalidConstantScalar {
        /// Offending literal.
        value: String,
    },
    /// The invocation record was not valid JSON or not the required shape.
    Invocation {
        /// What the parser expected.
        expected: &'static str,
    },
    /// An invocation record carried a JSON number where a decimal string is
    /// required; binary floating point must never enter the bundle.
    InvalidJson {
        /// Line of the first parse error.
        line: usize,
        /// Column of the first parse error.
        column: usize,
    },
}

impl fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow
            | Self::DivisionByZero
            | Self::InvalidDecimal
            | Self::InvalidDate
            | Self::NoElement
            | Self::InvalidConstant => self.fmt_plain(formatter),
            Self::Schema { .. } | Self::TableJson { .. } | Self::InvalidJson { .. } => {
                self.fmt_position(formatter)
            }
            Self::MissingRulesFile
            | Self::RulesNotUtf8
            | Self::UnsupportedSchema { .. }
            | Self::NoRules
            | Self::NoOutputs { .. }
            | Self::MissingProves { .. }
            | Self::UnknownRule { .. } => self.fmt_rules(formatter),
            Self::MissingInput { .. }
            | Self::UnknownInput { .. }
            | Self::MissingField { .. }
            | Self::UnknownOutput { .. }
            | Self::TypeMismatch { .. }
            | Self::UnknownField { .. }
            | Self::UnknownType { .. }
            | Self::InvalidConstantScalar { .. }
            | Self::Invocation { .. }
            | Self::InvalidOperands { .. } => self.fmt_values(formatter),
            Self::UnknownTable { .. }
            | Self::MissingRows { .. }
            | Self::MissingRow { .. }
            | Self::MissingColumn { .. }
            | Self::NonStringCell { .. }
            | Self::InvalidTableScalar { .. }
            | Self::InvalidPredicate { .. } => self.fmt_tables(formatter),
        }
    }
}

impl RuleError {
    /// Writes the message of a variant that carries no fields.
    fn fmt_plain(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => formatter.write_str("fixed-point overflow"),
            Self::DivisionByZero => formatter.write_str("division by zero"),
            Self::InvalidDecimal => formatter.write_str("invalid decimal string"),
            Self::InvalidDate => formatter.write_str("invalid calendar date"),
            Self::NoElement => formatter.write_str("no list element in scope"),
            Self::InvalidConstant => formatter.write_str("constant carries no literal"),
            _ => Ok(()),
        }
    }

    /// Writes a parse error with its line and column.
    fn fmt_position(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema { line, column } => {
                write!(
                    formatter,
                    "invalid rule schema at line {line}, column {column}"
                )
            }
            Self::TableJson {
                table,
                line,
                column,
            } => write!(
                formatter,
                "invalid parameter table `{table}` at line {line}, column {column}"
            ),
            Self::InvalidJson { line, column } => write!(
                formatter,
                "invalid invocation JSON at line {line}, column {column}"
            ),
            _ => Ok(()),
        }
    }

    /// Writes an error about the shape of the rule set.
    fn fmt_rules(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRulesFile => formatter.write_str("bundle carries no rules.json"),
            Self::RulesNotUtf8 => formatter.write_str("rules.json is not UTF-8"),
            Self::UnsupportedSchema { found } => {
                write!(formatter, "unsupported rule schema `{found}`")
            }
            Self::NoRules => formatter.write_str("rule schema defines no rules"),
            Self::NoOutputs { rule } => write!(formatter, "rule `{rule}` declares no outputs"),
            Self::MissingProves { rule } => {
                write!(
                    formatter,
                    "rule `{rule}` does not name the output it proves"
                )
            }
            Self::UnknownRule { rule } => write!(formatter, "unknown rule `{rule}`"),
            _ => Ok(()),
        }
    }

    /// Writes an error about a named input, field, output or type.
    fn fmt_values(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInput { name } => write!(formatter, "missing input `{name}`"),
            Self::UnknownInput { name } => write!(formatter, "unknown input `{name}`"),
            Self::MissingField { name } => write!(formatter, "missing record field `{name}`"),
            Self::UnknownOutput { name } => write!(formatter, "unknown output `{name}`"),
            Self::TypeMismatch { expected, found } => {
                write!(formatter, "expected {expected}, found {found}")
            }
            Self::UnknownField { name } => write!(formatter, "unknown record field `{name}`"),
            Self::UnknownType { name } => write!(formatter, "unknown record type `{name}`"),
            Self::InvalidConstantScalar { value } => {
                write!(formatter, "invalid constant scalar `{value}`")
            }
            Self::Invocation { expected } => {
                write!(formatter, "invocation record does not contain {expected}")
            }
            Self::InvalidOperands { op, expected } => {
                write!(formatter, "`{op}` requires {expected}")
            }
            _ => Ok(()),
        }
    }

    /// Writes an error about a parameter table.
    fn fmt_tables(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTable { table } => write!(formatter, "unknown parameter table `{table}`"),
            Self::MissingRows { table } => {
                write!(formatter, "parameter table `{table}` has no rows")
            }
            Self::MissingRow { table } => {
                write!(formatter, "no row matched in parameter table `{table}`")
            }
            Self::MissingColumn { table, column } => write!(
                formatter,
                "parameter table `{table}` has no column `{column}`"
            ),
            Self::NonStringCell { table, column } => write!(
                formatter,
                "parameter table `{table}` column `{column}` is not a string"
            ),
            Self::InvalidTableScalar {
                table,
                column,
                value,
            } => write!(
                formatter,
                "parameter table `{table}` column `{column}` value `{value}` is not a scalar"
            ),
            Self::InvalidPredicate { table } => {
                write!(
                    formatter,
                    "predicate on `{table}` is not exactly one matcher"
                )
            }
            _ => Ok(()),
        }
    }
}

impl core::error::Error for RuleError {}

#[cfg(test)]
mod tests {
    use super::RuleError;
    use alloc::string::String;

    #[test]
    fn displays_plain_variants() {
        assert_eq!(RuleError::Overflow.to_string(), "fixed-point overflow");
        assert_eq!(RuleError::DivisionByZero.to_string(), "division by zero");
        assert_eq!(
            RuleError::InvalidDecimal.to_string(),
            "invalid decimal string"
        );
        assert_eq!(RuleError::InvalidDate.to_string(), "invalid calendar date");
        assert_eq!(RuleError::NoElement.to_string(), "no list element in scope");
        assert_eq!(
            RuleError::InvalidConstant.to_string(),
            "constant carries no literal"
        );
    }

    #[test]
    fn displays_position_variants() {
        assert_eq!(
            RuleError::Schema { line: 1, column: 2 }.to_string(),
            "invalid rule schema at line 1, column 2"
        );
        assert_eq!(
            RuleError::TableJson {
                table: String::from("gwp"),
                line: 1,
                column: 2,
            }
            .to_string(),
            "invalid parameter table `gwp` at line 1, column 2"
        );
        assert_eq!(
            RuleError::InvalidJson { line: 3, column: 4 }.to_string(),
            "invalid invocation JSON at line 3, column 4"
        );
    }

    #[test]
    fn displays_rule_set_variants() {
        assert_eq!(
            RuleError::MissingRulesFile.to_string(),
            "bundle carries no rules.json"
        );
        assert_eq!(
            RuleError::RulesNotUtf8.to_string(),
            "rules.json is not UTF-8"
        );
        assert_eq!(
            RuleError::UnsupportedSchema {
                found: String::from("x")
            }
            .to_string(),
            "unsupported rule schema `x`"
        );
        assert_eq!(
            RuleError::NoRules.to_string(),
            "rule schema defines no rules"
        );
        assert_eq!(
            RuleError::NoOutputs {
                rule: String::from("a.b")
            }
            .to_string(),
            "rule `a.b` declares no outputs"
        );
        assert_eq!(
            RuleError::MissingProves {
                rule: String::from("a.b")
            }
            .to_string(),
            "rule `a.b` does not name the output it proves"
        );
        assert_eq!(
            RuleError::UnknownRule {
                rule: String::from("a.b")
            }
            .to_string(),
            "unknown rule `a.b`"
        );
    }

    #[test]
    fn displays_value_variants() {
        assert_eq!(
            RuleError::MissingInput {
                name: String::from("x")
            }
            .to_string(),
            "missing input `x`"
        );
        assert_eq!(
            RuleError::UnknownInput {
                name: String::from("x")
            }
            .to_string(),
            "unknown input `x`"
        );
        assert_eq!(
            RuleError::MissingField {
                name: String::from("x")
            }
            .to_string(),
            "missing record field `x`"
        );
        assert_eq!(
            RuleError::UnknownOutput {
                name: String::from("x")
            }
            .to_string(),
            "unknown output `x`"
        );
        assert_eq!(
            RuleError::UnknownField {
                name: String::from("x")
            }
            .to_string(),
            "unknown record field `x`"
        );
        assert_eq!(
            RuleError::UnknownType {
                name: String::from("x")
            }
            .to_string(),
            "unknown record type `x`"
        );
        assert_eq!(
            RuleError::InvalidConstantScalar {
                value: String::from("x")
            }
            .to_string(),
            "invalid constant scalar `x`"
        );
    }

    #[test]
    fn displays_shape_variants() {
        assert_eq!(
            RuleError::TypeMismatch {
                expected: "scalar",
                found: "text",
            }
            .to_string(),
            "expected scalar, found text"
        );
        assert_eq!(
            RuleError::Invocation {
                expected: "a rule name"
            }
            .to_string(),
            "invocation record does not contain a rule name"
        );
        assert_eq!(
            RuleError::InvalidOperands {
                op: "add",
                expected: "two scalar arguments",
            }
            .to_string(),
            "`add` requires two scalar arguments"
        );
    }

    #[test]
    fn displays_table_variants() {
        assert_eq!(
            RuleError::UnknownTable {
                table: String::from("gwp")
            }
            .to_string(),
            "unknown parameter table `gwp`"
        );
        assert_eq!(
            RuleError::MissingRows {
                table: String::from("gwp")
            }
            .to_string(),
            "parameter table `gwp` has no rows"
        );
        assert_eq!(
            RuleError::MissingRow {
                table: String::from("gwp")
            }
            .to_string(),
            "no row matched in parameter table `gwp`"
        );
        assert_eq!(
            RuleError::MissingColumn {
                table: String::from("gwp"),
                column: String::from("x"),
            }
            .to_string(),
            "parameter table `gwp` has no column `x`"
        );
        assert_eq!(
            RuleError::NonStringCell {
                table: String::from("gwp"),
                column: String::from("x"),
            }
            .to_string(),
            "parameter table `gwp` column `x` is not a string"
        );
        assert_eq!(
            RuleError::InvalidTableScalar {
                table: String::from("gwp"),
                column: String::from("x"),
                value: String::from("many"),
            }
            .to_string(),
            "parameter table `gwp` column `x` value `many` is not a scalar"
        );
        assert_eq!(
            RuleError::InvalidPredicate {
                table: String::from("gwp")
            }
            .to_string(),
            "predicate on `gwp` is not exactly one matcher"
        );
    }
}
