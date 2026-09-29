//! The `rules.json` schema: rules as data for the fixed interpreter.
//!
//! A bundle carries a `rules.json` file whose rule programs are expression
//! DAGs over a closed instruction set, with a legal citation on every rule.
//! The interpreter evaluates them without compiling anything: the schema and
//! the instruction set are the only fixed surface, so a new rule, sector,
//! parameter or CN code is new data for the same interpreter image
//! (whitepaper §4.3, §5.4; interpreter contract §3).

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;

use crate::error::RuleError;

/// Schema tag the interpreter implements.
pub const RULES_SCHEMA: &str = "kaimeter-rules-v1";

/// A parsed `rules.json`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RulesSchema {
    /// Schema tag; must equal [`RULES_SCHEMA`].
    pub schema: String,
    /// Record types referenced by list inputs.
    #[serde(default)]
    pub types: BTreeMap<String, RecordType>,
    /// Rule identifier to rule program.
    pub rules: BTreeMap<String, RuleDef>,
}

/// A named record type: field name to value type.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecordType {
    /// The record's fields.
    pub fields: BTreeMap<String, ValueType>,
}

/// Scalar, text or boolean value type.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    /// A fixed-point quantity.
    Scalar,
    /// A UTF-8 string.
    Text,
    /// A boolean.
    Bool,
}

impl ValueType {
    /// Returns the type name used in error messages.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Text => "text",
            Self::Bool => "bool",
        }
    }
}

/// Declaration of one named rule input.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputDecl {
    /// A fixed-point quantity.
    Scalar,
    /// A UTF-8 string.
    Text,
    /// A boolean.
    Bool,
    /// A list of records of the named type.
    List {
        /// Record type name declared in `types`.
        of: String,
    },
}

/// One rule program.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuleDef {
    /// Provision the rule implements.
    pub legal: String,
    /// Source URL of the provision.
    pub source: String,
    /// Bundle that introduced the rule, for example `2026.3.0`.
    pub since: String,
    /// Named and typed inputs.
    #[serde(default)]
    pub inputs: BTreeMap<String, InputDecl>,
    /// Named outputs; all are computed, `proves` selects the proven one.
    pub outputs: BTreeMap<String, Expr>,
    /// Output that a proof of this rule commits.
    #[serde(default)]
    pub proves: Option<String>,
}

impl RuleDef {
    /// Returns the output a proof of this rule commits.
    ///
    /// Only valid after [`RulesSchema::validate`]: a rule without an
    /// explicit `proves` has exactly one output.
    #[must_use]
    pub fn proves_name(&self) -> Option<&str> {
        match &self.proves {
            Some(name) => Some(name),
            None if self.outputs.len() == 1 => self.outputs.keys().next().map(String::as_str),
            None => None,
        }
    }
}

