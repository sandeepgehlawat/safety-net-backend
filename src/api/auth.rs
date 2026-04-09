use crate::data::{Database, User};
use alloy::primitives::{keccak256, PrimitiveSignature};
use anyhow::{anyhow, Result};
use axum::{
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::Response,
    body::Body,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// Session stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: Uuid,
    pub wallet_address: String,
    pub created_at: i64,
    pub expires_at: i64,
}

impl Session {
    pub fn new(user_id: Uuid, wallet_address: String) -> Self {
        let now = Utc::now().timestamp();
        Self {
            user_id,
            wallet_address,
            created_at: now,
            expires_at: now + Duration::days(7).num_seconds(),
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.expires_at
    }
}

#[derive(Clone)]
pub struct AuthState {
    pub db: Database,
    pub redis: redis::aio::ConnectionManager,
}

#[derive(Debug, Deserialize)]
pub struct ConnectWalletRequest {
    pub message: String,
    pub signature: String,
}

#[derive(Debug, Serialize)]
pub struct AuthPayload {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub wallet_address: String,
    pub tier: String,
    pub autopilot_enabled: bool,
}

impl From<User> for UserInfo {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            wallet_address: user.wallet_address,
            tier: user.tier,
            autopilot_enabled: user.autopilot_enabled,
        }
    }
}

/// SIWE authentication handler
/// Verifies the signature and creates a session
pub async fn connect_wallet(
    State(state): State<Arc<AuthState>>,
    Json(payload): Json<ConnectWalletRequest>,
) -> Result<Json<AuthPayload>, (StatusCode, String)> {
    // Parse wallet address from message
    let claimed_address = extract_address_from_message(&payload.message)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid SIWE message format".to_string()))?;

    // Verify the signature
    let recovered_address = verify_siwe_signature(&payload.message, &payload.signature)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Signature verification failed: {}", e)))?;

    // Check that the recovered address matches the claimed address
    if recovered_address.to_lowercase() != claimed_address.to_lowercase() {
        return Err((StatusCode::UNAUTHORIZED, "Signature does not match claimed address".to_string()));
    }

    let wallet_address = recovered_address.to_lowercase();

    // Get or create user
    let user = match state.db.get_user_by_wallet(&wallet_address).await {
        Ok(Some(user)) => {
            state.db.update_user_last_seen(user.id).await.ok();
            user
        }
        Ok(None) => {
            state.db.create_user(&wallet_address).await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create user: {}", e)))?
        }
        Err(e) => {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", e)));
        }
    };

    // Create session
    let session = Session::new(user.id, wallet_address);
    let token = Uuid::new_v4().to_string();

    // Store session in Redis
    let session_json = serde_json::to_string(&session)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Serialization error: {}", e)))?;

    let mut conn = state.redis.clone();
    redis::cmd("SETEX")
        .arg(format!("session:{}", token))
        .arg(Duration::days(7).num_seconds())
        .arg(&session_json)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Redis error: {}", e)))?;

    Ok(Json(AuthPayload {
        token,
        user: user.into(),
    }))
}

/// Verify SIWE signature and return the recovered address
fn verify_siwe_signature(message: &str, signature_hex: &str) -> Result<String, String> {
    // Decode the signature from hex
    let sig_bytes = hex::decode(signature_hex.trim_start_matches("0x"))
        .map_err(|e| format!("Invalid signature hex: {}", e))?;

    if sig_bytes.len() != 65 {
        return Err(format!("Invalid signature length: expected 65, got {}", sig_bytes.len()));
    }

    // Parse signature components (r, s, v)
    let signature = PrimitiveSignature::try_from(sig_bytes.as_slice())
        .map_err(|e| format!("Invalid signature format: {}", e))?;

    // Create the EIP-191 signed message hash
    // Format: "\x19Ethereum Signed Message:\n" + len(message) + message
    let prefixed_message = format!("\x19Ethereum Signed Message:\n{}{}", message.len(), message);
    let message_hash = keccak256(prefixed_message.as_bytes());

    // Recover the signer address
    let recovered = signature
        .recover_address_from_prehash(&message_hash)
        .map_err(|e| format!("Failed to recover address: {}", e))?;

    Ok(format!("{:?}", recovered))
}

