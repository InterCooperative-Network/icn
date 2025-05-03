# ICN Wallet Error Handling Guide

This guide outlines principles and implementation strategies for robust error handling in the ICN Wallet to ensure graceful handling of failures and clear user feedback.

## Error Handling Principles

### 1. Fail Gracefully
- Anticipate potential failures
- Recover when possible
- Degrade functionality gracefully when necessary
- Preserve user data and state

### 2. Provide Clear Feedback
- Error messages should be clear and actionable
- Distinguish between user errors and system errors
- Suggest possible remedies
- Avoid technical jargon in user-facing messages

### 3. Log Comprehensively
- Include detailed context for debugging
- Log errors at appropriate levels
- Ensure security in error logging (no sensitive data)
- Include correlation IDs for tracing

### 4. Maintain Security
- Avoid revealing implementation details in user-facing errors
- Validate all input regardless of prior failures
- Handle authentication and authorization errors securely
- Maintain secure state even during errors

## Error Categories

### 1. User Input Errors
- Invalid format
- Missing required fields
- Constraint violations
- Permission issues

### 2. Network Errors
- Connection failures
- Timeouts
- Service unavailability
- Protocol errors

### 3. Integration Errors
- AgoraNet API failures
- Federation sync issues
- Runtime execution failures
- Format incompatibilities

### 4. Resource Errors
- File system errors
- Memory limitations
- Database errors
- Concurrency issues

### 5. Security Errors
- Authentication failures
- Authorization failures
- Signature verification errors
- Encryption/decryption failures

## Implementation Strategies

### Core Error Types

Create a structured error hierarchy:

```rust
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("Input validation error: {0}")]
    ValidationError(String),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] NetworkError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("Integration error: {0}")]
    IntegrationError(#[from] IntegrationError),
    
    #[error("Security error: {0}")]
    SecurityError(#[from] SecurityError),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("Request timeout after {0} seconds")]
    Timeout(u64),
    
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    
    #[error("Protocol error: {0}")]
    ProtocolError(String),
}

// Similar enums for other error categories
```

### API Error Responses

Standardize API error responses:

```rust
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
    pub request_id: String,
    pub timestamp: String,
}

impl From<WalletError> for ApiErrorResponse {
    fn from(error: WalletError) -> Self {
        let (code, message, details) = match &error {
            WalletError::ValidationError(msg) => (
                "VALIDATION_ERROR",
                "Input validation failed",
                Some(msg.clone()),
            ),
            WalletError::NetworkError(network_err) => match network_err {
                NetworkError::ConnectionFailed(host) => (
                    "CONNECTION_FAILED",
                    "Failed to connect to service",
                    Some(format!("Could not reach {}", host)),
                ),
                NetworkError::Timeout(seconds) => (
                    "REQUEST_TIMEOUT",
                    "Request timed out",
                    Some(format!("Request exceeded timeout of {} seconds", seconds)),
                ),
                // ... other network error mappings
            },
            // ... other error type mappings
            _ => (
                "INTERNAL_ERROR",
                "An unexpected error occurred",
                None,
            ),
        };
        
        Self {
            code: code.to_string(),
            message: message.to_string(),
            details,
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}
```

### Context-Rich Errors

Use `anyhow` with context for internal error handling:

```rust
use anyhow::{Context, Result};

fn fetch_trust_bundle(id: &str) -> Result<TrustBundle> {
    let path = get_bundle_path(id)
        .context("Failed to determine trust bundle path")?;
        
    let data = std::fs::read_to_string(&path)
        .context(format!("Failed to read trust bundle from {}", path.display()))?;
        
    let bundle: TrustBundle = serde_json::from_str(&data)
        .context("Failed to parse trust bundle JSON")?;
        
    Ok(bundle)
}
```

### Network Resilience

Implement robust network error handling:

```rust
async fn fetch_with_retry<T, F>(
    operation: F,
    retries: usize,
    backoff_ms: u64,
) -> Result<T, NetworkError>
where
    F: Fn() -> Future<Output = Result<T, NetworkError>> + Clone,
{
    let mut attempt = 0;
    let mut delay_ms = backoff_ms;
    
    loop {
        attempt += 1;
        
        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                if attempt > retries {
                    return Err(err);
                }
                
                // Only retry certain errors
                match &err {
                    NetworkError::ConnectionFailed(_) |
                    NetworkError::Timeout(_) |
                    NetworkError::ServiceUnavailable(_) => {
                        // Log retry attempt
                        tracing::warn!("Retry {}/{} after error: {}", attempt, retries, err);
                        
                        // Exponential backoff
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        delay_ms *= 2;
                    },
                    _ => return Err(err), // Don't retry other errors
                }
            }
        }
    }
}
```

### Validation Framework

Implement comprehensive input validation:

