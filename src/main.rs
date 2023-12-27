use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::process;

use bcrypt::DEFAULT_COST;

use dockerust::server;
use dockerust::server::{Credentials, ServerConfig};
use dockerust::storage::clean_storage;
use dockerust::utils::{rand_str, request_input};

fn show_usage() {
    let args = std::env::args().collect::<Vec<_>>();
    eprintln!("Usage: {} {{init-config|serve|add_user}} [conf_file]", args[0]);
    process::exit(-1);
}

fn init_config(conf_path: &Path) -> std::io::Result<()> {
    if conf_path.exists() {
        eprintln!("Configuration file already exists!");
        process::exit(-4);
    }

    let conf = ServerConfig {
        storage_path: PathBuf::from(request_input("storage path")?),
        listen_address: request_input("listen_address (ex: 127.0.0.1:45654)")?,
        access_url: request_input("access_url")?,
        app_secret: rand_str(50),
        credentials: vec![],
    };

    std::fs::write(
        conf_path,
        serde_yaml::to_string(&conf).map_err(|_| Error::new(ErrorKind::Other, "failed to deserialize"))?,
    )?;

    Ok(())
}

fn add_user(conf_path: &Path) -> std::io::Result<()> {
    if !conf_path.exists() {
        eprintln!("Configuration file does not exists!");
        process::exit(-5);
    }

    let mut conf: ServerConfig = serde_yaml::from_str(&std::fs::read_to_string(conf_path)?)
        .map_err(|_| Error::new(ErrorKind::Other, "failed to deserialize"))?;

    conf.credentials.push(Credentials {
        user_name: request_input("user name")?,
        password_hash: bcrypt::hash(request_input("password")?, DEFAULT_COST)
            .map_err(|_| Error::new(ErrorKind::Other, "failed to hash password"))?,
    });

    std::fs::write(
        conf_path,
        serde_yaml::to_string(&conf).map_err(|_| Error::new(ErrorKind::Other, "failed to serialize config"))?,
    )?;

    println!("User added.");

    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        show_usage();
    }

    let conf_path: &Path = args[2].as_ref();

    match args[1].as_str() {
        "serve" => { /* Default usage*/ }
        "init-config" => init_config(conf_path)?,
        "add_user" => add_user(conf_path)?,
        _ => show_usage(),
    }

    if !conf_path.exists() {
        eprintln!("Specified configuration file does not exists!");
        process::exit(-2);
    }

    let config: ServerConfig = serde_yaml::from_str(&std::fs::read_to_string(conf_path)?)
        .map_err(|_| Error::new(ErrorKind::Other, "failed to deserialize"))?;

    if !config.storage_path.exists() {
        eprintln!("Specified storage path does not exists!");
        process::exit(-3);
    }

    println!("Cleaning storage...");
    clean_storage(&config.storage_path).unwrap();

    println!("Server will start to listen on {}", config.listen_address);

    server::start(config).await
}
