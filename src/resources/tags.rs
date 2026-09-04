//! Link tags: the account's tag catalogue.

use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Error;
use crate::http::{RequestSpec, encode_segment};

/// Tag management, from [`crate::Client::tags`].
pub struct Tags {
    pub(crate) client: Client,
}

/// A tag's colour. Fixed palette; the dashboard maps each key to a muted
/// dot colour.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TagColor {
    /// `gray`.
    Gray,
    /// `red`.
    Red,
    /// `orange`.
    Orange,
    /// `amber`.
    Amber,
    /// `green`.
    Green,
    /// `teal`.
    Teal,
    /// `blue`.
    Blue,
    /// `violet`.
    Violet,
    /// `pink`.
    Pink,
    /// A palette key this SDK version does not know yet, kept as the
    /// server sent it.
    #[serde(untagged)]
    Other(String),
}

/// A tag's icon: a key from the curated lucide set.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TagIcon {
    /// `banknote`.
    Banknote,
    /// `bar-chart-3`.
    #[serde(rename = "bar-chart-3")]
    BarChart3,
    /// `beaker`.
    Beaker,
    /// `bell`.
    Bell,
    /// `bird`.
    Bird,
    /// `book`.
    Book,
    /// `bookmark`.
    Bookmark,
    /// `box`.
    Box,
    /// `briefcase`.
    Briefcase,
    /// `bug`.
    Bug,
    /// `building`.
    Building,
    /// `calendar`.
    Calendar,
    /// `camera`.
    Camera,
    /// `car`.
    Car,
    /// `cat`.
    Cat,
    /// `clock`.
    Clock,
    /// `cloud`.
    Cloud,
    /// `code`.
    Code,
    /// `coffee`.
    Coffee,
    /// `compass`.
    Compass,
    /// `credit-card`.
    CreditCard,
    /// `crown`.
    Crown,
    /// `dog`.
    Dog,
    /// `file-text`.
    FileText,
    /// `fish`.
    Fish,
    /// `flag`.
    Flag,
    /// `flame`.
    Flame,
    /// `flask-conical`.
    FlaskConical,
    /// `folder`.
    Folder,
    /// `gamepad-2`.
    #[serde(rename = "gamepad-2")]
    Gamepad2,
    /// `gem`.
    Gem,
    /// `ghost`.
    Ghost,
    /// `gift`.
    Gift,
    /// `globe`.
    Globe,
    /// `graduation-cap`.
    GraduationCap,
    /// `handshake`.
    Handshake,
    /// `hash`.
    Hash,
    /// `heart`.
    Heart,
    /// `home`.
    Home,
    /// `hourglass`.
    Hourglass,
    /// `image`.
    Image,
    /// `key`.
    Key,
    /// `layers`.
    Layers,
    /// `leaf`.
    Leaf,
    /// `lightbulb`.
    Lightbulb,
    /// `link`.
    Link,
    /// `lock`.
    Lock,
    /// `mail`.
    Mail,
    /// `map-pin`.
    MapPin,
    /// `megaphone`.
    Megaphone,
    /// `message-square`.
    MessageSquare,
    /// `mic`.
    Mic,
    /// `moon`.
    Moon,
    /// `music`.
    Music,
    /// `newspaper`.
    Newspaper,
    /// `package`.
    Package,
    /// `pen-line`.
    PenLine,
    /// `phone`.
    Phone,
    /// `pie-chart`.
    PieChart,
    /// `pizza`.
    Pizza,
    /// `plane`.
    Plane,
    /// `puzzle`.
    Puzzle,
    /// `receipt`.
    Receipt,
    /// `rocket`.
    Rocket,
    /// `send`.
    Send,
    /// `settings`.
    Settings,
    /// `share-2`.
    #[serde(rename = "share-2")]
    Share2,
    /// `shield`.
    Shield,
    /// `shopping-cart`.
    ShoppingCart,
    /// `smile`.
    Smile,
    /// `sparkles`.
    Sparkles,
    /// `star`.
    Star,
    /// `store`.
    Store,
    /// `sun`.
    Sun,
    /// `tag` (the default).
    Tag,
    /// `target`.
    Target,
    /// `terminal`.
    Terminal,
    /// `timer`.
    Timer,
    /// `trending-up`.
    TrendingUp,
    /// `trophy`.
    Trophy,
    /// `umbrella`.
    Umbrella,
    /// `user`.
    User,
    /// `users`.
    Users,
    /// `video`.
    Video,
    /// `wallet`.
    Wallet,
    /// `wrench`.
    Wrench,
    /// `zap`.
    Zap,
    /// An icon key this SDK version does not know yet, kept as the server
    /// sent it.
    #[serde(untagged)]
    Other(String),
}

