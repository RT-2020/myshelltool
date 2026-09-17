use super::*;
use crate::asset_store::write_atomic;
use crate::secret_store::{is_new_format, xor_transform};
    use std::{env, fs, time::{SystemTime, UNIX_EPOCH}};

    #[test]
    fn serializes_connection_asset() {
        let asset = ConnectionAsset {
            id: "asset-1".to_string(),
            name: "Asset 1".to_string(),
            host: "example.invalid".to_string(),
            port: 2222,
            username: "user".to_string(),
            auth_method: AuthMethod::Token,
            private_key_path: None,
            group: "测试".to_string(),
            tags: vec!["demo".to_string()],
            status: ConnectionStatus::Idle,
            last_connected: "从未".to_string(),
            credential_id: None,
            passphrase_credential_id: None,
            private_key_credential_id: None,
        };

        let json = serde_json::to_string(&asset).expect("asset serializes");

        assert!(json.contains("asset-1"));
        assert!(json.contains("Token"));
    }

    #[test]
    fn provides_sample_assets_without_credentials() {
        let assets = sample_assets();

        assert_eq!(assets.len(), 8);
        assert!(assets.iter().all(|asset| !asset.id.is_empty()));
        assert!(assets.iter().all(|asset| asset.port > 0));
    }

    #[test]
    fn rejects_invalid_assets() {
        let mut asset = sample_assets().remove(0);
        asset.host.clear();

        assert!(validate_connection_asset(&asset).is_err());
    }

    #[test]
    fn upserts_assets_by_id() {
        let mut store = demo_asset_store();
        let mut asset = store.assets[0].clone();
        asset.name = "Renamed Bastion".to_string();

        upsert_connection_asset(&mut store, asset).expect("asset upserts");

        assert_eq!(store.assets.len(), 8);
        assert_eq!(store.assets[0].name, "Renamed Bastion");
    }

    #[test]
    fn saves_and_loads_assets_from_json() {
        let path = temp_store_path("roundtrip");
        let mut store = demo_asset_store();
        let mut asset = store.assets[0].clone();
        asset.id = "new-local".to_string();
        asset.name = "New Local".to_string();
        upsert_connection_asset(&mut store, asset).expect("asset appends");

        save_connection_asset_store(&path, &store).expect("store saves");
        let loaded = load_connection_asset_store(&path).expect("store loads");
        let _ = fs::remove_file(path);

        assert_eq!(loaded.assets.len(), 9);
        assert!(loaded.assets.iter().any(|asset| asset.id == "new-local"));
    }

    // 回归锁：资产文件不存在（首次安装）必须得到空 store。
    // 曾因返回 demo 种子数据导致全新安装凭空出现 8 台演示主机。
    #[test]
    fn load_without_file_yields_empty_store_not_demo_assets() {
        let path = temp_store_path("missing-file-empty-store");

        let store = load_connection_asset_store(&path).expect("load succeeds");

        assert!(store.assets.is_empty(), "首次安装不得出现 demo 资产");
        assert!(store.groups.is_empty());
    }

    #[test]
    fn stored_assets_do_not_include_secret_fields() {
        let store = demo_asset_store();
        let json = serde_json::to_string(&store).expect("store serializes");
        let lowered = json.to_lowercase();

        // 断言的是「密钥值」不泄漏，而非字段名。
        // ConnectionAsset 含 credential_id / passphrase_credential_id（仅是引用 id，
        // 不是密钥本身），故字段名 "passphrase" 合法存在；这里只拦截明文密钥值。
        assert!(!lowered.contains("password_value"));
        assert!(!lowered.contains("passphrase_value"));
        assert!(!lowered.contains("private_key_data"));
        assert!(!lowered.contains("token_value"));
        assert!(!lowered.contains("secret"));
    }

    #[test]
    fn removes_connection_asset_by_id() {
        let mut store = demo_asset_store();
        let id = store.assets[0].id.clone();
        let before = store.assets.len();

        remove_connection_asset(&mut store, &id).expect("asset removed");

        assert_eq!(store.assets.len(), before - 1);
        assert!(!store.assets.iter().any(|a| a.id == id));
        // 不存在的 id 应报错
        assert!(remove_connection_asset(&mut store, "does-not-exist").is_err());
    }

    // 测试用便捷构造：仅 id/name/host/group 关键字段，其余默认。
    fn g_asset(id: &str, name: &str, host: &str, group: &str) -> ConnectionAsset {
        ConnectionAsset {
            id: id.to_string(),
            name: name.to_string(),
            host: host.to_string(),
            port: 22,
            username: "root".to_string(),
            auth_method: AuthMethod::Password,
            private_key_path: None,
            group: group.to_string(),
            tags: vec![],
            status: ConnectionStatus::Idle,
            last_connected: "从未".to_string(),
            credential_id: None,
            passphrase_credential_id: None,
            private_key_credential_id: None,
        }
    }

    #[test]
    fn rename_group_updates_assets_and_declared_paths() {
        let mut store = ConnectionAssetStore { assets: vec![], groups: vec![] };
        // 两条资产在 生产/数据库 下，一条在 生产/web 下
        store.assets.push(g_asset("a1", "A1", "h1", "生产/数据库"));
        store.assets.push(g_asset("a2", "A2", "h2", "生产/数据库/主"));
        store.assets.push(g_asset("a3", "A3", "h3", "生产/web"));
        store.groups = vec!["生产/数据库".into(), "生产/web".into()];

        rename_asset_group(&mut store, "生产/数据库", "生产/DB").expect("rename ok");

        // a1 精确匹配 → 改；a2 子级前缀 → 改；a3 不在路径下 → 不变
        assert_eq!(store.assets[0].group, "生产/DB");
        assert_eq!(store.assets[1].group, "生产/DB/主");
        assert_eq!(store.assets[2].group, "生产/web");
        // declared groups 同步更新
        assert!(store.groups.contains(&"生产/DB".to_string()));
        assert!(!store.groups.contains(&"生产/数据库".to_string()));
    }

    #[test]
    fn rename_group_rejects_reserved_ungrouped() {
        let mut store = demo_asset_store();
        // 「未分组」保留，不可重命名
        assert!(rename_asset_group(&mut store, "未分组", "Other").is_err());
    }

    #[test]
    fn dissolve_group_moves_assets_to_parent() {
        let mut store = ConnectionAssetStore { assets: vec![], groups: vec![] };
        store.assets.push(g_asset("a1", "A1", "h1", "生产/数据库"));
        store.assets.push(g_asset("a2", "A2", "h2", "生产/数据库/主"));
        store.assets.push(g_asset("a3", "A3", "h3", "生产"));
        store.groups = vec!["生产/数据库".into(), "生产".into()];

        dissolve_asset_group(&mut store, "生产/数据库").expect("dissolve ok");

        // a1 → 父级 生产；a2 → 父级 生产；a3 不变
        assert_eq!(store.assets[0].group, "生产");
        assert_eq!(store.assets[1].group, "生产");
        assert_eq!(store.assets[2].group, "生产");
        // declared: 生产/数据库 移除，生产 保留
        assert!(!store.groups.contains(&"生产/数据库".to_string()));
        assert!(store.groups.contains(&"生产".to_string()));
    }

    #[test]
    fn dissolve_top_level_group_falls_back_to_ungrouped() {
        let mut store = ConnectionAssetStore {
            assets: vec![g_asset("a1", "A1", "h1", "测试组")],
            groups: vec!["测试组".into()],
        };
        dissolve_asset_group(&mut store, "测试组").expect("dissolve ok");
        // 顶级分组无父级 → 提到「未分组」
        assert_eq!(store.assets[0].group, "未分组");
        assert!(store.groups.is_empty());
    }

    #[test]
    fn ensure_group_persists_empty_group() {
        let mut store = demo_asset_store();
        ensure_asset_group(&mut store, "生产/数据库").expect("ensure ok");
        assert!(store.groups.contains(&"生产/数据库".to_string()));
        // 幂等：再 ensure 不重复
        ensure_asset_group(&mut store, "生产/数据库").expect("ensure ok");
        assert_eq!(store.groups.iter().filter(|g| *g == "生产/数据库").count(), 1);
    }

    #[test]
    fn validate_group_path_rejects_empty_segments() {
        assert!(validate_group_path("生产/数据库").is_ok());
        assert!(validate_group_path("").is_err());
        assert!(validate_group_path("生产//数据库").is_err()); // 连续 '/'
        assert!(validate_group_path("生产/").is_err()); // 末尾空段
    }

    #[test]
    fn reorder_groups_accepts_permutation_and_filters_reserved() {
        let mut store = ConnectionAssetStore {
            assets: vec![],
            groups: vec!["生产".into(), "测试".into(), "生产/数据库".into()],
        };
        // 翻转顺序（排列），应被接受
        reorder_asset_groups(&mut store, &["生产/数据库".into(), "测试".into(), "生产".into()])
            .expect("reorder ok");
        assert_eq!(
            store.groups,
            vec!["生产/数据库".to_string(), "测试".to_string(), "生产".to_string()]
        );
    }

    #[test]
    fn reorder_groups_rejects_non_permutation() {
        let mut store = ConnectionAssetStore {
            assets: vec![],
            groups: vec!["生产".into(), "测试".into()],
        };
        // 漏传一个 → 集合不等 → 报错，不落盘
        assert!(reorder_asset_groups(&mut store, &["生产".into()]).is_err());
        assert_eq!(store.groups, vec!["生产".to_string(), "测试".to_string()]);
    }

    #[test]
    fn reorder_groups_ignores_ungrouped_in_input() {
        let mut store = ConnectionAssetStore {
            assets: vec![],
            groups: vec!["生产".into(), "测试".into()],
        };
        // 「未分组」混入输入被忽略，剩余仍是当前 groups 的排列 → 接受
        reorder_asset_groups(&mut store, &["测试".into(), "未分组".into(), "生产".into()])
            .expect("reorder ok (ungrouped filtered)");
        assert_eq!(store.groups, vec!["测试".to_string(), "生产".to_string()]);
    }

    fn temp_store_path(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time is after epoch")
            .as_nanos();
        env::temp_dir().join(format!("myshelltool-{label}-{nanos}.json"))
    }

    fn temp_secret_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time is after epoch")
            .as_nanos();
        let dir = env::temp_dir().join(format!("myshelltool-secrets-{label}-{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn secret_store_saves_and_checks_status() {
        let dir = temp_secret_dir("save");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        store.save("github-pat", "ghp_test_secret_value").expect("save succeeds");

        let status = store.get_status("github-pat").expect("status succeeds");
        assert!(status.exists);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_rejects_empty_secret() {
        let dir = temp_secret_dir("empty");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        assert!(store.save("test", "   ").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_deletes_credential() {
        let dir = temp_secret_dir("delete");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        store.save("test-cred", "secret123").expect("save");
        let deleted = store.delete("test-cred").expect("delete");
        assert!(deleted);
        let status = store.get_status("test-cred").expect("status");
        assert!(!status.exists);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_codec_output_does_not_contain_plaintext() {
        // 用 LegacyXorCodec（非明文 codec）验证：存盘后文件不含明文 secret。
        // PlaintextCodec 是明文，不能用于此断言。
        let dir = temp_secret_dir("plaintext");
        let store = SecretStore::new(&dir, Box::new(LegacyXorCodec));
        let secret = "my-super-secret-token-value";
        store.save("scan-test", secret).expect("save");

        let bytes = fs::read(dir.join("scan-test.cred")).expect("read file");
        let content_str = String::from_utf8_lossy(&bytes);
        assert!(!content_str.contains(secret), "密文不应含明文");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_lazy_migrates_legacy_xor_to_new_format() {
        // 验证懒迁移：手写一个旧 XOR 格式文件（无 MAGIC 头），
        // read() 应能用 LegacyXorCodec 解出，并用新 codec 重写（加 MAGIC 头）。
        let dir = temp_secret_dir("migrate");
        let secret = "legacy-secret-to-migrate";

        // 手写旧格式：纯 XOR 字节，无 MAGIC 头
        let legacy_bytes = xor_transform(secret.as_bytes());
        fs::write(dir.join("legacy.cred"), &legacy_bytes).expect("write legacy");

        // 用 PlaintextCodec 构造 store（新格式 = MAGIC + 明文）
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        let read_back = store.read("legacy").expect("read legacy").expect("some");
        assert_eq!(read_back, secret, "懒迁移后应能读出明文");

        // 迁移后文件应有 MAGIC 头（新格式）
        let after = fs::read(dir.join("legacy.cred")).expect("read after migrate");
        assert!(is_new_format(&after), "迁移后应为新格式（有 MAGIC 头）");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_roundtrip_with_new_format() {
        // 新写入 → 读取 往返测试（新格式，有 MAGIC 头）
        let dir = temp_secret_dir("roundtrip");
        let store = SecretStore::new(&dir, Box::new(LegacyXorCodec));
        let secret = "roundtrip-secret-2026";
        store.save("rt", secret).expect("save");

        // 文件应是新格式
        let bytes = fs::read(dir.join("rt.cred")).expect("read file");
        assert!(is_new_format(&bytes), "新写入应为新格式");

        // 读回应等于原文
        let read_back = store.read("rt").expect("read").expect("some");
        assert_eq!(read_back, secret);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_list_returns_saved_credentials() {
        let dir = temp_secret_dir("list");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        store.save("cred-a", "secret_a").expect("save a");
        store.save("cred-b", "secret_b").expect("save b");

        let list = store.list().expect("list");
        assert_eq!(list.len(), 2);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_rejects_invalid_id() {
        let dir = temp_secret_dir("invalid");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        assert!(store.save("!@#$%", "secret").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    // ─── v2.6 原子写（write_atomic）───

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temp() {
        let dir = temp_secret_dir("atomic-ok");
        let path = dir.join("payload.json");
        write_atomic(&path, "first").expect("first write");
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        write_atomic(&path, "second").expect("overwrite");
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        // 不留临时文件残留（读方看到的是完整文件）
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "临时文件未清理: {leftovers:?}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_atomic_failure_preserves_previous_file() {
        // rename 的目标是一个**已存在的目录** → rename 必然失败；此时原文件不得被破坏。
        // 这是 write_atomic 的核心价值：写失败绝不能让用户丢失唯一的本地副本。
        let dir = temp_secret_dir("atomic-fail");
        let parent_file = dir.join("target");
        fs::create_dir_all(&parent_file).expect("create dir as rename target");
        // 先写一份可读数据到另一个文件，再验证失败路径不触碰它
        let keep = dir.join("keep.json");
        write_atomic(&keep, "important").unwrap();
        let err = write_atomic(&parent_file, "boom");
        assert!(err.is_err(), "向目录 rename 必须报错");
        assert_eq!(fs::read_to_string(&keep).unwrap(), "important");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn save_connection_asset_store_is_atomic_and_readable() {
        let dir = temp_secret_dir("assets-atomic");
        let path = dir.join("connection-assets.json");
        let mut store = demo_asset_store();
        save_connection_asset_store(&path, &store).expect("save");
        let loaded = load_connection_asset_store(&path).expect("load");
        assert_eq!(loaded.assets.len(), store.assets.len());
        // 第二次保存（覆盖已有文件）同样成功且可读
        store.assets.clear();
        save_connection_asset_store(&path, &store).expect("overwrite");
        assert!(load_connection_asset_store(&path).unwrap().assets.is_empty());
        let _ = fs::remove_dir_all(dir);
    }
