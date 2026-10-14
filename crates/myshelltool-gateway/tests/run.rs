//! run.rs 编排层测试：导入（解密→落位→写盘）、装载（缺秘密 fail-closed 指引）、
//! VPS 公钥探测。SecretStore 用 PlaintextCodec（core 提供），全链无 DPAPI 依赖。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use myshelltool_core::{PlaintextCodec, SecretStore};

use myshelltool_gateway::config::{self, GatewayBundle, GatewayServerConfig, HostKeyPin, MountConfig, VpsEndpoint};
use myshelltool_gateway::keys;
use myshelltool_gateway::run::{import_bundle, load_runtime, probe_vps_public_key};
use myshelltool_gateway::server::GatewayServerState;

// ─── 基件 ───

static SEQ: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "mst-gw-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).expect("mkdir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0); // best-effort 清理
    }
}

fn secret_store(dir: &TempDir) -> SecretStore {
    SecretStore::new(dir.0.join("credentials"), Box::new(PlaintextCodec))
}

fn make_bundle() -> (GatewayBundle, String /*password*/) {
    let (mount_pem, mount_pub) = keys::generate_ed25519().expect("mount key");
    let (host_pem, host_pub) = keys::generate_ed25519().expect("host key");
    let (_, client_pub) = keys::generate_ed25519().expect("client key");
    let pin = HostKeyPin {
        key_type: "ssh-ed25519".into(),
        key_hex: keys::public_key_hex(&keys::parse_public_key_line(&host_pub).unwrap()),
    };
    (
        GatewayBundle {
            version: config::BUNDLE_VERSION,
            vps: VpsEndpoint {
                host: "203.0.113.10".into(),
                port: 22222,
                username: "mount".into(),
                private_key: mount_pem,
                passphrase: None,
                host_key_pin: pin,
            },
            mount: MountConfig { bind_addr: "0.0.0.0".into(), remote_port: 2222 },
            gateway: GatewayServerConfig {
                listen_port: 22222,
                authorized_key: client_pub,
                allowed_targets: vec![],
            },
            host_key_private: host_pem,
        },
        "bundle-pass-123".into(),
    )
}

// ─── 导入与装载 ───

#[tokio::test]
async fn import_then_load_roundtrip() {
    let dir = TempDir::new("import");
    let store = secret_store(&dir);
    let (bundle, password) = make_bundle();

    let bundle_path = dir.0.join("bundle.json");
    std::fs::write(&bundle_path, config::export_bundle(&bundle, &password).unwrap()).unwrap();

    let rt = import_bundle(&bundle_path, &password, &store, &dir.0).expect("import");
    assert_eq!(rt.vps_host, "203.0.113.10");
    assert_eq!(rt.mount.remote_port, 2222);

    let loaded = load_runtime(&dir.0, &store).expect("load");
    // 秘密从 SecretStore 读出且与 bundle 一致
    assert_eq!(loaded.host_key_private, bundle.host_key_private);
    assert_eq!(loaded.mount_params.private_key_pem, bundle.vps.private_key);
    assert_eq!(loaded.mount_params.vps_host, "203.0.113.10");
    assert_eq!(loaded.mount_params.gateway_listen_port, 22222);
    assert_eq!(loaded.authorized_key_line, bundle.gateway.authorized_key);

    // 运行配置文件零秘密
    let on_disk = std::fs::read_to_string(dir.0.join("gateway-config.json")).unwrap();
    assert!(!on_disk.contains("PRIVATE KEY"), "gateway-config.json 不得含私钥");
}

#[test]
fn import_wrong_password_fails_and_keeps_old_runtime() {
    let dir = TempDir::new("wrongpw");
    let store = secret_store(&dir);
    let (bundle, password) = make_bundle();
    let bundle_path = dir.0.join("bundle.json");
    std::fs::write(&bundle_path, config::export_bundle(&bundle, &password).unwrap()).unwrap();

    // 先成功导入一次，再拿错口令导入第二次：必须失败且旧运行配置保留
    import_bundle(&bundle_path, &password, &store, &dir.0).expect("first import");
    let before = std::fs::read_to_string(dir.0.join("gateway-config.json")).unwrap();
    assert!(import_bundle(&bundle_path, "wrong", &store, &dir.0).is_err());
    let after = std::fs::read_to_string(dir.0.join("gateway-config.json")).unwrap();
    assert_eq!(before, after, "导入失败必须保留旧运行配置（不产生半配置态）");
}

#[test]
fn load_without_import_fails_with_hint() {
    let dir = TempDir::new("noimport");
    let store = secret_store(&dir);
    let err = load_runtime(&dir.0, &store).expect_err("未导入必须失败");
    assert!(err.contains("尚未导入"), "文案应指向未导入：{err}");
}

#[test]
fn load_with_missing_secret_fails_with_dpapi_hint() {
    let dir = TempDir::new("nosecret");
    let store = secret_store(&dir);
    let (bundle, password) = make_bundle();
    let bundle_path = dir.0.join("bundle.json");
    std::fs::write(&bundle_path, config::export_bundle(&bundle, &password).unwrap()).unwrap();
    import_bundle(&bundle_path, &password, &store, &dir.0).expect("import");

    // 模拟 DPAPI 用户不一致：秘密读不出来（删掉秘密项）
    myshelltool_gateway::config::SECRET_VPS_KEY;
    use myshelltool_gateway::config::{SECRET_HOST_KEY, SECRET_VPS_KEY};
    let _ = store.delete(SECRET_HOST_KEY);
    let _ = store.delete(SECRET_VPS_KEY);

    let err = load_runtime(&dir.0, &store).expect_err("缺秘密必须失败");
    assert!(
        err.contains("重新导入") && err.contains("同一 Windows 用户"),
        "文案必须指路 DPAPI 用户一致性：{err}"
    );
}

// ─── probe（pin 采集） ───

#[tokio::test]
async fn probe_captures_host_key_before_auth() {
    // 拿网关 server 当被探测对象（任意 SSH 服务端皆可）
    let (host_pem, host_pub) = keys::generate_ed25519().expect("host key");
    let expected_hex = keys::public_key_hex(&keys::parse_public_key_line(&host_pub).unwrap());
    let state = GatewayServerState::new(&host_pem, &host_pub, &[]).expect("state");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let _task = state.spawn_serving(listener);

    let (key_type, key_hex) =
        probe_vps_public_key("127.0.0.1", port, Duration::from_secs(10)).await.expect("probe");
    assert_eq!(key_hex, expected_hex, "probe 必须拿到 host key 公钥字节");
    assert_eq!(key_type, "ssh-ed25519");
}

#[tokio::test]
async fn probe_unreachable_fails_with_connectivity_hint() {
    // 端口几乎必然无人监听（绑定后立即释放）
    let probe_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe_listener.local_addr().unwrap().port();
    drop(probe_listener);

    let err = probe_vps_public_key("127.0.0.1", port, Duration::from_secs(5))
        .await
        .expect_err("不可达必须失败");
    assert!(err.contains("探测"), "文案应指向探测失败：{err}");
}