/// A tag as it appears on a link: enough to render, no counts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct TagRef {
    /// The tag's identifier.
    pub id: String,
    /// Lowercase name, unique per account.
    pub name: String,
    /// Palette colour.
    pub color: TagColor,
    /// Icon key.
    pub icon: TagIcon,
}

/// A tag on its own endpoints, with its link count.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Tag {
    /// The tag's identifier.
    pub id: String,
    /// Lowercase name, unique per account.
    pub name: String,
    /// Palette colour.
    pub color: TagColor,
    /// Icon key.
    pub icon: TagIcon,
    /// Links carrying the tag.
    pub link_count: u64,
    /// When the tag was created.
    pub created_at: DateTime<Utc>,
    /// When the tag was last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// Confirmation of a tag deletion.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct TagDeleted {
    /// Links the tag was removed from.
    pub links_updated: u64,
}

#[derive(Deserialize)]
struct ListWire {
    items: Vec<Tag>,
}

impl Tags {
    /// Every tag in your account with its link count, oldest first.
    pub async fn list(&self) -> Result<Vec<Tag>, Error> {
        let wire: ListWire = self
            .client
            .transport
            .execute(RequestSpec::new(Method::GET, "/api/v1/tags"))
            .await?;
        Ok(wire.items)
    }

    /// Create a tag. Returns a builder: chain a colour or icon, then
    /// `.send()`. Names are lowercased and trimmed server-side; a name you
    /// already have answers a conflict error.
    ///
    /// ```no_run
    /// # async fn run(client: spoo_me::Client) -> Result<(), spoo_me::Error> {
    /// use spoo_me::{TagColor, TagIcon};
    ///
    /// let tag = client
    ///     .tags()
    ///     .create("launch")
    ///     .color(TagColor::Violet)
    ///     .icon(TagIcon::Rocket)
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create(&self, name: impl Into<String>) -> CreateTagBuilder {
        CreateTagBuilder {
            client: self.client.clone(),
            body: CreateTagBody {
                name: name.into(),
                color: None,
                icon: None,
            },
        }
    }

    /// Change a tag's name, colour or icon. Returns a builder: chain
    /// changes, then `.send()`. Links point at the tag by id, so a rename
    /// shows up everywhere at once.
    pub fn update(&self, id: impl Into<String>) -> UpdateTagBuilder {
        UpdateTagBuilder {
            client: self.client.clone(),
            id: id.into(),
            body: UpdateTagBody::default(),
        }
    }

    /// Delete a tag and remove it from every link that carried it. The
    /// links themselves are otherwise untouched.
    pub async fn delete(&self, id: &str) -> Result<TagDeleted, Error> {
        self.client
            .transport
            .execute(RequestSpec::new(
                Method::DELETE,
                format!("/api/v1/tags/{}", encode_segment(id)),
            ))
            .await
    }
}

#[derive(Serialize)]
struct CreateTagBody {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<TagColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<TagIcon>,
}

/// Builder for [`Tags::create`]. Chain options, finish with
/// [`CreateTagBuilder::send`].
#[must_use = "builders do nothing until .send() is awaited"]
pub struct CreateTagBuilder {
    client: Client,
    body: CreateTagBody,
}

impl CreateTagBuilder {
    /// Palette colour. Defaults to the least-used colour in your account.
    pub fn color(mut self, color: TagColor) -> Self {
        self.body.color = Some(color);
        self
    }

