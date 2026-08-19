//! The Sign in with Spoo flow: build the consent URL, exchange the code,
//! attach a self-refreshing session.
//!
//! Run with: cargo run --example sign_in_with_spoo --features oauth

use std::sync::Arc;

use spoo_me::oauth::{Session, generate_pkce_pair, generate_state};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let anon = spoo_me::Client::anonymous();

    let pkce = generate_pkce_pair();
    let url = anon
        .oauth()
        .authorization_url("your_app_id", generate_state(), &pkce.challenge)
        .redirect_uri("http://127.0.0.1:8000/callback")
        .build()?;
    println!("open this in a browser:\n  {url}");
    println!("then paste the one-time code from your callback:");

    let mut code = String::new();
    std::io::stdin().read_line(&mut code)?;
    let tokens = anon
        .oauth()
        .exchange_code(code.trim(), pkce.verifier)
        .await?;
    println!("signed in as {:?}", tokens.user.email);

    // The session refreshes itself; persist rotated pairs in on_refresh.
    let session = Arc::new(Session::new(tokens.tokens()).on_refresh(|pair| {
        println!(
            "tokens rotated; persist refresh token {}...",
            &pair.refresh_token[..8.min(pair.refresh_token.len())]
        );
    }));
    let client = spoo_me::Client::builder().session(session).build()?;

    let me = client.auth().me().await?;
    println!("plan: {}", me.plan);
    Ok(())
}
