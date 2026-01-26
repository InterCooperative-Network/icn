//! Expression mini-language for CCL schemas.
//!
//! A deterministic, bounded expression language for computed values in schemas.
//!
//! # Grammar
//!
//! ```text
//! expr := literal | field_ref | binary_op | unary_op | function_call | conditional
//!
//! literal := number | string | boolean | fraction
//! field_ref := identifier ("." identifier)*
//! binary_op := expr (op expr)+
//! unary_op := ("not" | "-") expr
//! function_call := identifier "(" expr ("," expr)* ")"
//! conditional := "if" expr "then" expr "else" expr
//! ```
//!
//! # Determinism
//!
//! All expressions are deterministic: same input → same output.
//! No randomness, no I/O, no side effects.
//!
//! # Boundedness
//!
//! No loops, no recursion, no unbounded computation.
//! Expression depth is limited to prevent stack overflow.

use super::SchemaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum expression nesting depth.
const MAX_EXPR_DEPTH: usize = 32;

/// A schema expression (parsed from string).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SchemaExpr {
    /// Literal value
    Literal(SchemaValue),
    /// Field reference (e.g., "members", "trust_score")
    Field(String),
    /// Binary operation
    BinaryOp {
        left: Box<SchemaExpr>,
        op: BinaryOp,
        right: Box<SchemaExpr>,
    },
    /// Unary operation
    UnaryOp { op: UnaryOp, expr: Box<SchemaExpr> },
    /// Function call
    FunctionCall { name: String, args: Vec<SchemaExpr> },
    /// Conditional
    Conditional {
        condition: Box<SchemaExpr>,
        then_expr: Box<SchemaExpr>,
        else_expr: Box<SchemaExpr>,
    },
}

/// Values in schema expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SchemaValue {
    /// Integer
    Int(i64),
    /// Floating point
    Float(f64),
    /// Boolean
    Bool(bool),
    /// String
    String(String),
    /// Fraction (e.g., "2/3")
    Fraction { numerator: i64, denominator: i64 },
    /// List of values
    List(Vec<SchemaValue>),
    /// Null/None
    Null,
}

impl SchemaValue {
    /// Convert to f64 if numeric.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            SchemaValue::Int(i) => Some(*i as f64),
            SchemaValue::Float(f) => Some(*f),
            SchemaValue::Fraction {
                numerator,
                denominator,
            } => {
                if *denominator == 0 {
                    None
                } else {
                    Some(*numerator as f64 / *denominator as f64)
                }
            }
            _ => None,
        }
    }

    /// Convert to bool if boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SchemaValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Check if value is truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            SchemaValue::Bool(b) => *b,
            SchemaValue::Int(i) => *i != 0,
            SchemaValue::Float(f) => *f != 0.0,
            SchemaValue::String(s) => !s.is_empty(),
            SchemaValue::List(l) => !l.is_empty(),
            SchemaValue::Fraction { numerator, .. } => *numerator != 0,
            SchemaValue::Null => false,
        }
    }
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical
    And,
    Or,
}

impl BinaryOp {
    /// Parse operator from string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "+" => Some(BinaryOp::Add),
            "-" => Some(BinaryOp::Sub),
            "*" => Some(BinaryOp::Mul),
            "/" => Some(BinaryOp::Div),
            "==" => Some(BinaryOp::Eq),
            "!=" => Some(BinaryOp::Ne),
            "<" => Some(BinaryOp::Lt),
            "<=" => Some(BinaryOp::Le),
            ">" => Some(BinaryOp::Gt),
            ">=" => Some(BinaryOp::Ge),
            "and" => Some(BinaryOp::And),
            "or" => Some(BinaryOp::Or),
            _ => None,
        }
    }

    /// Get operator precedence (higher = binds tighter).
    pub fn precedence(&self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Eq | BinaryOp::Ne => 3,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 4,
            BinaryOp::Add | BinaryOp::Sub => 5,
            BinaryOp::Mul | BinaryOp::Div => 6,
        }
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnaryOp {
    Not,
    Neg,
}

