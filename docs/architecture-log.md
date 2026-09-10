# Architecture Log

> Cross-session memory for architectural drift. AI has no memory across sessions 鈥?this file is the sole persistent carrier.
> Read at session start to detect accumulated drift; append one row per session that touched code.
> Maintained by: `vibe-guard-pure` skill.

## How to use

- **At session start (when working on code)**: skim this log. If any file has grown by 鈮?.5脳 of its limit since the last reset row, that's a downstream trigger 鈥?propose a pure architecture session.
- **On every ARCH-CHECK report (medium/high tier)**: append one row to the table below **immediately after the report is produced** 鈥?do not wait for session end (session boundaries are undefined in real AI harnesses and cause rows to be silently dropped). Low-tier changes produce no report and therefore no row.
- **On a pure architecture session**: after restructuring, add a `RESET` row recording the new baseline.

## Size limits (from AGENTS.md 搂璐ㄩ噺绾㈢嚎)

| Type | Soft warn | Hard limit |
|---|---|---|
| Vue SFC `.vue` | 300 | **500** |
| Pinia store `.js` | 300 | **500** |
| Rust module `.rs` | 400 | **800** |

## Drift log

> ⚠️ 2026-09-10 编码说明：本表 2026-06-20 ~ 2026-07-02 早期行与下方 Baseline snapshot 段存在 mojibake（历史保存时 UTF-8 字节被按 GBK 误解，如 `鈥?` 应为 `—`、`脳` 应为 `×`、`搂` 应为 `§`）。已验证 GBK 逆变换存在不可逆字节丢失（`X?` 模式连后续数字/空格一并丢失，且 `。`/`、` 等有歧义），为不臆造历史内容，乱码文本原样保留。表格结构、日期、文件名与数字仍可读。

