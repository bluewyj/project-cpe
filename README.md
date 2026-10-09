# UDX710 后台管理系统

这是一个为市面上成品 5G CPE 设备开发的高级后台管理系统项目。本项目旨在为现有的 5G CPE 设备提供更多高级功能和可玩性，让用户能够更好地控制和定制他们的 5G CPE 设备。

基于 Rust + Axum + zbus 的 5G/LTE 调制解调器后端服务，通过 ofono D-Bus 接口控制。

powered by Cursor Claude Opus 4.5 & Sonnet 4.5 & OpenAI GPT-5.1/5.2

欢迎pr 和 issue 看到后会尽快处理。

## 本仓库相对原作者的改动

本仓库基于原作者 [1orz/project-cpe](https://github.com/1orz/project-cpe) 衍生修改。  
当前适配版本：**3.5.2**。

详细说明见：**[CHANGES-FROM-UPSTREAM.md](./CHANGES-FROM-UPSTREAM.md)**

摘要：

- **构建架构已变**：原作者 **musl**（`aarch64-unknown-linux-musl`）→ 本仓库 **glibc**（`aarch64-unknown-linux-gnu.2.27`；CPU 仍为 aarch64）
- 修复 USB/RNDIS 共享上网（关闭 SFP、清除 `usb0→sipa_eth0` FORWARD DROP、守护 `usb0`）
- **3.4.4**：PC 对端（`192.168.66.2`）丢失时自动 usb0 软复位（不做危险的 UDC bounce）
- **3.4.5/3.4.6**：IPv6 共享自愈（地址 + 策略路由回程探测；不再 `ip6tables -F`）
- **3.4.7–3.4.9**：关闭破坏性自愈（无自动 soft-reset、无周期性 iptables -F）
- 交叉编译锁定 **glibc 2.27**（避免默认 gnu 链到 2.28+ 导致 `GLIBC_2.28 not found`）
- 修复 OTA `meta.json` UTF-8 BOM 解析失败
- 修复网页 Commit 显示为 `unknown`
- **3.5.0**：自定义 APN 持久化；制式自适应（曾用 shell 补丁）
- **3.5.1**：制式自适应迁入后端 Watchdog；取消 `nr_lte_switch.sh` 补丁与 OTA 下发
- **3.5.2**：修复 IPv6 共享自愈（支持 `ip` 压缩地址 `::`；转发与前缀解析解耦）

成品 OTA：`release/udx710-ota-3.5.2.tar.gz`（**必须**用 `gnu.2.27` 构建；默认 gnu 会 GLIBC 过高导致服务起不来）

## 免责声明

本项目仅供技术交流和学习使用，不得用于任何非法用途。任何使用本项目造成的任何后果，均与本项目无关，由使用者自行承担。

且目前测试通过的设备仅有：

- 华为5G 通讯壳 P50 P60 Mate系列

其余设备由于缺少设备，本人未做测试，你如果手里有多余的设备，可尝试*小心的*尝试使用，但不提供任何 担保或保证。对设备的造成任何的损坏 本人不承担任何责任。

或者愿意捐献设备来测试，可联系我，我将在第一时间进行测试并更新本项目。

## ⚖️ 开源协议声明

本项目采用 GNU General Public License v3.0 (GPLv3) 开源协议

鉴于目前大部分人对版权意识薄弱，特此声明

本项目采用 GPLv3 开源协议，您可以自由使用、研究、修改本软件，但必须保留所有版权声明和许可证声明，并且公开源代码，任何基于本项目的衍生作品也必须使用 GPLv3 协议。

### ✅ 您可以

- 自由使用、研究、修改本软件
- 分发本软件的副本
- 分发修改后的版本

### ⚠️ 但您必须

1. **保留所有版权声明和许可证声明** - 不得删除或修改原作者的版权信息
2. **公开源代码** - 如果您分发本软件或其修改版本，必须以 GPLv3 协议公开完整源代码
3. **使用相同协议** - 任何基于本项目的衍生作品也必须使用 GPLv3 协议
4. **标注修改** - 修改后的版本必须明确标注修改内容和修改日期
5. **提供许可证副本** - 分发时必须附带完整的 GPLv3 许可证文本

### ❌ 严禁以下行为

- **禁止闭源商业化**：不得将本项目或其衍生版本闭源后进行商业销售
- **禁止删除版权信息**：不得移除原作者的版权声明
- **禁止更改许可证**：不得将本项目改为其他许可证（如 MIT、Apache 等）
- **禁止专有软件化**：不得将本项目整合到专有/闭源软件中而不开源

## 🚀 快速开始

### 构建后端（锁定 glibc 2.27）

设备系统是 **glibc 2.27**。必须用：

```bash
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.27
```

**不要**用默认 `aarch64-unknown-linux-gnu`（常链到 GLIBC 2.28+，OTA 后网页/服务全挂）。

```bash
# 一键构建（内部走 zigbuild .2.27）+ UPX + OTA 包
./scripts/build.sh

# Windows
scripts\build-windows.bat

# 仅打包已有产物为 OTA（含 udx710 + www）
./scripts/pack-ota.sh
```

OTA 包内容：`meta.json`、`udx710`、`www/`。  
制式自适应在后端 Watchdog 内执行；应用 OTA 时会停用并删除遗留的 `nr_lte_switch.sh`。

### 构建前端

```bash
cd frontend && pnpm install && pnpm run build
```

### 部署

```bash
./scripts/deploy.sh
```

设备默认监听 **80** 端口（`udx710 -p 80`），管理页：`http://192.168.66.1/`。

### USB 共享：电脑端静态地址（无 DHCP）

本项目**不向电脑下发 DHCP**。RNDIS/`usb0` 网段由设备固定为 `192.168.66.0/24`，电脑需**手动填写** IPv4；若要用 IPv6 上网，也需按蜂窝前缀**手动填写** IPv6（设备侧 Watchdog 只负责补 `usb0` 全球地址与回程路由，不发 RA/DHCPv6）。

#### 拓扑（勿改错）

| 角色 | 接口 | 地址角色 |
|------|------|----------|
| 通讯壳（CPE） | `usb0` | IPv4：`192.168.66.1/24`（后端守护，勿改） |
| 电脑 | RNDIS /「远程 NDIS」 | IPv4：建议 `192.168.66.2/24`，网关 `192.168.66.1` |
| 通讯壳 | `sipa_eth0` | 蜂窝 IPv6 `/64`（运营商下发，前缀会变） |
| 通讯壳 | `usb0` | 同前缀的全球 IPv6（自愈写入，如 EUI-64 `/128`） |
| 电脑 | 同上 RNDIS | 同前缀内另选一个主机地址 + 默认路由指向壳 |

不要把设备默认路由改成经 `192.168.66.2`；电脑也不要占用 `192.168.66.1`。

#### IPv4（电脑）

推荐固定：

- **IP**：`192.168.66.2`
- **掩码**：`255.255.255.0`（`/24`）
- **网关**：`192.168.66.1`
- **DNS**：可用 `192.168.66.1`，或公共 DNS（如 `223.5.5.5` / `1.1.1.1`）

同一网段也可用 `192.168.66.3`～`254`（避开 `.1`）。

**Windows（设置 → 网络 → 以太网/RNDIS → 编辑 IP → 手动）** 或管理员 PowerShell：

```powershell
# 将 "远程 NDIS..." 换成实际适配器名（Get-NetAdapter 查看）
New-NetIPAddress -InterfaceAlias "远程 NDIS 兼容虚拟小端口" -IPAddress 192.168.66.2 -PrefixLength 24 -DefaultGateway 192.168.66.1
Set-DnsClientServerAddress -InterfaceAlias "远程 NDIS 兼容虚拟小端口" -ServerAddresses 223.5.5.5,1.1.1.1
```

**Linux：**

```bash
sudo ip addr add 192.168.66.2/24 dev <rndis接口>
sudo ip route replace default via 192.168.66.1 dev <rndis接口>
```

填好后应能打开 `http://192.168.66.1/`，并经 USB 共享访问公网 IPv4。

#### IPv6（电脑，可选）

1. 在壳上确认蜂窝前缀（与 `usb0` 全球地址前 64 位一致），例如：
   ```bash
   adb shell "ip -6 addr show sipa_eth0; ip -6 addr show usb0"
   ```
   若 sipa 为 `2409:8d5c:240:47cf::1/64`，则前缀为 `2409:8d5c:240:47cf`。管理页「网络接口」里看 `usb0` 全球地址亦可。
2. 电脑在**同一 `/64`** 内自选主机地址，**不要**与下列冲突：
   - sipa 上的 `…::1`（或运营商已用的地址）
   - `usb0` 上已有的全球地址（多为 EUI-64）
3. 网关：优先用壳在 `usb0` 上的**链路本地**（`fe80::…`，需指定接口），或该口的全球 IPv6。
4. DNS：可用运营商 DNS，或公共如 `2400:3200::1` / `2606:4700:4700::1111`。

示例（前缀请换成你设备上的实际值）：

- **地址**：`2409:8d5c:240:47cf::2/64`（示例；也可用 `::3`、`::100` 等）
- **网关**：壳 `usb0` 的 `fe80::cee8:acff:fec0:0`（以实机为准）

**Windows：** 同一适配器开启 IPv6 → 手动，填「IPv6 地址 / 子网前缀长度 64 / 网关 / DNS」。

**Linux：**

```bash
# <前缀>、<壳fe80>、<rndis接口> 换成实机值
sudo ip -6 addr add <前缀>::2/64 dev <rndis接口>
sudo ip -6 route replace default via <壳fe80> dev <rndis接口>
```

说明：蜂窝重拨后 `/64` 前缀可能变化，需按新前缀改电脑静态 IPv6。设备 **3.5.2+** 会按压缩/`::` 地址正确自愈 `usb0` 侧；电脑侧仍须手工配置。

---

## 🔧 环境配置

### Windows（推荐）

```bat
scripts\install-zigbuild.bat
scripts\build-windows.bat
```

### macOS / Linux

```bash
# 1. Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustup target add aarch64-unknown-linux-gnu

# 2. Zig + cargo-zigbuild（用于锁定 glibc 2.27）
# 安装 zig: https://ziglang.org/download/
cargo install cargo-zigbuild

# 3. 构建
cd backend
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.27
```

> 旧的 brew `aarch64-unknown-linux-gnu` 交叉 gcc **不足以**保证 2.27 ABI，本仓库以 zigbuild 为准。

---

## 📡 ofono D-Bus 接口

### 核心接口

| 接口 | 说明 |
|------|------|
| `org.ofono.Manager` | 调制解调器管理 |
| `org.ofono.Modem` | Modem 属性和控制 |
| `org.ofono.NetworkRegistration` | 网络注册状态 |
| `org.ofono.SimManager` | SIM 卡管理 |
| `org.ofono.ConnectionManager` | 数据连接管理 |
| `org.ofono.VoiceCallManager` | 语音通话管理 |
| `org.ofono.MessageManager` | 短信管理 |

### 常用 D-Bus 命令

```bash
# 查看 Modem 属性
dbus-send --system --print-reply \
  --dest=org.ofono /ril_0 org.ofono.Modem.GetProperties

# 查看网络状态
dbus-send --system --print-reply \
  --dest=org.ofono /ril_0 org.ofono.NetworkRegistration.GetProperties

# 查看 SIM 卡信息
dbus-send --system --print-reply \
  --dest=org.ofono /ril_0 org.ofono.SimManager.GetProperties

# 设置飞行模式
dbus-send --system --print-reply \
  --dest=org.ofono /ril_0 org.ofono.Modem.SetProperty \
  string:"Online" variant:boolean:false

# 发送 AT 指令
dbus-send --system --print-reply \
  --dest=org.ofono /ril_0 org.ofono.Modem.SendAtcmd \
  string:"AT+CGSN"
```

### 监控 D-Bus

```bash
# 监听 ofono 发出的所有信号
dbus-monitor --system "sender='org.ofono'"

# 监听发给 ofono 的调用
dbus-monitor --system "destination='org.ofono'"

# 监听短信信号
dbus-monitor --system "interface='org.ofono.MessageManager'"
```

---

## 📶 频段锁定

仅供参考 真实性有待考证，请以实际设备为准

### LTE (4G) 频段

| 频段 | 位掩码 | 说明 |
|------|--------|------|
| B1 | 1 | FDD 2100MHz |
| B3 | 4 | FDD 1800MHz |
| B5 | 16 | FDD 850MHz |
| B8 | 128 | FDD 900MHz |
| B38 | 32 (TDD) | TDD 2600MHz |
| B40 | 128 (TDD) | TDD 2300MHz |
| B41 | 256 (TDD) | TDD 2500MHz |

### NR (5G) 频段

| 频段 | 位掩码 | 说明 |
|------|--------|------|
| N1 | 1 (FDD) | 2100MHz |
| N28 | 512 (FDD) | 700MHz |
| N41 | 16 (TDD) | 2500MHz |
| N77 | 128 (TDD) | 3700MHz |
| N78 | 256 (TDD) | 3500MHz |
| N79 | 512 (TDD) | 4500MHz |

### AT 指令

```bash
# 查询当前 LTE 频段
AT+SPLBAND=0

# 查询当前 NR 频段
AT+SPLBAND=3

# 锁定 LTE B1+B3
AT+SPLBAND=1,0,0,0,0,5,0

# 锁定 NR N78
AT+SPLBAND=2,0,0,256,0

# 解锁所有频段
AT+SPLBAND=1,0,0,0,0,0,0
AT+SPLBAND=2,0,0,0,0
```

---

## 📚 API 接口文档

### 基础信息
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/health` | GET | 健康检查 |
| `/api/device` | GET | 设备信息 (IMEI/ICCID/型号) |
| `/api/device/imeisv` | GET | 软件版本号 |
| `/api/sim` | GET | SIM 卡信息 |
| `/api/sim/slot` | GET | SIM 卡槽状态 |
| `/api/sim/slot/switch` | POST | 切换 SIM 卡槽 |

### 网络状态
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/network` | GET | 网络注册信息 |
| `/api/network/interfaces` | GET | 网络接口信息 |
| `/api/network/signal-strength` | GET | 信号强度 |
| `/api/network/nitz` | GET | 网络时间 |
| `/api/network/operators` | GET | 运营商列表 |
| `/api/network/operators/scan` | GET | 扫描运营商 (耗时) |
| `/api/network/register-manual` | POST | 手动注册运营商 |
| `/api/network/register-auto` | POST | 自动注册运营商 |
| `/api/cells` | GET | 基站信息 |
| `/api/location/cell-info` | GET | 基站定位参数 |
| `/api/qos` | GET | QoS 信息 |

### 模块控制
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/data` | GET/POST | 数据连接开关 |
| `/api/roaming` | GET/POST | 漫游开关 |
| `/api/airplane-mode` | GET/POST | 飞行模式开关 |
| `/api/radio-mode` | GET/POST | 射频模式 (4G/5G/自动) |
| `/api/band-lock` | GET/POST | 频段锁定 |
| `/api/cell-lock` | GET/POST | 小区锁定 |
| `/api/cell-lock/unlock-all` | POST | 解锁所有小区 |
| `/api/apn` | GET/POST | APN 配置（POST 会持久化到 ofono `defult_apn`/`gprs`） |
| `/api/usb-mode` | GET/POST | USB 模式切换 |
| `/api/usb-advance` | POST | 高级 USB 模式设置 |

### 通话功能
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/calls` | GET | 当前通话列表 |
| `/api/call/dial` | POST | 拨打电话 |
| `/api/call/hangup` | POST | 挂断指定电话 |
| `/api/call/hangup-all` | POST | 挂断所有电话 |
| `/api/call/answer` | POST | 接听来电 |
| `/api/call/volume` | GET/POST | 通话音量设置 |
| `/api/call/forwarding` | GET/POST | 呼叫转移设置 |
| `/api/call/settings` | GET/POST | 通话设置 |
| `/api/call/history` | GET | 通话记录列表 |
| `/api/call/history/{id}` | DELETE | 删除指定通话记录 |
| `/api/call/history/clear` | POST | 清空通话记录 |

### 短信功能
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/sms/send` | POST | 发送短信 |
| `/api/sms/list` | GET | 短信列表 |
| `/api/sms/conversation` | GET | 短信会话列表 |
| `/api/sms/stats` | GET | 短信统计 |
| `/api/sms/clear` | POST | 清空短信 |

### IMS/VoLTE
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/ims/status` | GET | IMS 状态 |
| `/api/voicemail/status` | GET | 语音信箱状态 |

### 系统信息
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/stats` | GET | 系统统计（网速/内存/运行时间） |
| `/api/stats/cpu` | GET | CPU 信息 |
| `/api/connectivity` | GET | 网络连通性检查 |
| `/api/system/reboot` | POST | 重启系统 |
| `/api/at` | POST | 执行 AT 指令 |

### Webhook 配置
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/webhook/config` | GET/POST | Webhook 配置管理 |
| `/api/webhook/test` | POST | 测试 Webhook |

### OTA 更新
| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/ota/status` | GET | OTA 更新状态 |
| `/api/ota/upload` | POST | 上传 OTA 包 (最大 50MB) |
| `/api/ota/apply` | POST | 应用 OTA 更新 |
| `/api/ota/cancel` | POST | 取消 OTA 更新 |

---

## 🛠 开发指南

### D-Bus 操作序列化

所有 D-Bus/AT 操作必须通过 `with_serial` 串行执行：

```rust
use crate::serial::with_serial;

pub async fn send_at_command(conn: &Connection, cmd: &str) -> zbus::Result<String> {
    with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.Modem").await?;
        proxy.call("SendAtcmd", &(cmd)).await
    }).await
}
```

### API 响应格式

```rust
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub status: String,   // "ok" 或 "error"
    pub message: String,
    pub data: Option<T>,
}
```

---

## 📦 依赖

- **zbus 5.x** - D-Bus 客户端
- **tokio 1.48** - 异步运行时
- **axum 0.8** - Web 框架
- **rusqlite 0.32** - SQLite (bundled)
- **tower-http 0.6** - HTTP 中间件

---

## license 许可证

GNU General Public License v3.0