/// Extract Ethereum address from SIWE message
fn extract_address_from_message(message: &str) -> Option<String> {
    // SIWE format includes "...with your Ethereum account:\n0x..."
    for line in message.lines() {
        let trimmed = line.trim();
        if is_valid_eth_address(trimmed) {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Validate Ethereum address format
fn is_valid_eth_address(address: &str) -> bool {
    // Must start with 0x and be exactly 42 characters
    if !address.starts_with("0x") || address.len() != 42 {
        return false;
    }

    // Remaining 40 characters must be valid hex
    address[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Get session from token
pub async fn get_session(
    redis: &mut redis::aio::ConnectionManager,
    token: &str,
) -> Result<Session> {
    let session_json: Option<String> = redis::cmd("GET")
        .arg(format!("session:{}", token))
        .query_async(redis)
        .await?;

    let session_json = session_json.ok_or_else(|| anyhow!("Session not found"))?;
    let session: Session = serde_json::from_str(&session_json)?;

    if session.is_expired() {
        return Err(anyhow!("Session expired"));
    }

    Ok(session)
}

/// Authentication middleware for protected routes
pub async fn auth_middleware(
    State(state): State<Arc<AuthState>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract token from Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let token = match auth_header {
        Some(t) => t.to_string(),
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // Get session from Redis
    let mut redis = state.redis.clone();
    let session = get_session(&mut redis, &token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Add session to request extensions
    request.extensions_mut().insert(session);

    Ok(next.run(request).await)
}

/// Extract current user from request
pub fn extract_user_id<B>(request: &Request<B>) -> Option<Uuid> {
    request.extensions().get::<Session>().map(|s| s.user_id)
}

/// Verify token and return user_id (used by GraphQL handler)
pub async fn verify_token(
    state: &Arc<AuthState>,
    token: &str,
) -> Result<Option<Uuid>> {
    let mut redis = state.redis.clone();
    match get_session(&mut redis, token).await {
        Ok(session) => Ok(Some(session.user_id)),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_eth_address() {
        assert!(is_valid_eth_address("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD47"));
        assert!(is_valid_eth_address("0x0000000000000000000000000000000000000000"));
        assert!(is_valid_eth_address("0xffffffffffffffffffffffffffffffffffffffff"));
    }

    #[test]
    fn test_invalid_eth_address() {
        // Too short
        assert!(!is_valid_eth_address("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD4"));
        // Too long
        assert!(!is_valid_eth_address("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD477"));
        // Missing 0x
        assert!(!is_valid_eth_address("742d35Cc6634C0532925a3b844Bc9e7595f2bD47"));
        // Invalid hex characters
        assert!(!is_valid_eth_address("0x742d35Cc6634C0532925a3b844Bc9e7595f2bDGG"));
        // Empty
        assert!(!is_valid_eth_address(""));
        // Just 0x
        assert!(!is_valid_eth_address("0x"));
    }

    #[test]
    fn test_extract_address_from_siwe_message() {
        let message = "example.com wants you to sign in with your Ethereum account:\n0x742d35Cc6634C0532925a3b844Bc9e7595f2bD47\n\nURI: https://example.com";
        let address = extract_address_from_message(message);
        assert_eq!(address, Some("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD47".to_string()));
    }

    #[test]
    fn test_extract_address_invalid_message() {
        // No address
        let message = "example.com wants you to sign in";
        assert_eq!(extract_address_from_message(message), None);

        // Invalid address format
        let message = "example.com\n0xNOTVALID";
        assert_eq!(extract_address_from_message(message), None);
    }

    #[test]
    fn test_session_expiry() {
        let session = Session::new(Uuid::new_v4(), "0x123".to_string());
        assert!(!session.is_expired());

        // Create an expired session
        let mut expired = session.clone();
        expired.expires_at = chrono::Utc::now().timestamp() - 1;
        assert!(expired.is_expired());
    }
}
