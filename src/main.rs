// Allow unused code in modules that expose public APIs for future phases
#![allow(dead_code)]

mod api;
mod alerter;
mod billing;
mod data;
mod executor;
mod indexer;
mod monitors;
mod protocols;

#[cfg(test)]
mod test_utils;

use api::{auth, graphql, ws, AuthState, WsState};
use alerter::{AlertService, PushNotifier, TelegramBot, EmailSender};
use data::{Database, PositionStore};
use executor::Simulator;
use monitors::HealthMonitor;

use anyhow::Result;
use axum::{
    extract::State,
    http::Method,
    routing::{get, post},
    Json, Router,
    response::{Html, IntoResponse},
};
use async_graphql::http::GraphiQLSource;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub store: Arc<PositionStore>,
    pub ws_state: Arc<WsState>,
    pub schema: graphql::SafetyNetSchema,
    pub auth_state: Arc<AuthState>,
    pub redis: redis::aio::ConnectionManager,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Early debug output (before tracing setup)
    eprintln!("[DEBUG] safety-net-backend starting...");

    // Load environment
    dotenvy::dotenv().ok();
    eprintln!("[DEBUG] dotenv loaded");

    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    eprintln!("[DEBUG] tracing initialized");

    info!("Starting Safety Net Backend...");

    // Load configuration
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/safetynet".to_string());
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string());

    // Log connection targets (mask password)
    let masked_db_url = mask_url(&database_url);
    let masked_redis_url = mask_url(&redis_url);
    info!("DATABASE_URL: {}", masked_db_url);
    info!("REDIS_URL: {}", masked_redis_url);

    // Initialize database
    info!("Connecting to database...");
    let db = Database::new(&database_url).await?;

    // Run database migrations
    info!("Running database migrations...");
    db.run_migrations().await?;
    info!("Migrations complete");

    // Initialize Redis
    info!("Connecting to Redis...");
    let redis_client = redis::Client::open(redis_url)?;
    let redis = redis::aio::ConnectionManager::new(redis_client).await?;

    // Initialize position store
    let store = Arc::new(PositionStore::new());

    // Initialize WebSocket state
    let ws_state = Arc::new(WsState::new());

    // Initialize auth state
    let auth_state = Arc::new(AuthState {
        db: db.clone(),
        redis: redis.clone(),
    });

    // Initialize alerter (optional components based on env vars)
    let push = std::env::var("FCM_API_KEY").ok().map(PushNotifier::new);
    let telegram = std::env::var("TELEGRAM_BOT_TOKEN").ok().map(TelegramBot::new);
    let email = std::env::var("SENDGRID_API_KEY").ok().map(|key| {
        EmailSender::new(key, "noreply@safetynet.app".to_string())
    });
    let alerter = Arc::new(AlertService::new(
        push,
        telegram,
        email,
        ws_state.clone(),
        db.clone(),
    ));

    // Initialize simulator
    let simulator = Arc::new(Simulator::new(
        std::env::var("TENDERLY_API_KEY").unwrap_or_default(),
        std::env::var("TENDERLY_PROJECT").unwrap_or_default(),
        std::env::var("TENDERLY_USER").unwrap_or_default(),
        db.clone(),
    ));

    // Initialize health monitor
    let health_monitor = Arc::new(HealthMonitor::new(
        store.clone(),
        db.clone(),
        ws_state.clone(),
        alerter.clone(),
        simulator.clone(),
    ));

    // Build GraphQL schema
    let schema = graphql::build_schema(db.clone(), store.clone());

    // Build app state
    let app_state = AppState {
        db: db.clone(),
        store: store.clone(),
        ws_state: ws_state.clone(),
        schema,
        auth_state: auth_state.clone(),
        redis: redis.clone(),
    };

    // Start block subscriber if RPC configured
    if let Ok(rpc_url) = std::env::var("ETH_RPC_WS") {
        info!("Starting block subscriber...");
        let _shutdown_tx = indexer::spawn_block_subscriber(
            rpc_url,
            store.clone(),
            db.clone(),
            ws_state.clone(),
            health_monitor.clone(),
        ).await;
    } else {
        info!("ETH_RPC_WS not configured, block subscriber disabled");
    }

    // Build router
    let app = Router::new()
        // Auth endpoints
        .route("/auth/connect", post(connect_wallet_handler))
        // GraphQL
        .route("/graphql", get(graphql_playground))
        .route("/graphql", post(graphql_handler))
        // Docs redirect to GraphQL playground
        .route("/docs", get(docs_redirect))
        // WebSocket
        .route("/ws", get(ws_handler))
        // Health check
        .route("/health", get(health_check))
        // Stats (public)
        .route("/stats", get(global_stats))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS, Method::PUT, Method::DELETE])
                .allow_headers(Any)
                .expose_headers(Any)
                .allow_credentials(false)
                .max_age(std::time::Duration::from_secs(3600)),
        )
        .with_state(app_state);

    // Wrap with additional CORS fallback for preflight
    let app = app.fallback(fallback_handler);

    // Start server
    let port = std::env::var("PORT").unwrap_or_else(|_| "3460".to_string());
    let addr = format!("0.0.0.0:{}", port);

    info!("Safety Net Backend running on http://{}", addr);
    info!("GraphQL Playground: http://{}/graphql", addr);
    info!("WebSocket: ws://{}/ws", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Auth connect wallet handler
async fn connect_wallet_handler(
    State(state): State<AppState>,
    payload: Json<auth::ConnectWalletRequest>,
) -> Result<Json<auth::AuthPayload>, (axum::http::StatusCode, String)> {
    auth::connect_wallet(State(state.auth_state), payload).await
}

/// WebSocket handler
async fn ws_handler(
    ws: axum::extract::ws::WebSocketUpgrade,
    axum::extract::Query(query): axum::extract::Query<ws::WsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        ws::handle_socket(socket, state.ws_state, state.redis, query.token).await
    })
}

