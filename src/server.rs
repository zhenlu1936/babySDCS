use serde_json::Value;
use crate::cache::Cache;
use std::time::Duration;
use std::thread::sleep;
#[allow(unused_imports)]  // Needed for .read_to_string() in handle_post
use std::io::Read;

// helper: try GET with retries. Return Ok((status_code, body)) when owner replies or Err(()) on total failure.
fn rpc_get_with_retry(url: &str) -> Result<(u16, String), ()> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(100))
        .timeout_read(Duration::from_millis(100))
        .timeout_write(Duration::from_millis(100))
        .build();
    let mut i = 0;
    let attempts = 1;

    while i < attempts {
        match agent.get(url).call() {
            Ok(resp) => {
                let status = resp.status() as u16;
                let body = resp.into_string().unwrap_or_default();
                if status >= 500 {
                    // treat 5xx as transient; retry
                    eprintln!("RPC GET to {} attempt {} got {} — retrying", url, i + 1, status);
                } else {
                    return Ok((status, body));
                }
            }
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                if code >= 500 {
                    eprintln!("RPC GET to {} attempt {} got {} — retrying", url, i + 1, code);
                } else {
                    // forward non-5xx (e.g., 404) immediately
                    return Ok((code as u16, body));
                }
            }
            Err(e) => {
                eprintln!("RPC GET to {} attempt {} failed: {}", url, i + 1, e);
            }
        }
        sleep(Duration::from_millis(50));
        i += 1;
    }
    Err(())
}

fn rpc_delete_with_retry(url: &str) -> Result<(u16, String), ()> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(100))
        .timeout_read(Duration::from_millis(100))
        .timeout_write(Duration::from_millis(100))
        .build();
    let mut i = 0;
    let attempts = 1;

    while i < attempts {
        match agent.delete(url).call() {
            Ok(resp) => {
                let status = resp.status() as u16;
                let body = resp.into_string().unwrap_or_default();
                if status >= 500 {
                    eprintln!("RPC DELETE to {} attempt {} got {} — retrying", url, i + 1, status);
                } else {
                    return Ok((status, body));
                }
            }
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                if code >= 500 {
                    eprintln!("RPC DELETE to {} attempt {} got {} — retrying", url, i + 1, code);
                } else {
                    return Ok((code as u16, body));
                }
            }
            Err(e) => {
                eprintln!("RPC DELETE to {} attempt {} failed: {}", url, i + 1, e);
            }
        }
        sleep(Duration::from_millis(50));
        i += 1;
    }
    Err(())
}

fn rpc_post_with_retry(url: &str, body: &str) -> Result<(u16, String), ()> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(100))
        .timeout_read(Duration::from_millis(100))
        .timeout_write(Duration::from_millis(100))
        .build();
    let mut i = 0;
    let attempts = 1;

    while i < attempts {
        match agent
            .post(url)
            .set("Content-Type", "application/json; charset=utf-8")
            .send_string(body)
        {
            Ok(resp) => {
                let status = resp.status() as u16;
                let body = resp.into_string().unwrap_or_default();
                if status >= 500 {
                    eprintln!("RPC POST to {} attempt {} got {} — retrying", url, i + 1, status);
                } else {
                    return Ok((status, body));
                }
            }
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                if code >= 500 {
                    eprintln!("RPC POST to {} attempt {} got {} — retrying", url, i + 1, code);
                } else {
                    return Ok((code as u16, body));
                }
            }
            Err(e) => {
                eprintln!("RPC POST to {} attempt {} failed: {}", url, i + 1, e);
            }
        }
        sleep(Duration::from_millis(50));
        i += 1;
    }
    Err(())
}

/// Starts an HTTP server bound to `addr`. This returns the tiny_http::Server which the caller
/// should pass to `run_server` to begin serving requests.
pub fn init_server(_name: &str, addr: &str) -> (tiny_http::Server, Cache) {
    let server = tiny_http::Server::http(addr).expect(&format!("failed to bind {}", addr));
    let store = Cache::new();
    println!("listening on http://{}", addr);
    (server, store)
}

/// Compute owner index for a key using a simple hash modulo number of peers.
fn owner_for_key(key: &str, peers: &[String]) -> usize {
    let h = seahash::hash(key.as_bytes());
    (h as usize) % peers.len()
}

/// Helper to create JSON response with appropriate headers
fn json_response(status: u16, body: String) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    tiny_http::Response::from_string(body)
        .with_status_code(status)
        .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json; charset=utf-8").unwrap())
}

