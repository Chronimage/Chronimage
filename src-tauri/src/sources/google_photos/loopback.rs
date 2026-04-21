//! One-shot loopback HTTP listener for the OAuth2 redirect.
//!
//! Binds `127.0.0.1:<ephemeral>`, accepts one TCP connection, parses the
//! request line's query string for `code` + `state`, serves a minimal HTML
//! success (or error) page, and hands the parsed payload back via a
//! `tokio::sync::oneshot`. Times out and drops the sender if the browser
//! never lands.
//!
//! Deliberately hand-rolled instead of pulling in `hyper` + `tower` — the
//! request line + a handful of headers is ~40 lines of byte parsing.

use crate::{AppError, AppResult};
use std::{collections::HashMap, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

/// Parsed `?code=…&state=…` query from the redirect.
#[derive(Debug, Clone)]
pub struct RedirectPayload {
    pub code: String,
    pub state: String,
}

/// Spawn the listener. Returns the `redirect_uri` to include in the auth
/// URL (the fully-qualified `http://127.0.0.1:<port>/`) and a receiver that
/// resolves with either the parsed payload or a descriptive error.
///
/// The sender is dropped after `timeout`; callers should treat the
/// `oneshot::error::RecvError` as a browser no-show.
pub async fn spawn_listener(
    timeout: Duration,
) -> AppResult<(String, oneshot::Receiver<AppResult<RedirectPayload>>)> {
    // Bind with port 0 → OS hands out an ephemeral free port.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| AppError::Internal(format!("loopback bind: {e}")))?;
    let addr = listener
        .local_addr()
        .map_err(|e| AppError::Internal(format!("loopback local_addr: {e}")))?;
    let redirect_uri = format!("http://127.0.0.1:{}/", addr.port());

    let (tx, rx) = oneshot::channel::<AppResult<RedirectPayload>>();

    tokio::spawn(async move {
        let accept_fut = listener.accept();
        let result = tokio::time::timeout(timeout, accept_fut).await;
        match result {
            Err(_) => {
                // Timed out; drop tx so the receiver observes RecvError.
                drop(tx);
            }
            Ok(Err(e)) => {
                let _ = tx.send(Err(AppError::Internal(format!("accept: {e}"))));
            }
            Ok(Ok((stream, _peer))) => match handle_connection(stream).await {
                Ok(payload) => {
                    let _ = tx.send(Ok(payload));
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            },
        }
    });

    Ok((redirect_uri, rx))
}

/// Read the request line, write a minimal success page, return the parsed
/// query.
async fn handle_connection(mut stream: TcpStream) -> AppResult<RedirectPayload> {
    let mut buf = vec![0u8; 8192];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| AppError::Internal(format!("loopback read: {e}")))?;
    if n == 0 {
        return Err(AppError::Internal("loopback empty request".into()));
    }
    let head = std::str::from_utf8(&buf[..n])
        .map_err(|e| AppError::Internal(format!("loopback utf8: {e}")))?;

    let request_line = head
        .split("\r\n")
        .next()
        .ok_or_else(|| AppError::Internal("loopback empty head".into()))?;
    let mut parts = request_line.split_whitespace();
    let _method = parts.next();
    let target = parts
        .next()
        .ok_or_else(|| AppError::Internal("loopback no target".into()))?;

    // We only care about GET /?... — ignore favicon etc. by looking for `?`.
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let params = parse_query(query);

    // Success if we got a code; otherwise treat `error` from Google as auth
    // failure (user denied consent etc.).
    let response_body;
    let payload_result: AppResult<RedirectPayload>;
    if let Some(err) = params.get("error") {
        payload_result = Err(AppError::PermissionDenied(format!(
            "google oauth consent denied: {err}"
        )));
        response_body = html_page(
            "Chronimage — sign-in failed",
            "Google returned an error. You can close this tab and try again.",
        );
    } else if let (Some(code), Some(state)) = (params.get("code"), params.get("state")) {
        payload_result = Ok(RedirectPayload {
            code: code.clone(),
            state: state.clone(),
        });
        response_body = html_page(
            "Chronimage — connected",
            "You can close this tab and return to Chronimage.",
        );
    } else {
        payload_result = Err(AppError::Internal(format!(
            "loopback redirect missing code/state (query: {query})"
        )));
        response_body = html_page(
            "Chronimage — unexpected redirect",
            "Missing `code` or `state`. Try signing in again.",
        );
    }

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
    // Politely shut down the write side before dropping the stream.
    let _ = stream.shutdown().await;

    payload_result
}

/// Parse `key=value&key=value` with minimal percent-decoding for the two
/// keys we care about (`code`, `state`, `error`). Accepts duplicate keys by
/// taking the last occurrence.
fn parse_query(q: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        out.insert(percent_decode(k), percent_decode(v));
    }
    out
}

