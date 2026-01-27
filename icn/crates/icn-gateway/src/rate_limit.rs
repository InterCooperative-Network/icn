//! Per-DID rate limiting using token bucket algorithm
//!
//! Prevents abuse by limiting the number of requests per DID per time window.
//! Uses a token bucket algorithm:
//! - Each DID gets a bucket with max capacity
//! - Buckets refill at a constant rate
//! - Requests consume tokens
//! - Reject requests when bucket is empty
//!
//! ## Endpoint Categories
//!
//! Rate limits are applied per-category to balance security and usability:
//! - **Read**: High limits for read-only operations (200 req/min)
//! - **Write**: Moderate limits for state mutations (60 req/min)
//! - **Governance**: Lower limits for voting/proposals (30 req/min)
//! - **Compute**: Very low limits for compute tasks (10 req/min)
//! - **Auth**: IP-based limits for authentication (120 req/min per IP)

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use actix_web::{dev::ServiceRequest, Error, HttpMessage};
use tracing::{error, warn};

use crate::auth::TokenClaims;
use crate::error::GatewayError;
use icn_kernel_api::{ActionKind, Domain, PolicyDecision, PolicyOracle, PolicyRequest};
use icn_obs::metrics::gateway;

// Local definitions tailored for Gateway needs
// (IpRateLimiter, VelocityLimiter, etc. are defined below)

/// Endpoint category for differentiated rate limiting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointCategory {
    /// Read-only operations (GET requests, listing)
    /// High limits: 200 burst, 20 req/sec sustained
    Read,
    /// State-modifying writes (POST/PUT/DELETE)
    /// Moderate limits: 60 burst, 6 req/sec sustained
    Write,
    /// Governance operations (voting, proposals, amendments)
    /// Lower limits: 30 burst, 3 req/sec sustained
    Governance,
    /// Compute-intensive operations (task submission, queries)
    /// Very low limits: 10 burst, 1 req/sec sustained
    Compute,
}

impl EndpointCategory {
    /// Get the rate limit configuration for this category
    pub fn config(&self) -> RateLimitConfig {
        match self {
            EndpointCategory::Read => RateLimitConfig {
                capacity: 200.0,
                refill_rate: 20.0, // 20 req/sec = 1200/min sustained
                cost_per_request: 1.0,
            },
            EndpointCategory::Write => RateLimitConfig {
                capacity: 60.0,
                refill_rate: 6.0, // 6 req/sec = 360/min sustained
                cost_per_request: 1.0,
            },
            EndpointCategory::Governance => RateLimitConfig {
                capacity: 30.0,
                refill_rate: 3.0, // 3 req/sec = 180/min sustained
                cost_per_request: 1.0,
            },
            EndpointCategory::Compute => RateLimitConfig {
                capacity: 10.0,
                refill_rate: 1.0, // 1 req/sec = 60/min sustained
                cost_per_request: 1.0,
            },
        }
    }

    /// Determine endpoint category from request path and method
    pub fn from_request(path: &str, method: &str) -> Self {
        // Compute endpoints - strictest limits
        if path.contains("/compute") {
            return EndpointCategory::Compute;
        }

        // Governance endpoints
        if path.contains("/governance")
            || path.contains("/amendments")
            || path.contains("/appeals")
            || path.contains("/proposals")
            || path.contains("/vote")
        {
            return EndpointCategory::Governance;
        }

        // Charter/membership operations are governance too
        if path.contains("/charter") || path.contains("/membership") {
            if method == "GET" {
                return EndpointCategory::Read;
            }
            return EndpointCategory::Governance;
        }

        // Steward operations
        if path.contains("/steward") {
            if method == "GET" {
                return EndpointCategory::Read;
            }
            return EndpointCategory::Governance;
        }

        // Method-based categorization for other endpoints
        match method {
            "GET" | "HEAD" | "OPTIONS" => EndpointCategory::Read,
            _ => EndpointCategory::Write,
        }
    }
}

