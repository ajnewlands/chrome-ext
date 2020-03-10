use futures::executor::block_on;
use lapin::{
    options::*, types::AMQPValue, types::FieldTable, BasicProperties, Connection,
    ConnectionProperties, ExchangeKind,
};
use uuid::Uuid;

pub struct Rabbit {
    conn: Connection,
    chan: lapin::Channel,
    q: lapin::Queue,
    pub consumer: lapin::Consumer,
}

impl Rabbit {
    pub async fn publish(&self, msg: Vec<u8>) -> Result<(), String> {
        self.chan
            .basic_publish(
                "chrome-ext",
                "",
                BasicPublishOptions::default(),
                msg,
                BasicProperties::default(),
            )
            .await
            .map_err(|e| format!("Error publishing to rabbit: {}", e))?;

        Ok(())
    }

    pub async fn new(ex: &str, q: &str) -> lapin::Result<Rabbit> {
        let addr =
            std::env::var("AMQP_ADDR").unwrap_or_else(|_| "amqp://127.0.0.1:5672/%2f".into());

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
        bindings.insert(
            "id".into(),
            AMQPValue::LongString(Uuid::new_v4().to_string().into()),
        );

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

        let consumer = chan
            .clone()
            .basic_consume(
                q,
                "my tag",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok(Rabbit {
            conn,
            chan,
            q: queue,
            consumer,
        })
    }
}

impl Drop for Rabbit {
    fn drop(&mut self) {
        block_on(self.chan.close(200, "client shut down")).unwrap();
        block_on(self.conn.close(200, "client shut down")).unwrap();
    }
}
