use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use eltdx::TdxClient;
use serde_json::{json, Value};
use tokio::sync::watch;

use eltdx_tools::rpc;
#[path = "ws.rs"]
mod ws;

const TITLE: &str = "eltdx HTTP/WebSocket gateway";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const WS_ONLY: [&str; 2] = ["quotes.subscribe", "quotes.unsubscribe"];

#[derive(Clone)]
struct App {
    client: TdxClient,
    collections: rpc::CollectionCache,
    shutdown: watch::Receiver<bool>,
    subscriptions: ws::Subscriptions,
}

pub(super) fn router(client: TdxClient) -> (Router, watch::Sender<bool>) {
    let (shutdown, receiver) = watch::channel(false);
    let router = Router::new()
        .route("/health", get(health))
        .route("/methods", get(methods))
        .route("/rpc", post(http_rpc))
        .route("/ws", get(ws::upgrade))
        .route("/openapi.json", get(openapi))
        .route("/docs", get(docs))
        .route("/redoc", get(redoc))
        .route("/docs/oauth2-redirect", get(oauth_redirect))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .fallback(fallback)
        .method_not_allowed_fallback(|| async {
            (
                StatusCode::METHOD_NOT_ALLOWED,
                Json(json!({"detail": "Method Not Allowed"})),
            )
        })
        .with_state(App {
            client,
            collections: rpc::CollectionCache::default(),
            shutdown: receiver,
            subscriptions: ws::Subscriptions::default(),
        });
    (router, shutdown)
}

async fn health() -> Json<Value> {
    Json(json!({
        "ok": true, "service": "eltdx", "version": VERSION,
        "transports": ["http", "websocket"], "native_push_method": WS_ONLY[0]
    }))
}

async fn methods() -> Json<Value> {
    Json(json!({"methods": rpc::methods(), "websocket_only": WS_ONLY}))
}

async fn http_rpc(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Response {
    if body.is_empty() {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({
            "detail": [{"type": "missing", "loc": ["body"], "msg": "Field required", "input": null}]
        }))).into_response();
    }
    let content_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok());
    let is_json = content_type.is_none_or(|value| {
        let mime = value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        mime == "application/json" || (mime.starts_with("application/") && mime.ends_with("+json"))
    });
    let value = if is_json {
        match serde_json::from_slice(&body) {
            Ok(Value::Null) => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({
                "detail": [{"type": "missing", "loc": ["body"], "msg": "Field required", "input": null}]
            }))).into_response(),
            Ok(value) => value,
            Err(error) => {
                let offset = body.split_inclusive(|byte| *byte == b'\n')
                    .take(error.line().saturating_sub(1)).map(<[u8]>::len).sum::<usize>()
                    + error.column().saturating_sub(1);
                return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({
                    "detail": [{"type": "json_invalid", "loc": ["body", offset], "msg": "JSON decode error", "input": {}, "ctx": {"error": format!("{error}")}}]
                }))).into_response();
            }
        }
    } else {
        Value::Null
    };
    let (status, response) = dispatch(app, value, None).await;
    (status, Json(response)).into_response()
}

async fn dispatch(app: App, body: Value, peer: Option<&ws::Peer>) -> (StatusCode, Value) {
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let result = match rpc::parse_request(body) {
        Ok((method, params)) => match (peer, method.as_str()) {
            (Some(peer), "quotes.subscribe") => ws::subscribe(&app, peer, params).await,
            (Some(_), "quotes.unsubscribe") => ws::unsubscribe(&app, params),
            _ => tokio::task::spawn_blocking(move || {
                rpc::call_with_cache(&app.client, &app.collections, &method, params)
            })
            .await
            .unwrap_or_else(|error| Err(rpc::RpcError::internal(format!("{error}")))),
        },
        Err(error) => Err(error),
    };
    match result {
        Ok(value) => (
            StatusCode::OK,
            json!({"id": id, "ok": true, "result": value}),
        ),
        Err(error) => (error.status, error.envelope(id)),
    }
}

