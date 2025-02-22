use crate::constants::{self, close_codes};
use crate::internal::prelude::*;
use crate::model::{
    event::{Event, GatewayEvent},
    gateway::Game,
    id::GuildId,
    user::OnlineStatus
};
use parking_lot::Mutex;
use std::{
    sync::Arc,
    time::{Duration as StdDuration, Instant}
};
use super::{
    ConnectionStage,
    CurrentPresence,
    ShardAction,
    GatewayError,
    ReconnectType,
    WsClient,
    WebSocketGatewayClientExt,
};
use websocket::{
    client::Url,
    stream::sync::AsTcpStream,
    sync::client::ClientBuilder,
    WebSocketError
};

/// A Shard is a higher-level handler for a websocket connection to Discord's
/// gateway. The shard allows for sending and receiving messages over the
/// websocket, such as setting the active game, reconnecting, syncing guilds,
/// and more.
///
/// Refer to the [module-level documentation][module docs] for information on
/// effectively using multiple shards, if you need to.
///
/// Note that there are additional methods available if you are manually
/// managing a shard yourself, although they are hidden from the documentation
/// since there are few use cases for doing such.
///
/// # Stand-alone shards
///
/// You may instantiate a shard yourself - decoupled from the [`Client`] - if
/// you need to. For most use cases, you will not need to do this, and you can
/// leave the client to do it.
///
/// This can be done by passing in the required parameters to [`new`]. You can
/// then manually handle the shard yourself and receive events via
/// [`receive`].
///
/// **Note**: You _really_ do not need to do this. Just call one of the
/// appropriate methods on the [`Client`].
///
/// # Examples
///
/// See the documentation for [`new`] on how to use this.
///
/// [`Client`]: ../client/struct.Client.html
/// [`new`]: #method.new
/// [`receive`]: #method.receive
/// [docs]: https://discordapp.com/developers/docs/topics/gateway#sharding
/// [module docs]: index.html#sharding
pub struct Shard {
    pub client: WsClient,
    current_presence: CurrentPresence,
    /// A tuple of:
    ///
    /// - the last instant that a heartbeat was sent
    /// - the last instant that an acknowledgement was received
    ///
    /// This can be used to calculate [`latency`].
    ///
    /// [`latency`]: fn.latency.html
    heartbeat_instants: (Option<Instant>, Option<Instant>),
    heartbeat_interval: Option<u64>,
    /// This is used by the heartbeater to determine whether the last
    /// heartbeat was sent without an acknowledgement, and whether to reconnect.
    // This _must_ be set to `true` in `Shard::handle_event`'s
    // `Ok(GatewayEvent::HeartbeatAck)` arm.
    last_heartbeat_acknowledged: bool,
    seq: u64,
    session_id: Option<String>,
    shard_info: [u64; 2],
    /// Whether the shard has permanently shutdown.
    shutdown: bool,
    stage: ConnectionStage,
    /// Instant of when the shard was started.
    // This acts as a timeout to determine if the shard has - for some reason -
    // not started within a decent amount of time.
    pub started: Instant,
    pub token: Arc<Mutex<String>>,
    ws_url: Arc<Mutex<String>>,
}

