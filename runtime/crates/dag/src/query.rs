/*!
# DAG Query Language

Provides a simple query language for traversing and filtering DAG nodes,
enabling easier navigation and exploration of the DAG structure.
*/

use crate::DagNode;
use cid::Cid;
use serde_json::Value;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use thiserror::Error;
use futures::stream::{self, StreamExt};
use tracing::{debug, warn};

/// Error types for DAG queries
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("Invalid query syntax: {0}")]
    SyntaxError(String),
    
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    
    #[error("Field not found: {0}")]
    FieldNotFound(String),
    
    #[error("Type error: {0}")]
    TypeError(String),
    
    #[error("Execution error: {0}")]
    ExecutionError(String),
}

/// Result type for DAG queries
pub type QueryResult<T> = Result<T, QueryError>;

/// Trait for loading DAG nodes
#[async_trait]
pub trait NodeLoader: Send + Sync {
    /// Load a node by CID
    async fn load_node(&self, cid: &Cid) -> QueryResult<Option<Arc<DagNode>>>;
}

/// Query operation types
#[derive(Debug, Clone)]
pub enum QueryOp {
    /// Traverse to parent nodes
    Parents,
    
    /// Filter nodes by a condition
    Filter(FilterCondition),
    
    /// Project specific fields from node
    Project(Vec<String>),
    
    /// Limit the number of results
    Limit(usize),
    
    /// Skip a number of results
    Skip(usize),
    
    /// Follow a specific path in the payload
    Path(Vec<String>),
    
    /// Order results by a field (ascending)
    OrderAsc(String),
    
    /// Order results by a field (descending)
    OrderDesc(String),
}

/// Filter condition types
#[derive(Debug, Clone)]
pub enum FilterCondition {
    /// Field equals value
    Equals(String, Value),
    
    /// Field contains substring
    Contains(String, String),
    
    /// Field greater than value
    GreaterThan(String, Value),
    
    /// Field less than value
    LessThan(String, Value),
    
    /// Field exists
    Exists(String),
    
    /// Issuer equals value
    IssuerEquals(String),
    
    /// Before timestamp
    BeforeTimestamp(u64),
    
    /// After timestamp
    AfterTimestamp(u64),
    
    /// Logical AND of conditions
    And(Box<FilterCondition>, Box<FilterCondition>),
    
    /// Logical OR of conditions
    Or(Box<FilterCondition>, Box<FilterCondition>),
    
    /// Logical NOT of condition
    Not(Box<FilterCondition>),
}

/// DAG query for traversing and filtering nodes
pub struct DagQuery {
    /// Starting CIDs
    start_cids: Vec<Cid>,
    
    /// Operations to perform
    operations: Vec<QueryOp>,
    
    /// Maximum traversal depth
    max_depth: Option<usize>,
    
    /// Whether to include intermediate results
    include_intermediate: bool,
}

impl DagQuery {
    /// Create a new query starting from the given CIDs
    pub fn from(cids: Vec<Cid>) -> Self {
        Self {
            start_cids: cids,
            operations: Vec::new(),
            max_depth: None,
            include_intermediate: false,
        }
    }
    
    /// Add parent traversal operation
    pub fn parents(mut self) -> Self {
        self.operations.push(QueryOp::Parents);
        self
    }
    
    /// Add filter operation
    pub fn filter(mut self, condition: FilterCondition) -> Self {
        self.operations.push(QueryOp::Filter(condition));
        self
    }
    
    /// Add projection operation
    pub fn project(mut self, fields: Vec<String>) -> Self {
        self.operations.push(QueryOp::Project(fields));
        self
    }
    
    /// Add limit operation
    pub fn limit(mut self, n: usize) -> Self {
        self.operations.push(QueryOp::Limit(n));
        self
    }
    
    /// Add skip operation
    pub fn skip(mut self, n: usize) -> Self {
        self.operations.push(QueryOp::Skip(n));
        self
    }
    
    /// Add path operation
    pub fn path(mut self, path: Vec<String>) -> Self {
        self.operations.push(QueryOp::Path(path));
        self
    }
    
    /// Add ascending order operation
    pub fn order_asc(mut self, field: String) -> Self {
        self.operations.push(QueryOp::OrderAsc(field));
        self
    }
    
