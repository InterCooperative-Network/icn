use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use chrono::{DateTime, Utc};

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("Insufficient balance for token {0}")]
    InsufficientBalance(String),
    
    #[error("Token {0} is expired")]
    TokenExpired(String),
    
    #[error("Invalid token type: {0}")]
    InvalidTokenType(String),
    
    #[error("Token transfer failed: {0}")]
    TransferFailed(String),
}

// TokenType defines the different kinds of tokens in the ICN system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TokenType {
    // Common Exchange Credit - the basic unit of account in ICN
    CEC,
    // Typed tokens represent specific use cases
    Typed(String),
}

impl TokenType {
    pub fn to_string(&self) -> String {
        match self {
            TokenType::CEC => "CEC".to_string(),
            TokenType::Typed(name) => name.clone(),
        }
    }
    
    pub fn from_string(s: &str) -> Result<Self, TokenError> {
        match s {
            "CEC" => Ok(TokenType::CEC),
            _ => Ok(TokenType::Typed(s.to_string())),
        }
    }
}

// TokenMetadata stores additional information about tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    // Some tokens may have an expiration date
    pub expires_at: Option<DateTime<Utc>>,
    // Decay rate (e.g., 0.1 means 10% per month)
    pub decay_rate: Option<f64>,
    // Last decay calculation timestamp
    pub last_decay_calc: Option<DateTime<Utc>>,
    // Additional metadata as key-value pairs
    pub attributes: HashMap<String, String>,
}

impl Default for TokenMetadata {
    fn default() -> Self {
        Self {
            expires_at: None,
            decay_rate: None,
            last_decay_calc: None,
            attributes: HashMap::new(),
        }
    }
}

// TokenBalance represents a balance of a specific token type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub token_type: TokenType,
    pub amount: f64,
    pub metadata: TokenMetadata,
}

impl TokenBalance {
    pub fn new(token_type: TokenType, amount: f64) -> Self {
        Self {
            token_type,
            amount,
            metadata: TokenMetadata::default(),
        }
    }
    
    pub fn new_with_metadata(token_type: TokenType, amount: f64, metadata: TokenMetadata) -> Self {
        Self {
            token_type,
            amount,
            metadata,
        }
    }
    
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.metadata.expires_at {
            expires_at < Utc::now()
        } else {
            false
        }
    }
    
    // Apply decay if applicable
    pub fn apply_decay(&mut self) {
        if let Some(decay_rate) = self.metadata.decay_rate {
            let now = Utc::now();
            
            if let Some(last_calc) = self.metadata.last_decay_calc {
                // Calculate months since last decay calculation (simplified)
                let duration = now.signed_duration_since(last_calc);
                let months = duration.num_days() as f64 / 30.0;
                
                if months > 0.0 {
                    // Apply decay formula: amount * (1 - decay_rate)^months
                    self.amount *= (1.0 - decay_rate).powf(months);
                    self.metadata.last_decay_calc = Some(now);
                }
            } else {
                // First time calculation
                self.metadata.last_decay_calc = Some(now);
            }
        }
    }
    
    pub fn set_expiry(&mut self, expires_at: DateTime<Utc>) {
        self.metadata.expires_at = Some(expires_at);
    }
    
    pub fn set_decay_rate(&mut self, rate: f64) {
        self.metadata.decay_rate = Some(rate);
        self.metadata.last_decay_calc = Some(Utc::now());
    }
}

// TokenStore manages all token balances for a user
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenStore {
    // Maps token types to balances
    balances: HashMap<TokenType, TokenBalance>,
    // Track the scope this token store belongs to
    scope: String,
}

impl TokenStore {
    pub fn new(scope: &str) -> Self {
        Self {
            balances: HashMap::new(),
            scope: scope.to_string(),
        }
    }
    
    pub fn add_balance(&mut self, balance: TokenBalance) {
        let token_type = balance.token_type.clone();
        self.balances.insert(token_type, balance);
    }
    
    pub fn get_balance(&self, token_type: &TokenType) -> Option<&TokenBalance> {
        self.balances.get(token_type)
    }
    
    pub fn get_balance_mut(&mut self, token_type: &TokenType) -> Option<&mut TokenBalance> {
        self.balances.get_mut(token_type)
    }
    
    pub fn list_balances(&self) -> Vec<&TokenBalance> {
        self.balances.values().collect()
    }
    
