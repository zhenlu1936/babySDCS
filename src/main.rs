use babySDCS::server;
use std::env;

fn main() {
    // If PEERS env var is set, run in container/single-node mode (useful for docker-compose).
    // PEERS should be a comma-separated list of peer addresses (e.g. server1:8001,server2:8002,server3:8003)
    if let Ok(peers_env) = env::var("PEERS") {
        let peers: Vec<String> = peers_env.split(',').map(|s| s.to_string()).collect();
        let port = env::var("PORT").unwrap_or_else(|_| {
            // fallback to last part of first peer
            peers
                .get(0)
                .and_then(|p| p.split(':').last())
                .unwrap_or("8001")
                .to_string()
        });
    let bind_addr = format!("0.0.0.0:{}", port);
    let name = env::var("NAME").unwrap_or_else(|_| format!("server{}", port));
    // self_addr should match the peer entries (e.g. server1:8001)
    let self_addr = format!("{}:{}", name, port);
    let (srv, store) = server::init_server(&name, &bind_addr);
    server::run_server(srv, &name, self_addr, peers, store);
        return;
    }

    // Default local dev: spawn three HTTP servers: server1..server3 on ports 8001..8003
    let peers: Vec<String> = (1..=3).map(|i| format!("127.0.0.1:{}", 8000 + i)).collect();

    for i in 1..=3 {
        let name = format!("server{}", i);
        let port = 8000 + i;
        let addr = format!("127.0.0.1:{}", port);
        let peers = peers.clone();
        std::thread::spawn(move || {
            let (srv, store) = server::init_server(&name, &addr);
            server::run_server(srv, &name, addr.clone(), peers, store);
        });
    }

    // keep main thread alive
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}