/// Token bucket for a single DID
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        // Add tokens based on elapsed time
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
    }

    /// Tolerance for floating point comparison (1e-6 is appropriate for user-configured values)
    const CONFIG_EPSILON: f64 = 1e-6;

    /// Update bucket configuration with proportional refill
    fn update_config(&mut self, capacity: f64, refill_rate: f64) {
        if (self.capacity - capacity).abs() > Self::CONFIG_EPSILON
            || (self.refill_rate - refill_rate).abs() > Self::CONFIG_EPSILON
        {
            // Proportional refill: scale tokens by the capacity ratio to prevent
            // exploitation via rapid trust cycling (repeatedly getting full buckets)
            let ratio = capacity / self.capacity.max(1.0);
            self.tokens = (self.tokens * ratio).min(capacity);
            self.capacity = capacity;
            self.refill_rate = refill_rate;
            self.last_refill = Instant::now();
        }
    }

    /// Try to consume tokens for a request
    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Get current token count (for metrics/debugging)
    fn available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }
}

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum tokens in bucket (burst capacity)
    pub capacity: f64,
    /// Tokens refilled per second (sustained rate)
    pub refill_rate: f64,
    /// Tokens consumed per request
    pub cost_per_request: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            capacity: 100.0,       // Allow burst of 100 requests
            refill_rate: 10.0,     // Refill 10 tokens/second (600/minute)
            cost_per_request: 1.0, // Each request costs 1 token
        }
    }
}

/// Rate limiter tracking buckets per DID
pub struct RateLimiter {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Check if request should be allowed for a DID
    pub fn check_rate_limit(&self, did: &str) -> Result<(), GatewayError> {
        let mut buckets = self.buckets.write().map_err(|e| {
            error!("CRITICAL: Rate limiter lock poisoned - indicates panic in another thread: {e}");
            GatewayError::InternalError(format!("Lock poisoned: {e}"))
        })?;

        let bucket = buckets
            .entry(did.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.capacity, self.config.refill_rate));

        if bucket.try_consume(self.config.cost_per_request) {
            Ok(())
        } else {
            let available = bucket.available();
            warn!(
                "Rate limit exceeded for DID: {} (available: {:.2}, needed: {:.2})",
                did, available, self.config.cost_per_request
            );
            // Track rate limit exceeded
            gateway::rate_limit_exceeded_inc(did);
            Err(GatewayError::RateLimitExceeded(did.to_string()))
        }
    }

    /// Check if request should be allowed for a DID with a specific policy
    pub fn check_rate_limit_with_policy(
        &self,
        did: &str,
        config: &RateLimitConfig,
    ) -> Result<(), GatewayError> {
        let mut buckets = self.buckets.write().map_err(|e| {
            error!("CRITICAL: Rate limiter lock poisoned - indicates panic in another thread: {e}");
            GatewayError::InternalError(format!("Lock poisoned: {e}"))
        })?;

        let bucket = buckets
            .entry(did.to_string())
            .or_insert_with(|| TokenBucket::new(config.capacity, config.refill_rate));

        // Update bucket config if policy changed (e.g., trust upgrade)
        bucket.update_config(config.capacity, config.refill_rate);

        if bucket.try_consume(config.cost_per_request) {
            Ok(())
        } else {
            let available = bucket.available();
            warn!(
                "Policy rate limit exceeded for DID: {} (available: {:.2}, needed: {:.2})",
                did, available, config.cost_per_request
            );
            // Track rate limit exceeded
            gateway::rate_limit_exceeded_inc(did);
            Err(GatewayError::RateLimitExceeded(did.to_string()))
        }
    }

    /// Get current token count for a DID (for debugging/metrics)
    pub fn get_available_tokens(&self, did: &str) -> Option<f64> {
        let mut buckets = self.buckets.write().ok()?;
        buckets.get_mut(did).map(|b| b.available())
    }

    /// Clean up old buckets to prevent unbounded memory growth
    pub fn cleanup_inactive_buckets(&self, inactive_duration: Duration) -> usize {
        let mut buckets = match self.buckets.write() {
            Ok(b) => b,
            Err(_) => return 0,
        };

        let now = Instant::now();
        let initial_count = buckets.len();

        buckets.retain(|_, bucket| {
            let elapsed = now.duration_since(bucket.last_refill);
            elapsed < inactive_duration
        });

        initial_count - buckets.len()
    }
}

/// Per-category rate limiter tracking buckets per (DID, category) pair
///
/// This allows different endpoint types to have different rate limits:
/// - Read operations get high limits for responsive UX
/// - Write operations get moderate limits
/// - Governance operations get lower limits to prevent spam
/// - Compute operations get very low limits due to resource intensity
pub struct CategoryRateLimiter {
    /// Buckets keyed by "did:category"
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

impl CategoryRateLimiter {
    /// Create a new category-aware rate limiter
    pub fn new() -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Build bucket key from DID and category
    fn bucket_key(did: &str, category: EndpointCategory) -> String {
        format!("{did}:{category:?}")
    }

