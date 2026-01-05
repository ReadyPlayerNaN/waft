use anyhow::{Context, Result};
use dbus::arg::RefArg;
use dbus::arg::Variant;
use dbus::message::MatchRule;
use dbus::nonblock::MsgMatch;
use dbus::nonblock::Proxy;
use dbus::nonblock::SyncConnection;
use dbus_tokio::connection;
use futures_channel::mpsc::UnboundedReceiver;
use std::sync::Arc;
use std::time::Duration;
// use tokio::sync::mpsc;

use tokio_stream::StreamExt;

/// Shared async DBus session-bus connection.
///
/// This wrapper is intentionally simple and generic:
/// - It owns a single `SyncConnection` created via `dbus_tokio::connection::new_session_sync`.
/// - It exposes a way to register a listener for DBus signals matching a `MatchRule`.
/// - It exposes a way to unregister that listener again.
///
/// Domain-specific code (like the darkman plugin) is responsible for:
/// - Defining the `MatchRule` (interface, path, member, etc.).
/// - Decoding the raw `Message` values into domain events.
#[derive(Clone)]
pub struct DbusHandle {
    conn: Arc<SyncConnection>,
}

impl DbusHandle {
    /// Initialize a shared DBus session-bus connection.
    ///
    /// This:
    /// - Connects to the session bus.
    /// - Spawns the background resource future that drives the connection.
    /// - Wraps the non-blocking `SyncConnection` in an `Arc` for cheap cloning.
    pub async fn connect() -> Result<Self> {
        let (resource, conn) =
            connection::new_session_sync().context("Failed to establish async DBus session")?;

        // Drive the DBus connection in the background. It will shut down
        // automatically once all clones of `conn` are dropped.
        tokio::spawn(async move {
            let _ = resource.await;
        });

        Ok(Self { conn: conn })
    }

    /// Register a listener for DBus signals matching the given `MatchRule`.
    ///
    /// All matching messages are forwarded into the provided unbounded
    /// channel. This method:
    /// - Installs the match rule on the connection.
    /// - Spawns a background task that pulls typed messages from the
    ///   connection and forwards them to `tx`.
    ///
    /// The returned `DbusListener` can later be passed to
    /// `unregister_listener` to remove the match rule.
    // // pub async fn register_listener(
    // //     &self,
    // //     mut rule: MatchRule<'static>,
    // //     tx: mpsc::UnboundedSender<Message>,
    // // ) -> Result<DbusListener> {
    // //     // Ensure we're only listening for signals.
    // //     if rule.msg_type.is_none() {
    // //         rule.msg_type = Some(dbus::message::MessageType::Signal);
    // //     }

    // //     let conn = self.conn.clone();

    // //     // Install the match rule on the non-blocking connection and get a match handle.
    // //     let msg_match = conn
    // //         .add_match(rule.clone())
    // //         .await
    // //         .context("Failed to add DBus match rule")?;

    // //     // Turn the match handle into a typed stream of raw `Message`s.
    // //     let (msg_match, mut stream) = msg_match.stream();

    // //     // Drive the stream in a background task and forward any messages
    // //     // to the provided channel.
    // //     tokio::spawn(async move {
    // //         while let Some((msg, (_,))) = stream.next().await {
    // //             if tx.send(msg).is_err() {
    // //                 // Receiver dropped; stop forwarding.
    // //                 break;
    // //             }
    // //         }

    // //         // Drop the match handle so the rule is removed when the task ends.
    // //         drop(msg_match);
    // //     });

    // //     Ok(DbusListener { conn, rule })
    // }

    /// Unregister a previously registered listener.
    ///
    /// This removes the match rule associated with the given listener from
    /// the underlying connection. After this returns, no further messages
    /// will be delivered to the channel associated with that listener.
    // pub async fn unregister_listener(&self, listener: DbusListener) -> Result<()> {
    //     // Remove the match rule from the connection. We ignore the result
    //     // of the background task spawned in `register_listener` — it will
    //     // simply see no more messages and exit.
    //     listener
    //         .conn
    //         .remove_match(listener.rule)
    //         .await
    //         .context("Failed to remove DBus match rule")?;

    //     Ok(())
    // }

    /// Expose the underlying connection for advanced use-cases.
    // pub fn connection(&self) -> Arc<SyncConnection> {
    //     self.conn.clone()
    // }

    pub fn proxy<'a>(&'a self, interface: &'a str, path: &'a str) -> Proxy<'a, &'a SyncConnection> {
        Proxy::new(interface, path, Duration::from_secs(5), &self.conn)
    }

    pub async fn get_property(
        &self,
        interface: &str,
        path: &str,
        property: &str,
    ) -> Result<Option<String>> {
        let (value,): (Variant<Box<dyn RefArg>>,) = self
            .proxy(interface, path)
            .method_call(
                "org.freedesktop.DBus.Properties",
                "Get",
                (interface, property),
            )
            .await
            .context("Failed to get property via DBus")?;

        Ok(value.0.as_str().map(|s| s.to_owned()))
    }

    pub async fn set_property(
        &self,
        interface: &str,
        path: &str,
        property: &str,
        value: &str,
    ) -> Result<()> {
        self.proxy(interface, path)
            .method_call(
                "org.freedesktop.DBus.Properties",
                "Set",
                (interface, property, Variant(Box::new(value))),
            )
            .await
            .context("Failed to set Mode property via DBus")
    }

    pub async fn stream<'a, T>(
        &'a self,
        rule: MatchRule<'static>,
    ) -> Result<(MsgMatch, UnboundedReceiver<(dbus::Message, T)>)>
    where
        T: dbus::arg::ReadAll + Send + 'static,
    {
        let msgmatch = self.conn.add_match(rule).await?;
        let (m, stream) = msgmatch.stream::<T>();
        Ok((m, stream))
    }

    pub async fn listen_for_values<'a, T>(
        &'a self,
        rule: MatchRule<'static>,
        mut on_value: impl FnMut(Option<T>) + Send + 'static,
    ) -> Result<()>
    where
        T: dbus::arg::Arg + for<'z> dbus::arg::Get<'z> + Send + 'static,
    {
        let (msgmatch, mut rx) = self.stream::<(T,)>(rule).await?;
        tokio::spawn(async move {
            let _msgmatch = msgmatch;
            while let Some((_msg, (value,))) = rx.next().await {
                on_value(Some(value));
            }
        });
        Ok(())
    }
}
// Extension trait to bring `.next()` into scope for the typed stream.
// This mirrors what `tokio_stream::StreamExt` provides, but we keep the
// dependency surface small and local to this module.
// trait StreamExtLocal: futures_core::Stream {
//     fn next(&mut self) -> futures_util::stream::Next<'_, Self>
//     where
//         Self: Unpin,
//     {
//         futures_util::stream::StreamExt::next(self)
//     }
// }

// impl<T: futures_core::Stream> StreamExtLocal for T {}
