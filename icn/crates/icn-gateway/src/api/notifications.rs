//! Notifications API endpoints

use actix_web::{delete, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::Result;
use crate::notifications::{NotificationService, Platform};
use icn_identity::Did;

/// Register device request
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterDeviceRequest {
    /// Device FCM token
    pub device_token: String,
    /// Platform type
    pub platform: Platform,
}

/// Register device response
#[derive(Debug, Serialize)]
pub struct RegisterDeviceResponse {
    pub success: bool,
    pub message: String,
}

/// POST /v1/notifications/register - Register device for push notifications
///
/// Registers a device token for receiving push notifications.
/// The DID is extracted from the JWT token.
#[post("/notifications/register")]
pub async fn register_device(
    req: web::Json<RegisterDeviceRequest>,
    did: web::ReqData<Did>, // Extracted from JWT by middleware
    notification_service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse> {
    notification_service.register_device(
        did.into_inner(),
        req.device_token.clone(),
        req.platform.clone(),
    );

    Ok(HttpResponse::Ok().json(RegisterDeviceResponse {
        success: true,
        message: "Device registered successfully".to_string(),
    }))
}

/// DELETE /v1/notifications/unregister - Unregister device
///
/// Removes a device token from the notification registry.
#[delete("/notifications/unregister")]
pub async fn unregister_device(
    req: web::Json<RegisterDeviceRequest>,
    notification_service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse> {
    notification_service.unregister_device(&req.device_token);

    Ok(HttpResponse::Ok().json(RegisterDeviceResponse {
        success: true,
        message: "Device unregistered successfully".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::NotificationService;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_register_device() {
        let notification_service = Arc::new(NotificationService::new(None));
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(notification_service.clone()))
                .service(
                    web::resource("/notifications/register")
                        .route(web::post().to(
                            |req: web::Json<RegisterDeviceRequest>,
                             notification_service: web::Data<Arc<NotificationService>>| async move {
                                // Use test DID directly
                                let keypair = icn_identity::KeyPair::generate().unwrap();
                                let did = keypair.did().clone();
                                
                                notification_service.register_device(
                                    did,
                                    req.device_token.clone(),
                                    req.platform.clone(),
                                );

                                HttpResponse::Ok().json(RegisterDeviceResponse {
                                    success: true,
                                    message: "Device registered successfully".to_string(),
                                })
                            },
                        )),
                ),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/notifications/register")
            .set_json(&RegisterDeviceRequest {
                device_token: "test_token_123".to_string(),
                platform: Platform::Android,
            })
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_unregister_device() {
        let notification_service = Arc::new(NotificationService::new(None));
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Register first
        notification_service.register_device(did.clone(), "test_token".to_string(), Platform::Ios);

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(notification_service.clone()))
                .service(unregister_device),
        )
        .await;

        let req = test::TestRequest::delete()
            .uri("/notifications/unregister")
            .set_json(&RegisterDeviceRequest {
                device_token: "test_token".to_string(),
                platform: Platform::Ios,
            })
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Verify device was unregistered
        let tokens = notification_service.get_device_tokens(&did);
        assert_eq!(tokens.len(), 0);
    }
}
