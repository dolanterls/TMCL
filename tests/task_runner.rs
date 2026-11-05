use std::path::PathBuf;

use tmcl::core::loader::service::InstallLoader;
use tmcl::core::task::LoaderSpec;
use tmcl::core::task::runner::install_loader;

#[test]
fn loader_spec_maps_to_the_existing_install_request_type() {
    assert_eq!(install_loader(&LoaderSpec::Vanilla), InstallLoader::Vanilla);
    assert_eq!(
        install_loader(&LoaderSpec::Fabric {
            version: "0.16.10".into()
        }),
        InstallLoader::Fabric {
            version: "0.16.10".into()
        }
    );
    assert_eq!(
        install_loader(&LoaderSpec::Forge {
            version: "47.3.0".into()
        }),
        InstallLoader::Forge {
            version: "47.3.0".into()
        }
    );
    assert_eq!(
        install_loader(&LoaderSpec::NeoForge {
            version: "21.1.247".into()
        }),
        InstallLoader::NeoForge {
            version: "21.1.247".into()
        }
    );
}

#[test]
fn loader_operation_data_keeps_paths_outside_the_runner_adapter() {
    let spec = tmcl::core::task::InstanceInstallSpec {
        instance_id: uuid::Uuid::new_v4(),
        game_directory: PathBuf::from("game"),
        instance_directory: PathBuf::from("game/instances/demo"),
        instance_name: "demo".into(),
        minecraft_version: "1.21.1".into(),
        loader: LoaderSpec::NeoForge {
            version: "21.1.247".into(),
        },
        java_executable: Some(PathBuf::from("java/bin/java.exe")),
        download_snapshot: Default::default(),
    };
    assert_eq!(spec.game_directory, PathBuf::from("game"));
    assert_eq!(
        spec.instance_directory,
        PathBuf::from("game/instances/demo")
    );
    assert_eq!(
        spec.java_executable,
        Some(PathBuf::from("java/bin/java.exe"))
    );
}
