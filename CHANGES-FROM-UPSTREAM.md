# 相对原作者（1orz/project-cpe）的改动说明

> 基线：原项目 [1orz/project-cpe](https://github.com/1orz/project-cpe)  
> 本分支版本：`3.5.0`  
> 更新日期：2026-07-31

本仓库在原作者开源项目基础上，针对 **华为/展锐 UDX710 5G 通讯壳（RNDIS USB 共享）** 场景做了稳定性与构建兼容性修复。  
原作者版权与 GPLv3 协议保持不变。

---

## 0. 架构 / 构建目标变更（重要）

原作者正式构建走 **musl**；本仓库成品改为 **glibc (gnu)**，并锁定兼容设备上的 glibc 2.27。

| 项目 | 原作者（1orz） | 本仓库成品 |
|------|----------------|------------|
| C 库 / ABI | **musl**（静态） | **glibc**（动态） |
| Rust target | `aarch64-unknown-linux-musl` | **`aarch64-unknown-linux-gnu.2.27`** |
| 链接要求 | 不依赖系统 glibc | 必须兼容设备 **glibc 2.27**（勿用默认 gnu→易带 2.28+） |
| CPU 架构 | aarch64 | aarch64（不变） |
| OTA `meta.json` → `arch` | `aarch64-unknown-linux-musl` | **`aarch64-unknown-linux-gnu`**（校验字段已同步改掉） |

说明：

- **CPU 仍是 aarch64**，没有换成 x86 / ARM32。  
- **真正变的是 libc**：`musl` → `gnu/glibc`。  
- 设备系统本身是 **glibc 2.27**，因此本仓库用 `gnu.2.27` 打出的包可直接跑；若用 `cargo zigbuild` 的默认 `aarch64-unknown-linux-gnu`（常链到 glibc 2.28+），OTA 后会出现 `GLIBC_2.28 not found`，服务起不来、网页打不开。  
- Windows 推荐构建命令：

```bash
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.27
```

---

## 1. USB 共享稳定性（核心）

### 问题
设备通过 USB（RNDIS）把蜂窝网络共享给电脑时，常见：

- `usb0` 异常、USB 偶发断连  
- Windows「以太网 2」显示无网络 / 无网络访问权限  
- 电脑无法经 USB 访问外网  
- **过一段时间后网页打不开**（后端进程其实还在）

### 根因
1. 展锐 **SFP 硬件加速** 在 USB + IPv6 场景下易触发 `sfp init mac fail`，导致异常  
2. 开机脚本 `/etc/route_test.sh` 会：
   - `echo 1 > /proc/net/sfp/enable`
   - `echo 1 > /proc/net/sfp/tether_scheme`
   - `iptables/ip6tables -I FORWARD -i usb0 -o sipa_eth0 -j DROP`  
3. SFP 关闭后必须走内核软件转发；上述 **FORWARD DROP** 会直接阻断「电脑 → 设备 → 蜂窝」共享上网  
4. `connman` 启用 gadget tethering 时可能再次打开 SFP  
5. RNDIS 二层易僵死：`usb0` RX frame 错误持续偏高 → PC ARP Incomplete / 169.254 → 打不开后端

### 改动（代码）
主要文件：`backend/src/usb_switch.rs`、`backend/src/main.rs`、`backend/src/dbus.rs`

新增 / 强化 `ensure_usb_tether_health()`，在启动与 Watchdog 周期中：

| 动作 | 说明 |
|------|------|
| 关闭 SFP | `/proc/net/sfp/enable`、`tether_scheme` 置 `0` |
| 关闭 `sipa_usb0` | 避免与 `usb0` 冲突 |
| 确保 `usb0` 为 `192.168.66.1/24` | 管理口与共享网段稳定 |
| 清掉偶发的 `192.168.67.1` | 避免 connman 改网段干扰 |
| 确保 `192.168.66.0/24` MASQUERADE | 不调用 `connmanctl tether`（会改网段/权限失败） |
| **清除** `FORWARD -i usb0 -o sipa_eth0 -j DROP` | 恢复 USB→蜂窝转发 |
| 持久修补 `/etc/route_test.sh` | 开机不再打开 SFP；注释 DROP 规则（必要时 remount rw） |
| **PC 对端丢失软恢复**（3.4.4） | 曾 ping 通 `192.168.66.2` 后连续失败，且 usb0 仍有 RX → 短 down/up 软复位（**不做 UDC bounce**） |
| **IPv6 共享自愈**（3.4.5/3.4.6，**3.5.2 修**） | 重建 `usb0` 全球地址；补 policy rule（181/200）与前缀 `/64`；用 `ip -6 route get` 探测回程；**3.5.2**：解析支持 `::` 压缩，转发与前缀解耦（修复 sipa 为 `…::1` 时自愈整段跳过） |
| **soft-reset 防抖**（3.4.7） | PC 不可达时不为 IPv6 触发 soft-reset；提高失败阈值/冷却；`ifconfig` 软复位（避免反复 bounce 打僵 RNDIS） |
| **断流热修**（3.4.8/3.4.9） | **移除**自动 soft-reset；Watchdog/**数据连接**不再 `iptables -F`，只清 usb0 DROP；IPv6 地址先补后清 |
| **APN 持久化**（3.5.0） | 设置 APN 写入 `defult_apn`/`gprs`；空 APN 优先磁盘恢复 |
| **制式自适应**（3.5.0→3.5.1） | 固定 LTE/NR 尊重偏好；4G 卡不促切 5G；**3.5.1 起迁入后端 Watchdog**，取消 `nr_lte_switch.sh` |

**明确不做的事：**  
- 不把设备默认路由强制改为 `via 192.168.66.2`（电脑）  
- 不自动 UDC bounce / 不自动 `connmanctl tether off/on`（实测会弄死枚举或改到 67 网段）

---

## 2. 构建 / 发布兼容性

> 详见上文 **「0. 架构 / 构建目标变更」**。

### 问题
1. 原作者用 **musl**；本仓库改为 **gnu/glibc**（与设备系统库一致，且便于 Windows 下 `cargo zigbuild`）。  
2. 若再用 `cargo zigbuild` **默认** `aarch64-unknown-linux-gnu`，会链到 **GLIBC_2.28+**，而通讯壳为 **glibc 2.27**，OTA 后 `GLIBC_2.28 not found`，服务全挂。

### 改动
- **libc / target**：`aarch64-unknown-linux-musl` → **`aarch64-unknown-linux-gnu.2.27`**
- OTA `arch` 校验同步改为 `aarch64-unknown-linux-gnu`
- 推荐命令：

```bash
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.27
```

- OTA `meta.json` 使用 **UTF-8 无 BOM**（PowerShell 默认 UTF-8 BOM 会导致  
  `Invalid meta.json: expected value at line 1 column 1`）

---

## 3. 版本 / Commit 显示

### 问题
网页 OTA 页「Commit」长期显示 `unknown`（本机无 `.git` 或无 `git` 命令）。

### 改动
`backend/build.rs` 按顺序解析：

1. 环境变量 `GIT_COMMIT` / `GIT_BRANCH`  
2. `git rev-parse`（兼容 Windows Git 路径）  
3. 仓库根目录 `COMMIT` / `BRANCH` 文件  
4. 最后回退为 `build-{version}`

本发布写入 `COMMIT=5b703ba`，API `/api/ota/status` 中 `current_commit` 可正常显示。

---

## 4. 成品包

| 文件 | 说明 |
|------|------|
| `release/udx710-ota-3.4.3.tar.gz` | OTA 更新包（后端 + 前端 + meta.json） |
| 版本 | `3.4.3` |
| 架构 | `aarch64-unknown-linux-gnu.2.27`（glibc；原作者为 musl） |

部署方式：

- 网页 OTA 上传上述 tar.gz；或  
- ADB：`adb push udx710 /home/root/udx710 && chmod 755 ... && 重启服务`

---

## 5. 未改动的部分

- 原作者功能框架、ofono/D-Bus、前端整体 UI 结构  
- GPLv3 协议与原作者版权声明  
- 非 USB 共享相关的业务逻辑（短信、通话、射频模式等）原则上未做无关重构  

---

## 6. 验证结论（本机实测）

- ADB 连接正常  
- `http://192.168.66.1/` 可打开，版本 `3.4.3` / Commit `5b703ba`  
- 以太网 2（RNDIS）可经设备共享上网（ping `8.8.8.8` 成功）  
- SFP 保持关闭；FORWARD DROP 已清除  

---

## 致谢

感谢原作者 [1orz](https://github.com/1orz) 开源 [project-cpe](https://github.com/1orz/project-cpe)。  
本仓库为衍生修改版，仅用于个人设备适配与问题修复，请遵守 GPLv3。