    /// Check if request should be allowed for a DID and category
    pub fn check_rate_limit(
        &self,
        did: &str,
        category: EndpointCategory,
    ) -> Result<(), GatewayError> {
        let config = category.config();
        let key = Self::bucket_key(did, category);

        let mut buckets = self.buckets.write().map_err(|e| {
            error!("CRITICAL: Rate limiter lock poisoned - indicates panic in another thread: {e}");
            GatewayError::InternalError(format!("Lock poisoned: {e}"))
        })?;

        let bucket = buckets
            .entry(key.clone())
            .or_insert_with(|| TokenBucket::new(config.capacity, config.refill_rate));

        if bucket.try_consume(config.cost_per_request) {
            Ok(())
        } else {
            let available = bucket.available();
            warn!(
                "Rate limit exceeded for DID {} on {:?} (available: {:.2}, needed: {:.2})",
                did, category, available, config.cost_per_request
            );
            // Track rate limit exceeded with category
            gateway::rate_limit_exceeded_inc(&format!("{did}:{category:?}"));
            Err(GatewayError::RateLimitExceeded(format!(
                "{did} ({category:?})"
            )))
        }
    }

    /// Check rate limit using request path and method to determine category
    pub fn check_rate_limit_for_request(
        &self,
        did: &str,
        path: &str,
        method: &str,
    ) -> Result<(), GatewayError> {
        let category = EndpointCategory::from_request(path, method);
        self.check_rate_limit(did, category)
    }

    /// Get current token count for a DID and category (for debugging/metrics)
    pub fn get_available_tokens(&self, did: &str, category: EndpointCategory) -> Option<f64> {
        let key = Self::bucket_key(did, category);
        let mut buckets = self.buckets.write().ok()?;
        buckets.get_mut(&key).map(|b| b.available())
    }

