<script setup lang="ts">
/**
 * 网关部署向导（v0.22，modal type: gatewayWizard）——左竖导航两页：
 *
 * 「部署网关」（本机·运维侧，默认页）三段式：
 *   第一步：VPS 信息 → 生成挂载密钥对（gateway_wizard_start）→ 展示公钥与
 *           已代入公钥的 docker run 模板，用户拿去 VPS 部署 relay。
 *   第二步：用户部署完成后 → 填挂载口/白名单/加密口令/保存路径 →
 *           gateway_wizard_finish（probe 抓 pin + 防轮换 + 导出 bundle）→
 *           gateway_pretrust_host_key 预写 known_hosts → 经**常规 saveAsset 链**
 *           保存网关资产（tag=gateway，私钥走 credentials 常规路径——同步/删除/
 *           Gist 备份全复用，勿另建存储：计划 §5 决策 16 / v3-L5）。
 *   第三步：完成指引（跳板机侧三条后续命令：拷文件 / 导入 / schtasks 模板）。
 * 「导入配置」（跳板机侧）：GatewayImportPanel 自包含（装完 mst 后 GUI 点选，
 *   v0.20.1 补齐，替代 CLI 拷文件+手敲命令）。
 *
 * 布局镜像 SettingsPanelContent（左竖导航 + 右内容 + 页面标题，设计语言一致），
 * 样式拆至 GatewayWizardContent.modal.scss（500 行 SFC 硬上限）。
 * 两页 v-show 常驻：切页不丢部署表单/导入面板的轮询状态。
 * 表单状态自持（非 useModalFormSync）：本弹窗不进 dispatchModalSubmit 分发。
 */
import { computed, ref } from 'vue';
import { Import, Rocket } from 'lucide-vue-next';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import GatewayImportPanel from '@/components/shell/GatewayImportPanel.vue';
import { useWorkbenchStore } from '@/stores/workbench';
import {
  invokeBackend,
  gatewayWizardStart,
  gatewayWizardFinish,
  gatewayPretrustHostKey
} from '@/services/backend';

const store = useWorkbenchStore();

// 顶层页签：「deploy」本机走向导部署 |「import」跳板机侧（已装 mst）GUI 导入
const tab = ref<'deploy' | 'import'>('deploy');
const step = ref<1 | 2 | 3>(1); // 1 收集 VPS → 2 部署+导出 → 3 完成指引
const busy = ref(false);
const error = ref('');

// 第一步表单
const vpsHost = ref('');
const vpsPort = ref(22222);
const vpsUsername = ref('mount');

// 第一步产物
const mountPublicKey = ref('');
const dockerTemplate = ref('');

// 第二步表单
const remotePort = ref(2222);
const allowedTargetsText = ref('');
const password = ref('');
const password2 = ref('');
const bundlePath = ref('');


// 完成指引的跳板机导入命令（含 bundle 文件名）
const importCommand = computed(() =>
  bundlePath.value ? `myshelltool.exe --gateway --import "${bundlePath.value.split(/[\\/]/).pop()}"` : ''
);
// 跳板机服务化模板（占位路径避免盘符假设——按实际安装位置替换；详细说明见部署指南）
const schtasksCommand =
  'schtasks /Create /TN myshelltool-gateway /TR "\\"<myshelltool安装路径>\\myshelltool.exe\\" --gateway" /SC ONSTART /RU %USERNAME% /RP';

function copyText(text: string, what: string) {
  navigator.clipboard?.writeText(text).then(
    () => store.announce(`已复制${what}`, { level: 'success' }),
    () => store.announce('复制失败，请手动选择复制', { level: 'warn' })
  );
}