impl Shard {
    /// Instantiates a new instance of a Shard, bypassing the client.
    ///
    /// **Note**: You should likely never need to do this yourself.
    ///
    /// # Examples
    ///
    /// Instantiating a new Shard manually for a bot with no shards, and
    /// then listening for events:
    ///
    /// ```rust,no_run
    /// extern crate parking_lot;
    /// extern crate serenity;
    /// #
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// #
    /// use parking_lot::Mutex;
    /// use serenity::gateway::Shard;
    /// use serenity::http;
    /// use std::env;
    /// use std::sync::Arc;
    ///
    /// let token = Arc::new(Mutex::new(env::var("DISCORD_BOT_TOKEN")?));
    /// // retrieve the gateway response, which contains the URL to connect to
    /// let gateway = Arc::new(Mutex::new(http::get_gateway()?.url));
    /// let shard = Shard::new(gateway, token, [0, 1])?;
    ///
    /// // at this point, you can create a `loop`, and receive events and match
    /// // their variants
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn new(
        ws_url: Arc<Mutex<String>>,
        token: Arc<Mutex<String>>,
        shard_info: [u64; 2],
    ) -> Result<Shard> {
        let mut client = connect(&*ws_url.lock())?;

        let _ = set_client_timeout(&mut client);

        let current_presence = (None, OnlineStatus::Online);
        let heartbeat_instants = (None, None);
        let heartbeat_interval = None;
        let last_heartbeat_acknowledged = true;
        let seq = 0;
        let stage = ConnectionStage::Handshake;
        let session_id = None;

        Ok(Shard {
            shutdown: false,
            client,
            current_presence,
            heartbeat_instants,
            heartbeat_interval,
            last_heartbeat_acknowledged,
            seq,
            stage,
            started: Instant::now(),
            token,
            session_id,
            shard_info,
            ws_url,
        })
    }

    /// Retrieves the current presence of the shard.
    #[inline]
    pub fn current_presence(&self) -> &CurrentPresence {
        &self.current_presence
    }
    /// Whether the shard has permanently shutdown.
    ///
    /// This should normally happen due to manual calling of [`shutdown`] or
    /// [`shutdown_clean`].
    ///
    /// [`shutdown`]: #method.shutdown
    /// [`shutdown_clean`]: #method.shutdown_clean
    #[inline]
    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    /// Retrieves the heartbeat instants of the shard.
    ///
    /// This is the time of when a heartbeat was sent and when an
    /// acknowledgement was last received.
    #[inline]
    pub fn heartbeat_instants(&self) -> &(Option<Instant>, Option<Instant>) {
        &self.heartbeat_instants
    }

    /// Retrieves the value of when the last heartbeat was sent.
    #[inline]
    pub fn last_heartbeat_sent(&self) -> Option<&Instant> {
        self.heartbeat_instants.0.as_ref()
    }

    /// Retrieves the value of when the last heartbeat ack was received.
    #[inline]
    pub fn last_heartbeat_ack(&self) -> Option<&Instant> {
        self.heartbeat_instants.1.as_ref()
    }

    /// Sends a heartbeat to the gateway with the current sequence.
    ///
    /// This sets the last heartbeat time to now, and
    /// `last_heartbeat_acknowledged` to `false`.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::HeartbeatFailed`] if there was an error sending
    /// a heartbeat.
    ///
    /// [`GatewayError::HeartbeatFailed`]: enum.GatewayError.html#variant.HeartbeatFailed
    pub fn heartbeat(&mut self) -> Result<()> {
        match self.client.send_heartbeat(&self.shard_info, Some(self.seq)) {
            Ok(()) => {
                self.heartbeat_instants.0 = Some(Instant::now());
                self.last_heartbeat_acknowledged = false;

                Ok(())
            },
            Err(why) => {
                match why {
                    Error::WebSocket(WebSocketError::IoError(err)) => if err.raw_os_error() != Some(32) {
                        debug!("[Shard {:?}] Err heartbeating: {:?}",
                               self.shard_info,
                               err);
                    },
                    other => {
                        warn!("[Shard {:?}] Other err w/ keepalive: {:?}",
                              self.shard_info,
                              other);
                    },
                }

                Err(Error::Gateway(GatewayError::HeartbeatFailed))
            }
        }
    }

    #[inline]
    pub fn heartbeat_interval(&self) -> Option<&u64> {
        self.heartbeat_interval.as_ref()
    }

    #[inline]
    pub fn last_heartbeat_acknowledged(&self) -> bool {
        self.last_heartbeat_acknowledged
    }

    #[inline]
    pub fn seq(&self) -> u64 {
        self.seq
    }

