//! Linux portal text scale. Port of `SystemTheme::requestPortalTextScale` and
//! `sanitizedTextScale`, with a session-bus guard before the portal call.
//! Non-Linux builds expose only `sanitized_text_scale` and no-op portal entry
//! points.

/// Matches Qt `sanitizedTextScale`: invalid values are rejected; valid values clamp to
/// `[0.5, 3.0]`.
#[cfg(any(test, target_os = "linux"))]
pub fn sanitized_text_scale(scale: f64) -> Option<f32> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    Some((scale.clamp(0.5, 3.0)) as f32)
}

/// The portal's current text scale, read once with a bounded wait. `None` when the
/// session bus is refused, the read times out, or the value is not a known scale.
#[cfg(target_os = "linux")]
pub fn read_portal_text_scale() -> Option<f32> {
    linux::read_portal_text_scale()
}

/// Subscribes to portal `SettingChanged` for `text-scaling-factor`. Each sanitized
/// scale is sent on the returned channel. `None` when the session bus is refused or
/// the worker cannot subscribe.
#[cfg(target_os = "linux")]
pub fn start_portal_text_scale_watcher() -> Option<async_channel::Receiver<f32>> {
    linux::start_portal_text_scale_watcher()
}

#[cfg(not(target_os = "linux"))]
pub fn read_portal_text_scale() -> Option<f32> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn start_portal_text_scale_watcher() -> Option<async_channel::Receiver<f32>> {
    None
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::Path;
    use std::time::Duration;

    use futures::StreamExt;
    use zbus::{proxy, zvariant, Connection};

    use super::sanitized_text_scale;

    const GNOME_INTERFACE_NS: &str = "org.gnome.desktop.interface";
    const TEXT_SCALING_KEY: &str = "text-scaling-factor";
    const READ_TIMEOUT: Duration = Duration::from_secs(2);

    pub fn read_portal_text_scale() -> Option<f32> {
        if !linux_session_bus_allowed() {
            return None;
        }
        bounded(READ_TIMEOUT, read_portal_text_scale_async())
    }

    pub fn start_portal_text_scale_watcher() -> Option<async_channel::Receiver<f32>> {
        if !linux_session_bus_allowed() {
            return None;
        }
        let (sender, receiver) = async_channel::unbounded();
        std::thread::spawn(move || {
            futures::executor::block_on(run_portal_watcher(sender));
        });
        Some(receiver)
    }

    fn linux_session_bus_allowed() -> bool {
        match std::env::var("DBUS_SESSION_BUS_ADDRESS") {
            Ok(address) => {
                if address.starts_with("autolaunch:") {
                    return false;
                }
                const PREFIX: &str = "unix:path=";
                if let Some(path_part) = address.strip_prefix(PREFIX) {
                    let path = path_part.split(',').next().unwrap_or("");
                    if path.is_empty() || !Path::new(path).exists() {
                        return false;
                    }
                }
                true
            }
            Err(_) => true,
        }
    }

    fn bounded(
        timeout: Duration,
        call: impl std::future::Future<Output = Option<f32>> + Send + 'static,
    ) -> Option<f32> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(futures::executor::block_on(call));
        });
        receiver.recv_timeout(timeout).ok().flatten()
    }

    #[proxy(
        interface = "org.freedesktop.portal.Settings",
        default_service = "org.freedesktop.portal.Desktop",
        default_path = "/org/freedesktop/portal/desktop"
    )]
    trait PortalSettings {
        fn read(&self, namespace: &str, key: &str) -> zbus::Result<zvariant::OwnedValue>;

        #[zbus(signal, name = "SettingChanged")]
        fn setting_changed(
            &self,
            namespace: &str,
            key: &str,
            value: zvariant::OwnedValue,
        ) -> zbus::Result<()>;
    }

    async fn read_portal_text_scale_async() -> Option<f32> {
        let connection = Connection::session().await.ok()?;
        let settings = PortalSettingsProxy::new(&connection).await.ok()?;
        let value = settings
            .read(GNOME_INTERFACE_NS, TEXT_SCALING_KEY)
            .await
            .ok()?;
        portal_value_to_f64(&value).and_then(sanitized_text_scale)
    }

    async fn run_portal_watcher(sender: async_channel::Sender<f32>) {
        let connection = match Connection::session().await {
            Ok(connection) => connection,
            Err(_) => return,
        };
        let settings = match PortalSettingsProxy::new(&connection).await {
            Ok(settings) => settings,
            Err(_) => return,
        };
        let mut stream = match settings.receive_setting_changed().await {
            Ok(stream) => stream,
            Err(_) => return,
        };
        while let Some(signal) = stream.next().await {
            let args = match signal.args() {
                Ok(args) => args,
                Err(_) => continue,
            };
            if *args.namespace() != GNOME_INTERFACE_NS || *args.key() != TEXT_SCALING_KEY {
                continue;
            }
            if let Some(scale) = portal_value_to_f64(args.value()).and_then(sanitized_text_scale)
            {
                if sender.send(scale).await.is_err() {
                    return;
                }
            }
        }
    }

    /// Unwraps nested `QDBusVariant`-style values, then reads a scalar double.
    fn portal_value_to_f64(value: &zvariant::OwnedValue) -> Option<f64> {
        let mut current: &zvariant::Value<'_> = value;
        for _ in 0..8 {
            if let Some(scale) = scalar_f64(current) {
                return Some(scale);
            }
            current = match current {
                zvariant::Value::Value(inner) => inner,
                zvariant::Value::Structure(structure) if structure.fields().len() == 1 => {
                    structure.fields().first()?
                }
                _ => return None,
            };
        }
        None
    }

    fn scalar_f64(value: &zvariant::Value<'_>) -> Option<f64> {
        match value {
            zvariant::Value::F64(scale) => Some(*scale),
            zvariant::Value::I16(scale) => Some(f64::from(*scale)),
            zvariant::Value::U16(scale) => Some(f64::from(*scale)),
            zvariant::Value::I32(scale) => Some(f64::from(*scale)),
            zvariant::Value::U32(scale) => Some(f64::from(*scale)),
            zvariant::Value::I64(scale) => Some(*scale as f64),
            zvariant::Value::U64(scale) => Some(*scale as f64),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::linux_session_bus_allowed;
        use std::sync::{Mutex, OnceLock};

        #[test]
        fn session_bus_guard_refuses_autolaunch_and_missing_unix_paths() {
            static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
            let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

            let key = "DBUS_SESSION_BUS_ADDRESS";
            let previous = std::env::var(key).ok();

            std::env::set_var(key, "autolaunch:");
            assert!(!linux_session_bus_allowed());

            let missing = format!("unix:path=/tmp/omairc-missing-dbus-{}", std::process::id());
            std::env::set_var(key, &missing);
            assert!(!linux_session_bus_allowed());

            std::env::remove_var(key);
            assert!(linux_session_bus_allowed());

            match previous {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sanitized_text_scale;

    #[test]
    fn sanitized_text_scale_rejects_non_positive_and_nan() {
        assert_eq!(sanitized_text_scale(0.0), None);
        assert_eq!(sanitized_text_scale(-1.0), None);
        assert_eq!(sanitized_text_scale(f64::NAN), None);
        assert_eq!(sanitized_text_scale(f64::INFINITY), None);
    }

    #[test]
    fn sanitized_text_scale_clamps_to_portal_bounds() {
        assert_eq!(sanitized_text_scale(0.25), Some(0.5));
        assert_eq!(sanitized_text_scale(0.5), Some(0.5));
        assert_eq!(sanitized_text_scale(1.0), Some(1.0));
        assert_eq!(sanitized_text_scale(2.5), Some(2.5));
        assert_eq!(sanitized_text_scale(3.0), Some(3.0));
        assert_eq!(sanitized_text_scale(4.0), Some(3.0));
    }
}
