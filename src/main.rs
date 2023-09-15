use std::{error::Error, io::ErrorKind, pin::Pin};

use cfg_if::cfg_if;
use messages::Broadcaster;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
#[cfg(feature = "tls")]
use tonic::transport::{
    server::{TcpConnectInfo, TlsConnectInfo},
    Identity, ServerTlsConfig,
};
use tonic::{transport::Server, Request, Response, Status, Streaming};

use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest, ListMessagesReply, ListMessagesRequest};

pub mod hello_world {
    tonic::include_proto!("helloworld");

    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("helloworld_descriptor");
}

use tonic_hello_tls::db;

type GreeterResult<T> = Result<Response<T>, Status>;
type GreeterResponseStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

fn match_for_io_error(err_status: &Status) -> Option<&std::io::Error> {
    let mut err: &(dyn Error + 'static) = err_status;

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        // h2::Error do not expose std::io::Error with `source()`
        // https://github.com/hyperium/h2/pull/462
        if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
            if let Some(io_err) = h2_err.get_io() {
                return Some(io_err);
            }
        }

        err = match err.source() {
            Some(err) => err,
            None => return None,
        };
    }
}

pub struct MyGreeter {
    db: db::Db,
    broadcaster: messages::Broadcaster,
}

impl MyGreeter {
    pub fn new(db: db::Db, broadcaster: Broadcaster) -> Self {
        Self { db, broadcaster }
    }
}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(&self, request: Request<HelloRequest>) -> GreeterResult<HelloReply> {
        cfg_if! {
            if #[cfg(feature = "tls")] {
                let conn_info = request
                    .extensions()
                    .get::<TlsConnectInfo<TcpConnectInfo>>()
                    .unwrap();
                println!(
                    "Got a request from '{}' with info {:?}",
                    request
                        .remote_addr()
                        .map(|c| c.to_string())
                        .unwrap_or_default(),
                    conn_info
                );
            } else {
                println!(
                    "Got a request from '{}'",
                    request
                        .remote_addr()
                        .map(|c| c.to_string())
                        .unwrap_or_default(),
                );
            }
        }

        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        self.db
            .insert_message(&reply.message)
            .await
            .map_err(|err| Status::new(tonic::Code::Internal, err.to_string()))?;
        self.broadcaster
            .broadcast(&reply.message)
            .map_err(|err| Status::new(tonic::Code::Internal, err.to_string()))?;
        Ok(Response::new(reply))
    }

    type SayHelloStreamStream = GreeterResponseStream<HelloReply>;

    async fn say_hello_stream(
        &self,
        request: Request<Streaming<HelloRequest>>,
    ) -> GreeterResult<Self::SayHelloStreamStream> {
        let remote_addr = request
            .remote_addr()
            .map(|c| c.to_string())
            .unwrap_or_default();
        cfg_if! {
            if #[cfg(feature = "tls")] {
                let conn_info = request
                    .extensions()
                    .get::<TlsConnectInfo<TcpConnectInfo>>()
                    .unwrap();
                println!(
                    "Got a stream request from '{}' with info {:?}",
                    &remote_addr,
                    conn_info
                );
            } else {
                println!(
                    "Got a stream request from '{}'",
                    &remote_addr,
                );
            }
        }

        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(128);

        let db = self.db.clone();
        let broadcaster = self.broadcaster.clone();

        // this spawn here is required if you want to handle connection error.
        // If we just map `in_stream` and write it back as `out_stream` the `out_stream`
        // will be drooped when connection error occurs and error will never be propagated
        // to mapped version of `in_stream`.
        tokio::spawn(async move {
            let db = db.clone();
            let broadcaster = broadcaster.clone();
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(v) => {
                        println!(
                            concat!("\t", r#"received name: "{}" from '{}'"#),
                            v.name, &remote_addr
                        );
                        tx.send(Ok(HelloReply {
                            message: format!("Hello {}!", v.name),
                        }))
                        .await
                        .expect("working rx");
                        if let Err(err) = db.insert_message(&v.name).await {
                            eprintln!("failed to insert message: {}", err);
                        }
                        if let Err(err) = broadcaster.broadcast(&v.name) {
                            eprint!("failed to broadcast message: {}", err);
                        }
                    }
                    Err(err) => {
                        if let Some(io_err) = match_for_io_error(&err) {
                            if io_err.kind() == ErrorKind::BrokenPipe {
                                // here you can handle special case when client
                                // disconnected in unexpected way
                                eprintln!("\tclient disconnected {}: broken pipe", &remote_addr);
                                break;
                            }
                        }

                        match tx.send(Err(err)).await {
                            Ok(_) => (),
                            Err(_err) => break, // response was droped
                        }
                    }
                }
            }
            println!("\tstream ended for {}", &remote_addr);
        });

        // echo just write the same data that was received
        let out_stream = ReceiverStream::new(rx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::SayHelloStreamStream
        ))
    }

    async fn list_messages(
        &self,
        request: Request<ListMessagesRequest>,
    ) -> GreeterResult<ListMessagesReply> {
        cfg_if! {
            if #[cfg(feature = "tls")] {
                let conn_info = request
                    .extensions()
                    .get::<TlsConnectInfo<TcpConnectInfo>>()
                    .unwrap();
                println!(
                    "Got a request from '{}' with info {:?}",
                    request
                        .remote_addr()
                        .map(|c| c.to_string())
                        .unwrap_or_default(),
                    conn_info
                );
            } else {
                println!(
                    "Got a request from '{}'",
                    request
                        .remote_addr()
                        .map(|c| c.to_string())
                        .unwrap_or_default(),
                );
            }
        }
        let messages = self
            .db
            .get_messages()
            .await
            .map_err(|err| Status::new(tonic::Code::Internal, err.to_string()))?;
        let messages = messages
            .into_iter()
            .map(|d| d.message.unwrap_or_default())
            .collect();
        let reply = ListMessagesReply { messages };
        Ok(Response::new(reply))
    }

    type ListMessagesStreamStream = GreeterResponseStream<HelloReply>;

    async fn list_messages_stream(
        &self,
        _request: Request<ListMessagesRequest>,
    ) -> GreeterResult<Self::ListMessagesStreamStream> {
        let mut broadcast_rx = self.broadcaster.subscribe();
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            while let Ok(msg) = broadcast_rx.recv().await {
                let msg = Ok(HelloReply { message: msg });
                match tx.send(msg).await {
                    Ok(_) => (),
                    Err(_) => break,
                }
            }
        });
        let out_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(out_stream) as Self::ListMessagesStreamStream
        ))
    }
}