    /// Clean up old buckets to prevent unbounded memory growth
    pub fn cleanup_inactive_buckets(&self, inactive_duration: Duration) -> usize {
        let mut buckets = match self.buckets.write() {
            Ok(b) => b,
            Err(_) => return 0,
        };

        let now = Instant::now();
        let initial_count = buckets.len();

        buckets.retain(|_, bucket| {
            let elapsed = now.duration_since(bucket.last_refill);
            elapsed < inactive_duration
        });

        initial_count - buckets.len()
    }
}

impl Default for CategoryRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// IP-based rate limiter for unauthenticated endpoints (auth endpoints)
///
/// Uses more aggressive limits to prevent DoS attacks. This limiter is used
/// for endpoints that don't have DID-based authentication yet.
///
/// # Security Limitation: IP Rotation
///
/// **Important**: Attackers can bypass IP-based rate limiting via IP rotation
/// (e.g., using botnets, cloud instances, or Tor exit nodes). This limiter
/// provides basic DoS mitigation but should not be relied upon as the sole
/// defense. For authenticated endpoints, use DID-based rate limiting with
/// personhood anchors for proper Sybil resistance.
pub struct IpRateLimiter {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    config: RateLimitConfig,
}

impl IpRateLimiter {
    /// Create a new IP-based rate limiter with aggressive limits
    pub fn new_for_auth() -> Self {
        // More aggressive limits for unauthenticated endpoints
        let config = RateLimitConfig {
            capacity: 20.0,   // Allow burst of 20 requests
            refill_rate: 2.0, // Refill 2 tokens/second (120/minute)
            cost_per_request: 1.0,
        };

        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Create a new IP-based rate limiter with custom config (for testing)
    #[cfg(test)]
    pub fn new_with_config(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Check if request should be allowed for an IP address
    pub fn check_rate_limit(&self, ip: &str) -> Result<(), GatewayError> {
        let mut buckets = self.buckets.write().map_err(|e| {
            error!("CRITICAL: Rate limiter lock poisoned - indicates panic in another thread: {e}");
            GatewayError::InternalError(format!("Lock poisoned: {e}"))
        })?;

        let bucket = buckets
            .entry(ip.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.capacity, self.config.refill_rate));

        if bucket.try_consume(self.config.cost_per_request) {
            Ok(())
        } else {
            let available = bucket.available();
            warn!(
                "IP rate limit exceeded for: {} (available: {:.2}, needed: {:.2})",
                ip, available, self.config.cost_per_request
            );
            gateway::ip_rate_limit_exceeded_inc();
            Err(GatewayError::RateLimitExceeded(format!("IP: {ip}")))
        }
    }

    /// Clean up old buckets to prevent unbounded memory growth
    pub fn cleanup_inactive_buckets(&self, inactive_duration: Duration) -> usize {
        let mut buckets = match self.buckets.write() {
            Ok(b) => b,
            Err(_) => return 0,
        };

        let now = Instant::now();
        let initial_count = buckets.len();

        buckets.retain(|_, bucket| {
            let elapsed = now.duration_since(bucket.last_refill);
            elapsed < inactive_duration
        });

        initial_count - buckets.len()
    }
}

// ============================================================================
// Trust-Gated Rate Limiter (Issue #500)
// ============================================================================

/// Trust-gated rate limiting middleware
///
/// Applies policy-based rate limits using the PolicyOracle.
/// If an oracle is not available, falls back to regular rate limiting.
pub async fn trust_rate_limit_middleware(
    req: ServiceRequest,
    next: actix_web::middleware::Next<actix_web::body::BoxBody>,
) -> Result<actix_web::dev::ServiceResponse, Error> {
    // Get PolicyOracle and RateLimiter from app data
    // Note: Use weak dependency on oracle to allow running without it (fallback to standard limits)
    let oracle = req.app_data::<actix_web::web::Data<Arc<dyn PolicyOracle>>>();
    let rate_limiter = req.app_data::<actix_web::web::Data<Arc<RateLimiter>>>();

    let (oracle, rate_limiter) = match (oracle, rate_limiter) {
        (Some(o), Some(rl)) => (o.get_ref().clone(), rl.get_ref().clone()),
        _ => return rate_limit_middleware(req, next).await,
    };

    // Get DID from token claims (inserted by jwt_auth middleware)
    let did_opt = {
        let extensions = req.extensions();
        extensions.get::<TokenClaims>().map(|c| c.sub.clone())
    };

    let did = match did_opt {
        Some(d) => d,
        None => {
            // No claims means unauthenticated request
            // Use synthetic anonymous DID with IP suffix (treated as Isolated/Default)
            let ip = req
                .connection_info()
                .peer_addr()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            format!("did:icn:anonymous:{ip}")
        }
    };

    // Create policy request
    // We use a custom action "network_access" and the "trust" domain
    let policy_req = PolicyRequest::new(
        did.clone(),
        ActionKind::Custom("network_access".to_string()),
        Domain::trust(),
    );

    // Evaluate policy
    let decision = oracle.evaluate(&policy_req);

    match decision {
        PolicyDecision::Allow { constraints } => {
            // Extract rate limit from constraints
            let config = if let Some(limit) = constraints.rate_limit {
                RateLimitConfig {
                    capacity: limit.burst_size as f64,
                    refill_rate: limit.messages_per_second as f64,
                    cost_per_request: 1.0,
                }
            } else {
                // Default if no specific limit in constraints
                RateLimitConfig {
                    capacity: 10.0,
                    refill_rate: 2.0,
                    cost_per_request: 1.0,
                }
            };

            // Enforce rate limit
            match rate_limiter.check_rate_limit_with_policy(&did, &config) {
                Ok(()) => next.call(req).await,
                Err(e) => Err(Error::from(e)),
            }
        }
        PolicyDecision::Deny { reason } => {
            warn!("Access denied by policy for DID {}: {}", did, reason);
            Err(Error::from(GatewayError::Forbidden(reason.to_string())))
        }
    }
}

/// Rate limiting middleware for authenticated requests (legacy, uniform limits)
pub async fn rate_limit_middleware(
    req: ServiceRequest,
    next: actix_web::middleware::Next<actix_web::body::BoxBody>,
) -> Result<actix_web::dev::ServiceResponse, Error> {
    // Get rate limiter from app data
    let rate_limiter = match req.app_data::<actix_web::web::Data<Arc<RateLimiter>>>() {
        Some(limiter) => limiter.get_ref().clone(),
        None => {
            // Rate limiter not configured - allow request
            return next.call(req).await;
        }
    };

    // Get DID from token claims (inserted by jwt_auth middleware)
    // Need to clone the DID before borrowing ends
    let did_opt = {
        let extensions = req.extensions();
        extensions.get::<TokenClaims>().map(|c| c.sub.clone())
    };

    let did = match did_opt {
        Some(d) => d,
        None => {
            // No claims means unauthenticated request (public endpoint)
            // Don't rate limit public endpoints
            return next.call(req).await;
        }
    };

    // Check rate limit
    match rate_limiter.check_rate_limit(&did) {
        Ok(()) => next.call(req).await,
        Err(e) => Err(Error::from(e)),
    }
}

/// Category-aware rate limiting middleware for authenticated requests
///
/// This middleware applies different rate limits based on endpoint category:
/// - Read: High limits (200 burst, 20/sec)
/// - Write: Moderate limits (60 burst, 6/sec)
/// - Governance: Lower limits (30 burst, 3/sec)
/// - Compute: Very low limits (10 burst, 1/sec)
pub async fn category_rate_limit_middleware(
    req: ServiceRequest,
    next: actix_web::middleware::Next<actix_web::body::BoxBody>,
) -> Result<actix_web::dev::ServiceResponse, Error> {
    // Get category rate limiter from app data
    let rate_limiter = match req.app_data::<actix_web::web::Data<Arc<CategoryRateLimiter>>>() {
        Some(limiter) => limiter.get_ref().clone(),
        None => {
            // Rate limiter not configured - allow request
            return next.call(req).await;
        }
    };

    // Get DID from token claims (inserted by jwt_auth middleware)
    let did_opt = {
        let extensions = req.extensions();
        extensions.get::<TokenClaims>().map(|c| c.sub.clone())
    };

    let did = match did_opt {
        Some(d) => d,
        None => {
            // No claims means unauthenticated request (public endpoint)
            // Don't rate limit public endpoints
            return next.call(req).await;
        }
    };

    // Get request path and method for category determination
    let path = req.path().to_string();
    let method = req.method().as_str().to_string();

    // Check rate limit for this category
    match rate_limiter.check_rate_limit_for_request(&did, &path, &method) {
        Ok(()) => next.call(req).await,
        Err(e) => Err(Error::from(e)),
    }
}

// ============================================================================
// Transaction Velocity Limiter (Issue #164)
// ============================================================================

/// Configuration for transaction velocity limits per trust class
#[derive(Debug, Clone)]
pub struct VelocityLimitConfig {
    /// Maximum transactions per hour for isolated/unknown DIDs (trust < 0.1)
    pub isolated_limit: u32,
    /// Maximum transactions per hour for known DIDs (trust 0.1-0.4)
    pub known_limit: u32,
    /// Maximum transactions per hour for partner DIDs (trust 0.4-0.7)
    pub partner_limit: u32,
    /// Maximum transactions per hour for federated DIDs (trust > 0.7)
    pub federated_limit: u32,
    /// Time window for counting transactions
    pub window: Duration,
}

impl Default for VelocityLimitConfig {
    fn default() -> Self {
        Self {
            isolated_limit: 10,                // Very conservative for unknown users
            known_limit: 50,                   // Moderate for known users
            partner_limit: 100,                // Higher for trusted partners
            federated_limit: 200,              // Highest for federated nodes
            window: Duration::from_secs(3600), // 1 hour window
        }
    }
}

impl VelocityLimitConfig {
    /// Get the transaction limit based on trust score
    pub fn limit_for_trust(&self, trust_score: f64) -> u32 {
        if trust_score >= 0.7 {
            self.federated_limit
        } else if trust_score >= 0.4 {
            self.partner_limit
        } else if trust_score >= 0.1 {
            self.known_limit
        } else {
            self.isolated_limit
        }
    }
}

/// Record of transactions within a time window
#[derive(Debug, Clone)]
struct TransactionWindow {
    /// Transaction timestamps within the window
    timestamps: Vec<Instant>,
    /// Last cleanup time
    last_cleanup: Instant,
}

impl TransactionWindow {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
            last_cleanup: Instant::now(),
        }
    }

