//! Loopback HTTP server for the embedded frontend.
//!
//! WKWebView gives Tauri's custom protocol the origin `tauri://localhost`,
//! and Kuro's socket.io endpoint rejects WebSocket upgrades from it (403).
//! The upgrade is accepted from `http://localhost:1421`, the origin the
//! Windows build/vendor dev server uses, so all windows are loaded from an
//! in-process server bound to that port that serves Tauri's embedded assets.

use std::io::Cursor;

use tauri::AppHandle;
use tiny_http::{Header, Response, Server, StatusCode};

pub const ORIGIN: &str = "http://localhost:1421";
/// `localhost` may resolve to ::1 first in WebKit, so listen on both.
const BINDS: [&str; 2] = ["127.0.0.1:1421", "[::1]:1421"];

pub fn start(app: AppHandle) -> Result<(), String> {
    let mut bound = 0;
    for bind in BINDS {
        match Server::http(bind) {
            Ok(server) => {
                bound += 1;
                serve(app.clone(), server, bind)?;
            }
            Err(e) => log::warn!("[http] bind {bind} failed: {e}"),
        }
    }
    if bound == 0 {
        return Err("could not bind port 1421 on any loopback address".into());
    }
    log::info!("[http] serving frontend at {ORIGIN}");
    Ok(())
}

fn serve(app: AppHandle, server: Server, bind: &'static str) -> Result<(), String> {
    std::thread::Builder::new()
        .name(format!("wuwa-http-{bind}"))
        .spawn(move || {
            let resolver = app.asset_resolver();
            for req in server.incoming_requests() {
                let url = req.url();
                let path = url.split(['?', '#']).next().unwrap_or("/");
                let path = if path == "/" { "/index.html".to_string() } else { path.to_string() };
                let path = percent_decode(&path);
                log::debug!("[http] {} {}", req.method(), req.url());
                match resolver.get(path.clone()) {
                    Some(asset) => {
                        let mime = asset.mime_type().to_string();
                        let bytes = asset.bytes().to_vec();
                        let len = bytes.len();
                        let mut resp = Response::new(
                            StatusCode(200),
                            vec![
                                Header::from_bytes("Content-Type", mime.as_bytes()).unwrap(),
                                Header::from_bytes("Cache-Control", "no-cache").unwrap(),
                            ],
                            Cursor::new(bytes),
                            Some(len),
                            None,
                        );
                        if let Some(csp) = asset.csp_header() {
                            if let Ok(h) = Header::from_bytes("Content-Security-Policy", csp.as_bytes()) {
                                resp.add_header(h);
                            }
                        }
                        let _ = req.respond(resp);
                    }
                    None => {
                        log::warn!("[http] 404 {path}");
                        let _ = req.respond(Response::from_string("not found").with_status_code(404));
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes.get(i + 1)), hex(bytes.get(i + 2))) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: Option<&u8>) -> Option<u8> {
    match b? {
        c @ b'0'..=b'9' => Some(c - b'0'),
        c @ b'a'..=b'f' => Some(c - b'a' + 10),
        c @ b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
