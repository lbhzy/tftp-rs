use tftp::TftpClient;

#[tokio::main]
async fn main() {
    let config = tftp::SessionConfig {
        directory: std::path::PathBuf::from("."),
        timeout: 1000,
        retry: 3,
        gbn: true,
    };
    let client = TftpClient::new(config, 512, 1);
    client
        .get_file("127.0.0.1:6666".parse().unwrap(), "test.txt".to_string())
        .await
        .unwrap();
}