async function runStep1() {
  if (!vpsHost.value.trim() || !vpsUsername.value.trim()) {
    error.value = 'VPS 地址与挂载用户名不能为空';
    return;
  }
  if (!(Number(vpsPort.value) > 0 && Number(vpsPort.value) < 65536)) {
    error.value = 'VPS SSH 端口须为 1-65535';
    return;
  }
  error.value = '';
  busy.value = true;
  try {
    const r = await gatewayWizardStart(vpsHost.value.trim(), Number(vpsPort.value), vpsUsername.value.trim());
    mountPublicKey.value = r.mount_public_key;
    dockerTemplate.value = r.docker_template;
    step.value = 2;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function chooseSavePath() {
  const path = await invokeBackend<string | null>('plugin:dialog|save', {
    title: '保存网关 bundle（加密文件）',
    defaultPath: 'gateway-bundle.json',
    filters: [{ name: 'Gateway Bundle', extensions: ['json'] }]
  });
  if (path) bundlePath.value = path;
}

async function runStep2() {
  if (!bundlePath.value.trim()) {
    error.value = '请先选择 bundle 保存位置';
    return;
  }
  if (password.value.length < 6) {
    error.value = '加密口令至少 6 位（bundle 含 VPS 凭据与网关私钥）';
    return;
  }
  if (password.value !== password2.value) {
    error.value = '两次输入的加密口令不一致';
    return;
  }
  error.value = '';
  busy.value = true;
  try {
    const r = await gatewayWizardFinish({
      vpsHost: vpsHost.value.trim(),
      vpsPort: Number(vpsPort.value),
      vpsUsername: vpsUsername.value.trim(),
      remotePort: Number(remotePort.value),
      allowedTargets: allowedTargetsText.value
        .split(/[\n,，;；]/)
        .map(s => s.trim())
        .filter(Boolean),
      password: password.value,
      savePath: bundlePath.value.trim()
    });
    bundlePath.value = r.bundle_path;

    // 预写 known_hosts（"VPS:挂载口" → 网关 host key）：全链路首次连接零弹窗
    await gatewayPretrustHostKey(r.known_host_port, r.known_key_type, r.known_key_hex);

    // 网关资产走常规 saveAsset（凭据常规链；tag gateway 驱动徽标与连接拦截）
    await store.saveAsset(
      {
        name: `网关 ${r.asset_host}:${r.asset_port}`,
        host: r.asset_host,
        port: r.asset_port,
        username: 'gateway',
        auth_method: 'PrivateKey',
        group: '未分组',
        tags: ['gateway'],
        jump_host: null,
        private_key_path: null
      },
      { privateKey: r.client_private_key }
    );

    store.announce('网关 bundle 已导出，网关资产已保存', { level: 'success' });
    step.value = 3;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="gw-panel" data-gateway-wizard>
    <!-- 左侧竖导航：本机走向导部署 | 跳板机（已装 mst）GUI 导入（规格镜像 settings-nav） -->
    <nav class="gw-nav" aria-label="网关操作">
      <button
        type="button"
        class="nav-item"
        :class="{ active: tab === 'deploy' }"
        :aria-current="tab === 'deploy' ? 'page' : undefined"
        @click="tab = 'deploy'"
      >
        <Rocket :size="14" />
        部署网关
      </button>
      <button
        type="button"
        class="nav-item"
        :class="{ active: tab === 'import' }"
        :aria-current="tab === 'import' ? 'page' : undefined"
        @click="tab = 'import'"
      >
        <Import :size="14" />
        导入配置
      </button>
    </nav>

    <!-- 右侧内容区：页面标题与左导航选中项呼应；页面副题回答「这一页在哪台机器、给谁用」 -->
    <main class="gw-content">
      <!-- ═══ 部署页（默认）：本机·运维侧三段式向导 ═══ -->
      <section v-show="tab === 'deploy'" class="tab-pane">
        <header class="page-title"><Rocket :size="15" />部署网关</header>
        <p class="page-sub">在本机（运维侧）操作——第一次搭建网关从本页开始。</p>

        <!-- 使用流程：真实三段顺序（先部署后导入才能连），编号承载顺序信息 -->
        <div class="flow-steps">
          <div class="flow-step">
            <span class="flow-no" aria-hidden="true">1</span>
            <span><b>本机 · 部署</b>：在本页填公网 VPS 信息，向导生成部署命令，导出加密配置。</span>
          </div>
          <div class="flow-step">
            <span class="flow-no" aria-hidden="true">2</span>
            <span><b>跳板机 · 导入</b>：把左侧「导入配置」页发给客户内网那台机器，输入口令一键启动。</span>
          </div>
          <div class="flow-step">
            <span class="flow-no" aria-hidden="true">3</span>
            <span><b>本机 · 连接</b>：新建资产时跳板机选网关，终端 / 文件 / 隧道直达客户内网。</span>
          </div>
        </div>

    <!-- 第一步：VPS 信息 → 生成挂载密钥 -->
    <template v-if="step === 1">
      <p class="muted">
        在任意一台公网服务器上部署 relay（Docker 一条命令），本机即可经它访问客户内网跳板机。
        VPS 前提：已装 Docker。
      </p>
      <label class="stack"><span>VPS 地址</span>
        <AppInput v-model="vpsHost" placeholder="例如 117.72.25.115" data-gateway-field="vps-host" />
      </label>
      <label class="stack"><span>SSH 控制口（relay 默认 22222）</span>
        <AppInput v-model="vpsPort" type="number" data-gateway-field="vps-port" />
      </label>
      <label class="stack"><span>挂载用户名（relay 内置账户，默认 mount）</span>
        <AppInput v-model="vpsUsername" data-gateway-field="vps-user" />
      </label>
      <p v-if="error" class="form-error">{{ error }}</p>
      <div class="wizard-actions">
        <AppButton variant="primary" :disabled="busy" @click="runStep1">
          {{ busy ? '生成中…' : '生成挂载密钥' }}
        </AppButton>
      </div>
    </template>

    <!-- 第二步：部署确认 + 导出 -->
    <template v-else-if="step === 2">
      <p class="muted">第一步：在 VPS 上执行以下命令部署 relay（公钥已代入）：</p>
      <div class="code-block">
        <pre>{{ dockerTemplate }}</pre>
        <button type="button" class="link-btn" @click="copyText(dockerTemplate, 'docker 命令')">复制</button>
      </div>
      <p class="muted">
        部署并启动后，回到这里完成导出。挂载密钥公钥（已包含在命令里）：
        <code class="key-fragment">{{ mountPublicKey.slice(0, 50) }}…</code>
      </p>
      <label class="stack"><span>挂载口（VPS 上对外暴露的端口，默认 2222）</span>
        <AppInput v-model="remotePort" type="number" data-gateway-field="mount-port" />
      </label>
      <label class="stack">
        <span>内网目标白名单（可选，空 = 允许全部；每行一条，如 10.0.0.0/8:22）</span>
        <textarea
          v-model="allowedTargetsText"
          class="native-input"
          rows="2"
          spellcheck="false"
          placeholder="10.0.0.0/8&#10;192.168.0.0/16:3389"
          data-gateway-field="allowed-targets"
        ></textarea>
      </label>
      <label class="stack"><span>bundle 加密口令（至少 6 位，拷去跳板机导入时要用）</span>
        <AppInput v-model="password" type="password" data-gateway-field="password" />
      </label>
      <label class="stack"><span>确认口令</span>
        <AppInput v-model="password2" type="password" data-gateway-field="password2" />
      </label>
      <label class="stack"><span>bundle 保存位置</span>
        <div class="path-row">
          <span class="path-value">{{ bundlePath || '未选择' }}</span>
          <AppButton :disabled="busy" @click="chooseSavePath">选择位置…</AppButton>
        </div>
      </label>
      <p v-if="error" class="form-error" style="white-space:pre-wrap">{{ error }}</p>
      <div class="wizard-actions">
        <AppButton :disabled="busy" @click="step = 1">上一步</AppButton>
        <AppButton variant="primary" :disabled="busy" @click="runStep2">
          {{ busy ? '正在探测并导出…' : '我已在 VPS 部署完成，生成 bundle' }}
        </AppButton>
      </div>
    </template>

    <!-- 第三步：完成指引 -->
    <template v-else>
      <p class="muted">bundle 已导出、网关资产已保存。接下来在<b>跳板机</b>上完成三步：</p>
      <ol class="steps">
        <li>
          把加密文件拷到跳板机（U 盘 / 受控 IM）：<code>{{ bundlePath }}</code>
        </li>
        <li>
          安装 myshelltool 后，在有终端的 PowerShell 中执行：
          <div class="code-block">
            <pre>{{ importCommand }}</pre>
            <button type="button" class="link-btn" @click="copyText(importCommand, '导入命令')">复制</button>
          </div>
        </li>
        <li>
          注册开机自启（无需登录桌面也运行；<b>必须与导入时的 Windows 用户一致</b>）：
          <div class="code-block">
            <pre>{{ schtasksCommand }}</pre>
            <button type="button" class="link-btn" @click="copyText(schtasksCommand, '服务化命令')">复制</button>
          </div>
        </li>
      </ol>
      <p class="muted">
        完成后在本机新建内网机器资产，「跳板机」填
        <code>{{ vpsHost }}:{{ remotePort }}</code> 即可经网关连接（终端/文件/隧道全通）。
        完整文档见 docs/网关模式部署指南.md。
      </p>
    </template>
      </section>

      <!-- ═══ 导入页：跳板机侧「服务器 + 口令」（实时状态卡 + 两栏 + 高级折叠）═══ -->
      <section v-show="tab === 'import'" class="tab-pane">
        <header class="page-title"><Import :size="15" />导入配置</header>
        <p class="page-sub">
          在跳板机（客户内网的 Windows 机器）上的 myshelltool 中打开本页：填运维方给的服务器地址与口令，一键启动网关。
        </p>
        <GatewayImportPanel />
      </section>
    </main>
  </div>
</template>

<style scoped lang="scss" src="./GatewayWizardContent.modal.scss"></style>
