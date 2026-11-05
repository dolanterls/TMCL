use std::path::{Path, PathBuf};

use crate::core::account::models::Account;
use crate::core::account::skin::SkinStore;
use crate::core::account::yggdrasil::{PlayerProfile, YggdrasilRegistry, YggdrasilServer};
use crate::core::error::{Error, Result};

pub fn find_injector(data_root: &Path) -> Option<PathBuf> {
    let path = data_root.join("authlib-injector.jar");
    path.is_file().then_some(path)
}

pub struct LocalAuthlibService {
    pub jvm_args: Vec<String>,
    server: YggdrasilServer,
}

impl LocalAuthlibService {
    pub async fn start(data_root: &Path, account: &Account) -> Result<Option<Self>> {
        let Some(jar) = find_injector(data_root) else {
            return Ok(None);
        };
        let registry = YggdrasilRegistry::default();
        registry.register(PlayerProfile {
            uuid: account.uuid,
            name: account.username.clone(),
            properties: Vec::new(),
        });
        let server = YggdrasilServer::bind(registry, SkinStore::default()).await?;
        let jvm_args = agent_args(Some(&jar), &server.endpoint, account)?;
        Ok(Some(Self { jvm_args, server }))
    }

    pub async fn stop(self) {
        self.server.stop().await;
    }
}

pub fn agent_args(
    jar: Option<&Path>,
    yggdrasil_url: &str,
    account: &Account,
) -> Result<Vec<String>> {
    let Some(jar) = jar else {
        return Ok(Vec::new());
    };
    if !jar.is_file() {
        return Err(Error::Account("Authlib-Injector JAR does not exist".into()));
    }
    validate_loopback_url(yggdrasil_url)?;
    let prefetched = serde_json::json!({
        "uuid": account.uuid.simple().to_string(),
        "name": account.username,
    });
    Ok(vec![
        format!("-javaagent:{}={yggdrasil_url}", jar.display()),
        format!(
            "-Dauthlibinjector.yggdrasil.prefetched={}",
            serde_json::to_string(&prefetched)?
        ),
    ])
}

pub fn validate_loopback_url(value: &str) -> Result<()> {
    let url =
        reqwest::Url::parse(value).map_err(|_| Error::Account("invalid Yggdrasil URL".into()))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::Account("Yggdrasil URL has no host".into()))?;
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return Err(Error::Account(
            "Authlib-Injector URL must point to loopback".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{agent_args, validate_loopback_url};
    use crate::core::account::offline::create_account;

    #[test]
    fn generates_agent_without_secret_data() {
        let root = std::env::temp_dir().join(format!(
            "tmcl-authlib-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create test directory");
        let jar = root.join("authlib-injector.jar");
        std::fs::write(&jar, b"jar").expect("write test jar");
        let account = create_account("Alex", None).expect("offline account");
        let args =
            agent_args(Some(&jar), "http://127.0.0.1:12345", &account).expect("agent arguments");
        assert!(args[0].contains("-javaagent:"));
        assert!(args[1].contains("prefetched"));
        assert!(!args.iter().any(|arg| arg.contains("refresh")));
        assert!(
            agent_args(None, "http://127.0.0.1:12345", &account)
                .expect("basic offline arguments")
                .is_empty()
        );
        std::fs::remove_dir_all(root).expect("clean test directory");
    }

    #[test]
    fn rejects_non_loopback_url() {
        assert!(validate_loopback_url("https://example.com").is_err());
        assert!(validate_loopback_url("http://127.0.0.1:12345").is_ok());
    }
}