    /// Add descending order operation
    pub fn order_desc(mut self, field: String) -> Self {
        self.operations.push(QueryOp::OrderDesc(field));
        self
    }
    
    /// Set maximum traversal depth
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }
    
    /// Include intermediate results
    pub fn include_intermediate(mut self, include: bool) -> Self {
        self.include_intermediate = include;
        self
    }
    
    /// Execute the query
    pub async fn execute<L: NodeLoader>(self, loader: &L) -> QueryResult<Vec<Arc<DagNode>>> {
        let mut current_nodes = Vec::new();
        
        // Load starting nodes
        for cid in &self.start_cids {
            if let Some(node) = loader.load_node(cid).await? {
                current_nodes.push(node);
            }
        }
        
        // Apply operations
        for op in self.operations {
            current_nodes = match op {
                QueryOp::Parents => traverse_parents(current_nodes, loader, self.max_depth).await?,
                QueryOp::Filter(condition) => filter_nodes(current_nodes, &condition)?,
                QueryOp::Project(fields) => project_nodes(current_nodes, &fields)?,
                QueryOp::Limit(n) => limit_nodes(current_nodes, n),
                QueryOp::Skip(n) => skip_nodes(current_nodes, n),
                QueryOp::Path(path) => follow_path(current_nodes, &path)?,
                QueryOp::OrderAsc(field) => order_nodes(current_nodes, &field, true)?,
                QueryOp::OrderDesc(field) => order_nodes(current_nodes, &field, false)?,
            };
        }
        
        Ok(current_nodes)
    }
    
    /// Parse a query string into a DagQuery
    pub fn parse(query_str: &str, start_cids: Vec<Cid>) -> QueryResult<Self> {
        let mut query = Self::from(start_cids);
        
        // Simple parsing logic - can be expanded with a proper parser
        for part in query_str.split('|').map(|s| s.trim()) {
            if part.starts_with("parents") {
                query = query.parents();
            } else if part.starts_with("filter ") {
                let condition_str = &part[7..];
                let condition = parse_filter_condition(condition_str)?;
                query = query.filter(condition);
            } else if part.starts_with("project ") {
                let fields_str = &part[8..];
                let fields = fields_str.split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
                query = query.project(fields);
            } else if part.starts_with("limit ") {
                let limit_str = &part[6..];
                let limit = limit_str.parse::<usize>()
                    .map_err(|_| QueryError::SyntaxError(format!("Invalid limit: {}", limit_str)))?;
                query = query.limit(limit);
            } else if part.starts_with("skip ") {
                let skip_str = &part[5..];
                let skip = skip_str.parse::<usize>()
                    .map_err(|_| QueryError::SyntaxError(format!("Invalid skip: {}", skip_str)))?;
                query = query.skip(skip);
            } else if part.starts_with("path ") {
                let path_str = &part[5..];
                let path = path_str.split('.')
                    .map(|s| s.trim().to_string())
                    .collect();
                query = query.path(path);
            } else if part.starts_with("order_asc ") {
                let field = part[10..].trim().to_string();
                query = query.order_asc(field);
            } else if part.starts_with("order_desc ") {
                let field = part[11..].trim().to_string();
                query = query.order_desc(field);
            } else if part.starts_with("max_depth ") {
                let depth_str = &part[10..];
                let depth = depth_str.parse::<usize>()
                    .map_err(|_| QueryError::SyntaxError(format!("Invalid max_depth: {}", depth_str)))?;
                query = query.max_depth(depth);
            } else if part == "include_intermediate" {
                query = query.include_intermediate(true);
            } else {
                return Err(QueryError::SyntaxError(format!("Unknown operation: {}", part)));
            }
        }
        
        Ok(query)
    }
}

