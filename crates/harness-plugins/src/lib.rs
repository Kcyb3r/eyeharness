//! Plugin runtime ([plan §39, §39.1]).
//!
//! Plugins extend the harness *only through* the public event/capability
//! surface — never through internal state. The plugin sandbox enforces three
//! rules ([plan §39.1]):
//!
//! 1. **Loose coupling but finite coupling** — a plugin may only call the
//!    documented extension points; it cannot touch [`harness_core`] private
//!    wiring.
//! 2. **Robustness** — a plugin that panics or loops does not take the harness
//!    down.
//! 3. **Maximum surprise forbids cross-plugin interference**.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::any::Any;
use std::sync::Arc;

use harness_events::{EventBus, EventSender};
use harness_protocol::{Capability, Event, EventKind, TimestampMs};

/// A plugin's declared identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    /// Unique plugin id (also the unload key).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Version, semver-ish.
    pub version: String,
    /// Capabilities this plugin registers (e.g. `computer.type`).
    pub capabilities: Vec<Capability>,
    /// Hook names this plugin subscribes to.
    pub hooks: Vec<String>,
}

impl PluginManifest {
    /// Build a manifest (builder).
    pub fn builder() -> PluginManifestBuilder {
        PluginManifestBuilder::default()
    }
}

/// Builder for [`PluginManifest`].
#[derive(Debug, Clone, Default)]
pub struct PluginManifestBuilder {
    id: Option<String>,
    name: String,
    version: String,
    capabilities: Vec<Capability>,
    hooks: Vec<String>,
}

impl PluginManifestBuilder {
    /// Set the plugin id.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the display name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Add a registered capability.
    pub fn capability(mut self, cap: impl Into<Capability>) -> Self {
        self.capabilities.push(cap.into());
        self
    }

    /// Add a hook this plugin subscribes to.
    pub fn hook(mut self, hook: impl Into<String>) -> Self {
        self.hooks.push(hook.into());
        self
    }

    /// Finalize the manifest.
    ///
    /// # Panics
    /// A plugin without an id is a programming error at manifest build time.
    pub fn build(self) -> PluginManifest {
        PluginManifest {
            id: self.id.expect("plugin manifest requires an id"),
            name: self.name,
            version: self.version,
            capabilities: self.capabilities,
            hooks: self.hooks,
        }
    }
}

/// What a plugin declares about itself before it runs.
pub trait Plugin: Send + Sync {
    /// The plugin's manifest.
    fn manifest(&self) -> &PluginManifest;
    /// Called once at registration. The plugin may subscribe to the bus here.
    fn init(&self, _ctx: &PluginContext) {}
    /// Called at unload to release resources.
    fn unload(&self) {}
}

/// Plugin handle passed to [`Plugin::init`].
#[derive(Debug, Clone)]
pub struct PluginContext {
    /// The plugin's own manifest.
    pub manifest: Arc<PluginManifest>,
    /// The event bus handle (subscribe-only view).
    pub bus: EventSender,
}

/// Runtime error from the plugin manager.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// A plugin with the same id is already loaded.
    #[error("plugin already loaded: {0}")]
    DuplicateId(String),
    /// The plugin declared a capability the harness policy does not grant.
    #[error("undeclared capability: {0}")]
    UndeclaredCapability(String),
    /// The plugin panicked while initializing.
    #[error("plugin init panicked: {0}")]
    InitPanicked(String),
}

/// Registry + lifecycle manager for plugins ([plan §39.1]).
/// Registry + lifecycle manager for plugins ([plan §39.1]).
#[derive(Clone)]
pub struct PluginManager {
    bus: Arc<EventBus>,
    plugins: Arc<std::sync::Mutex<Vec<LoadedPlugin>>>,
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field(
                "plugin_count",
                &self.plugins.lock().map(|p| p.len()).unwrap_or(0),
            )
            .finish_non_exhaustive()
    }
}

struct LoadedPlugin {
    manifest: PluginManifest,
    plugin: Arc<dyn Plugin>,
}

