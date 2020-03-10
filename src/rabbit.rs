use futures::executor::block_on;
use lapin::{
    options::*, types::AMQPValue, types::FieldTable, BasicProperties, Connection,
    ConnectionProperties, ExchangeKind,
};

/// Represents an established rabbit bus connection and prebound queue.
pub struct Rabbit {
    conn: Connection,
    chan: lapin::Channel,
    q: lapin::Queue,
}

impl Rabbit {
    /// Dispatch a message to the rabbit bus with default headers
    pub async fn publish(&self, msg: Vec<u8>) -> Result<(), String> {
        let mut headers = FieldTable::default();
        headers.insert(
            "from-id".into(),
            AMQPValue::LongString(self.q.name().to_string().into()),
        );

        self.chan
            .basic_publish(
                "chrome-ext",
                "",
                BasicPublishOptions::default(),
                msg,
                BasicProperties::default().with_headers(headers),
            )
            .await
            .map_err(|e| format!("Error publishing to rabbit: {}", e))?;

        Ok(())
    }

    /// Create a new rabbit bus connection, getting the AMQP connection string
    /// either from the AMQP environment variable or else defaulting to localhost/guest.
    pub async fn new(ex: &str, q: &str) -> lapin::Result<Rabbit> {
        let addr = std::env::var("AMQP").unwrap_or_else(|_| "amqp://127.0.0.1:5672/%2f".into());

        let conn = Connection::connect(&addr, ConnectionProperties::default()).await?;
        let chan = conn.create_channel().await?;

        let exchange_opts = ExchangeDeclareOptions {
            passive: false,
            durable: false,
            auto_delete: true,
            internal: false,
            nowait: false,
        };

        chan.exchange_declare(
            ex,
            ExchangeKind::Headers,
            exchange_opts,
            FieldTable::default(),
        )
        .await?;

        let mut bindings = FieldTable::default();
        bindings.insert("service".into(), AMQPValue::LongString("chrome-ext".into()));
        bindings.insert("id".into(), AMQPValue::LongString(q.into()));
        bindings.insert("x-match".into(), AMQPValue::LongString("all".into()));

        let queue_opts = QueueDeclareOptions {
            durable: false,
            exclusive: true,
            auto_delete: true,
            nowait: false,
            passive: false,
        };
        let queue = chan
            .queue_declare(q, queue_opts, FieldTable::default())
            .await?;

        chan.queue_bind(q, ex, "", QueueBindOptions::default(), bindings)
            .await?;

        Ok(Rabbit {
            conn,
            chan,
            q: queue
        })
    }

    /// Return the consumer for our pre-built queue.
    pub async fn get_consumer(&self, tag: &str) -> Result<lapin::Consumer, String> {
        self.chan
            .clone()
            .basic_consume(
                self.q.name().as_str(),
                tag,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| format!("Failed to create rabbit consumer: {:?}", e))
    }
}

/// Ensure that the channel and connection are closed when the Rabbit object goes out of scope.
impl Drop for Rabbit {
    fn drop(&mut self) {
        block_on(self.chan.close(200, "client shut down")).unwrap();
        block_on(self.conn.close(200, "client shut down")).unwrap();
    }
}
