use jsonschema::{JSONSchema, ValidationError};
use jsonschema::error::ValidationErrorKind;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::CompilerError;

/// Schema manager for validating DSL inputs against JSON schemas
pub struct SchemaManager {
    /// Base directory for schema files
    schema_dir: PathBuf,
    
    /// Map of action types to schema files
    action_schemas: HashMap<String, String>,
    
    /// Map of template types to schema files
    template_schemas: HashMap<String, String>,
    
    /// Cache of compiled schemas
    schema_cache: HashMap<String, Arc<JSONSchema>>,
}

impl SchemaManager {
    /// Create a new schema manager with default schema directory
    pub fn new() -> Self {
        // Default to examples/schemas directory relative to the current directory
        let schema_dir = PathBuf::from("examples/schemas");
        Self::with_schema_dir(schema_dir)
    }
    
    /// Create a new schema manager with a specific schema directory
    pub fn with_schema_dir<P: AsRef<Path>>(schema_dir: P) -> Self {
        let mut manager = Self {
            schema_dir: schema_dir.as_ref().to_path_buf(),
            action_schemas: HashMap::new(),
            template_schemas: HashMap::new(),
            schema_cache: HashMap::new(),
        };
        
        // Register default schemas
        manager.register_default_schemas();
        
        manager
    }
    
    /// Register default schemas for common actions and templates
    fn register_default_schemas(&mut self) {
        // Register action schemas
        self.register_action_schema("propose_membership", "propose_join.schema.json");
        self.register_action_schema("propose_budget", "submit_budget.schema.json");
        
        // Register template schemas
        self.register_template_schema("coop_bylaws", "coop_bylaws.schema.json");
        self.register_template_schema("community_charter", "community_charter.schema.json");
    }
    
    /// Register a schema file for a specific action type
    pub fn register_action_schema(&mut self, action: &str, schema_file: &str) {
        self.action_schemas.insert(action.to_string(), schema_file.to_string());
    }
    
    /// Register a schema file for a specific template type
    pub fn register_template_schema(&mut self, template: &str, schema_file: &str) {
        self.template_schemas.insert(template.to_string(), schema_file.to_string());
    }
    
    /// Get the schema file path for a specific action
    fn get_schema_path_for_action(&self, action: &str) -> Option<PathBuf> {
        self.action_schemas.get(action).map(|schema_file| {
            self.schema_dir.join(schema_file)
        })
    }
    
    /// Get the schema file path for a specific template type
    fn get_schema_path_for_template(&self, template: &str) -> Option<PathBuf> {
        self.template_schemas.get(template).map(|schema_file| {
            self.schema_dir.join(schema_file)
        })
    }
    
    /// Load and compile a JSON schema from a file
    fn load_schema(&mut self, schema_path: &Path) -> Result<Arc<JSONSchema>, CompilerError> {
        // Check if we already have this schema cached
        let path_str = schema_path.to_string_lossy().to_string();
        if let Some(schema) = self.schema_cache.get(&path_str) {
            return Ok(schema.clone());
        }
        
        // Load the schema file
        let schema_content = fs::read_to_string(schema_path)
            .map_err(|e| CompilerError::ValidationError(format!(
                "Failed to read schema file {}: {}", schema_path.display(), e
            )))?;
        
        // Parse the schema
        let schema_value: JsonValue = serde_json::from_str(&schema_content)
            .map_err(|e| CompilerError::ValidationError(format!(
                "Failed to parse schema file {}: {}", schema_path.display(), e
            )))?;
        
        // Compile the schema
        let schema = JSONSchema::compile(&schema_value)
            .map_err(|e| CompilerError::ValidationError(format!(
                "Failed to compile schema from {}: {}", schema_path.display(), e
            )))?;
        
        // Cache the compiled schema
        let schema_arc = Arc::new(schema);
        self.schema_cache.insert(path_str, schema_arc.clone());
        
        Ok(schema_arc)
    }
    
