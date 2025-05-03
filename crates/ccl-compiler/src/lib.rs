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
                    // Add action-specific validations
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
        
        module.section(&types);
        
        // Step 4: Define import section - host functions
        let mut imports = ImportSection::new();
        
        // Import host_log_message from env module
        imports.import("env", "host_log_message", EntityType::Function(1));
        
        // Import storage functions from env module
        imports.import("env", "host_storage_get", EntityType::Function(3));
        imports.import("env", "host_storage_put", EntityType::Function(4));
        
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
        
        // For store_data or get_data action, embed key_cid in data section
        if action == "store_data" || action == "get_data" {
            // Store key_cid at memory offset 2048
            let key_cid_bytes = key_cid_str.as_bytes();
            data_section.active(
                0, // Memory index
                &wasm_encoder::ConstExpr::i32_const(2048), // Offset for key_cid
                key_cid_bytes.iter().copied(),
            );
            
            // For store_data action, also embed the value
            if action == "store_data" {
                // Store value at memory offset 4096
                data_section.active(
                    0, // Memory index
                    &wasm_encoder::ConstExpr::i32_const(4096), // Offset for value
                    value_bytes.iter().copied(),
                );
            }
        }
        
        module.section(&data_section);
        
        // Step 9: Define code section - function bodies
        let mut code = CodeSection::new();
        
        // Create _start function body
        // This function logs the template info on module initialization
        let mut start_func = wasm_encoder::Function::new(vec![]);
        
        // Log message at INFO level (1)
        start_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        start_func.instruction(&wasm_encoder::Instruction::I32Const(1024)); // Pointer to message
        start_func.instruction(&wasm_encoder::Instruction::I32Const(template_info_bytes.len() as i32)); // Message length
        start_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message (first imported function)
        start_func.instruction(&wasm_encoder::Instruction::End);
        
        code.function(&start_func);
        
        // Create invoke function body
        // This function is called by the runtime to execute the governance action
        let mut invoke_func = wasm_encoder::Function::new(vec![
            // Local variables - format is (count, type)
            (2, wasm_encoder::ValType::I32), // Local 0: Status code, Local 1: Result code from host calls
        ]);
        
        // Store default return value (failure = 1, will be set to 0 on success)
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1));
        invoke_func.instruction(&wasm_encoder::Instruction::LocalSet(0));
        
        // Log that we're starting execution
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1)); // Log level (INFO)
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(1024)); // Pointer to message
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(template_info_bytes.len() as i32)); // Message length
        invoke_func.instruction(&wasm_encoder::Instruction::Call(0)); // Call host_log_message
        
        // Add execution logic based on action type
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
        } else {
            // Default logic for unknown actions
            // Return "not implemented" status (1) - already set as default
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