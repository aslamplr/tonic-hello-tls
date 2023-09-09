use tonic::{
    transport::{
        server::{TcpConnectInfo, TlsConnectInfo},
        Identity, Server, ServerTlsConfig,
    },
    Request, Response, Status,
};

use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};

pub mod hello_world {
    tonic::include_proto!("helloworld");

    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("helloworld_descriptor");
}

#[derive(Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
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

        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tls_dir = std::path::PathBuf::from("tls");
    let cert = std::fs::read_to_string(tls_dir.join("server.pem"))?;
    let key = std::fs::read_to_string(tls_dir.join("server.key"))?;

    let identity = Identity::from_pem(cert, key);

    let addr = "[::1]:50051".parse().unwrap();
    let greeter = MyGreeter::default();

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(hello_world::FILE_DESCRIPTOR_SET)
        .build()
        .unwrap();

    println!("GreeterServer listening on {}", addr);

    Server::builder()
        .tls_config(ServerTlsConfig::new().identity(identity))?
        .add_service(reflection_service)
        .add_service(GreeterServer::new(greeter))
        .serve(addr)
        .await?;

    Ok(())
}