| Date | Files touched (current size, delta) | Suspected duplicates | Tier | Outcome |
|---|---|---|---|---|
| 2026-06-20 | baseline snapshot (see below) | 鈥?| 鈥?| log initialized post v0.3.0 |
| 2026-06-22 | re-baseline via `wc -l` (see below) | 鈥?| 鈥?| **RESET** 鈥?skill re-init, 6 files still over hard limit (down from 8) |
| 2026-06-22 | `FileColumn.vue` 780 (鈭?0 from 820瀹炴祴), `files.js` 664 (+51), `workbench.js` 431 (+36), `ui.js` 344 (+1); 鍒?`AppBreadcrumb.vue` + barrel 琛?| none (manualLocal* 瀵圭О澶嶅埗 manualRemote*, Gate A/B pass 涓嶆娊鍏叡) | medium | passed 鈥?`npm run build` exit 0 (12.66s). FileColumn 杩滅纭笂闄愶紱files/workbench 浠?1.33脳/0.86脳锛屾棤鏂扮儹鐐?|
| 2026-06-22 | `TerminalSurface.vue` 323 (+13, 杩涘叆 soft-warn), `TerminalPane.vue` 217 (+9), `useClipboard.js` 68 (+16), `workbench.js` 432 (+1 vs 涓婁竴琛?; 鍔?`tauri-plugin-clipboard-manager` (Cargo+capabilities+lib.rs); `useTerminalConfig.js` 鍒?rightClickSelectsWord | none (鍙抽敭鑿滃崟绾帴绾垮鐢?copyTerminalSelection/pasteToTerminal/handlePasteWithGuard, Gate 涓嶈Е鍙? | medium | passed 鈥?`npm run build` exit 0 (3.80s) + `cargo check` exit 0 (1m42s, 娓?42 涓兊灏?MCP 杩涚▼鍚? + `test:core` 41/41 pass |
| 2026-06-23 | 鏂囨。鍚屾閲嶅啓锛坴1.4 钀藉湴锛夛細README/mcp-setup/AGENTS/architecture-log銆?*鍙敼鏂囨。 + 鐗堟湰鍙?bump 0.3.0鈫?.4.0锛岄浂浠ｇ爜閫昏緫鏀瑰姩**銆傚疄娴嬪彂鐜?`FileColumn.vue` 768鈫?123锛?355, **cycle-tier 瑙﹀彂**, 1脳鈫?.25脳锛夛紝`ssh.rs` 1874鈫?097銆傛竻 2 涓?v1.2 閬楃暀 `myshelltool-mcp.exe` 鍍靛案杩涚▼锛堥攣 WebView2Loader.dll 鑷?os error 32锛夈€俙cargo check` exit 0 (17.50s) | none | low | passed 鈥?瑙佷笅鏂?RESET 鍩虹嚎閲嶇疆銆?*涓嬫绾灦鏋勪細璇濓細鎷?FileColumn.vue + ssh.rs锛坈ycle-tier 鍙岃Е鍙戯級** |
| 2026-06-26 | v1.5 鏂规 A锛歁CP 楂樺嵄宸ュ叿 GUI 寮圭獥瀹℃壒闄嶇骇锛堣ˉ v1.4 follow-up锛夈€? 鏂囦欢锛歚tools.rs` 369鈫?14(+45), `approval.rs` 258鈫?09(+51), `server.rs` 340鈫?03(+63), `lib.rs` 470鈫?01(+31, **韪?.rs soft-warn 400**), `mcp.js` 115鈫?35(+120, store 浠庣函鏌ヨ鍗囩骇涓哄甫鐩戝惉/dispose), `workbench.js` 429鈫?44(+15), `GlobalModals.vue` 660鈫?16(+56, **1.43脳 hard 500锛屾媶鍒嗗€欓€?*)銆傚悗绔鐢?ssh.rs pending-oneshot **妯″紡**锛堥潪 import锛孏ate A/B pass锛夈€?| none锛堝鎵归檷绾ф槸鍗忚灞傝亴璐ｏ紝鐓?ssh.rs:55-153 鐘舵€佹満妯″紡鍦?mcp/ 鍐呮柊寤虹瓑浠风墿锛岄潪璺ㄥ眰鑰﹀悎锛?| medium | passed 鈥?`cargo check` exit 0 (1m20s, 1 warning `McpToolContext::new` 鏈敤 鈫?`#[allow(dead_code)]` 鏍囨敞涓?headless/娴嬭瘯淇濈暀), `npm run build` exit 0 (12.44s, 1676 modules), `test:core` 41/41 pass銆?*GlobalModals.vue 716 搴旂撼鍏ヤ笅娆℃媶鍒嗙洰鏍囷紙涓?FileColumn.vue/ssh.rs 骞跺垪锛?* |
| 2026-06-29 | 鍒嗙粍绠＄悊涓変欢濂楋細璧勪骇鎷栨嫿绉诲姩/鍒嗙粍鎷栨嫿鎺掑簭 + 璧勪骇缂栬緫鍣ㄥ垎缁勫瓧娈垫敼 datalist + 鍒?PASSWORD badge銆? 鏂囦欢锛歚ConnectionSidebar.vue` 711鈫?42(+131, **1.42脳鈫?.68脳锛?drag mixed" 杩涗竴姝ュ潗瀹炴媶鍒嗗€欓€?*), `AssetGroupNode.vue` 332鈫?46(+14), `GlobalModals.vue` 728(瀹炴祴, moveGroupOptions鈫抔roupOptions 鍏辩敤 + 璧勪骇缂栬緫鍣ㄥ垎缁勬崲 input+datalist), `assets.js` ~310鈫?47(+37 reorderGroups), `workbench.js` 445(+1 re-export), `App.vue` +3 handler, `lib.rs` 501鈫?18(+17 reorder_asset_groups 鍛戒护+娉ㄥ唽), `core/lib.rs` 957(+~45 reorder_asset_groups + 3 娴嬭瘯)銆?*ConnectionSidebar.vue 宸叉槸宸茬煡 over-limit 鍊欓€?*锛坆aseline 娉ㄦ槑 "tree + menu + drag mixed"锛夛紝鎷栨嫿閫昏緫鏈睘姝ょ粍浠惰亴璐ｅ唴鑱氱偣锛屾湭鏂板紩鍏ヨ法灞傝€﹀悎銆?| none锛堟嫋鎷借蛋 emit鈫扐pp.vue鈫抯tore锛屼繚鎸?store-agnostic锛沵oveAsset/reorderGroups 澶嶇敤鏃㈡湁 id-upsert 妯″紡锛?| medium | passed 鈥?`npm run build` exit 0 (5.72s, 1676 modules), `cargo check` exit 0 (26.39s), `test:core` 44/44 pass锛堝惈鏂板 3 涓?reorder 娴嬭瘯锛? UI smoke + host-key passed銆?|
| 2026-06-29 | v1.6 Gist 鍚屾闈㈡澘閲嶈璁?+ PAT 鑾峰彇寮曞銆? 鏂囦欢锛歚SyncPanelContent.vue` 358鈫?75(+117, 0.72脳鈫?*0.95脳 hard 500锛宻oft-warn 鐩戣**), **鏂板 `SyncPatGuide.vue` 206**锛? 姝ュ紩瀵?鍔犲瘑璇存槑瀛愮粍浠讹紝0.41脳锛? `package.json` +`@tauri-apps/plugin-opener`, `Cargo.toml` +`tauri-plugin-opener`, `lib.rs` +1 琛?`.plugin(tauri_plugin_opener::init())`, `capabilities/default.json` +`opener:default`銆?*棣栨韪?500 纭笂闄愯Е鍙戞媶鍒?*锛氬垵鐗堥噸鍐欒揪 620 琛岋紝绔嬪嵆鎸?plan 鎵胯鎷?PAT 寮曞涓?SyncPatGuide锛堝弬鐓?McpPanelContent鈫扢cpCapabilityList 鍏堜緥锛夈€?| none锛坔ero/block-head/detail-grid/field 鑼冨紡鐓ф惉 McpPanelContent 鍚屽煙鍚屽眰澶嶇敤锛岄潪璺ㄥ眰鑰﹀悎锛沷penExternal 鐓?useClipboard銆屽姩鎬?import+runtime 妫€娴嬨€嶈寖寮忥紱store.saveToken/syncSetup 绛?action 鍏ㄩ儴澶嶇敤 re-export锛?| medium | passed 鈥?`npm run build` exit 0 (5.42s, 1679 modules), `cargo check` exit 0 (25.73s, tauri-plugin-opener v2.5.4 姝ｇ‘閾炬帴), `test:core` 44/44 pass銆傛竻 1 涓仐鐣?myshelltool.exe(8776) 瑙ｉ攣 os error 32銆?|
| 2026-06-29 | **v1.6 鑷姩鍚屾锛堜細璇濆瘑閽?DPAPI锛? 鍚姩杩滅鎺㈡祴**銆俬igh-tier锛氭柊澧炲姞瑙ｅ瘑鎶借薄灞傘€?1 鏂囦欢锛歚crypto.rs` +3 鍑芥暟(derive_session_key/encrypt_with_key/decrypt_with_key)+7 娴嬭瘯, `core/sync.rs` SyncState +auto_sync_enabled 瀛楁, `src-tauri/sync.rs` 699鈫拁830(+~130, **韪?.rs soft-warn**, +3 鍛戒护 enable/disable_auto_sync + check_remote_updates + 鏀归€?push/pull/resolve_conflict/reset/clear 璧颁細璇濆瘑閽ヨ矾寰?+ SessionKey helper + b64 宸ュ叿), `lib.rs` 518鈫?21(+3 鍛戒护娉ㄥ唽), `sync.js` 223鈫?39(+116, +autoSyncEnabled/remoteHasUpdates 鐘舵€?+ 4 action + attachWorkbench), `assets.js` 347鈫拁365(+maybeAutoPush helper + 6 鎸傝浇鐐?, `workbench.js` 445鈫拁460(+syncStore.attachWorkbench + initialize 鎺㈡祴 + 5 re-export), `SyncPanelContent.vue` 475鈫?61(鈭?4, hero 鍔犲窘绔?鑷姩鍚屾鐘舵€佽), **鏂板 `SyncAutoSyncControl.vue` 156**(鑷姩鍚屾寮€鍏?, **鏂板 `SyncConflictResolver.vue` 141**(鍐茬獊妗嗚縼鍑? 瑙?syncPanelContent 瓒?500 纭笂闄?. | none锛坈rypto 鍘熻澶嶇敤锛歟ncrypt_with_key 澶嶇敤 AES-256-GCM 鏍稿績锛屼粎澶栨彁 derive_key锛汼essionKey 瀛樺偍鐓?github-pat 鐨?SecretStore+DPAPI 鑼冨紡锛沵aybeAutoPush 璧?workbench bridge 璋?syncStore 绂佸惊鐜?import锛沨ero/block 鑼冨紡鍚屽煙鍚屽眰澶嶇敤锛?| high | passed 鈥?`test:core` 51/51 pass锛?7 key-based 娴嬭瘯锛? `npm run build` exit 0 (3.31s, 1681 modules), `cargo check` exit 0 (1m18s)銆?*瀹夊叏妯″瀷鍙樻洿宸插啓鍏?AGENTS.md 搂8**锛氫富瀵嗙爜浠嶄笉钀界洏锛岃嚜鍔ㄥ悓姝ョ敤 DPAPI 淇濇姢娲剧敓浼氳瘽瀵嗛挜銆?*src-tauri/sync.rs ~830 杩?soft-warn锛屼笅娆″ぇ鏀瑰墠鍏虫敞**銆?|
| 2026-06-30 | **UI 鏁翠綋閲嶆瀯锛坥pen-design 璁捐绋胯惤鍦帮級鍚姩鍓嶇殑 vibe-guard 鏋舵瀯妫€鏌?*銆俬igh-tier锛堟柊鎶借薄+鍏ㄩ」鐩瑙夊绾﹂噸瀹氫箟锛夈€傝鍒掞細鍩轰簬 5 涓?HTML 璁捐绋?index 910/sidebar 600/terminal 788/files 793/monitor 921 琛? 閲嶅啓 `_tokens.scss`(309琛? **44 澶勫紩鐢ㄧ垎鐐稿崐寰?*) + 鏁翠綋閲嶅啓 shell/terminal/files/resource-monitor 32 涓?.vue銆?*Plan Gate 閿氬畾閫氳繃**锛堝澶?file:line 缁?grep 楠岃瘉鐪熷疄锛夈€?*Reuse Check on "鏂板缓 RightSidebar.vue 瀹瑰櫒"**锛歛dversarial AGAINST 鑳滃嚭锛堝弻閲嶆姌鍙犳帶鍒堕闄╋級锛孏ate A FAIL锛圓ppShellLayout 鏄敮涓€甯冨眬瀹瑰櫒涓嶅彉閲忥紝鏂板缓瀹瑰櫒灞備笌涔嬪啿绐侊級锛孏ate B FAIL锛堝弽浜嬪疄锛氬崟缁勪欢鏃朵笉闇€瑕佸鍣級銆?*杈圭晫閲嶅 HALT 浜ょ敤鎴?*锛歊ightSidebar.vue 瀹瑰櫒鏄惁鏂板缓 + OpsSummaryPanel鈫扴essionSummary 鏀瑰悕銆?| 寰呭畾锛堝彇鍐充簬鐢ㄦ埛閫?A/B/C锛?| high | **HALT 鈥?杈圭晫閲嶅妫€娴嬪埌锛岀瓑鐢ㄦ埛鍐冲畾鏂规 A/B/C 鍚庣户缁?*銆傛湰娆℃湭鍐欎唬鐮侊紝闆堕獙璇併€?|
| 2026-06-30 | **UI 鏁翠綋閲嶆瀯鍒囩墖 1锛歵oken 钀藉湴 + 鍙充晶鏍忛噸鍐?*锛堢敤鎴烽€夋柟妗?B锛夈€? 鏂囦欢锛歚_tokens.scss` 309鈫拁340(閲嶅啓涓?open-design 娴呰壊鍊?#fafafa/#2c5fe5, 淇?data-theme='auto'鈫?system' 姝讳唬鐮?bug, 淇濈暀鍏ㄩ儴鍙橀噺鍚嶅绾?SCSS maps), **鏂板 `tokens.css` ~120**(璁捐绋垮師濮?CSS 澶囦唤, 鍙涓嶅弬涓庢瀯寤?, `index.html` +Google Fonts(Inter+JetBrains Mono, 淇瓧浣撴湭鍔犺浇 bug), `ResourceMonitorPanel.vue` 157鈫拁240(璐村悎 monitor.html rs-header/rs-section/metric-card 缁撴瀯, 鏂板 emit('collapse') + conn-pill + 绌虹姸鎬佹í骞?, `CpuChart.vue` 75鈫拁115(鍔?hasData prop + 绌虹姸鎬佺伆鍩虹嚎 grid-line/line-empty/baseline), `MemoryChart.vue` 84鈫拁125(鍚屼笂+mem-bar 寰呮満鐏?, `NetworkChart.vue` 75鈫拁130(鍚屼笂+鍙岀伆鍩虹嚎 RX/TX), `DiskChart.vue` 136鈫拁165(鍚屼笂+disk-row 鍗犱綅), `OpsSummaryPanel.vue` 208鈫拁230(璐村悎 monitor.html session summary 閿€煎垪琛? 瀹夊叏杈圭晫绾緥: 鍑嵁/MCP 鐢ㄤ腑鎬х伆 badge 姘镐笉鍙樼豢)銆?*鏋舵瀯涓嶅彉**锛氫繚鐣?store 璁㈤槄 + 瀛愬浘琛ㄥ垎鍙戣亴璐ｅ垎绂伙紱鎶樺彔浠嶇敱 AppShellLayout 绠★紙鏈柊寤?RightSidebar 瀹瑰櫒锛岄伒寰柟妗?B锛夈€?| none锛坈hart-utils buildLinePath/buildAreaPath/formatBytes/formatRate 鍘熸牱澶嶇敤锛泂tore 璁㈤槄妯″紡涓嶅彉锛沨asData 鏄柊澧?prop 涓嶇牬鍧忔棫濂戠害锛?| medium | passed 鈥?`npm run build` exit 0 (2.97s, 1690 modules), CSS 381.94鈫?89.64 kB(+7.7kB)銆?*寰呯敤鎴风‘璁ゅ彸渚ф爮椋庢牸鍚庣户缁叾瀹冨尯鍩熷垏鐗?*銆?|
| 2026-06-30 | **鍒囩墖 1 绾犳锛氱敤鎴峰弽棣?璧勬簮杩愮淮杩樻槸鑰佺増鏈?鈫?璇婃柇鍑烘柟妗?B 婕忔帀璁捐绋挎牳蹇冪粨鏋勶紙缁熶竴瀹瑰櫒+涓夋寮?缁熶竴婊氬姩+footer+emit 鎺ョ嚎锛夆啋 vibe-guard Gate A 璇垽绾犳**銆傞噸鏂版牳瀵瑰彂鐜?`ConnectionSidebar` 鏈韩灏辨槸宸︿晶鏍忕粺涓€瀹瑰櫒鍏堜緥锛堣嚜甯?header/footer + emit('toggle-collapse') + 鐖剁骇鎺?store锛夛紝涓?AppShellLayout 涓嶅啿绐?鈫?RightSidebar 鍚屾瀯鍚堟硶銆? 鏂囦欢锛?*鏂板 `RightSidebar.vue` ~280**(璐村悎 monitor.html rs-header[conn-pill+鏆傚仠/瀵煎嚭/鏀惰捣鎸夐挳]/rs-body[缁熶竴婊氬姩,宓孯esourceMonitorPanel+OpsSummaryPanel]/rs-footer[poll 2s路history 60+鏀惰捣]), `ResourceMonitorPanel.vue` 浠庣嫭绔嬮潰鏉块檷绾т负瑁?section(鍘?header/婊氬姩, 鐢辩埗缁熶竴绠?, `OpsSummaryPanel.vue` 鍚屾牱闄嶇骇涓鸿８ section, `App.vue` 鎺ョ嚎 `<RightSidebar @collapse="onToggleRight"/>`(澶嶇敤鏃㈡湁 onToggleRight鈫抯tore.toggleRight). **Gate A 绾犳璁板綍**锛氫箣鍓嶅垽 FAIL 鏄鍒わ紝ConnectionSidebar 鍏堜緥璇佹槑銆屽尯鍩熺骇缁熶竴瀹瑰櫒 + AppShellLayout grid 瀹藉害鍙橀噺銆嶆槸瑙ｈ€︾殑涓ゅ眰锛屼笉鍐茬獊銆?| none锛圧ightSidebar 涓?ConnectionSidebar 鍚屾瀯锛汻esourceMonitorPanel/OpsSummaryPanel 闄嶇骇涓?section 涓嶆敼 store 璁㈤槄锛沷nToggleRight 澶嶇敤锛?| high | passed 鈥?`npm run build` exit 0 (5.03s, 1690 modules)銆?*vibe-guard 璇垽宸茶褰曪紝鍚庣画鍖哄煙瀹瑰櫒锛堝 FileRegion/TerminalRegion锛夊彲鍙傜収 RightSidebar/ConnectionSidebar 鍚屾瀯妯″紡**銆?|
| 2026-07-01 | **app.html 鍏ㄩ噺杩樺師锛堟暣浣撹瀺鍚堣璁＄涓ユ牸杩樺師锛?*銆俬igh-tier锛堝叏椤圭洰瑙嗚濂戠害閲嶅畾涔?+ grid 缁撴瀯鏀归€狅級銆傚熀浜?app.html/app.css/app.js 涓変欢濂楅噸鏋?8 涓牳蹇冨竷灞€鏂囦欢锛歚_tokens.scss` 閲嶅啓涓?app.css 瀹炶壊 token( accent-soft #e8efff 绛?+ oklch tinted dark + --term-* 鐙珛缁堢 token + --app-selected + 灏哄 token --titlebar-h/--sidebar-w 绛?+ --shadow-pop), `AppShellLayout.vue` grid 浠庢墎骞?4 琛屾敼涓?3 琛?.main 浜岀骇 grid(titlebar/sidebar/main[terminal+files]/right/statusbar) + class .app/.main + resize data-target + 鎶樺彔绫诲悕椹卞姩(.sidebar-collapsed/.right-collapsed), `AppTitleBar.vue` 鍔?traffic 绾㈢豢鐏?SVG logo+.tb-brand+.tb-search(鈱楰)+绾?icon-btn(鍘绘枃瀛楁爣绛?, `AppStatusBar.vue` 鍔?.sb-center(UTF-8/LF/zsh)+badge.muted/.warn 鏍峰紡, `ConnectionSidebar.vue` chrome-label+sb-count+鏂扮┖鐘舵€?铏氱嚎杈规瀹瑰櫒+涓绘寜閽?+sb-search/sb-tree/sb-footer+鎶樺彔鎬佷繚鐣欑珫鎺掑浘鏍囧垪(瀹炵敤浼樺厛鍋忕鐐?, `TerminalSurface` region-terminal grid 38px/44px/1fr + term-canvas-wrap + term-watermark(缃戞牸鑳屾櫙) + term-cursor(ready 鍏夋爣), `FileSurface` region-files + file-header(chrome-label+view-pills+file-actions) + 榛樿鍙屾爮(file-dual 1fr 1px 1fr) + drop-hint + files.js localPaneVisible 榛樿鏀?true, `RightSidebar.vue` class 瀵归綈(rs-status-pill/rs-actions/chrome-label/rs-conn-meta, 鍘?collapse-btn 鏂囧瓧鎸夐挳). **鏋舵瀯涓嶅彉閲忎繚鐣?*: 鎵€鏈?emit/props/store 璁㈤槄/provide-inject 鍘熸牱淇濈暀; slot 鍚?center-top/center-bottom 淇濈暀(App.vue 涓嶇敤鏀?; usePanelResize 鍐?--center-top-h 涓嶅彉(.main grid 娑堣垂). | none锛坕con-btn 28x28 瑙勬牸缁熶竴澶嶇敤 app.css; chrome-label 涓枃鍙嬪ソ璋冩暣鍘?uppercase; traffic 绾瑙夎楗?data-tauri-drag-region; RightSidebar 涓?ConnectionSidebar 鍚屾瀯妯″紡澶嶇敤锛?| high | passed 鈥?`npm run build` exit 0 (3.25s, 1690 modules). **寰?tauri:dev 浜哄伐楠屾敹鍏ㄩ儴鍖哄煙瑙嗚**銆?|
| 2026-07-01 | 鎺ュ叆 open-design 鍙嫋鎷藉垎鏍忋€傛妸 `usePanelResize` 瀵归綈 `app.js` 鐨勪笁鏉?resize 浜や簰锛氬乏鏍?/ 鍙虫爮 / 缁堢-鏂囦欢涓婁笅鍒嗙晫锛屾柊澧?`--terminal-h` 涓?`--center-top-h` 鍚屾銆佸竷灞€姣斾緥鎸佷箙鍖栥€佸弻鍑诲浣嶃€佹柟鍚戦敭寰皟锛屽苟鍦?`OpenDesignShell` 鏆撮湶涓夋潯 separator + sidebar rail 鎶樺彔鎬併€?| 鍏煎鏃?`myshelltool-layout` 瀛樺偍锛屼繚鐣?`--center-top-h` 渚涙棫 shell/娴嬭瘯璇诲彇锛涙柊 shell 浠?`myshelltool:layout:v1` 鍜?`--terminal-h` 涓轰富銆?| medium | passed 鈥?`npm run build` exit 0锛沗node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 閫氳繃锛汸laywright 楠岃瘉涓夋潯 handle 鍙嫋鍔ㄣ€佹姌鍙犳€佸搴?44/0銆侀敭鐩?Arrow 寰皟鐢熸晥銆?|
| 2026-07-02 | 涓ユ牸瀵归綈 open-design 鏂囦欢鍖哄昂瀵搞€傚皢鏂?shell 鐨勪腑涓嬫枃浠剁郴缁熷尯鍩熶粠 `pathbar/file-table-head/file-empty` 缁撴瀯鏀瑰洖璁捐绋胯涔?`pane-header/file-list/col-header/col-empty`锛屽苟鎸?`app.css` 璁剧疆 38px file header銆?6px pane header銆?1.5px view pill銆?0px pane tag銆?0.5px col header銆?2.5px/11px empty state銆?1.5px mono drop hint銆?| 鏃犳柊澧炴娊璞★紱澶嶇敤 open-design 鍘熷 class 鍚嶃€俙OpenDesignShell.vue` 宸茶秴杩?500 琛岋紝鏈鍙仛灏哄杩樺師锛屽悗缁簲鍗曠嫭鎷嗗垎 open-design shell銆?| medium | passed 鈥?`npm run build` exit 0锛沗node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 閫氳繃锛汸laywright computed style 纭 file header 38銆乸ane header 36銆乿iew pill 11.5px銆乧ol header 10.5px銆乨rop hint 11.5px銆?|
| 2026-07-02 | 鏂囦欢鍖烘爣棰樹笌鍙充晶鎽樿鍏ㄩ儴涓枃鍖栵紝骞舵妸鏂囦欢浼犺緭鍖烘爣棰樿兌鍥婅繘涓€姝ュ帇绐勶細`chrome-label` 鏀逛腑鏂囷紙鏂囦欢浼犺緭 / 璧勬簮鐩戞帶 / 浼氳瘽鎽樿 / 杞 2 绉?路 鍘嗗彶 60 鏉★級锛宍view-pill` line-height 鏀剁揣涓?1銆乸ane tools 鏀?24px 鍥炬爣鎸夐挳锛屼繚鎸佽璁＄鐨?10-11.5px 瀵嗗害銆?| 缁х画澶嶇敤 open-design 鍘?class 缁撴瀯锛屼笉鏂板甯冨眬灞傘€?| medium | passed 鈥?`npm run build` exit 0锛沗node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 閫氳繃锛汸laywright 楠岃瘉 `viewPills=24px`銆乣viewPill=18px`銆乣chrome-label=10px`銆?|
| 2026-07-02 | 浼氳瘽鎽樿涓嬬殑瀛楁鍚嶅叏閮ㄤ腑鏂囧寲锛歚session/host/fingerprint/uptime/tunnels/forwards/sync/snapshots/last cmd` 鏀逛负 `浼氳瘽/涓绘満/鎸囩汗/鏃堕暱/闅ч亾/杞彂/鍚屾/蹇収/鏈€杩戝懡浠锛屽彸渚у簳閮?`tunnels` 鍜岀姸鎬佹爮 `SSH idle 路 backend` 涔熶竴骞朵腑鏂囧寲銆?| 浠呮枃妗堟湰鍦板寲锛屼笉鏀规憳瑕佸竷灞€涓庢暟鎹祦銆?| low | passed 鈥?`npm run build` exit 0锛沗node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 閫氳繃锛汸laywright 璇诲洖 summary/footer/statusbar 鏂囨湰鍧囦负涓枃銆?|
| 2026-07-02 | 鎺ュ洖鐪熷疄鍔熻兘缁勪欢骞舵竻鐞嗕复鏃惰璁″懡鍚嶏細`WorkbenchShell.vue` 251 琛岋紙鐢变复鏃堕潤鎬佸３鏀逛负澶栧３ + `ConnectionSidebar`/`RightSidebar`/`TerminalSurface`/`FileSurface` 缁勫悎锛夛紝鍒犻櫎 `ConnectionAssetGroupNode.vue`锛宍RightSidebar.vue` 237 琛岋紝`OpsSummaryPanel.vue` 249 琛岋紝`ResourceMonitorPanel.vue` 174 琛岋紝`AppStatusBar.vue` 258 琛岋紝`workbench-shell.scss` 808 琛岋紱鍚屾椂淇 `FileSurface` 涓敊璇殑 `openMkdirModal?/refreshRemote?` 璋冪敤锛岀Щ闄ょ姸鎬佹爮鍜屽彸涓嬭 footer 鐨勯噸澶嶁€滈毀閬撯€濄€?| none锛堝鐢ㄧ幇鏈?`ConnectionSidebar` 浜嬩欢濂戠害銆乣GlobalModals` 鍒嗙粍/璧勪骇寮圭獥銆乣RightSidebar` 璧勬簮鐩戞帶瀹瑰櫒锛涗笉鍐嶄繚鐣?`OpenDesign*`/`ConnectionAssetGroupNode` 涓存椂鏍戙€傚澹冲鍣ㄦ敼鍚?`sidebar-region`锛岄伩鍏嶄笌鐪熷疄 `.sidebar` 缁勪欢閫夋嫨鍣ㄥ啿绐侊級 | medium | passed 鈥?`npm run build` exit 0锛沗MYSHELLTOOL_BASE_URL=http://127.0.0.1:41236/ node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 閫氳繃锛汸laywright 鏂囨湰妫€鏌ワ細璧勬簮鐩戞帶/浼氳瘽鎽樿涓轰腑鏂囷紝鐘舵€佹爮涓庡彸渚?footer 鍧囦笉鍚€滈毀閬撯€濓紝椤甸潰鎬烩€滈毀閬撯€濆嚭鐜?1 娆★紙浼氳瘽鎽樿锛夈€?|
| 2026-07-02 | 淇鐪熷疄缁勪欢鎺ュ叆鍚庣殑鏍峰紡鍥炴祦锛歚workbench-shell.scss` 808鈫?23锛屽垹闄ゆ棫闈欐€佸３閬楃暀鐨?`.region-terminal/.term-*/.region-files/.file-*/.right-sidebar/.rs-*` 鍏ㄥ眬瑙勫垯锛岄伩鍏嶈鐩?`TerminalSurface`銆乣FileSurface`銆乣RightSidebar` 鐨?scoped 鏍峰紡锛沗RightSidebar.vue` 237鈫?38锛屾牴鑺傜偣琛?`grid-area: right`銆?| none锛堜笉鏀逛笟鍔＄粍浠堕€昏緫锛屽彧鎶婂澹虫牱寮忚竟鐣屾敹鍥炲埌甯冨眬/titlebar/statusbar/resize锛涗唬浠锋槸 `workbench-shell.scss` 涓嶅啀鎵挎媴鐪熷疄缁勪欢鍐呴儴缁嗚妭锛屽悗缁瑙夎皟鏁村繀椤昏繘瀵瑰簲缁勪欢鏂囦欢锛?| medium | passed 鈥?`npm run build` exit 0锛沗MYSHELLTOOL_BASE_URL=http://127.0.0.1:41236/ node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 閫氳繃锛汸laywright DOM 妫€鏌ワ細main/terminal/canvasWrap 瀹藉害鍧?900px銆乺atio=1锛屽彸渚у惈鈥滆祫婧愮洃鎺?浼氳瘽鎽樿鈥濅笖涓嶅惈 `resource monitor/session summary`锛宖ooter 涓衡€滈噰鏍?2s 路 60 鐐光€濄€?|
| 2026-07-02 | 鍙充晶鏍忓洖杩佷负鐪熷疄缁勪欢鏍峰紡骞惰ˉ榻愮郴缁熷姛鑳藉尯锛歚RightSidebar.vue` 鎭㈠ monitor.html 鐨勪笁娈靛紡瀹瑰櫒銆侀噰鏍?瀵煎嚭/鏀惰捣鎿嶄綔涓庝腑鏂?footer锛沗ResourceMonitorPanel.vue` 鏀瑰洖鍒嗛殧绾垮紡鐩戞帶 section锛沗OpsSummaryPanel.vue` 澧炲姞鈥滅郴缁熷姛鑳解€濆尯骞朵繚鐣欎細璇濇憳瑕佷腑鏂囧瓧娈碉紱`CpuChart.vue` / `MemoryChart.vue` / `NetworkChart.vue` / `DiskChart.vue` 缁熶竴鏇挎崲娈嬬暀鑻辨枃/涔辩爜鏂囨銆?| none锛堝鐢ㄧ幇鏈?store 涓?chart-utils锛屼笉鏂板鎶借薄锛涗唬浠锋槸鍙充晶鏍忔粴鍔ㄥ唴瀹规洿瀵嗭紝榛樿瑙嗗彛涓嬬郴缁熷姛鑳介渶瑕侀殢鍙虫爮婊氬姩鏌ョ湅锛?| medium | passed 鈥?`npm run build` exit 0锛沗node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 閫氳繃锛汸laywright 妫€鏌ュ彸渚ф爮涓枃鏂囨湰銆乫ooter 鏃犻噸澶嶁€滈毀閬撯€濓紝缁堢瀹藉害 826px銆佸彸鏍忓搴?280px銆?|
| 2026-07-02 | 纠正右侧资源监控与终端空状态：`ResourceMonitorPanel.vue` 恢复 app.html/app.css 的 2×2 `metric-grid` 四卡结构；四个 chart 组件把无数据态收紧为单个 `—`，隐藏纵向版 foot/内存条/磁盘行；`TerminalPane.vue` 抬高空状态层级并恢复“暂无活跃会话 / ssh user@host / 新建标签”设计稿文案。 | none（继续复用 chart-utils 和现有终端 store 接线；只改展示，不改连接/采样/终端动作逻辑） | medium | passed — `npm run build` exit 0；`node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 通过；Playwright 确认 4 张卡为 2 列 2 行、终端空状态可见且 z-index=2。 |
| 2026-07-02 | 紧凑化右侧四格资源卡片文本：`chart-utils.js` 54（新增 `formatCompactRate`），`MemoryChart.vue` 112，`NetworkChart.vue` 119，`DiskChart.vue` 137，`ResourceMonitorPanel.vue` 178；内存标题行改为百分比，网络/磁盘改为紧凑中文读数并保留完整 `title/aria-label`。 | none（复用既有 resource-monitor chart-utils 共享格式化层；不新增组件或跨层依赖） | medium | passed — `git diff --check` exit 0；`npm run build` exit 0；Playwright 验证空状态与最坏样例 `收999M发999M` / `读999M写999M` 均无截断、越界或标题重叠。 |
| 2026-07-04 | UI 未接线按钮修复：`WorkbenchShell.vue` 422 接回全局搜索输入/建议；`sessions.js` 862 接回命令面板 connect；`files.js` 765 + `fs_local.rs` 233 + `lib.rs` 525 接本地文件上传远程；`FileSurface.vue`/`FileColumn.vue` 清掉“待接线”入口。 | none（复用 ui searchState/activateSuggestion、sessions connectSelected、既有 sftp_upload_* 分块上传；只新增本地按块读取命令，未引入依赖） | medium | partial passed — `git diff --check` exit 0；`npm run build` exit 0；`cargo check` 默认 target 被 WebView2Loader.dll os error 32 阻断，独立 target 被 ring/gcc 工具链阻断。 |
| 2026-07-04 | 文件管理 loading 接线：`files.js` 约 780（远程/本地文件操作 busy stack + messages），`FileColumn.vue` 约 1150（列级遮罩/Spinner/忙碌时交互 guard），`FileSurface.vue` 约 420（远程忙碌时禁用上传/刷新/新建）。 | none（复用现有 Pinia files store、lucide `Loader2`、TransferDrawer 的 spinner 语义；未新增抽象，因热点文件已超限仅做 scoped 接线） | medium | passed — `npm run build` exit 0；`git diff --check` exit 0（仅 CRLF 提示）。 |
| 2026-07-05 | 文件管理拆分第一刀：`FileColumn.vue` 1176→1136，新增 `fileColumnUtils.js` 50 行，抽出类型/大小/时间/owner/路径面包屑纯 helper；`bump-version.mjs` release 防呆已先提交。 | none（helper 属于 files 组件同域展示层，未跨 store/后端边界；模板函数名保留为薄转发以降低行为风险） | medium | passed — `node --check src/components/files/fileColumnUtils.js` exit 0；纯函数断言通过；`npm run build` exit 0。 |
| 2026-07-05 | 文件管理拆分第二刀：`FileColumn.vue` 1194→842，新增 `FileColumnHeader.vue` 466，把路径面包屑、路径输入、过滤弹层、上级/刷新按钮迁出父列组件。 | none（同属 `src/components/files` 展示层，父组件继续持有 store 绑定与 SFTP/本地动作；新组件只接 props/emit，不新增状态源或跨层依赖） | medium | passed — `node --check src/components/files/fileColumnUtils.js` exit 0；`npm run build` exit 0。 |
| 2026-07-05 | 文件管理拆分第三刀：`FileColumn.vue` 842→589，新增 `FileColumnList.vue` 308，把文件行、空状态、loading overlay 迁出父列组件。 | none（沿用 `FileColumnHeader.vue` 的同域 props/emit 模式；父组件继续持有选择、导航、下载、右键菜单行为，列表组件只渲染并转发事件） | medium | passed — `npm run build` exit 0；`npm run test:ui` exit 0；`git diff --check` exit 0（仅 CRLF 提示）。 |
| 2026-07-05 | 文件管理拆分第四刀：`FileColumn.vue` 589→473，新增 `FileColumnColumns.vue` 88，把可排序列头迁出父列组件，使 `FileColumn.vue` 首次低于 Vue SFC 500 行硬上限。 | none（同域展示层 props/emit；父组件继续持有 sort action 和本地/远程排序状态，列头组件只显示 active/方向并发出 sort key） | medium | passed — `npm run build` exit 0；`npm run test:ui` exit 0；`git diff --check` exit 0（仅 CRLF 提示）。 |
| 2026-09-07 | RSA 私钥认证修复 + 私钥连接 UX（方案 C）：`ssh.rs` 2111（+15，GUI/headless 两处重复的私钥签名构造收敛为 `wrap_key_with_preferred_hash` 单 helper）；`GlobalModals.vue` 729→1031（**+302，2.06× hard 500，拆分加急候选**：新增 reauthPassword 表单 + 浏览/清除按钮）；`TerminalSurface.vue` 459（+20 错误卡片认证失败引导）；`assets.js` 369（+19 凭据 clear 语义）；`sessions.js` 1003（+14，修 session.asset 旧快照 + wasActive 时序两个 P1）；`backend.js` 149（+18）；新增 `tauri-plugin-dialog`（Cargo+lib.rs+capabilities `dialog:allow-open`）。 | none（reauthPassword 走 GlobalModals 既有 modal.type 分支；dialog 走官方插件，无自造轮子） | medium | passed — `cargo check` exit 0（1m29s）；`npm run build` exit 0（9.47s）；`npm run test:ui` 3/3 pass。**下次拆分会话应纳入 GlobalModals.vue（首列 reauthPassword 表单 / assetEditor 凭据区块抽子组件，参照 PatConfigCard 先例）** |
| 2026-09-07 | 状态栏更新提示死链接修复：`WorkbenchShell.vue` 452→476（+24，autoUpdate prop + available/error 态真按钮 + 设置图标徽标）；`workbench-shell.scss` +30（.sb-update-btn / .update-badge）；`App.vue` 137（+1 传 prop）。根因：useAutoUpdate 文案引导「点击状态栏更新」但渲染的是纯 span，自 v0.6.0 起即无点击事件，唯一入口是设置面板。 | none（复用 App.vue 既有 autoUpdate 实例，仅补 prop 接线，未新建状态源） | low | passed — `npm run build` exit 0（3.08s）；`npm run test:ui` 3/3 pass（浏览器预览态 state 恒 idle，纯文本路径不受影响） |

## Baseline snapshot (2026-06-23, RESET 鈥?measured via `wc -l` via PowerShell `Get-Content.Count`)

Files currently over hard limit 鈥?refactor candidates (real numbers, not estimated):

| File | Lines | Limit | Multiple | Note |
|---|---|---|---|---|
| `src-tauri/src/ssh.rs` | 2197 | 800 | 2.75脳 | 4 domains fused: session/terminal, SFTP, tunnel (local+remote+SOCKS5), headless. **cycle-tier** (1874鈫?097 since 06-22 RESET). Highest priority to split. |
| `src/components/files/FileColumn.vue` | 493 | 500 | 0.99脳 | **cycle-tier 瑙﹀彂**锛?68鈫?123, +355, 1脳鈫?.25脳 鑷?06-22 RESET锛夈€倂1.4銆屾枃浠剁鐞嗗尯绮剧畝閲嶆瀯銆峜ommit 8fa14a0 鍚庡弽鑰屾毚娑ㄣ€傛渶楂樹紭鍏堢骇鎷嗗垎銆?|
| `src/stores/sessions.ts` | 1323 | 500 | 2.65脳 | terminal lifecycle + session mgmt |
| `src/components/shell/ConnectionSidebar.vue` | 972 | 500 | 1.94脳 | tree + menu + drag mixed锛?6-29 +131 鎷栨嫿閫昏緫锛?drag mixed" 鍧愬疄锛涗笅娆℃媶鍒嗭細鎶?drag state/handlers 鎶?useAssetDnd composable锛?|
| `src/components/shell/GlobalModals.vue` | 1182 | 500 | 2.36脳 | all modals centralized |
| `src/stores/files.ts` | 1357 | 500 | 2.71脳 | SFTP + transfer queue |

> 2026-09-09 全量 TS 迁移（.js→.ts），行数含类型标注增量；上表为 wc -l 实测重估值：FileColumn.vue 已回落至 493 行（<500，不再是超标项）；`workbench.ts` 实测 507、`src-tauri/src/sync.rs` 实测 955、`crates/myshelltool-core/src/lib.rs` 实测 969 均已超各自 hard limit（见下方 soft-warn 表）；soft-warn 表中未超标项行数未重测。2026-09-10 `wc -l` 重测并刷新了本文件全部数字快照（上表 6 项 + soft-warn 表 4 项）。

### 螖 vs previous baseline (2026-06-20)

All previously-over-limit hotspots shrank; 2 dropped below hard limit:

| File | 2026-06-20 | 2026-06-22 | 螖 | Status |
|---|---|---|---|---|
| `src-tauri/src/ssh.rs` | 2055 | 1874 | 鈭?81 | still over (2.57脳 鈫?2.34脳) |
| `src/stores/sessions.js` | 888 | 839 | 鈭?9 | still over (1.78脳 鈫?1.68脳) |
| `crates/myshelltool-core/src/lib.rs` | 891 | 796 | 鈭?5 | **below limit now** (was 1.11脳, now 0.99脳) |
| `src-tauri/src/resource_monitor.rs` | 875 | 792 | 鈭?3 | **below limit now** (was 1.09脳, now 0.99脳) |
| `src/components/files/FileColumn.vue` | 820 | 768 | 鈭?2 | still over (1.64脳 鈫?1.54脳) |
| `src/components/shell/GlobalModals.vue` | 729 | 693 | 鈭?6 | still over (1.46脳 鈫?1.39脳) |
| `src/components/shell/ConnectionSidebar.vue` | 711 | 651 | 鈭?0 | still over (1.42脳 鈫?1.30脳) |
| `src/stores/files.js` | 661 | 613 | 鈭?8 | still over (1.32脳 鈫?1.23脳) |

### 螖 vs previous baseline (2026-06-22 鈫?2026-06-23)

鈿狅笍 **涓ゅ cycle-tier 瑙﹀彂**锛堝崟鏂囦欢鑷笂娆?RESET 璺ㄨ秺澧為暱闃堝€硷級锛?

| File | 2026-06-22 | 2026-06-23 | 螖 | Status |
|---|---|---|---|---|
| `src-tauri/src/ssh.rs` | 1874 | 2097 | **+223** | still over (2.34脳 鈫?2.62脳), **cycle-tier** |
| `src/components/files/FileColumn.vue` | 768 | 1123 | **+355** | still over (1.54脳 鈫?**2.25脳**), **cycle-tier 涓ラ噸** |
| `src/stores/sessions.js` | 839 | 858 | +19 | still over (1.68脳 鈫?1.72脳) |
| `src/stores/files.js` | 613 | 688 | +75 | still over (1.23脳 鈫?1.38脳) |
| `src/components/shell/ConnectionSidebar.vue` | 651 | 711 | +60 | still over (1.30脳 鈫?1.42脳) |
| `src/components/shell/GlobalModals.vue` | 693 | 694 | +1 | still over (1.39脳 鈫?1.39脳, 鎸佸钩) |

> **FileColumn.vue 璀︽姤**锛歷1.4 commit `8fa14a0`锛坒eat: 鏂囦欢绠＄悊鍖虹簿绠€閲嶆瀯 + MCP 鍐呭祵 HTTP 鏀跺熬锛夊悕涔夋槸銆岀簿绠€閲嶆瀯銆嶏紝浣?FileColumn.vue 浠?768 鏆存定鍒?1123锛?355锛屽嚑涔庣炕鍊嶏級銆傝繖鏄吀鍨嬬殑"閲嶆瀯鍚嶄箟涓嬪爢鍙?鈥斺€斾笅娆′細璇濅紭鍏堝仛绾灦鏋勬媶鍒嗭紝鑰岄潪缁х画寰€閲屽姞鍔熻兘銆傚缓璁媶鍒嗚竟鐣岋細`FileColumnHeader` / `FileColumnBody`锛堝垪琛級/ `FileColumnContextMenu` / `FileColumnDropZone`锛堟嫋鎷戒笂浼狅級銆?

### v1.4 MCP 鍐呭祵 GUI 閲嶆瀯锛?026-06-23锛屾灦鏋勭骇鍙樻洿锛?

閲嶅ぇ鏋舵瀯閲嶆瀯锛歁CP 浠庛€屽弻浜岃繘鍒?+ stdio + named pipe 妗ャ€嶆敼涓恒€屽唴宓?GUI + Streamable HTTP transport銆嶃€傚噣鏂囦欢鍙樺寲锛?

| 鎿嶄綔 | 鏂囦欢 | 璇存槑 |
|---|---|---|
| **鍒犻櫎** | `src-tauri/src/mcp/pipe.rs` (592 琛? | named pipe 妗ワ紝鍐呭祵鍚庢棤闇€锛堜粠 watch list 娑堝け锛?|
| **鍒犻櫎** | `src-tauri/src/bin/mcp.rs` (19 琛? | 鐙珛 console bin 鍏ュ彛锛屽彇娑堝弻浜岃繘鍒?|
| **鍒犻櫎** | `scripts/mcp-dev-watch.mjs` | 鐙珛 bin 鐨?dev watch 鑴氭湰锛屽凡鏃犵洰鏍?|
| **鏂板** | `src-tauri/src/mcp/http_server.rs` (~110 琛? | axum + rmcp Streamable HTTP server |
| **閲嶅啓** | `src-tauri/src/mcp/probe.rs` (318鈫拁190 琛? | spawn 鎺㈡祴 鈫?HTTP 鍋ュ悍妫€鏌?|
| **鐦﹁韩** | `src-tauri/src/mcp/server.rs` (379鈫拁340 琛? | 鍒?serve_stdio + pipe 瀹℃壒闄嶇骇 |
| **鐦﹁韩** | `src-tauri/src/mcp/tools.rs` (427鈫拁360 琛? | 鍒?pipe 澶嶇敤鍒嗘敮锛宔xec_on_asset 鐩磋蛋 headless |
| **鐦﹁韩** | `src-tauri/src/lib.rs` (529鈫拁470 琛? | 鍒?run_mcp_stdio/init_mcp_logger/pipe 鍚姩 |
| **娓呯悊** | `src/stores/sessions.js` / `workbench.js` / `GlobalModals.vue` | 鍒?mcpApproval modal 姝讳唬鐮侊紙v1.1 pipe 瀹℃壒锛?|

**鏋舵瀯鏀剁泭**锛氬崟 exe 鍗曞畨瑁呭寘锛堟牴娌?NSIS 鎵撳寘缂哄彛锛夈€佹牴娌诲兊灏歌繘绋?os error 32銆佹秷闄?pipe 妗ュ鏉傚害銆?*Follow-up**锛氫細璇濆鐢紙娉ㄥ叆 SshSessionManager锛夈€丟UI 寮圭獥瀹℃壒锛堟敞鍏?AppHandle锛夈€?

### Known drift from AGENTS.md 搂0.5 (manual list, still stale)

AGENTS.md 搂0.5 records `ssh.rs(1548), sessions.js(810), FileColumn.vue(768), files.js(612), GlobalModals.vue(512)` 鈥?actual ssh.rs is 1874 (off by +326), GlobalModals.vue is 693 (off by +181). The hand-maintained list rotted again. This log remains the source of truth; AGENTS.md 搂0.5 list should be regenerated from here or removed.

### Soft-warn zone (300鈥?00 for .vue/.js, 400鈥?00 for .rs) 鈥?watch list

Not over the hard limit, but approaching 鈥?track so they don't silently cross:

| File | Lines | Limit type | Soft% |
|---|---|---|---|
| `src-tauri/src/sync.rs` | 955 | .rs (soft 400 / hard 800) | **119% of hard**锛?6-29 v1.6 鑷姩鍚屾 +SessionKey helper + 3 鍛戒护 + b64 宸ュ叿锛?85鈫拁830锛屸殸 鎺ヨ繎 hard limit锛屼笅娆″ぇ鏀瑰墠鍏虫敞锛?|
| `crates/myshelltool-core/src/lib.rs` | 969 | .rs (soft 400 / hard 800) | 鈿?**121% 鈥?宸茶秴 hard limit**锛?6-22 璁?796 宸叉紓绉伙紝瀹炴祴鍥炲崌锛涘簲绉诲叆涓婃柟纭笂闄愯〃锛屼笅娆?RESET 鏁寸悊锛?|
| `src-tauri/src/resource_monitor.rs` | 875 | .rs (soft 400 / hard 800) | 鈿?**132% 鈥?宸茶秴 hard limit**锛堝悓涓婏紝06-22 璁?792 宸叉紓绉伙級 |
| `src/stores/workbench.ts` | 507 | store .js (soft 300 / hard 500) | **101% of hard**锛?6-29 v1.6 +syncStore bridge + 鎺㈡祴 + 5 re-export锛?32鈫拁460锛岃创纭笂闄愶級 |
| `src/stores/sync.ts` | 339 | store .js (soft 300 / hard 500) | 68% of hard锛?6-29 v1.6 +autoSync/remoteUpdates 鐘舵€?+ 4 action + attachWorkbench锛?23鈫?39锛?|
| `src/components/shell/McpPanelContent.vue` | 374 | .vue (soft 300 / hard 500) | 75% of hard |
| `src-tauri/src/dangerous_commands.rs` | 402 | .rs (soft 400 / hard 800) | 50% of hard锛堣繘鍏?soft-warn锛?|
| `src/components/shell/SyncPanelContent.vue` | 461 | .vue (soft 300 / hard 500) | **92% of hard**锛?6-29 v1.6 鎷?SyncAutoSyncControl + SyncConflictResolver 鍚庝粠 586 鍥為檷鍒?461锛屼粛璐寸‖涓婇檺锛屼笅娆″姞鍔熻兘蹇呴』鍏堟媶锛?|
| `src-tauri/src/mcp/tools.rs` | 368 | .rs (soft 400 / hard 800) | 46% of hard |
| `src/stores/ui.ts` | 344 | store .js (soft 300 / hard 500) | 69% of hard |
| `src/components/terminal/TerminalTabs.vue` | 338 | .vue (soft 300 / hard 500) | 68% of hard |
| `src/components/shell/AppTitleBar.vue` | 334 | .vue (soft 300 / hard 500) | 67% of hard锛堟柊杩涘叆锛?|
| `src/components/shell/AssetGroupNode.vue` | 324 | .vue (soft 300 / hard 500) | 65% of hard锛堟柊杩涘叆锛?|
| `src/components/terminal/TerminalSurface.vue` | 323 | .vue (soft 300 / hard 500) | 65% of hard |
| `src/stores/assets.ts` | ~365 | store .js (soft 300 / hard 500) | 73% of hard锛?6-29 v1.6 +maybeAutoPush helper + 6 鎸傝浇鐐癸紝323鈫拁365锛?|
| `src-tauri/src/mcp/server.rs` | 339 | .rs (soft 400 / hard 800) | 42% of hard |
| `src/components/shell/SyncPatGuide.vue` | 206 | .vue (soft 300 / hard 500) | 41% of hard锛?6-29 v1.6 鏂板缓锛孭AT 寮曞瀛愮粍浠讹級 |
| `src/components/shell/SyncAutoSyncControl.vue` | 156 | .vue (soft 300 / hard 500) | 31% of hard锛?6-29 v1.6 鏂板缓锛岃嚜鍔ㄥ悓姝ュ紑鍏冲瓙缁勪欢锛?|
| `src/components/shell/SyncConflictResolver.vue` | 141 | .vue (soft 300 / hard 500) | 28% of hard锛?6-29 v1.6 鏂板缓锛屽啿绐佽В鍐冲瓙缁勪欢锛?|

> 宸茬Щ闄わ細`mcp/pipe.rs`锛坴1.4 宸插垹锛夈€乣lib.rs`/`resource_monitor.rs` 瀹炴祴宸茶秴 hard limit锛堟爣 鈿?寰呬笅娆?RESET 绉诲叆涓昏〃锛夈€?

## Next architecture session (proposed)

**Target 1**: split `src-tauri/src/ssh.rs` (2097, 2.62脳 鈫?~5 modules of 300-500 each). Natural boundaries already marked by `// ---` comment dividers in the file:
- `ssh/session.rs` 鈥?connect/auth/host-key/keyboard-interactive
- `ssh/sftp.rs` 鈥?all `sftp_*` commands + chunked upload
- `ssh/tunnel.rs` 鈥?`tunnel_*` + local/remote/dynamic-forward + SOCKS5
- `ssh/headless.rs` 鈥?`HeadlessSshClient` + `connect_headless` + `exec_command_once`
- `ssh/known_hosts.rs` 鈥?host-key verification helpers

Mechanical refactor: move code, adjust `pub`, re-export from `ssh/mod.rs`. Zero logic change. Frontend fully insulated (commands keep same names).

**Target 2锛坈ycle-tier 涓ラ噸锛屼紭鍏堢骇 = Target 1锛?*: split `src/components/files/FileColumn.vue` (1123, 2.25脳). 鍊欓€夋媶鍒嗚竟鐣岋紙闇€鍏堣鏂囦欢纭鑱岃矗鍒嗗竷锛夛細
- `FileColumnHeader.vue` 鈥?璺緞鏍?/ 鎺掑簭 / 瑙嗗浘鍒囨崲
- `FileColumnBody.vue` 鈥?鍒楄〃娓叉煋锛堣〃鏍?鍥炬爣鍙岃鍥撅級
- `FileColumnContextMenu.vue` 鈥?鍙抽敭鑿滃崟椤?+ actions
- `FileColumnDropZone.vue` 鈥?鎷栨嫿涓婁紶澶勭悊

> 鈿狅笍 FileColumn.vue 鍦?v1.4銆岀簿绠€閲嶆瀯銆峜ommit 鍚庝粠 768 鏆存定鍒?1123锛屾槸褰撳墠鏈€鍗遍櫓鐨勫爢鍙犵偣銆傛媶鍒嗗墠闇€ vibe-guard Gate A/B 鍒ゅ畾锛堢‘璁ゆ槸鍗曠粍浠惰亴璐ｈ繃杞斤紝鑰岄潪璇ユ湁鐙珛灞傦級銆?

**Secondary target (after ssh.rs lands)**: `src/stores/sessions.js` (839, 1.68脳) 鈥?split terminal lifecycle from session management.

---

## 2026-09-08 — MCP v2.2 文件传输全链路能力落地与无桩化

### 1. 架构目标与背景
MCP server 原先仅具备单一 exec shell 通道，导致 AI 宿主无法浏览目录、无法读写与传输文件，且 `sftp_remove` 在审批后报未实现造成负信任。本次迭代将 MCP 扩展为具备完整 SFTP 能力的安全文件通道，并消除全部工具桩。

### 2. 模块划分与行数控制（严格遵守 800 行硬上限）
- **`src-tauri/src/mcp/file_policy.rs` (186 行)**：纯安全策略模块。远程/本地路径规整防穿越；敏感文件判定（D2：/etc/shadow、SSH 私钥、.env 等）；根级毁灭性删除 HardBlock 判定；本机受保护系统目录防写入（D3）。
- **`src-tauri/src/mcp/sftp_ops.rs` (377 行)**：底层 Headless SFTP 封装。基于 `connect_headless` 的 handle 创建 SFTP session，提供目录遍历、1MB 上限小文件带 512B 二进制嗅探读取、临时文件原子写入、受控递归删除（最大深度 16 层）、大文件流式上传/下载与 SHA256 校验。
- **`src-tauri/src/mcp/file_tools.rs` (342 行)**：6 个文件传输工具的 Schema 定义与分发（`sftp_list` / `sftp_read_file` / `sftp_write_file` / `sftp_upload` / `sftp_download` / `sftp_remove`）。
- **`src-tauri/src/mcp/tools.rs` (433 行)**：消除所有桩；将文件工具委托给 `file_tools.rs`，自身行数从 486 行下降至 433 行；`list_sessions` 接入真实 GUI 会话池，`resource_monitor_snapshot` 接入真实系统资源快照。
- **`src-tauri/src/mcp/server.rs` (760 行)**：将审批逻辑与日志记录覆盖新增工具；执行 **D4 审计日志硬红线**（`sftp_read_file` 成功内容脱敏，绝不写入凭据与文件正文）。
- **`src/components/shell/McpExecutionLogList.vue` (295 行)**：前端日志表支持新增文件传输工具的中文名称映射与标签渲染。

### 3. 验收验证
- `cargo check`: OK
- `cargo check --tests`: OK
- `cargo test (core 51 tests)`: 51 passed, 0 failed
- `npm run build`: Vite build 成功

---

## 2026-09-09 — 资产独立工作台窗口（Tauri 2 多 WebviewWindow v1）

### 1. 架构决策
主窗口侧栏资产拖出窗口边界释放 / 右键「在独立窗口打开」→ 为该资产生成独立 WebviewWindow（`?win=asset&assetId=` query 路由）。**每窗口独立 webview + 独立 Pinia 实例，共享同一 Rust 后端进程**——会话/SFTP/资源监控全在后端（HashMap 键控天然多开），Rust 侧仅改 capabilities。独立窗口 = 该资产专注工作台：精简标题栏 + 上终端/下文件 + 完整可收起右栏监控；总是新建自己的会话（不跨窗口接管终端回放）。事前经 multi-role-review 五角色评审（10 问题全采纳：含 2 个 Critical——host-key 事件跨窗口串扰弹错资产名确认框、connecting 态关窗泄漏孤儿 SSH 连接）。

### 2. 关键实现与防串流设计
- `src/lib/assetWindows.js`（58 行）：label 确定性 sanitize（Tauri label 仅 `[a-zA-Z0-9-/:_]`）、openAssetWindow（已存在则 setFocus+unminimize）、isPointOutsideViewport（**client 坐标 vs viewport，规避 DPI 缩放下 screen 坐标歧义**——评审 Issue 6）。
- `src/components/workbench/AssetWindowShell.vue`（374 行）：复用无 props 的 TerminalSurface/FileSurface/RightSidebar；关窗 onCloseRequested → 确认 modal → connecting 会话轮询 settle（250ms/10s 上限，防 pending 占位 sessionId 断不开致 Rust 孤儿连接）→ allSettled 断开 → destroy。
- 跨窗口事件防串流：`resourceMonitor.applySnapshot` 按 sessionId 过滤（activeSessionId 为 null 拦截全部）；sessions 新增 `ownsConnectingSession(hostPort)` 守卫（payload 为 snake_case `host_port`，IPv6 用 `host:port` 全等分支安全覆盖）。
- 布局隔离：usePanelResize 支持实例 storageKey（asset 窗口用 `myshelltool:layout-asset:v1`）；ui.js asset 模式右栏折叠不写共享 localStorage；App.vue **单实例条件创建** panelResize（双实例会互相覆盖 CSS 变量）。
- 防呆：`assetWindowBoot.bootAssetWindowConnect` 找不到资产绝不 fallback 到 selectedAsset（防静默连错主机）；workbench.initialize({mode:'asset'}) 跳过 MCP/sync（MCP 审批弹窗只在主窗口，避免双弹窗竞争）。
- capabilities/default.json：windows 加 `asset-*` glob + create-webview-window/set-focus/unminimize/destroy 四权限（官方 ACL 文档核实拼写）。

### 3. 验收验证
- `npm run build`: exit 0（三轮独立复核）
- `node tests/ui-smoke.mjs`: exit 0（主窗口无 query 路径零行为变化）
- `node tests/ui-host-key.mjs`: exit 0（**已适配守卫**：测试先注入 connecting 会话再触发事件——守卫引入时的测试回归已修复）
- 手工走查（tauri:dev，待用户执行）：拖出/右键开窗、自动连接、右栏收起独立、关窗确认、双窗口监控不串流、重复开聚焦
- 行数债务备注：ConnectionSidebar.vue 936 / GlobalModals.vue 1091 / sessions.js 1095 均为既有超标 + 本功能最小接线，列入后续拆分清单（ConnectionSidebar 的 useAssetDnd 拆分预案见上文 Baseline）

---

## 2026-09-09 — 终端 tab 跨窗口拖出/合并（会话所有权迁移，Windows Terminal 式）

### 1. 架构决策
在多窗口基建上实现终端 tab 拖出成独立窗口 / 拖回 drop 合并。核心是**跨 webview 会话所有权迁移**：xterm 实例无法跨窗口搬运，采用「源窗口导出 scrollback（buffer.normal 纯文本，末 2000 行）→ localStorage 中转（`myshelltool:session-handoff:<sessionId>`，TTL 60s、1.5MB 字符预算 500 行步进截断）→ `await unlisten`（先解绑防双写）→ `removeSessionEntry`（仅移 UI，**不 ssh_disconnect**，会话在 Rust 后端存活）→ 目标窗口 `adoptSession` 重建 xterm + `registerSessionStream` + 100 行分块 rAF 回放」。Rust 依旧零改动。事前五角色评审发现 4 Critical（drop/dragend 双路径竞争丢会话、主窗菜单自吞、asset 窗口拖出行为未定义、registerSessionStream 契约错误），全部以「drop 仲裁 + 窗口角色门控 + 单一互斥迁移原语」修正后实施。

### 2. 关键实现
- `src/lib/sessionHandoff.js`（372 行新模块）：五事件协议（`session-handoff-tearoff/drop-claimed/merge-request/merge-ready/merge-push`，Tauri event 全局广播 + `sourceWindowId` 防自吞）；**仲裁**：目标 drop 先发 DROP_CLAIMED，源 tearoff 延迟 250ms 执行，窗口期内可取消；**角色门控矩阵**：TEAROFF 仅 asset 窗口且 assetId 匹配、MERGE_PUSH 仅主窗口、DROP_CLAIMED/MERGE_REQUEST 全窗口；`migrateSessionOut` inFlight 互斥；merge pull 协议 5s 超时先试 adoptFromStorage 再报错；asset 窗口迁移后空会话自动关窗。
- `src/stores/sessions.js`：**提炼 `createTerminalForAsset` 私有 helper**（connectSelected 与 adoptSession 共用——消除「同一概念两份实现」漂移风险，危险粘贴守卫 attachCustomKeyEventHandler 随 helper 天然保留）；`adoptSession`（幂等、资产重解析、分块回放、onSessionConnected 在 setActiveSession 后、不重注入 OSC 7）；`session.unlisten` 组合句柄 async 化。净增 69 行（1164 行，超标债务已注释）。
- `src/composables/useDragOutsideViewport.js`：从 ConnectionSidebar 提炼「document dragenter/dragleave(relatedTarget===null) 标记 + 坐标判定取或」composable，ConnectionSidebar 复用净减 21 行。
- `TerminalTabs.vue`（499 行）：connected tab 拖拽源（HANDOFF_MIME）+ tab 栏 drop 合并目标（仅认自定义 MIME）+ 右键「移回主窗口」（仅 asset 窗口且 main 存活，防自吞丢会话）；`TerminalSurface.vue` 按 getWindowRole 分流（main 拖出=scheduleTearOff / asset 拖出=pushSessionToMainWindow）。
- 顺手修复 GlobalModals 存量缺陷：hostKeyVerify accept/deny 后 closeModal 补发错误路由（ui-host-key 测试阻断项）。

### 3. 验收验证
- `npm run build` / `node tests/ui-smoke.mjs` / `node tests/ui-host-key.mjs` 全 exit 0（A、B 两轮 review-guard 各自独立复跑一致）。
- review-guard 五场景端到端推演全部闭环无丢会话：主窗拖出空白/拖出已开窗、asset→主窗 drop（三重防护收敛：inFlight 互斥+幂等+5s 超时兜底）、主窗→asset 窗 drop、asset 拖出窗外 push 回主窗、快速拖出再拖回。
- 手工走查（tauri:dev，待用户执行）：见下方清单。
- 已知限制：回放含 SGR 颜色/样式（v2.4 起 `sessionHandoff.ts` 走 SerializeAddon 序列化，早期纯文本褪色问题已修）；alt 屏与软换行仍不保留；迁移瞬间 1-2s 输出丢失；跨 WebView2 自定义 MIME 若丢失 drop 合并失效（菜单兜底）；溢出折叠 tab 不可拖；adoptSession 并发 in-flight 互斥待加（当前路径不可达）。
- 手动走查清单：①主窗 connected tab 拖出窗外→独立窗接管+文本回放；②拖回主窗 tab 栏 drop 合并；③asset 窗 tab 拖出窗外→回主窗+空窗自动关；④tab 右键「移回主窗口」（主窗 tab 上应无此项）；⑤同资产窗已开时拖出=投递已有窗口；⑥迁移后 Ctrl+V 危险粘贴仍弹确认；⑦vim 中拖出的 alt 屏表现（已知限制验证）。

### 4. 用户实测返修（同日第二轮）
- **「会话迁移失败，已尝试重新连接」高频出现**：根因为迁移数据走 localStorage 跨窗口中转——此前「跨窗口共享」的证据只证明同分区持久化（重启可见），WebView2 多窗口间 localStorage 并不保证实时互通，新窗口 boot adopt 读到空 → 兜底重连。**修复：中转通道改 Rust 侧内存**（lib.rs AppState 新增 `session_handoff: Mutex<HashMap>` + `session_handoff_put`/`session_handoff_take` 两命令，TTL 60s 惰性清理，take 即原子删除）——同进程强一致，彻底消除 renderer storage 语义依赖。附带防呆：boot adopt 包 try/catch（防 App 层 .catch 吞错后无提示也不重连）、take 返 null 先查本窗口 sessions（防误杀已接管会话）。
- **独立窗口删除顶部 tab 条**（用户需求：窗口即会话，标题栏已标识资产）：TerminalSurface 新增 `showTabs` prop（默认 true，主窗口零变化）+ `.no-tabs` grid 收行；pull 协议（跨窗口 drop 合并：HANDOFF_MIME/DROP_CLAIMED/MERGE_REQUEST/MERGE_READY/250ms 仲裁）因失去全部触发路径整体移除（WebView2 跨窗口自定义 MIME 本就不可靠）。
- **回迁入口改标题栏按钮**：「移回主窗口」（Undo2）置于 AssetWindowShell 标题栏（mainAlive 探测、非 connected disabled、push 后空窗自动关），主窗口已关时拒绝迁移防会话无主。
- 验证：cargo check / npm run build / ui-smoke / ui-host-key 全 exit 0；六个死符号 grep 0 残留。
