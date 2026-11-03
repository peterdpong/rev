use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

pub type Reply = Result<Value, String>;
type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Reply>>>>;

/// Locate supporting files, including the compiled TypeScript host (`host.mjs`).
pub fn resource_dir() -> PathBuf {
    // An explicit override takes priority over bundled and development resources.
    if let Some(path) = std::env::var_os("REV_RESOURCES") {
        return path.into();
    }

    // TODO: Support packaged launches on other platforms:
    // - Define Windows and Linux resource layouts and locate host.mjs in each.
    // - Update node_binary() for bundled runtimes, including Windows node.exe.
    // - Update host_path() with platform-appropriate command search locations.
    // - Verify packaged launches without relying on the source tree or shell PATH.
    // In a macOS app bundle, the executable lives in Contents/MacOS and
    // supporting files live in Contents/Resources. Use it if the host is present.
    if let Ok(exe) = std::env::current_exe() {
        let bundled = exe.parent().unwrap().join("../Resources");
        if bundled.join("host.mjs").is_file() {
            return bundled;
        }
    }

    // During development, npm run build:host writes host.mjs to the project's
    // dist folder and Cargo embeds the Cargo.toml directory here at compile time.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist")
}

pub fn node_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("REV_NODE_BINARY") {
        return path.into();
    }

    let bundled = resource_dir().join("node");
    if bundled.is_file() {
        return bundled;
    }

    for path in ["/opt/homebrew/bin/node", "/usr/local/bin/node"] {
        if std::path::Path::new(path).is_file() {
            return path.into();
        }
    }
    "node".into()
}

/// Build the backend's PATH so it can find commands such as Git and Node.
pub fn host_path() -> std::ffi::OsString {
    // Search bundled tools and the selected Node installation first.
    let mut paths = vec![resource_dir()];
    if let Some(parent) = node_binary().parent().filter(|p| !p.as_os_str().is_empty()) {
        paths.push(parent.to_path_buf());
    }

    // Preserve command locations inherited from the launching environment.
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }

    // Finder launches may lack the terminal's PATH, so include common Homebrew
    // and system executable directories as fallbacks.
    paths.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);

    // Include tools installed just for this user.
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".local/bin"));
    }
    
    // Encode the folders in the platform's PATH format (colon-separated on macOS).
    std::env::join_paths(paths).unwrap_or_default()
}

struct BackendState {
    input: Mutex<Option<ChildStdin>>,
    pending: Pending,
    next: AtomicU64,
}

#[derive(Clone)]
pub struct Backend {
    state: Arc<BackendState>,
}

impl Backend {
    pub fn start() -> Result<Self, String> {
        let script = resource_dir().join("host.mjs");
        if !script.is_file() {
            return Err("Build the local service first with npm run build:host.".into());
        }

        let mut child = Command::new(node_binary()).arg(script).env("PATH", host_path())
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
            .spawn().map_err(|e| format!("Could not start the local service: {e}. Install Node 22+ or set REV_NODE_BINARY."))?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let pending: Pending = Arc::default();
        let inbox = pending.clone();
        
        std::thread::spawn(move || {
            // Wait for replies until stdout closes or a read fails.
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };

                let Some(id) = value["id"].as_u64() else {
                    continue;
                };
                
                if let Some(sender) = inbox.lock().unwrap().remove(&id) {
                    let reply = match value["error"].as_str() {
                        Some(error) => Err(error.to_string()),
                        None => Ok(value["result"].clone()),
                    };

                    let _ = sender.send(reply);
                }
            }

            // The response pipe is gone; clear pending requests and notify callers.
            for (_, sender) in inbox.lock().unwrap().drain() {
                let _ = sender.send(Err("The local service stopped. Restart Rev.".into()));
            }

            // Wait for the process to exit and collect its exit status.
            let _ = child.wait();
        });
        
        Ok(Self {
            state: Arc::new(BackendState {
                input: Mutex::new(Some(input)),
                pending,
                next: AtomicU64::new(1),
            }),
        })
    }

    // Called on GPUI's background executor, never its drawing/event thread.
    pub fn call(&self, method: &str, params: Value) -> Reply {
        // Register a channel so the response reader can route this request's reply.
        let id = self.state.next.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        self.state.pending.lock().unwrap().insert(id, sender);

        // Send one JSON request line to the TypeScript host's stdin.
        let line = format!(
            "{}\n",
            json!({ "id": id, "method": method, "params": params })
        );
        let write = self
            .state
            .input
            .lock()
            .unwrap()
            .as_mut()
            .ok_or_else(|| "The local service is closed.".to_string())
            .and_then(|input| input.write_all(line.as_bytes()).map_err(|e| e.to_string()));

        if let Err(error) = write {
            // A failed send must not leave a pending request behind.
            self.state.pending.lock().unwrap().remove(&id);
            return Err(error);
        }

        // Block until the response reader delivers the reply, or the wait expires.
        let result = receiver
            .recv_timeout(Duration::from_secs(150))
            .map_err(|_| "The local service did not respond in time.".to_string());
            
        // Remove any entry left after a timeout; replies normally remove it first.
        self.state.pending.lock().unwrap().remove(&id);
        result?
    }

    pub fn close(&self) {
        self.state.input.lock().unwrap().take();
    }
}
