fn main() {
    println!("cargo:rerun-if-env-changed=TMCL_CURSEFORGE_API_KEY");
    if let Ok(key) = std::env::var("TMCL_CURSEFORGE_API_KEY") {
        if !key.trim().is_empty() {
            println!("cargo:rustc-env=TMCL_CURSEFORGE_API_KEY={key}");
        }
    }
}
