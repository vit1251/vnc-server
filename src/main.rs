
use std::error::Error;

mod rfb;


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {

    env_logger::init();

    let rfb_server = rfb::RfbServerBuilder::new()
        .build();

    rfb_server.run().await;

    Ok(())
}