    /// Record a new transaction, cleaning up old entries
    fn record(&mut self, window: Duration) {
        let now = Instant::now();
        let cutoff = now - window;

        // Cleanup old timestamps periodically
        if now.duration_since(self.last_cleanup) > Duration::from_secs(60) {
            self.timestamps.retain(|t| *t > cutoff);
            self.last_cleanup = now;
        }

        self.timestamps.push(now);
    }

    /// Count transactions within the window
    fn count(&self, window: Duration) -> u32 {
        let cutoff = Instant::now() - window;
        self.timestamps.iter().filter(|t| **t > cutoff).count() as u32
    }
}

/// Transaction velocity limiter - limits transactions per DID per time window
#[derive(Debug, Clone)]
pub struct VelocityLimiter {
    config: VelocityLimitConfig,
    windows: Arc<RwLock<HashMap<String, TransactionWindow>>>,
}

impl VelocityLimiter {
    /// Create a new velocity limiter with the given configuration
    pub fn new(config: VelocityLimitConfig) -> Self {
        Self {
            config,
            windows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a transaction is allowed, and record it if so
    ///
    /// # Arguments
    /// * `did` - The DID making the transaction
    /// * `trust_score` - Trust score of the DID (0.0 - 1.0)
    ///
    /// # Returns
    /// * `Ok(())` if transaction is allowed
    /// * `Err(GatewayError)` if transaction exceeds velocity limit
    pub fn check_and_record(&self, did: &str, trust_score: f64) -> Result<(), GatewayError> {
        let limit = self.config.limit_for_trust(trust_score);

        let mut windows = self
            .windows
            .write()
            .map_err(|_| GatewayError::InternalError("Failed to acquire lock".to_string()))?;

        let window = windows
            .entry(did.to_string())
            .or_insert_with(TransactionWindow::new);

        let current_count = window.count(self.config.window);

        if current_count >= limit {
            warn!(
                did = did,
                trust_score = trust_score,
                current_count = current_count,
                limit = limit,
                "Transaction velocity limit exceeded"
            );
            gateway::velocity_limit_exceeded_inc();
            return Err(GatewayError::RateLimitExceeded(format!(
                "Transaction velocity limit exceeded: {current_count} transactions/hour (limit: {limit})"
            )));
        }

        window.record(self.config.window);
        Ok(())
    }

    /// Get current transaction count for a DID
    pub fn current_count(&self, did: &str) -> u32 {
        let windows = match self.windows.read() {
            Ok(w) => w,
            Err(_) => return 0,
        };

        windows
            .get(did)
            .map(|w| w.count(self.config.window))
            .unwrap_or(0)
    }

    /// Cleanup inactive DIDs (those with no recent transactions)
    pub fn cleanup_inactive(&self, max_age: Duration) -> usize {
        let mut windows = match self.windows.write() {
            Ok(w) => w,
            Err(_) => return 0,
        };

        let before = windows.len();
        windows.retain(|_, window| window.count(max_age) > 0);
        before - windows.len()
    }
}

impl Default for VelocityLimiter {
    fn default() -> Self {
        Self::new(VelocityLimitConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_category_from_request() {
        // Compute endpoints
        assert_eq!(
            EndpointCategory::from_request("/v1/compute/submit", "POST"),
            EndpointCategory::Compute
        );
        assert_eq!(
            EndpointCategory::from_request("/v1/compute/status", "GET"),
            EndpointCategory::Compute
        );

        // Governance endpoints
        assert_eq!(
            EndpointCategory::from_request("/v1/governance/proposals", "POST"),
            EndpointCategory::Governance
        );
        assert_eq!(
            EndpointCategory::from_request("/v1/amendments/123", "PUT"),
            EndpointCategory::Governance
        );
        assert_eq!(
            EndpointCategory::from_request("/v1/appeals/456", "GET"),
            EndpointCategory::Governance
        );
        assert_eq!(
            EndpointCategory::from_request("/v1/domain/abc/vote", "POST"),
            EndpointCategory::Governance
        );

        // Charter/membership GET = Read
        assert_eq!(
            EndpointCategory::from_request("/v1/charter/list", "GET"),
            EndpointCategory::Read
        );
        // Charter/membership POST = Governance
        assert_eq!(
            EndpointCategory::from_request("/v1/charter/create", "POST"),
            EndpointCategory::Governance
        );
        assert_eq!(
            EndpointCategory::from_request("/v1/membership/approve", "POST"),
            EndpointCategory::Governance
        );

        // Steward GET = Read
        assert_eq!(
            EndpointCategory::from_request("/v1/steward/list", "GET"),
            EndpointCategory::Read
        );
        // Steward POST = Governance
        assert_eq!(
            EndpointCategory::from_request("/v1/steward/register", "POST"),
            EndpointCategory::Governance
        );

        // Regular endpoints - method based
        assert_eq!(
            EndpointCategory::from_request("/v1/ledger/balance", "GET"),
            EndpointCategory::Read
        );
        assert_eq!(
            EndpointCategory::from_request("/v1/ledger/pay", "POST"),
            EndpointCategory::Write
        );
        assert_eq!(
            EndpointCategory::from_request("/v1/coops", "DELETE"),
            EndpointCategory::Write
        );
    }

    #[test]
    fn test_endpoint_category_configs() {
        // Read should have highest limits
        let read = EndpointCategory::Read.config();
        assert_eq!(read.capacity, 200.0);
        assert_eq!(read.refill_rate, 20.0);

        // Write should be moderate
        let write = EndpointCategory::Write.config();
        assert_eq!(write.capacity, 60.0);
        assert_eq!(write.refill_rate, 6.0);

        // Governance should be lower
        let gov = EndpointCategory::Governance.config();
        assert_eq!(gov.capacity, 30.0);
        assert_eq!(gov.refill_rate, 3.0);

        // Compute should be lowest
        let compute = EndpointCategory::Compute.config();
        assert_eq!(compute.capacity, 10.0);
        assert_eq!(compute.refill_rate, 1.0);
    }

    #[test]
    fn test_category_rate_limiter_different_limits() {
        let limiter = CategoryRateLimiter::new();
        let did = "did:icn:test";

        // Compute has lowest limits (10)
        for _ in 0..10 {
            assert!(limiter
                .check_rate_limit(did, EndpointCategory::Compute)
                .is_ok());
        }
        // 11th compute request should fail
        assert!(limiter
            .check_rate_limit(did, EndpointCategory::Compute)
            .is_err());

        // But governance still has tokens (30)
        for _ in 0..30 {
            assert!(limiter
                .check_rate_limit(did, EndpointCategory::Governance)
                .is_ok());
        }
        assert!(limiter
            .check_rate_limit(did, EndpointCategory::Governance)
            .is_err());

        // And read has even more tokens (200)
        for _ in 0..200 {
            assert!(limiter
                .check_rate_limit(did, EndpointCategory::Read)
                .is_ok());
        }
        assert!(limiter
            .check_rate_limit(did, EndpointCategory::Read)
            .is_err());
    }

    #[test]
    fn test_category_rate_limiter_per_did_independence() {
        let limiter = CategoryRateLimiter::new();
        let did1 = "did:icn:alice";
        let did2 = "did:icn:bob";

        // Exhaust compute bucket for alice
        for _ in 0..10 {
            assert!(limiter
                .check_rate_limit(did1, EndpointCategory::Compute)
                .is_ok());
        }
        assert!(limiter
            .check_rate_limit(did1, EndpointCategory::Compute)
            .is_err());

        // Bob should still have full compute quota
        for _ in 0..10 {
            assert!(limiter
                .check_rate_limit(did2, EndpointCategory::Compute)
                .is_ok());
        }
        assert!(limiter
            .check_rate_limit(did2, EndpointCategory::Compute)
            .is_err());
    }

    #[test]
    fn test_category_rate_limiter_for_request() {
        let limiter = CategoryRateLimiter::new();
        let did = "did:icn:test";

        // POST to /compute should use Compute limits
        for _ in 0..10 {
            assert!(limiter
                .check_rate_limit_for_request(did, "/v1/compute/submit", "POST")
                .is_ok());
        }
        assert!(limiter
            .check_rate_limit_for_request(did, "/v1/compute/submit", "POST")
            .is_err());

        // GET to /ledger should use Read limits (still has tokens)
        assert!(limiter
            .check_rate_limit_for_request(did, "/v1/ledger/balance", "GET")
            .is_ok());
    }

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10.0, 1.0);

        // Should have full capacity initially
        let initial = bucket.available();
        assert!((initial - 10.0).abs() < 0.001);

        // Consume 5 tokens
        assert!(bucket.try_consume(5.0));
        let after_first = bucket.available();
        // Allow small variance due to refill during test execution
        assert!((4.9..=5.1).contains(&after_first));

        // Consume another 5 tokens (accounting for potential refill)
        assert!(bucket.try_consume(5.0));
        let after_second = bucket.available();
        assert!(after_second < 0.5); // Should be near zero

        // Should reject when near empty
        assert!(!bucket.try_consume(1.0));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10.0, 10.0); // 10 tokens/second

        // Consume all tokens
        assert!(bucket.try_consume(10.0));
        let after_consume = bucket.available();
        assert!(after_consume < 0.1); // Should be near zero

        // Wait for refill
        std::thread::sleep(Duration::from_millis(500)); // 0.5 seconds

        // Should have ~5 tokens refilled
        let available = bucket.available();
        assert!((4.5..=5.5).contains(&available));

        // Should be able to consume refilled tokens
        assert!(bucket.try_consume(4.0));
    }

    #[test]
    fn test_token_bucket_cap() {
        let mut bucket = TokenBucket::new(10.0, 10.0);

        // Consume 5 tokens
        assert!(bucket.try_consume(5.0));

        // Wait longer than needed to refill
        std::thread::sleep(Duration::from_secs(2));

        // Should be capped at capacity
        assert_eq!(bucket.available(), 10.0);
    }

    #[test]
    fn test_rate_limiter_per_did() {
        let config = RateLimitConfig {
            capacity: 5.0,
            refill_rate: 1.0,
            cost_per_request: 1.0,
        };
        let limiter = RateLimiter::new(config);

        let did1 = "did:icn:alice";
        let did2 = "did:icn:bob";

        // Each DID has independent bucket
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(did1).is_ok());
            assert!(limiter.check_rate_limit(did2).is_ok());
        }