    /// Icon key. Defaults to [`TagIcon::Tag`].
    pub fn icon(mut self, icon: TagIcon) -> Self {
        self.body.icon = Some(icon);
        self
    }

    /// Create the tag.
    pub async fn send(self) -> Result<Tag, Error> {
        let spec = RequestSpec::new(Method::POST, "/api/v1/tags").json(&self.body)?;
        self.client.transport.execute(spec).await
    }
}

#[derive(Default, Serialize)]
struct UpdateTagBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<TagColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<TagIcon>,
}

/// Builder for [`Tags::update`]. Untouched fields keep their stored values.
#[must_use = "builders do nothing until .send() is awaited"]
pub struct UpdateTagBuilder {
    client: Client,
    id: String,
    body: UpdateTagBody,
}

impl UpdateTagBuilder {
    /// Rename the tag. Must not collide with another of your tags.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.body.name = Some(name.into());
        self
    }

    /// Change the palette colour.
    pub fn color(mut self, color: TagColor) -> Self {
        self.body.color = Some(color);
        self
    }

    /// Change the icon.
    pub fn icon(mut self, icon: TagIcon) -> Self {
        self.body.icon = Some(icon);
        self
    }

    /// Apply the update.
    pub async fn send(self) -> Result<Tag, Error> {
        let spec = RequestSpec::new(
            Method::PATCH,
            format!("/api/v1/tags/{}", encode_segment(&self.id)),
        )
        .json(&self.body)?;
        self.client.transport.execute(spec).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON_KEYS: &str = "banknote, bar-chart-3, beaker, bell, bird, book, bookmark, box, \
        briefcase, bug, building, calendar, camera, car, cat, clock, cloud, code, coffee, \
        compass, credit-card, crown, dog, file-text, fish, flag, flame, flask-conical, folder, \
        gamepad-2, gem, ghost, gift, globe, graduation-cap, handshake, hash, heart, home, \
        hourglass, image, key, layers, leaf, lightbulb, link, lock, mail, map-pin, megaphone, \
        message-square, mic, moon, music, newspaper, package, pen-line, phone, pie-chart, pizza, \
        plane, puzzle, receipt, rocket, send, settings, share-2, shield, shopping-cart, smile, \
        sparkles, star, store, sun, tag, target, terminal, timer, trending-up, trophy, umbrella, \
        user, users, video, wallet, wrench, zap";

    #[test]
    fn every_icon_key_round_trips() {
        let keys: Vec<&str> = ICON_KEYS.split(", ").collect();
        assert_eq!(keys.len(), 87);
        for key in keys {
            let icon: TagIcon = serde_json::from_value(serde_json::Value::String(key.into()))
                .unwrap_or_else(|e| panic!("{key}: {e}"));
            assert!(
                !matches!(icon, TagIcon::Other(_)),
                "{key} fell through to Other"
            );
            assert_eq!(serde_json::to_value(icon).unwrap(), key, "{key} re-encodes");
        }
    }

    #[test]
    fn every_color_key_round_trips() {
        for key in [
            "gray", "red", "orange", "amber", "green", "teal", "blue", "violet", "pink",
        ] {
            let color: TagColor =
                serde_json::from_value(serde_json::Value::String(key.into())).unwrap();
            assert!(
                !matches!(color, TagColor::Other(_)),
                "{key} fell through to Other"
            );
            assert_eq!(serde_json::to_value(color).unwrap(), key);
        }
    }

    #[test]
    fn unknown_keys_survive_as_the_server_sent_them() {
        let tag: TagRef =
            serde_json::from_str(r#"{"id":"t1","name":"x","color":"chartreuse","icon":"unicorn"}"#)
                .unwrap();
        assert_eq!(tag.color, TagColor::Other("chartreuse".into()));
        assert_eq!(tag.icon, TagIcon::Other("unicorn".into()));
        assert_eq!(serde_json::to_value(&tag.icon).unwrap(), "unicorn");
    }
}