    #[inline]
    pub fn session_id(&self) -> Option<&String> {
        self.session_id.as_ref()
    }

    /// ```rust,no_run
    /// # #[cfg(feature = "model")]
    /// # fn main() {
    /// # use serenity::client::gateway::Shard;
    /// # use std::sync::Arc;
    /// # use serenity::prelude::Mutex;
    /// #
    /// # let mutex = Arc::new(Mutex::new("".to_string()));
    /// #
    /// # let mut shard = Shard::new(mutex.clone(), mutex, [0, 1]).unwrap();
    /// #
    /// use serenity::model::gateway::Game;
    ///
    /// shard.set_game(Some(Game::playing("Heroes of the Storm")));
    /// # }
    /// #
    /// # #[cfg(not(feature = "model"))]
    /// # fn main() { }
    /// ```
    #[inline]
    pub fn set_game(&mut self, game: Option<Game>) {
        self.current_presence.0 = game;
    }

    #[inline]
    pub fn set_presence(&mut self, status: OnlineStatus, game: Option<Game>) {
        self.set_game(game);
        self.set_status(status);
    }

    #[inline]
    pub fn set_status(&mut self, mut status: OnlineStatus) {
        if status == OnlineStatus::Offline {
            status = OnlineStatus::Invisible;
        }

        self.current_presence.1 = status;
    }

    /// Retrieves a copy of the current shard information.
    ///
    /// The first element is the _current_ shard - 0-indexed - while the second
    /// element is the _total number_ of shards -- 1-indexed.
    ///
    /// For example, if using 3 shards in total, and if this is shard 1, then it
    /// can be read as "the second of three shards".
    ///
    /// # Examples
    ///
    /// Retrieving the shard info for the second shard, out of two shards total:
    ///
    /// ```rust,no_run
    /// # extern crate serenity;
    /// # #[cfg(feature = "model")]
    /// # fn main() {
    /// # use serenity::client::gateway::Shard;
    /// # use serenity::prelude::Mutex;
    /// # use std::sync::Arc;
    /// #
    /// # let mutex = Arc::new(Mutex::new("".to_string()));
    /// #
    /// # let shard = Shard::new(mutex.clone(), mutex, [1, 2]).unwrap();
    /// #
    /// assert_eq!(shard.shard_info(), [1, 2]);
    /// # }
    /// #
    /// # #[cfg(not(feature = "model"))]
    /// # fn main() {}
    /// ```
    pub fn shard_info(&self) -> [u64; 2] { self.shard_info }

    /// Returns the current connection stage of the shard.
    pub fn stage(&self) -> ConnectionStage {
        self.stage
    }