/// Evaluation context with field bindings.
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    /// Field values available during evaluation
    fields: HashMap<String, SchemaValue>,
}

impl EvalContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field binding.
    pub fn with_field(mut self, name: impl Into<String>, value: SchemaValue) -> Self {
        self.fields.insert(name.into(), value);
        self
    }

    /// Set a field value.
    pub fn set_field(&mut self, name: impl Into<String>, value: SchemaValue) {
        self.fields.insert(name.into(), value);
    }

    /// Get a field value.
    pub fn get_field(&self, name: &str) -> Option<&SchemaValue> {
        self.fields.get(name)
    }

    /// Evaluate an expression in this context.
    pub fn evaluate(&self, expr: &SchemaExpr) -> Result<SchemaValue, SchemaError> {
        self.evaluate_with_depth(expr, 0)
    }

    fn evaluate_with_depth(
        &self,
        expr: &SchemaExpr,
        depth: usize,
    ) -> Result<SchemaValue, SchemaError> {
        if depth > MAX_EXPR_DEPTH {
            return Err(SchemaError::Expression(
                "Maximum expression depth exceeded".to_string(),
            ));
        }

        match expr {
            SchemaExpr::Literal(v) => Ok(v.clone()),

            SchemaExpr::Field(name) => {
                // Handle dotted field references (e.g., "board.seats")
                let parts: Vec<&str> = name.split('.').collect();
                let base = self.fields.get(parts[0]).cloned().ok_or_else(|| {
                    SchemaError::Reference(format!("Unknown field: {}", parts[0]))
                })?;

                // For now, only support single-level references
                if parts.len() > 1 {
                    return Err(SchemaError::Expression(
                        "Nested field references not yet supported".to_string(),
                    ));
                }

                Ok(base)
            }

            SchemaExpr::BinaryOp { left, op, right } => {
                let lval = self.evaluate_with_depth(left, depth + 1)?;
                let rval = self.evaluate_with_depth(right, depth + 1)?;
                self.apply_binary_op(*op, lval, rval)
            }

            SchemaExpr::UnaryOp { op, expr } => {
                let val = self.evaluate_with_depth(expr, depth + 1)?;
                self.apply_unary_op(*op, val)
            }

            SchemaExpr::FunctionCall { name, args } => {
                let evaluated_args: Result<Vec<_>, _> = args
                    .iter()
                    .map(|a| self.evaluate_with_depth(a, depth + 1))
                    .collect();
                self.apply_function(name, evaluated_args?)
            }

            SchemaExpr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                let cond = self.evaluate_with_depth(condition, depth + 1)?;
                if cond.is_truthy() {
                    self.evaluate_with_depth(then_expr, depth + 1)
                } else {
                    self.evaluate_with_depth(else_expr, depth + 1)
                }
            }
        }
    }

    fn apply_binary_op(
        &self,
        op: BinaryOp,
        left: SchemaValue,
        right: SchemaValue,
    ) -> Result<SchemaValue, SchemaError> {
        match op {
            // Arithmetic operations
            BinaryOp::Add => {
                let l = left.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Left operand not numeric".to_string())
                })?;
                let r = right.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Right operand not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(l + r))
            }
            BinaryOp::Sub => {
                let l = left.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Left operand not numeric".to_string())
                })?;
                let r = right.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Right operand not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(l - r))
            }
            BinaryOp::Mul => {
                let l = left.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Left operand not numeric".to_string())
                })?;
                let r = right.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Right operand not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(l * r))
            }
            BinaryOp::Div => {
                let l = left.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Left operand not numeric".to_string())
                })?;
                let r = right.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Right operand not numeric".to_string())
                })?;
                if r == 0.0 {
                    return Err(SchemaError::Expression("Division by zero".to_string()));
                }
                Ok(SchemaValue::Float(l / r))
            }

            // Comparison operations
            BinaryOp::Eq => Ok(SchemaValue::Bool(left == right)),
            BinaryOp::Ne => Ok(SchemaValue::Bool(left != right)),
            BinaryOp::Lt => {
                let l = left.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Left operand not numeric".to_string())
                })?;
                let r = right.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Right operand not numeric".to_string())
                })?;
                Ok(SchemaValue::Bool(l < r))
            }
            BinaryOp::Le => {
                let l = left.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Left operand not numeric".to_string())
                })?;
                let r = right.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Right operand not numeric".to_string())
                })?;
                Ok(SchemaValue::Bool(l <= r))
            }
            BinaryOp::Gt => {
                let l = left.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Left operand not numeric".to_string())
                })?;
                let r = right.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Right operand not numeric".to_string())
                })?;
                Ok(SchemaValue::Bool(l > r))
            }
            BinaryOp::Ge => {
                let l = left.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Left operand not numeric".to_string())
                })?;
                let r = right.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Right operand not numeric".to_string())
                })?;
                Ok(SchemaValue::Bool(l >= r))
            }

            // Logical operations
            BinaryOp::And => Ok(SchemaValue::Bool(left.is_truthy() && right.is_truthy())),
            BinaryOp::Or => Ok(SchemaValue::Bool(left.is_truthy() || right.is_truthy())),
        }
    }

    fn apply_unary_op(&self, op: UnaryOp, val: SchemaValue) -> Result<SchemaValue, SchemaError> {
        match op {
            UnaryOp::Not => Ok(SchemaValue::Bool(!val.is_truthy())),
            UnaryOp::Neg => {
                let n = val.as_f64().ok_or_else(|| {
                    SchemaError::Expression("Cannot negate non-numeric value".to_string())
                })?;
                Ok(SchemaValue::Float(-n))
            }
        }
    }

    fn apply_function(
        &self,
        name: &str,
        args: Vec<SchemaValue>,
    ) -> Result<SchemaValue, SchemaError> {
        match name {
            "min" => {
                if args.len() != 2 {
                    return Err(SchemaError::Expression(
                        "min() requires exactly 2 arguments".to_string(),
                    ));
                }
                let a = args[0].as_f64().ok_or_else(|| {
                    SchemaError::Expression("min() argument not numeric".to_string())
                })?;
                let b = args[1].as_f64().ok_or_else(|| {
                    SchemaError::Expression("min() argument not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(a.min(b)))
            }
            "max" => {
                if args.len() != 2 {
                    return Err(SchemaError::Expression(
                        "max() requires exactly 2 arguments".to_string(),
                    ));
                }
                let a = args[0].as_f64().ok_or_else(|| {
                    SchemaError::Expression("max() argument not numeric".to_string())
                })?;
                let b = args[1].as_f64().ok_or_else(|| {
                    SchemaError::Expression("max() argument not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(a.max(b)))
            }
            "abs" => {
                if args.len() != 1 {
                    return Err(SchemaError::Expression(
                        "abs() requires exactly 1 argument".to_string(),
                    ));
                }
                let a = args[0].as_f64().ok_or_else(|| {
                    SchemaError::Expression("abs() argument not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(a.abs()))
            }
            "floor" => {
                if args.len() != 1 {
                    return Err(SchemaError::Expression(
                        "floor() requires exactly 1 argument".to_string(),
                    ));
                }
                let a = args[0].as_f64().ok_or_else(|| {
                    SchemaError::Expression("floor() argument not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(a.floor()))
            }
            "ceil" => {
                if args.len() != 1 {
                    return Err(SchemaError::Expression(
                        "ceil() requires exactly 1 argument".to_string(),
                    ));
                }
                let a = args[0].as_f64().ok_or_else(|| {
                    SchemaError::Expression("ceil() argument not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(a.ceil()))
            }
            "round" => {
                if args.len() != 1 {
                    return Err(SchemaError::Expression(
                        "round() requires exactly 1 argument".to_string(),
                    ));
                }
                let a = args[0].as_f64().ok_or_else(|| {
                    SchemaError::Expression("round() argument not numeric".to_string())
                })?;
                Ok(SchemaValue::Float(a.round()))
            }
            "sum" => {
                if args.len() != 1 {
                    return Err(SchemaError::Expression(
                        "sum() requires exactly 1 argument (a list)".to_string(),
                    ));
                }
                match &args[0] {
                    SchemaValue::List(items) => {
                        let mut total = 0.0;
                        for item in items {
                            let v = item.as_f64().ok_or_else(|| {
                                SchemaError::Expression("sum() list item not numeric".to_string())
                            })?;
                            total += v;
                        }
                        Ok(SchemaValue::Float(total))
                    }
                    _ => Err(SchemaError::Expression(
                        "sum() requires a list argument".to_string(),
                    )),
                }
            }
            "count" => {
                if args.len() != 1 {
                    return Err(SchemaError::Expression(
                        "count() requires exactly 1 argument (a list)".to_string(),
                    ));
                }
                match &args[0] {
                    SchemaValue::List(items) => Ok(SchemaValue::Int(items.len() as i64)),
                    _ => Err(SchemaError::Expression(
                        "count() requires a list argument".to_string(),
                    )),
                }
            }
            _ => Err(SchemaError::Expression(format!(
                "Unknown function: {}",
                name
            ))),
        }
    }
}

/// Parse a simple expression string.
///
/// This is a simplified parser for common patterns. Full parsing support
/// can be added incrementally.
pub fn parse_expr(s: &str) -> Result<SchemaExpr, SchemaError> {
    let s = s.trim();

    // Try to parse as literal number
    if let Ok(i) = s.parse::<i64>() {
        return Ok(SchemaExpr::Literal(SchemaValue::Int(i)));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Ok(SchemaExpr::Literal(SchemaValue::Float(f)));
    }

    // Try to parse as boolean
    if s == "true" {
        return Ok(SchemaExpr::Literal(SchemaValue::Bool(true)));
    }
    if s == "false" {
        return Ok(SchemaExpr::Literal(SchemaValue::Bool(false)));
    }

    // Try to parse as fraction (e.g., "2/3") - only if no parentheses
    if !s.contains('(') {
        if let Some((num, den)) = s.split_once('/') {
            if let (Ok(n), Ok(d)) = (num.trim().parse::<i64>(), den.trim().parse::<i64>()) {
                if d != 0 {
                    return Ok(SchemaExpr::Literal(SchemaValue::Fraction {
                        numerator: n,
                        denominator: d,
                    }));
                }
            }
        }
    }

    // Try to parse as simple field reference (no operators)
    if s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return Ok(SchemaExpr::Field(s.to_string()));
    }

    // Try to parse function call BEFORE binary operators
    // (so operators inside function args don't cause early splits)
    if let Some(paren_pos) = s.find('(') {
        if s.ends_with(')') {
            let name = s[..paren_pos].trim();
            // Only treat as function if name is a valid identifier
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_')
            {
                let args_str = &s[paren_pos + 1..s.len() - 1];
                // Split on commas, but respect nested parentheses
                let args = split_args(args_str)?;
                let parsed_args: Result<Vec<_>, _> =
                    args.iter().map(|a| parse_expr(a.trim())).collect();
                return Ok(SchemaExpr::FunctionCall {
                    name: name.to_string(),
                    args: parsed_args?,
                });
            }
        }
    }

    // Try to parse simple binary expressions (e.g., "0.67 * members")
    // Only split on operators that are outside of parentheses
    if let Some((left, op, right)) = find_binary_op(s) {
        let left_expr = parse_expr(left)?;
        let right_expr = parse_expr(right)?;
        return Ok(SchemaExpr::BinaryOp {
            left: Box::new(left_expr),
            op,
            right: Box::new(right_expr),
        });
    }

    // Fallback: treat as field reference
    Ok(SchemaExpr::Field(s.to_string()))
}

/// Split function arguments, respecting nested parentheses.
fn split_args(s: &str) -> Result<Vec<String>, SchemaError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in s.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }

    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }

    Ok(args)
}

/// Find a binary operator that is outside of parentheses.
/// Returns (left, op, right) if found.
fn find_binary_op(s: &str) -> Option<(&str, BinaryOp, &str)> {
    // Check operators in order of precedence (lowest first, so they bind loosest)
    let operators = [
        " or ", " and ", ">=", "<=", "!=", "==", ">", "<", "+", "-", "*", "/",
    ];

    for op_str in operators {
        let mut depth = 0;
        let mut i = 0;
        let bytes = s.as_bytes();

        while i < s.len() {
            if bytes[i] == b'(' {
                depth += 1;
            } else if bytes[i] == b')' {
                depth -= 1;
            } else if depth == 0 && s[i..].starts_with(op_str) {
                // Found operator at top level
                let left = s[..i].trim();
                let right = s[i + op_str.len()..].trim();
                if let Some(op) = BinaryOp::parse(op_str.trim()) {
                    return Some((left, op, right));
                }
            }
            i += 1;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_literal() {
        let expr = parse_expr("42").unwrap();
        assert_eq!(expr, SchemaExpr::Literal(SchemaValue::Int(42)));

        let expr = parse_expr("3.14").unwrap();
        assert!(matches!(expr, SchemaExpr::Literal(SchemaValue::Float(_))));
    }

    #[test]
    fn test_parse_fraction() {
        let expr = parse_expr("2/3").unwrap();
        assert_eq!(
            expr,
            SchemaExpr::Literal(SchemaValue::Fraction {
                numerator: 2,
                denominator: 3
            })
        );
    }

    #[test]
    fn test_parse_field() {
        let expr = parse_expr("members").unwrap();
        assert_eq!(expr, SchemaExpr::Field("members".to_string()));
    }

    #[test]
    fn test_parse_binary_op() {
        let expr = parse_expr("0.67 * members").unwrap();
        assert!(matches!(expr, SchemaExpr::BinaryOp { .. }));
    }

    #[test]
    fn test_parse_function() {
        let expr = parse_expr("min(1000, patronage)").unwrap();
        assert!(matches!(expr, SchemaExpr::FunctionCall { .. }));
    }

    #[test]
    fn test_evaluate_literal() {
        let ctx = EvalContext::new();
        let expr = SchemaExpr::Literal(SchemaValue::Int(42));
        let result = ctx.evaluate(&expr).unwrap();
        assert_eq!(result, SchemaValue::Int(42));
    }

    #[test]
    fn test_evaluate_field() {
        let ctx = EvalContext::new().with_field("members", SchemaValue::Int(100));
        let expr = SchemaExpr::Field("members".to_string());
        let result = ctx.evaluate(&expr).unwrap();
        assert_eq!(result, SchemaValue::Int(100));
    }

    #[test]
    fn test_evaluate_binary_op() {
        let ctx = EvalContext::new().with_field("members", SchemaValue::Int(100));
        let expr = SchemaExpr::BinaryOp {
            left: Box::new(SchemaExpr::Literal(SchemaValue::Float(0.67))),
            op: BinaryOp::Mul,
            right: Box::new(SchemaExpr::Field("members".to_string())),
        };
        let result = ctx.evaluate(&expr).unwrap();
        if let SchemaValue::Float(f) = result {
            assert!((f - 67.0).abs() < 0.001);
        } else {
            panic!("Expected float");
        }
    }

    #[test]
    fn test_evaluate_min() {
        let ctx = EvalContext::new().with_field("patronage", SchemaValue::Int(800));
        let expr = parse_expr("min(1000, patronage)").unwrap();
        let result = ctx.evaluate(&expr).unwrap();
        if let SchemaValue::Float(f) = result {
            assert_eq!(f, 800.0);
        } else {
            panic!("Expected float");
        }
    }

    #[test]
    fn test_determinism() {
        let ctx = EvalContext::new().with_field("patronage", SchemaValue::Int(800));
        let expr = parse_expr("min(1000, patronage * 0.5)").unwrap();

        // Same result every time
        let r1 = ctx.evaluate(&expr).unwrap();
        let r2 = ctx.evaluate(&expr).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_depth_limit() {
        // Create deeply nested expression
        let mut expr = SchemaExpr::Literal(SchemaValue::Int(1));
        for _ in 0..50 {
            expr = SchemaExpr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            };
        }
        let ctx = EvalContext::new();
        let result = ctx.evaluate(&expr);
        assert!(result.is_err());
    }
}
