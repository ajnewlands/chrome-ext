#![recursion_limit = "256"]
use bytes::Bytes;
use futures::prelude::*;
use futures::{select, StreamExt};
use futures_util::sink::SinkExt;
use serde_json::json;
use tokio_util::codec::length_delimited;
mod rabbit;
use log::{error, info};
use rabbit::Rabbit;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::init();
    let output = tokio::io::stdout();

    let rabbit = Rabbit::new("chrome-ext", "chrome-ext")
        .await
        .map_err(|e| format!("Failed to initialize rabbit: {:?}", e))?;

    let mut transport = length_delimited::Builder::new()
        .little_endian()
        .new_write(output);
    /*
        let message = json!({
            "type" : "go_to_url",
            "url": "https://www.slashdot.com/"
        });

        transport
            .send(Bytes::from(serde_json::to_vec(&message).unwrap()))
            .await
            .map_err(|e| format!("Error sending to browser: {}", e))?;
    */
    let input = tokio::io::stdin();
    let mut fut1 = length_delimited::Builder::new()
        .little_endian()
        .new_read(input)
        .into_future()
        .fuse();

    let mut fut2 = rabbit.consumer.clone().into_future().fuse();

    let mut res: Result<(), String> = Ok(());
    while res == Ok(()) {
        res = select!(
            (msg, stream) = fut1 => match msg {
                    Some(Ok(message)) => {
                        rabbit.publish(message.to_vec()).await?;
                        Ok(fut1 = stream.into_future().fuse())
                    },
                    Some(Err(e)) => Err(format!("Error reading from browser: {}",e)),
                    None => Err(String::from("Browser connection was shut down at source")),
            },
            (msg, consumer) = fut2 => match msg {
                Some(Ok(delivery)) => {
                        println!("{:?}", delivery);
                        Ok(fut2 = consumer.into_future().fuse())
                },
                Some(Err(e)) => Err(format!("Rabbit error: {:?}", e)),
                None => Err(String::from("Rabbit connection was shut down at source")),
            },
        );
    }

    res
}
