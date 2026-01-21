use std::io;
use tokio::net::TcpStream;

pub async fn create_tcp_connection(host: &str, port: u16) -> io::Result<TcpStream> {
    let address = format!("{}:{}", host, port);
    TcpStream::connect(address).await
}
