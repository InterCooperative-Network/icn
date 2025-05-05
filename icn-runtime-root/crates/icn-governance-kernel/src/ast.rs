use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root structure for a CCL document
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CclRoot {
    /// Type of the template (e.g., "coop_bylaws", "community_charter")
    pub template_type: String,
    
    /// Main content object of the CCL document
    pub content: CclValue,
}

/// Key-value pair in a CCL object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CclPair {
    /// Key for this pair (always a string in CCL)
    pub key: String,
    
    /// Value associated with the key
    pub value: CclValue,
}

/// Possible values in CCL
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CclValue {
    /// String literal
    String(String),
    
    /// Number literal (stored as f64 to handle both integers and floating point)
    Number(f64),
    
    /// Boolean value
    Boolean(bool),
    
    /// Object containing key-value pairs
    Object(Vec<CclPair>),
    
    /// Array of values
    Array(Vec<CclValue>),
    
    /// Identifier reference (variable or function name)
    Identifier(String),
    
    /// Null value
    Null,
}

impl CclValue {
    /// Convenience method to create a string value
    pub fn string<S: Into<String>>(s: S) -> Self {
        CclValue::String(s.into())
    }
    
    /// Convenience method to create a number value
    pub fn number(n: f64) -> Self {
        CclValue::Number(n)
    }
    
    /// Convenience method to create a boolean value
    pub fn boolean(b: bool) -> Self {
        CclValue::Boolean(b)
    }
    
    /// Convenience method to create an object from key-value pairs
    pub fn object(pairs: Vec<CclPair>) -> Self {
        CclValue::Object(pairs)
    }
    
    /// Convenience method to create an array of values
    pub fn array(values: Vec<CclValue>) -> Self {
        CclValue::Array(values)
    }
    
    /// Convenience method to create an identifier reference
    pub fn identifier<S: Into<String>>(s: S) -> Self {
        CclValue::Identifier(s.into())
    }
    
    /// Convenience method to create a null value
    pub fn null() -> Self {
        CclValue::Null
    }
}

/// Helper function to create a CclPair
pub fn pair<K: Into<String>, V: Into<CclValue>>(key: K, value: V) -> CclPair {
    CclPair {
        key: key.into(),
        value: value.into(),
    }
}

// Implement conversion from various types to CclValue for convenience
impl From<String> for CclValue {
    fn from(s: String) -> Self {
        CclValue::String(s)
    }
}

impl From<&str> for CclValue {
    fn from(s: &str) -> Self {
        CclValue::String(s.to_string())
    }
}

impl From<f64> for CclValue {
    fn from(n: f64) -> Self {
        CclValue::Number(n)
    }
}

impl From<i32> for CclValue {
    fn from(n: i32) -> Self {
        CclValue::Number(n as f64)
    }
}

impl From<bool> for CclValue {
    fn from(b: bool) -> Self {
        CclValue::Boolean(b)
    }
}

impl From<Vec<CclValue>> for CclValue {
    fn from(v: Vec<CclValue>) -> Self {
        CclValue::Array(v)
    }
}

impl From<Vec<CclPair>> for CclValue {
    fn from(v: Vec<CclPair>) -> Self {
        CclValue::Object(v)
    }
}

/// AST node for CCL parsing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node {
    /// A block with a name and properties
    Block {
        /// Name of the block
        name: String,
        
        /// Properties of the block
        properties: HashMap<String, Box<Node>>,
        
        /// Content of the block
        content: Vec<Box<Node>>,
    },
    
    /// A property with a name and value
    Property {
        /// Name of the property
        name: String,
        
        /// Value of the property
        value: Value,
    },
    
    /// An object with fields
    Object {
        /// Properties of the object
        properties: HashMap<String, Value>,
    },
}

impl Node {
    /// Get a property from this node
    pub fn get_property(&self, name: &str) -> Option<&Node> {
        match self {
            Node::Block { properties, .. } => properties.get(name).map(|boxed| boxed.as_ref()),
            _ => None,
        }
    }
}

/// Value type for property values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// String value
    String(String),
    
    /// Number value
    Number(String),
    
    /// Boolean value
    Boolean(bool),
    
    /// Identifier reference
    Identifier(String),
    
    /// Array of values
    Array(Vec<Box<Node>>),
    
    /// Object with properties
    Object(HashMap<String, Value>),
} 