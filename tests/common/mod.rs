//! Shared harness for the wiremock suite.

use spoo_me::Client;
use wiremock::MockServer;

pub async fn server_and_client() -> (MockServer, Client) {
    let server = MockServer::start().await;
    let client = Client::builder()
        .api_key("spoo_test_key")
        .base_url(server.uri())
        .max_retries(2)
        .build()
        .expect("test client config is valid");
    (server, client)
}

// Not every integration-test binary uses every helper.
#[allow(dead_code)]
pub async fn anonymous_client(server: &MockServer) -> Client {
    Client::builder()
        .base_url(server.uri())
        .build()
        .expect("test client config is valid")
}