    /// Validate a DSL input against a schema for a specific action
    pub fn validate_dsl_for_action(&mut self, action: &str, dsl_input: &JsonValue) -> Result<(), CompilerError> {
        // Get the schema path for this action
        let schema_path = self.get_schema_path_for_action(action)
            .ok_or_else(|| CompilerError::ValidationError(format!(
                "No schema registered for action '{}'", action
            )))?;
        
        // Validate if the schema file exists
        if !schema_path.exists() {
            return Err(CompilerError::ValidationError(format!(
                "Schema file not found: {}", schema_path.display()
            )));
        }
        
        // Load and compile the schema
        let schema = self.load_schema(&schema_path)?;
        
        // Validate the DSL input against the schema
        let validation_result = schema.validate(dsl_input);
        if let Err(errors) = validation_result {
            // Format validation errors
            let error_messages: Vec<String> = errors
                .into_iter()
                .map(|err| format_validation_error(&err, dsl_input))
                .collect();
            
            return Err(CompilerError::ValidationError(format!(
                "DSL validation failed for action '{}': {}",
                action,
                error_messages.join("; ")
            )));
        }
        
        Ok(())
    }
    
    /// Validate a DSL input against a schema for a specific template
    pub fn validate_dsl_for_template(&mut self, template: &str, dsl_input: &JsonValue) -> Result<(), CompilerError> {
        // Extract the action from the DSL input
        let action = extract_action(dsl_input)
            .ok_or_else(|| CompilerError::ValidationError(
                "DSL input is missing required 'action' field".to_string()
            ))?;
        
        // Try to validate by action first (more specific)
        match self.validate_dsl_for_action(&action, dsl_input) {
            Ok(()) => return Ok(()),
            Err(CompilerError::ValidationError(msg)) if msg.contains("No schema registered") || msg.contains("Schema file not found") => {
                // Fall back to template validation if no action schema is found
            }
            Err(e) => return Err(e),
        }
        
        // Get the schema path for this template
        let schema_path = self.get_schema_path_for_template(template)
            .ok_or_else(|| CompilerError::ValidationError(format!(
                "No schema registered for template '{}' or action '{}'", template, action
            )))?;
        
        // Validate if the schema file exists
        if !schema_path.exists() {
            return Err(CompilerError::ValidationError(format!(
                "Schema file not found: {}", schema_path.display()
            )));
        }
        
        // Load and compile the schema
        let schema = self.load_schema(&schema_path)?;
        
        // Validate the DSL input against the schema
        let validation_result = schema.validate(dsl_input);
        if let Err(errors) = validation_result {
            // Format validation errors
            let error_messages: Vec<String> = errors
                .into_iter()
                .map(|err| format_validation_error(&err, dsl_input))
                .collect();
            
            return Err(CompilerError::ValidationError(format!(
                "DSL validation failed for template '{}': {}",
                template,
                error_messages.join("; ")
            )));
        }
        
        Ok(())
    }
}

/// Helper function to extract the action from a DSL input
fn extract_action(dsl_input: &JsonValue) -> Option<String> {
    dsl_input.get("action")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Format a validation error in a user-friendly way
pub fn format_validation_error(err: &ValidationError, instance: &JsonValue) -> String {
    let path = err.instance_path.to_string();
    let path_display = if path.is_empty() { "root" } else { &path };
    
    // Extract the property name from the path
    let property = path.split('/')
        .last()
        .unwrap_or("unknown");
    
    // Format based on error type
    match &err.kind {
        ValidationErrorKind::Required { property } => {
            format!("Missing required property: '{}'", property)
        }
        ValidationErrorKind::Type { .. } => {
            let value = instance.pointer(err.instance_path.to_string().as_str());
            format!("Invalid type for '{}': expected {}, got {}",
                    path_display, err.schema_path, value.map_or("null".to_string(), |v| format!("{:?}", v)))
        }
        ValidationErrorKind::Enum { .. } => {
            let value = instance.pointer(err.instance_path.to_string().as_str());
            format!("Invalid value for '{}': must be one of the allowed values",
                    path_display)
        }
        ValidationErrorKind::MinLength { limit, .. } => {
            format!("'{}' is too short: minimum length is {}", path_display, limit)
        }
        ValidationErrorKind::MaxLength { limit, .. } => {
            format!("'{}' is too long: maximum length is {}", path_display, limit)
        }
        ValidationErrorKind::Pattern { .. } => {
            format!("'{}' does not match the required pattern", path_display)
        }
        _ => {
            format!("Validation error at '{}': {}", path_display, err.to_string())
        }
    }
} 