    /// Handles an event from the gateway over the receiver, requiring the
    /// receiver to be passed if a reconnect needs to occur.
    ///
    /// The best case scenario is that one of two values is returned:
    ///
    /// - `Ok(None)`: a heartbeat, late hello, or session invalidation was
    ///   received;
    /// - `Ok(Some((event, None)))`: an op0 dispatch was received, and the
    ///   shard's voice state will be updated, _if_ the `voice` feature is
    ///   enabled.
    ///
    /// # Errors
    ///
    /// Returns a `GatewayError::InvalidAuthentication` if invalid
    /// authentication was sent in the IDENTIFY.
    ///
    /// Returns a `GatewayError::InvalidShardData` if invalid shard data was
    /// sent in the IDENTIFY.
    ///
    /// Returns a `GatewayError::NoAuthentication` if no authentication was sent
    /// in the IDENTIFY.
    ///
    /// Returns a `GatewayError::OverloadedShard` if the shard would have too
    /// many guilds assigned to it.
    #[allow(cyclomatic_complexity)]
    pub(crate) fn handle_event(&mut self, event: &Result<GatewayEvent>)
        -> Result<Option<ShardAction>> {
        match *event {
            Ok(GatewayEvent::Dispatch(seq, ref event)) => {
                if seq > self.seq + 1 {
                    warn!("[Shard {:?}] Sequence off; them: {}, us: {}", self.shard_info, seq, self.seq);
                }

                match *event {
                    Event::Ready(ref ready) => {
                        debug!("[Shard {:?}] Received Ready", self.shard_info);

                        self.session_id = Some(ready.ready.session_id.clone());
                        self.stage = ConnectionStage::Connected;
                    },
                    Event::Resumed(_) => {
                        info!("[Shard {:?}] Resumed", self.shard_info);

                        self.stage = ConnectionStage::Connected;
                        self.last_heartbeat_acknowledged = true;
                        self.heartbeat_instants = (Some(Instant::now()), None);
                    },
                    _ => {},
                }

                self.seq = seq;

                Ok(None)
            },
            Ok(GatewayEvent::Heartbeat(s)) => {
                info!("[Shard {:?}] Received shard heartbeat", self.shard_info);

                // Received seq is off -- attempt to resume.
                if s > self.seq + 1 {
                    info!(
                        "[Shard {:?}] Received off sequence (them: {}; us: {}); resuming",
                        self.shard_info,
                        s,
                        self.seq
                    );

                    if self.stage == ConnectionStage::Handshake {
                        self.stage = ConnectionStage::Identifying;

                        return Ok(Some(ShardAction::Identify));
                    } else {
                        warn!(
                            "[Shard {:?}] Heartbeat during non-Handshake; auto-reconnecting",
                            self.shard_info
                        );

                        return Ok(Some(ShardAction::Reconnect(self.reconnection_type())));
                    }
                }

                Ok(Some(ShardAction::Heartbeat))
            },
            Ok(GatewayEvent::HeartbeatAck) => {
                self.heartbeat_instants.1 = Some(Instant::now());
                self.last_heartbeat_acknowledged = true;

                trace!("[Shard {:?}] Received heartbeat ack", self.shard_info);

                Ok(None)
            },
            Ok(GatewayEvent::Hello(interval)) => {
                debug!("[Shard {:?}] Received a Hello; interval: {}",
                       self.shard_info,
                       interval);

                if self.stage == ConnectionStage::Resuming {
                    return Ok(None);
                }

                if interval > 0 {
                    self.heartbeat_interval = Some(interval);
                }

                Ok(Some(if self.stage == ConnectionStage::Handshake {
                    ShardAction::Identify
                } else {
                    debug!("[Shard {:?}] Received late Hello; autoreconnecting",
                           self.shard_info);

                    ShardAction::Reconnect(self.reconnection_type())
                }))
            },
            Ok(GatewayEvent::InvalidateSession(resumable)) => {
                info!(
                    "[Shard {:?}] Received session invalidation",
                    self.shard_info,
                );

                Ok(Some(if resumable {
                    ShardAction::Reconnect(ReconnectType::Resume)
                } else {
                    ShardAction::Reconnect(ReconnectType::Reidentify)
                }))
            },
            Ok(GatewayEvent::Reconnect) => {
                Ok(Some(ShardAction::Reconnect(ReconnectType::Reidentify)))
            },
            Err(Error::Gateway(GatewayError::Closed(ref data))) => {
                let num = data.as_ref().map(|d| d.status_code);
                let clean = num == Some(1000);

                match num {
                    Some(close_codes::UNKNOWN_OPCODE) => {
                        warn!("[Shard {:?}] Sent invalid opcode",
                              self.shard_info);
                    },
                    Some(close_codes::DECODE_ERROR) => {
                        warn!("[Shard {:?}] Sent invalid message",
                              self.shard_info);
                    },
                    Some(close_codes::NOT_AUTHENTICATED) => {
                        warn!("[Shard {:?}] Sent no authentication",
                              self.shard_info);

                        return Err(Error::Gateway(GatewayError::NoAuthentication));
                    },
                    Some(close_codes::AUTHENTICATION_FAILED) => {
                        warn!("Sent invalid authentication");

                        return Err(Error::Gateway(GatewayError::InvalidAuthentication));
                    },
                    Some(close_codes::ALREADY_AUTHENTICATED) => {
                        warn!("[Shard {:?}] Already authenticated",
                              self.shard_info);
                    },
                    Some(close_codes::INVALID_SEQUENCE) => {
                        warn!("[Shard {:?}] Sent invalid seq: {}",
                              self.shard_info,
                              self.seq);

                        self.seq = 0;
                    },
                    Some(close_codes::RATE_LIMITED) => {
                        warn!("[Shard {:?}] Gateway ratelimited",
                              self.shard_info);
                    },
                    Some(close_codes::INVALID_SHARD) => {
                        warn!("[Shard {:?}] Sent invalid shard data",
                              self.shard_info);

                        return Err(Error::Gateway(GatewayError::InvalidShardData));
                    },
                    Some(close_codes::SHARDING_REQUIRED) => {
                        error!("[Shard {:?}] Shard has too many guilds",
                               self.shard_info);

                        return Err(Error::Gateway(GatewayError::OverloadedShard));
                    },
                    Some(4006) | Some(close_codes::SESSION_TIMEOUT) => {
                        info!("[Shard {:?}] Invalid session", self.shard_info);

                        self.session_id = None;
                    },
                    Some(other) if !clean => {
                        warn!(
                            "[Shard {:?}] Unknown unclean close {}: {:?}",
                            self.shard_info,
                            other,
                            data.as_ref().map(|d| &d.reason),
                        );
                    },
                    _ => {},
                }

                let resume = num.map(|x| {
                    x != close_codes::AUTHENTICATION_FAILED &&
                    self.session_id.is_some()
                }).unwrap_or(true);

                Ok(Some(if resume {
                    ShardAction::Reconnect(ReconnectType::Resume)
                } else {
                    ShardAction::Reconnect(ReconnectType::Reidentify)
                }))
            },
            Err(Error::WebSocket(ref why)) => {
                if let WebSocketError::NoDataAvailable = *why {
                    if self.heartbeat_instants.1.is_none() {
                        return Ok(None);
                    }
                }

                warn!("[Shard {:?}] Websocket error: {:?}",
                      self.shard_info,
                      why);
                info!("[Shard {:?}] Will attempt to auto-reconnect",
                      self.shard_info);

                Ok(Some(ShardAction::Reconnect(self.reconnection_type())))
            },
            _ => Ok(None),
        }
    }

