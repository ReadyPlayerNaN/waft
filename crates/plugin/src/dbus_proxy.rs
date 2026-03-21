//! Lightweight D-Bus service proxy that reduces boilerplate.
//!
//! Most plugins follow the same pattern when calling D-Bus methods:
//!
//! 1. Create a `zbus::Proxy` with destination, path, and interface
//! 2. Call a method on it with error context
//! 3. Repeat with the same constants dozens of times
//!
//! [`DbusService`] wraps this into a reusable handle.
//!
//! # Example
//!
//! ```rust,no_run
//! # use anyhow::Result;
//! # async fn example(conn: &zbus::Connection) -> Result<()> {
//! use waft_plugin::dbus_proxy::DbusService;
//!
//! let manager = DbusService::new(
//!     conn,
//!     "org.freedesktop.systemd1",
//!     "/org/freedesktop/systemd1",
//!     "org.freedesktop.systemd1.Manager",
//! );
//!
//! let state: String = manager.call("GetUnitFileState", &("foo.service",)).await?;
//! # Ok(())
//! # }
//! ```

use anyhow::Context;
use zbus::Connection;

/// A reusable D-Bus service handle that reduces proxy creation boilerplate.
///
/// Holds the connection and service coordinates (destination, path, interface)
/// and provides `call` / `try_call` methods that create a proxy, invoke the
/// method, and add error context automatically.
pub struct DbusService<'a> {
    conn: &'a Connection,
    destination: &'a str,
    path: &'a str,
    interface: &'a str,
}

impl<'a> DbusService<'a> {
    /// Create a new D-Bus service handle.
    pub fn new(
        conn: &'a Connection,
        destination: &'a str,
        path: &'a str,
        interface: &'a str,
    ) -> Self {
        Self {
            conn,
            destination,
            path,
            interface,
        }
    }

    /// Create a handle that shares the connection and service but targets a different object path.
    pub fn at(&self, path: &'a str) -> Self {
        Self {
            conn: self.conn,
            destination: self.destination,
            path,
            interface: self.interface,
        }
    }

    /// Create a handle that shares the connection and destination but targets a different interface.
    pub fn interface(&self, interface: &'a str) -> Self {
        Self {
            conn: self.conn,
            destination: self.destination,
            path: self.path,
            interface,
        }
    }

    /// Call a D-Bus method and return the deserialized result.
    ///
    /// Automatically adds error context including the method name.
    pub async fn call<B, R>(&self, method: &str, args: &B) -> anyhow::Result<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        let proxy = zbus::Proxy::new(self.conn, self.destination, self.path, self.interface)
            .await
            .with_context(|| {
                format!(
                    "failed to create D-Bus proxy for {}.{}",
                    self.interface, method
                )
            })?;

        proxy
            .call(method, args)
            .await
            .with_context(|| format!("D-Bus call {}.{} failed", self.interface, method))
    }

    /// Call a D-Bus method, returning `None` if the call fails.
    ///
    /// Useful for best-effort queries where failure is expected (e.g.,
    /// querying properties of units that may not exist).
    pub async fn try_call<B, R>(&self, method: &str, args: &B) -> Option<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        let proxy = zbus::Proxy::new(self.conn, self.destination, self.path, self.interface)
            .await
            .ok()?;
        proxy.call(method, args).await.ok()
    }
}
