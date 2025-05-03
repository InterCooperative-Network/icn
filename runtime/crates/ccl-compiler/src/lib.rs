/*!
# CCL to WASM Compiler

This crate implements the compiler that transforms Constitutional Cooperative Language (CCL)
configurations and DSL inputs into executable WASM modules. It serves as the bridge
between the declarative governance rules and their executable representation.

## Architecture
- Takes a CCL config (e.g., bylaws, charter) and DSL input parameters
- Validates the inputs against each other
- Generates WASM bytecode that encapsulates the logic defined in the CCL
- The compiled WASM can be executed in the ICN Runtime
*/

use icn_governance_kernel::config::GovernanceConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use wasm_encoder::{
    CodeSection, EntityType, ExportSection, FunctionSection, ImportSection, Module, TypeSection,
    ValType,
};

// Re-export related types
pub use icn_governance_kernel::config;

// Schema validation
mod schema;
pub use schema::SchemaManager;

// Integration tests
#[cfg(test)]
mod tests;

/// Metadata information to embed in the compiled WASM module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataInfo {
    /// Template type (e.g., coop_bylaws, community_charter)
    pub template_type: String,
    
    /// Template version
    pub template_version: String,
    
    /// The action being performed
    pub action: String,
    
    /// The DID of the caller who initiated the action
    pub caller_did: Option<String>,
    
    /// Timestamp of compilation
    pub compilation_timestamp: i64,
    
    /// Execution ID (if known at compile time)
    pub execution_id: Option<String>,
    
    /// Additional metadata fields
    pub additional_data: HashMap<String, String>,
}

/// Errors that can occur during CCL compilation
#[derive(Debug, Error)]
pub enum CompilerError {
    /// Error during CCL validation
    #[error("CCL validation error: {0}")]
    ValidationError(String),

    /// Error during DSL parsing
    #[error("DSL parsing error: {0}")]
    DslError(String),

    /// Error during WASM generation
    #[error("WASM generation error: {0}")]
    WasmGenerationError(String),

    /// Error during template processing
    #[error("Template processing error: {0}")]
    TemplateError(String),

    /// Schema validation error
    #[error("Schema validation error: {0}")]
    SchemaError(String),

    /// General compilation error
    #[error("Compilation error: {0}")]
    General(String),
}

impl From<serde_json::Error> for CompilerError {
    fn from(error: serde_json::Error) -> Self {
        CompilerError::DslError(error.to_string())
    }
}

impl From<anyhow::Error> for CompilerError {
    fn from(error: anyhow::Error) -> Self {
        CompilerError::General(error.to_string())
    }
}

/// Result type for compiler operations
pub type CompilerResult<T> = Result<T, CompilerError>;

/// Compilation options that control how the WASM is generated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationOptions {
    /// Whether to include debug information
    pub include_debug_info: bool,
    
    /// Whether to optimize the generated WASM
    pub optimize: bool,
    
    /// Memory limits in pages (64KB per page)
    pub memory_limits: Option<MemoryLimits>,
    
    /// Additional metadata to include in the WASM module
    pub additional_metadata: Option<HashMap<String, String>>,
    
    /// Caller DID (if known at compile time)
    pub caller_did: Option<String>,
    
    /// Execution ID (if known at compile time)
    pub execution_id: Option<String>,
    
    /// Path to the schema file to use for validation (if different from default)
    pub schema_path: Option<PathBuf>,
    
    /// Whether to validate DSL input against schema
    pub validate_schema: bool,
}

impl Default for CompilationOptions {
    fn default() -> Self {
        Self {
            include_debug_info: false,
            optimize: true,
            memory_limits: Some(MemoryLimits::default()),
            additional_metadata: None,
            caller_did: None,
            execution_id: None,
            schema_path: None,
            validate_schema: true,
        }
    }
}

/// Memory limits for the generated WASM module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLimits {
    /// Minimum memory in pages (64KB per page)
    pub min_pages: u32,
    
    /// Maximum memory in pages (64KB per page, None means no maximum)
    pub max_pages: Option<u32>,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            min_pages: 1,   // 64KB minimum
            max_pages: Some(16), // 1MB maximum
        }
    }
}

/// Main compiler interface
#[derive(Default)]
pub struct CclCompiler {
    /// Schema manager for validating DSL inputs
    schema_manager: Option<SchemaManager>,
}

impl CclCompiler {
    /// Create a new compiler
    pub fn new() -> Self {
        Self {
            schema_manager: Some(SchemaManager::new()),
        }
    }
    
    /// Create a new compiler with a specific schema directory
    pub fn with_schema_dir<P: AsRef<Path>>(schema_dir: P) -> Self {
        Self {
            schema_manager: Some(SchemaManager::with_schema_dir(schema_dir)),
        }
    }

    /// Compile a CCL configuration and DSL input into a WASM module
    ///
    /// # Arguments
    /// * `ccl_config` - The parsed and validated CCL governance configuration
    /// * `dsl_input` - The DSL input parameters as JSON
    /// * `options` - Compilation options
    ///
    /// # Returns
    /// The compiled WASM module as a byte vector
    pub fn compile_to_wasm(
        &mut self,
        ccl_config: &GovernanceConfig,
        dsl_input: &JsonValue,
        options: Option<CompilationOptions>,
    ) -> CompilerResult<Vec<u8>> {
        // Use default options if none provided
        let options = options.unwrap_or_default();
        
        // Extract template type and action
        let template_type = &ccl_config.template_type;
        let action = match self.extract_action_from_dsl(dsl_input) {
            Ok(action) => action,
            Err(_) => {
                // If we can't extract action, we'll do basic validation
                self.validate_dsl_for_template(ccl_config, dsl_input, !options.validate_schema)?;
                // Default action for metadata
                "unknown".to_string()
            }
        };

        // Validate the DSL input against JSON schema if enabled
        if options.validate_schema {
            self.validate_against_schema(template_type, &action, dsl_input, options.schema_path.as_deref())?;
        } else {
            // Still do basic structural validation
            self.validate_dsl_for_template(ccl_config, dsl_input, true)?;
        }

        // Generate WASM using the appropriate backend
        let wasm_bytes = self.generate_wasm_module(ccl_config, dsl_input, &options)?;

        Ok(wasm_bytes)
    }
    