/// Helper function to parse a filter condition from a string
fn parse_filter_condition(condition_str: &str) -> QueryResult<FilterCondition> {
    // Simple condition parsing - can be expanded
    if condition_str.contains(" = ") {
        let parts: Vec<&str> = condition_str.splitn(2, " = ").collect();
        let field = parts[0].trim().to_string();
        let value = parts[1].trim();
        
        // Try to parse as number or boolean first, fall back to string
        if let Ok(num) = value.parse::<i64>() {
            return Ok(FilterCondition::Equals(field, Value::Number(num.into())));
        } else if value == "true" {
            return Ok(FilterCondition::Equals(field, Value::Bool(true)));
        } else if value == "false" {
            return Ok(FilterCondition::Equals(field, Value::Bool(false)));
        } else {
            return Ok(FilterCondition::Equals(field, Value::String(value.to_string())));
        }
    } else if condition_str.contains(" > ") {
        let parts: Vec<&str> = condition_str.splitn(2, " > ").collect();
        let field = parts[0].trim().to_string();
        let value = parts[1].trim();
        
        if let Ok(num) = value.parse::<i64>() {
            return Ok(FilterCondition::GreaterThan(field, Value::Number(num.into())));
        } else {
            return Err(QueryError::SyntaxError(format!("Non-numeric value for GreaterThan: {}", value)));
        }
    } else if condition_str.contains(" < ") {
        let parts: Vec<&str> = condition_str.splitn(2, " < ").collect();
        let field = parts[0].trim().to_string();
        let value = parts[1].trim();
        
        if let Ok(num) = value.parse::<i64>() {
            return Ok(FilterCondition::LessThan(field, Value::Number(num.into())));
        } else {
            return Err(QueryError::SyntaxError(format!("Non-numeric value for LessThan: {}", value)));
        }
    } else if condition_str.contains(" contains ") {
        let parts: Vec<&str> = condition_str.splitn(2, " contains ").collect();
        let field = parts[0].trim().to_string();
        let value = parts[1].trim().to_string();
        return Ok(FilterCondition::Contains(field, value));
    } else if condition_str.starts_with("exists ") {
        let field = condition_str[7..].trim().to_string();
        return Ok(FilterCondition::Exists(field));
    } else if condition_str.starts_with("issuer = ") {
        let issuer = condition_str[9..].trim().to_string();
        return Ok(FilterCondition::IssuerEquals(issuer));
    } else if condition_str.starts_with("timestamp > ") {
        let timestamp = condition_str[12..].trim().parse::<u64>()
            .map_err(|_| QueryError::SyntaxError(format!("Invalid timestamp: {}", &condition_str[12..])))?;
        return Ok(FilterCondition::AfterTimestamp(timestamp));
    } else if condition_str.starts_with("timestamp < ") {
        let timestamp = condition_str[12..].trim().parse::<u64>()
            .map_err(|_| QueryError::SyntaxError(format!("Invalid timestamp: {}", &condition_str[12..])))?;
        return Ok(FilterCondition::BeforeTimestamp(timestamp));
    }
    
    Err(QueryError::SyntaxError(format!("Failed to parse condition: {}", condition_str)))
}

/// Traverse parent nodes
async fn traverse_parents<L: NodeLoader>(
    nodes: Vec<Arc<DagNode>>,
    loader: &L,
    max_depth: Option<usize>,
) -> QueryResult<Vec<Arc<DagNode>>> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    
    // Add current nodes to result if we're returning intermediate results
    for node in &nodes {
        visited.insert(node.issuer.clone());
        result.push(node.clone());
    }
    
    // For each node, traverse parent links
    for node in nodes {
        let mut queue = VecDeque::new();
        let mut node_visited = HashSet::new();
        
        // Add immediate parents to queue with depth 1
        for parent_cid in &node.parents {
            queue.push_back((*parent_cid, 1));
        }
        
        while let Some((parent_cid, depth)) = queue.pop_front() {
            // Skip if already visited
            if node_visited.contains(&parent_cid) {
                continue;
            }
            
            // Check max depth
            if let Some(max) = max_depth {
                if depth > max {
                    continue;
                }
            }
            
            // Mark as visited
            node_visited.insert(parent_cid);
            
            // Load parent node
            if let Some(parent_node) = loader.load_node(&parent_cid).await? {
                result.push(parent_node.clone());
                
                // Add grandparents to queue with incremented depth
                for grandparent_cid in &parent_node.parents {
                    if !node_visited.contains(grandparent_cid) {
                        queue.push_back((*grandparent_cid, depth + 1));
                    }
                }
            }
        }
    }
    
    Ok(result)
}

