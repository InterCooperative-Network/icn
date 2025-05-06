use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use icn_identity::Did;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, warn};

use crate::{TokenAmount, EscrowError};

/// A record of a token transfer
#[derive(Debug, Clone)]
pub struct TokenTransfer {
    /// Unique identifier for this transfer
    pub id: String,
    
    /// The sender's DID
    pub from_did: Did,
    
    /// The recipient's DID
    pub to_did: Did,
    
    /// Amount of tokens transferred
    pub amount: TokenAmount,
    
    /// Timestamp of the transfer
    pub timestamp: chrono::DateTime<Utc>,
    
    /// Reference (e.g., contract ID, reason)
    pub reference: String,
}

/// Interface for token payment systems
#[async_trait]
pub trait PaymentInterface: Send + Sync {
    /// Transfer tokens from one account to another
    async fn transfer(&self, from: &Did, to: &Did, amount: TokenAmount, reference: &str) -> Result<TokenTransfer>;
    
    /// Get account balance
    async fn get_balance(&self, account: &Did) -> Result<TokenAmount>;
    
    /// Check if an account has sufficient balance
    async fn has_sufficient_balance(&self, account: &Did, amount: TokenAmount) -> Result<bool>;
    
    /// Lock tokens for a specific purpose
    async fn lock_tokens(&self, account: &Did, amount: TokenAmount, purpose: &str) -> Result<String>;
    
    /// Release locked tokens back to the account
    async fn release_tokens(&self, lock_id: &str) -> Result<()>;
}

/// Simple in-memory payment system for testing and development
pub struct SimplePaymentSystem {
    /// Account balances by DID
    balances: Arc<Mutex<HashMap<Did, TokenAmount>>>,
    
    /// Locked tokens by lock ID
    locked_tokens: Arc<Mutex<HashMap<String, (Did, TokenAmount, String)>>>,
    
    /// Transfer history
    transfers: Arc<Mutex<Vec<TokenTransfer>>>,
    
    /// Decimal precision for tokens
    decimals: u8,
}

impl SimplePaymentSystem {
    /// Create a new payment system
    pub fn new(decimals: u8) -> Self {
        Self {
            balances: Arc::new(Mutex::new(HashMap::new())),
            locked_tokens: Arc::new(Mutex::new(HashMap::new())),
            transfers: Arc::new(Mutex::new(Vec::new())),
            decimals,
        }
    }
    
    /// Add initial balance to an account (for testing)
    pub fn add_balance(&self, account: &Did, amount: u64) -> Result<()> {
        let mut balances = self.balances.lock().unwrap();
        
        let balance = balances.entry(account.clone())
            .or_insert(TokenAmount::new(0, self.decimals));
        
        *balance = TokenAmount::new(
            balance.value.checked_add(amount)
                .ok_or_else(|| anyhow!("Balance overflow"))?,
            self.decimals
        );
        
        Ok(())
    }
    
    /// Generate a unique lock ID
    fn generate_lock_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

#[async_trait]
impl PaymentInterface for SimplePaymentSystem {
    async fn transfer(&self, from: &Did, to: &Did, amount: TokenAmount, reference: &str) -> Result<TokenTransfer> {
        // Check decimals match
        if amount.decimals != self.decimals {
            return Err(anyhow!("Token decimals mismatch"));
        }
        
        let mut balances = self.balances.lock().unwrap();
        
        // Check if sender has sufficient balance
        let sender_balance = balances.entry(from.clone())
            .or_insert(TokenAmount::new(0, self.decimals));
        
        if sender_balance.value < amount.value {
            return Err(anyhow!("Insufficient balance"));
        }
        
        // Subtract from sender
        *sender_balance = TokenAmount::new(
            sender_balance.value - amount.value,
            self.decimals
        );
        
        // Add to recipient
        let recipient_balance = balances.entry(to.clone())
            .or_insert(TokenAmount::new(0, self.decimals));
        
        *recipient_balance = TokenAmount::new(
            recipient_balance.value + amount.value,
            self.decimals
        );
        
        // Create transfer record
        let transfer = TokenTransfer {
            id: uuid::Uuid::new_v4().to_string(),
            from_did: from.clone(),
            to_did: to.clone(),
            amount,
            timestamp: Utc::now(),
            reference: reference.to_string(),
        };
        
        // Add to history
        let mut transfers = self.transfers.lock().unwrap();
        transfers.push(transfer.clone());
        
        info!("Transferred {} tokens from {} to {}: {}", 
            amount.value, from, to, reference);
        
        Ok(transfer)
    }
    
    async fn get_balance(&self, account: &Did) -> Result<TokenAmount> {
        let balances = self.balances.lock().unwrap();
        
        Ok(*balances.get(account)
            .unwrap_or(&TokenAmount::new(0, self.decimals)))
    }
    
    async fn has_sufficient_balance(&self, account: &Did, amount: TokenAmount) -> Result<bool> {
        let balance = self.get_balance(account).await?;
        
        if balance.decimals != amount.decimals {
            return Err(anyhow!("Token decimals mismatch"));
        }
        
        Ok(balance.value >= amount.value)
    }
    
