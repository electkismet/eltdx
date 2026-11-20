use std::collections::{hash_map::RandomState, HashMap, VecDeque};
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use eltdx::{
    CancellationToken, ErrorKind, OperationOptions, PushEvent, RawPolicy, SubscriptionOptions,
};
use serde_json::{json, Value};
use tokio::sync::Notify;

use super::rpc::{quote, securities, Result, RpcError};
use super::{dispatch, App};

const MAX_EVENTS: usize = 256;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Default)]
pub(super) struct Subscriptions(Arc<Mutex<HashMap<String, Registration>>>);

struct Registration {
    peer: u64,
    token: CancellationToken,
}

pub(super) struct Peer {
    id: u64,
    subscriptions: Subscriptions,
    events: Arc<Events>,
}

impl Drop for Peer {
    fn drop(&mut self) {
        if let Ok(mut registrations) = self.subscriptions.0.lock() {
            registrations.retain(|_, registration| {
                if registration.peer == self.id {
                    registration.token.cancel();
                    false
                } else {
                    true
                }
            });
        }
    }
}

#[derive(Default)]
struct Events {
    queue: Mutex<VecDeque<Value>>,
    ready: Notify,
}

impl Events {
    fn push(&self, value: Value) {
        if let Ok(mut queue) = self.queue.lock() {
            if queue.len() == MAX_EVENTS {
                queue.pop_front();
            }
            queue.push_back(value);
            self.ready.notify_one();
        }
    }

    async fn next(&self) -> Option<Value> {
        loop {
            if let Some(event) = self.queue.lock().ok()?.pop_front() {
                return Some(event);
            }
            self.ready.notified().await;
        }
    }
}

pub(super) async fn upgrade(State(app): State<App>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| session(socket, app))
}

async fn session(mut socket: WebSocket, app: App) {
    let peer = Peer {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        subscriptions: app.subscriptions.clone(),
        events: Arc::new(Events::default()),
    };
    let mut shutdown = app.shutdown.clone();
    loop {
        if *shutdown.borrow() {
            break;
        }
        let outgoing = tokio::select! {
            biased;
            _ = shutdown.changed() => break,
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str(text.as_str()) {
                        Ok(body) => tokio::select! {
                            _ = shutdown.changed() => break,
                            (_, response) = dispatch(app.clone(), body, Some(&peer)) => response,
                        },
                        Err(error) => RpcError::value(format!("{error}")).envelope(Value::Null),
                    }
                }
                Some(Ok(Message::Binary(_))) => RpcError::internal("WebSocket JSON messages must be text").envelope(Value::Null),
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                _ => break,
            },
            event = peer.events.next() => match event {
                Some(event) => event,
                None => break,
            }
        };
        let message = Message::Text(format!("{outgoing}").into());
        if !matches!(
            tokio::select! {
                _ = shutdown.changed() => break,
                sent = socket.send(message) => sent,
            },
            Ok(())
        ) {
            break;
        }
    }
}

pub(super) async fn subscribe(app: &App, peer: &Peer, params: Value) -> Result<Value> {
    let params = params
        .as_object()
        .ok_or_else(|| RpcError::request("quotes.subscribe params must be an object"))?;
    let values = params
        .get("codes")
        .filter(|value| value.is_string() || value.is_array())
        .ok_or_else(|| RpcError::request("quotes.subscribe requires a codes array"))?;
    if let Some(values) = values.as_array() {
        if values.is_empty() {
            return Err(RpcError::request("codes must not be empty"));
        }
        if values.len() > 100 {
            return Err(RpcError::request("codes accepts at most 100 securities"));
        }
    }
    let mut seen = std::collections::HashSet::new();
    let ids = securities(values)
        .map_err(|error| RpcError::request(error.message()))?
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect::<Vec<_>>();
    let client = app.client.clone();
    let codes = ids.iter().map(|id| format!("{id}")).collect::<Vec<_>>();
    let (initial, mut subscription) = tokio::task::spawn_blocking(move || {
        let options = OperationOptions::default().with_raw(RawPolicy::RetainPayload);
        let initial = quote::refresh_page(&client.quotes().depth(&ids, &options)?);
        let subscription =
            client
                .quotes()
                .subscribe(&ids, &SubscriptionOptions::default(), &options)?;
        Ok::<_, RpcError>((initial, subscription))
    })
    .await
    .map_err(|error| RpcError::internal(format!("{error}")))??;

    let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let high = (RandomState::new().hash_one(serial) & !0xf000) | 0x4000;
    let low = (RandomState::new().hash_one(serial) & 0x3fff_ffff_ffff_ffff) | 0x8000_0000_0000_0000;
    let id = format!("{high:016x}{low:016x}");
    let token = CancellationToken::default();
    app.subscriptions
        .0
        .lock()
        .map_err(|_| RpcError::internal("subscription registry unavailable"))?
        .insert(
            id.clone(),
            Registration {
                peer: peer.id,
                token: token.clone(),
            },
        );
    let events = Arc::clone(&peer.events);
    let subscription_id = id.clone();
    tokio::task::spawn_blocking(move || {
        let options = OperationOptions::default()
            .with_timeout(Duration::from_millis(500))
            .with_cancellation(token.clone());
        while !token.is_cancelled() {
            let event = match subscription.next_event(&options) {
                Ok(Some(PushEvent::Update(record))) => json!({
                    "event": "quote", "subscription_id": subscription_id, "data": quote::update(&record)
                }),
                Ok(Some(PushEvent::Gap(gap))) => json!({
                    "event": "error", "subscription_id": subscription_id,
                    "error": {"type": "PushOverflowError", "message": format!("lost {} push events ({:?})", gap.dropped(), gap.reason())}
                }),
                Ok(None) => break,
                Ok(Some(_)) => continue,
                Err(error) if error.kind() == ErrorKind::Timeout => continue,
                Err(_) if token.is_cancelled() => break,
                Err(error) => {
                    events.push(json!({"event": "error", "subscription_id": subscription_id, "error": RpcError::from(error).detail()}));
                    break;
                }
            };
            if !token.is_cancelled() {
                events.push(event);
            }
        }
        let _ = subscription.close();
    });
    Ok(json!({"subscription_id": id, "codes": codes, "initial": initial}))
}

pub(super) fn unsubscribe(app: &App, params: Value) -> Result<Value> {
    let params = params
        .as_object()
        .ok_or_else(|| RpcError::request("quotes.unsubscribe params must be an object"))?;
    let id = params
        .get("subscription_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| RpcError::request("quotes.unsubscribe requires a subscription_id string"))?;
    let removed = app
        .subscriptions
        .0
        .lock()
        .map_err(|_| RpcError::internal("subscription registry unavailable"))?
        .remove(id);
    if let Some(registration) = &removed {
        registration.token.cancel();
    }
    Ok(json!({"subscription_id": id, "removed": removed.is_some()}))
}