/// Filter nodes by condition
fn filter_nodes(
    nodes: Vec<Arc<DagNode>>,
    condition: &FilterCondition,
) -> QueryResult<Vec<Arc<DagNode>>> {
    nodes.into_iter()
        .filter(|node| matches_condition(node, condition))
        .collect::<Vec<_>>()
        .pipe(Ok)
}

/// Check if a node matches a condition
fn matches_condition(node: &Arc<DagNode>, condition: &FilterCondition) -> bool {
    match condition {
        FilterCondition::Equals(field, value) => {
            match get_field_value(node, field) {
                Some(field_val) => field_val == *value,
                None => false,
            }
        },
        FilterCondition::Contains(field, value) => {
            match get_field_value(node, field) {
                Some(Value::String(s)) => s.contains(value),
                _ => false,
            }
        },
        FilterCondition::GreaterThan(field, value) => {
            match get_field_value(node, field) {
                Some(Value::Number(n)) => {
                    if let Some(cmp_num) = value.as_i64() {
                        n.as_i64().map(|n_val| n_val > cmp_num).unwrap_or(false)
                    } else if let Some(cmp_num) = value.as_f64() {
                        n.as_f64().map(|n_val| n_val > cmp_num).unwrap_or(false)
                    } else {
                        false
                    }
                },
                _ => false,
            }
        },
        FilterCondition::LessThan(field, value) => {
            match get_field_value(node, field) {
                Some(Value::Number(n)) => {
                    if let Some(cmp_num) = value.as_i64() {
                        n.as_i64().map(|n_val| n_val < cmp_num).unwrap_or(false)
                    } else if let Some(cmp_num) = value.as_f64() {
                        n.as_f64().map(|n_val| n_val < cmp_num).unwrap_or(false)
                    } else {
                        false
                    }
                },
                _ => false,
            }
        },
        FilterCondition::Exists(field) => {
            get_field_value(node, field).is_some()
        },
        FilterCondition::IssuerEquals(issuer) => {
            node.issuer.as_str() == issuer
        },
        FilterCondition::BeforeTimestamp(timestamp) => {
            node.metadata.timestamp < *timestamp
        },
        FilterCondition::AfterTimestamp(timestamp) => {
            node.metadata.timestamp > *timestamp
        },
        FilterCondition::And(left, right) => {
            matches_condition(node, left) && matches_condition(node, right)
        },
        FilterCondition::Or(left, right) => {
            matches_condition(node, left) || matches_condition(node, right)
        },
        FilterCondition::Not(inner) => {
            !matches_condition(node, inner)
        },
    }
}

/// Get a field value from a node
fn get_field_value(node: &Arc<DagNode>, field: &str) -> Option<Value> {
    match field {
        "issuer" => Some(Value::String(node.issuer.to_string())),
        "timestamp" => Some(Value::Number(node.metadata.timestamp.into())),
        // Extract from payload (assuming it's JSON-like)
        field_path => {
            if let libipld::Ipld::Map(map) = &node.payload {
                let parts: Vec<&str> = field_path.split('.').collect();
                let mut current = Some(&node.payload);
                
                for &part in &parts {
                    current = match current {
                        Some(libipld::Ipld::Map(m)) => m.get(part),
                        _ => None,
                    };
                    
                    if current.is_none() {
                        break;
                    }
                }
                
                // Convert IPLD to serde_json Value
                current.map(ipld_to_json)
            } else {
                None
            }
        }
    }
}

/// Convert IPLD to JSON Value
fn ipld_to_json(ipld: &libipld::Ipld) -> Value {
    match ipld {
        libipld::Ipld::Null => Value::Null,
        libipld::Ipld::Bool(b) => Value::Bool(*b),
        libipld::Ipld::Integer(i) => Value::Number((*i).into()),
        libipld::Ipld::Float(f) => {
            // Converting f64 to Value::Number can fail if the number is NaN or infinite
            if let Some(num) = serde_json::Number::from_f64(*f) {
                Value::Number(num)
            } else {
                Value::Null
            }
        },
        libipld::Ipld::String(s) => Value::String(s.clone()),
        libipld::Ipld::Bytes(b) => {
            // Convert bytes to base64 string for JSON
            let encoded = base64::encode(b);
            Value::String(encoded)
        },
        libipld::Ipld::List(list) => {
            let values: Vec<Value> = list.iter().map(ipld_to_json).collect();
            Value::Array(values)
        },
        libipld::Ipld::Map(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), ipld_to_json(v));
            }
            Value::Object(obj)
        },
        libipld::Ipld::Link(cid) => {
            // Convert CID to string for JSON
            Value::String(cid.to_string())
        },
    }
}

