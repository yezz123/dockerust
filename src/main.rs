use std::path::Path;
use std::process;
use dockerust::server;
use dockerust::server::ServerConfig;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // TODO : change arguments processing to add more field (like auth) with write support addition
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("Usage: {} [storage_path] [listen_address]", args[0]);
        process::exit(-1);
    }

    let storage_path = Path::new(&args[1]);
    let listen_address = &args[2];

    if !storage_path.exists() {
        eprintln!("Specified storage path does not exists!");
        process::exit(-2);
    }

    println!("Server will start to listen on {}", listen_address);

    server::start(ServerConfig {
        storage_path: storage_path.to_path_buf(),
        listen_address: listen_address.to_string(),
    }).await
}