    async fn lock_tokens(&self, account: &Did, amount: TokenAmount, purpose: &str) -> Result<String> {
        // Check if account has sufficient balance
        if !self.has_sufficient_balance(account, amount).await? {
            return Err(anyhow!("Insufficient balance to lock tokens"));
        }
        
        // Generate lock ID
        let lock_id = Self::generate_lock_id();
        
        // Subtract tokens from account
        let mut balances = self.balances.lock().unwrap();
        
        let balance = balances.get_mut(account)
            .ok_or_else(|| anyhow!("Account not found"))?;
        
        *balance = TokenAmount::new(
            balance.value - amount.value,
            self.decimals
        );
        
        // Store lock
        let mut locked_tokens = self.locked_tokens.lock().unwrap();
        locked_tokens.insert(lock_id.clone(), (account.clone(), amount, purpose.to_string()));
        
        info!("Locked {} tokens for {}: {}", amount.value, account, purpose);
        
        Ok(lock_id)
    }
    
    async fn release_tokens(&self, lock_id: &str) -> Result<()> {
        let mut locked_tokens = self.locked_tokens.lock().unwrap();
        
        // Get the lock
        let (account, amount, purpose) = locked_tokens.remove(lock_id)
            .ok_or_else(|| anyhow!("Lock not found: {}", lock_id))?;
        
        // Return tokens to account
        let mut balances = self.balances.lock().unwrap();
        
        let balance = balances.entry(account.clone())
            .or_insert(TokenAmount::new(0, self.decimals));
        
        *balance = TokenAmount::new(
            balance.value + amount.value,
            self.decimals
        );
        
        info!("Released {} locked tokens back to {}: {}", 
            amount.value, account, purpose);
        
        Ok(())
    }
}

/// A mock payment adapter for connecting to external token systems
pub struct ExternalPaymentAdapter {
    /// URL of the external payment system API
    api_url: String,
    
    /// Authorization token
    auth_token: String,
    
    /// Decimal precision
    decimals: u8,
}

impl ExternalPaymentAdapter {
    /// Create a new external payment adapter
    pub fn new(api_url: &str, auth_token: &str, decimals: u8) -> Self {
        Self {
            api_url: api_url.to_string(),
            auth_token: auth_token.to_string(),
            decimals,
        }
    }
}

#[async_trait]
impl PaymentInterface for ExternalPaymentAdapter {
    async fn transfer(&self, from: &Did, to: &Did, amount: TokenAmount, reference: &str) -> Result<TokenTransfer> {
        // In a real implementation, this would make an API call to the external system
        // For now, we'll just log and return a mock response
        
        info!("Would call external API to transfer {} tokens from {} to {}: {}", 
            amount.value, from, to, reference);
        
        // Create mock transfer (would come from API in real implementation)
        let transfer = TokenTransfer {
            id: uuid::Uuid::new_v4().to_string(),
            from_did: from.clone(),
            to_did: to.clone(),
            amount,
            timestamp: Utc::now(),
            reference: reference.to_string(),
        };
        
        Ok(transfer)
    }
    
    async fn get_balance(&self, account: &Did) -> Result<TokenAmount> {
        // In a real implementation, this would make an API call to the external system
        // For now, return a mock balance
        
        info!("Would call external API to get balance for {}", account);
        
        Ok(TokenAmount::new(1000, self.decimals))
    }
    
    async fn has_sufficient_balance(&self, account: &Did, amount: TokenAmount) -> Result<bool> {
        let balance = self.get_balance(account).await?;
        
        Ok(balance.value >= amount.value)
    }
    
    async fn lock_tokens(&self, account: &Did, amount: TokenAmount, purpose: &str) -> Result<String> {
        // In a real implementation, this would make an API call to the external system
        // For now, return a mock lock ID
        
        info!("Would call external API to lock {} tokens for {}: {}", 
            amount.value, account, purpose);
        
        Ok(uuid::Uuid::new_v4().to_string())
    }
    
    async fn release_tokens(&self, lock_id: &str) -> Result<()> {
        // In a real implementation, this would make an API call to the external system
        
        info!("Would call external API to release lock {}", lock_id);
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_simple_payment_system() {
        let payments = SimplePaymentSystem::new(6);
        
        // Add initial balances
        let alice = "did:icn:alice".to_string();
        let bob = "did:icn:bob".to_string();
        
        payments.add_balance(&alice, 1000).unwrap();
        
        // Check balance
        let balance = payments.get_balance(&alice).await.unwrap();
        assert_eq!(balance.value, 1000);
        
        // Transfer
        let amount = TokenAmount::new(500, 6);
        let transfer = payments.transfer(&alice, &bob, amount, "test").await.unwrap();
        
        assert_eq!(transfer.from_did, alice);
        assert_eq!(transfer.to_did, bob);
        assert_eq!(transfer.amount.value, 500);
        
        // Check balances after transfer
        let alice_balance = payments.get_balance(&alice).await.unwrap();
        let bob_balance = payments.get_balance(&bob).await.unwrap();
        
        assert_eq!(alice_balance.value, 500);
        assert_eq!(bob_balance.value, 500);
        
        // Test token locking
        let lock_amount = TokenAmount::new(200, 6);
        let lock_id = payments.lock_tokens(&alice, lock_amount, "escrow").await.unwrap();
        
        // Check balance after lock
        let alice_balance = payments.get_balance(&alice).await.unwrap();
        assert_eq!(alice_balance.value, 300); // 500 - 200
        
        // Release lock
        payments.release_tokens(&lock_id).await.unwrap();
        
        // Check balance after release
        let alice_balance = payments.get_balance(&alice).await.unwrap();
        assert_eq!(alice_balance.value, 500); // 300 + 200
    }
} 