/// GraphQL handler
async fn graphql_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> impl IntoResponse {
    // Parse GraphQL request
    let mut request: async_graphql::Request = match serde_json::from_str(&body) {
        Ok(req) => req,
        Err(e) => {
            return Json(serde_json::json!({
                "errors": [{"message": format!("Invalid request: {}", e)}]
            }));
        }
    };

    // Extract and verify JWT token from Authorization header
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                // Verify token and get user_id
                if let Ok(Some(user_id)) = auth::verify_token(&state.auth_state, token).await {
                    request = request.data(user_id);
                }
            }
        }
    }

    // Execute query
    let response = state.schema.execute(request).await;

    // Convert to JSON
    Json(serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({
        "errors": [{"message": "Internal error"}]
    })))
}

/// GraphQL playground
async fn graphql_playground() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}

/// Docs redirect to GraphQL playground
async fn docs_redirect() -> impl IntoResponse {
    axum::response::Redirect::permanent("/graphql")
}

/// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

/// Mask password in URL for logging
fn mask_url(url: &str) -> String {
    // Replace password in URLs like postgres://user:pass@host or redis://:pass@host
    if let Some(at_pos) = url.find('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let scheme_end = url.find("://").map(|p| p + 3).unwrap_or(0);
            if colon_pos > scheme_end {
                return format!("{}***{}", &url[..colon_pos + 1], &url[at_pos..]);
            }
        }
    }
    url.to_string()
}

/// Public global stats endpoint
async fn global_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.db.get_global_stats().await {
        Ok(stats) => Json(serde_json::json!({
            "total_saved_usd": stats.total_saved_usd.to_string(),
            "saved_this_week_usd": stats.saved_this_week_usd.to_string(),
            "total_positions": stats.total_positions,
            "updated_at": stats.updated_at.to_rfc3339(),
        })),
        Err(_) => Json(serde_json::json!({
            "total_saved_usd": "0",
            "saved_this_week_usd": "0",
            "total_positions": 0,
        })),
    }
}

/// Fallback handler for unmatched routes (handles OPTIONS preflight)
async fn fallback_handler(
    method: Method,
    uri: axum::http::Uri,
) -> impl IntoResponse {
    let status = if method == Method::OPTIONS {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    };

    let body = if method == Method::OPTIONS {
        String::new()
    } else {
        format!("Not found: {} {}", method, uri)
    };

    (
        status,
        [
            ("Access-Control-Allow-Origin", "*"),
            ("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS"),
            ("Access-Control-Allow-Headers", "*"),
            ("Access-Control-Max-Age", "3600"),
        ],
        body,
    )
}
