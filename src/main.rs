use cfg_if::cfg_if;

use tonic::transport::Server;
#[cfg(feature = "tls")]
use tonic::transport::{Identity, ServerTlsConfig};

use tonic_hello_tls::{
    db,
    greeter::{GreeterServer, MyGreeter, FILE_DESCRIPTOR_SET},
};

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

    let greeter = MyGreeter::new(db);

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
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
