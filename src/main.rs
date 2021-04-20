use anyhow::Result;

mod proxy;


#[tokio::main]
pub async fn main() -> Result<()> {

    let mut args = std::env::args();

    if args.len() != 3 {
        eprintln!("Usage: croquette <IRC bind address> <rocket server>\n    Example: croquette 0.0.0.0:6668 rocket.example.com");
        return Ok(());
    }

    args.next();
    let bind = args.next().unwrap();
    let backend = args.next().unwrap();

    //let bind, backend;

    let proxy = proxy::Proxy::new(bind, backend);
    proxy.run().await
}
