#[cfg(feature = "model")]
use hyper::Client as HyperClient;
#[cfg(feature = "model")]
use crate::internal::prelude::*;
#[cfg(feature = "model")]
use std::io::Read;

/// A file uploaded with a message. Not to be confused with [`Embed`]s.
///
/// [`Embed`]: struct.Embed.html
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Attachment {
    /// The unique ID given to this attachment.
    pub id: String,
    /// The filename of the file that was uploaded. This is equivalent to what
    /// the uploader had their file named.
    pub filename: String,
    /// If the attachment is an image, then the height of the image is provided.
    pub height: Option<u64>,
    /// The proxy URL.
    pub proxy_url: String,
    /// The size of the file in bytes.
    pub size: u64,
    /// The URL of the uploaded attachment.
    pub url: String,
    /// If the attachment is an image, then the width of the image is provided.
    pub width: Option<u64>,
}

#[cfg(feature = "model")]
impl Attachment {
    /// If this attachment is an image, then a tuple of the width and height
    /// in pixels is returned.
    pub fn dimensions(&self) -> Option<(u64, u64)> {
        self.width
            .and_then(|width| self.height.map(|height| (width, height)))
    }

    /// Downloads the attachment, returning back a vector of bytes.
    ///
    /// # Examples
    ///
    /// Download all of the attachments associated with a [`Message`]:
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "client")] {
    /// use serenity::model::prelude::*;
    /// use serenity::prelude::*;
    /// use std::env;
    /// use std::fs::File;
    /// use std::io::Write;
    /// use std::path::Path;
    ///
    /// struct Handler;
    ///
    /// impl EventHandler for Handler {
    ///     fn message(&self, _: Context, message: Message) {
    ///         for attachment in message.attachments {
    ///             let content = match attachment.download() {
    ///                 Ok(content) => content,
    ///                 Err(why) => {
    ///                     println!("Error downloading attachment: {:?}", why);
    ///                     let _ = message.channel_id.say("Error downloading attachment");
    ///
    ///                     return;
    ///                 },
    ///             };
    ///
    ///             let mut file = match File::create(&attachment.filename) {
    ///                 Ok(file) => file,
    ///                 Err(why) => {
    ///                     println!("Error creating file: {:?}", why);
    ///                     let _ = message.channel_id.say("Error creating file");
    ///
    ///                     return;
    ///                 },
    ///             };
    ///
    ///             if let Err(why) = file.write(&content) {
    ///                 println!("Error writing to file: {:?}", why);
    ///
    ///                 return;
    ///             }
    ///
    ///             let _ = message.channel_id.say(&format!("Saved {:?}", attachment.filename));
    ///         }
    ///     }
    ///
    ///     fn ready(&self, _: Context, ready: Ready) {
    ///         println!("{} is connected!", ready.user.name);
    ///     }
    /// }
    /// let token = env::var("DISCORD_TOKEN").expect("token in environment");
    /// let mut client = Client::new(&token, Handler).unwrap();
    ///
    /// client.start().unwrap();
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an [`Error::Io`] when there is a problem reading the contents
    /// of the HTTP response.
    ///
    /// Returns an [`Error::Hyper`] when there is a problem retrieving the
    /// attachment.
    ///
    /// [`Error::Hyper`]: ../../enum.Error.html#variant.Hyper
    /// [`Error::Io`]: ../../enum.Error.html#variant.Io
    /// [`Message`]: struct.Message.html
    pub fn download(&self) -> Result<Vec<u8>> {
        let hyper = request_client!();
        let mut response = hyper.get(&self.url).send()?;

        let mut bytes = vec![];
        response.read_to_end(&mut bytes)?;

        Ok(bytes)
    }
}