/// Project specific fields from nodes
fn project_nodes(
    nodes: Vec<Arc<DagNode>>,
    fields: &[String],
) -> QueryResult<Vec<Arc<DagNode>>> {
    // For now, this is a no-op as we don't modify the actual nodes
    // In a full implementation, this would create new nodes with only the selected fields
    Ok(nodes)
}

/// Limit the number of results
fn limit_nodes(
    nodes: Vec<Arc<DagNode>>,
    n: usize,
) -> Vec<Arc<DagNode>> {
    nodes.into_iter().take(n).collect()
}

/// Skip a number of results
fn skip_nodes(
    nodes: Vec<Arc<DagNode>>,
    n: usize,
) -> Vec<Arc<DagNode>> {
    nodes.into_iter().skip(n).collect()
}

/// Follow a path in the payload
fn follow_path(
    nodes: Vec<Arc<DagNode>>,
    path: &[String],
) -> QueryResult<Vec<Arc<DagNode>>> {
    // For now, this is a no-op as we don't modify the actual nodes
    // In a full implementation, this would navigate to linked nodes along the path
    Ok(nodes)
}

/// Order nodes by a field
fn order_nodes(
    nodes: Vec<Arc<DagNode>>,
    field: &str,
    ascending: bool,
) -> QueryResult<Vec<Arc<DagNode>>> {
    let mut nodes = nodes;
    
    match field {
        "timestamp" => {
            if ascending {
                nodes.sort_by_key(|node| node.metadata.timestamp);
            } else {
                nodes.sort_by_key(|node| std::cmp::Reverse(node.metadata.timestamp));
            }
        },
        "issuer" => {
            if ascending {
                nodes.sort_by(|a, b| a.issuer.as_str().cmp(b.issuer.as_str()));
            } else {
                nodes.sort_by(|a, b| b.issuer.as_str().cmp(a.issuer.as_str()));
            }
        },
        _ => {
            // Order by custom field from payload - less efficient
            nodes.sort_by(|a, b| {
                let a_val = get_field_value(a, field);
                let b_val = get_field_value(b, field);
                
                match (a_val, b_val) {
                    (Some(Value::Number(a_num)), Some(Value::Number(b_num))) => {
                        // Try to compare as numbers
                        if let (Some(a_i), Some(b_i)) = (a_num.as_i64(), b_num.as_i64()) {
                            if ascending { a_i.cmp(&b_i) } else { b_i.cmp(&a_i) }
                        } else if let (Some(a_f), Some(b_f)) = (a_num.as_f64(), b_num.as_f64()) {
                            if ascending {
                                a_f.partial_cmp(&b_f).unwrap_or(std::cmp::Ordering::Equal)
                            } else {
                                b_f.partial_cmp(&a_f).unwrap_or(std::cmp::Ordering::Equal)
                            }
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    },
                    (Some(Value::String(a_str)), Some(Value::String(b_str))) => {
                        // Compare as strings
                        if ascending { a_str.cmp(b_str) } else { b_str.cmp(a_str) }
                    },
                    (Some(Value::Bool(a_bool)), Some(Value::Bool(b_bool))) => {
                        // Compare as booleans
                        if ascending { a_bool.cmp(b_bool) } else { b_bool.cmp(a_bool) }
                    },
                    // Handle cases where one or both values are missing or not comparable
                    (Some(_), None) => if ascending { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater },
                    (None, Some(_)) => if ascending { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less },
                    _ => std::cmp::Ordering::Equal,
                }
            });
        },
    }
    
    Ok(nodes)
}

// Add trait extension method to aid chaining operations
trait Pipe<T> {
    fn pipe<U>(self, f: impl FnOnce(Self) -> U) -> U where Self: Sized;
}

impl<T> Pipe<T> for T {
    fn pipe<U>(self, f: impl FnOnce(Self) -> U) -> U {
        f(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DagNode, DagNodeMetadata};
    use std::collections::HashMap;
    
    // Mock node loader for testing
    struct MockLoader {
        nodes: HashMap<Cid, Arc<DagNode>>,
    }
    
    #[async_trait]
    impl NodeLoader for MockLoader {
        async fn load_node(&self, cid: &Cid) -> QueryResult<Option<Arc<DagNode>>> {
            Ok(self.nodes.get(cid).cloned())
        }
    }
    
    // Helper to create test nodes
    fn create_test_node(cid_str: &str, payload: libipld::Ipld, issuer: &str, parents: Vec<Cid>) -> (Cid, Arc<DagNode>) {
        let mh = create_sha256_multihash(cid_str.as_bytes());
        let cid = Cid::new_v1(0x71, mh);
        
        let node = Arc::new(DagNode {
            payload,
            parents,
            issuer: crate::IdentityId(issuer.to_string()),
            signature: vec![1, 2, 3, 4],
            metadata: DagNodeMetadata::with_timestamp(1000).with_sequence(1),
        });
        
        (cid, node)
    }
    
    #[tokio::test]
    async fn test_basic_query() {
        let mut nodes = HashMap::new();
        
        // Create a simple DAG
        // A -> B -> C
        // |
        // v
        // D
        
        let (cid_c, node_c) = create_test_node(
            "c", 
            ipld!({ "name": "Node C", "value": 300 }),
            "did:icn:user1",
            vec![]
        );
        
        let (cid_b, node_b) = create_test_node(
            "b", 
            ipld!({ "name": "Node B", "value": 200 }),
            "did:icn:user2",
            vec![cid_c]
        );
        
        let (cid_d, node_d) = create_test_node(
            "d", 
            ipld!({ "name": "Node D", "value": 400 }),
            "did:icn:user2",
            vec![]
        );
        
        let (cid_a, node_a) = create_test_node(
            "a", 
            ipld!({ "name": "Node A", "value": 100 }),
            "did:icn:user1",
            vec![cid_b, cid_d]
        );
        
        nodes.insert(cid_a, node_a.clone());
        nodes.insert(cid_b, node_b.clone());
        nodes.insert(cid_c, node_c.clone());
        nodes.insert(cid_d, node_d.clone());
        
        let loader = MockLoader { nodes };
        
        // Test: starting from A, get all parents
        let query = DagQuery::from(vec![cid_a])
            .parents()
            .max_depth(2);
            
        let results = query.execute(&loader).await.unwrap();
        assert_eq!(results.len(), 4); // A + B + C + D
        
        // Test: filter by issuer
        let query = DagQuery::from(vec![cid_a])
            .parents()
            .filter(FilterCondition::IssuerEquals("did:icn:user2".to_string()))
            .max_depth(2);
            
        let results = query.execute(&loader).await.unwrap();
        assert_eq!(results.len(), 2); // B + D
        
        // Test: filter by value
        let query = DagQuery::from(vec![cid_a])
            .parents()
            .filter(FilterCondition::GreaterThan("value".to_string(), Value::Number(200.into())))
            .max_depth(2);
            
        let results = query.execute(&loader).await.unwrap();
        assert_eq!(results.len(), 2); // C + D (value > 200)
    }
    
    #[tokio::test]
    async fn test_query_parser() {
        let node_cid = {
            let mh = create_sha256_multihash(b"test");
            Cid::new_v1(0x71, mh)
        };
        
        // Test basic query parsing
        let query_str = "parents | filter value > 100 | limit 10";
        let query = DagQuery::parse(query_str, vec![node_cid]).unwrap();
        
        assert_eq!(query.start_cids.len(), 1);
        assert_eq!(query.operations.len(), 3);
        
        match &query.operations[0] {
            QueryOp::Parents => {},
            _ => panic!("First operation should be Parents"),
        }
        
        match &query.operations[1] {
            QueryOp::Filter(FilterCondition::GreaterThan(field, _)) => {
                assert_eq!(field, "value");
            },
            _ => panic!("Second operation should be Filter"),
        }
        
        match &query.operations[2] {
            QueryOp::Limit(n) => {
                assert_eq!(*n, 10);
            },
            _ => panic!("Third operation should be Limit"),
        }
    }
} 