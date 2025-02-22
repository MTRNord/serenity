use crate::internal::prelude::*;
use serde_json::Value;
use std::default::Default;
use crate::utils::VecMap;

/// A builder to create a [`RichInvite`] for use via [`GuildChannel::create_invite`].
///
/// This is a structured and cleaner way of creating an invite, as all
/// parameters are optional.
///
/// # Examples
///
/// Create an invite with a max age of 3600 seconds and 10 max uses:
///
/// ```rust,no_run
/// # #[cfg(feature = "client")] {
/// # use serenity::prelude::*;
/// # use serenity::model::prelude::*;
/// # use serenity::model::channel::Channel;
///
/// struct Handler;
///
/// impl EventHandler for Handler {
///     fn message(&self, _: Context, msg: Message) {
///         if msg.content == "!createinvite" {
///             let channel = match msg.channel_id.to_channel() {
///                 Ok(channel) => channel,
///                 Err(why) => {
///                     let _ = msg.channel_id.say(&format!("Error creating invite: {:?}", why));
///
///                     return;
///                 },
///             };
///
///             if let Channel::Guild(channel) = channel {
///                 let channel = channel.read();
///
///             let invite = match channel.create_invite(|i| i.max_age(3600).max_uses(10)) {
///                 Ok(invite) => invite,
///                 Err(why) => {
///                     println!("Err creating invite: {:?}", why);
///
///                     if let Err(why) = msg.channel_id.say("Error creating invite") {
///                         println!("Err sending err msg: {:?}", why);
///                     }
///
///                     return;
///
///                     },
///                 };
///
///
///                 let content = format!("Here's your invite: {}", invite.url());
///                 let _ = msg.channel_id.say(&content);
///             }
///         }
///     }
/// }
///
/// let mut client = Client::new("token", Handler).unwrap();
///
/// client.start().unwrap();
/// # }
/// ```
///
/// [`GuildChannel::create_invite`]: ../model/channel/struct.GuildChannel.html#method.create_invite
/// [`RichInvite`]: ../model/invite/struct.RichInvite.html
#[derive(Clone, Debug)]
pub struct CreateInvite(pub VecMap<&'static str, Value>);

impl CreateInvite {
    /// The duration that the invite will be valid for.
    ///
    /// Set to `0` for an invite which does not expire after an amount of time.
    ///
    /// Defaults to `86400`, or 24 hours.
    ///
    /// # Examples
    ///
    /// Create an invite with a max age of `3600` seconds, or 1 hour:
    ///
    /// ```rust,no_run
    /// # use serenity::model::id::ChannelId;
    /// # use serenity::model::channel::Channel;
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// #     let channel = ChannelId(81384788765712384).to_channel().unwrap();
    /// #
    /// #     if let Channel::Guild(guild_channel) = channel {
    /// #         let guild_channel = guild_channel.read();
    /// let invite = guild_channel.create_invite(|i| i.max_age(3600))?;
    /// #     }
    /// #
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn max_age(mut self, max_age: u64) -> Self {
        self.0.insert("max_age", Value::Number(Number::from(max_age)));

        self
    }

    /// The number of uses that the invite will be valid for.
    ///
    /// Set to `0` for an invite which does not expire after a number of uses.
    ///
    /// Defaults to `0`.
    ///
    /// # Examples
    ///
    /// Create an invite with a max use limit of `5`:
    ///
    /// ```rust,no_run
    /// # use serenity::model::id::ChannelId;
    /// # use serenity::model::channel::Channel;
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// #     let channel = ChannelId(81384788765712384).to_channel().unwrap();
    /// #
    /// #       if let Channel::Guild(channel) = channel {
    /// #           let channel = channel.read();
    /// let invite = channel.create_invite(|i| i.max_uses(5))?;
    /// #       }
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn max_uses(mut self, max_uses: u64) -> Self {
        self.0.insert("max_uses", Value::Number(Number::from(max_uses)));

        self
    }

    /// Whether an invite grants a temporary membership.
    ///
    /// Defaults to `false`.
    ///
    /// # Examples
    ///
    /// Create an invite which is temporary:
    ///
    /// ```rust,no_run
    /// # use serenity::model::id::ChannelId;
    /// # use serenity::model::channel::Channel;
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// #     let channel = ChannelId(81384788765712384).to_channel().unwrap();
    /// #
    /// #     if let Channel::Guild(channel) = channel {
    /// #         let channel = channel.read();
    /// #
    /// let invite = channel.create_invite(|i| i.temporary(true))?;
    /// #     }
    /// #
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn temporary(mut self, temporary: bool) -> Self {
        self.0.insert("temporary", Value::Bool(temporary));

        self
    }

    /// Whether or not to try to reuse a similar invite.
    ///
    /// Defaults to `false`.
    ///
    /// # Examples
    ///
    /// Create an invite which is unique:
    ///
    /// ```rust,no_run
    /// # use serenity::model::id::ChannelId;
    /// # use std::error::Error;
    /// # use serenity::model::channel::Channel;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// #     let channel = ChannelId(81384788765712384).to_channel().unwrap();
    /// #
    /// # if let Channel::Guild(channel) = channel {
    /// #    let channel = channel.read();
    /// let invite = channel.create_invite(|i| i.unique(true))?;
    /// # }
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn unique(mut self, unique: bool) -> Self {
        self.0.insert("unique", Value::Bool(unique));

        self
    }
}

impl Default for CreateInvite {
    /// Creates a builder with default values, setting `validate` to `null`.
    ///
    /// # Examples
    ///
    /// Create a default `CreateInvite` builder:
    ///
    /// ```rust
    /// use serenity::builder::CreateInvite;
    ///
    /// let invite_builder = CreateInvite::default();
    /// ```
    fn default() -> CreateInvite {
        let mut map = VecMap::new();
        map.insert("validate", Value::Null);

        CreateInvite(map)
    }
}
