//! The rule bundle as data: parse, validate and evaluate rule programs.
//!
//! A bundle supplies `rules.json` and its parameter tables as files; the
//! interpreter reads them from the witness set, so the rules travel as data
//! and only the instruction set is fixed by the guest image (whitepaper
//! §5.4). Evaluation is pure: no clock, no I/O, no randomness, no floating
//! point.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cmp::Ordering;

use serde_json::Value as Json;

use crate::bundle::{BundleError, BundleFile, BundleHash, bundle_hash};
use crate::date::Date;
use crate::error::RuleError;
use crate::fixed::Fixed;
use crate::journal::{Context, Journal};
use crate::schema::{
    CellType, Expr, InputDecl, Predicate, RecordType, RuleDef, RulesSchema, ValueType,
};
use crate::value::Value;

/// A parsed rule bundle: the file set, its schema and its parameter tables.
#[derive(Clone, Debug)]
pub struct RuleBundle<'a> {
    files: Vec<BundleFile<'a>>,
    schema: RulesSchema,
    tables: BTreeMap<String, Vec<u8>>,
}

/// A typed evaluation request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// Rule identifier, for example `aluminium.primary.slope`.
    pub rule: String,
    /// Public context recorded in the journal.
    pub context: Context,
    /// Typed inputs the rule declares.
    pub inputs: BTreeMap<String, Value>,
}

/// The result of one rule evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleOutcome {
    /// Every declared output, computed.
    pub outputs: BTreeMap<String, Value>,
    /// Name of the output the rule proves.
    pub proves: String,
    /// The proven output as a scalar.
    pub output: Fixed,
}

impl RuleOutcome {
    /// Builds the public journal for this outcome.
    #[must_use]
    pub fn journal(&self, bundle_hash: BundleHash, context: Context) -> Journal {
        Journal {
            bundle_hash,
            output: self.output,
            context,
        }
    }
}

impl<'a> RuleBundle<'a> {
    /// Reads a bundle from its canonical file set.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::MissingRulesFile`] when the set carries no
    /// `rules.json`, [`RuleError::RulesNotUtf8`] when it is not UTF-8, and
    /// the parse and validation errors of [`RulesSchema`].
    pub fn from_files(files: &[BundleFile<'a>]) -> Result<Self, RuleError> {
        let rules = files
            .iter()
            .find(|file| file.path == "rules.json")
            .ok_or(RuleError::MissingRulesFile)?;
        let text = core::str::from_utf8(rules.content).map_err(|_error| RuleError::RulesNotUtf8)?;
        let schema = RulesSchema::from_json(text)?;
        schema.validate()?;
        Ok(Self {
            files: files.to_vec(),
            schema,
            tables: collect_tables(files),
        })
    }

    /// Returns the parsed schema.
    #[must_use]
    pub fn schema(&self) -> &RulesSchema {
        &self.schema
    }

    /// Computes the canonical hash of the accepted file set.
    ///
    /// # Errors
    ///
    /// Returns the canonicalisation errors of [`crate::bundle::bundle_hash`].
    pub fn bundle_hash(&self) -> Result<BundleHash, BundleError> {
        bundle_hash(&self.files)
    }

    /// Parses an invocation record against this bundle's schema.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidJson`] for malformed JSON, the schema
    /// errors of [`Invocation::from_json`], and the coercion errors of
    /// [`Value::coerce`].
    pub fn parse_invocation(&self, text: &str) -> Result<Invocation, RuleError> {
        Invocation::from_json(&self.schema, text)
    }

    /// Evaluates one invocation.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::UnknownRule`] for a rule the schema does not
    /// define, [`RuleError::MissingInput`] and [`RuleError::UnknownInput`]
    /// for a mismatch with the declared inputs, and any evaluation error of
    /// the rule program.
    pub fn evaluate(&self, invocation: &Invocation) -> Result<RuleOutcome, RuleError> {
        let rule = self.rule(&invocation.rule)?;
        check_inputs(rule, invocation, &self.schema.types)?;
        evaluate_rule(rule, &self.tables, invocation)
    }

    /// Resolves a rule identifier against the schema.
    fn rule(&self, name: &str) -> Result<&RuleDef, RuleError> {
        self.schema
            .rules
            .get(name)
            .ok_or_else(|| RuleError::UnknownRule {
                rule: name.to_string(),
            })
    }
}

/// Evaluates one rule against one invocation.
fn evaluate_rule(
    rule: &RuleDef,
    tables: &BTreeMap<String, Vec<u8>>,
    invocation: &Invocation,
) -> Result<RuleOutcome, RuleError> {
    let mut evaluator = Evaluator {
        tables,
        inputs: &invocation.inputs,
        elements: Vec::new(),
    };
    let mut outputs = BTreeMap::new();
    for (name, expr) in &rule.outputs {
        outputs.insert(name.clone(), evaluator.eval(expr)?);
    }
    let proves = rule
        .proves_name()
        .ok_or_else(|| RuleError::MissingProves {
            rule: invocation.rule.clone(),
        })?
        .to_string();
    let output = outputs
        .get(&proves)
        .ok_or_else(|| RuleError::UnknownOutput {
            name: proves.clone(),
        })?
        .as_scalar()?;
    Ok(RuleOutcome {
        outputs,
        proves,
        output,
    })
}

/// Collects the named parameter tables of a bundle file set.
fn collect_tables(files: &[BundleFile<'_>]) -> BTreeMap<String, Vec<u8>> {
    files
        .iter()
        .filter_map(|file| {
            let name = parameter_table_name(file.path)?;
            Some((name.to_string(), file.content.to_vec()))
        })
        .collect()
}

/// Returns the logical table name of a `parameters/<name>.json` path.
fn parameter_table_name(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("parameters/")?;
    rest.strip_suffix(".json")
        .filter(|name| !name.contains('/'))
}

/// Checks an invocation against a rule's declared inputs.
fn check_inputs(
    rule: &RuleDef,
    invocation: &Invocation,
    types: &BTreeMap<String, RecordType>,
) -> Result<(), RuleError> {
    for name in invocation.inputs.keys() {
        if !rule.inputs.contains_key(name) {
            return Err(RuleError::UnknownInput { name: name.clone() });
        }
    }
    for (name, decl) in &rule.inputs {
        let value = invocation
            .inputs
            .get(name)
            .ok_or_else(|| RuleError::MissingInput { name: name.clone() })?;
        check_input(decl, value, types)?;
    }
    Ok(())
}

/// Checks one provided value against its declaration.
fn check_input(
    decl: &InputDecl,
    value: &Value,
    types: &BTreeMap<String, RecordType>,
) -> Result<(), RuleError> {
    match (decl, value) {
        (InputDecl::Scalar, Value::Scalar(_))
        | (InputDecl::Text, Value::Text(_))
        | (InputDecl::Bool, Value::Bool(_)) => Ok(()),
        (InputDecl::List { of }, Value::List(items)) => {
            let record = types
                .get(of)
                .ok_or_else(|| RuleError::UnknownType { name: of.clone() })?;
            for item in items {
                check_record(item, record)?;
            }
            Ok(())
        }
        (decl, other) => Err(RuleError::TypeMismatch {
            expected: declaration_name(decl),
            found: other.kind_name(),
        }),
    }
}

/// Checks one provided record against its declared type.
fn check_record(value: &Value, record: &RecordType) -> Result<(), RuleError> {
    let Value::Record(fields) = value else {
        return Err(RuleError::TypeMismatch {
            expected: "record",
            found: value.kind_name(),
        });
    };
    for name in fields.keys() {
        if !record.fields.contains_key(name) {
            return Err(RuleError::UnknownField { name: name.clone() });
        }
    }
    for (name, kind) in &record.fields {
        let value = fields
            .get(name)
            .ok_or_else(|| RuleError::MissingField { name: name.clone() })?;
        if !value_matches(value, *kind) {
            return Err(RuleError::TypeMismatch {
                expected: kind.name(),
                found: value.kind_name(),
            });
        }
    }
    Ok(())
}

/// Returns `true` when a value has the declared type.
fn value_matches(value: &Value, kind: ValueType) -> bool {
    match kind {
        ValueType::Scalar => matches!(value, Value::Scalar(_)),
        ValueType::Text => matches!(value, Value::Text(_)),
        ValueType::Bool => matches!(value, Value::Bool(_)),
    }
}

/// Names an input declaration for error messages.
fn declaration_name(decl: &InputDecl) -> &'static str {
    match decl {
        InputDecl::Scalar => "scalar",
        InputDecl::Text => "text",
        InputDecl::Bool => "bool",
        InputDecl::List { .. } => "list",
    }
}

impl Invocation {
    /// Parses an invocation record against a schema.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidJson`] for malformed JSON,
    /// [`RuleError::Invocation`] for a missing or malformed `rule`,
    /// `context` or `inputs` member, [`RuleError::UnknownRule`] for a rule
    /// the schema does not define, [`RuleError::MissingInput`] and
    /// [`RuleError::UnknownInput`] for a mismatch with the declared inputs,
    /// and the coercion errors of [`Value::coerce`].
    pub fn from_json(schema: &RulesSchema, text: &str) -> Result<Self, RuleError> {
        let json: Json = serde_json::from_str(text).map_err(|error| RuleError::InvalidJson {
            line: error.line(),
            column: error.column(),
        })?;
        let Json::Object(object) = json else {
            return Err(RuleError::Invocation {
                expected: "an object with rule, context and inputs",
            });
        };
        let Some(Json::String(rule)) = object.get("rule") else {
            return Err(RuleError::Invocation {
                expected: "a rule name",
            });
        };
        let definition = schema
            .rules
            .get(rule)
            .ok_or_else(|| RuleError::UnknownRule { rule: rule.clone() })?;
        let Some(context_json) = object.get("context") else {
            return Err(RuleError::Invocation {
                expected: "a context with sector, route, cnCode and period",
            });
        };
        let context: Context = serde_json::from_value(context_json.clone()).map_err(|_error| {
            RuleError::Invocation {
                expected: "a context with sector, route, cnCode and period",
            }
        })?;
        let Some(Json::Object(raw_inputs)) = object.get("inputs") else {
            return Err(RuleError::Invocation {
                expected: "an inputs object",
            });
        };
        if let Some(name) = raw_inputs
            .keys()
            .find(|name| !definition.inputs.contains_key(*name))
        {
            return Err(RuleError::UnknownInput { name: name.clone() });
        }
        let mut inputs = BTreeMap::new();
        for (name, decl) in &definition.inputs {
            let raw = raw_inputs
                .get(name)
                .ok_or_else(|| RuleError::MissingInput { name: name.clone() })?;
            inputs.insert(name.clone(), Value::coerce(raw, decl, &schema.types)?);
        }
        Ok(Self {
            rule: rule.clone(),
            context,
            inputs,
        })
    }
}

/// One evaluation of one rule program.
struct Evaluator<'a> {
    tables: &'a BTreeMap<String, Vec<u8>>,
    inputs: &'a BTreeMap<String, Value>,
    elements: Vec<BTreeMap<String, Value>>,
}