    /// Checks whether a heartbeat needs to be sent, as well as whether a
    /// heartbeat acknowledgement was received.
    ///
    /// `true` is returned under one of the following conditions:
    ///
    /// - the heartbeat interval has not elapsed
    /// - a heartbeat was successfully sent
    /// - there is no known heartbeat interval yet
    ///
    /// `false` is returned under one of the following conditions:
    ///
    /// - a heartbeat acknowledgement was not received in time
    /// - an error occurred while heartbeating
    pub fn check_heartbeat(&mut self) -> bool {
        let wait = {
            let heartbeat_interval = match self.heartbeat_interval {
                Some(heartbeat_interval) => heartbeat_interval,
                None => {
                    return self.started.elapsed() < StdDuration::from_secs(15);
                },
            };

            StdDuration::from_secs(heartbeat_interval / 1000)
        };

        // If a duration of time less than the heartbeat_interval has passed,
        // then don't perform a keepalive or attempt to reconnect.
        if let Some(last_sent) = self.heartbeat_instants.0 {
            if last_sent.elapsed() <= wait {
                return true;
            }
        }

        // If the last heartbeat didn't receive an acknowledgement, then
        // auto-reconnect.
        if !self.last_heartbeat_acknowledged {
            debug!(
                "[Shard {:?}] Last heartbeat not acknowledged",
                self.shard_info,
            );

            return false;
        }

        // Otherwise, we're good to heartbeat.
        if let Err(why) = self.heartbeat() {
            warn!("[Shard {:?}] Err heartbeating: {:?}", self.shard_info, why);

            false
        } else {
            trace!("[Shard {:?}] Heartbeated", self.shard_info);

            true
        }
    }

