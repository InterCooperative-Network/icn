/*!
 * ICN Wallet Receipt Filtering
 *
 * Provides functionality for filtering execution receipts
 * based on various criteria for selective disclosure.
 */

use chrono::{DateTime, Utc};
use icn_wallet_agent::import::ExecutionReceipt;

/// Filter criteria for execution receipts
#[derive(Debug, Clone)]
pub struct ReceiptFilter {
    /// Filter by federation scope
    pub scope: Option<String>,
    
    /// Filter by execution outcome (e.g., "Success", "Failure")
    pub outcome: Option<String>,
    
    /// Filter by timestamp (Unix timestamp) - only receipts after this time
    pub since: Option<i64>,
    
    /// Filter by proposal ID prefix
    pub proposal_prefix: Option<String>,
    
    /// Maximum number of receipts to return
    pub limit: Option<usize>,
}

impl Default for ReceiptFilter {
    fn default() -> Self {
        Self {
            scope: None,
            outcome: None,
            since: None,
            proposal_prefix: None,
            limit: None,
        }
    }
}

/// Filter receipts based on the provided criteria
pub fn filter_receipts(
    receipts: &[ExecutionReceipt],
    filter: &ReceiptFilter,
) -> Vec<ExecutionReceipt> {
    let mut filtered: Vec<_> = receipts
        .iter()
        .filter(|receipt| {
            // Filter by scope
            if let Some(scope) = &filter.scope {
                if receipt.federation_scope != *scope {
                    return false;
                }
            }
            
            // Filter by outcome
            if let Some(outcome) = &filter.outcome {
                if receipt.outcome != *outcome {
                    return false;
                }
            }
            
            // Filter by timestamp
            if let Some(since) = filter.since {
                if let Ok(date_time) = DateTime::parse_from_rfc3339(&receipt.credential.issuance_date) {
                    let receipt_timestamp = date_time.timestamp();
                    if receipt_timestamp < since {
                        return false;
                    }
                }
            }
            
            // Filter by proposal ID prefix
            if let Some(prefix) = &filter.proposal_prefix {
                if !receipt.proposal_id.starts_with(prefix) {
                    return false;
                }
            }
            
            true
        })
        .cloned()
        .collect();
    
    // Apply limit if specified
    if let Some(limit) = filter.limit {
        filtered.truncate(limit);
    }
    
    filtered
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use icn_wallet_sync::VerifiableCredential;
    
    /// Create a test receipt for filtering tests
    fn create_test_receipt(
        proposal_id: &str,
        outcome: &str,
        federation_scope: &str,
        issuance_date: &str,
    ) -> ExecutionReceipt {
        let credential = VerifiableCredential {
            context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
            id: format!("receipt-{}", proposal_id),
            types: vec!["VerifiableCredential".to_string(), "ExecutionReceipt".to_string()],
            issuer: "did:icn:test-federation".to_string(),
            issuance_date: issuance_date.to_string(),
            credential_subject: json!({
                "id": "did:icn:user1",
                "proposal_id": proposal_id,
                "outcome": outcome,
                "federation_scope": federation_scope,
                "dag_anchor": "bafybeihczzwsuj5huiqnuoo7nmwdkahxi7ny2qgwib4g34lqebzs5mmz4q",
            }),
            proof: None,
        };
        
        ExecutionReceipt {
            credential,
            proposal_id: proposal_id.to_string(),
            dag_anchor: Some("bafybeihczzwsuj5huiqnuoo7nmwdkahxi7ny2qgwib4g34lqebzs5mmz4q".to_string()),
            federation_scope: federation_scope.to_string(),
            outcome: outcome.to_string(),
        }
    }
    
    #[test]
    fn test_filter_by_scope() {
        let receipts = vec![
            create_test_receipt("prop-1", "Success", "cooperative", "2023-01-01T12:00:00Z"),
            create_test_receipt("prop-2", "Success", "technical", "2023-01-02T12:00:00Z"),
            create_test_receipt("prop-3", "Failure", "cooperative", "2023-01-03T12:00:00Z"),
        ];
        
        let filter = ReceiptFilter {
            scope: Some("cooperative".to_string()),
            ..Default::default()
        };
        
        let filtered = filter_receipts(&receipts, &filter);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].proposal_id, "prop-1");
        assert_eq!(filtered[1].proposal_id, "prop-3");
    }
    
    #[test]
    fn test_filter_by_outcome() {
        let receipts = vec![
            create_test_receipt("prop-1", "Success", "cooperative", "2023-01-01T12:00:00Z"),
            create_test_receipt("prop-2", "Success", "technical", "2023-01-02T12:00:00Z"),
            create_test_receipt("prop-3", "Failure", "cooperative", "2023-01-03T12:00:00Z"),
        ];
        
        let filter = ReceiptFilter {
            outcome: Some("Success".to_string()),
            ..Default::default()
        };
        
        let filtered = filter_receipts(&receipts, &filter);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].proposal_id, "prop-1");
        assert_eq!(filtered[1].proposal_id, "prop-2");
    }
    
    #[test]
    fn test_filter_by_timestamp() {
        let receipts = vec![
            create_test_receipt("prop-1", "Success", "cooperative", "2023-01-01T12:00:00Z"),
            create_test_receipt("prop-2", "Success", "technical", "2023-01-02T12:00:00Z"),
            create_test_receipt("prop-3", "Failure", "cooperative", "2023-01-03T12:00:00Z"),
        ];
        
        // Filter for receipts after Jan 2, 2023
        let filter = ReceiptFilter {
            since: Some(1672653600), // 2023-01-02T00:00:00Z
            ..Default::default()
        };
        
        let filtered = filter_receipts(&receipts, &filter);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].proposal_id, "prop-2");
        assert_eq!(filtered[1].proposal_id, "prop-3");
    }
    
    #[test]
    fn test_filter_by_proposal_prefix() {
        let receipts = vec![
            create_test_receipt("tech-1", "Success", "cooperative", "2023-01-01T12:00:00Z"),
            create_test_receipt("gov-1", "Success", "technical", "2023-01-02T12:00:00Z"),
            create_test_receipt("tech-2", "Failure", "cooperative", "2023-01-03T12:00:00Z"),
        ];
        
        let filter = ReceiptFilter {
            proposal_prefix: Some("tech".to_string()),
            ..Default::default()
        };
        
        let filtered = filter_receipts(&receipts, &filter);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].proposal_id, "tech-1");
        assert_eq!(filtered[1].proposal_id, "tech-2");
    }
    
    #[test]
    fn test_filter_with_limit() {
        let receipts = vec![
            create_test_receipt("prop-1", "Success", "cooperative", "2023-01-01T12:00:00Z"),
            create_test_receipt("prop-2", "Success", "technical", "2023-01-02T12:00:00Z"),
            create_test_receipt("prop-3", "Failure", "cooperative", "2023-01-03T12:00:00Z"),
        ];
        
        let filter = ReceiptFilter {
            limit: Some(1),
            ..Default::default()
        };
        
        let filtered = filter_receipts(&receipts, &filter);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].proposal_id, "prop-1");
    }
    
    #[test]
    fn test_combined_filters() {
        let receipts = vec![
            create_test_receipt("tech-1", "Success", "cooperative", "2023-01-01T12:00:00Z"),
            create_test_receipt("gov-1", "Success", "technical", "2023-01-02T12:00:00Z"),
            create_test_receipt("tech-2", "Failure", "cooperative", "2023-01-03T12:00:00Z"),
            create_test_receipt("gov-2", "Failure", "technical", "2023-01-04T12:00:00Z"),
        ];
        
        let filter = ReceiptFilter {
            scope: Some("technical".to_string()),
            outcome: Some("Success".to_string()),
            ..Default::default()
        };
        
        let filtered = filter_receipts(&receipts, &filter);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].proposal_id, "gov-1");
    }
} 