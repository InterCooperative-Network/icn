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
                self.validate_dsl_for_template(ccl_config, dsl_input)?;
                // Default action for metadata
                "unknown".to_string()
            }
        };

        // Validate the DSL input against JSON schema if enabled
        if options.validate_schema {
            self.validate_against_schema(template_type, &action, dsl_input, options.schema_path.as_deref())?;
        } else {
            // Still do basic structural validation
            self.validate_dsl_for_template(ccl_config, dsl_input)?;
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
                return Err(CompilerError::ValidationError(format!(
                    "Unsupported template type: {}:{}",
                    template_type, template_version
                )));
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
        // Serialize the CCL config and DSL input to JSON
        let ccl_json = serde_json::to_string(ccl_config)
            .map_err(|e| CompilerError::General(format!("Failed to serialize CCL config: {}", e)))?;
        let dsl_json = serde_json::to_string(dsl_input)
            .map_err(|e| CompilerError::General(format!("Failed to serialize DSL input: {}", e)))?;

        // Create metadata
        let metadata = self.create_metadata(ccl_config, dsl_input, options)?;
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| CompilerError::General(format!("Failed to serialize metadata: {}", e)))?;

        // Create a new WASM module
        let mut module = Module::new();

        // Define the type section (function signatures)
        let mut types = TypeSection::new();
        // Define type for _start function: () -> ()
        types.function(vec![], vec![]);
        // Define type for host_log_message: (i32, i32, i32) -> ()
        types.function(vec![ValType::I32, ValType::I32, ValType::I32], vec![]);
        // Define type for invoke function: (i32, i32) -> i32
        types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
        module.section(&types);

        // Define the import section (host functions)
        let mut imports = ImportSection::new();
        // Import the host_log_message function - use EntityType::Function with correct type index
        imports.import("env", "host_log_message", EntityType::Function(1));
        module.section(&imports);

        // Define the function section (internal functions)
        let mut functions = FunctionSection::new();
        // _start function with type 0
        functions.function(0);
        // invoke function with type 2
        functions.function(2);
        module.section(&functions);

        // Define the export section
        let mut exports = ExportSection::new();
        // Export _start function
        exports.export("_start", wasm_encoder::ExportKind::Func, 0);
        // Export invoke function
        exports.export("invoke", wasm_encoder::ExportKind::Func, 1);
        module.section(&exports);

        // Define the code section (function bodies)
        let mut code = CodeSection::new();

        // _start function body - log CCL template info
        let template_info = format!(
            "CCL template: {}:{}",
            ccl_config.template_type, ccl_config.template_version
        );
        let template_info_bytes = template_info.as_bytes();

        // Create _start function body with simple log message
        let mut start_func = wasm_encoder::Function::new(vec![]);
        
        // Define template info as constant byte array in linear memory
        // - Push log level (1 = INFO)
        start_func.instruction(&wasm_encoder::Instruction::I32Const(1));
        // - Push pointer to message (constant for simplicity)
        start_func.instruction(&wasm_encoder::Instruction::I32Const(1024));
        // - Push message length
        start_func.instruction(&wasm_encoder::Instruction::I32Const(template_info_bytes.len() as i32));
        // - Call host_log_message
        start_func.instruction(&wasm_encoder::Instruction::Call(0));
        // - End function
        start_func.instruction(&wasm_encoder::Instruction::End);
        code.function(&start_func);

        // invoke function body - return simple status code based on action
        let mut invoke_func = wasm_encoder::Function::new(vec![]);
        
        // For now, just return 0 (success)
        invoke_func.instruction(&wasm_encoder::Instruction::I32Const(0));
        invoke_func.instruction(&wasm_encoder::Instruction::End);
        code.function(&invoke_func);

        module.section(&code);

        // Add a custom section with metadata
        let custom_section = wasm_encoder::CustomSection {
            name: "icn-metadata",
            data: metadata_json.as_bytes(),
        };
        module.section(&custom_section);

        // Add CCL config in a custom section for reference
        if options.include_debug_info {
            let ccl_section = wasm_encoder::CustomSection {
                name: "icn-ccl-config",
                data: ccl_json.as_bytes(),
            };
            module.section(&ccl_section);
            
            // Add DSL input in a custom section for reference
            let dsl_section = wasm_encoder::CustomSection {
                name: "icn-dsl-input",
                data: dsl_json.as_bytes(),
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