        // Both should be exhausted
        assert!(limiter.check_rate_limit(did1).is_err());
        assert!(limiter.check_rate_limit(did2).is_err());
    }

    #[test]
    fn test_rate_limiter_cleanup() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        // Create some buckets
        let _ = limiter.check_rate_limit("did:icn:alice");
        let _ = limiter.check_rate_limit("did:icn:bob");

        // No cleanup for recent activity
        let removed = limiter.cleanup_inactive_buckets(Duration::from_secs(60));
        assert_eq!(removed, 0);

        // Cleanup after long inactivity
        std::thread::sleep(Duration::from_millis(100));
        let removed = limiter.cleanup_inactive_buckets(Duration::from_millis(50));
        assert_eq!(removed, 2);
    }

    // ========================================================================
    // Velocity Limiter Tests (Issue #164)
    // ========================================================================

    #[test]
    fn test_velocity_limit_config_defaults() {
        let config = VelocityLimitConfig::default();

        assert_eq!(config.isolated_limit, 10);
        assert_eq!(config.known_limit, 50);
        assert_eq!(config.partner_limit, 100);
        assert_eq!(config.federated_limit, 200);
        assert_eq!(config.window.as_secs(), 3600);
    }

    #[test]
    fn test_velocity_limit_trust_levels() {
        let config = VelocityLimitConfig::default();

        // Isolated (trust < 0.1)
        assert_eq!(config.limit_for_trust(0.0), 10);
        assert_eq!(config.limit_for_trust(0.05), 10);

        // Known (trust 0.1-0.4)
        assert_eq!(config.limit_for_trust(0.1), 50);
        assert_eq!(config.limit_for_trust(0.3), 50);

        // Partner (trust 0.4-0.7)
        assert_eq!(config.limit_for_trust(0.4), 100);
        assert_eq!(config.limit_for_trust(0.6), 100);

        // Federated (trust >= 0.7)
        assert_eq!(config.limit_for_trust(0.7), 200);
        assert_eq!(config.limit_for_trust(1.0), 200);
    }

    #[test]
    fn test_velocity_limiter_basic() {
        let config = VelocityLimitConfig {
            isolated_limit: 3,
            known_limit: 5,
            partner_limit: 10,
            federated_limit: 20,
            window: Duration::from_secs(60),
        };
        let limiter = VelocityLimiter::new(config);
        let did = "did:icn:test";

        // Isolated trust (limit = 3)
        assert!(limiter.check_and_record(did, 0.0).is_ok());
        assert!(limiter.check_and_record(did, 0.0).is_ok());
        assert!(limiter.check_and_record(did, 0.0).is_ok());

        // Fourth should fail (limit reached)
        assert!(limiter.check_and_record(did, 0.0).is_err());

        // But with higher trust, can continue (limit = 5 for known)
        assert!(limiter.check_and_record(did, 0.2).is_ok());
        assert!(limiter.check_and_record(did, 0.2).is_ok());

        // Sixth should fail for known trust (4 + 2 = 6 > 5)
        assert!(limiter.check_and_record(did, 0.2).is_err());
    }

    #[test]
    fn test_velocity_limiter_per_did() {
        let config = VelocityLimitConfig {
            isolated_limit: 2,
            known_limit: 5,
            partner_limit: 10,
            federated_limit: 20,
            window: Duration::from_secs(60),
        };
        let limiter = VelocityLimiter::new(config);

        let did1 = "did:icn:alice";
        let did2 = "did:icn:bob";

        // Each DID has independent limits
        assert!(limiter.check_and_record(did1, 0.0).is_ok());
        assert!(limiter.check_and_record(did1, 0.0).is_ok());
        assert!(limiter.check_and_record(did1, 0.0).is_err()); // Alice exhausted

        // Bob still has capacity
        assert!(limiter.check_and_record(did2, 0.0).is_ok());
        assert!(limiter.check_and_record(did2, 0.0).is_ok());
        assert!(limiter.check_and_record(did2, 0.0).is_err()); // Bob exhausted
    }

    #[test]
    fn test_velocity_limiter_current_count() {
        let limiter = VelocityLimiter::default();
        let did = "did:icn:counter-test";

        assert_eq!(limiter.current_count(did), 0);

        let _ = limiter.check_and_record(did, 0.5);
        assert_eq!(limiter.current_count(did), 1);

        let _ = limiter.check_and_record(did, 0.5);
        assert_eq!(limiter.current_count(did), 2);
    }
}