/// Minimal percent-decoder. Unknown sequences pass through unchanged.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

fn html_page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>{title}</title>\
         <style>body{{font-family:system-ui,-apple-system,sans-serif;\
         background:#0b0b0d;color:#e8e8ea;max-width:420px;margin:80px auto;\
         padding:32px;border-radius:12px;text-align:center}}\
         h1{{font-size:18px;margin:0 0 12px}}p{{color:#a8a8ad;margin:0}}</style>\
         </head><body><h1>{title}</h1><p>{body}</p></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_handles_empty() {
        assert!(parse_query("").is_empty());
    }

    #[test]
    fn parse_query_extracts_code_and_state() {
        let q = parse_query("code=ABC123&state=xyz");
        assert_eq!(q.get("code").map(String::as_str), Some("ABC123"));
        assert_eq!(q.get("state").map(String::as_str), Some("xyz"));
    }

    #[test]
    fn parse_query_percent_decodes() {
        let q = parse_query("code=a%20b&state=hello%2Fworld");
        assert_eq!(q.get("code").map(String::as_str), Some("a b"));
        assert_eq!(q.get("state").map(String::as_str), Some("hello/world"));
    }

    #[test]
    fn parse_query_plus_is_space() {
        let q = parse_query("code=a+b");
        assert_eq!(q.get("code").map(String::as_str), Some("a b"));
    }

    #[test]
    fn parse_query_returns_error_param_when_present() {
        let q = parse_query("error=access_denied&state=xyz");
        assert_eq!(q.get("error").map(String::as_str), Some("access_denied"));
    }

    #[tokio::test]
    async fn listener_times_out_when_no_connection() {
        let (_redirect, rx) = spawn_listener(Duration::from_millis(50))
            .await
            .expect("spawn");
        // No browser ever lands → sender is dropped → RecvError.
        let result = rx.await;
        assert!(result.is_err(), "expected timeout, got {result:?}");
    }

    #[tokio::test]
    async fn listener_delivers_parsed_payload() {
        let (redirect, rx) = spawn_listener(Duration::from_secs(5)).await.expect("spawn");
        // Dial the listener and send a minimal GET /?code=X&state=Y.
        let port: u16 = redirect
            .trim_start_matches("http://127.0.0.1:")
            .trim_end_matches('/')
            .parse()
            .expect("port");
        let handle = tokio::spawn(async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).await.expect("dial");
            let req = b"GET /?code=live-code&state=live-state HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
            stream.write_all(req).await.expect("write");
            // Read + discard the response so we keep the TCP peer alive
            // until the server's write_all returns.
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
        });
        let payload = rx.await.expect("recv").expect("ok");
        handle.await.expect("client task");
        assert_eq!(payload.code, "live-code");
        assert_eq!(payload.state, "live-state");
    }

    #[tokio::test]
    async fn listener_surfaces_error_param() {
        let (redirect, rx) = spawn_listener(Duration::from_secs(5)).await.expect("spawn");
        let port: u16 = redirect
            .trim_start_matches("http://127.0.0.1:")
            .trim_end_matches('/')
            .parse()
            .expect("port");
        let handle = tokio::spawn(async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).await.expect("dial");
            let req = b"GET /?error=access_denied&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
            stream.write_all(req).await.expect("write");
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
        });
        let recv = rx.await.expect("recv");
        handle.await.expect("client task");
        assert!(matches!(recv, Err(AppError::PermissionDenied(_))));
    }
}