impl Evaluator<'_> {
    /// Evaluates one expression node.
    fn eval(&mut self, expr: &Expr) -> Result<Value, RuleError> {
        match expr {
            Expr::Const {
                scalar,
                text,
                boolean,
            } => constant(scalar.as_deref(), text.as_deref(), *boolean),
            Expr::Input { name } => self
                .inputs
                .get(name)
                .cloned()
                .ok_or_else(|| RuleError::UnknownInput { name: name.clone() }),
            Expr::Field { name } => {
                let record = self.elements.last().ok_or(RuleError::NoElement)?;
                record
                    .get(name)
                    .cloned()
                    .ok_or_else(|| RuleError::UnknownField { name: name.clone() })
            }
            Expr::Table {
                table,
                column,
                cell,
                matches,
            } => self.table(table, column, *cell, matches),
            Expr::Add { args } => self.arithmetic(args, "add", Fixed::try_add),
            Expr::Sub { args } => self.arithmetic(args, "sub", Fixed::try_sub),
            Expr::Mul { args } => self.arithmetic(args, "mul", Fixed::try_mul),
            Expr::Div { args } => self.arithmetic(args, "div", Fixed::try_div),
            Expr::Min { args } => self.choose(args, "min", Fixed::min),
            Expr::Max { args } => self.choose(args, "max", Fixed::max),
            Expr::Neg { args } => self.unary(args, "neg", Fixed::try_neg),
            Expr::Abs { args } => self.unary(args, "abs", Fixed::try_abs),
            Expr::Round { args } => self.unary(args, "round", |value| Ok(value.round_to_integer())),
            Expr::Select {
                cond,
                then_branch,
                else_branch,
            } => self.select(cond, then_branch, else_branch),
            Expr::Eq { args } => self.equality(args, true),
            Expr::Ne { args } => self.equality(args, false),
            Expr::Lt { args } => self.ordering(args, Ordering::is_lt),
            Expr::Le { args } => self.ordering(args, Ordering::is_le),
            Expr::Gt { args } => self.ordering(args, Ordering::is_gt),
            Expr::Ge { args } => self.ordering(args, Ordering::is_ge),
            Expr::And { args } => conjunction(self, args),
            Expr::Or { args } => disjunction(self, args),
            Expr::Not { args } => self.negate(args),
            Expr::Sum { list, of } => self.sum(list, of),
            Expr::Count { list, condition } => count(self, list, condition.as_deref()),
        }
    }

    /// Evaluates a conditional value.
    fn select(
        &mut self,
        cond: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
    ) -> Result<Value, RuleError> {
        if self.eval(cond)?.as_bool()? {
            self.eval(then_branch)
        } else {
            self.eval(else_branch)
        }
    }

    /// Evaluates a logical negation over one boolean operand.
    fn negate(&mut self, args: &[Expr]) -> Result<Value, RuleError> {
        let [operand] = args else {
            return Err(RuleError::InvalidOperands {
                op: "not",
                expected: "one operand",
            });
        };
        Ok(Value::Bool(!self.eval(operand)?.as_bool()?))
    }

    /// Evaluates a two-operand arithmetic node.
    fn arithmetic<F>(
        &mut self,
        args: &[Expr],
        op: &'static str,
        apply: F,
    ) -> Result<Value, RuleError>
    where
        F: Fn(Fixed, Fixed) -> Result<Fixed, RuleError>,
    {
        let (first, second) = self.scalar_pair(args, op)?;
        apply(first, second).map(Value::Scalar)
    }

    /// Evaluates a two-operand scalar choice node.
    fn choose(
        &mut self,
        args: &[Expr],
        op: &'static str,
        pick: fn(Fixed, Fixed) -> Fixed,
    ) -> Result<Value, RuleError> {
        let (first, second) = self.scalar_pair(args, op)?;
        Ok(Value::Scalar(pick(first, second)))
    }

    /// Evaluates a one-operand scalar node.
    fn unary<F>(&mut self, args: &[Expr], op: &'static str, apply: F) -> Result<Value, RuleError>
    where
        F: Fn(Fixed) -> Result<Fixed, RuleError>,
    {
        let [operand] = args else {
            return Err(RuleError::InvalidOperands {
                op,
                expected: "one operand",
            });
        };
        apply(self.eval(operand)?.as_scalar()?).map(Value::Scalar)
    }

    /// Evaluates two scalar operands.
    fn scalar_pair(
        &mut self,
        args: &[Expr],
        op: &'static str,
    ) -> Result<(Fixed, Fixed), RuleError> {
        let [first, second] = args else {
            return Err(RuleError::InvalidOperands {
                op,
                expected: "two operands",
            });
        };
        Ok((
            self.eval(first)?.as_scalar()?,
            self.eval(second)?.as_scalar()?,
        ))
    }

    /// Evaluates equality over two values of the same type.
    fn equality(&mut self, args: &[Expr], equal: bool) -> Result<Value, RuleError> {
        let op = if equal { "eq" } else { "ne" };
        let [first, second] = args else {
            return Err(RuleError::InvalidOperands {
                op,
                expected: "two operands",
            });
        };
        let first = self.eval(first)?;
        let second = self.eval(second)?;
        let same = match (&first, &second) {
            (Value::Scalar(left), Value::Scalar(right)) => left == right,
            (Value::Text(left), Value::Text(right)) => left == right,
            (Value::Bool(left), Value::Bool(right)) => left == right,
            _ => {
                return Err(RuleError::TypeMismatch {
                    expected: "two values of the same type",
                    found: second.kind_name(),
                });
            }
        };
        Ok(Value::Bool(same == equal))
    }

    /// Evaluates an ordering comparison over scalars or ISO dates.
    fn ordering(&mut self, args: &[Expr], test: fn(Ordering) -> bool) -> Result<Value, RuleError> {
        let [first, second] = args else {
            return Err(RuleError::InvalidOperands {
                op: "order",
                expected: "two operands",
            });
        };
        let first = self.eval(first)?;
        let second = self.eval(second)?;
        let ordering = match (&first, &second) {
            (Value::Scalar(left), Value::Scalar(right)) => left.cmp(right),
            (Value::Text(left), Value::Text(right)) => {
                Date::from_iso(left)?.cmp(&Date::from_iso(right)?)
            }
            _ => {
                return Err(RuleError::TypeMismatch {
                    expected: "two scalars or two ISO dates",
                    found: second.kind_name(),
                });
            }
        };
        Ok(Value::Bool(test(ordering)))
    }

    /// Evaluates a sum over a list of records.
    fn sum(&mut self, list: &Expr, of: &Expr) -> Result<Value, RuleError> {
        let items = self.eval(list)?;
        let mut total = Fixed::ZERO;
        for item in items.as_list()? {
            let term = self.with_element(item, |evaluator| evaluator.eval(of)?.as_scalar())?;
            total = total.try_add(term)?;
        }
        Ok(Value::Scalar(total))
    }

    /// Evaluates a closure with one record element in scope.
    fn with_element<T>(
        &mut self,
        item: &Value,
        evaluate: impl FnOnce(&mut Self) -> Result<T, RuleError>,
    ) -> Result<T, RuleError> {
        let Value::Record(record) = item else {
            return Err(RuleError::TypeMismatch {
                expected: "record",
                found: item.kind_name(),
            });
        };
        self.elements.push(record.clone());
        let result = evaluate(self);
        self.elements.pop();
        result
    }

    /// Evaluates a table access with its predicates.
    fn table(
        &mut self,
        name: &str,
        column: &str,
        cell: CellType,
        matches: &[Predicate],
    ) -> Result<Value, RuleError> {
        let text = self
            .tables
            .get(name)
            .ok_or_else(|| RuleError::UnknownTable {
                table: name.to_string(),
            })?;
        let json: Json = serde_json::from_slice(text).map_err(|error| RuleError::TableJson {
            table: name.to_string(),
            line: error.line(),
            column: error.column(),
        })?;
        let rows =
            json.get("rows")
                .and_then(Json::as_array)
                .ok_or_else(|| RuleError::MissingRows {
                    table: name.to_string(),
                })?;
        let predicates = bind_predicates(self, name, matches)?;
        let row = select_row(rows, &predicates).ok_or_else(|| RuleError::MissingRow {
            table: name.to_string(),
        })?;
        let value = string_cell(row, name, column)?;
        match cell {
            CellType::Text => Ok(Value::Text(value.to_string())),
            CellType::Scalar => value.parse::<Fixed>().map(Value::Scalar).map_err(|_error| {
                RuleError::InvalidTableScalar {
                    table: name.to_string(),
                    column: column.to_string(),
                    value: value.to_string(),
                }
            }),
        }
    }
}