async fn fallback(uri: axum::http::Uri) -> Response {
    let path = uri.path();
    let alternate = path.strip_suffix('/').unwrap_or_default();
    if matches!(
        alternate,
        "/health"
            | "/methods"
            | "/rpc"
            | "/openapi.json"
            | "/docs"
            | "/redoc"
            | "/docs/oauth2-redirect"
    ) {
        let query = uri
            .query()
            .map_or_else(String::new, |query| format!("?{query}"));
        return Redirect::temporary(&format!("{alternate}{query}")).into_response();
    }
    (StatusCode::NOT_FOUND, Json(json!({"detail": "Not Found"}))).into_response()
}

async fn openapi() -> Json<Value> {
    Json(json!({
        "openapi": "3.1.0",
        "info": {"title": TITLE, "version": VERSION, "description": "HTTP JSON and WebSocket RPC access to the public eltdx APIs. Use quotes.subscribe for native 0x0547 quote updates."},
        "paths": {
            "/health": {"get": {"summary": "Health", "operationId": "health_health_get",
                "responses": {"200": {"description": "Successful Response", "content": {"application/json": {"schema": {"type": "object"}}}}}}},
            "/methods": {"get": {"summary": "Methods", "operationId": "methods_methods_get",
                "responses": {"200": {"description": "Successful Response", "content": {"application/json": {"schema": {"type": "object"}}}}}}},
            "/rpc": {"post": {"summary": "Rpc", "operationId": "rpc_rpc_post",
                "requestBody": {"required": true, "content": {"application/json": {"schema": {"title": "Body"}}}},
                "responses": {
                    "200": {"description": "Successful Response", "content": {"application/json": {"schema": {}}}},
                    "422": {"description": "Validation Error", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/HTTPValidationError"}}}}
                }}}
        },
        "components": {"schemas": {
            "HTTPValidationError": {"title": "HTTPValidationError", "type": "object", "properties": {
                "detail": {"type": "array", "title": "Detail", "items": {"$ref": "#/components/schemas/ValidationError"}}
            }},
            "ValidationError": {"title": "ValidationError", "type": "object", "required": ["loc", "msg", "type"], "properties": {
                "loc": {"type": "array", "title": "Location", "items": {"anyOf": [{"type": "string"}, {"type": "integer"}]}},
                "msg": {"type": "string", "title": "Message"}, "type": {"type": "string", "title": "Error Type"}
            }}
        }}
    }))
}

async fn docs() -> Html<&'static str> {
    Html(
        r#"<!doctype html><html><head><title>eltdx HTTP/WebSocket gateway - Swagger UI</title>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui.css"></head>
<body><div id="swagger-ui"></div><script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
<script>SwaggerUIBundle({url:'/openapi.json',dom_id:'#swagger-ui',deepLinking:true,oauth2RedirectUrl:location.origin+'/docs/oauth2-redirect'});</script></body></html>"#,
    )
}

async fn redoc() -> Html<&'static str> {
    Html(
        r#"<!doctype html><html><head><title>eltdx HTTP/WebSocket gateway - ReDoc</title></head>
<body><redoc spec-url="/openapi.json"></redoc><script src="https://cdn.jsdelivr.net/npm/redoc@2/bundles/redoc.standalone.js"></script></body></html>"#,
    )
}

async fn oauth_redirect() -> Html<&'static str> {
    Html(
        r#"<!doctype html><html><head><title>Swagger UI: OAuth2 Redirect</title></head><body>
<script>
const oauth = window.opener?.swaggerUIRedirectOauth2;
if (oauth) {
    const args = Object.fromEntries(new URLSearchParams(location.search || location.hash.slice(1)));
    const valid = args.state === oauth.state;
    if (args.code) {
        if (valid) delete oauth.state;
        oauth.auth.code = args.code;
        oauth.callback({auth:oauth.auth,redirectUrl:oauth.redirectUrl});
    } else {
        oauth.callback({auth:oauth.auth,token:args,isValid:valid,redirectUrl:oauth.redirectUrl});
    }
    window.close();
}
</script></body></html>"#,
    )
}
