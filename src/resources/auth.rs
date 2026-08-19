//! Identity: who this client is signed in as.

use reqwest::Method;
use serde::Deserialize;

use crate::client::Client;
use crate::error::Error;
use crate::http::RequestSpec;

/// Identity reads, from [`crate::Client::auth`].
pub struct AuthResource {
    pub(crate) client: Client,
}

/// A linked sign-in provider.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct AuthProvider {
    /// Provider name, e.g. `google` or `github`.
    #[serde(default)]
    pub provider: Option<String>,
    /// The provider-side account email, when shared.
    #[serde(default)]
    pub email: Option<String>,
}

/// Profile picture info.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ProfilePicture {
    /// Picture URL.
    #[serde(default)]
    pub url: Option<String>,
    /// Where it came from: an OAuth provider name or `upload`.
    #[serde(default)]
    pub source: Option<String>,
}

/// The signed-in user's profile.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct User {
    /// User id.
    pub id: String,
    /// Email address.
    #[serde(default)]
    pub email: Option<String>,
    /// Whether the email address is verified.
    pub email_verified: bool,
    /// Display name.
    #[serde(default)]
    pub user_name: Option<String>,
    /// Subscription plan.
    pub plan: String,
    /// Whether the user has set a password.
    pub password_set: bool,
    /// Linked sign-in providers.
    #[serde(default)]
    pub auth_providers: Vec<AuthProvider>,
    /// Profile picture, when set.
    #[serde(default)]
    pub pfp: Option<ProfilePicture>,
}

#[derive(Deserialize)]
struct MeWire {
    user: User,
}

impl AuthResource {
    /// Who this client is signed in as. Works with both API keys and
    /// Sign in with Spoo sessions.
    pub async fn me(&self) -> Result<User, Error> {
        let wire: MeWire = self
            .client
            .transport
            .execute(RequestSpec::new(Method::GET, "/auth/me"))
            .await?;
        Ok(wire.user)
    }
}