mod messages {
    type BroadcastError = tokio::sync::broadcast::error::SendError<String>;

    #[derive(Clone)]
    pub struct Broadcaster {
        tx: tokio::sync::broadcast::Sender<String>,
    }

    impl Broadcaster {
        pub fn new() -> Self {
            let (tx, _rx) = tokio::sync::broadcast::channel(16);
            Self { tx }
        }

        pub fn broadcast<T: Into<String>>(&self, msg: T) -> Result<(), BroadcastError> {
            self.tx.send(msg.into())?;
            Ok(())
        }

        pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<String> {
            self.tx.subscribe()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    #[cfg(feature = "tls")]
    let identity = {
        let tls_dir = std::path::PathBuf::from("tls");
        let cert = std::fs::read_to_string(tls_dir.join("server.pem"))?;
        let key = std::fs::read_to_string(tls_dir.join("server.key"))?;

        Identity::from_pem(cert, key)
    };

    let addr = "[::0]:50051".parse().unwrap();

    let db_url = std::env::var("DATABASE_URL")?;
    let db = db::Db::new(&db_url).await?;

    let broadcaster = messages::Broadcaster::new();

    let greeter = MyGreeter::new(db, broadcaster);

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(hello_world::FILE_DESCRIPTOR_SET)
        .build()
        .unwrap();

    println!("GreeterServer listening on {}", addr);

    let mut server_builder = Server::builder();

    cfg_if! {
        if #[cfg(feature = "tls")] {
            server_builder = server_builder.tls_config(ServerTlsConfig::new().identity(identity))?;
        }
    }

    server_builder
        .add_service(reflection_service)
        .add_service(GreeterServer::new(greeter))
        .serve(addr)
        .await?;

    Ok(())
}
