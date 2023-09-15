#[derive(Clone)]
pub struct Broadcaster {
    tx: tokio::sync::broadcast::Sender<String>,
}

impl Broadcaster {
    pub fn new() -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        Self { tx }
    }

    pub fn broadcast<T: Into<String>>(&self, msg: T) {
        if let Err(err) = self.tx.send(msg.into()) {
            eprintln!("Error broadcasting message: {}", err)
        }
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.tx.subscribe()
    }
}