    /// Calculates the heartbeat latency between the shard and the gateway.
    // Shamelessly stolen from brayzure's commit in eris:
    // <https://github.com/abalabahaha/eris/commit/0ce296ae9a542bcec0edf1c999ee2d9986bed5a6>
    pub fn latency(&self) -> Option<StdDuration> {
        if let (Some(sent), Some(received)) = self.heartbeat_instants {
            if received > sent {
                return Some(received - sent);
            }
        }

        None
    }

    /// Performs a deterministic reconnect.
    ///
    /// The type of reconnect is deterministic on whether a [`session_id`].
    ///
    /// If the `session_id` still exists, then a RESUME is sent. If not, then
    /// an IDENTIFY is sent.
    ///
    /// Note that, if the shard is already in a stage of
    /// [`ConnectionStage::Connecting`], then no action will be performed.
    ///
    /// [`ConnectionStage::Connecting`]: ../gateway/enum.ConnectionStage.html#variant.Connecting
    /// [`session_id`]: ../gateway/struct.Shard.html#method.session_id
    pub fn should_reconnect(&mut self) -> Option<ReconnectType> {
        if self.stage == ConnectionStage::Connecting {
            return None;
        }

        Some(self.reconnection_type())
    }

    pub fn reconnection_type(&self) -> ReconnectType {
        if self.session_id().is_some() {
            ReconnectType::Resume
        } else {
            ReconnectType::Reidentify
        }
    }

    /// Requests that one or multiple [`Guild`]s be chunked.
    ///
    /// This will ask the gateway to start sending member chunks for large
    /// guilds (250 members+). If a guild is over 250 members, then a full
    /// member list will not be downloaded, and must instead be requested to be
    /// sent in "chunks" containing members.
    ///
    /// Member chunks are sent as the [`Event::GuildMembersChunk`] event. Each
    /// chunk only contains a partial amount of the total members.
    ///
    /// If the `cache` feature is enabled, the cache will automatically be
    /// updated with member chunks.
    ///
    /// # Examples
    ///
    /// Chunk a single guild by Id, limiting to 2000 [`Member`]s, and not
    /// specifying a query parameter:
    ///
    /// ```rust,no_run
    /// # extern crate parking_lot;
    /// # extern crate serenity;
    /// #
    /// # use parking_lot::Mutex;
    /// # use serenity::client::gateway::Shard;
    /// # use std::error::Error;
    /// # use std::sync::Arc;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// #     let mutex = Arc::new(Mutex::new("".to_string()));
    /// #
    /// #     let mut shard = Shard::new(mutex.clone(), mutex, [0, 1])?;
    /// #
    /// use serenity::model::id::GuildId;
    ///
    /// let guild_ids = vec![GuildId(81384788765712384)];
    ///
    /// shard.chunk_guilds(guild_ids, Some(2000), None);
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    ///
    /// Chunk a single guild by Id, limiting to 20 members, and specifying a
    /// query parameter of `"do"`:
    ///
    /// ```rust,no_run
    /// # extern crate parking_lot;
    /// # extern crate serenity;
    /// #
    /// # use parking_lot::Mutex;
    /// # use serenity::client::gateway::Shard;
    /// # use std::error::Error;
    /// # use std::sync::Arc;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// #     let mutex = Arc::new(Mutex::new("".to_string()));
    /// #
    /// #     let mut shard = Shard::new(mutex.clone(), mutex, [0, 1])?;
    /// #
    /// use serenity::model::id::GuildId;
    ///
    /// let guild_ids = vec![GuildId(81384788765712384)];
    ///
    /// shard.chunk_guilds(guild_ids, Some(20), Some("do"));
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    ///
    /// [`Event::GuildMembersChunk`]: ../model/event/enum.Event.html#variant.GuildMembersChunk
    /// [`Guild`]: ../model/guild/struct.Guild.html
    /// [`Member`]: ../model/guild/struct.Member.html
    pub fn chunk_guilds<It>(
        &mut self,
        guild_ids: It,
        limit: Option<u16>,
        query: Option<&str>,
    ) -> Result<()> where It: IntoIterator<Item=GuildId> {
        debug!("[Shard {:?}] Requesting member chunks", self.shard_info);

        self.client.send_chunk_guilds(
            guild_ids,
            &self.shard_info,
            limit,
            query,
        )
    }