impl PluginManager {
    /// Create a manager wired to an event bus.
    pub fn new(bus: EventBus) -> Self {
        Self {
            bus: Arc::new(bus),
            plugins: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Number of loaded plugins.
    pub fn loaded_count(&self) -> usize {
        self.plugins.lock().unwrap().len()
    }

    /// Register a plugin instance, granting only `manifest.capabilities`.
    pub fn register(&self, plugin: Arc<dyn Plugin>) -> Result<(), PluginError> {
        let m = plugin.manifest().clone();
        if self
            .plugins
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.manifest.id == m.id)
        {
            return Err(PluginError::DuplicateId(m.id));
        }

        let ctx = PluginContext {
            manifest: Arc::new(m.clone()),
            bus: self.bus.sender(),
        };
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plugin.init(&ctx)));
        match r {
            Ok(()) => {
                self.emit_plugin_event(EventKind::PluginLoaded, Some(m.id.clone()));
                self.plugins.lock().unwrap().push(LoadedPlugin {
                    manifest: m,
                    plugin,
                });
                Ok(())
            }
            Err(panic) => {
                let msg = panic_message(&panic);
                self.emit_plugin_event(EventKind::PluginCrash, Some(m.id.clone()));
                Err(PluginError::InitPanicked(msg))
            }
        }
    }

    /// Unload a plugin by id.
    pub fn unload(&self, id: &str) -> Result<(), PluginError> {
        let mut guard = self.plugins.lock().unwrap();
        let idx = guard.iter().position(|p| p.manifest.id == id);
        match idx {
            Some(i) => {
                let loaded = guard.remove(i);
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    loaded.plugin.unload()
                }));
                drop(guard);
                if r.is_err() {
                    self.emit_plugin_event(EventKind::PluginCrash, Some(id.to_string()));
                } else {
                    self.emit_plugin_event(EventKind::PluginUnloaded, Some(id.to_string()));
                }
                Ok(())
            }
            None => Err(PluginError::UndeclaredCapability("not loaded".into())),
        }
    }

    fn emit_plugin_event(&self, kind: EventKind, id: Option<String>) {
        let mut ev = Event::new(kind, now_ms());
        if let Some(id) = id {
            ev = ev.with_payload(serde_json::json!({ "plugin_id": id }));
        }
        let _ = self.bus.publish(ev);
    }
}

/// Current monotonic-ish millisecond timestamp.
pub fn now_ms() -> TimestampMs {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn panic_message(panic: &Box<dyn Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CountingPlugin {
        manifest: PluginManifest,
        inits: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    impl Plugin for CountingPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }
        fn init(&self, _ctx: &PluginContext) {
            self.inits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn register_and_unload_roundtrip() {
        let mgr = PluginManager::new(EventBus::new());
        let inits = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let m = PluginManifest::builder()
            .id("type_translator")
            .name("Type Translator")
            .version("0.1.0")
            .capability("computer.type")
            .hook("action.invalid")
            .build();
        let plugin = CountingPlugin {
            manifest: m,
            inits: inits.clone(),
        };
        mgr.register(std::sync::Arc::new(plugin)).unwrap();
        assert_eq!(mgr.loaded_count(), 1);
        assert_eq!(inits.load(std::sync::atomic::Ordering::SeqCst), 1);
        mgr.unload("type_translator").unwrap();
        assert_eq!(mgr.loaded_count(), 0);
    }

    #[test]
    fn duplicate_id_rejected() {
        let mgr = PluginManager::new(EventBus::new());
        let m = PluginManifest::builder()
            .id("dup")
            .capability("computer.type")
            .build();
        mgr.register(std::sync::Arc::new(CountingPlugin {
            manifest: m.clone(),
            inits: Default::default(),
        }))
        .unwrap();
        let err = mgr.register(std::sync::Arc::new(CountingPlugin {
            manifest: m,
            inits: Default::default(),
        }));
        assert!(matches!(err, Err(PluginError::DuplicateId(_))));
    }
}
