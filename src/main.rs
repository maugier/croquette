use anyhow::Result;

mod proxy;
mod event;
mod util;

#[tokio::main]
pub async fn main() -> Result<()> {

    env_logger::init();

    let mut args = std::env::args();

    if args.len() != 3 {
        eprintln!("Usage: croquette <IRC bind address> <rocket server>");
        eprintln!("   Example: croquette 0.0.0.0:6668 rocket.example.com");
        eprintln!("   export RUST_LOG=[error|warn|info|debug|trace] for logging");
        return Ok(());
    }

    args.next();
    let bind = args.next().unwrap();
    let backend = args.next().unwrap();

    //let bind, backend;

    let proxy = proxy::ProxyListener::new(bind, backend);
    proxy.run().await
}
