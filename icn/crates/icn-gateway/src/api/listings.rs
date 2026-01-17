//! Listings API endpoints
//!
//! RESTful API for the cooperative internal exchange/marketplace.
//! Enables cooperatives to post offers/wants before going to external markets.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::{GatewayError, Result};
use crate::listings_mgr::{
    Listing, ListingCategory, ListingFilter, ListingId, ListingInterest, ListingStatus,
    ListingType, ListingVisibility, ListingsManager,
};
use crate::middleware::{get_claims, require_scope};
use crate::models::{
    CreateListingRequest, ExpressInterestRequest, ListingFilterParams, ListingInterestResponse,
    ListingResponse, UpdateListingRequest,
};
use icn_identity::Did;

// ============================================================================
// Validation Constants
// ============================================================================

const MAX_TITLE_LENGTH: usize = 200;
const MAX_DESCRIPTION_LENGTH: usize = 5000;
const MAX_SEEKING_LENGTH: usize = 1000;
const MAX_PHOTOS: usize = 10;
const MAX_PHOTO_URL_LENGTH: usize = 500;
const MAX_TAGS: usize = 15;
const MAX_TAG_LENGTH: usize = 50;

// ============================================================================
// Validation Functions
// ============================================================================

/// Validate listing create input
fn validate_listing_input(req: &CreateListingRequest) -> Result<()> {
    // Title validation
    if req.title.is_empty() {
        return Err(GatewayError::BadRequest("Title cannot be empty".to_string()));
    }
    if req.title.len() > MAX_TITLE_LENGTH {
        return Err(GatewayError::BadRequest(format!(
            "Title exceeds maximum length of {MAX_TITLE_LENGTH} characters"
        )));
    }

    // Description validation
    if req.description.is_empty() {
        return Err(GatewayError::BadRequest(
            "Description cannot be empty".to_string(),
        ));
    }
    if req.description.len() > MAX_DESCRIPTION_LENGTH {
        return Err(GatewayError::BadRequest(format!(
            "Description exceeds maximum length of {MAX_DESCRIPTION_LENGTH} characters"
        )));
    }

    // Seeking validation
    if req.seeking.len() > MAX_SEEKING_LENGTH {
        return Err(GatewayError::BadRequest(format!(
            "Seeking text exceeds maximum length of {MAX_SEEKING_LENGTH} characters"
        )));
    }

    // Photos validation
    if req.photos.len() > MAX_PHOTOS {
        return Err(GatewayError::BadRequest(format!(
            "Too many photos (maximum {MAX_PHOTOS})"
        )));
    }
    for (i, photo) in req.photos.iter().enumerate() {
        if photo.len() > MAX_PHOTO_URL_LENGTH {
            return Err(GatewayError::BadRequest(format!(
                "Photo URL {} exceeds maximum length of {MAX_PHOTO_URL_LENGTH} characters",
                i + 1
            )));
        }
        if photo.is_empty() {
            return Err(GatewayError::BadRequest(
                "Photo URLs cannot be empty strings".to_string(),
            ));
        }
    }

    // Tags validation
    if req.tags.len() > MAX_TAGS {
        return Err(GatewayError::BadRequest(format!(
            "Too many tags (maximum {MAX_TAGS})"
        )));
    }
    for tag in &req.tags {
        if tag.len() > MAX_TAG_LENGTH {
            return Err(GatewayError::BadRequest(format!(
                "Tag '{tag}' exceeds maximum length of {MAX_TAG_LENGTH} characters"
            )));
        }
        if tag.is_empty() {
            return Err(GatewayError::BadRequest(
                "Tags cannot be empty strings".to_string(),
            ));
        }
    }

    // Expiry date validation - can't be in the past
    if let Some(expires_at) = req.expires_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if expires_at < now {
            return Err(GatewayError::BadRequest(
                "Expiry date cannot be in the past".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate listing update input
fn validate_listing_update(req: &UpdateListingRequest) -> Result<()> {
    // Title validation
    if let Some(ref title) = req.title {
        if title.is_empty() {
            return Err(GatewayError::BadRequest("Title cannot be empty".to_string()));
        }
        if title.len() > MAX_TITLE_LENGTH {
            return Err(GatewayError::BadRequest(format!(
                "Title exceeds maximum length of {MAX_TITLE_LENGTH} characters"
            )));
        }
    }

    // Description validation
    if let Some(ref desc) = req.description {
        if desc.is_empty() {
            return Err(GatewayError::BadRequest(
                "Description cannot be empty".to_string(),
            ));
        }
        if desc.len() > MAX_DESCRIPTION_LENGTH {
            return Err(GatewayError::BadRequest(format!(
                "Description exceeds maximum length of {MAX_DESCRIPTION_LENGTH} characters"
            )));
        }
    }

    // Seeking validation
    if let Some(ref seeking) = req.seeking {
        if seeking.len() > MAX_SEEKING_LENGTH {
            return Err(GatewayError::BadRequest(format!(
                "Seeking text exceeds maximum length of {MAX_SEEKING_LENGTH} characters"
            )));
        }
    }

    // Photos validation
    if let Some(ref photos) = req.photos {
        if photos.len() > MAX_PHOTOS {
            return Err(GatewayError::BadRequest(format!(
                "Too many photos (maximum {MAX_PHOTOS})"
            )));
        }
        for (i, photo) in photos.iter().enumerate() {
            if photo.len() > MAX_PHOTO_URL_LENGTH {
                return Err(GatewayError::BadRequest(format!(
                    "Photo URL {} exceeds maximum length of {MAX_PHOTO_URL_LENGTH} characters",
                    i + 1
                )));
            }
            if photo.is_empty() {
                return Err(GatewayError::BadRequest(
                    "Photo URLs cannot be empty strings".to_string(),
                ));
            }
        }
    }

    // Tags validation
    if let Some(ref tags) = req.tags {
        if tags.len() > MAX_TAGS {
            return Err(GatewayError::BadRequest(format!(
                "Too many tags (maximum {MAX_TAGS})"
            )));
        }
        for tag in tags {
            if tag.len() > MAX_TAG_LENGTH {
                return Err(GatewayError::BadRequest(format!(
                    "Tag '{tag}' exceeds maximum length of {MAX_TAG_LENGTH} characters"
                )));
            }
            if tag.is_empty() {
                return Err(GatewayError::BadRequest(
                    "Tags cannot be empty strings".to_string(),
                ));
            }
        }
    }

    // Expiry date validation - if provided, can't be in the past
    if let Some(expires_at) = req.expires_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if expires_at < now {
            return Err(GatewayError::BadRequest(
                "Expiry date cannot be in the past".to_string(),
            ));
        }
    }

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_listing_id(id_str: &str) -> Result<ListingId> {
    Uuid::parse_str(id_str)
        .map(ListingId)
        .map_err(|_| GatewayError::BadRequest(format!("Invalid listing ID: {id_str}")))
}

fn parse_listing_type(type_str: &str) -> Result<ListingType> {
    match type_str.to_lowercase().as_str() {
        "offer" => Ok(ListingType::Offer),
        "want" => Ok(ListingType::Want),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid listing type: {type_str}. Must be 'offer' or 'want'"
        ))),
    }
}

fn parse_category(cat_str: &str) -> ListingCategory {
    ListingCategory::from_string(cat_str)
}

fn parse_visibility(vis_str: &str) -> Result<ListingVisibility> {
    match vis_str.to_lowercase().as_str() {
        "coop" => Ok(ListingVisibility::Coop),
        "federation" => Ok(ListingVisibility::Federation),
        "network" | "public" => Ok(ListingVisibility::Network),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid visibility: {vis_str}. Must be 'coop', 'federation', or 'network'"
        ))),
    }
}