```rust
struct ValidationError {
    field: String,
    message: String,
}

type ValidationResult<T> = std::result::Result<T, Vec<ValidationError>>;

trait Validate {
    fn validate(&self) -> ValidationResult<()>;
}

impl Validate for CreateIdentityRequest {
    fn validate(&self) -> ValidationResult<()> {
        let mut errors = Vec::new();
        
        // Validate scope
        match self.scope.as_str() {
            "personal" | "organization" | "device" | "service" => {},
            scope if !scope.is_empty() => { /* Custom scope - no validation */ },
            _ => errors.push(ValidationError {
                field: "scope".to_string(),
                message: "Scope is required".to_string(),
            }),
        }
        
        // Validate metadata if present
        if let Some(ref metadata) = self.metadata {
            // Perform metadata validation
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// In request handler
fn handle_create_identity(request: CreateIdentityRequest) -> Result<IdentityResponse> {
    // Validate request
    request.validate()
        .map_err(|errs| {
            let messages = errs.iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join(", ");
            WalletError::ValidationError(messages)
        })?;
        
    // Process valid request
    // ...
}
```

### Integration Fallbacks

Implement fallbacks for integration failures:

```rust
async fn sync_trust_bundles() -> Result<Vec<TrustBundle>> {
    // Try network sync first
    match sync_from_network().await {
        Ok(bundles) => {
            // Store bundles locally for fallback
            store_bundles_locally(&bundles)?;
            Ok(bundles)
        },
        Err(err) => {
            tracing::warn!("Network sync failed: {}", err);
            
            // Fallback to local storage
            tracing::info!("Falling back to local trust bundles");
            let local_bundles = load_bundles_from_disk()?;
            
            if local_bundles.is_empty() {
                // No fallback available
                Err(err.into())
            } else {
                tracing::info!("Loaded {} local trust bundles as fallback", local_bundles.len());
                Ok(local_bundles)
            }
        }
    }
}
```

### State Recovery

Implement state recovery mechanisms:

```rust
fn recover_wallet_state() -> Result<WalletState> {
    // Try loading the primary state file
    match load_state_from_primary() {
        Ok(state) => {
            tracing::info!("Loaded wallet state from primary storage");
            return Ok(state);
        },
        Err(primary_err) => {
            tracing::warn!("Failed to load from primary storage: {}", primary_err);
            
            // Try loading from backup
            match load_state_from_backup() {
                Ok(state) => {
                    tracing::info!("Loaded wallet state from backup storage");
                    
                    // Restore primary from backup
                    if let Err(restore_err) = restore_primary_from_backup() {
                        tracing::error!("Failed to restore primary storage: {}", restore_err);
                    }
                    
                    return Ok(state);
                },
                Err(backup_err) => {
                    tracing::error!("Failed to load from backup storage: {}", backup_err);
                    
                    // Last resort: initialize a new state
                    tracing::warn!("Initializing new wallet state due to recovery failure");
                    let new_state = WalletState::new();
                    
                    // Save the new state
                    if let Err(save_err) = save_state_to_primary(&new_state) {
                        tracing::error!("Failed to save new state: {}", save_err);
                    }
                    
                    return Ok(new_state);
                }
            }
        }
    }
}
```

## API Error Handling Improvements

### 1. Error Response Middleware

Implement middleware to standardize error responses:

```rust
async fn error_handling_middleware<B>(
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    // Process the request
    let response = next.run(request).await;
    
    // Check if it's an error response
    if response.status().is_client_error() || response.status().is_server_error() {
        let body = response.into_body();
        let bytes = to_bytes(body).await.unwrap_or_default();
        
        // Try to parse as ApiErrorResponse
        if let Ok(api_error) = serde_json::from_slice::<ApiErrorResponse>(&bytes) {
            // Already a structured error, return as is
            return Ok(Response::builder()
                .status(response.status())
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&api_error).unwrap()))
                .unwrap());
        }
        
        // Create a properly structured error
        let error = ApiErrorResponse {
            code: format!("HTTP_{}", response.status().as_u16()),
            message: response.status().to_string(),
            details: std::str::from_utf8(&bytes).ok().map(String::from),
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        
        return Ok(Response::builder()
            .status(response.status())
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&error).unwrap()))
            .unwrap());
    }
    
    Ok(response)
}
```

### 2. Specific Status Codes

Map error types to appropriate HTTP status codes:

```rust
impl WalletError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            WalletError::ValidationError(_) => StatusCode::BAD_REQUEST,
            WalletError::NetworkError(network_err) => match network_err {
                NetworkError::ConnectionFailed(_) => StatusCode::BAD_GATEWAY,
                NetworkError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
                NetworkError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
                NetworkError::ProtocolError(_) => StatusCode::BAD_GATEWAY,
            },
            WalletError::StorageError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            WalletError::IntegrationError(_) => StatusCode::BAD_GATEWAY,
            WalletError::SecurityError(_) => StatusCode::FORBIDDEN,
            WalletError::ConfigError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            WalletError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
```