    /// Validate DSL input against a JSON schema
    fn validate_against_schema(
        &mut self, 
        template_type: &str, 
        action: &str, 
        dsl_input: &JsonValue,
        custom_schema_path: Option<&Path>
    ) -> CompilerResult<()> {
        // If we have a custom schema path, load and validate directly
        if let Some(schema_path) = custom_schema_path {
            if let Some(_schema_manager) = &mut self.schema_manager {
                // Load and compile the schema
                let schema_content = std::fs::read_to_string(schema_path)
                    .map_err(|e| CompilerError::SchemaError(format!(
                        "Failed to read schema file {}: {}", schema_path.display(), e
                    )))?;
                
                // Parse the schema
                let schema_value: JsonValue = serde_json::from_str(&schema_content)
                    .map_err(|e| CompilerError::SchemaError(format!(
                        "Failed to parse schema file {}: {}", schema_path.display(), e
                    )))?;
                
                // Compile the schema
                let schema = jsonschema::JSONSchema::compile(&schema_value)
                    .map_err(|e| CompilerError::SchemaError(format!(
                        "Failed to compile schema from {}: {}", schema_path.display(), e
                    )))?;
                
                // Validate the DSL input
                let validation_result = schema.validate(dsl_input);
                if let Err(errors) = validation_result {
                    // Format validation errors
                    let error_messages: Vec<String> = errors
                        .into_iter()
                        .map(|err| schema::format_validation_error(&err, dsl_input))
                        .collect();
                    
                    return Err(CompilerError::SchemaError(format!(
                        "DSL validation failed: {}",
                        error_messages.join("; ")
                    )));
                }
                
                return Ok(());
            }
        }
        
        // Otherwise use the schema manager
        if let Some(schema_manager) = &mut self.schema_manager {
            match schema_manager.validate_dsl_for_action(action, dsl_input) {
                Ok(()) => return Ok(()),
                Err(CompilerError::ValidationError(msg)) if msg.contains("No schema registered") || msg.contains("Schema file not found") => {
                    // Fall back to template validation
                    schema_manager.validate_dsl_for_template(template_type, dsl_input)
                        .map_err(|e| CompilerError::SchemaError(e.to_string()))?;
                }
                Err(e) => return Err(CompilerError::SchemaError(e.to_string())),
            }
        } else {
            // Fall back to basic validation if schema manager is not available
            tracing::warn!("Schema manager not available, falling back to basic validation");
        }
        
        Ok(())
    }

    /// Validate that the DSL input is compatible with the CCL template
    fn validate_dsl_for_template(
        &self,
        ccl_config: &GovernanceConfig,
        dsl_input: &JsonValue,
        skip_strict_validation: bool,
    ) -> CompilerResult<()> {
        // Extract template type and version
        let template_type = &ccl_config.template_type;
        let template_version = &ccl_config.template_version;

        // Check that the DSL input has the required fields for this template type
        match template_type.as_str() {
            "coop_bylaws" => {
                // For cooperative bylaws, we expect specific fields in the DSL input
                if !dsl_input.is_object() {
                    return Err(CompilerError::DslError(
                        "DSL input must be a JSON object".to_string(),
                    ));
                }

                let dsl_obj = dsl_input.as_object().unwrap();

                // Check for required fields based on template type
                if !dsl_obj.contains_key("action") {
                    return Err(CompilerError::DslError(
                        "DSL input for cooperative bylaws must contain 'action' field".to_string(),
                    ));
                }

                // Skip strict action validation when requested
                if skip_strict_validation {
                    return Ok(());
                }

                // Specific checks based on action type
                let action = dsl_obj.get("action").unwrap().as_str().unwrap_or("");
                match action {
                    "propose_membership" => {
                        if !dsl_obj.contains_key("applicant_did") {
                            return Err(CompilerError::DslError(
                                "Membership proposal requires 'applicant_did' field".to_string(),
                            ));
                        }
                    }
                    "propose_budget" => {
                        if !dsl_obj.contains_key("amount") || !dsl_obj.contains_key("category") {
                            return Err(CompilerError::DslError(
                                "Budget proposal requires 'amount' and 'category' fields"
                                    .to_string(),
                            ));
                        }
                    }
                    "log_caller_info" => {
                        // This action doesn't require additional fields
                    }
                    "perform_metered_action" => {
                        if !dsl_obj.contains_key("resource_type") {
                            return Err(CompilerError::DslError(
                                "perform_metered_action requires 'resource_type' field".to_string(),
                            ));
                        }
                        if !dsl_obj.contains_key("amount") {
                            return Err(CompilerError::DslError(
                                "perform_metered_action requires 'amount' field".to_string(),
                            ));
                        }
                    }
                    "anchor_data" => {
                        if !dsl_obj.contains_key("content") {
                            return Err(CompilerError::DslError(
                                "anchor_data requires 'content' field".to_string(),
                            ));
                        }
                        // parents is optional, so no validation needed
                    }
                    // Add more action-specific validations as needed
                    _ => {
                        return Err(CompilerError::DslError(format!(
                            "Unknown action '{}' for cooperative bylaws",
                            action
                        )));
                    }
                }
            }
            "community_charter" => {
                // Similar validations for community charter
                if !dsl_input.is_object() {
                    return Err(CompilerError::DslError(
                        "DSL input must be a JSON object".to_string(),
                    ));
                }

                let dsl_obj = dsl_input.as_object().unwrap();

                // Check for required fields based on template type
                if !dsl_obj.contains_key("action") {
                    return Err(CompilerError::DslError(
                        "DSL input for community charter must contain 'action' field".to_string(),
                    ));
                }

                // Skip strict action validation when requested
                if skip_strict_validation {
                    return Ok(());
                }

                // Specific checks based on action type
                let action = dsl_obj.get("action").unwrap().as_str().unwrap_or("");
                match action {
                    "log_caller_info" => {
                        // This action doesn't require additional fields
                    }
                    _ => {
                        return Err(CompilerError::DslError(format!(
                            "Unknown action '{}' for community charter",
                            action
                        )));
                    }
                }
            }
            "budget_proposal" => {
                // Validations for budget proposals
                if !dsl_input.is_object() {
                    return Err(CompilerError::DslError(
                        "DSL input must be a JSON object".to_string(),
                    ));
                }

                let dsl_obj = dsl_input.as_object().unwrap();

                // Check for required fields
                if !dsl_obj.contains_key("amount") || !dsl_obj.contains_key("purpose") {
                    return Err(CompilerError::DslError(
                        "Budget proposal requires 'amount' and 'purpose' fields".to_string(),
                    ));
                }
            }
            // Add more template type validations as needed
            _ => {
                if !skip_strict_validation {
                    return Err(CompilerError::ValidationError(format!(
                        "Unsupported template type: {}:{}",
                        template_type, template_version
                    )));
                }
            }
        }

        // If all validations pass, return Ok
        Ok(())
    }

    /// Extract action from DSL input
    fn extract_action_from_dsl(&self, dsl_input: &JsonValue) -> CompilerResult<String> {
        if let Some(action) = dsl_input.get("action") {
            if let Some(action_str) = action.as_str() {
                return Ok(action_str.to_string());
            }
        }
        
        Err(CompilerError::DslError("Cannot extract action from DSL input".to_string()))
    }
    
    /// Create metadata information for the WASM module
    fn create_metadata(
        &self,
        ccl_config: &GovernanceConfig,
        dsl_input: &JsonValue,
        options: &CompilationOptions,
    ) -> CompilerResult<MetadataInfo> {
        // Extract the action from DSL input
        let action = self.extract_action_from_dsl(dsl_input).unwrap_or_else(|_| "unknown".to_string());
        
        // Get current timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| CompilerError::General(format!("Failed to get system time: {}", e)))?
            .as_secs() as i64;
        
        // Create additional data hashmap
        let mut additional_data = options.additional_metadata.clone().unwrap_or_default();
        
        // Add some info from DSL to additional data
        if let Some(obj) = dsl_input.as_object() {
            for (key, value) in obj {
                // Only add string values to metadata
                if let Some(value_str) = value.as_str() {
                    if key != "action" {  // Skip action since it's already included
                        additional_data.insert(format!("dsl_{}", key), value_str.to_string());
                    }
                }
            }
        }
        
        // Create metadata
        let metadata = MetadataInfo {
            template_type: ccl_config.template_type.clone(),
            template_version: ccl_config.template_version.clone(),
            action,
            caller_did: options.caller_did.clone(),
            compilation_timestamp: timestamp,
            execution_id: options.execution_id.clone(),
            additional_data,
        };
        
        Ok(metadata)
    }