/// One expression node of a rule program.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Expr {
    /// A constant scalar, text or boolean.
    Const {
        /// Decimal scalar literal.
        #[serde(default)]
        scalar: Option<String>,
        /// Text literal.
        #[serde(default)]
        text: Option<String>,
        /// Boolean literal.
        #[serde(default, rename = "bool")]
        boolean: Option<bool>,
    },
    /// One named rule input.
    Input {
        /// Input name.
        name: String,
    },
    /// One field of the list element in scope.
    Field {
        /// Field name.
        name: String,
    },
    /// One cell of a named parameter table row.
    Table {
        /// Logical table name; the file is `parameters/<table>.json`.
        table: String,
        /// Column to return.
        column: String,
        /// Cell type; scalars are parsed from decimal strings.
        #[serde(default, rename = "as")]
        cell: CellType,
        /// Row predicates, combined with AND.
        #[serde(rename = "match")]
        matches: Vec<Predicate>,
    },
    /// Sum.
    Add {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Difference.
    Sub {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Product.
    Mul {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Quotient.
    Div {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Negation.
    Neg {
        /// Operand.
        args: Vec<Expr>,
    },
    /// Absolute value.
    Abs {
        /// Operand.
        args: Vec<Expr>,
    },
    /// Smaller of two scalars.
    Min {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Larger of two scalars.
    Max {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Nearest whole number, half to even.
    Round {
        /// Operand.
        args: Vec<Expr>,
    },
    /// Conditional value.
    Select {
        /// Condition; must be boolean.
        cond: Box<Expr>,
        /// Value when the condition holds.
        #[serde(rename = "then")]
        then_branch: Box<Expr>,
        /// Value otherwise.
        #[serde(rename = "else")]
        else_branch: Box<Expr>,
    },
    /// Equality.
    Eq {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Inequality.
    Ne {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Less than.
    Lt {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Less than or equal.
    Le {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Greater than.
    Gt {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Greater than or equal.
    Ge {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Logical conjunction.
    And {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Logical disjunction.
    Or {
        /// Operands.
        args: Vec<Expr>,
    },
    /// Logical negation.
    Not {
        /// Operand.
        args: Vec<Expr>,
    },
    /// Sum over a list of records.
    Sum {
        /// The list expression.
        list: Box<Expr>,
        /// Summand evaluated per element.
        of: Box<Expr>,
    },
    /// Count over a list.
    Count {
        /// The list expression.
        list: Box<Expr>,
        /// Keep only elements for which this holds.
        #[serde(default, rename = "where")]
        condition: Option<Box<Expr>>,
    },
}

/// Cell type of a parameter table column.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CellType {
    /// Decimal string parsed into a scalar.
    #[default]
    Scalar,
    /// UTF-8 string returned as text.
    Text,
}

/// One row predicate of a table access.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Predicate {
    /// Column the predicate inspects.
    pub column: String,
    /// Matches rows whose cell equals this text expression.
    #[serde(default)]
    pub equals: Option<Expr>,
    /// Matches rows whose cell is a prefix of this text expression; the
    /// longest match wins.
    #[serde(default, rename = "prefixOf")]
    pub prefix_of: Option<Expr>,
}

impl RulesSchema {
    /// Parses `rules.json` content.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Schema`] with the position of the first parse
    /// error when the text is not valid schema data.
    pub fn from_json(text: &str) -> Result<Self, RuleError> {
        serde_json::from_str(text).map_err(|error| RuleError::Schema {
            line: error.line(),
            column: error.column(),
        })
    }

    /// Validates the schema tag, rule shape and every expression.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::UnsupportedSchema`] for a foreign schema tag,
    /// [`RuleError::NoRules`] for an empty bundle, [`RuleError::NoOutputs`]
    /// and [`RuleError::MissingProves`] for malformed rules,
    /// [`RuleError::UnknownOutput`] for a `proves` name that is not an
    /// output, [`RuleError::UnknownType`] for a list input of an undeclared
    /// record type, and the expression checks of [`Expr::validate`].
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.schema != RULES_SCHEMA {
            return Err(RuleError::UnsupportedSchema {
                found: self.schema.clone(),
            });
        }
        if self.rules.is_empty() {
            return Err(RuleError::NoRules);
        }
        for (name, rule) in &self.rules {
            rule.validate(name, &self.types)?;
        }
        Ok(())
    }
}

impl RuleDef {
    /// Validates one rule against the declared record types.
    fn validate(&self, name: &str, types: &BTreeMap<String, RecordType>) -> Result<(), RuleError> {
        self.validate_proves(name)?;
        self.validate_inputs(types)?;
        for expr in self.outputs.values() {
            expr.validate()?;
        }
        Ok(())
    }

    /// Checks the outputs and the `proves` selection.
    fn validate_proves(&self, name: &str) -> Result<(), RuleError> {
        if self.outputs.is_empty() {
            return Err(RuleError::NoOutputs {
                rule: name.to_string(),
            });
        }
        match &self.proves {
            Some(proves) if !self.outputs.contains_key(proves) => Err(RuleError::UnknownOutput {
                name: proves.clone(),
            }),
            None if self.outputs.len() > 1 => Err(RuleError::MissingProves {
                rule: name.to_string(),
            }),
            _ => Ok(()),
        }
    }

    /// Checks every list input against the declared record types.
    fn validate_inputs(&self, types: &BTreeMap<String, RecordType>) -> Result<(), RuleError> {
        self.inputs
            .values()
            .try_for_each(|decl| check_list_decl(decl, types))
    }
}

/// Checks one input declaration against the declared record types.
fn check_list_decl(
    decl: &InputDecl,
    types: &BTreeMap<String, RecordType>,
) -> Result<(), RuleError> {
    let InputDecl::List { of } = decl else {
        return Ok(());
    };
    if types.contains_key(of) {
        Ok(())
    } else {
        Err(RuleError::UnknownType { name: of.clone() })
    }
}

impl Expr {
    /// Validates the node's shape and, recursively, its children.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidConstant`] for a constant that does not
    /// carry exactly one literal, [`RuleError::InvalidPredicate`] for a
    /// table predicate without exactly one matcher, and
    /// [`RuleError::InvalidOperands`] for a wrong operand count.
    pub fn validate(&self) -> Result<(), RuleError> {
        match self {
            Self::Const { .. } => self.validate_const(),
            Self::Table { .. } => self.validate_table(),
            Self::Add { args }
            | Self::Sub { args }
            | Self::Mul { args }
            | Self::Div { args }
            | Self::Min { args }
            | Self::Max { args }
            | Self::Eq { args }
            | Self::Ne { args }
            | Self::Lt { args }
            | Self::Le { args }
            | Self::Gt { args }
            | Self::Ge { args } => arity(args, 2, "two operands"),
            Self::Neg { args } | Self::Abs { args } | Self::Round { args } | Self::Not { args } => {
                arity(args, 1, "one operand")
            }
            Self::And { args } => validate_logical(args, "and"),
            Self::Or { args } => validate_logical(args, "or"),
            Self::Input { .. } | Self::Field { .. } => Ok(()),
            Self::Select {
                cond,
                then_branch,
                else_branch,
            } => {
                cond.validate()?;
                then_branch.validate()?;
                else_branch.validate()
            }
            Self::Sum { list, of } => {
                list.validate()?;
                of.validate()
            }
            Self::Count { list, condition } => {
                list.validate()?;
                condition.as_deref().map_or(Ok(()), Expr::validate)
            }
        }
    }

    /// Checks a constant node's literal and scalar syntax.
    fn validate_const(&self) -> Result<(), RuleError> {
        let Self::Const {
            scalar,
            text,
            boolean,
        } = self
        else {
            return Ok(());
        };
        let literals = usize::from(scalar.is_some())
            + usize::from(text.is_some())
            + usize::from(boolean.is_some());
        if literals != 1 {
            return Err(RuleError::InvalidConstant);
        }
        if let Some(value) = scalar {
            check_scalar(value)?;
        }
        Ok(())
    }

    /// Checks a table node's predicates.
    fn validate_table(&self) -> Result<(), RuleError> {
        let Self::Table { table, matches, .. } = self else {
            return Ok(());
        };
        if matches.is_empty() {
            return Err(RuleError::InvalidPredicate {
                table: table.clone(),
            });
        }
        matches
            .iter()
            .try_for_each(|predicate| check_predicate(predicate, table))
    }
}

/// Checks a logical node's operand count and operand shapes.
fn validate_logical(args: &[Expr], op: &'static str) -> Result<(), RuleError> {
    if args.is_empty() {
        return Err(RuleError::InvalidOperands {
            op,
            expected: "at least one operand",
        });
    }
    args.iter().try_for_each(Expr::validate)
}

/// Checks that a constant scalar literal parses at the bundle scale.
fn check_scalar(value: &str) -> Result<(), RuleError> {
    value
        .parse::<crate::fixed::Fixed>()
        .map_err(|_error| RuleError::InvalidConstantScalar {
            value: value.to_string(),
        })
        .map(|_parsed| ())
}

/// Checks one table predicate: exactly one matcher, over a valid operand.
fn check_predicate(predicate: &Predicate, table: &str) -> Result<(), RuleError> {
    match (&predicate.equals, &predicate.prefix_of) {
        (Some(operand), None) | (None, Some(operand)) => operand.validate(),
        _ => Err(RuleError::InvalidPredicate {
            table: table.to_string(),
        }),
    }
}

/// Checks an operand count.
fn arity(args: &[Expr], expected: usize, description: &'static str) -> Result<(), RuleError> {
    if args.len() != expected {
        return Err(RuleError::InvalidOperands {
            op: "operation",
            expected: description,
        });
    }
    for arg in args {
        arg.validate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A schema exercising every expression node.
    const FULL: &str = r#"{
  "schema": "kaimeter-rules-v1",
  "types": {
    "supply": {
      "fields": {
        "see": "scalar",
        "origin": "text",
        "zero_rated": "bool"
      }
    }
  },
  "rules": {
    "test.rule": {
      "legal": "IR (EU) 2025/2547, Annex II",
      "source": "http://data.europa.eu/eli/reg_impl/2025/2547/oj",
      "since": "2026.3.0",
      "inputs": {
        "production_t": {"type": "scalar"},
        "cn_code": {"type": "text"},
        "active": {"type": "bool"},
        "supplies": {"type": "list", "of": "supply"}
      },
      "outputs": {
        "out": {"op": "add", "args": [
          {"op": "const", "scalar": "1"},
          {"op": "input", "name": "production_t"}
        ]}
      },
      "proves": "out"
    }
  }
}"#;

    fn parsed(text: &str) -> RulesSchema {
        RulesSchema::from_json(text).unwrap()
    }

    /// Builds a rule whose only output is one table access.
    fn table_rule(matches: &str) -> RulesSchema {
        let template = r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"a":{"op":"table","table":"gwp","column":"gwp100","match":[MATCHES]}}}}}"#;
        RulesSchema::from_json(&template.replace("MATCHES", matches)).unwrap()
    }

    #[test]
    fn parses_and_validates_the_full_schema() {
        let schema = parsed(FULL);
        assert_eq!(schema.schema, RULES_SCHEMA);
        assert_eq!(schema.types["supply"].fields["see"], ValueType::Scalar);
        assert_eq!(schema.types["supply"].fields["origin"], ValueType::Text);
        assert_eq!(schema.types["supply"].fields["zero_rated"], ValueType::Bool);
        let rule = &schema.rules["test.rule"];
        assert_eq!(rule.legal, "IR (EU) 2025/2547, Annex II");
        assert!(rule.source.starts_with("http"));
        assert_eq!(rule.since, "2026.3.0");
        assert_eq!(
            rule.inputs["supplies"],
            InputDecl::List {
                of: String::from("supply")
            }
        );
        assert_eq!(rule.proves.as_deref(), Some("out"));
        assert_eq!(rule.proves_name(), Some("out"));
        schema.validate().unwrap();
    }

    #[test]
    fn rejects_foreign_schemas() {
        let schema = parsed(
            r#"{"schema":"other","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"o":{"op":"const","scalar":"1"}}}}}"#,
        );
        assert_eq!(
            schema.validate(),
            Err(RuleError::UnsupportedSchema {
                found: String::from("other"),
            })
        );
    }

    #[test]
    fn rejects_empty_rule_sets() {
        let schema = parsed(r#"{"schema":"kaimeter-rules-v1","rules":{}}"#);
        assert_eq!(schema.validate(), Err(RuleError::NoRules));
    }

    #[test]
    fn rejects_rules_without_outputs() {
        let schema = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{}}}}"#,
        );
        assert_eq!(
            schema.validate(),
            Err(RuleError::NoOutputs {
                rule: String::from("r"),
            })
        );
    }

    #[test]
    fn resolves_a_single_output_as_the_proven_one() {
        let schema = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"only":{"op":"const","scalar":"1"}}}}}"#,
        );
        schema.validate().unwrap();
        assert_eq!(schema.rules["r"].proves_name(), Some("only"));
    }

    #[test]
    fn demands_proves_when_several_outputs_exist() {
        let schema = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"a":{"op":"const","scalar":"1"},"b":{"op":"const","scalar":"2"}}}}}"#,
        );
        assert_eq!(
            schema.validate(),
            Err(RuleError::MissingProves {
                rule: String::from("r"),
            })
        );
        assert_eq!(schema.rules["r"].proves_name(), None);
    }

    #[test]
    fn rejects_unknown_proves_and_types() {
        let schema = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"a":{"op":"const","scalar":"1"}},"proves":"b"}}}"#,
        );
        assert_eq!(
            schema.validate(),
            Err(RuleError::UnknownOutput {
                name: String::from("b"),
            })
        );

        let schema = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","inputs":{"xs":{"type":"list","of":"missing"}},"outputs":{"a":{"op":"const","scalar":"1"}}}}}"#,
        );
        assert_eq!(
            schema.validate(),
            Err(RuleError::UnknownType {
                name: String::from("missing"),
            })
        );
    }

    #[test]
    fn validates_constants_and_predicates() {
        let constant = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"a":{"op":"const"}}}}}"#,
        );
        assert_eq!(constant.validate(), Err(RuleError::InvalidConstant));

        let two_literals = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"a":{"op":"const","scalar":"1","text":"x"}}}}}"#,
        );
        assert_eq!(two_literals.validate(), Err(RuleError::InvalidConstant));

        let malformed = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"a":{"op":"const","scalar":"many"}}}}}"#,
        );
        assert_eq!(
            malformed.validate(),
            Err(RuleError::InvalidConstantScalar {
                value: String::from("many"),
            })
        );

        assert_eq!(
            table_rule("").validate(),
            Err(RuleError::InvalidPredicate {
                table: String::from("gwp"),
            })
        );
        assert_eq!(
            table_rule(r#"{"column":"substance","equals":{"op":"const","text":"CF4"},"prefixOf":{"op":"input","name":"x"}}"#).validate(),
            Err(RuleError::InvalidPredicate {
                table: String::from("gwp"),
            })
        );
        assert_eq!(
            table_rule(r#"{"column":"substance"}"#).validate(),
            Err(RuleError::InvalidPredicate {
                table: String::from("gwp"),
            })
        );
        table_rule(r#"{"column":"substance","equals":{"op":"const","text":"CF4"}}"#)
            .validate()
            .unwrap();
        table_rule(r#"{"column":"cn_code","prefixOf":{"op":"input","name":"cn"}}"#)
            .validate()
            .unwrap();
    }

    #[test]
    fn validates_operand_counts() {
        fn rule(expr: &str) -> RulesSchema {
            let template = r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"a":EXPR}}}}"#;
            RulesSchema::from_json(&template.replace("EXPR", expr)).unwrap()
        }
        assert!(matches!(
            rule(r#"{"op":"add","args":[{"op":"const","scalar":"1"}]}"#).validate(),
            Err(RuleError::InvalidOperands { .. })
        ));
        assert!(matches!(
            rule(r#"{"op":"neg","args":[]}"#).validate(),
            Err(RuleError::InvalidOperands { .. })
        ));
        assert!(matches!(
            rule(r#"{"op":"and","args":[]}"#).validate(),
            Err(RuleError::InvalidOperands { .. })
        ));
        assert!(matches!(
            rule(r#"{"op":"or","args":[]}"#).validate(),
            Err(RuleError::InvalidOperands { .. })
        ));
        assert!(matches!(
            rule(r#"{"op":"not","args":[]}"#).validate(),
            Err(RuleError::InvalidOperands { .. })
        ));
        rule(r#"{"op":"and","args":[{"op":"const","bool":true}]}"#)
            .validate()
            .unwrap();
        rule(r#"{"op":"min","args":[{"op":"const","scalar":"1"},{"op":"const","scalar":"2"}]}"#)
            .validate()
            .unwrap();
        rule(r#"{"op":"max","args":[{"op":"const","scalar":"1"},{"op":"const","scalar":"2"}]}"#)
            .validate()
            .unwrap();
    }

    #[test]
    fn validates_nested_and_aggregate_nodes() {
        let schema = parsed(
            r#"{"schema":"kaimeter-rules-v1","rules":{"r":{"legal":"l","source":"s","since":"1","outputs":{"a":{"op":"select","cond":{"op":"eq","args":[{"op":"const","scalar":"1"},{"op":"const","scalar":"1"}]},"then":{"op":"sum","list":{"op":"input","name":"xs"},"of":{"op":"field","name":"y"}},"else":{"op":"count","list":{"op":"input","name":"xs"},"where":{"op":"not","args":[{"op":"const","bool":true}]}}}}}}}"#,
        );
        schema.validate().unwrap();
    }

    #[test]
    fn reports_schema_parse_positions() {
        match RulesSchema::from_json("{") {
            Err(RuleError::Schema { line, column }) => {
                assert!(line >= 1);
                assert!(column >= 1);
            }
            other => panic!("expected a schema error, got {other:?}"),
        }
    }

    #[test]
    fn names_value_types() {
        assert_eq!(ValueType::Scalar.name(), "scalar");
        assert_eq!(ValueType::Text.name(), "text");
        assert_eq!(ValueType::Bool.name(), "bool");
    }
}