### 3. User-Friendly Messages

Provide user-friendly error messages:

```rust
impl WalletError {
    pub fn user_message(&self) -> String {
        match self {
            WalletError::ValidationError(msg) => {
                format!("Please check your input: {}", msg)
            },
            WalletError::NetworkError(network_err) => match network_err {
                NetworkError::ConnectionFailed(_) => {
                    "Unable to connect to the service. Please check your network connection and try again.".to_string()
                },
                NetworkError::Timeout(_) => {
                    "The request took too long to complete. Please try again later.".to_string()
                },
                NetworkError::ServiceUnavailable(service) => {
                    format!("{} is currently unavailable. Please try again later.", service)
                },
                NetworkError::ProtocolError(_) => {
                    "Communication error with the service. Please try again or contact support.".to_string()
                },
            },
            // ... other error types
            _ => "An unexpected error occurred. Please try again or contact support.".to_string(),
        }
    }
}
```

### 4. Recovery Suggestions

Include recovery suggestions in error responses:

```rust
impl WalletError {
    pub fn recovery_suggestion(&self) -> Option<String> {
        match self {
            WalletError::ValidationError(_) => Some(
                "Please review the input requirements and try again.".to_string()
            ),
            WalletError::NetworkError(network_err) => match network_err {
                NetworkError::ConnectionFailed(_) => Some(
                    "Check your internet connection and verify the service is running.".to_string()
                ),
                NetworkError::Timeout(_) => Some(
                    "The service might be experiencing high load. Wait a moment and try again.".to_string()
                ),
                // ... other network errors
            },
            // ... other error types
            _ => None,
        }
    }
}
```

## Error Handling in CLI

### 1. Structured Error Output

Improve CLI error reporting:

```rust
fn handle_cli_error(err: &anyhow::Error) {
    let error_chain = err.chain().collect::<Vec<_>>();
    
    if atty::is(atty::Stream::Stderr) {
        // Interactive terminal - use colors and formatting
        eprintln!("{}: {}", "ERROR".red().bold(), err);
        
        if error_chain.len() > 1 {
            eprintln!("\n{}:", "CAUSED BY".yellow().bold());
            for (i, e) in error_chain.iter().skip(1).enumerate() {
                eprintln!("  {}: {}", (i + 1).to_string().yellow(), e);
            }
        }
        
        // Try to get recovery suggestion
        if let Some(wallet_err) = err.downcast_ref::<WalletError>() {
            if let Some(suggestion) = wallet_err.recovery_suggestion() {
                eprintln!("\n{}: {}", "SUGGESTION".green().bold(), suggestion);
            }
        }
    } else {
        // Non-interactive - plain output
        eprintln!("ERROR: {}", err);
        
        if error_chain.len() > 1 {
            eprintln!("CAUSED BY:");
            for (i, e) in error_chain.iter().skip(1).enumerate() {
                eprintln!("  {}: {}", i + 1, e);
            }
        }
    }
}
```

### 2. Status Indicators

Add clear status indicators for CLI operations:

```rust
fn run_cli_command(cmd: &Command) -> Result<(), anyhow::Error> {
    println!("{} {}...", "→".blue(), cmd.description());
    
    match cmd.execute() {
        Ok(result) => {
            println!("{} {}", "✓".green(), result.success_message());
            if let Some(details) = result.details() {
                println!("{}", details);
            }
            Ok(())
        },
        Err(err) => {
            print!("{} ", "✗".red());
            handle_cli_error(&err);
            Err(err)
        }
    }
}
```

## Implementation Checklist

### 1. Error Types
- [ ] Create structured error hierarchy
- [ ] Implement error conversion traits
- [ ] Add user-friendly messages
- [ ] Add recovery suggestions

### 2. Network Resilience
- [ ] Implement retry mechanisms
- [ ] Add timeout handling
- [ ] Create circuit breakers for external services
- [ ] Implement fallbacks where appropriate

### 3. State Recovery
- [ ] Add state backup mechanisms
- [ ] Implement corruption detection
- [ ] Create recovery procedures
- [ ] Test recovery scenarios

### 4. Validation
- [ ] Implement comprehensive input validation
- [ ] Create validation framework for reuse
- [ ] Add detailed validation error messages
- [ ] Validate at all trust boundaries

### 5. API Error Responses
- [ ] Standardize error response format
- [ ] Add error middleware
- [ ] Map errors to appropriate status codes
- [ ] Include recovery suggestions

### 6. CLI Error Handling
- [ ] Improve error display
- [ ] Add color and formatting
- [ ] Include detailed error chains
- [ ] Add status indicators for operations

### 7. Logging
- [ ] Ensure comprehensive error logging
- [ ] Add context to log messages
- [ ] Include correlation IDs
- [ ] Filter sensitive information 