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
                        if !dsl_obj.contains_key("key") {
                            return Err(CompilerError::DslError(
                                "anchor_data requires 'key' field".to_string(),
                            ));
                        }
                        if !dsl_obj.contains_key("value") {
                            return Err(CompilerError::DslError(
                                "anchor_data requires 'value' field".to_string(),
                            ));
                        }
                        // parents is optional, so no validation needed
                    }
                    "mint_token" => {
                        if !dsl_obj.contains_key("resource_type") {
                            return Err(CompilerError::DslError(
                                "mint_token requires 'resource_type' field".to_string(),
                            ));
                        }
                        if !dsl_obj.contains_key("recipient") {
                            return Err(CompilerError::DslError(
                                "mint_token requires 'recipient' field".to_string(),
                            ));
                        }
                        if !dsl_obj.contains_key("amount") {
                            return Err(CompilerError::DslError(
                                "mint_token requires 'amount' field".to_string(),
                            ));
                        }
                    }
                    "transfer_resource" => {
                        if !dsl_obj.contains_key("from") {
                            return Err(CompilerError::DslError(
                                "transfer_resource requires 'from' field".to_string(),
                            ));
                        }
                        if !dsl_obj.contains_key("to") {
                            return Err(CompilerError::DslError(
                                "transfer_resource requires 'to' field".to_string(),
                            ));
                        }
                        if !dsl_obj.contains_key("amount") {
                            return Err(CompilerError::DslError(
                                "transfer_resource requires 'amount' field".to_string(),
                            ));
                        }
                        if !dsl_obj.contains_key("resource_type") {
                            return Err(CompilerError::DslError(
                                "transfer_resource requires 'resource_type' field".to_string(),
                            ));
                        }
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

    /// Generate a WASM module for the given CCL config and DSL input
    fn generate_wasm_module(
        &self,
        ccl_config: &GovernanceConfig,
        dsl_input: &JsonValue,
        options: &CompilationOptions,
    ) -> CompilerResult<Vec<u8>> {
        // Extract action from DSL input
        let action = self.extract_action_from_dsl(dsl_input)?;
        
        // Generate WASM for the action
        match action.as_str() {
            #[cfg(feature = "templating")]
            "use_template" => {
                // If the action is "use_template", use the template approach
                return self.generate_templated_wasm(ccl_config, dsl_input, options);
            }
            // For all other actions, generate bytecode directly
            _ => {
                // Create a basic WASM module with host function calls
                let module_bytes = self.generate_basic_wasm_module(ccl_config, dsl_input, options)?;
                
                Ok(module_bytes)
            }
        }
    }
    
    /// Generate a basic WASM module for the given action
    fn generate_basic_wasm_module(
        &self,
        ccl_config: &GovernanceConfig,
        dsl_input: &JsonValue,
        options: &CompilationOptions,
    ) -> CompilerResult<Vec<u8>> {
        // Extract action and parameters
        let action = self.extract_action_from_dsl(dsl_input)?;
        
        // Create a new WASM module with basic host imports
        let mut module = Module::new();
        
        // Define memory section with default limits
        let memory_limits = options.memory_limits.as_ref().unwrap_or(&MemoryLimits::default());
        let memory = wasm_encoder::MemorySection::new().entry(
            wasm_encoder::MemoryType {
                minimum: memory_limits.min_pages,
                maximum: memory_limits.max_pages,
                memory64: false,
                shared: false,
            }
        );
        
        // Add memory section to module
        module.section(&memory);
        
        // Define type section (function signatures)
        let mut types = TypeSection::new();
        
        // Type 0: () -> () for _start function
        types.function(vec![], vec![]);
        
        // Type 1: (i32, i32) -> i32 for host_log_message function
        types.function(vec![ValType::I32, ValType::I32, ValType::I32], vec![]);
        
        // Type 2: (i32, i32) -> i32 for invoke function (our main entry point)
        types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 3: (i32, i32, i32, i32) -> i32 for host_storage_get
        types.function(
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
        );
        
        // Type 4: (i32, i32, i32, i32) -> i32 for host_storage_put
        types.function(
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
        );
        
        // Type 5: (i32, i32) -> i32 for host_get_caller_did
        types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 6: () -> i32 for host_get_caller_scope
        types.function(vec![], vec![ValType::I32]);
        
        // Type 7: (i32, i32) -> i32 for host_check_resource_authorization
        types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
        
        // Type 8: (i32, i32) -> () for host_record_resource_usage
        types.function(vec![ValType::I32, ValType::I32], vec![]);
        
        // Type 9: (i32, i32, i32, i32, i32, i32) -> i32 for host_anchor_to_dag
        types.function(
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
        );
        
        // Type 10: (i32, i32, i32, i32) -> i32 for host_mint_token
        types.function(
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], 
            vec![ValType::I32]
        );
        
        // Type 11: (i32, i32, i32, i32, i32, i32) -> i32 for host_transfer_resource
        types.function(
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
        );
        
        // Add the type section to the module
        module.section(&types);
        
        // Define import section (host functions we'll use)
        let mut imports = ImportSection::new();
        
        // Import host_log_message from env
        imports.import(
            "env",
            "host_log_message",
            EntityType::Function(1), // Using type index 1 (log message function)
        );
        
        // Import host_storage_get from env
        imports.import("env", "host_storage_get", EntityType::Function(3));
        
        // Import host_storage_put from env
        imports.import("env", "host_storage_put", EntityType::Function(4));
        
        // Import host_get_caller_did from env
        imports.import("env", "host_get_caller_did", EntityType::Function(5));
        
        // Import host_get_caller_scope from env
        imports.import("env", "host_get_caller_scope", EntityType::Function(6));
        
        // Import host_check_resource_authorization from env
        imports.import("env", "host_check_resource_authorization", EntityType::Function(7));
        
        // Import host_record_resource_usage from env
        imports.import("env", "host_record_resource_usage", EntityType::Function(8));
        
        // Import host_anchor_to_dag from env
        imports.import("env", "host_anchor_to_dag", EntityType::Function(9));
        
        // Import host_mint_token from env
        imports.import("env", "host_mint_token", EntityType::Function(10));
        
        // Import host_transfer_resource from env
        imports.import("env", "host_transfer_resource", EntityType::Function(11));
        
        // Add import section to module
        module.section(&imports);
        
        // Define function section (indices of our functions' signatures)
        let mut functions = FunctionSection::new();
        
        // Function 10: _start function (type 0)
        functions.function(0);
        
        // Function 11: invoke function (type 2)
        functions.function(2);
        
        // Add function section to module
        module.section(&functions);
        
        // Define export section (functions we export)
        let mut exports = ExportSection::new();
        
        // Export memory
        exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
        
        // Export _start function
        exports.export("_start", wasm_encoder::ExportKind::Func, 10);
        
        // Export invoke function
        exports.export("invoke", wasm_encoder::ExportKind::Func, 11);
        
        // Add export section to module
        module.section(&exports);
        
        // Extract parameters we'll need for data section
        let mut data_items = vec![];
        
        // Some common messages in our data section
        let mut data_offset = 0;
        
        // Add a debugging message
        let debug_msg = format!("Executing {} for template {}", action, ccl_config.template_type);
        data_items.push((data_offset, debug_msg.into_bytes()));
        data_offset += debug_msg.len();
        
        // Memory layout:
        // 0 - 1000: Debug and status messages
        // 1000 - 2000: Input parameters
        // 2000 - 3000: Result buffers
        // 4000+: Dynamic memory allocation
        
        // Reset offset for our input data
        data_offset = 1000;
        
        // Allocate space for parameters and extract values
        let mut param_offsets = HashMap::new();
        
        // Extract and store all String parameters
        if let Some(obj) = dsl_input.as_object() {
            for (key, value) in obj {
                // Skip the action since we've already processed it
                if key == "action" {
                    continue;
                }
                
                // Handle different parameter types
                if let Some(value_str) = value.as_str() {
                    // Store string values in data section
                    let bytes = value_str.as_bytes();
                    data_items.push((data_offset, bytes.to_vec()));
                    param_offsets.insert(key.clone(), (data_offset, bytes.len()));
                    data_offset += bytes.len() + 1; // +1 for null terminator
                } else if value.is_number() {
                    // We'll handle numeric values directly in the code section
                    // For now just record their existence
                    param_offsets.insert(key.clone(), (0, 0));
                } else if let Some(values) = value.as_array() {
                    // Handle arrays by converting to JSON string for now
                    // TODO: Handle arrays more efficiently
                    let json_str = serde_json::to_string(value).unwrap_or_default();
                    let bytes = json_str.as_bytes();
                    data_items.push((data_offset, bytes.to_vec()));
                    param_offsets.insert(key.clone(), (data_offset, bytes.len()));
                    data_offset += bytes.len() + 1;
                }
                // Skip other types for now
            }
        }
        
        // Add a success message
        let success_msg = "Operation completed successfully";
        data_items.push((2000, success_msg.as_bytes().to_vec()));
        
        // Add an error message
        let error_msg = "Operation failed";
        data_items.push((2050, error_msg.as_bytes().to_vec()));
        
        // Add data section to module
        let mut data_section = wasm_encoder::DataSection::new();
        for (offset, bytes) in data_items {
            data_section.active(0, &wasm_encoder::ConstExpr::i32_const(offset as i32), bytes);
        }
        module.section(&data_section);
        
        // Create code section with our function bodies
        let mut code_section = CodeSection::new();
        
        // Define _start function (just calls invoke with default parameters)
        let mut start_func = wasm_encoder::Function::new([]);
        start_func.instruction(&wasm_encoder::Instruction::End);
        code_section.function(&start_func);
        
        // Define invoke function based on action
        let invoke_func = match action.as_str() {
            "log_caller_info" => self.generate_log_caller_info_function(),
            "perform_metered_action" => {
                let resource_type = dsl_input.get("resource_type")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                
                let amount = dsl_input.get("amount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                
                self.generate_perform_metered_action_function(resource_type, amount)
            },
            "anchor_data" => {
                let key_offset = param_offsets.get("key")
                    .map(|(offset, _)| *offset as i32)
                    .unwrap_or(0);
                
                let key_len = param_offsets.get("key")
                    .map(|(_, len)| *len as i32)
                    .unwrap_or(0);
                
                let value_offset = param_offsets.get("value")
                    .map(|(offset, _)| *offset as i32)
                    .unwrap_or(0);
                
                let value_len = param_offsets.get("value")
                    .map(|(_, len)| *len as i32)
                    .unwrap_or(0);
                
                self.generate_anchor_data_function(key_offset, key_len, value_offset, value_len)
            },
            "mint_token" => {
                let resource_type = dsl_input.get("resource_type")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                
                let recipient_offset = param_offsets.get("recipient")
                    .map(|(offset, _)| *offset as i32)
                    .unwrap_or(0);
                
                let recipient_len = param_offsets.get("recipient")
                    .map(|(_, len)| *len as i32)
                    .unwrap_or(0);
                
                let amount = dsl_input.get("amount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                
                self.generate_mint_token_function(resource_type, recipient_offset, recipient_len, amount)
            },
            "transfer_resource" => {
                let from_offset = param_offsets.get("from")
                    .map(|(offset, _)| *offset as i32)
                    .unwrap_or(0);
                
                let from_len = param_offsets.get("from")
                    .map(|(_, len)| *len as i32)
                    .unwrap_or(0);
                
                let to_offset = param_offsets.get("to")
                    .map(|(offset, _)| *offset as i32)
                    .unwrap_or(0);
                
                let to_len = param_offsets.get("to")
                    .map(|(_, len)| *len as i32)
                    .unwrap_or(0);
                
                let resource_type = dsl_input.get("resource_type")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                
                let amount = dsl_input.get("amount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                
                self.generate_transfer_resource_function(
                    from_offset, from_len, 
                    to_offset, to_len,
                    resource_type, amount
                )
            },
            _ => {
                // Default function that just logs and returns success
                self.generate_default_function(&action)
            }
        };
        
        // Add the invoke function to code section
        code_section.function(&invoke_func);
        
        // Add code section to module
        module.section(&code_section);
        
        // Add metadata if enabled
        if options.include_debug_info {
            // Create metadata info
            let metadata = self.create_metadata(ccl_config, dsl_input, options)?;
            let metadata_json = serde_json::to_string(&metadata)
                .map_err(|e| CompilerError::General(format!("Failed to serialize metadata: {}", e)))?;
            
            // Add custom section with metadata
            let custom_section = wasm_encoder::CustomSection {
                name: std::borrow::Cow::Borrowed("icn-ccl-metadata"),
                data: std::borrow::Cow::Borrowed(metadata_json.as_bytes()),
            };
            module.section(&custom_section);
            
            // Also add the raw CCL config and DSL input for debugging
            let ccl_json = serde_json::to_string(ccl_config)
                .map_err(|e| CompilerError::General(format!("Failed to serialize CCL config: {}", e)))?;
            let dsl_json = serde_json::to_string(dsl_input)
                .map_err(|e| CompilerError::General(format!("Failed to serialize DSL input: {}", e)))?;
                
            // Add CCL config in a custom section
            let ccl_section = wasm_encoder::CustomSection {
                name: std::borrow::Cow::Borrowed("icn-ccl-config"),
                data: std::borrow::Cow::Borrowed(ccl_json.as_bytes()),
            };
            module.section(&ccl_section);
            
            // Add DSL input in a custom section
            let dsl_section = wasm_encoder::CustomSection {
                name: std::borrow::Cow::Borrowed("icn-dsl-input"),
                data: std::borrow::Cow::Borrowed(dsl_json.as_bytes()),
            };
            module.section(&dsl_section);
        }
        
        // Return the compiled WASM module
        Ok(module.finish())
    }
    
    /// Generate a WASM function body for the log_caller_info action
    fn generate_log_caller_info_function(&self) -> wasm_encoder::Function {
        let mut func = wasm_encoder::Function::new([
            // Local variables: 
            // Local 0: Return value
            // Local 1: Temporary variable for results
            // Local 2: String length
            wasm_encoder::ValType::I32,
            wasm_encoder::ValType::I32,
            wasm_encoder::ValType::I32,
        ]);
        
        // Initialize return value to 0 (success)
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Call host_get_caller_did to get the caller's DID
        func.instruction(&wasm_encoder::Instruction::I32Const(2000)); // Output buffer
        func.instruction(&wasm_encoder::Instruction::I32Const(100)); // Buffer size
        func.instruction(&wasm_encoder::Instruction::Call(4)); // host_get_caller_did
        func.instruction(&wasm_encoder::Instruction::LocalSet(2)); // Save the returned length
        
        // Check if we got a valid result (length > 0)
        func.instruction(&wasm_encoder::Instruction::LocalGet(2));
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::I32GtS());
        func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        
        // Log the DID
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level INFO
        func.instruction(&wasm_encoder::Instruction::I32Const(2000)); // DID buffer
        func.instruction(&wasm_encoder::Instruction::LocalGet(2)); // DID length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // host_log_message
        
        // End if
        func.instruction(&wasm_encoder::Instruction::End);
        
        // Call host_get_caller_scope to get the caller's scope
        func.instruction(&wasm_encoder::Instruction::Call(5)); // host_get_caller_scope
        func.instruction(&wasm_encoder::Instruction::LocalSet(1)); // Save the result
        
        // Log the scope value
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level INFO
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Debug message
        func.instruction(&wasm_encoder::Instruction::I32Const(20)); // Message length (approximate)
        func.instruction(&wasm_encoder::Instruction::Call(0)); // host_log_message
        
        // Return success
        func.instruction(&wasm_encoder::Instruction::LocalGet(0));
        func.instruction(&wasm_encoder::Instruction::End);
        
        func
    }
    
    /// Generate a WASM function body for the perform_metered_action action
    fn generate_perform_metered_action_function(&self, resource_type: i32, amount: i32) -> wasm_encoder::Function {
        let mut func = wasm_encoder::Function::new([
            // Local variables: 
            // Local 0: Return value
            // Local 1: Result of authorization check
            wasm_encoder::ValType::I32,
            wasm_encoder::ValType::I32,
        ]);
        
        // Initialize return value to -1 (error by default)
        func.instruction(&wasm_encoder::Instruction::I32Const(-1));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Log start of metered action
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level INFO
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Debug message
        func.instruction(&wasm_encoder::Instruction::I32Const(20)); // Message length (approximate)
        func.instruction(&wasm_encoder::Instruction::Call(0)); // host_log_message
        
        // Check resource authorization
        func.instruction(&wasm_encoder::Instruction::I32Const(resource_type)); // Resource type
        func.instruction(&wasm_encoder::Instruction::I32Const(amount)); // Amount
        func.instruction(&wasm_encoder::Instruction::Call(6)); // host_check_resource_authorization
        
        // Store the result in local 1
        func.instruction(&wasm_encoder::Instruction::LocalSet(1));
        
        // Check if authorized (value in local 1)
        func.instruction(&wasm_encoder::Instruction::LocalGet(1));
        
        // If-else block
        func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        
        // If branch (authorized)
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2000)); // Success message
        func.instruction(&wasm_encoder::Instruction::I32Const(30)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // Record resource usage
        func.instruction(&wasm_encoder::Instruction::I32Const(resource_type)); // Resource type
        func.instruction(&wasm_encoder::Instruction::I32Const(amount)); // Amount
        func.instruction(&wasm_encoder::Instruction::Call(7)); // Call host_record_resource_usage
        
        // Set return value to success (0)
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Else branch (not authorized)
        func.instruction(&wasm_encoder::Instruction::Else);
        
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2050)); // Error message
        func.instruction(&wasm_encoder::Instruction::I32Const(16)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // End if-else
        func.instruction(&wasm_encoder::Instruction::End);
        
        // Return status
        func.instruction(&wasm_encoder::Instruction::LocalGet(0));
        func.instruction(&wasm_encoder::Instruction::End);
        
        func
    }
    
    /// Generate a WASM function body for the anchor_data action
    fn generate_anchor_data_function(&self, key_offset: i32, key_len: i32, value_offset: i32, value_len: i32) -> wasm_encoder::Function {
        let mut func = wasm_encoder::Function::new([
            // Local variables: 
            // Local 0: Return value
            // Local 1: Result of anchor operation
            wasm_encoder::ValType::I32,
            wasm_encoder::ValType::I32,
        ]);
        
        // Initialize return value to -1 (error by default)
        func.instruction(&wasm_encoder::Instruction::I32Const(-1));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Log start of anchor operation
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level INFO
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Debug message
        func.instruction(&wasm_encoder::Instruction::I32Const(20)); // Message length (approximate)
        func.instruction(&wasm_encoder::Instruction::Call(0)); // host_log_message
        
        // First check authorization for DAG anchoring (compute resource)
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Resource type (Compute)
        func.instruction(&wasm_encoder::Instruction::I32Const(100)); // Amount
        func.instruction(&wasm_encoder::Instruction::Call(6)); // host_check_resource_authorization
        
        // If authorization check passes
        func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        
        // Anchor to DAG
        func.instruction(&wasm_encoder::Instruction::I32Const(key_offset)); // Key pointer
        func.instruction(&wasm_encoder::Instruction::I32Const(key_len)); // Key length
        func.instruction(&wasm_encoder::Instruction::I32Const(value_offset)); // Value pointer
        func.instruction(&wasm_encoder::Instruction::I32Const(value_len)); // Value length
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Parent pointer (none)
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Parent count (none)
        func.instruction(&wasm_encoder::Instruction::Call(8)); // host_anchor_to_dag
        func.instruction(&wasm_encoder::Instruction::LocalSet(1)); // Store result
        
        // Check if anchor succeeded (result > 0)
        func.instruction(&wasm_encoder::Instruction::LocalGet(1));
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::I32GtS());
        func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        
        // Success branch
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2000)); // Success message
        func.instruction(&wasm_encoder::Instruction::I32Const(30)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // Record resource usage
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Resource type (Compute)
        func.instruction(&wasm_encoder::Instruction::I32Const(50)); // Amount
        func.instruction(&wasm_encoder::Instruction::Call(7)); // Call host_record_resource_usage
        
        // Set return value to success (0)
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Else branch (anchor failed)
        func.instruction(&wasm_encoder::Instruction::Else);
        
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2050)); // Error message
        func.instruction(&wasm_encoder::Instruction::I32Const(16)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // End if-else (anchor result)
        func.instruction(&wasm_encoder::Instruction::End);
        
        // End if (authorization check)
        func.instruction(&wasm_encoder::Instruction::End);
        
        // Return status
        func.instruction(&wasm_encoder::Instruction::LocalGet(0));
        func.instruction(&wasm_encoder::Instruction::End);
        
        func
    }
    
    /// Generate a WASM function body for the mint_token action
    fn generate_mint_token_function(&self, resource_type: i32, recipient_offset: i32, recipient_len: i32, amount: i32) -> wasm_encoder::Function {
        let mut func = wasm_encoder::Function::new([
            // Local variables: 
            // Local 0: Return value
            // Local 1: Result of mint operation
            // Local 2: Caller scope (to verify Guardian status)
            wasm_encoder::ValType::I32,
            wasm_encoder::ValType::I32,
            wasm_encoder::ValType::I32,
        ]);
        
        // Initialize return value to -1 (error by default)
        func.instruction(&wasm_encoder::Instruction::I32Const(-1));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Get caller scope to check if Guardian
        func.instruction(&wasm_encoder::Instruction::Call(5)); // host_get_caller_scope
        func.instruction(&wasm_encoder::Instruction::LocalSet(2));
        
        // Check if caller has Guardian scope (scope value is 3)
        func.instruction(&wasm_encoder::Instruction::LocalGet(2));
        func.instruction(&wasm_encoder::Instruction::I32Const(3));
        func.instruction(&wasm_encoder::Instruction::I32Eq());
        func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        
        // Caller is Guardian, proceed with mint operation
        func.instruction(&wasm_encoder::Instruction::I32Const(resource_type)); // Resource type
        func.instruction(&wasm_encoder::Instruction::I32Const(recipient_offset)); // Recipient pointer
        func.instruction(&wasm_encoder::Instruction::I32Const(recipient_len)); // Recipient length
        func.instruction(&wasm_encoder::Instruction::I32Const(amount)); // Amount
        func.instruction(&wasm_encoder::Instruction::Call(9)); // host_mint_token
        func.instruction(&wasm_encoder::Instruction::LocalSet(1)); // Store result
        
        // Check if mint succeeded (result > 0)
        func.instruction(&wasm_encoder::Instruction::LocalGet(1));
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::I32GtS());
        func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        
        // Success branch
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2000)); // Success message
        func.instruction(&wasm_encoder::Instruction::I32Const(30)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // Set return value to success (0)
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Else branch (mint failed)
        func.instruction(&wasm_encoder::Instruction::Else);
        
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2050)); // Error message
        func.instruction(&wasm_encoder::Instruction::I32Const(16)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // End if-else (mint result)
        func.instruction(&wasm_encoder::Instruction::End);
        
        // Else branch (not a Guardian)
        func.instruction(&wasm_encoder::Instruction::Else);
        
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2050)); // Error message
        func.instruction(&wasm_encoder::Instruction::I32Const(16)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // End if-else (Guardian check)
        func.instruction(&wasm_encoder::Instruction::End);
        
        // Return status
        func.instruction(&wasm_encoder::Instruction::LocalGet(0));
        func.instruction(&wasm_encoder::Instruction::End);
        
        func
    }
    
    /// Generate a WASM function body for the transfer_resource action
    fn generate_transfer_resource_function(&self, from_offset: i32, from_len: i32, to_offset: i32, to_len: i32, resource_type: i32, amount: i32) -> wasm_encoder::Function {
        let mut func = wasm_encoder::Function::new([
            // Local variables: 
            // Local 0: Return value
            // Local 1: Result of transfer operation
            wasm_encoder::ValType::I32,
            wasm_encoder::ValType::I32,
        ]);
        
        // Initialize return value to -1 (error by default)
        func.instruction(&wasm_encoder::Instruction::I32Const(-1));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // First check authorization for resource usage
        func.instruction(&wasm_encoder::Instruction::I32Const(resource_type)); // Resource type
        func.instruction(&wasm_encoder::Instruction::I32Const(amount)); // Amount
        func.instruction(&wasm_encoder::Instruction::Call(6)); // host_check_resource_authorization
        
        // If authorization check passes
        func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        
        // Perform the transfer
        func.instruction(&wasm_encoder::Instruction::I32Const(from_offset)); // From pointer
        func.instruction(&wasm_encoder::Instruction::I32Const(from_len)); // From length
        func.instruction(&wasm_encoder::Instruction::I32Const(to_offset)); // To pointer
        func.instruction(&wasm_encoder::Instruction::I32Const(to_len)); // To length
        func.instruction(&wasm_encoder::Instruction::I32Const(resource_type)); // Resource type
        func.instruction(&wasm_encoder::Instruction::I32Const(amount)); // Amount
        func.instruction(&wasm_encoder::Instruction::Call(10)); // host_transfer_resource
        func.instruction(&wasm_encoder::Instruction::LocalSet(1)); // Store result
        
        // Check if transfer succeeded (result > 0)
        func.instruction(&wasm_encoder::Instruction::LocalGet(1));
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::I32GtS());
        func.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        
        // Success branch
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2000)); // Success message
        func.instruction(&wasm_encoder::Instruction::I32Const(30)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // Record resource usage
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Resource type (Compute)
        func.instruction(&wasm_encoder::Instruction::I32Const(20)); // Amount
        func.instruction(&wasm_encoder::Instruction::Call(7)); // Call host_record_resource_usage
        
        // Set return value to success (0)
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Else branch (transfer failed)
        func.instruction(&wasm_encoder::Instruction::Else);
        
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        func.instruction(&wasm_encoder::Instruction::I32Const(2050)); // Error message
        func.instruction(&wasm_encoder::Instruction::I32Const(16)); // Message length
        func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // End if-else (transfer result)
        func.instruction(&wasm_encoder::Instruction::End);
        
        // End if (authorization check)
        func.instruction(&wasm_encoder::Instruction::End);
        
        // Return status
        func.instruction(&wasm_encoder::Instruction::LocalGet(0));
        func.instruction(&wasm_encoder::Instruction::End);
        
        func
    }
    
    /// Generate a WASM function body for the default (fallback) function
    fn generate_default_function(&self, action: &str) -> wasm_encoder::Function {
        let mut func = wasm_encoder::Function::new([
            // Local variables: 
            // Local 0: Return value
            wasm_encoder::ValType::I32,
        ]);
        
        // Initialize return value to 0 (success)
        func.instruction(&wasm_encoder::Instruction::I32Const(0));
        func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Log that we're executing the action
        func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level INFO
        func.instruction(&wasm_encoder::Instruction::I32Const(0)); // Debug message
        func.instruction(&wasm_encoder::Instruction::I32Const(20)); // Message length (approximate)
        func.instruction(&wasm_encoder::Instruction::Call(0)); // host_log_message
        
        // Return success
        func.instruction(&wasm_encoder::Instruction::LocalGet(0));
        func.instruction(&wasm_encoder::Instruction::End);
        
        func
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