fn parse_status(status_str: &str) -> Result<ListingStatus> {
    match status_str.to_lowercase().as_str() {
        "active" => Ok(ListingStatus::Active),
        "matched" => Ok(ListingStatus::Matched),
        "completed" => Ok(ListingStatus::Completed),
        "expired" => Ok(ListingStatus::Expired),
        "cancelled" => Ok(ListingStatus::Cancelled),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid status: {status_str}. Must be 'active', 'matched', 'completed', 'expired', or 'cancelled'"
        ))),
    }
}

fn listing_to_response(listing: &Listing, interest_count: usize) -> ListingResponse {
    ListingResponse {
        id: listing.id.0.to_string(),
        listing_type: match listing.listing_type {
            ListingType::Offer => "offer".to_string(),
            ListingType::Want => "want".to_string(),
        },
        title: listing.title.clone(),
        description: listing.description.clone(),
        category: listing.category.as_str().to_string(),
        photos: listing.photos.clone(),
        offered_by: listing.offered_by.to_string(),
        coop_id: listing.coop_id.clone(),
        seeking: listing.seeking.clone(),
        visibility: match listing.visibility {
            ListingVisibility::Coop => "coop".to_string(),
            ListingVisibility::Federation => "federation".to_string(),
            ListingVisibility::Network => "network".to_string(),
        },
        status: match listing.status {
            ListingStatus::Active => "active".to_string(),
            ListingStatus::Matched => "matched".to_string(),
            ListingStatus::Completed => "completed".to_string(),
            ListingStatus::Expired => "expired".to_string(),
            ListingStatus::Cancelled => "cancelled".to_string(),
        },
        created_at: listing.created_at,
        updated_at: listing.updated_at,
        expires_at: listing.expires_at,
        tags: listing.tags.clone(),
        interest_count,
    }
}

