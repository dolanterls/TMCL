use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use sha1::{Digest, Sha1};

#[derive(Debug, Clone)]
pub struct Texture {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

#[derive(Clone, Default)]
pub struct SkinStore {
    textures: Arc<RwLock<HashMap<String, Texture>>>,
}

impl SkinStore {
    pub fn register(&self, bytes: Vec<u8>, content_type: impl Into<String>) -> String {
        let hash = format!("{:x}", Sha1::digest(&bytes));
        self.textures.write().expect("skin store lock").insert(
            hash.clone(),
            Texture {
                bytes,
                content_type: content_type.into(),
            },
        );
        hash
    }

    pub fn get(&self, hash: &str) -> Option<Texture> {
        self.textures
            .read()
            .expect("skin store lock")
            .get(hash)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::SkinStore;

    #[test]
    fn only_registered_texture_hashes_are_available() {
        let store = SkinStore::default();
        let hash = store.register(b"skin".to_vec(), "image/png");
        assert_eq!(store.get(&hash).expect("registered texture").bytes, b"skin");
        assert!(store.get("not-registered").is_none());
    }
}
