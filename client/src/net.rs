//! Les appels à l'API d'Alley (`fetch`), en JSON.

use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, RequestCache, RequestInit, Response};

use crate::dom::{meta, window};
use crate::model::app;

/// Une erreur d'API : le statut HTTP (0 sans réseau) et un message lisible.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl ApiError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        ApiError {
            status,
            message: message.into(),
        }
    }
}

pub type ApiResult<T = Value> = Result<T, ApiError>;

/// L'API : celle choisie dans les réglages, sinon celle de la page (vide :
/// le même site).
pub fn api_base() -> String {
    let saved = app(|a| a.server.clone());
    let base = if saved.is_empty() {
        meta("alley-api")
    } else {
        saved
    };
    base.trim_end_matches('/').to_owned()
}

/// Sur GitHub Pages sans API configurée, il faut une adresse de serveur.
pub fn needs_server() -> bool {
    api_base().is_empty()
        && window()
            .location()
            .hostname()
            .is_ok_and(|h| h.ends_with(".github.io"))
}

/// L'adresse de la WebSocket de l'API (`wss://…`).
pub fn ws_base() -> String {
    let base = api_base();
    let base = if base.is_empty() {
        window().location().origin().unwrap_or_default()
    } else {
        base
    };
    if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base
    }
}

pub async fn request(
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Option<&Value>,
) -> ApiResult {
    let h = Headers::new().map_err(|_| ApiError::new(0, "en-têtes"))?;
    for (k, v) in headers {
        let _ = h.append(k, v);
    }
    let init = RequestInit::new();
    init.set_method(method);
    init.set_cache(RequestCache::NoStore);
    if let Some(body) = body {
        let _ = h.append("content-type", "application/json");
        init.set_body(&JsValue::from_str(&body.to_string()));
    }
    init.set_headers(&h);
    let url = format!("{}{path}", api_base());
    let res = JsFuture::from(window().fetch_with_str_and_init(&url, &init))
        .await
        .map_err(|_| ApiError::new(0, "Pas de connexion au serveur Alley."))?;
    let res: Response = res.unchecked_into();
    let status = res.status();
    if status == 204 {
        return Ok(Value::Null);
    }
    let text = match res.text() {
        Ok(p) => JsFuture::from(p)
            .await
            .ok()
            .and_then(|t| t.as_string())
            .unwrap_or_default(),
        Err(_) => String::new(),
    };
    let data: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !res.ok() {
        let message = ["message", "detail", "title"]
            .iter()
            .find_map(|k| data[k].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Erreur {status}"));
        return Err(ApiError::new(status, message));
    }
    Ok(data)
}
