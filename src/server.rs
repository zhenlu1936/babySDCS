use serde_json::Value;
use crate::cache::Cache;

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
            // make a mutable local binding so we can read and respond
            let mut req = request;
            // path could be "/" for POST or "/{key}" for GET/DELETE
            if method == "POST" && url == "/" {
                // read body
                let mut body = String::new();
                if let Err(e) = req.as_reader().read_to_string(&mut body) {
                    eprintln!("{}: failed to read body: {}", name, e);
                    let _ = req.respond(tiny_http::Response::empty(400));
                    return;
                }
                // parse JSON object
                match serde_json::from_str::<serde_json::Map<String, Value>>(&body) {
                    Ok(map) => {
                        // for each KV in JSON, store using owner selection (but requirement says single key per request; so we'll handle single entry)
                        if map.len() != 1 {
                            // respond 400 if not single key
                            let _ = req.respond(tiny_http::Response::empty(400));
                            return;
                        }
                        let (k, v) = map.into_iter().next().unwrap();
                        let owner_idx = owner_for_key(&k, &peers);
                        let owner = &peers[owner_idx];
                        if owner == &self_addr {
                            // store locally
                            store.set(k.clone(), v.clone());
                            let resp = tiny_http::Response::from_string(serde_json::to_string(&serde_json::json!({k: v})).unwrap())
                                .with_status_code(200)
                                .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json; charset=utf-8").unwrap());
                            let _ = req.respond(resp);
                        } else {
                            // forward to owner via internal HTTP POST
                            let url = format!("http://{}{}/", owner, "");
                            match ureq::post(&url).set("Content-Type", "application/json; charset=utf-8").send_string(&body) {
                                Ok(resp) => {
                                    let text = resp.into_string().unwrap_or_default();
                                    let response = tiny_http::Response::from_string(text)
                                        .with_status_code(200)
                                        .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json; charset=utf-8").unwrap());
                                    let _ = req.respond(response);
                                }
                                Err(e) => {
                                    eprintln!("{}: RPC POST to {} failed: {}", name, url, e);
                                    let _ = req.respond(tiny_http::Response::empty(502));
                                }
                            }
                        }
                    }
                    Err(_) => {
                            let _ = req.respond(tiny_http::Response::empty(400));
                    }
                }
            } else if method == "GET" {
                // health endpoint
                if url == "/health" {
                    let resp = tiny_http::Response::from_string("{\"status\": \"ok\"}\n")
                        .with_status_code(200)
                        .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json; charset=utf-8").unwrap());
                    let _ = req.respond(resp);
                    return;
                }
                // extract key from path
                let key = url.trim_start_matches('/');
                if key.is_empty() {
                    let _ = req.respond(tiny_http::Response::empty(400));
                    return;
                }
                let owner_idx = owner_for_key(key, &peers);
                let owner = &peers[owner_idx];
                if owner == &self_addr {
                    // local
                    if let Some(v) = store.get(key) {
                        let body = serde_json::to_string(&serde_json::json!({key: v})).unwrap();
                        let resp = tiny_http::Response::from_string(body)
                            .with_status_code(200)
                            .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json; charset=utf-8").unwrap());
                        let _ = req.respond(resp);
                    } else {
                        let _ = req.respond(tiny_http::Response::empty(404));
                    }
                } else {
                    // RPC GET to owner
                    let url = format!("http://{}/{}", owner, key);
                    match ureq::get(&url).call() {
                        Ok(resp) => {
                            let text = resp.into_string().unwrap_or_default();
                            let response = tiny_http::Response::from_string(text)
                                .with_status_code(200)
                                .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json; charset=utf-8").unwrap());
                            let _ = req.respond(response);
                        }
                        Err(e) => {
                            eprintln!("{}: RPC GET to {} failed: {}", name, url, e);
                            let _ = req.respond(tiny_http::Response::empty(502));
                        }
                    }
                }
            } else if method == "DELETE" {
                let key = url.trim_start_matches('/');
                if key.is_empty() {
                    let _ = req.respond(tiny_http::Response::empty(400));
                    return;
                }
                let owner_idx = owner_for_key(key, &peers);
                let owner = &peers[owner_idx];
                if owner == &self_addr {
                    // local delete
                    let removed = store.delete(key);
                    let resp = tiny_http::Response::from_string(format!("{}", removed))
                        .with_status_code(200)
                        .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json; charset=utf-8").unwrap());
                    let _ = req.respond(resp);
                } else {
                    // forward delete to owner
                    let url = format!("http://{}/{}", owner, key);
                    match ureq::delete(&url).call() {
                        Ok(resp) => {
                            let text = resp.into_string().unwrap_or_default();
                            let response = tiny_http::Response::from_string(text)
                                .with_status_code(200)
                                .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json; charset=utf-8").unwrap());
                            let _ = req.respond(response);
                        }
                        Err(e) => {
                            eprintln!("{}: RPC DELETE to {} failed: {}", name, url, e);
                            let _ = req.respond(tiny_http::Response::empty(502));
                        }
                    }
                }
            } else {
                let _ = req.respond(tiny_http::Response::empty(405));
            }
        });
    }
}