fn interest_to_response(interest: &ListingInterest) -> ListingInterestResponse {
    ListingInterestResponse {
        id: interest.id.to_string(),
        listing_id: interest.listing_id.0.to_string(),
        from_did: interest.from_did.to_string(),
        from_coop: interest.from_coop.clone(),
        message: interest.message.clone(),
        offer: interest.offer.clone(),
        created_at: interest.created_at,
    }
}

fn build_listing_filter(params: &ListingFilterParams) -> Result<ListingFilter> {
    let mut filter = ListingFilter::default();

    if let Some(ref t) = params.listing_type {
        filter.listing_type = Some(parse_listing_type(t)?);
    }
    if let Some(ref c) = params.category {
        filter.category = Some(parse_category(c));
    }
    if let Some(ref s) = params.status {
        filter.status = Some(parse_status(s)?);
    }
    if let Some(ref coop) = params.coop_id {
        filter.coop_id = Some(coop.clone());
    }
    if let Some(ref tag) = params.tag {
        filter.tag = Some(tag.clone());
    }
    if let Some(ref owner) = params.offered_by {
        filter.offered_by = Some(owner.parse().map_err(|e| {
            GatewayError::BadRequest(format!("Invalid offered_by DID: {e}"))
        })?);
    }
    // Note: visibility filter not currently in ListingFilter
    // search filter not currently supported

    Ok(filter)
}

// ============================================================================
// Listing Endpoints
// ============================================================================

/// POST /listings - Create a new listing
#[post("")]
pub async fn create_listing(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
    req: web::Json<CreateListingRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let creator_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let coop_id = claims.coop_id.clone();

    // Validate input
    validate_listing_input(&req)?;

    // Parse inputs
    let listing_type = parse_listing_type(&req.listing_type)?;
    let category = parse_category(&req.category);
    let visibility = parse_visibility(&req.visibility)?;

    // Create the listing through the manager
    let mgr = listings_mgr.write().await;
    let listing = mgr
        .create_listing(
            listing_type,
            req.title.clone(),
            req.description.clone(),
            category,
            creator_did,
            coop_id,
            req.seeking.clone(),
            req.photos.clone(),
            visibility,
            req.expires_at,
            req.tags.clone(),
        )
        .map_err(|e| GatewayError::InternalError(format!("Failed to create listing: {e}")))?;

    Ok(HttpResponse::Created().json(listing_to_response(&listing, 0)))
}

