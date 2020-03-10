#![recursion_limit = "256"]
use bytes::Bytes;
use futures::prelude::*;
use futures::{select, StreamExt};
use futures_util::sink::SinkExt;
use uuid::Uuid;

mod browser;
mod rabbit;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::init();

    let mut browser_writer = browser::writer(tokio::io::stdout());

    let id = Uuid::new_v4().to_string();
    let rabbit = rabbit::Rabbit::new("chrome-ext", &id)
        .await
        .map_err(|e| format!("Failed to initialize rabbit: {:?}", e))?;

    let mut fut1 = browser::reader(tokio::io::stdin()).into_future().fuse();
    let mut fut2 = rabbit.get_consumer("my tag").await?.into_future().fuse();

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
                        browser_writer.send(Bytes::from(delivery.data)).await.map_err(|e| format!("Error sending data to browser: {}",e))?;
                        Ok(fut2 = consumer.into_future().fuse())
                },
                Some(Err(e)) => Err(format!("Rabbit error: {:?}", e)),
                None => Err(String::from("Rabbit connection was shut down at source")),
            },
        );
    }

    res
}