    /// Generate a WASM module with embedded CCL config and DSL input
    fn generate_wasm_module(
        &self,
        ccl_config: &GovernanceConfig,
        dsl_input: &JsonValue,
        options: &CompilationOptions,
    ) -> CompilerResult<Vec<u8>> {
        use std::borrow::Cow;
        
        // Step 1: Extract key information from inputs
        let template_type = &ccl_config.template_type;
        let template_version = &ccl_config.template_version;
        
        // Extract action from DSL input (with fallback)
        let action = self.extract_action_from_dsl(dsl_input)
            .unwrap_or_else(|_| "unknown".to_string());
        
        // Create metadata
        let metadata = self.create_metadata(ccl_config, dsl_input, options)?;
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| CompilerError::General(format!("Failed to serialize metadata: {}", e)))?;
        
        // Extract key and value for store_data action or key for get_data action
        let mut key_cid_str = String::new();
        let mut value_bytes = Vec::new();
        
        // Extract key_cid for both store_data and get_data actions
        if action == "store_data" || action == "get_data" {
            if let Some(key) = dsl_input.get("key_cid") {
                if let Some(key_str) = key.as_str() {
                    key_cid_str = key_str.to_string();
                } else {
                    return Err(CompilerError::DslError("key_cid must be a string".to_string()));
                }
            } else {
                return Err(CompilerError::DslError(format!("{} action requires key_cid field", action)));
            }
        }
        
        // Extract value only for store_data action
        if action == "store_data" {
            if let Some(value) = dsl_input.get("value") {
                // Handle different value types
                if let Some(value_str) = value.as_str() {
                    value_bytes = value_str.as_bytes().to_vec();
                } else {
                    // For non-string values, serialize to JSON
                    value_bytes = serde_json::to_vec(value)
                        .map_err(|e| CompilerError::DslError(format!("Failed to serialize value: {}", e)))?;
                }
            } else {
                return Err(CompilerError::DslError("store_data action requires value field".to_string()));
            }
        }
        
        // Step 2: Create a new WASM module
        let mut module = Module::new();
        
        // Step 3: Define type section - function signatures
        let mut types = TypeSection::new();
        
        // Type 0: () -> () for _start function
        types.function(vec![], vec![]);
        
        // Type 1: (i32, i32, i32) -> () for host_log_message function
        // Parameters: (log_level, message_ptr, message_len)
        types.function(vec![ValType::I32, ValType::I32, ValType::I32], vec![]);
        
        // Type 2: (i32, i32) -> i32 for invoke function
        // Parameters: (params_ptr, params_len), Returns: status code
        types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 3: (i32, i32, i32, i32) -> i32 for host_storage_get function
        // Parameters: (key_ptr, key_len, value_ptr, value_len), Returns: result code
        types.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 4: (i32, i32, i32, i32) -> i32 for host_storage_put function
        // Parameters: (key_ptr, key_len, value_ptr, value_len), Returns: result code
        types.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 5: (i32, i32) -> i32 for host_get_caller_did function
        // Parameters: (output_ptr, max_len), Returns: actual string length
        types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 6: () -> i32 for host_get_caller_scope function
        // No parameters, Returns: scope as integer
        types.function(vec![], vec![ValType::I32]);

        // Type 7: (i32, i64) -> i32 for host_check_resource_authorization function
        // Parameters: (resource_type, amount), Returns: authorization status
        types.function(vec![ValType::I32, ValType::I64], vec![ValType::I32]);
        
        // Type 8: (i32, i64) -> () for host_record_resource_usage function
        // Parameters: (resource_type, amount), Returns: nothing
        types.function(vec![ValType::I32, ValType::I64], vec![]);
        
        // Type 9: (i32, i32, i32, i32, i32, i32) -> i32 for host_anchor_to_dag function
        // Parameters: (content_ptr, content_len, parents_ptr, parents_count, result_ptr, result_capacity), Returns: result length or error code
        types.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 10: (i32, i32, i32, i32, i32, i32, i32, i32) -> i32 for host_create_sub_dag function
        // Parameters: (parent_did_ptr, parent_did_len, genesis_payload_ptr, genesis_payload_len, entity_type_ptr, entity_type_len, did_out_ptr, did_out_max_len),
        // Returns: length of result DID or error code
        types.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 11: (i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 for host_store_node function
        // Parameters: (entity_did_ptr, entity_did_len, payload_ptr, payload_len, parents_ptr_ptr, parents_count, parents_lens_ptr, 
        //              signature_ptr, signature_len, metadata_ptr, metadata_len, cid_out_ptr, cid_out_max_len)
        // Returns: length of CID bytes or error code
        types.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, 
                            ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 12: (i32, i32, i32, i32, i32, i32) -> i32 for host_get_node function
        // Parameters: (entity_did_ptr, entity_did_len, cid_ptr, cid_len, node_out_ptr, node_out_max_len)
        // Returns: length of node bytes or error code
        types.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 13: (i32, i32, i32, i32) -> i32 for host_contains_node function
        // Parameters: (entity_did_ptr, entity_did_len, cid_ptr, cid_len)
        // Returns: 1 if node exists, 0 if it doesn't, or negative error code
        types.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
        
        module.section(&types);
        
        // Step 4: Define import section - host functions
        let mut imports = ImportSection::new();
        
        // Import host_log_message from env module
        imports.import("env", "host_log_message", EntityType::Function(1));
        
        // Import storage functions from env module
        imports.import("env", "host_storage_get", EntityType::Function(3));
        imports.import("env", "host_storage_put", EntityType::Function(4));
        
        // Import identity functions for log_caller_info action
        if action == "log_caller_info" {
            imports.import("env", "host_get_caller_did", EntityType::Function(5));
            imports.import("env", "host_get_caller_scope", EntityType::Function(6));
        }
        
        // Import economic functions for perform_metered_action
        if action == "perform_metered_action" {
            imports.import("env", "host_check_resource_authorization", EntityType::Function(7));
            imports.import("env", "host_record_resource_usage", EntityType::Function(8));
        }
        
        // Import DAG functions for anchor_data action
        if action == "anchor_data" {
            imports.import("env", "host_anchor_to_dag", EntityType::Function(9));
        }
        
        // Import host functions for entity creation
        if action == "create_cooperative" || action == "create_community" {
            imports.import("env", "host_create_sub_dag", EntityType::Function(10));
        }
        
        // Import host functions for DAG node management
        if action == "store_dag_node" {
            imports.import("env", "host_store_node", EntityType::Function(11));
        }
        
        if action == "get_dag_node" {
            imports.import("env", "host_get_node", EntityType::Function(12));
            imports.import("env", "host_contains_node", EntityType::Function(13));
        }
        
        module.section(&imports);
        
        // Step 5: Define function section - internal functions
        let mut functions = FunctionSection::new();
        
        // _start function with type 0
        functions.function(0);
        
        // invoke function with type 2
        functions.function(2);
        
        module.section(&functions);
        
        // Step 6: Define memory section if needed (using default for now)
        let default_mem_limits = MemoryLimits::default();
        let memory_limits = options.memory_limits.as_ref().unwrap_or(&default_mem_limits);
        let mut memory = wasm_encoder::MemorySection::new();
        memory.memory(wasm_encoder::MemoryType {
            minimum: memory_limits.min_pages as u64,
            maximum: memory_limits.max_pages.map(|pages| pages as u64),
            memory64: false,
            shared: false,
        });
        module.section(&memory);
        
        // Step 7: Define export section
        let mut exports = ExportSection::new();
        
        // Export memory
        exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
        
        // Export _start function
        exports.export("_start", wasm_encoder::ExportKind::Func, 2); // Index 2 including imported functions
        
        // Export invoke function
        exports.export("invoke", wasm_encoder::ExportKind::Func, 3); // Index 3 including imported functions
        
        module.section(&exports);
        
        // Step 8: Define data section for static strings and embedded data
        let mut data_section = wasm_encoder::DataSection::new();
        
        // Create template info string for logging
        let template_info = format!(
            "CCL template: {}:{} - Action: {}",
            template_type, template_version, action
        );
        let template_info_bytes = template_info.as_bytes();
        
        // Add template info to data section at offset 1024
        data_section.active(
            0, // Memory index
            &wasm_encoder::ConstExpr::i32_const(1024), // Offset in memory
            template_info_bytes.iter().copied(), // Data bytes - copied to ensure we have actual u8 values
        );
        
        // Define additional message strings for get_data action
        let data_found_msg = "Data found for key";
        let data_not_found_msg = "Data not found for key";

        // Add message strings to data section
        data_section.active(
            0, // Memory index
            &wasm_encoder::ConstExpr::i32_const(1536), // Offset for data_found_msg
            data_found_msg.as_bytes().iter().copied(),
        );
        
        data_section.active(
            0, // Memory index
            &wasm_encoder::ConstExpr::i32_const(1600), // Offset for data_not_found_msg
            data_not_found_msg.as_bytes().iter().copied(),
        );
        
        // Define message strings for log_caller_info action
        if action == "log_caller_info" {
            let caller_did_prefix = "Caller DID: ";
            let caller_scope_prefix = "Caller Scope: ";
            
            // Add message strings to data section
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(1700), // Offset for caller_did_prefix
                caller_did_prefix.as_bytes().iter().copied(),
            );
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(1750), // Offset for caller_scope_prefix
                caller_scope_prefix.as_bytes().iter().copied(),
            );
            
