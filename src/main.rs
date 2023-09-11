use std::{error::Error, io::ErrorKind, pin::Pin};

use cfg_if::cfg_if;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
#[cfg(feature = "tls")]
use tonic::transport::{
    server::{TcpConnectInfo, TlsConnectInfo},
    Identity, ServerTlsConfig,
};
use tonic::{transport::Server, Request, Response, Status, Streaming};

use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};

pub mod hello_world {
    tonic::include_proto!("helloworld");

    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("helloworld_descriptor");
}

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

#[derive(Default)]
pub struct MyGreeter {}

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

        // this spawn here is required if you want to handle connection error.
        // If we just map `in_stream` and write it back as `out_stream` the `out_stream`
        // will be drooped when connection error occurs and error will never be propagated
        // to mapped version of `in_stream`.
        tokio::spawn(async move {
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
                        .expect("working rx")
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tls")]
    let identity = {
        let tls_dir = std::path::PathBuf::from("tls");
        let cert = std::fs::read_to_string(tls_dir.join("server.pem"))?;
        let key = std::fs::read_to_string(tls_dir.join("server.key"))?;

        Identity::from_pem(cert, key)
    };

    let addr = "[::0]:50051".parse().unwrap();
    let greeter = MyGreeter::default();

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