    pub fn add_amount(&mut self, token_type: TokenType, amount: f64) {
        if let Some(balance) = self.balances.get_mut(&token_type) {
            balance.apply_decay();
            balance.amount += amount;
        } else {
            let balance = TokenBalance::new(token_type.clone(), amount);
            self.balances.insert(token_type, balance);
        }
    }
    
    pub fn subtract_amount(&mut self, token_type: &TokenType, amount: f64) -> Result<(), TokenError> {
        if let Some(balance) = self.balances.get_mut(token_type) {
            balance.apply_decay();
            
            if balance.is_expired() {
                return Err(TokenError::TokenExpired(token_type.to_string()));
            }
            
            if balance.amount < amount {
                return Err(TokenError::InsufficientBalance(token_type.to_string()));
            }
            
            balance.amount -= amount;
            Ok(())
        } else {
            Err(TokenError::InsufficientBalance(token_type.to_string()))
        }
    }
    
    pub fn transfer(&mut self, 
                   recipient: &mut TokenStore, 
                   token_type: &TokenType, 
                   amount: f64) -> Result<(), TokenError> {
        // First check if we have enough balance
        self.subtract_amount(token_type, amount)?;
        
        // Then add to recipient
        recipient.add_amount(token_type.clone(), amount);
        
        Ok(())
    }
    
    pub fn scope(&self) -> &str {
        &self.scope
    }
    
    // Update all balances by applying decay rules
    pub fn update_all_balances(&mut self) {
        for balance in self.balances.values_mut() {
            balance.apply_decay();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    
    #[test]
    fn test_basic_token_operations() {
        let mut store = TokenStore::new("coop1");
        
        // Add some CEC
        store.add_amount(TokenType::CEC, 100.0);
        
        // Add some hours token
        store.add_amount(TokenType::Typed("hours".to_string()), 8.0);
        
        // Check balances
        let cec_balance = store.get_balance(&TokenType::CEC).unwrap();
        assert_eq!(cec_balance.amount, 100.0);
        
        let hours_balance = store.get_balance(&TokenType::Typed("hours".to_string())).unwrap();
        assert_eq!(hours_balance.amount, 8.0);
        
        // Test subtraction
        store.subtract_amount(&TokenType::CEC, 25.0).unwrap();
        let cec_balance = store.get_balance(&TokenType::CEC).unwrap();
        assert_eq!(cec_balance.amount, 75.0);
    }
    
    #[test]
    fn test_token_expiry() {
        let mut store = TokenStore::new("coop1");
        
        // Add a token balance that expires
        let token_type = TokenType::Typed("food_credit".to_string());
        let mut balance = TokenBalance::new(token_type.clone(), 50.0);
        
        // Set expiry to yesterday
        balance.set_expiry(Utc::now() - Duration::days(1));
        store.add_balance(balance);
        
        // Attempt to use expired token
        let result = store.subtract_amount(&token_type, 10.0);
        assert!(result.is_err());
        match result {
            Err(TokenError::TokenExpired(_)) => assert!(true),
            _ => assert!(false, "Expected TokenExpired error"),
        }
    }
    
    #[test]
    fn test_token_decay() {
        let mut store = TokenStore::new("coop1");
        
        // Add a token with decay
        let token_type = TokenType::Typed("housing".to_string());
        let mut balance = TokenBalance::new(token_type.clone(), 100.0);
        
        // Set 10% monthly decay
        balance.set_decay_rate(0.1);
        store.add_balance(balance);
        
        // Manually override last_decay_calc to simulate time passing
        if let Some(balance) = store.get_balance_mut(&token_type) {
            balance.metadata.last_decay_calc = Some(Utc::now() - Duration::days(30));
        }
        
        // Apply decay
        store.update_all_balances();
        
        // Check balance after decay (should be around 90.0 after one month of 10% decay)
        let balance = store.get_balance(&token_type).unwrap();
        assert!(balance.amount < 100.0 && balance.amount > 89.0 && balance.amount < 91.0);
    }
    
    #[test]
    fn test_token_transfer() {
        let mut store1 = TokenStore::new("coop1");
        let mut store2 = TokenStore::new("coop1");
        
        // Add tokens to first store
        store1.add_amount(TokenType::CEC, 100.0);
        
        // Transfer tokens
        store1.transfer(&mut store2, &TokenType::CEC, 50.0).unwrap();
        
        // Check balances
        assert_eq!(store1.get_balance(&TokenType::CEC).unwrap().amount, 50.0);
        assert_eq!(store2.get_balance(&TokenType::CEC).unwrap().amount, 50.0);
    }
} 