            // Reserve space for scope digit (just one byte at 1799)
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(1799), // Offset for scope digit
                [0].iter().copied(), // Single byte placeholder
            );
            
            // Reserve space for caller DID (100 bytes at 9000)
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(9000), // Offset for caller DID buffer
                vec![0; 100].iter().copied(), // 100 bytes of zeros
            );
        }
        
        // Add message strings for perform_metered_action action
        if action == "perform_metered_action" {
            let checking_resource_msg = "Checking resource:";
            let authorized_msg = "Authorized";
            let not_authorized_msg = "NOT Authorized";
            let recording_usage_msg = "Recording usage:";
            
            // Add message strings to data section
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(2000), // Offset for checking_resource_msg
                checking_resource_msg.as_bytes().iter().copied(),
            );
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(2050), // Offset for authorized_msg
                authorized_msg.as_bytes().iter().copied(),
            );
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(2100), // Offset for not_authorized_msg
                not_authorized_msg.as_bytes().iter().copied(),
            );
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(2150), // Offset for recording_usage_msg
                recording_usage_msg.as_bytes().iter().copied(),
            );
        }
        
        // Add data section for anchor_data action
        if action == "anchor_data" {
            // Add message strings to data section
            let anchoring_success_msg = "Data anchored successfully. CID: ";
            let anchoring_failed_msg = "Anchoring failed";
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(5000), // Offset for success message
                anchoring_success_msg.as_bytes().iter().copied(),
            );
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(5050), // Offset for failure message
                anchoring_failed_msg.as_bytes().iter().copied(),
            );
            
            // Extract content from the DSL input
            let content_bytes = if let Some(content) = dsl_input.get("content") {
                if let Some(content_str) = content.as_str() {
                    content_str.as_bytes().to_vec()
                } else {
                    // Serialize non-string values to JSON bytes
                    serde_json::to_vec(content)
                        .map_err(|e| CompilerError::DslError(format!("Failed to serialize content: {}", e)))?
                }
            } else {
                return Err(CompilerError::DslError("anchor_data requires content field".to_string()));
            };
            
            // Add content to data section at offset 3000
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(3000), // Offset for content
                content_bytes.iter().copied(),
            );
            
            // Process parent CIDs if present
            let parents = dsl_input.get("parents").and_then(|p| p.as_array()).cloned().unwrap_or_default();
            let parent_cids: Vec<String> = parents.iter()
                .filter_map(|p| p.as_str().map(|s| s.to_string()))
                .collect();
            
            // Add each parent CID to the data section if we have any
            if !parent_cids.is_empty() {
                for (i, cid_str) in parent_cids.iter().enumerate() {
                    let offset = 4000 + (i * 100); // Assume each CID gets 100 bytes of space
                    data_section.active(
                        0, // Memory index
                        &wasm_encoder::ConstExpr::i32_const(offset as i32),
                        cid_str.as_bytes().iter().copied(),
                    );
                }
            }
            
            // Create a buffer for the resulting CID at offset 6000
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(6000), // Offset for CID result buffer
                vec![0; 100].iter().copied(), // 100 bytes of zeros for CID buffer
            );
        }
        
        // For store_data action, also embed the value
        if action == "store_data" {
            // Store key_cid at memory offset 2048
            let key_cid_bytes = key_cid_str.as_bytes();
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(2048), // Offset for key_cid
                key_cid_bytes.iter().copied(),
            );
            
            // Store value at memory offset 4096
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(4096), // Offset for value
                value_bytes.iter().copied(),
            );
        } else if action == "get_data" {
            // Store key_cid at memory offset 2048
            let key_cid_bytes = key_cid_str.as_bytes();
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(2048), // Offset for key_cid
                key_cid_bytes.iter().copied(),
            );
            
            // Create buffer spaces for use by the get_data action
            // Buffer at 6144 for storing the length (4 bytes)
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(6144), // Offset for length buffer
                vec![0; 4].iter().copied(), // 4 bytes of zeros
            );
            
            // Buffer at 8192 for storing the retrieved data (1024 bytes should be enough)
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(8192), // Offset for data buffer
                vec![0; 1024].iter().copied(), // 1024 bytes of zeros
            );
        }
        
        // Add data section for entity creation actions (after the anchor_data action section, around line 777)
        if action == "create_cooperative" || action == "create_community" {
            // Add message strings to data section
            let entity_created_msg = "Entity created successfully. DID: ";
            let entity_failed_msg = "Entity creation failed";
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(5100), // Offset for success message
                entity_created_msg.as_bytes().iter().copied(),
            );
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(5150), // Offset for failure message
                entity_failed_msg.as_bytes().iter().copied(),
            );
            
            // Extract parent_did from DSL input
            let parent_did = dsl_input.get("parent_did")
                .and_then(|p| p.as_str())
                .unwrap_or("did:icn:federation"); // Default to federation DID if not specified
            
            // Store parent_did at offset 5200
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(5200), // Offset for parent_did
                parent_did.as_bytes().iter().copied(),
            );
            
            // Extract genesis_payload from DSL input or create default
            let genesis_payload_bytes = if let Some(payload) = dsl_input.get("genesis_payload") {
                // Serialize to CBOR
                let ipld_value = convert_json_to_ipld(payload)?;
                libipld_dagcbor::DagCborCodec.encode(&ipld_value)
                    .map_err(|e| CompilerError::DslError(format!("Failed to encode payload as CBOR: {}", e)))?
            } else {
                // Create a default payload with name and description
                let entity_name = dsl_input.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(if action == "create_cooperative" { "New Cooperative" } else { "New Community" });
                
                let description = dsl_input.get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("Created via CCL");
                
                let now = chrono::Utc::now().timestamp();
                
                let default_payload = serde_json::json!({
                    "name": entity_name,
                    "description": description,
                    "created_at": now,
                    "created_by": options.caller_did.clone().unwrap_or_default()
                });
                
                let ipld_value = convert_json_to_ipld(&default_payload)?;
                libipld_dagcbor::DagCborCodec.encode(&ipld_value)
                    .map_err(|e| CompilerError::DslError(format!("Failed to encode default payload as CBOR: {}", e)))?
            };
            
            // Store genesis_payload at offset 5500
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(5500), // Offset for genesis_payload
                genesis_payload_bytes.iter().copied(),
            );
            
            // Set entity_type based on action
            let entity_type = if action == "create_cooperative" { "Cooperative" } else { "Community" };
            
            // Store entity_type at offset 5800
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(5800), // Offset for entity_type
                entity_type.as_bytes().iter().copied(),
            );
            
            // Create buffer for output DID at offset 6100
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(6100), // Offset for output DID
                vec![0; 100].iter().copied(), // 100 bytes for DID
            );
        }
        
        if action == "store_dag_node" {
            // Add message strings
            let node_stored_msg = "Node stored successfully. CID: ";
            let node_failed_msg = "Node storage failed";
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(6200), // Offset for success message
                node_stored_msg.as_bytes().iter().copied(),
            );
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(6250), // Offset for failure message
                node_failed_msg.as_bytes().iter().copied(),
            );
            
            // Extract entity_did from DSL input
            let entity_did = dsl_input.get("entity_did")
                .and_then(|p| p.as_str())
                .unwrap_or(""); // No default, will be a runtime error if not provided
            
            // Store entity_did at offset 6300
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(6300), // Offset for entity_did
                entity_did.as_bytes().iter().copied(),
            );
            
            // Extract payload from DSL input
            if let Some(payload) = dsl_input.get("payload") {
                // Serialize to CBOR
                let ipld_value = convert_json_to_ipld(payload)?;
                let payload_bytes = libipld_dagcbor::DagCborCodec.encode(&ipld_value)
                    .map_err(|e| CompilerError::DslError(format!("Failed to encode payload as CBOR: {}", e)))?;
                
                // Store payload at offset 6400
                data_section.active(
                    0, // Memory index
                    &wasm_encoder::ConstExpr::i32_const(6400), // Offset for payload
                    payload_bytes.iter().copied(),
                );
            } else {
                return Err(CompilerError::DslError("store_dag_node requires payload field".to_string()));
            }
            
            // Extract parent CIDs if present
            let parents = dsl_input.get("parents").and_then(|p| p.as_array()).cloned().unwrap_or_default();
            let parent_cids: Vec<String> = parents.iter()
                .filter_map(|p| p.as_str().map(|s| s.to_string()))
                .collect();
            
            // Store parent CIDs at sequential offsets starting at 6500
            for (i, cid_str) in parent_cids.iter().enumerate() {
                let offset = 6500 + (i * 100); // Assume each CID gets 100 bytes of space
                data_section.active(
                    0, // Memory index
                    &wasm_encoder::ConstExpr::i32_const(offset as i32),
                    cid_str.as_bytes().iter().copied(),
                );
            }
            
            // Store pointers to parent CIDs at 7500
            // This is an array of 32-bit pointers to each parent CID string
            let mut parent_ptrs = Vec::new();
            for i in 0..parent_cids.len() {
                // Convert each offset to little-endian bytes
                let ptr = (6500 + (i * 100)) as u32;
                parent_ptrs.extend_from_slice(&ptr.to_le_bytes());
            }
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(7500), // Offset for parent pointers array
                parent_ptrs.iter().copied(),
            );
            
            // Store lengths of parent CIDs at 7600
            let mut parent_lens = Vec::new();
            for cid_str in &parent_cids {
                // Convert each length to little-endian bytes
                let len = cid_str.len() as u32;
                parent_lens.extend_from_slice(&len.to_le_bytes());
            }
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(7600), // Offset for parent lengths array
                parent_lens.iter().copied(),
            );
            
            // Create empty/dummy signature at 7700
            // In a real implementation, this would be generated by the calling client
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(7700), // Offset for signature
                vec![0; 64].iter().copied(), // 64 bytes placeholder signature
            );
            
            // Create default metadata at 7800
            let metadata = serde_json::json!({
                "timestamp": chrono::Utc::now().timestamp() as u64,
                "sequence": 1,
                "scope": entity_did,
            });
            
            let ipld_value = convert_json_to_ipld(&metadata)?;
            let metadata_bytes = libipld_dagcbor::DagCborCodec.encode(&ipld_value)
                .map_err(|e| CompilerError::DslError(format!("Failed to encode metadata as CBOR: {}", e)))?;
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(7800), // Offset for metadata
                metadata_bytes.iter().copied(),
            );
            
            // Create buffer for output CID at offset 7900
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(7900), // Offset for output CID
                vec![0; 100].iter().copied(), // 100 bytes for CID
            );
        }
        
        if action == "get_dag_node" {
            // Add message strings
            let node_found_msg = "Node found. CID: ";
            let node_not_found_msg = "Node not found";
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(8000), // Offset for found message
                node_found_msg.as_bytes().iter().copied(),
            );
            
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(8050), // Offset for not found message
                node_not_found_msg.as_bytes().iter().copied(),
            );
            
            // Extract entity_did from DSL input
            let entity_did = dsl_input.get("entity_did")
                .and_then(|p| p.as_str())
                .unwrap_or(""); // No default, will be a runtime error if not provided
            
            // Store entity_did at offset 8100
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(8100), // Offset for entity_did
                entity_did.as_bytes().iter().copied(),
            );
            
            // Extract CID from DSL input
            let cid_str = dsl_input.get("cid")
                .and_then(|c| c.as_str())
                .unwrap_or(""); // No default, will be a runtime error if not provided
            
            // Store CID at offset 8200
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(8200), // Offset for CID
                cid_str.as_bytes().iter().copied(),
            );
            
            // Create buffer for output node at offset 8300
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(8300), // Offset for output node
                vec![0; 1024].iter().copied(), // 1024 bytes for node (adjust as needed)
            );
        }
        
        // Add data section to module
        module.section(&data_section);
        
        // Step 9: Define code section - function bodies
        let mut code = CodeSection::new();
        
        // Create _start function body
        let mut start_func = wasm_encoder::Function::new(vec![]);
        
        // _start function just returns
        start_func.instruction(&wasm_encoder::Instruction::End);
        code.function(&start_func);
        
        // Create invoke function body
        let mut invoke_func = wasm_encoder::Function::new(vec![
            (1, ValType::I32), // local 0 = status code (i32, return value)
            (1, ValType::I32), // local 1 = temporary value 1 (i32)
            (1, ValType::I32), // local 2 = temporary value 2 (i32)
            (1, ValType::I32), // local 3 = temporary value 3 (i32)
            (1, ValType::I32), // local 4 = temporary value 4 (i32)
            (1, ValType::I32), // local 5 = temporary value 5 (i32)
            (1, ValType::I32), // local 6 = temporary value 6 (i32)
            (1, ValType::I32), // local 7 = temporary value 7 (i32)
        ]);
        
        // Initialize return value to error (1)
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
        invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Log the template info
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1024)); // Template info string
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(template_info.len() as i32));
        invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // Logic based on action type
        if action == "store_data" {
            // For store_data action, call host_storage_put with the embedded data
            
            // Load key_cid pointer and length
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(2048)); // key_cid pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(key_cid_str.len() as i32)); // key_cid length
            
            // Load value pointer and length
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(4096)); // value pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(value_bytes.len() as i32)); // value length
            
            // Call host_storage_put (imported function at index 2)
            invoke_func.instruction(&wasm_encoder::Instruction::Call(2));
            
            // Check return value and set status code
            // If host_storage_put returns non-zero (success), set status to 0 (success)
            // Otherwise, return 1 (failure, which is the default)
            
            // Create a block for if/else construct
            invoke_func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            
            // If result from host_storage_put is non-zero
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Success status
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0)); // Set local 0 to success
            
            invoke_func.instruction(&wasm_encoder::Instruction::End); // End if block
            
        } else if action == "get_data" {
            // For get_data action, call host_storage_get with the embedded key_cid

            // Define spaces for retrieved data and length
            // 6144 for storing the length (4 bytes)
            // 8192 for storing the retrieved data (up to some reasonable size)
            
            // Load key_cid pointer and length
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(2048)); // key_cid pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(key_cid_str.len() as i32)); // key_cid length
            
            // Load output buffer pointer and length pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8192)); // Output buffer at 8192
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(6144)); // Length pointer at 6144
            
            // Call host_storage_get (imported function at index 1)
            invoke_func.instruction(&wasm_encoder::Instruction::Call(1));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(1)); // Store result in local 1
            
            // Check if data was found (host_storage_get returns 1 on success)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1));
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::I32Eq);
            
            // If data was found
            invoke_func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            
            // Log "Data found" message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1536)); // "Data found" message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(data_found_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set success status (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // Else block (data not found)
            invoke_func.instruction(&wasm_encoder::Instruction::Else);
            
            // Log "Data not found" message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1600)); // "Data not found" message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(data_not_found_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set failure status (1) - already the default, but being explicit
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // End if/else
            invoke_func.instruction(&wasm_encoder::Instruction::End);
            
        } else if action == "log_caller_info" {
            // For log_caller_info action, call identity host functions and log the results
            
            // Get the caller's DID into a buffer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(9000)); // Pointer to DID buffer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(100)); // Max length for DID
            invoke_func.instruction(&wasm_encoder::Instruction::Call(3)); // Call host_get_caller_did (offset by base imports)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(1)); // Store returned length in local 1
            
            // Log the DID prefix
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1700)); // "Caller DID: " message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(11)); // Length of "Caller DID: "
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Log the actual DID
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(9000)); // Pointer to DID buffer
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1)); // Get the actual DID length
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Get the caller's scope
            invoke_func.instruction(&wasm_encoder::Instruction::Call(4)); // Call host_get_caller_scope
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(1)); // Store scope in local 1
            
            // Convert scope integer to ASCII digit and store at memory[1799]
            // We need to add '0' (ASCII 48) to the scope value (0-9)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1)); // Get scope value
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(48)); // ASCII '0'
            invoke_func.instruction(&wasm_encoder::Instruction::I32Add); // scope + '0' = ASCII digit
            
            // Store the ASCII digit at memory[1799]
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1799)); // Memory address
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1)); // Get scope value + ASCII '0'
            
            // Create a MemArg for the I32Store8 instruction
            let store_memarg = wasm_encoder::MemArg { 
                offset: 0,
                align: 0,
                memory_index: 0,
            };
            invoke_func.instruction(&wasm_encoder::Instruction::I32Store8(store_memarg)); // Store 1 byte
            
            // Log the scope prefix
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1750)); // "Caller Scope: " message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(14)); // Length of "Caller Scope: "
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Log the actual scope digit
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1799)); // Pointer to scope digit
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Length is 1 byte
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set success status (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
        } else if action == "propose_membership" {
            // Logic for propose_membership action
            // For this example, just return success (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        } else if action == "propose_budget" {
            // Logic for propose_budget action
            // For this example, just return success (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        } else if action == "perform_metered_action" {
            // Extract resource type and amount from the DSL input
            let resource_type = dsl_input.get("resource_type")
                .and_then(|v| v.as_i64())
                .unwrap_or(0); // Default to Compute (0) if not specified
            
            let amount = dsl_input.get("amount")
                .and_then(|v| v.as_i64())
                .unwrap_or(1); // Default to 1 if not specified
            
            // Log "Checking resource:" message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(2000)); // Checking resource message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(17)); // Message length
            invoke_func.instruction(&wasm_encoder::Instruction::Call(2)); // Call host_log_message
            
            // Call host_check_resource_authorization
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(resource_type as i32)); // Resource type
            invoke_func.instruction(&wasm_encoder::Instruction::I64Const(amount)); // Amount
            invoke_func.instruction(&wasm_encoder::Instruction::Call(7)); // Call host_check_resource_authorization
            
            // Store the result in local 1
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(1));
            
            // Check if authorized (value in local 1)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1));
            
            // If-else block
            invoke_func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            
            // If branch (authorized)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(2050)); // Authorized message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(10)); // Message length
            invoke_func.instruction(&wasm_encoder::Instruction::Call(2)); // Call host_log_message
            
            // Log recording usage message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(2100)); // Recording usage message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(16)); // Message length
            invoke_func.instruction(&wasm_encoder::Instruction::Call(2)); // Call host_log_message
            
            // Record resource usage
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(resource_type as i32)); // Resource type
            invoke_func.instruction(&wasm_encoder::Instruction::I64Const(amount)); // Amount
            invoke_func.instruction(&wasm_encoder::Instruction::Call(8)); // Call host_record_resource_usage
            
            // Set return value to success (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // Else branch (not authorized)
            invoke_func.instruction(&wasm_encoder::Instruction::Else);
            
            // Log not authorized message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(2150)); // Not authorized message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(14)); // Message length
            invoke_func.instruction(&wasm_encoder::Instruction::Call(2)); // Call host_log_message
            
            // Set return value to error (1)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // End if
            invoke_func.instruction(&wasm_encoder::Instruction::End);
            
        } else if action == "anchor_data" {
            // Extract content pointer and length
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(3000)); // Pointer to content data
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(1)); // Store in local 1
            
            // Get content length
            let content_bytes = if let Some(content) = dsl_input.get("content") {
                if let Some(content_str) = content.as_str() {
                    content_str.as_bytes().len()
                } else {
                    // Serialize non-string values to JSON bytes
                    if let Ok(bytes) = serde_json::to_vec(content) {
                        bytes.len()
                    } else {
                        0
                    }
                }
            } else {
                0
            };
            
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(content_bytes as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(2)); // Store content length in local 2
            
            // Process parent CIDs
            let parents = dsl_input.get("parents").and_then(|p| p.as_array()).cloned().unwrap_or_default();
            let parents_count = parents.len();
            
            // Parents array pointer (if we have parents)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(4000)); // Pointer to parents array
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(3)); // Store in local 3
            
            // Parents count
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(parents_count as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(4)); // Store in local 4
            
            // Prepare buffer for result CID
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(6000)); // Pointer to result buffer
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(5)); // Store in local 5
            
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(100)); // Length of result buffer
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(6)); // Store in local 6
            
            // Call host_anchor_to_dag with content_ptr, content_len, parents_ptr, parents_count, result_ptr, result_capacity
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1)); // content_ptr
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(2)); // content_len
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(3)); // parents_ptr
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(4)); // parents_count
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(5)); // result_ptr
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(6)); // result_capacity
            invoke_func.instruction(&wasm_encoder::Instruction::Call(9)); // Call host_anchor_to_dag
            
            // Store the result (CID string length or error code) in local 7
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(7));
            
            // Check if successful (result >= 0)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(7));
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::I32GeS); // result >= 0
            
            // If-else block
            invoke_func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            
            // If branch (success)
            // Log success message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(5000)); // Success message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(29)); // Success message length
            invoke_func.instruction(&wasm_encoder::Instruction::Call(2)); // Call host_log_message
            
            // Log the CID string
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(5)); // CID string pointer
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(7)); // CID string length
            invoke_func.instruction(&wasm_encoder::Instruction::Call(2)); // Call host_log_message
            
            // Set return value to success (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // Else branch (failure)
            invoke_func.instruction(&wasm_encoder::Instruction::Else);
            
            // Log failure message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(5050)); // Failure message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(16)); // Failure message length
            invoke_func.instruction(&wasm_encoder::Instruction::Call(2)); // Call host_log_message
            
            // Set return value to error (1)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // End if
            invoke_func.instruction(&wasm_encoder::Instruction::End);
        } else if action == "create_cooperative" || action == "create_community" {
            // Get parameters for create_sub_dag call
            // Parent DID pointer and length
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(5200)); // parent_did pointer
            let parent_did = dsl_input.get("parent_did")
                .and_then(|p| p.as_str())
                .unwrap_or("did:icn:federation");
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(parent_did.len() as i32)); // parent_did length
            
            // Genesis payload pointer and length
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(5500)); // genesis_payload pointer
            
            // Calculate payload length
            let genesis_payload_bytes = if let Some(payload) = dsl_input.get("genesis_payload") {
                let ipld_value = convert_json_to_ipld(payload)?;
                libipld_dagcbor::DagCborCodec.encode(&ipld_value)
                    .map_err(|e| CompilerError::DslError(format!("Failed to encode payload as CBOR: {}", e)))?
            } else {
                // Calculate length for default payload (already stored in data section)
                let entity_name = dsl_input.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(if action == "create_cooperative" { "New Cooperative" } else { "New Community" });
                
                let description = dsl_input.get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("Created via CCL");
                
                let now = chrono::Utc::now().timestamp();
                
                let default_payload = serde_json::json!({
                    "name": entity_name,
                    "description": description,
                    "created_at": now,
                    "created_by": options.caller_did.clone().unwrap_or_default()
                });
                
                let ipld_value = convert_json_to_ipld(&default_payload)?;
                libipld_dagcbor::DagCborCodec.encode(&ipld_value)
                    .map_err(|e| CompilerError::DslError(format!("Failed to encode default payload as CBOR: {}", e)))?
            };
            
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(genesis_payload_bytes.len() as i32)); // genesis_payload length
            
            // Entity type pointer and length
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(5800)); // entity_type pointer
            let entity_type = if action == "create_cooperative" { "Cooperative" } else { "Community" };
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(entity_type.len() as i32)); // entity_type length
            
            // Output DID buffer pointer and max length
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(6100)); // output DID pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(100)); // max output DID length
            
            // Call host_create_sub_dag
            let import_index = imports.count() - 1; // Get the index of the last imported function
            invoke_func.instruction(&wasm_encoder::Instruction::Call(import_index));
            
            // Store result in local 1
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(1));
            
            // Check if creation was successful (result > 0)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1));
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::I32GtS); // result > 0
            
            // If-else block
            invoke_func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            
            // If branch (success)
            // Log success message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(5100)); // Success message
            let success_msg = "Entity created successfully. DID: ";
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(success_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Log the created DID
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(6100)); // DID buffer
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1)); // DID length from result
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set success status (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // Else branch (creation failed)
            invoke_func.instruction(&wasm_encoder::Instruction::Else);
            
            // Log failure message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(5150)); // Failure message
            let failure_msg = "Entity creation failed";
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(failure_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set return value to error (1)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // End if
            invoke_func.instruction(&wasm_encoder::Instruction::End);
            
        } else if action == "store_dag_node" {
            // Parameters for host_store_node
            // entity_did_ptr, entity_did_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(6300)); // entity_did pointer
            let entity_did = dsl_input.get("entity_did")
                .and_then(|p| p.as_str())
                .unwrap_or(""); // No default, will be a runtime error if not provided
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(entity_did.len() as i32)); // entity_did length
            
            // payload_ptr, payload_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(6400)); // payload pointer
            // Calculate payload length (already computed when adding to data section)
            let payload_bytes = if let Some(payload) = dsl_input.get("payload") {
                let ipld_value = convert_json_to_ipld(payload)?;
                libipld_dagcbor::DagCborCodec.encode(&ipld_value)
                    .map_err(|e| CompilerError::DslError(format!("Failed to encode payload as CBOR: {}", e)))?;
            } else {
                return Err(CompilerError::DslError("store_dag_node requires payload field".to_string()));
            };
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(payload_bytes.len() as i32)); // payload length
            
            // parents_cids_ptr_ptr, parents_cids_count
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(7500)); // parents pointers array
            let parents = dsl_input.get("parents").and_then(|p| p.as_array()).cloned().unwrap_or_default();
            let parent_cids: Vec<String> = parents.iter()
                .filter_map(|p| p.as_str().map(|s| s.to_string()))
                .collect();
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(parent_cids.len() as i32)); // parents count
            
            // parent_cid_lens_ptr
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(7600)); // parents lengths array
            
            // signature_ptr, signature_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(7700)); // signature pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(64)); // signature length (64 bytes placeholder)
            
            // metadata_ptr, metadata_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(7800)); // metadata pointer
            // Calculate metadata length (already computed when adding to data section)
            let metadata = serde_json::json!({
                "timestamp": chrono::Utc::now().timestamp() as u64,
                "sequence": 1,
                "scope": entity_did
            });
            let ipld_value = convert_json_to_ipld(&metadata)?;
            let metadata_bytes = libipld_dagcbor::DagCborCodec.encode(&ipld_value)
                .map_err(|e| CompilerError::DslError(format!("Failed to encode metadata as CBOR: {}", e)))?;
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(metadata_bytes.len() as i32)); // metadata length
            
            // cid_out_ptr, cid_out_max_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(7900)); // output CID pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(100)); // max output CID length
            
            // Call host_store_node
            let import_index = imports.count() - 1; // Get the index of the last imported function
            invoke_func.instruction(&wasm_encoder::Instruction::Call(import_index));
            
            // Store result in local 1
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(1));
            
            // Check if storage was successful (result > 0)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1));
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::I32GtS); // result > 0
            
            // If-else block
            invoke_func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            
            // If branch (success)
            // Log success message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(6200)); // Success message
            let success_msg = "Node stored successfully. CID: ";
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(success_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set success status (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // Else branch (storage failed)
            invoke_func.instruction(&wasm_encoder::Instruction::Else);
            
            // Log failure message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(6250)); // Failure message
            let failure_msg = "Node storage failed";
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(failure_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set failure status (already default, but being explicit)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // End if/else
            invoke_func.instruction(&wasm_encoder::Instruction::End);
            
        } else if action == "get_dag_node" {
            // First, check if the node exists using host_contains_node
            // entity_did_ptr, entity_did_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8100)); // entity_did pointer
            let entity_did = dsl_input.get("entity_did")
                .and_then(|p| p.as_str())
                .unwrap_or(""); // No default, will be a runtime error if not provided
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(entity_did.len() as i32)); // entity_did length
            
            // cid_ptr, cid_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8200)); // CID pointer
            let cid_str = dsl_input.get("cid")
                .and_then(|c| c.as_str())
                .unwrap_or(""); // No default, will be a runtime error if not provided
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(cid_str.len() as i32)); // CID length
            
            // Call host_contains_node
            let contains_index = imports.count() - 1; // Get the index of the host_contains_node function
            invoke_func.instruction(&wasm_encoder::Instruction::Call(contains_index));
            
            // Store result in local 1
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(1));
            
            // Check if node exists (result == 1)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(1));
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::I32Eq); // result == 1
            
            // If-else block
            invoke_func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            
            // If branch (node exists), call host_get_node
            // entity_did_ptr, entity_did_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8100)); // entity_did pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(entity_did.len() as i32)); // entity_did length
            
            // cid_ptr, cid_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8200)); // CID pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(cid_str.len() as i32)); // CID length
            
            // node_out_ptr, node_out_max_len
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8300)); // output node pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1024)); // max output node length
            
            // Call host_get_node
            let get_node_index = contains_index - 1; // Get the index of the host_get_node function
            invoke_func.instruction(&wasm_encoder::Instruction::Call(get_node_index));
            
            // Store result in local 2
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(2));
            
            // Check if retrieval was successful (result > 0)
            invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(2));
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::I32GtS); // result > 0
            
            // If-else block
            invoke_func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            
            // If branch (retrieval success)
            // Log success message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8000)); // Success message
            let success_msg = "Node found. CID: ";
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(success_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Log the CID
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8200)); // CID pointer
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(cid_str.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set success status (0)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // Else branch (retrieval failed)
            invoke_func.instruction(&wasm_encoder::Instruction::Else);
            
            // Log failure message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8050)); // Failure message
            let failure_msg = "Node not found";
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(failure_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set failure status
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // End inner if/else
            invoke_func.instruction(&wasm_encoder::Instruction::End);
            
            // Else branch (node doesn't exist)
            invoke_func.instruction(&wasm_encoder::Instruction::Else);
            
            // Log "Node not found" message
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(8050)); // Not found message
            let not_found_msg = "Node not found";
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(not_found_msg.len() as i32));
            invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
            
            // Set failure status
            invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
            invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
            
            // End outer if/else
            invoke_func.instruction(&wasm_encoder::Instruction::End);
        }
        
        // Return the status code from local 0
        invoke_func.instruction(&wasm_encoder::Instruction::LocalGet(0));
        invoke_func.instruction(&wasm_encoder::Instruction::End);
        
        code.function(&invoke_func);
        
        module.section(&code);
        
        // Step 10: Add custom sections for metadata
        
        // Add the essential metadata in a custom section named "icn-metadata"
        let custom_section = wasm_encoder::CustomSection {
            name: Cow::Borrowed("icn-metadata"),
            data: Cow::Borrowed(metadata_json.as_bytes()),
        };
        module.section(&custom_section);
        
        // Add CCL config and DSL input in custom sections if debug info is enabled
        if options.include_debug_info {
            // Serialize CCL config and DSL input to JSON
            let ccl_json = serde_json::to_string(ccl_config)
                .map_err(|e| CompilerError::General(format!("Failed to serialize CCL config: {}", e)))?;
            let dsl_json = serde_json::to_string(dsl_input)
                .map_err(|e| CompilerError::General(format!("Failed to serialize DSL input: {}", e)))?;
                
            // Add CCL config in a custom section
            let ccl_section = wasm_encoder::CustomSection {
                name: Cow::Borrowed("icn-ccl-config"),
                data: Cow::Borrowed(ccl_json.as_bytes()),
            };
            module.section(&ccl_section);
            
            // Add DSL input in a custom section
            let dsl_section = wasm_encoder::CustomSection {
                name: Cow::Borrowed("icn-dsl-input"),
                data: Cow::Borrowed(dsl_json.as_bytes()),
            };
            module.section(&dsl_section);
        }
        
        // Finalize and return the WASM module bytes
        Ok(module.finish())
    }

    /// Generate a more complex WASM module with actual business logic
    #[cfg(feature = "templating")]
    fn generate_templated_wasm(
        &self,
        ccl_config: &GovernanceConfig,
        dsl_input: &JsonValue,
        options: &CompilationOptions,
    ) -> CompilerResult<Vec<u8>> {
        // This is a placeholder for the templating approach
        // In a real implementation, this would:
        // 1. Select a Rust template based on the CCL template type
        // 2. Fill in the template with the DSL input values
        // 3. Compile the Rust code to WASM
        // 4. Return the compiled WASM bytes

        Err(CompilerError::General(
            "Templated WASM generation not yet implemented".to_string(),
        ))
    }
}

/// Helper function to convert JSON to IPLD
fn convert_json_to_ipld(json: &serde_json::Value) -> CompilerResult<libipld::Ipld> {
    match json {
        serde_json::Value::Null => Ok(libipld::Ipld::Null),
        serde_json::Value::Bool(b) => Ok(libipld::Ipld::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(libipld::Ipld::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(libipld::Ipld::Float(f))
            } else {
                Err(CompilerError::DslError(format!("Unsupported number format: {}", n)))
            }
        },
        serde_json::Value::String(s) => Ok(libipld::Ipld::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut ipld_array = Vec::new();
            for item in arr {
                ipld_array.push(convert_json_to_ipld(item)?);
            }
            Ok(libipld::Ipld::List(ipld_array))
        },
        serde_json::Value::Object(obj) => {
            let mut ipld_map = std::collections::BTreeMap::new();
            for (key, value) in obj {
                ipld_map.insert(key.clone(), convert_json_to_ipld(value)?);
            }
            Ok(libipld::Ipld::Map(ipld_map))
        }
    }
} 