/// Returns a row cell as a string, or the error naming what is missing.
fn string_cell<'a>(row: &'a Json, table: &str, column: &str) -> Result<&'a str, RuleError> {
    match row.get(column) {
        Some(Json::String(value)) => Ok(value),
        Some(_) => Err(RuleError::NonStringCell {
            table: table.to_string(),
            column: column.to_string(),
        }),
        None => Err(RuleError::MissingColumn {
            table: table.to_string(),
            column: column.to_string(),
        }),
    }
}

/// Evaluates a conjunction with short-circuiting.
fn conjunction(evaluator: &mut Evaluator<'_>, args: &[Expr]) -> Result<Value, RuleError> {
    for argument in args {
        if !evaluator.eval(argument)?.as_bool()? {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

/// Evaluates a disjunction with short-circuiting.
fn disjunction(evaluator: &mut Evaluator<'_>, args: &[Expr]) -> Result<Value, RuleError> {
    for argument in args {
        if evaluator.eval(argument)?.as_bool()? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

/// Evaluates a count over a list of records.
fn count(
    evaluator: &mut Evaluator<'_>,
    list: &Expr,
    condition: Option<&Expr>,
) -> Result<Value, RuleError> {
    let items = evaluator.eval(list)?;
    let mut count = 0_usize;
    for item in items.as_list()? {
        let keep = match condition {
            Some(condition) => {
                evaluator.with_element(item, |inner| inner.eval(condition)?.as_bool())?
            }
            None => true,
        };
        count += usize::from(keep);
    }
    let scaled = i128::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(crate::fixed::SCALE))
        .ok_or(RuleError::Overflow)?;
    Ok(Value::Scalar(Fixed::from_scaled(scaled)))
}

/// Evaluates every predicate's operand into a bound matcher.
fn bind_predicates(
    evaluator: &mut Evaluator<'_>,
    table: &str,
    matches: &[Predicate],
) -> Result<Vec<BoundPredicate>, RuleError> {
    let mut predicates = Vec::with_capacity(matches.len());
    for predicate in matches {
        let (expr, prefix) = matcher(predicate, table)?;
        let evaluated = evaluator.eval(expr)?;
        predicates.push(BoundPredicate {
            column: predicate.column.clone(),
            value: evaluated.as_text()?.to_string(),
            prefix,
        });
    }
    Ok(predicates)
}

/// Returns one predicate's operand and whether it is a prefix matcher.
fn matcher<'a>(predicate: &'a Predicate, table: &str) -> Result<(&'a Expr, bool), RuleError> {
    match (&predicate.equals, &predicate.prefix_of) {
        (Some(equals), None) => Ok((equals, false)),
        (None, Some(prefix_of)) => Ok((prefix_of, true)),
        _ => Err(RuleError::InvalidPredicate {
            table: table.to_string(),
        }),
    }
}

/// A table predicate with its operand evaluated.
struct BoundPredicate {
    column: String,
    value: String,
    prefix: bool,
}

/// Selects the best row: all equalities hold, and the longest prefix match
/// wins where a prefix predicate is present.
fn select_row<'a>(rows: &'a [Json], predicates: &[BoundPredicate]) -> Option<&'a Json> {
    let mut best: Option<(usize, &Json)> = None;
    let prefix = predicates.iter().any(|predicate| predicate.prefix);
    for row in rows {
        let Json::Object(object) = row else {
            continue;
        };
        let Some(score) = match_row(object, predicates) else {
            continue;
        };
        if !prefix {
            return Some(row);
        }
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, row));
        }
    }
    best.map(|(_, row)| row)
}

