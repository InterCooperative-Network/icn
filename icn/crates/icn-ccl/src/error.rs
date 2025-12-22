//! CCL error types with source location tracking

use thiserror::Error;

/// Source location in CCL code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// Starting line (1-indexed)
    pub line: u32,
    /// Starting column (1-indexed)
    pub col: u32,
}

impl Span {
    /// Create a new span with the given line and column
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }

    /// Create an unknown span (for errors without source location)
    pub fn unknown() -> Self {
        Self { line: 0, col: 0 }
    }

    /// Check if this span has a known location
    pub fn is_known(&self) -> bool {
        self.line > 0
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_known() {
            write!(f, "line {}", self.line)
        } else {
            write!(f, "unknown location")
        }
    }
}

/// CCL interpreter errors with typed variants and source locations
#[derive(Debug, Error)]
pub enum CclError {
    /// Type mismatch error
    #[error("Type mismatch at {span}: expected {expected}, got {actual}")]
    TypeMismatch {
        span: Span,
        expected: &'static str,
        actual: String,
    },

    /// Undefined variable reference
    #[error("Undefined variable '{name}' at {span}")]
    UndefinedVariable { span: Span, name: String },

    /// Division by zero
    #[error("Division by zero at {span}")]
    DivisionByZero { span: Span },

    /// Fuel exhausted during execution
    #[error("Fuel exhausted: used {used} units (limit: {limit})")]
    FuelExhausted { used: u64, limit: u64 },

    /// Rule not found in contract
    #[error("Rule not found: '{name}'")]
    RuleNotFound { name: String },

    /// Field not found on map/object
    #[error("Field '{field}' not found at {span}")]
    FieldNotFound { span: Span, field: String },

    /// Missing required capability
    #[error("Missing capability: {capability}")]
    MissingCapability { capability: String },

    /// Invalid binary operation
    #[error("Invalid operation at {span}: cannot apply {op} to {left_type} and {right_type}")]
    InvalidBinaryOp {
        span: Span,
        op: String,
        left_type: String,
        right_type: String,
    },

    /// Invalid unary operation
    #[error("Invalid unary operation at {span}: cannot apply {op} to {operand_type}")]
    InvalidUnaryOp {
        span: Span,
        op: String,
        operand_type: String,
    },

    /// Precondition (require) failed
    #[error("Precondition failed at {span}: require #{index}")]
    PreconditionFailed { span: Span, index: usize },

    /// Loop exceeds maximum iterations
    #[error("Loop exceeds maximum iterations at {span}: {count} > {max}")]
    LoopLimitExceeded {
        span: Span,
        count: usize,
        max: usize,
    },

    /// Invalid argument count for function
    #[error("Function '{name}' at {span}: takes {expected} argument(s), got {actual}")]
    ArgumentCount {
        span: Span,
        name: String,
        expected: usize,
        actual: usize,
    },

    /// Invalid argument type for function
    #[error("Function '{name}' at {span}: requires {expected}, got {actual}")]
    ArgumentType {
        span: Span,
        name: String,
        expected: &'static str,
        actual: String,
    },

    /// Unknown function call
    #[error("Unknown function '{name}' at {span}")]
    UnknownFunction { span: Span, name: String },

    /// Invalid collection type for iteration
    #[error("Cannot iterate over {actual} at {span}: expected list or set")]
    NotIterable { span: Span, actual: String },

    /// Invalid collection type for 'in' operator
    #[error("Cannot use 'in' with {actual} at {span}: expected list or set")]
    InvalidInOperator { span: Span, actual: String },

    /// Cannot access field on non-map value
    #[error("Cannot access field '{field}' on {actual} at {span}: expected map")]
    NotAMap {
        span: Span,
        field: String,
        actual: String,
    },
}

/// Result type for CCL operations
pub type Result<T> = std::result::Result<T, CclError>;

impl CclError {
    /// Get the span associated with this error, if any
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::TypeMismatch { span, .. }
            | Self::UndefinedVariable { span, .. }
            | Self::DivisionByZero { span }
            | Self::FieldNotFound { span, .. }
            | Self::InvalidBinaryOp { span, .. }
            | Self::InvalidUnaryOp { span, .. }
            | Self::PreconditionFailed { span, .. }
            | Self::LoopLimitExceeded { span, .. }
            | Self::ArgumentCount { span, .. }
            | Self::ArgumentType { span, .. }
            | Self::UnknownFunction { span, .. }
            | Self::NotIterable { span, .. }
            | Self::InvalidInOperator { span, .. }
            | Self::NotAMap { span, .. } => Some(*span),
            Self::FuelExhausted { .. }
            | Self::RuleNotFound { .. }
            | Self::MissingCapability { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_display() {
        let span = Span::new(10, 5);
        assert_eq!(span.to_string(), "line 10");

        let unknown = Span::unknown();
        assert_eq!(unknown.to_string(), "unknown location");
    }

    #[test]
    fn test_error_messages() {
        let err = CclError::TypeMismatch {
            span: Span::new(5, 1),
            expected: "integer",
            actual: "string".to_string(),
        };
        assert!(err.to_string().contains("line 5"));
        assert!(err.to_string().contains("integer"));
        assert!(err.to_string().contains("string"));

        let err = CclError::UndefinedVariable {
            span: Span::new(10, 1),
            name: "foo".to_string(),
        };
        assert!(err.to_string().contains("foo"));
        assert!(err.to_string().contains("line 10"));
    }

    #[test]
    fn test_span_extraction() {
        let err = CclError::DivisionByZero {
            span: Span::new(42, 1),
        };
        assert_eq!(err.span(), Some(Span::new(42, 1)));

        let err = CclError::FuelExhausted {
            used: 100,
            limit: 50,
        };
        assert_eq!(err.span(), None);
    }
}