    // Sets the shard as going into identifying stage, which sets:
    //
    // - the time that the last heartbeat sent as being now
    // - the `stage` to `Identifying`
    pub fn identify(&mut self) -> Result<()> {
        self.client.send_identify(&self.shard_info, &self.token.lock())?;

        self.heartbeat_instants.0 = Some(Instant::now());
        self.stage = ConnectionStage::Identifying;

        Ok(())
    }

    /// Initializes a new WebSocket client.
    ///
    /// This will set the stage of the shard before and after instantiation of
    /// the client.
    pub fn initialize(&mut self) -> Result<WsClient> {
        debug!("[Shard {:?}] Initializing", self.shard_info);

        // We need to do two, sort of three things here:
        //
        // - set the stage of the shard as opening the websocket connection
        // - open the websocket connection
        // - if successful, set the current stage as Handshaking
        //
        // This is used to accurately assess whether the state of the shard is
        // accurate when a Hello is received.
        self.stage = ConnectionStage::Connecting;
        self.started = Instant::now();
        let mut client = connect(&self.ws_url.lock())?;
        self.stage = ConnectionStage::Handshake;

        let _ = set_client_timeout(&mut client);

        Ok(client)
    }

    pub fn reset(&mut self) {
        self.heartbeat_instants = (Some(Instant::now()), None);
        self.heartbeat_interval = None;
        self.last_heartbeat_acknowledged = true;
        self.session_id = None;
        self.stage = ConnectionStage::Disconnected;
        self.seq = 0;
    }

    pub fn resume(&mut self) -> Result<()> {
        debug!("Shard {:?}] Attempting to resume", self.shard_info);

        self.client = self.initialize()?;
        self.stage = ConnectionStage::Resuming;

        match self.session_id.as_ref() {
            Some(session_id) => {
                self.client.send_resume(
                    &self.shard_info,
                    session_id,
                    &self.seq,
                    &self.token.lock(),
                )
            },
            None => Err(Error::Gateway(GatewayError::NoSessionId)),
        }
    }

    pub fn reconnect(&mut self) -> Result<()> {
        info!("[Shard {:?}] Attempting to reconnect", self.shard_info());

        self.reset();
        self.client = self.initialize()?;

        Ok(())
    }

    pub fn update_presence(&mut self) -> Result<()> {
        self.client.send_presence_update(
            &self.shard_info,
            &self.current_presence,
        )
    }
}

fn connect(base_url: &str) -> Result<WsClient> {
    let url = build_gateway_url(base_url)?;
    let client = ClientBuilder::from_url(&url).connect_secure(None)?;

    Ok(client)
}

fn set_client_timeout(client: &mut WsClient) -> Result<()> {
    let stream = client.stream_ref().as_tcp();
    stream.set_read_timeout(Some(StdDuration::from_millis(100)))?;
    stream.set_write_timeout(Some(StdDuration::from_secs(5)))?;

    Ok(())
}

fn build_gateway_url(base: &str) -> Result<Url> {
    Url::parse(&format!("{}?v={}", base, constants::GATEWAY_VERSION))
        .map_err(|why| {
            warn!("Error building gateway URL with base `{}`: {:?}", base, why);

            Error::Gateway(GatewayError::BuildingUrl)
        })
}