/// Handle POST / - write/update cache
fn handle_post(
    req: tiny_http::Request,
    name: &str,
    self_addr: &str,
    peers: &[String],
    store: &Cache,
) {
    let mut req = req;  // Mutable needed for as_reader()
    
    // Read request body
    let mut body = String::new();
    if let Err(e) = req.as_reader().read_to_string(&mut body) {
        eprintln!("{}: failed to read body: {}", name, e);
        let _ = req.respond(tiny_http::Response::empty(400));
        return;
    }

    // Parse JSON object
    let map = match serde_json::from_str::<serde_json::Map<String, Value>>(&body) {
        Ok(m) => m,
        Err(_) => {
            let _ = req.respond(tiny_http::Response::empty(400));
            return;
        }
    };

    // Validate single key constraint
    if map.len() != 1 {
        let _ = req.respond(tiny_http::Response::empty(400));
        return;
    }

    let (key, value) = map.into_iter().next().unwrap();
    let owner_idx = owner_for_key(&key, peers);
    let owner = &peers[owner_idx];

    if owner == self_addr {
        // Store locally
        store.set(key.clone(), value.clone());
        let response_body = serde_json::to_string(&serde_json::json!({key: value})).unwrap();
        let _ = req.respond(json_response(200, response_body));
    } else {
        // Forward to owner
        let url = format!("http://{}/", owner);
        match rpc_post_with_retry(&url, &body) {
            Ok((status, text)) => {
                let _ = req.respond(json_response(status, text));
            }
            Err(_) => {
                eprintln!("{}: RPC POST to {} failed after retries", name, url);
                let _ = req.respond(tiny_http::Response::empty(502));
            }
        }
    }
}

/// Handle GET /{key} - read from cache
fn handle_get(
    req: tiny_http::Request,
    name: &str,
    self_addr: &str,
    peers: &[String],
    store: &Cache,
    key: &str,
) {
    if key.is_empty() {
        let _ = req.respond(tiny_http::Response::empty(400));
        return;
    }

    let owner_idx = owner_for_key(key, peers);
    let owner = &peers[owner_idx];

    if owner == self_addr {
        // Local lookup
        if let Some(value) = store.get(key) {
            let response_body = serde_json::to_string(&serde_json::json!({key: value})).unwrap();
            let _ = req.respond(json_response(200, response_body));
        } else {
            let _ = req.respond(tiny_http::Response::empty(404));
        }
    } else {
        // Forward to owner
        let url = format!("http://{}/{}", owner, key);
        match rpc_get_with_retry(&url) {
            Ok((200, text)) => {
                let _ = req.respond(json_response(200, text));
            }
            Ok(_) | Err(_) => {
                // Any non-200 or failure → 404 (hide internal errors from client)
                eprintln!("{}: RPC GET to {} failed — returning 404", name, url);
                let _ = req.respond(tiny_http::Response::empty(404));
            }
        }
    }
}

/// Handle DELETE /{key} - remove from cache
fn handle_delete(
    req: tiny_http::Request,
    name: &str,
    self_addr: &str,
    peers: &[String],
    store: &Cache,
    key: &str,
) {
    if key.is_empty() {
        let _ = req.respond(tiny_http::Response::empty(400));
        return;
    }

    let owner_idx = owner_for_key(key, peers);
    let owner = &peers[owner_idx];

    if owner == self_addr {
        // Local delete
        let removed = store.delete(key);
        let _ = req.respond(json_response(200, removed.to_string()));
    } else {
        // Forward to owner
        let url = format!("http://{}/{}", owner, key);
        match rpc_delete_with_retry(&url) {
            Ok((status, text)) => {
                let _ = req.respond(json_response(status, text));
            }
            Err(_) => {
                eprintln!("{}: RPC DELETE to {} failed after retries", name, url);
                let _ = req.respond(tiny_http::Response::empty(502));
            }
        }
    }
}

/// Handle GET /health - health check endpoint
fn handle_health(req: tiny_http::Request) {
    let _ = req.respond(json_response(200, "{\"status\": \"ok\"}\n".to_string()));
}

/// Run the server loop. `name` is the server name (for logs), `peers` is the ordered list of peer base URLs
/// (including self) used for owner selection and internal RPC. `store` is the in-memory key-value store.
pub fn run_server(server: tiny_http::Server, name: &str, self_addr: String, peers: Vec<String>, store: Cache) {
    println!("{} running on {} with peers: {:?}", name, self_addr, peers);
    
    for request in server.incoming_requests() {
        let method = request.method().as_str().to_string();
        let url = request.url().to_string();
        let peers = peers.clone();
        let store = store.clone();
        let name = name.to_string();
        let self_addr = self_addr.clone();
        
        std::thread::spawn(move || {
            // Route request to appropriate handler
            match (method.as_str(), url.as_str()) {
                ("POST", "/") => {
                    handle_post(request, &name, &self_addr, &peers, &store);
                }
                ("GET", "/health") => {
                    handle_health(request);
                }
                ("GET", path) => {
                    let key = path.trim_start_matches('/');
                    handle_get(request, &name, &self_addr, &peers, &store, key);
                }
                ("DELETE", path) => {
                    let key = path.trim_start_matches('/');
                    handle_delete(request, &name, &self_addr, &peers, &store, key);
                }
                _ => {
                    let _ = request.respond(tiny_http::Response::empty(405));
                }
            }
        });
    }
}