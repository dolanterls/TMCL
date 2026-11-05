//! Shared helpers for the end-to-end tests.
//!
//! Each `tests/*.rs` is its own crate, so the only way to share code is a
//! module both of them declare. Nothing here touches the network beyond
//! `127.0.0.1`.

// Each test crate pulls in this module separately and uses a different subset.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A local HTTP server that answers a fixed set of routes.
///
/// It keeps listening until it is dropped: the download stack may open more than
/// one connection (a retry, a resume probe), and a listener that accepts once
/// turns those into a "connection refused" that looks like a bug in the code
/// under test.
pub struct Server {
    base: String,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    pub fn start(routes: Vec<(&'static str, &'static [u8], &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        listener
            .set_nonblocking(true)
            .expect("non-blocking listener");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0u8; 4096];
                        let read = stream.read(&mut buffer).unwrap_or(0);
                        let request = String::from_utf8_lossy(&buffer[..read]);
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or("/")
                            .split('?')
                            .next()
                            .unwrap_or("/")
                            .to_owned();
                        let route = routes.iter().find(|(name, _, _)| *name == path);
                        let response = match route {
                            Some((_, body, content_type)) => format!(
                                "HTTP/1.1 200 OK\r\nServer: tmcl-test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            ),
                            None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
                        };
                        let _ = stream.write_all(response.as_bytes());
                        if let Some((_, body, _)) = route {
                            let _ = stream.write_all(body);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base: format!("http://{address}"),
            stop,
            handle: Some(handle),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// A throwaway launcher home plus one discoverable vanilla instance.
///
/// Every test gets its own `APPDATA` and game directory, so nothing reads or
/// writes the developer's real launcher data. Dropping it removes both.
pub struct Sandbox {
    pub root: std::path::PathBuf,
}

/// Version JSON shared by the instance fixtures.
pub const CLIENT: &str = r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]}}"#;

impl Sandbox {
    pub fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "tmcl-cli-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("appdata")).expect("appdata");
        let instance = root.join("game").join("versions").join("demo");
        std::fs::create_dir_all(&instance).expect("instance dir");
        std::fs::write(instance.join("demo.jar"), b"jar").expect("jar");
        std::fs::write(instance.join("demo.json"), CLIENT).expect("client json");
        std::fs::write(
            instance.join("tmcl.json"),
            br#"{"id":"11111111-1111-4111-8111-111111111111","name":"demo","game_directory_id":"22222222-2222-4222-8222-222222222222","minecraft_version":"1.21.1","loader":"Vanilla","stats":{"launch_count":0,"play_time_seconds":0}}"#,
        )
        .expect("metadata");
        Self { root }
    }

    pub fn appdata(&self) -> std::path::PathBuf {
        self.root.join("appdata")
    }

    /// Directory `DataPaths::discover` uses for persisted tasks.
    pub fn tasks_dir(&self) -> std::path::PathBuf {
        self.appdata().join("TMCL").join("tasks")
    }

    pub fn game(&self) -> std::path::PathBuf {
        self.root.join("game")
    }

    pub fn instance(&self) -> std::path::PathBuf {
        self.game().join("versions").join("demo")
    }

    pub fn run(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_tmcl"))
            .env("APPDATA", self.appdata())
            .args(args)
            .output()
            .expect("run tmcl")
    }

    pub fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{} did not print JSON ({error}): {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stdout)
            )
        })
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
