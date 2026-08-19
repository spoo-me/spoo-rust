//! The Sign in with Spoo flow: build the consent URL, exchange the code,
//! attach a self-refreshing session.
//!
//! Run with: cargo run --example sign_in_with_spoo --features oauth

use std::sync::Arc;

use spoo_me::oauth::{Session, generate_pkce_pair, generate_state};

fn prompt(label: &str) -> Result<String, std::io::Error> {
    println!("{label}");
    let mut value = String::new();
    std::io::stdin().read_line(&mut value)?;
    Ok(value.trim().to_owned())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let anon = spoo_me::Client::anonymous();

    let pkce = generate_pkce_pair();
    let state = generate_state();
    let url = anon
        .oauth()
        .authorization_url("your_app_id", &state, &pkce.challenge)
        .redirect_uri("http://127.0.0.1:8000/callback")
        .build()?;
    println!("open this in a browser:\n  {url}");

    // The callback URL carries `code` and `state`. Verify the echoed state
    // against the one sent before exchanging the code: a mismatch means the
    // code was not produced by this flow, so it must be rejected (CSRF).
    let returned_state = prompt("paste the state parameter from your callback URL:")?;
    if returned_state != state {
        return Err("state mismatch: aborting the sign-in".into());
    }
    let code = prompt("paste the one-time code:")?;
    let tokens = anon.oauth().exchange_code(code, pkce.verifier).await?;
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