/// Matches one row, returning the total prefix length when it matches.
fn match_row(row: &serde_json::Map<String, Json>, predicates: &[BoundPredicate]) -> Option<usize> {
    let mut score = 0_usize;
    for predicate in predicates {
        let Json::String(cell) = row.get(&predicate.column)? else {
            return None;
        };
        score += match_predicate(cell, predicate)?;
    }
    Some(score)
}

/// Returns one predicate's contribution to a row's match score.
fn match_predicate(cell: &str, predicate: &BoundPredicate) -> Option<usize> {
    if predicate.prefix {
        return prefix_score(cell, &predicate.value);
    }
    (cell == predicate.value).then_some(0)
}

/// Scores a prefix predicate: the cell must be a non-empty prefix.
fn prefix_score(cell: &str, value: &str) -> Option<usize> {
    if cell.is_empty() || !value.starts_with(cell) {
        return None;
    }
    Some(cell.len())
}

/// Builds a constant value.
fn constant(
    scalar: Option<&str>,
    text: Option<&str>,
    boolean: Option<bool>,
) -> Result<Value, RuleError> {
    match (scalar, text, boolean) {
        (Some(value), None, None) => value.parse::<Fixed>().map(Value::Scalar).map_err(|_error| {
            RuleError::InvalidConstantScalar {
                value: value.to_string(),
            }
        }),
        (None, Some(value), None) => Ok(Value::Text(value.to_string())),
        (None, None, Some(value)) => Ok(Value::Bool(value)),
        _ => Err(RuleError::InvalidConstant),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::vec;

    /// A bundle fixture: `rules.json` plus named parameter tables.
    struct Fixture {
        files: Vec<(String, Vec<u8>)>,
    }

    impl Fixture {
        fn new(rules: &str) -> Self {
            Self {
                files: vec![(String::from("rules.json"), rules.as_bytes().to_vec())],
            }
        }

        fn table(mut self, name: &str, content: &str) -> Self {
            self.files.push((
                format!("parameters/{name}.json"),
                content.as_bytes().to_vec(),
            ));
            self
        }

        fn bundle(&self) -> RuleBundle<'_> {
            let files: Vec<BundleFile<'_>> = self
                .files
                .iter()
                .map(|(path, content)| BundleFile {
                    path: path.as_str(),
                    content: content.as_slice(),
                })
                .collect();
            RuleBundle::from_files(&files).unwrap()
        }

        fn files(&self) -> Vec<BundleFile<'_>> {
            self.files
                .iter()
                .map(|(path, content)| BundleFile {
                    path: path.as_str(),
                    content: content.as_slice(),
                })
                .collect()
        }
    }

    /// Synthetic rules mirroring the aluminium methodology shapes.
    const RULES: &str = r#"{
  "schema": "kaimeter-rules-v1",
  "types": {
    "supply": {"fields": {"see_tco2e_per_t": "scalar", "quantity_t": "scalar", "origin": "text"}}
  },
  "rules": {
    "test.slope": {
      "legal": "IR (EU) 2025/2547, Annex II, B.7.1",
      "source": "http://data.europa.eu/eli/reg_impl/2025/2547/oj",
      "since": "2026.3.0",
      "inputs": {
        "production_t": {"type": "scalar"},
        "direct_co2_t": {"type": "scalar"},
        "aem": {"type": "scalar"},
        "sef": {"type": "scalar"},
        "fc2f6": {"type": "scalar"}
      },
      "outputs": {
        "cf4_t": {"op": "div", "args": [
          {"op": "mul", "args": [
            {"op": "mul", "args": [{"op": "input", "name": "aem"}, {"op": "input", "name": "sef"}]},
            {"op": "input", "name": "production_t"}
          ]},
          {"op": "const", "scalar": "1000"}
        ]},
        "c2f6_t": {"op": "mul", "args": [
          {"op": "div", "args": [
            {"op": "mul", "args": [
              {"op": "mul", "args": [{"op": "input", "name": "aem"}, {"op": "input", "name": "sef"}]},
              {"op": "input", "name": "production_t"}
            ]},
            {"op": "const", "scalar": "1000"}
          ]},
          {"op": "input", "name": "fc2f6"}
        ]},
        "pfc_tco2e": {"op": "add", "args": [
          {"op": "mul", "args": [
            {"op": "table", "table": "gwp", "column": "gwp100",
              "match": [{"column": "substance", "equals": {"op": "const", "text": "CF4"}}]},
            {"op": "div", "args": [
              {"op": "mul", "args": [
                {"op": "mul", "args": [{"op": "input", "name": "aem"}, {"op": "input", "name": "sef"}]},
                {"op": "input", "name": "production_t"}
              ]},
              {"op": "const", "scalar": "1000"}
            ]}
          ]},
          {"op": "mul", "args": [
            {"op": "table", "table": "gwp", "column": "gwp100",
              "match": [{"column": "substance", "equals": {"op": "const", "text": "C2F6"}}]},
            {"op": "mul", "args": [
              {"op": "div", "args": [
                {"op": "mul", "args": [
                  {"op": "mul", "args": [{"op": "input", "name": "aem"}, {"op": "input", "name": "sef"}]},
                  {"op": "input", "name": "production_t"}
                ]},
                {"op": "const", "scalar": "1000"}
              ]},
              {"op": "input", "name": "fc2f6"}
            ]}
          ]}
        ]},
        "see_tco2e_per_t": {"op": "div", "args": [
          {"op": "add", "args": [
            {"op": "input", "name": "direct_co2_t"},
            {"op": "add", "args": [
              {"op": "mul", "args": [
                {"op": "table", "table": "gwp", "column": "gwp100",
                  "match": [{"column": "substance", "equals": {"op": "const", "text": "CF4"}}]},
                {"op": "div", "args": [
                  {"op": "mul", "args": [
                    {"op": "mul", "args": [{"op": "input", "name": "aem"}, {"op": "input", "name": "sef"}]},
                    {"op": "input", "name": "production_t"}
                  ]},
                  {"op": "const", "scalar": "1000"}
                ]}
              ]},
              {"op": "mul", "args": [
                {"op": "table", "table": "gwp", "column": "gwp100",
                  "match": [{"column": "substance", "equals": {"op": "const", "text": "C2F6"}}]},
                {"op": "mul", "args": [
                  {"op": "div", "args": [
                    {"op": "mul", "args": [
                      {"op": "mul", "args": [{"op": "input", "name": "aem"}, {"op": "input", "name": "sef"}]},
                      {"op": "input", "name": "production_t"}
                    ]},
                    {"op": "const", "scalar": "1000"}
                  ]},
                  {"op": "input", "name": "fc2f6"}
                ]}
              ]}
            ]}
          ]},
          {"op": "input", "name": "production_t"}
        ]}
      },
      "proves": "see_tco2e_per_t"
    },
    "test.complex": {
      "legal": "IR (EU) 2025/2547, Articles 13-14",
      "source": "http://data.europa.eu/eli/reg_impl/2025/2547/oj",
      "since": "2026.3.0",
      "inputs": {
        "production_t": {"type": "scalar"},
        "direct_co2_t": {"type": "scalar"},
        "supplies": {"type": "list", "of": "supply"}
      },
      "outputs": {
        "weighted_tco2e": {"op": "sum",
          "list": {"op": "input", "name": "supplies"},
          "of": {"op": "select",
            "cond": {"op": "eq", "args": [
              {"op": "field", "name": "origin"},
              {"op": "const", "text": "third_country"}
            ]},
            "then": {"op": "mul", "args": [
              {"op": "field", "name": "quantity_t"},
              {"op": "field", "name": "see_tco2e_per_t"}
            ]},
            "else": {"op": "const", "scalar": "0"}
          }
        },
        "count": {"op": "count", "list": {"op": "input", "name": "supplies"}},
        "count_third": {"op": "count",
          "list": {"op": "input", "name": "supplies"},
          "where": {"op": "eq", "args": [
            {"op": "field", "name": "origin"},
            {"op": "const", "text": "third_country"}
          ]}
        },
        "see_tco2e_per_t": {"op": "div", "args": [
          {"op": "add", "args": [
            {"op": "input", "name": "direct_co2_t"},
            {"op": "sum",
              "list": {"op": "input", "name": "supplies"},
              "of": {"op": "select",
                "cond": {"op": "eq", "args": [
                  {"op": "field", "name": "origin"},
                  {"op": "const", "text": "third_country"}
                ]},
                "then": {"op": "mul", "args": [
                  {"op": "field", "name": "quantity_t"},
                  {"op": "field", "name": "see_tco2e_per_t"}
                ]},
                "else": {"op": "const", "scalar": "0"}
              }
            }
          ]},
          {"op": "input", "name": "production_t"}
        ]}
      },
      "proves": "see_tco2e_per_t"
    },
    "test.tables": {
      "legal": "test",
      "source": "https://example.test",
      "since": "2026.3.0",
      "inputs": {
        "substance": {"type": "text"},
        "cn_code": {"type": "text"}
      },
      "outputs": {
        "gwp": {"op": "table", "table": "gwp", "column": "gwp100",
          "match": [{"column": "substance", "equals": {"op": "input", "name": "substance"}}]},
        "label": {"op": "table", "table": "gwp", "column": "label", "as": "text",
          "match": [{"column": "substance", "equals": {"op": "input", "name": "substance"}}]},
        "region": {"op": "table", "table": "defaults", "column": "region", "as": "text",
          "match": [{"column": "cn_code", "prefixOf": {"op": "input", "name": "cn_code"}}]}
      },
      "proves": "gwp"
    },
    "test.controls": {
      "legal": "test",
      "source": "https://example.test",
      "since": "2026.3.0",
      "inputs": {
        "x": {"type": "scalar"},
        "date": {"type": "text"},
        "flag": {"type": "bool"}
      },
      "outputs": {
        "neg": {"op": "neg", "args": [{"op": "input", "name": "x"}]},
        "abs": {"op": "abs", "args": [{"op": "input", "name": "x"}]},
        "round": {"op": "round", "args": [{"op": "input", "name": "x"}]},
        "min": {"op": "min", "args": [{"op": "input", "name": "x"}, {"op": "const", "scalar": "1"}]},
        "max": {"op": "max", "args": [{"op": "input", "name": "x"}, {"op": "const", "scalar": "1"}]},
        "sub": {"op": "sub", "args": [{"op": "input", "name": "x"}, {"op": "const", "scalar": "1"}]},
        "lt": {"op": "lt", "args": [{"op": "input", "name": "x"}, {"op": "const", "scalar": "1"}]},
        "le": {"op": "le", "args": [{"op": "input", "name": "x"}, {"op": "const", "scalar": "1"}]},
        "gt": {"op": "gt", "args": [{"op": "input", "name": "x"}, {"op": "const", "scalar": "1"}]},
        "ge": {"op": "ge", "args": [{"op": "input", "name": "x"}, {"op": "const", "scalar": "1"}]},
        "ne": {"op": "ne", "args": [{"op": "input", "name": "x"}, {"op": "const", "scalar": "1"}]},
        "date_ok": {"op": "lt", "args": [
          {"op": "input", "name": "date"},
          {"op": "const", "text": "2027-01-01"}
        ]},
        "and": {"op": "and", "args": [
          {"op": "input", "name": "flag"},
          {"op": "const", "bool": false}
        ]},
        "or": {"op": "or", "args": [
          {"op": "input", "name": "flag"},
          {"op": "const", "bool": false}
        ]},
        "not": {"op": "not", "args": [{"op": "input", "name": "flag"}]},
        "select": {"op": "select",
          "cond": {"op": "input", "name": "flag"},
          "then": {"op": "const", "scalar": "1"},
          "else": {"op": "const", "scalar": "2"}
        }
      },
      "proves": "neg"
    }
  }
}"#;

    fn fixture() -> Fixture {
        Fixture::new(RULES)
            .table(
                "gwp",
                r#"{"rows":[{"substance":"CF4","gwp100":"6630","label":"cf4"},{"substance":"C2F6","gwp100":"11100","label":"c2f6"}]}"#,
            )
            .table(
                "defaults",
                r#"{"rows":[{"cn_code":"76","region":"aluminium"},{"cn_code":"7604","region":"profiles"},{"cn_code":"76041090","region":"profiles-8"},{"cn_code":"","region":"never"}]}"#,
            )
    }

    /// Parses an invocation with the shared context.
    fn invoke(bundle: &RuleBundle<'_>, rule: &str, inputs: &str) -> Result<Invocation, RuleError> {
        bundle.parse_invocation(&format!(
            r#"{{"rule":"{rule}","context":{{"sector":"aluminium","route":"primary","cnCode":"7601","period":"2026"}},"inputs":{inputs}}}"#
        ))
    }

    fn scalar(value: &str) -> Value {
        Value::Scalar(value.parse().unwrap())
    }

    #[test]
    fn evaluates_the_slope_rule_with_the_gwp_table() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        let invocation = invoke(
            &bundle,
            "test.slope",
            r#"{"production_t":"100000","direct_co2_t":"155000","aem":"0.25",
                "sef":"0.143","fc2f6":"0.121"}"#,
        )
        .unwrap();
        let outcome = bundle.evaluate(&invocation).unwrap();
        assert_eq!(outcome.proves, "see_tco2e_per_t");
        assert_eq!(outcome.output, "1.835038".parse::<Fixed>().unwrap());
        assert_eq!(outcome.outputs["cf4_t"], scalar("3.575"));
        assert_eq!(outcome.outputs["c2f6_t"], scalar("0.432575"));
        assert_eq!(outcome.outputs["pfc_tco2e"], scalar("28503.8325"));
        assert_eq!(outcome.outputs["see_tco2e_per_t"], scalar("1.835038"));
    }

    #[test]
    fn reports_zero_production_and_overflow() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        let empty = invoke(
            &bundle,
            "test.slope",
            r#"{"production_t":"0","direct_co2_t":"155000","aem":"0.25",
                "sef":"0.143","fc2f6":"0.121"}"#,
        )
        .unwrap();
        assert_eq!(bundle.evaluate(&empty), Err(RuleError::DivisionByZero));
        let huge = invoke(
            &bundle,
            "test.slope",
            r#"{"production_t":"100000","direct_co2_t":"170141183460469231731687303715884105727",
                "aem":"0.25","sef":"0.143","fc2f6":"0.121"}"#,
        );
        assert!(matches!(huge, Err(RuleError::InvalidConstantScalar { .. })));
    }

    #[test]
    fn evaluates_the_complex_rule_and_counts() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        let invocation = invoke(
            &bundle,
            "test.complex",
            r#"{"production_t":"1000","direct_co2_t":"120","supplies":[
                {"see_tco2e_per_t":"1.835","quantity_t":"600","origin":"third_country"},
                {"see_tco2e_per_t":"1.910","quantity_t":"430","origin":"third_country"},
                {"see_tco2e_per_t":"9.9","quantity_t":"100","origin":"eu_or_excluded"}
            ]}"#,
        )
        .unwrap();
        let outcome = bundle.evaluate(&invocation).unwrap();
        assert_eq!(outcome.outputs["weighted_tco2e"], scalar("1922.3"));
        assert_eq!(outcome.outputs["count"], scalar("3"));
        assert_eq!(outcome.outputs["count_third"], scalar("2"));
        assert_eq!(outcome.outputs["see_tco2e_per_t"], scalar("2.0423"));
    }

    #[test]
    fn evaluates_table_forms() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        let invocation = invoke(
            &bundle,
            "test.tables",
            r#"{"substance":"CF4","cn_code":"76041090"}"#,
        )
        .unwrap();
        let outcome = bundle.evaluate(&invocation).unwrap();
        assert_eq!(outcome.outputs["gwp"], scalar("6630"));
        assert_eq!(outcome.outputs["label"], Value::Text(String::from("cf4")));
        assert_eq!(
            outcome.outputs["region"],
            Value::Text(String::from("profiles-8"))
        );

        let shorter = invoke(
            &bundle,
            "test.tables",
            r#"{"substance":"C2F6","cn_code":"76049999"}"#,
        )
        .unwrap();
        assert_eq!(
            bundle.evaluate(&shorter).unwrap().outputs["region"],
            Value::Text(String::from("profiles"))
        );

        let unlisted = invoke(
            &bundle,
            "test.tables",
            r#"{"substance":"C2F6","cn_code":"99999999"}"#,
        )
        .unwrap();
        assert_eq!(
            bundle.evaluate(&unlisted),
            Err(RuleError::MissingRow {
                table: String::from("defaults"),
            })
        );
    }

    #[test]
    fn evaluates_control_forms() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        let invocation = invoke(
            &bundle,
            "test.controls",
            r#"{"x":"-2.5","date":"2026-06-15","flag":true}"#,
        )
        .unwrap();
        let outputs = bundle.evaluate(&invocation).unwrap().outputs;
        assert_eq!(outputs["neg"], scalar("2.5"));
        assert_eq!(outputs["abs"], scalar("2.5"));
        assert_eq!(outputs["round"], scalar("-2"));
        assert_eq!(outputs["min"], scalar("-2.5"));
        assert_eq!(outputs["max"], scalar("1"));
        assert_eq!(outputs["sub"], scalar("-3.5"));
        assert_eq!(outputs["lt"], Value::Bool(true));
        assert_eq!(outputs["le"], Value::Bool(true));
        assert_eq!(outputs["gt"], Value::Bool(false));
        assert_eq!(outputs["ge"], Value::Bool(false));
        assert_eq!(outputs["ne"], Value::Bool(true));
        assert_eq!(outputs["date_ok"], Value::Bool(true));
        assert_eq!(outputs["and"], Value::Bool(false));
        assert_eq!(outputs["or"], Value::Bool(true));
        assert_eq!(outputs["not"], Value::Bool(false));
        assert_eq!(outputs["select"], scalar("1"));
    }

    #[test]
    fn reports_missing_and_unknown_inputs() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        assert_eq!(
            invoke(&bundle, "test.slope", r#"{"production_t":"1"}"#),
            Err(RuleError::MissingInput {
                name: String::from("aem"),
            })
        );
        assert_eq!(
            invoke(&bundle, "test.slope", r#"{"extra":"1"}"#),
            Err(RuleError::UnknownInput {
                name: String::from("extra"),
            })
        );
        assert_eq!(
            invoke(&bundle, "missing", "{}"),
            Err(RuleError::UnknownRule {
                rule: String::from("missing"),
            })
        );
        assert_eq!(
            invoke(
                &bundle,
                "test.slope",
                r#"{"production_t":1,"direct_co2_t":"0","aem":"0","sef":"0","fc2f6":"0"}"#
            )
            .unwrap_err(),
            RuleError::TypeMismatch {
                expected: "decimal string",
                found: "number",
            }
        );
    }

    #[test]
    fn rejects_malformed_invocations() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        assert!(matches!(
            bundle.parse_invocation("{"),
            Err(RuleError::InvalidJson { .. })
        ));
        assert_eq!(
            bundle.parse_invocation("[]"),
            Err(RuleError::Invocation {
                expected: "an object with rule, context and inputs",
            })
        );
        assert_eq!(
            bundle.parse_invocation("{}"),
            Err(RuleError::Invocation {
                expected: "a rule name",
            })
        );
        assert_eq!(
            bundle.parse_invocation(r#"{"rule":"test.slope","context":{},"inputs":{}}"#),
            Err(RuleError::Invocation {
                expected: "a context with sector, route, cnCode and period",
            })
        );
        assert_eq!(
            bundle.parse_invocation(
                r#"{"rule":"test.slope","context":{"sector":"a","route":"b","cnCode":"c","period":"d"}}"#
            ),
            Err(RuleError::Invocation {
                expected: "an inputs object",
            })
        );
    }

    #[test]
    fn checks_programmatic_invocations() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        let context = Context {
            sector: String::from("aluminium"),
            route: String::from("primary"),
            cn_code: String::from("7601"),
            period: String::from("2026"),
        };
        let missing = Invocation {
            rule: String::from("test.complex"),
            context: context.clone(),
            inputs: BTreeMap::new(),
        };
        assert_eq!(
            bundle.evaluate(&missing),
            Err(RuleError::MissingInput {
                name: String::from("direct_co2_t"),
            })
        );
        let extra = Invocation {
            rule: String::from("test.complex"),
            context: context.clone(),
            inputs: BTreeMap::from([
                (String::from("production_t"), scalar("1")),
                (String::from("direct_co2_t"), scalar("0")),
                (String::from("supplies"), Value::List(Vec::new())),
                (String::from("extra"), scalar("1")),
            ]),
        };
        assert_eq!(
            bundle.evaluate(&extra),
            Err(RuleError::UnknownInput {
                name: String::from("extra"),
            })
        );
        let wrong_type = Invocation {
            rule: String::from("test.complex"),
            context,
            inputs: BTreeMap::from([
                (String::from("production_t"), Value::Text(String::from("1"))),
                (String::from("direct_co2_t"), scalar("0")),
                (String::from("supplies"), Value::List(Vec::new())),
            ]),
        };
        assert_eq!(
            bundle.evaluate(&wrong_type),
            Err(RuleError::TypeMismatch {
                expected: "scalar",
                found: "text",
            })
        );
    }

    #[test]
    fn checks_record_shapes() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        let context = Context {
            sector: String::from("aluminium"),
            route: String::from("primary"),
            cn_code: String::from("7601"),
            period: String::from("2026"),
        };
        let mut record = BTreeMap::from([
            (String::from("see_tco2e_per_t"), scalar("1.835")),
            (String::from("quantity_t"), scalar("600")),
            (
                String::from("origin"),
                Value::Text(String::from("third_country")),
            ),
        ]);
        let invocation = |record: BTreeMap<String, Value>| Invocation {
            rule: String::from("test.complex"),
            context: context.clone(),
            inputs: BTreeMap::from([
                (String::from("production_t"), scalar("1000")),
                (String::from("direct_co2_t"), scalar("0")),
                (
                    String::from("supplies"),
                    Value::List(vec![Value::Record(record)]),
                ),
            ]),
        };
        bundle.evaluate(&invocation(record.clone())).unwrap();

        record.remove("origin");
        assert_eq!(
            bundle.evaluate(&invocation(record.clone())),
            Err(RuleError::MissingField {
                name: String::from("origin"),
            })
        );

        record.insert(String::from("origin"), Value::Text(String::from("x")));
        record.insert(String::from("extra"), scalar("1"));
        assert_eq!(
            bundle.evaluate(&invocation(record.clone())),
            Err(RuleError::UnknownField {
                name: String::from("extra"),
            })
        );

        record.remove("extra");
        record.insert(String::from("origin"), scalar("1"));
        assert_eq!(
            bundle.evaluate(&invocation(record)),
            Err(RuleError::TypeMismatch {
                expected: "text",
                found: "scalar",
            })
        );
    }

    #[test]
    fn loads_bundles_and_hashes_them() {
        let fixture = fixture();
        let files = fixture.files();
        let bundle = RuleBundle::from_files(&files).unwrap();
        assert!(bundle.schema().rules.contains_key("test.slope"));
        assert_eq!(bundle.bundle_hash().unwrap().to_hex().len(), 64);
        assert!(matches!(
            RuleBundle::from_files(&[BundleFile {
                path: "bundle.json",
                content: b"{}",
            },]),
            Err(RuleError::MissingRulesFile)
        ));
        assert!(matches!(
            RuleBundle::from_files(&[BundleFile {
                path: "rules.json",
                content: &[0xff],
            }]),
            Err(RuleError::RulesNotUtf8)
        ));
        assert!(matches!(
            RuleBundle::from_files(&[BundleFile {
                path: "rules.json",
                content: b"{",
            }]),
            Err(RuleError::Schema { .. })
        ));
    }

    /// Builds an evaluator over the fixture for hand-built expressions.
    fn evaluator<'a>(
        inputs: &'a BTreeMap<String, Value>,
        tables: &'a BTreeMap<String, Vec<u8>>,
    ) -> Evaluator<'a> {
        Evaluator {
            tables,
            inputs,
            elements: Vec::new(),
        }
    }

    /// Inputs and parameter tables for the hand-built expression tests.
    fn expression_fixture() -> (BTreeMap<String, Value>, BTreeMap<String, Vec<u8>>) {
        let mut inputs = BTreeMap::new();
        inputs.insert(String::from("x"), scalar("-2.5"));
        inputs.insert(
            String::from("text"),
            Value::Text(String::from("2026-06-15")),
        );
        inputs.insert(String::from("flag"), Value::Bool(true));
        let mut tables = BTreeMap::new();
        tables.insert(
            String::from("gwp"),
            br#"{"rows":[{"substance":"CF4","gwp100":"6630"}]}"#.to_vec(),
        );
        (inputs, tables)
    }

    #[test]
    fn reports_operand_errors() {
        let (inputs, tables) = expression_fixture();
        let mut evaluator = evaluator(&inputs, &tables);

        let eval = |evaluator: &mut Evaluator<'_>, expr: Expr| evaluator.eval(&expr);
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Add {
                    args: vec![Expr::Input {
                        name: String::from("x")
                    }],
                }
            ),
            Err(RuleError::InvalidOperands {
                op: "add",
                expected: "two operands",
            })
        );
        assert_eq!(
            eval(&mut evaluator, Expr::Neg { args: Vec::new() }),
            Err(RuleError::InvalidOperands {
                op: "neg",
                expected: "one operand",
            })
        );
        assert_eq!(
            eval(&mut evaluator, Expr::Not { args: Vec::new() }),
            Err(RuleError::InvalidOperands {
                op: "not",
                expected: "one operand",
            })
        );
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Field {
                    name: String::from("see")
                }
            ),
            Err(RuleError::NoElement)
        );
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Input {
                    name: String::from("missing")
                }
            ),
            Err(RuleError::UnknownInput {
                name: String::from("missing"),
            })
        );
    }

    #[test]
    fn reports_constant_errors() {
        let (inputs, tables) = expression_fixture();
        let mut evaluator = evaluator(&inputs, &tables);
        let eval = |evaluator: &mut Evaluator<'_>, expr: Expr| evaluator.eval(&expr);
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Eq {
                    args: vec![
                        Expr::Const {
                            scalar: Some(String::from("1")),
                            text: None,
                            boolean: None
                        },
                        Expr::Const {
                            scalar: None,
                            text: Some(String::from("1")),
                            boolean: None
                        },
                    ],
                }
            ),
            Err(RuleError::TypeMismatch {
                expected: "two values of the same type",
                found: "text",
            })
        );
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Const {
                    scalar: None,
                    text: None,
                    boolean: None
                }
            ),
            Err(RuleError::InvalidConstant)
        );
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Const {
                    scalar: Some(String::from("many")),
                    text: None,
                    boolean: None,
                }
            ),
            Err(RuleError::InvalidConstantScalar {
                value: String::from("many"),
            })
        );
    }

    #[test]
    fn evaluates_inputs_and_dates() {
        let (inputs, tables) = expression_fixture();
        let mut evaluator = evaluator(&inputs, &tables);
        let eval = |evaluator: &mut Evaluator<'_>, expr: Expr| evaluator.eval(&expr);
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Input {
                    name: String::from("text")
                }
            ),
            Ok(Value::Text(String::from("2026-06-15")))
        );
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Lt {
                    args: vec![
                        Expr::Input {
                            name: String::from("text")
                        },
                        Expr::Const {
                            scalar: None,
                            text: Some(String::from("2027-01-01")),
                            boolean: None
                        },
                    ],
                }
            ),
            Ok(Value::Bool(true))
        );
        assert!(
            eval(
                &mut evaluator,
                Expr::Lt {
                    args: vec![
                        Expr::Const {
                            scalar: None,
                            text: Some(String::from("not a date")),
                            boolean: None
                        },
                        Expr::Const {
                            scalar: None,
                            text: Some(String::from("2027-01-01")),
                            boolean: None
                        },
                    ],
                }
            )
            .is_err()
        );
    }

    #[test]
    fn reports_unknown_tables() {
        let (inputs, tables) = expression_fixture();
        let mut evaluator = evaluator(&inputs, &tables);
        let eval = |evaluator: &mut Evaluator<'_>, expr: Expr| evaluator.eval(&expr);
        assert_eq!(
            eval(
                &mut evaluator,
                Expr::Table {
                    table: String::from("missing"),
                    column: String::from("x"),
                    cell: CellType::Scalar,
                    matches: Vec::new(),
                }
            ),
            Err(RuleError::UnknownTable {
                table: String::from("missing"),
            })
        );
    }

    #[test]
    fn journal_binds_the_outcome() {
        let fixture = fixture();
        let bundle = fixture.bundle();
        let invocation = invoke(
            &bundle,
            "test.slope",
            r#"{"production_t":"100000","direct_co2_t":"155000","aem":"0.25",
                "sef":"0.143","fc2f6":"0.121"}"#,
        )
        .unwrap();
        let outcome = bundle.evaluate(&invocation).unwrap();
        let journal = outcome.journal(bundle.bundle_hash().unwrap(), invocation.context.clone());
        assert_eq!(journal.output, outcome.output);
        assert_eq!(journal.context, invocation.context);
        assert!(
            journal
                .canonical_bytes()
                .starts_with(b"kaimeter-journal-v1\n")
        );
    }

    #[test]
    fn names_parameter_table_paths() {
        assert_eq!(parameter_table_name("parameters/gwp.json"), Some("gwp"));
        assert_eq!(parameter_table_name("parameters/sub/gwp.json"), None);
        assert_eq!(parameter_table_name("gwp.json"), None);
        assert_eq!(parameter_table_name("parameters/gwp.txt"), None);
    }
}
