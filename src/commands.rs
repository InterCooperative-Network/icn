/// Handle resource token burn commands
pub fn handle_resource_burn_command(cmd: ResourceBurnCommand, repo: &Repository) -> Result<(), Error> {
    match cmd {
        ResourceBurnCommand::Consume { 
            token_id, 
            amount, 
            owner, 
            job_id, 
            receipt_id, 
            reason 
        } => {
            // Fetch the token to get its type and scope
            let token = repo.get_resource_token_by_id(&token_id)?;
            
            // Update token balance in the database
            let current_amount = token.amount;
            if current_amount < amount {
                return Err(Error::InsufficientBalance { 
                    token_id: token_id.clone(),
                    requested: amount,
                    available: current_amount,
                });
            }
            
            // Update token balance
            repo.update_resource_token_amount(&token_id, current_amount - amount)?;
            
            // Record the burn
            let token_burn = TokenBurn::new(
                token_id,
                amount,
                token.token_type,
                token.federation_scope,
                owner,
                job_id,
                receipt_id,
                reason,
            );
            
            repo.record_token_burn(&token_burn)?;
            
            println!("Successfully consumed {} tokens of type {}", 
                amount, token.token_type);
            
            Ok(())
        },
        ResourceBurnCommand::List { 
            owner, 
            token_type, 
            scope, 
            job_id, 
            receipt_id, 
            limit 
        } => {
            // Fetch token burns with filters
            let burns = repo.get_token_burns_filtered(
                owner.as_deref(),
                token_type.as_deref(),
                scope.as_deref(),
                job_id.as_deref(),
                receipt_id.as_deref(),
                limit
            )?;
            
            if burns.is_empty() {
                println!("No token burn records found with the specified filters.");
                return Ok(());
            }
            
            // Print burn records in a table format
            println!("{:<36} {:<10} {:<20} {:<24} {:<36} {:<24}",
                "ID", "AMOUNT", "TYPE", "SCOPE", "OWNER", "TIMESTAMP");
                
            println!("{}", "-".repeat(150));
            
            for burn in burns {
                println!("{:<36} {:<10.2} {:<20} {:<24} {:<36} {:<24}",
                    burn.id,
                    burn.amount,
                    burn.token_type,
                    burn.federation_scope,
                    burn.owner_did,
                    burn.formatted_timestamp()
                );
            }
            
            Ok(())
        }
    }
}

