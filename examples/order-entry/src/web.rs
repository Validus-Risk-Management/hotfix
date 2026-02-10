use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Json};
use axum::routing::{get, post};
use hotfix::initiator::Initiator;
use hotfix::session::SessionHandle;
use hotfix_web::{RouterConfig, build_router_with_config};
use serde::Deserialize;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::application::SharedState;
use crate::messages::{NewOrderSingleDto, OutboundMsg, random_order_json};

#[derive(Clone)]
pub struct OrderAppState {
    pub shared_state: Arc<Mutex<SharedState>>,
    pub initiator: Initiator<OutboundMsg>,
}

pub async fn start_web_service(
    session_handle: SessionHandle<OutboundMsg>,
    cancellation_token: CancellationToken,
    order_app_state: OrderAppState,
) -> Result<()> {
    let config = RouterConfig {
        enable_admin_endpoints: true,
    };
    let hotfix_router = build_router_with_config(session_handle, config);

    let order_router = Router::new()
        .route("/order", get(order_page))
        .route("/api/send-order", post(send_order))
        .route("/api/messages", get(get_messages))
        .route("/api/random-order", get(get_random_order))
        .with_state(order_app_state);

    let router = hotfix_router.merge(order_router);

    let host_and_port = std::env::var("HOST_AND_PORT").unwrap_or("0.0.0.0:9881".to_string());
    let listener = tokio::net::TcpListener::bind(&host_and_port)
        .await
        .context("failed to bind TCP listener")?;

    info!("starting web interface on http://{host_and_port}");

    select! {
        result = axum::serve(listener, router) => {
            if let Err(e) = result {
                tracing::error!("status service error: {}", e);
            }
        },
        () = cancellation_token.cancelled() => {
            info!("status service cancelled");
        }
    }

    Ok(())
}

async fn order_page() -> Html<&'static str> {
    Html(ORDER_HTML)
}

async fn send_order(
    State(state): State<OrderAppState>,
    Json(order_json): Json<NewOrderSingleDto>,
) -> impl IntoResponse {
    let order = match order_json.into_order() {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };
    let msg = OutboundMsg::NewOrderSingle(order);
    match state.initiator.send_forget(msg).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "sent"}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct MessageQuery {
    since: Option<u64>,
}

async fn get_messages(
    State(state): State<OrderAppState>,
    Query(query): Query<MessageQuery>,
) -> Json<serde_json::Value> {
    let since = query.since.unwrap_or(0);
    let shared = state.shared_state.lock().unwrap();
    let messages: Vec<_> = shared.messages.iter().filter(|m| m.id > since).collect();
    Json(serde_json::json!({ "messages": messages }))
}

async fn get_random_order() -> Json<NewOrderSingleDto> {
    Json(random_order_json())
}

const ORDER_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>New Order Single</title>
    <script src="https://cdn.tailwindcss.com/3.4.17"></script>
</head>
<body class="bg-gray-100 min-h-screen">
    <div class="container mx-auto px-4 py-8">
        <h1 class="text-2xl font-bold mb-6 text-gray-800">FIX New Order Single</h1>
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <!-- Left: Order Form -->
            <div class="bg-white rounded-lg shadow p-6">
                <h2 class="text-lg font-semibold mb-4 text-gray-700">Order JSON</h2>
                <textarea id="orderJson" rows="16"
                    class="w-full font-mono text-sm border border-gray-300 rounded-lg p-3 focus:ring-2 focus:ring-blue-500 focus:border-blue-500 resize-vertical"
                    spellcheck="false"></textarea>
                <div class="mt-4 flex gap-3">
                    <button onclick="sendOrder()"
                        class="bg-blue-600 hover:bg-blue-700 text-white font-medium py-2 px-6 rounded-lg transition-colors">
                        Send
                    </button>
                    <button onclick="regenerateOrder()"
                        class="bg-gray-200 hover:bg-gray-300 text-gray-700 font-medium py-2 px-6 rounded-lg transition-colors">
                        New Random ID
                    </button>
                </div>
                <div id="sendStatus" class="mt-3 text-sm"></div>
            </div>

            <!-- Right: Message Log -->
            <div class="bg-white rounded-lg shadow p-6">
                <h2 class="text-lg font-semibold mb-4 text-gray-700">FIX Message Log</h2>
                <div id="messageLog" class="space-y-2 max-h-[600px] overflow-y-auto font-mono text-xs">
                    <div class="text-gray-400 italic">No messages yet...</div>
                </div>
            </div>
        </div>
    </div>

    <script>
        let lastMessageId = 0;

        async function loadRandomOrder() {
            try {
                const res = await fetch('/api/random-order');
                const order = await res.json();
                document.getElementById('orderJson').value = JSON.stringify(order, null, 2);
            } catch (e) {
                console.error('Failed to load random order:', e);
            }
        }

        async function sendOrder() {
            const statusEl = document.getElementById('sendStatus');
            try {
                const json = document.getElementById('orderJson').value;
                const order = JSON.parse(json);
                const res = await fetch('/api/send-order', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(order),
                });
                const result = await res.json();
                if (res.ok) {
                    statusEl.innerHTML = '<span class="text-green-600">Order sent successfully</span>';
                    await loadRandomOrder();
                } else {
                    statusEl.innerHTML = '<span class="text-red-600">Error: ' + (result.error || 'Unknown error') + '</span>';
                }
            } catch (e) {
                statusEl.innerHTML = '<span class="text-red-600">Error: ' + e.message + '</span>';
            }
            setTimeout(() => { statusEl.innerHTML = ''; }, 5000);
        }

        async function regenerateOrder() {
            await loadRandomOrder();
        }

        async function pollMessages() {
            try {
                const res = await fetch('/api/messages?since=' + lastMessageId);
                const data = await res.json();
                if (data.messages && data.messages.length > 0) {
                    const logEl = document.getElementById('messageLog');
                    // Remove placeholder text
                    if (lastMessageId === 0) {
                        logEl.innerHTML = '';
                    }
                    for (const msg of data.messages) {
                        const div = document.createElement('div');
                        const borderColor = msg.direction === 'OUT' ? 'border-blue-400' : 'border-green-400';
                        const badgeColor = msg.direction === 'OUT' ? 'bg-blue-100 text-blue-800' : 'bg-green-100 text-green-800';
                        div.className = 'border-l-4 ' + borderColor + ' pl-3 py-2';
                        div.innerHTML = '<span class="inline-block px-2 py-0.5 rounded text-xs font-semibold ' + badgeColor + ' mb-1">' + msg.direction + '</span>'
                            + '<div class="break-all text-gray-700 whitespace-pre-wrap">' + escapeHtml(msg.fix_string) + '</div>';
                        logEl.appendChild(div);
                        lastMessageId = msg.id;
                    }
                    logEl.scrollTop = logEl.scrollHeight;
                }
            } catch (e) {
                console.error('Poll error:', e);
            }
        }

        function escapeHtml(text) {
            const div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML;
        }

        // Initialize
        loadRandomOrder();
        setInterval(pollMessages, 1000);
    </script>
</body>
</html>
"##;