/// GET /listings - List/search listings
#[get("")]
pub async fn list_listings(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
    query: web::Query<ListingFilterParams>,
) -> Result<HttpResponse> {
    // Check authorization - read access is sufficient
    require_scope(&http_req, "coop:read")?;

    // Get caller DID for privacy check
    let caller_did: Option<Did> = get_claims(&http_req).and_then(|c| c.sub.parse().ok());

    // Build filter
    let filter = build_listing_filter(&query)?;

    // Get listings
    let mgr = listings_mgr.read().await;
    let listings = mgr
        .list_listings(&filter)
        .map_err(|e| GatewayError::InternalError(format!("Failed to list listings: {e}")))?;

    // Convert to responses with interest counts
    // Privacy: only show interest count for the caller's own listings
    let responses: Vec<ListingResponse> = listings
        .iter()
        .map(|l| {
            let is_owner = caller_did.as_ref().is_some_and(|did| did == &l.offered_by);
            let interest_count = if is_owner {
                mgr.get_interests(&l.id).map(|i| i.len()).unwrap_or(0)
            } else {
                0 // Don't reveal interest count to non-owners
            };
            listing_to_response(l, interest_count)
        })
        .collect();

    Ok(HttpResponse::Ok().json(responses))
}

/// GET /listings/{id} - Get a specific listing
#[get("/{id}")]
pub async fn get_listing(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:read")?;

    // Get caller DID for privacy check
    let caller_did: Option<Did> = get_claims(&http_req).and_then(|c| c.sub.parse().ok());

    let listing_id = parse_listing_id(&path)?;

    let mgr = listings_mgr.read().await;
    let listing = mgr
        .get_listing(&listing_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get listing: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Listing not found: {listing_id}")))?;

    // Privacy: only show interest count if caller owns the listing
    let is_owner = caller_did.as_ref().is_some_and(|did| did == &listing.offered_by);
    let interest_count = if is_owner {
        mgr.get_interests(&listing_id).map(|i| i.len()).unwrap_or(0)
    } else {
        0 // Don't reveal interest count to non-owners
    };

    Ok(HttpResponse::Ok().json(listing_to_response(&listing, interest_count)))
}

/// PUT /listings/{id} - Update a listing
#[put("/{id}")]
pub async fn update_listing(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
    path: web::Path<String>,
    req: web::Json<UpdateListingRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let listing_id = parse_listing_id(&path)?;

    // Validate input
    validate_listing_update(&req)?;

    let mgr = listings_mgr.write().await;

    // Get the existing listing
    let mut listing = mgr
        .get_listing(&listing_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get listing: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Listing not found: {listing_id}")))?;

    // Check ownership
    if listing.offered_by != caller_did {
        return Err(GatewayError::Forbidden(
            "Only the listing owner can update it".to_string(),
        ));
    }

    // Apply updates
    if let Some(ref title) = req.title {
        listing.title = title.clone();
    }
    if let Some(ref description) = req.description {
        listing.description = description.clone();
    }
    if let Some(ref category) = req.category {
        listing.category = parse_category(category);
    }
    if let Some(ref seeking) = req.seeking {
        listing.seeking = seeking.clone();
    }
    if let Some(ref visibility) = req.visibility {
        listing.visibility = parse_visibility(visibility)?;
    }
    if let Some(ref photos) = req.photos {
        listing.photos = photos.clone();
    }
    if let Some(expires_at) = req.expires_at {
        listing.expires_at = Some(expires_at);
    }
    if let Some(ref tags) = req.tags {
        listing.tags = tags.clone();
    }

    listing.updated_at = icn_time::current_timestamp_secs();

    // Save updates
    mgr.update_listing(&listing)
        .map_err(|e| GatewayError::InternalError(format!("Failed to update listing: {e}")))?;

    let interest_count = mgr.get_interests(&listing_id).map(|i| i.len()).unwrap_or(0);

    Ok(HttpResponse::Ok().json(listing_to_response(&listing, interest_count)))
}

/// DELETE /listings/{id} - Delete a listing
#[delete("/{id}")]
pub async fn delete_listing(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let listing_id = parse_listing_id(&path)?;

    let mgr = listings_mgr.write().await;

    // Get the existing listing to check ownership
    let listing = mgr
        .get_listing(&listing_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get listing: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Listing not found: {listing_id}")))?;

    // Check ownership
    if listing.offered_by != caller_did {
        return Err(GatewayError::Forbidden(
            "Only the listing owner can delete it".to_string(),
        ));
    }

    // Delete the listing
    mgr.delete_listing(&listing_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to delete listing: {e}")))?;

    Ok(HttpResponse::NoContent().finish())
}

/// PUT /listings/{id}/status - Update listing status (mark as matched/completed/cancelled)
#[put("/{id}/status")]
pub async fn update_listing_status(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
    path: web::Path<String>,
    req: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let listing_id = parse_listing_id(&path)?;

    let status_str = req
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GatewayError::BadRequest("Missing status field".to_string()))?;

    let new_status = parse_status(status_str)?;

    let mgr = listings_mgr.write().await;

    // Get the existing listing to check ownership
    let listing = mgr
        .get_listing(&listing_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get listing: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Listing not found: {listing_id}")))?;

    // Check ownership
    if listing.offered_by != caller_did {
        return Err(GatewayError::Forbidden(
            "Only the listing owner can update its status".to_string(),
        ));
    }

    // Use the manager's status methods
    let updated_listing = match new_status {
        ListingStatus::Matched => mgr.mark_matched(&listing_id),
        ListingStatus::Completed => mgr.mark_completed(&listing_id),
        ListingStatus::Cancelled => mgr.cancel_listing(&listing_id),
        _ => Err(anyhow::anyhow!(
            "Cannot set status to {status_str} directly. Use matched, completed, or cancelled."
        )),
    }
    .map_err(|e| GatewayError::InternalError(format!("Failed to update listing status: {e}")))?;

    let interest_count = mgr.get_interests(&listing_id).map(|i| i.len()).unwrap_or(0);

    Ok(HttpResponse::Ok().json(listing_to_response(&updated_listing, interest_count)))
}

// ============================================================================
// Interest Endpoints
// ============================================================================

/// POST /listings/{id}/interest - Express interest in a listing
#[post("/{id}/interest")]
pub async fn express_interest(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
    path: web::Path<String>,
    req: web::Json<ExpressInterestRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let coop_id = claims.coop_id.clone();
    let listing_id = parse_listing_id(&path)?;

    // Verify listing exists
    let mgr = listings_mgr.write().await;
    let listing = mgr
        .get_listing(&listing_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get listing: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Listing not found: {listing_id}")))?;

    // Can't express interest in your own listing
    if listing.offered_by == caller_did {
        return Err(GatewayError::BadRequest(
            "Cannot express interest in your own listing".to_string(),
        ));
    }

    // Express interest through the manager
    let interest = mgr
        .express_interest(
            listing_id,
            caller_did,
            coop_id,
            req.message.clone(),
            req.offer.clone(),
        )
        .map_err(|e| GatewayError::InternalError(format!("Failed to add interest: {e}")))?;

    Ok(HttpResponse::Created().json(interest_to_response(&interest)))
}

/// GET /listings/{id}/interests - Get all interests for a listing (owner only)
#[get("/{id}/interests")]
pub async fn get_interests(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:read")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let listing_id = parse_listing_id(&path)?;

    let mgr = listings_mgr.read().await;

    // Verify listing exists and caller is owner
    let listing = mgr
        .get_listing(&listing_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get listing: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Listing not found: {listing_id}")))?;

    // Only owner can see interests
    if listing.offered_by != caller_did {
        return Err(GatewayError::Forbidden(
            "Only the listing owner can view interests".to_string(),
        ));
    }

    let interests = mgr
        .get_interests(&listing_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get interests: {e}")))?;

    let responses: Vec<ListingInterestResponse> =
        interests.iter().map(interest_to_response).collect();

    Ok(HttpResponse::Ok().json(responses))
}

/// GET /listings/my - Get listings created by the current user
#[get("/my")]
pub async fn get_my_listings(
    http_req: HttpRequest,
    listings_mgr: web::Data<Arc<RwLock<ListingsManager>>>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:read")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let mgr = listings_mgr.read().await;

    // Get all listings and filter by creator
    // Note: In a production system, we'd add offered_by to ListingFilter
    let filter = ListingFilter::default();
    let all_listings = mgr
        .list_listings(&filter)
        .map_err(|e| GatewayError::InternalError(format!("Failed to list listings: {e}")))?;

    let my_listings: Vec<ListingResponse> = all_listings
        .iter()
        .filter(|l| l.offered_by == caller_did)
        .map(|l| {
            let interest_count = mgr.get_interests(&l.id).map(|i| i.len()).unwrap_or(0);
            listing_to_response(l, interest_count)
        })
        .collect();

    Ok(HttpResponse::Ok().json(my_listings))
}

/// Configure listing routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/listings")
            // Note: /my must come before /{id} to avoid being captured as an ID
            .service(get_my_listings)
            .service(create_listing)
            .service(list_listings)
            .service(get_listing)
            .service(update_listing)
            .service(delete_listing)
            .service(update_listing_status)
            .service(express_interest)
            .service(get_interests),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_listing_type() {
        assert!(matches!(
            parse_listing_type("offer"),
            Ok(ListingType::Offer)
        ));
        assert!(matches!(parse_listing_type("want"), Ok(ListingType::Want)));
        assert!(matches!(
            parse_listing_type("OFFER"),
            Ok(ListingType::Offer)
        ));
        assert!(parse_listing_type("invalid").is_err());
    }

    #[test]
    fn test_parse_category() {
        assert!(matches!(
            parse_category("equipment"),
            ListingCategory::Equipment
        ));
        assert!(matches!(
            parse_category("services"),
            ListingCategory::Services
        ));
        assert!(matches!(
            parse_category("MATERIALS"),
            ListingCategory::Materials
        ));
        // Unknown categories become Other
        assert!(matches!(
            parse_category("unknown"),
            ListingCategory::Other(_)
        ));
    }

    #[test]
    fn test_parse_visibility() {
        assert!(matches!(
            parse_visibility("coop"),
            Ok(ListingVisibility::Coop)
        ));
        assert!(matches!(
            parse_visibility("federation"),
            Ok(ListingVisibility::Federation)
        ));
        assert!(matches!(
            parse_visibility("network"),
            Ok(ListingVisibility::Network)
        ));
        assert!(matches!(
            parse_visibility("public"),
            Ok(ListingVisibility::Network)
        )); // Alias
        assert!(parse_visibility("invalid").is_err());
    }

    #[test]
    fn test_parse_status() {
        assert!(matches!(parse_status("active"), Ok(ListingStatus::Active)));
        assert!(matches!(
            parse_status("matched"),
            Ok(ListingStatus::Matched)
        ));
        assert!(matches!(
            parse_status("completed"),
            Ok(ListingStatus::Completed)
        ));
        assert!(matches!(
            parse_status("expired"),
            Ok(ListingStatus::Expired)
        ));
        assert!(matches!(
            parse_status("cancelled"),
            Ok(ListingStatus::Cancelled)
        ));
        assert!(parse_status("invalid").is_err());
    }
}
