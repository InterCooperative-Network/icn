use crate::vc::{VerifiableCredential, CredentialError};
use qrcode::{QrCode, render::unicode};
use std::path::Path;
use std::fs;
use serde_json;
use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use std::io;
use thiserror::Error;

/// Errors specific to QR code operations
#[derive(Debug, Error)]
pub enum QrCodeError {
    #[error("QR code generation error: {0}")]
    QrGenerationError(#[from] qrcode::types::QrError),
    
    #[error("Image error: {0}")]
    ImageError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    
    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Invalid credential format")]
    InvalidFormat,
    
    #[error("Credential error: {0}")]
    CredentialError(#[from] CredentialError),
}

/// Supported output formats for QR codes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QrFormat {
    Terminal,
    Svg,
    Png,
}

impl QrFormat {
    /// Parse format from string
    pub fn from_str(format: &str) -> Option<Self> {
        match format.to_lowercase().as_str() {
            "terminal" => Some(QrFormat::Terminal),
            "svg" => Some(QrFormat::Svg),
            "png" => Some(QrFormat::Png),
            _ => None,
        }
    }
}

/// Compress and encode a Verifiable Credential for QR code
pub fn encode_credential_for_qr(credential: &VerifiableCredential) -> Result<String, QrCodeError> {
    // Serialize to JSON and compress the representation
    let json = serde_json::to_string(credential)?;
    
    // Base64url encode for safe QR code content
    let encoded = URL_SAFE.encode(json.as_bytes());
    
    // Add a prefix to identify this as an ICN credential
    let prefixed = format!("icn:vc:{}", encoded);
    
    Ok(prefixed)
}

/// Decode a QR code string back to a Verifiable Credential
pub fn decode_credential_from_qr(qr_content: &str) -> Result<VerifiableCredential, QrCodeError> {
    // Check prefix
    if !qr_content.starts_with("icn:vc:") {
        return Err(QrCodeError::InvalidFormat);
    }
    
    // Remove prefix
    let encoded = qr_content.trim_start_matches("icn:vc:");
    
    // Base64url decode
    let decoded = URL_SAFE.decode(encoded.as_bytes())
        .map_err(|_| QrCodeError::InvalidFormat)?;
    
    // Parse JSON
    let json = String::from_utf8(decoded)
        .map_err(|_| QrCodeError::InvalidFormat)?;
    
    // Deserialize into VerifiableCredential
    let credential: VerifiableCredential = serde_json::from_str(&json)?;
    
    Ok(credential)
}

/// Generate a QR code for a credential and return it in the specified format
pub fn generate_credential_qr(
    credential: &VerifiableCredential,
    format: QrFormat,
    output_path: Option<&Path>,
) -> Result<String, QrCodeError> {
    // Encode credential for QR
    let encoded = encode_credential_for_qr(credential)?;
    
    // Generate QR code
    let code = QrCode::new(encoded)?;
    
    match format {
        QrFormat::Terminal => {
            // For terminal output, use unicode renderer
            let qr_string = code
                .render::<unicode::Dense1x2>()
                .build();
            Ok(qr_string)
        },
        QrFormat::Svg => {
            // Generate SVG manually since qrcode-rust doesn't have a direct SVG renderer
            let size = code.size();
            let modules = code.modules();
            
            let mut svg = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n\
                 <svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" viewBox=\"0 0 ");
            svg.push_str(&(size + 2).to_string());
            svg.push_str(" ");
            svg.push_str(&(size + 2).to_string());
            svg.push_str("\" stroke=\"none\">\n\
                          <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n");
            
            // Draw modules as squares
            for y in 0..size {
                for x in 0..size {
                    if modules[y][x] {
                        svg.push_str("<rect x=\"");
                        svg.push_str(&(x + 1).to_string());
                        svg.push_str("\" y=\"");
                        svg.push_str(&(y + 1).to_string());
                        svg.push_str("\" width=\"1\" height=\"1\" fill=\"black\"/>\n");
                    }
                }
            }
            
            svg.push_str("</svg>");
            
            // Save to file if path provided
            if let Some(path) = output_path {
                fs::write(path, svg.as_bytes())?;
                Ok(format!("QR code saved to {}", path.display()))
            } else {
                Ok(svg)
            }
        },
        QrFormat::Png => {
            // For PNG, we need a more complex approach
            // This is a simplified implementation - for a real app you'd want to use the image crate directly
            
            if let Some(path) = output_path {
                // Image size in pixels
                let size = 250;
                let module_size = size / code.size();
                
                // Create a new image
                let mut imgbuf = image::RgbImage::new(size as u32, size as u32);
                
                // Fill with white
                for pixel in imgbuf.pixels_mut() {
                    *pixel = image::Rgb([255, 255, 255]);
                }
                
                // Draw QR code
                for y in 0..code.size() {
                    for x in 0..code.size() {
                        if code.module(x, y) {
                            // Draw a black square for each module
                            for dy in 0..module_size {
                                for dx in 0..module_size {
                                    let px = (x * module_size + dx) as u32;
                                    let py = (y * module_size + dy) as u32;
                                    if px < size as u32 && py < size as u32 {
                                        imgbuf.put_pixel(px, py, image::Rgb([0, 0, 0]));
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Save to file
                imgbuf.save(path).map_err(|e| QrCodeError::ImageError(e.to_string()))?;
                Ok(format!("QR code saved to {}", path.display()))
            } else {
                Err(QrCodeError::ImageError("PNG format requires an output path".to_string()))
            }
        }
    }
}

/// Scan and decode a QR code from an image file
/// Note: This is a placeholder for a real QR scanning implementation
/// In a real application, you might use a library like quirc or zbar
pub fn scan_credential_qr(_image_path: &Path) -> Result<VerifiableCredential, QrCodeError> {
    // This would be replaced with actual QR scanning code
    Err(QrCodeError::ImageError("QR scanning from images not implemented yet".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vc::{Proof, FederationMemberSubject, FederationMember};
    use chrono::Utc;
    
    #[test]
    fn test_qr_encode_decode() {
        // Create a test VC
        let vc = VerifiableCredential {
            context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
            id: "urn:uuid:test123".to_string(),
            types: vec!["VerifiableCredential".to_string(), "TestCredential".to_string()],
            issuer: "did:icn:test:issuer".to_string(),
            issuanceDate: Utc::now(),
            credentialSubject: FederationMemberSubject {
                id: "did:icn:test:subject".to_string(),
                federationMember: FederationMember {
                    scope: "test".to_string(),
                    username: "testuser".to_string(),
                    role: "member".to_string(),
                },
            },
            proof: Some(Proof {
                type_: "Ed25519Signature2020".to_string(),
                created: Utc::now(),
                verificationMethod: "did:icn:test:issuer#keys-1".to_string(),
                proofPurpose: "assertionMethod".to_string(),
                proofValue: "test_signature".to_string(),
            }),
        };
        
        // Encode the VC
        let encoded = encode_credential_for_qr(&vc).unwrap();
        
        // Decode it back
        let decoded = decode_credential_from_qr(&encoded).unwrap();
        
        // Check the result
        assert_eq!(vc.id, decoded.id);
        assert_eq!(vc.issuer, decoded.issuer);
        assert_eq!(vc.credentialSubject.id, decoded.credentialSubject.id);
    }
} 