/// Handle resource token commands
pub fn handle_resource_command(cmd: ResourceCommand, repo: &Repository) -> Result<(), Error> {
    match cmd {
        ResourceCommand::Mint { 
            token_type, 
            amount, 
            owner, 
            scope, 
            expiration_seconds 
        } => {
            // Create a new resource token
            let token_id = format!("icn:resource/{}", uuid::Uuid::new_v4());
            let expires_at = expiration_seconds.map(|secs| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                now + secs
            });
            
            let resource_token = ResourceToken {
                id: token_id.clone(),
                token_type: token_type.clone(),
                amount,
                federation_scope: scope.clone(),
                owner_did: owner.clone(),
                created_at: chrono::Utc::now().timestamp(),
                expires_at: expires_at.map(|t| t as i64),
                is_revoked: false,
            };
            
            // Save to repository
            repo.create_resource_token(&resource_token)?;
            
            println!("Created resource token: {}", token_id);
            println!("Type: {}", token_type);
            println!("Amount: {}", amount);
            println!("Owner: {}", owner);
            println!("Scope: {}", scope);
            
            if let Some(exp) = expires_at {
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(exp as i64, 0)
                    .unwrap_or_default();
                println!("Expires: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));
            } else {
                println!("Expires: Never");
            }
            
            Ok(())
        },
        ResourceCommand::Balance { token_id, owner } => {
            // Check if the token exists and belongs to the specified owner
            let token = repo.get_resource_token_by_id(&token_id)?;
            
            if token.owner_did != owner {
                return Err(Error::Unauthorized { 
                    token_id,
                    owner: owner.clone(),
                });
            }
            
            // Check expiration
            if let Some(expires_at) = token.expires_at {
                let now = chrono::Utc::now().timestamp();
                if expires_at < now {
                    println!("Token is expired. Balance: 0");
                    return Ok(());
                }
            }
            
            // Check if revoked
            if token.is_revoked {
                println!("Token is revoked. Balance: 0");
                return Ok(());
            }
            
            println!("Token: {}", token_id);
            println!("Type: {}", token.token_type);
            println!("Balance: {}", token.amount);
            println!("Scope: {}", token.federation_scope);
            
            Ok(())
        },
        ResourceCommand::List { owner, token_type, scope, include_expired } => {
            // Get tokens with filters
            let tokens = repo.get_resource_tokens_filtered(
                owner.as_deref(),
                token_type.as_deref(),
                scope.as_deref(),
                None, // min_amount
                !include_expired, // exclude_expired
            )?;
            
            if tokens.is_empty() {
                println!("No resource tokens found with the specified filters.");
                return Ok(());
            }
            
            // Print tokens in a table format
            println!("{:<36} {:<20} {:<10} {:<20} {:<36} {:<15}",
                "ID", "TYPE", "AMOUNT", "SCOPE", "OWNER", "EXPIRES");
                
            println!("{}", "-".repeat(140));
            
            for token in tokens {
                let expires = match token.expires_at {
                    Some(ts) => {
                        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
                            .unwrap_or_default();
                        dt.format("%Y-%m-%d").to_string()
                    },
                    None => "Never".to_string(),
                };
                
                println!("{:<36} {:<20} {:<10.2} {:<20} {:<36} {:<15}",
                    token.id,
                    token.token_type,
                    token.amount,
                    token.federation_scope,
                    token.owner_did,
                    expires
                );
            }
            
            Ok(())
        },
        ResourceCommand::Burn(burn_cmd) => {
            handle_resource_burn_command(burn_cmd, repo)
        },
        ResourceCommand::FederationReport { 
            federation, 
            format, 
            period, 
            output, 
            anchor_epoch 
        } => {
            // Get federation stats based on filters
            let stats = if let Some(fed_id) = &federation {
                // Get stats for a specific federation
                let stat_result = repo.get_federation_burn_stats_by_id(fed_id, period)?;
                match stat_result {
                    Some(stat) => vec![stat],
                    None => {
                        return Err(Error::NotFound { 
                            entity_type: "federation".to_string(),
                            entity_id: fed_id.clone()
                        });
                    }
                }
            } else {
                // Get stats for all federations
                repo.get_federation_burn_stats(period)?
            };
            
            if stats.is_empty() {
                println!("No federation statistics found with the specified filters.");
                return Ok(());
            }
            
            // Convert database stats to report format
            let reports: Vec<FederationResourceReport> = stats.into_iter()
                .map(|stat| {
                    // For now, hardcode quota as 1000 tokens per federation
                    // In a real implementation, this would be fetched from federation contracts
                    let quota = 1000.0;
                    let remaining = quota - stat.total_tokens_burned;
                    
                    let quota_remaining_percent = if quota > 0.0 {
                        Some(100.0 * remaining / quota)
                    } else {
                        None
                    };
                    
                    let projected_exhaustion_days = if stat.avg_daily_burn > 0.0 && remaining > 0.0 {
                        Some((remaining / stat.avg_daily_burn) as i64)
                    } else if remaining <= 0.0 {
                        Some(0)
                    } else {
                        None
                    };
                    
                    let projected_exhaustion_date = projected_exhaustion_days.map(|days| {
                        Utc::now() + chrono::Duration::days(days)
                    });
                    
                    FederationResourceReport {
                        federation_id: stat.federation_id,
                        report_generated_at: Utc::now(),
                        period_days: stat.period_days,
                        total_tokens_burned: stat.total_tokens_burned,
                        avg_daily_burn: stat.avg_daily_burn,
                        peak_daily_burn: stat.peak_daily_burn,
                        peak_date: stat.peak_date,
                        quota_total: quota,
                        quota_remaining: remaining,
                        quota_remaining_percent,
                        projected_exhaustion_days,
                        projected_exhaustion_date,
                    }
                })
                .collect();
            
            // Process according to requested format
            match format.as_str() {
                "json" => {
                    // Serialize to JSON
                    let json_output = serde_json::to_string_pretty(&reports)?;
                    
                    // Output to file or stdout based on the output parameter
                    if let Some(output_path) = output {
                        let mut file = File::create(output_path)?;
                        file.write_all(json_output.as_bytes())?;
                        println!("Federation report saved to {}", output_path);
                    } else {
                        println!("{}", json_output);
                    }
                },
                "vc" => {
                    let mut vcs = Vec::new();
                    
                    // Create a VC for each federation report
                    for report in &reports {
                        // Create credential subject with report data
                        let credential_subject = FederationReportCredential {
                            id: format!("did:icn:federation:{}", report.federation_id),
                            federation_id: report.federation_id.clone(),
                            report_type: "ComputeResourceUsage".to_string(),
                            total_tokens_burned: report.total_tokens_burned,
                            avg_daily_burn: report.avg_daily_burn,
                            peak_daily_burn: report.peak_daily_burn,
                            period_days: report.period_days,
                            quota_total: report.quota_total,
                            quota_remaining: report.quota_remaining,
                            generated_at: report.report_generated_at.to_rfc3339(),
                        };
                        
                        // Create the base VC
                        let mut credential = VerifiableCredential::new(
                            "FederationResourceReport", 
                            report.federation_id.clone(), 
                            credential_subject
                        );
                        
                        // If anchor_epoch provided, add reference to it
                        if let Some(epoch_id) = &anchor_epoch {
                            // In a real implementation, we would fetch the actual AnchorCredential
                            // from the wallet's VC store or the federation's DAG
                            
                            // For now, just include the reference
                            credential.add_related_resource(
                                "anchorEpoch", 
                                format!("did:icn:epoch:{}", epoch_id)
                            );
                        }
                        
                        // Sign the credential (in real implementation this would use wallet's DID key)
                        credential.sign();
                        
                        vcs.push(credential);
                    }
                    
                    // Output to file or stdout based on the output parameter
                    let json_output = serde_json::to_string_pretty(&vcs)?;
                    
                    if let Some(output_path) = output {
                        let mut file = File::create(output_path)?;
                        file.write_all(json_output.as_bytes())?;
                        println!("Federation report VCs saved to {}", output_path);
                    } else {
                        println!("{}", json_output);
                    }
                },
                _ => {
                    return Err(Error::InvalidFormat {
                        format,
                        supported: vec!["json".to_string(), "vc".to_string()]
                    });
                }
            }
            
            Ok(())
        }
    }
} 