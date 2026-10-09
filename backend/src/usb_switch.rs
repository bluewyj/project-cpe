/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-07 07:33:11
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:46:20
 * @FilePath: /udx710-backend/backend/src/usb_switch.rs
 * @Description: 
 * 
 * Copyright (c) 2025 by 1orz, All Rights Reserved. 
 */
//! USB 模式热切换模块
//! 
//! 通过 USB configfs 实现 NCM/ECM/RNDIS 模式的热切换，无需重启
//!
//! ## 技术背景
//! 
//! 本模块基于对设备固件的深入分析实现，参考了以下关键脚本：
//! - `/usr/bin/PRJ_SRT880.sh` - 启动时 USB 模式初始化
//! - `/usr/bin/usbenum.sh` - 运行时 USB 模式切换
//! - `/etc/route_test.sh` - IP 协议和网络接口初始化
//!
//! ## IPA 硬件加速
//! 
//! 展锐 UDX710 使用 IPA (Internet Packet Accelerator) 硬件加速：
//! - `/sys/devices/platform/soc/soc:ipa/2b300000.pamu3/pamu3_protocol` - 协议类型
//! - `/sys/devices/platform/soc/soc:ipa/2b300000.pamu3/max_dl_pkts` - 下行包批量数
//!
//! ## 模式定义
//!
//! | mode | 类型   | VID    | PID    | ADB | 说明 |
//! |------|--------|--------|--------|-----|------|
//! | 1    | NCM    | 0x1782 | 0x4040 | Yes | NCM + 调试接口 |
//! | 2    | ECM    | 0x1782 | 0x4039 | Yes | ECM + 调试接口 |
//! | 3    | RNDIS  | 0x1782 | 0x4038 | Yes | RNDIS + 调试接口 |

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lazy_static::lazy_static;
use tracing::{info, warn};

/// USB 模式配置
#[derive(Debug, Clone)]
pub struct UsbModeConfig {
    pub vid: &'static str,
    pub pid: &'static str,
    pub configuration: &'static str,
    pub pamu3_protocol: Option<&'static str>,
    pub functions: &'static str,
    pub bcd_device: &'static str,
    /// 是否需要启用 USB 共享（RNDIS 需要）
    pub usb_share_enable: bool,
}

impl UsbModeConfig {
    /// 获取指定模式的配置
    /// 
    /// # 模式说明
    /// - 1: NCM (CDC-NCM) + ADB + 调试接口
    /// - 2: ECM (CDC-ECM) + ADB + 调试接口
    /// - 3: RNDIS + ADB + 调试接口
    pub fn get(mode: u8) -> Option<Self> {
        match mode {
            // NCM 模式
            1 => Some(Self {
                vid: "0x1782",
                pid: "0x4040",
                configuration: "ncm",
                pamu3_protocol: Some("NCM"),
                functions: "ncm.gs0",
                bcd_device: "0x0404",
                usb_share_enable: false,
            }),
            // ECM 模式
            2 => Some(Self {
                vid: "0x1782",
                pid: "0x4039",
                configuration: "ecm",
                pamu3_protocol: None, // ECM 不需要设置 pamu3_protocol
                functions: "ecm.gs0",
                bcd_device: "0x0404",
                usb_share_enable: false,
            }),
            // RNDIS 模式
            3 => Some(Self {
                vid: "0x1782",
                pid: "0x4038",
                configuration: "rndis",
                pamu3_protocol: Some("RNDIS"),
                functions: "rndis.gs4",
                bcd_device: "0x0404",
                usb_share_enable: true, // RNDIS 需要启用 USB 共享
            }),
            _ => None,
        }
    }
}

/// USB configfs 路径
const GADGET_PATH: &str = "/sys/kernel/config/usb_gadget/g1";
const CONFIG_PATH: &str = "/sys/kernel/config/usb_gadget/g1/configs/b.1";
const FUNCTIONS_PATH: &str = "/sys/kernel/config/usb_gadget/g1/functions";
const UDC_PATH: &str = "/sys/kernel/config/usb_gadget/g1/UDC";

/// IPA 硬件加速路径
const PAMU3_PROTOCOL_PATH: &str = "/sys/devices/platform/soc/soc:ipa/2b300000.pamu3/pamu3_protocol";
const PAMU3_MAX_DL_PKTS_PATH: &str = "/sys/devices/platform/soc/soc:ipa/2b300000.pamu3/max_dl_pkts";

/// SFP (Sprd Forward Path) 转发加速路径
const SFP_ENABLE_PATH: &str = "/proc/net/sfp/enable";
const SFP_TETHER_SCHEME_PATH: &str = "/proc/net/sfp/tether_scheme";

/// 系统 USB 网络初始化脚本（开机后会开启 SFP，需持久修补）
const ROUTE_TEST_SCRIPT_PATH: &str = "/etc/route_test.sh";

/// slog_bridge 日志传输路径
const SLOG_TRANSPORT_PATH: &str = "/sys/module/slog_bridge/parameters/log_transport";

/// AT 指令设备路径
const AT_DEVICE_PATH: &str = "/dev/stty_lte30";

/// 默认 USB 网络接口 IP 地址
const USB_INTERFACE_IP: &str = "192.168.66.1";
const USB_INTERFACE_MAC: &str = "CC:E8:AC:C0:00:00";
/// PC 侧 RNDIS 地址（共享网段对端）
/// connman 偶发写到的错误网段，需清掉以免干扰
const USB_BAD_ALIAS_IP: &str = "192.168.67.1";

/// 默认 UDC 名称
const DEFAULT_UDC: &str = "29100000.dwc3";

/// IPv6 共享自愈冷却（避免地址抖动时反复写地址）
const USB_IPV6_RECOVER_COOLDOWN: Duration = Duration::from_secs(120);

struct UsbLinkMonitor {
    last_ipv6_recover: Option<Instant>,
}

impl UsbLinkMonitor {
    fn new() -> Self {
        Self {
            last_ipv6_recover: None,
        }
    }
}

lazy_static! {
    static ref USB_LINK_MONITOR: Mutex<UsbLinkMonitor> = Mutex::new(UsbLinkMonitor::new());
}

/// 写入文件的辅助函数
fn write_to_file(path: &str, content: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    file.flush()?;
    Ok(())
}

/// 发送 AT 指令到 modem（直接写入设备）
/// 
/// 用于发送 USB 模式相关的 AT 指令，如 AT+SPASENGMD
fn send_at_command_direct(cmd: &str) -> io::Result<()> {
    if !Path::new(AT_DEVICE_PATH).exists() {
        // AT 设备不存在，静默跳过
        return Ok(());
    }
    
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(AT_DEVICE_PATH)?;
    
    // 添加换行符
    let cmd_with_newline = format!("{}\r\n", cmd);
    file.write_all(cmd_with_newline.as_bytes())?;
    file.flush()?;
    Ok(())
}

/// 设置 USB 共享模式
/// 
/// 通过 AT+SPASENGMD 指令控制 USB 共享：
/// - enable=true: RNDIS 模式需要启用
/// - enable=false: ECM/NCM/MBIM 模式禁用
fn set_usb_share_mode(enable: bool) -> io::Result<()> {
    let value = if enable { "1" } else { "0" };
    let cmd = format!("AT+SPASENGMD=\"#dsm_usb_share_enable\",{}", value);
    send_at_command_direct(&cmd)
}


/// 设置 slog_bridge 日志传输
/// 
/// 控制是否通过 USB vser 接口传输日志
fn set_log_transport(enable: bool) -> io::Result<()> {
    if Path::new(SLOG_TRANSPORT_PATH).exists() {
        let value = if enable { "1" } else { "0" };
        write_to_file(SLOG_TRANSPORT_PATH, value)?;
    }
    Ok(())
}

/// 禁用 SFP 硬件转发加速
///
/// 展锐 SFP 在 USB 共享 + IPv6 场景下会因 MAC 表查找失败（`sfp init mac fail`）
/// 导致 usb0 RX frame 错误飙升，最终触发 USB 断连。关闭 SFP 后 IPv4/IPv6 仍走
/// 内核协议栈经 usb0 转发，仅失去硬件加速。
pub fn disable_sfp_acceleration() -> io::Result<()> {
    if Path::new(SFP_ENABLE_PATH).exists() {
        write_to_file(SFP_ENABLE_PATH, "0")?;
    }
    if Path::new(SFP_TETHER_SCHEME_PATH).exists() {
        write_to_file(SFP_TETHER_SCHEME_PATH, "0")?;
    }
    Ok(())
}

fn sfp_proc_enabled(path: &str) -> bool {
    fs::read_to_string(path)
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

/// 若 SFP 被系统重新开启则再次关闭，返回是否执行了关闭操作
pub fn ensure_sfp_disabled() -> io::Result<bool> {
    let needs_disable = (Path::new(SFP_ENABLE_PATH).exists() && sfp_proc_enabled(SFP_ENABLE_PATH))
        || (Path::new(SFP_TETHER_SCHEME_PATH).exists() && sfp_proc_enabled(SFP_TETHER_SCHEME_PATH));

    if needs_disable {
        disable_sfp_acceleration()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 尝试以可写方式写入根分区文件（必要时临时 remount rw）
fn write_rootfs_file(path: &Path, content: &str) -> io::Result<()> {
    if fs::write(path, content).is_ok() {
        return Ok(());
    }
    let _ = Command::new("mount")
        .args(["-o", "remount,rw", "/"])
        .output();
    let result = fs::write(path, content);
    let _ = Command::new("mount")
        .args(["-o", "remount,ro", "/"])
        .output();
    result
}

/// 持久修补 `/etc/route_test.sh`：禁止开机开启 SFP，并注释掉会阻断软件转发的 DROP 规则
///
/// 原脚本在 USB 初始化后会执行：
/// - `echo 1 > /proc/net/sfp/enable`
/// - `echo 1 > /proc/net/sfp/tether_scheme`
/// - `iptables/ip6tables -I FORWARD -i usb0 -o sipa_eth0 -j DROP`
///
/// SFP 关闭后必须走内核协议栈转发，上述 DROP 会直接导致 PC 无法经 USB 上网。
pub fn patch_route_test_disable_sfp() -> io::Result<bool> {
    let path = Path::new(ROUTE_TEST_SCRIPT_PATH);
    if !path.exists() {
        return Ok(false);
    }

    let original = fs::read_to_string(path)?;
    let mut patched = original
        .replace(
            "echo 1 > /proc/net/sfp/enable",
            "echo 0 > /proc/net/sfp/enable",
        )
        .replace(
            "echo 1 > /proc/net/sfp/tether_scheme",
            "echo 0 > /proc/net/sfp/tether_scheme",
        );

    patched = patched
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let should_comment = !trimmed.starts_with('#')
                && (trimmed.starts_with("iptables -I FORWARD -i usb0")
                    || trimmed.starts_with("ip6tables -I FORWARD -i usb0"));
            if should_comment {
                format!("# {}", line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !patched.ends_with('\n') {
        patched.push('\n');
    }

    if patched == original {
        return Ok(false);
    }

    write_rootfs_file(path, &patched)?;
    Ok(true)
}

fn interface_is_up(name: &str) -> bool {
    Command::new("ifconfig")
        .arg(name)
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.contains("UP"))
        .unwrap_or(false)
}

fn ensure_sipa_usb0_down() -> bool {
    if !interface_is_up("sipa_usb0") {
        return false;
    }
    let _ = Command::new("ifconfig")
        .args(["sipa_usb0", "down"])
        .output();
    true
}

fn ensure_usb0_address() -> bool {
    let has_ip = Command::new("ifconfig")
        .arg("usb0")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.contains(USB_INTERFACE_IP))
        .unwrap_or(false);

    if has_ip {
        return false;
    }

    let _ = Command::new("ifconfig")
        .args([
            "usb0",
            USB_INTERFACE_IP,
            "netmask",
            "255.255.255.0",
            "up",
        ])
        .output();
    true
}

/// 删除会阻断 USB→蜂窝转发的 DROP 规则（SFP 关闭后必须走软件转发）
///
/// Watchdog 只应调用本函数，**禁止**周期性 `iptables -F`（整表清空会打断连接）。
pub fn clear_usb_forward_drop_rules() -> bool {
    let mut acted = false;
    for cmd in ["iptables", "ip6tables"] {
        // 重复删除直到失败（规则可能被插入多次）
        loop {
            let ok = Command::new(cmd)
                .args(["-D", "FORWARD", "-i", "usb0", "-o", "sipa_eth0", "-j", "DROP"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !ok {
                break;
            }
            acted = true;
        }
    }
    acted
}

/// 去掉 connman tether 重启后偶发出现的 192.168.67.1/24
fn clear_usb0_bad_alias() -> bool {
    let has_alias = Command::new("ip")
        .args(["addr", "show", "dev", "usb0"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|t| t.contains(USB_BAD_ALIAS_IP))
        .unwrap_or(false);
    if !has_alias {
        return false;
    }
    Command::new("ip")
        .args(["addr", "del", &format!("{}/24", USB_BAD_ALIAS_IP), "dev", "usb0"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 确保 USB→蜂窝 MASQUERADE 存在（不调用 connman tether，避免改网段/权限问题）
fn ensure_usb66_masquerade() -> bool {
    let check = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-C",
            "POSTROUTING",
            "-s",
            "192.168.66.0/24",
            "-o",
            "sipa_eth0",
            "-j",
            "MASQUERADE",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if check {
        return false;
    }
    // connman 链优先；没有则直接挂 POSTROUTING
    let via_connman = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-A",
            "connman-POSTROUTING",
            "-s",
            "192.168.66.0/24",
            "-o",
            "sipa_eth0",
            "-j",
            "MASQUERADE",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if via_connman {
        return true;
    }
    Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-s",
            "192.168.66.0/24",
            "-o",
            "sipa_eth0",
            "-j",
            "MASQUERADE",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 读取接口上的全球 IPv6（addr, prefix_len）
fn iface_global_ipv6(iface: &str) -> Vec<(String, u8)> {
    let output = Command::new("ip")
        .args(["-6", "addr", "show", "dev", iface])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with("inet6 ") || !line.contains("scope global") {
            continue;
        }
        // inet6 2409:...:a1ec/64 scope global
        let Some(rest) = line.strip_prefix("inet6 ") else {
            continue;
        };
        let Some(token) = rest.split_whitespace().next() else {
            continue;
        };
        let mut parts = token.split('/');
        let Some(addr) = parts.next() else { continue };
        let Ok(plen) = parts.next().unwrap_or("128").parse::<u8>() else {
            continue;
        };
        out.push((addr.to_string(), plen));
    }
    out
}

/// 将 IPv6 地址展开为 8 个 hextet（支持 `::` 压缩与 zone id）
fn expand_ipv6_hextets(addr: &str) -> Option<Vec<String>> {
    let addr = addr.split('%').next()?.trim();
    if addr.is_empty() {
        return None;
    }

    let has_compress = addr.contains("::");
    if has_compress && addr.matches("::").count() != 1 {
        return None;
    }

    let (left, right) = if has_compress {
        let mut parts = addr.splitn(2, "::");
        (parts.next().unwrap_or(""), parts.next().unwrap_or(""))
    } else {
        (addr, "")
    };

    let parse_side = |side: &str| -> Option<Vec<String>> {
        if side.is_empty() {
            return Some(Vec::new());
        }
        side.split(':')
            .map(|h| {
                if h.is_empty() || h.len() > 4 || u16::from_str_radix(h, 16).is_err() {
                    None
                } else {
                    let t = h.trim_start_matches('0');
                    Some(if t.is_empty() {
                        "0".to_string()
                    } else {
                        t.to_string()
                    })
                }
            })
            .collect()
    };

    let left_parts = parse_side(left)?;
    let right_parts = parse_side(right)?;
    let used = left_parts.len() + right_parts.len();
    if used > 8 {
        return None;
    }
    if !has_compress && used != 8 {
        return None;
    }

    let missing = 8 - used;
    let mut out = Vec::with_capacity(8);
    out.extend(left_parts);
    out.extend(std::iter::repeat_with(|| "0".to_string()).take(missing));
    out.extend(right_parts);
    (out.len() == 8).then_some(out)
}

/// 取 IPv6 地址的 /64 前缀（4 个 hextet，如 `2409:8d5c:240:47cf`）
///
/// 支持内核/`ip` 常见的 `::` 压缩写法（如 `2409:8d5c:240:47cf::1`）。
fn ipv6_prefix_64(addr: &str) -> Option<String> {
    let hextets = expand_ipv6_hextets(addr)?;
    Some(format!(
        "{}:{}:{}:{}",
        hextets[0], hextets[1], hextets[2], hextets[3]
    ))
}

/// MAC → EUI-64 接口标识（与内核给 usb0 的全球地址一致）
fn mac_to_eui64_iid(mac: &str) -> Option<String> {
    let bytes: Option<Vec<u8>> = mac
        .split(|c| c == ':' || c == '-')
        .map(|h| u8::from_str_radix(h, 16).ok())
        .collect();
    let b = bytes?;
    if b.len() != 6 {
        return None;
    }
    Some(format!(
        "{:02x}{:02x}:{:02x}ff:fe{:02x}:{:02x}{:02x}",
        b[0] ^ 0x02,
        b[1],
        b[2],
        b[3],
        b[4],
        b[5]
    ))
}

fn read_usb0_mac() -> String {
    fs::read_to_string("/sys/class/net/usb0/address")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| USB_INTERFACE_MAC.to_lowercase())
}

fn write_sysctl(path: &str, value: &str) -> bool {
    if !Path::new(path).exists() {
        return false;
    }
    write_to_file(path, value).is_ok()
}

fn ensure_ipv6_forwarding() -> bool {
    let mut acted = false;
    for path in [
        "/proc/sys/net/ipv6/conf/all/forwarding",
        "/proc/sys/net/ipv6/conf/usb0/forwarding",
        "/proc/sys/net/ipv6/conf/sipa_eth0/forwarding",
    ] {
        let cur = fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if cur != "1" && write_sysctl(path, "1") {
            acted = true;
        }
    }
    acted
}

/// 模拟“从蜂窝口进来、目标为前缀内主机”的路由查询；健康时应走 usb0（table 181/200）
fn ipv6_return_path_via_usb0(prefix: &str) -> bool {
    let probe = format!("{}:1234:5678:9abc:def0", prefix);
    let output = Command::new("ip")
        .args([
            "-6",
            "route",
            "get",
            &probe,
            "from",
            "2001:4860:4860::8888",
            "iif",
            "sipa_eth0",
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    output.contains("dev usb0")
}

fn ensure_ipv6_policy_rules() -> bool {
    let rules = Command::new("ip")
        .args(["-6", "rule"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let mut acted = false;
    if !rules.contains("iif sipa_eth0 lookup 181") {
        acted |= Command::new("ip")
            .args([
                "-6",
                "rule",
                "add",
                "iif",
                "sipa_eth0",
                "lookup",
                "181",
                "priority",
                "422",
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }
    if !rules.contains("iif usb0 lookup 181") {
        acted |= Command::new("ip")
            .args([
                "-6",
                "rule",
                "add",
                "iif",
                "usb0",
                "lookup",
                "181",
                "priority",
                "442",
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }
    if !rules.contains("lookup 200") {
        acted |= Command::new("ip")
            .args(["-6", "rule", "add", "lookup", "200", "priority", "2000"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }
    acted
}

fn install_usb0_prefix_routes(prefix: &str) -> bool {
    let route_prefix = format!("{}::/64", prefix);
    let mut acted = false;
    // main + 策略表：回程必须能命中 usb0
    for args in [
        vec![
            "-6",
            "route",
            "replace",
            route_prefix.as_str(),
            "dev",
            "usb0",
            "metric",
            "1024",
        ],
        vec![
            "-6",
            "route",
            "replace",
            route_prefix.as_str(),
            "dev",
            "usb0",
            "table",
            "200",
            "metric",
            "1024",
        ],
        vec![
            "-6",
            "route",
            "replace",
            route_prefix.as_str(),
            "dev",
            "usb0",
            "table",
            "181",
            "metric",
            "1024",
        ],
    ] {
        let ok = Command::new("ip")
            .args(&args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            acted = true;
        }
    }
    acted
}

fn restore_usb0_global_addr(prefix: &str, usb_addrs: &[(String, u8)]) -> bool {
    let mac = read_usb0_mac();
    let Some(iid) = mac_to_eui64_iid(&mac) else {
        return false;
    };
    // 与内核展示一致：末段不补前导零，避免无谓「地址变更」抖动
    let usb_global = normalize_ipv6_hextets(&format!("{}:{}", prefix, iid));
    info!(
        addr = %usb_global,
        prefix = %prefix,
        "USB tether: restore usb0 global IPv6"
    );

    // 先 add/replace，再删「错误前缀」旧地址，避免短暂无全球地址
    let addr_spec = format!("{}/128", usb_global);
    let mut added = Command::new("ip")
        .args(["-6", "addr", "replace", &addr_spec, "dev", "usb0"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !added {
        added = Command::new("ip")
            .args(["-6", "addr", "add", &addr_spec, "dev", "usb0"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }

    for (old, plen) in usb_addrs {
        let old_prefix_ok = ipv6_prefix_64(old)
            .map(|p| p.eq_ignore_ascii_case(prefix))
            .unwrap_or(false);
        // 同前缀已有地址保留；只清其它前缀残留
        if old_prefix_ok {
            continue;
        }
        let _ = Command::new("ip")
            .args([
                "-6",
                "addr",
                "del",
                &format!("{}/{}", old, plen),
                "dev",
                "usb0",
            ])
            .output();
    }

    added
}

fn normalize_ipv6_hextets(addr: &str) -> String {
    addr.split(':')
        .map(|h| {
            let t = h.trim_start_matches('0');
            if t.is_empty() {
                "0"
            } else {
                t
            }
        })
        .collect::<Vec<_>>()
        .join(":")
}

/// 蜂窝有 IPv6 时，确保 usb0 地址 + 策略路由回程（跑久后 table 181/200 丢失会导致电脑无 IPv6）
///
/// **稳定版策略**：只补地址/路由，绝不 soft-reset / UDC bounce。
fn ensure_usb0_ipv6_tether() -> bool {
    ensure_usb0_ipv6_tether_inner()
}

fn ensure_usb0_ipv6_tether_inner() -> bool {
    // 转发与前缀解析解耦：即使暂时解不出前缀，也尽量打开 forwarding
    let mut acted = ensure_ipv6_forwarding();

    let sipa_addrs = iface_global_ipv6("sipa_eth0");
    let Some((sipa_addr, _)) = sipa_addrs.first() else {
        return acted;
    };
    let Some(prefix) = ipv6_prefix_64(sipa_addr) else {
        warn!(
            addr = %sipa_addr,
            "USB tether: cannot parse IPv6 /64 prefix from sipa_eth0"
        );
        return acted;
    };

    let usb_addrs = iface_global_ipv6("usb0");
    let usb_ok = usb_addrs.iter().any(|(addr, _)| {
        ipv6_prefix_64(addr)
            .map(|p| p.eq_ignore_ascii_case(&prefix))
            .unwrap_or(false)
    });

    acted |= ensure_ipv6_policy_rules();

    let in_cooldown = USB_LINK_MONITOR
        .lock()
        .ok()
        .and_then(|m| m.last_ipv6_recover)
        .map(|t| t.elapsed() < USB_IPV6_RECOVER_COOLDOWN)
        .unwrap_or(false);

    if !usb_ok && !in_cooldown {
        if restore_usb0_global_addr(&prefix, &usb_addrs) {
            if let Ok(mut monitor) = USB_LINK_MONITOR.lock() {
                monitor.last_ipv6_recover = Some(Instant::now());
            }
            acted = true;
        }
    }

    // 健康时不做 route replace（避免 Watchdog 每轮都误报 acted）
    if ipv6_return_path_via_usb0(&prefix) {
        return acted;
    }

    info!(prefix = %prefix, "USB tether: IPv6 return path not via usb0, repairing");
    acted |= install_usb0_prefix_routes(&prefix);
    acted |= ensure_ipv6_policy_rules();
    acted
}

/// 已验证安全：软复位 usb0（不做 UDC bounce / 不碰 connman tether）
///
/// 使用短 down/up 刷新 RNDIS 网卡状态；避免长时间 down 导致主机侧枚举异常。
/// USB 共享健康维护：关 SFP、关 sipa_usb0、确保 usb0 IP、清除转发 DROP、修补 route_test.sh、
/// 维护 66 网段 NAT、按需补 usb0 全球 IPv6 / 策略回程。
///
/// 拓扑：手机壳走蜂窝上网，经 USB/NAT 共享给电脑。
/// 不要把默认路由改到 192.168.66.2（那会抢走蜂窝出口，导致以太网2无互联网）。
/// **禁止**自动 soft-reset / UDC bounce / connman tether 重开（会弄死 RNDIS 或改网段）。
///
/// 返回是否执行了任何修正动作。
pub fn ensure_usb_tether_health() -> io::Result<bool> {
    let mut acted = false;

    if ensure_sfp_disabled()? {
        acted = true;
    }
    if ensure_sipa_usb0_down() {
        acted = true;
    }
    if ensure_usb0_address() {
        acted = true;
    }
    if clear_usb0_bad_alias() {
        acted = true;
    }
    if clear_usb_forward_drop_rules() {
        acted = true;
    }
    if ensure_usb66_masquerade() {
        acted = true;
    }
    if ensure_usb0_ipv6_tether() {
        acted = true;
    }
    if patch_route_test_disable_sfp()? {
        acted = true;
    }

    Ok(acted)
}

/// 删除 CDC 功能
fn remove_cdc(function: &str) -> io::Result<()> {
    let path = format!("{}/{}", FUNCTIONS_PATH, function);
    if Path::new(&path).exists() {
        fs::remove_dir(&path)?;
    }
    Ok(())
}

/// 删除所有 CDC 功能
fn remove_all_cdc() -> io::Result<()> {
    let cdcs = vec![
        "rndis.gs4",
        "ecm.gs0",
        "ecm.gs1",
        "ecm.gs2",
        "ecm.gs3",
        "ncm.gs0",
        "ncm.gs1",
        "ncm.gs2",
        "ncm.gs3",
        "mbim.gs0",
    ];
    
    for cdc in cdcs {
        let _ = remove_cdc(cdc); // 忽略错误，继续删除其他
    }
    Ok(())
}

/// 删除所有配置链接
fn remove_all_links() -> io::Result<()> {
    for i in 0..=15 {
        let link = format!("{}/f{}", CONFIG_PATH, i);
        if Path::new(&link).exists() {
            let _ = fs::remove_file(&link); // 忽略错误
        }
    }
    Ok(())
}

/// 创建 gser 功能
fn create_gser_functions() -> io::Result<()> {
    let gsers = vec![
        "vser.gs0",
        "ffs.adb",
        "gser.gs0",
        "gser.gs1",
        "gser.gs2",
        "gser.gs3",
        "gser.gs4",
        "gser.gs5",
        "gser.gs6",
        "gser.gs7",
    ];
    
    for gser in gsers {
        let path = format!("{}/{}", FUNCTIONS_PATH, gser);
        if !Path::new(&path).exists() {
            fs::create_dir_all(&path)?;
            // 设置权限为 755
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = fs::Permissions::from_mode(0o755);
                fs::set_permissions(&path, perms)?;
            }
        }
    }
    Ok(())
}

/// 读取序列号（基于 MAC 地址）
fn read_serial_number() -> String {
    // 尝试读取网络接口的 MAC 地址生成序列号
    let interfaces = vec!["eth0", "wlan0", "usb0", "enp0s3"];
    
    for iface in interfaces {
        let mac_path = format!("/sys/class/net/{}/address", iface);
        if let Ok(mac) = fs::read_to_string(&mac_path) {
            let mac = mac.trim().replace(":", "").to_uppercase();
            if mac.len() >= 12 {
                // 使用 MAC 地址作为序列号（16位：UDX + 12位MAC + 1位）
                return format!("UDX{}1", &mac[..12]);
            }
        }
    }
    
    // 默认值
    "UDXDEFAULT000000".to_string()
}

/// 读取硬件型号 ID（从设备树）
fn read_hardware_id() -> String {
    // 从设备树读取型号
    // 示例: "Spreadtrum UDX710_4h10 Board" -> "U710"
    if let Ok(model) = fs::read_to_string("/proc/device-tree/model") {
        let model = model.trim().to_uppercase();
        
        // 尝试提取型号编号
        if let Some(pos) = model.find("UDX") {
            // 提取 UDX 后面的3-4位数字
            let rest = &model[pos..];
            if rest.len() >= 7 { // "UDX710_"
                return rest[..7].replace("_", ""); // "UDX710"
            } else if rest.len() >= 6 { // "UDX710"
                return rest[..6].to_string(); // "UDX710"
            }
        }
        
        // 备用方案：如果只是包含 710
        if model.contains("710") {
            return "U710".to_string();
        }
    }
    
    // 默认值
    "UDX7".to_string()
}

/// 生成产品名称
fn generate_product_name() -> String {
    let sn = read_serial_number();
    let model_id = read_hardware_id();
    
    // 从序列号中提取后4位（如果长度足够）
    let sn_suffix = if sn.len() >= 4 {
        &sn[sn.len()-4..]
    } else {
        // 如果序列号太短，用0填充
        &format!("{:0<4}", sn)[..4]
    };
    
    format!("unisoc-5g-modem-{}00{}", model_id, sn_suffix)
}

/// 启动 adbd
fn start_adbd() -> io::Result<()> {
    let _ = Command::new("/bin/sh")
        .arg("/etc/init.d/adbd-init")
        .arg("start")
        .output()?;
    Ok(())
}

/// 等待 functionfs 挂载完成
/// 
/// adbd-init 是后台启动的，会挂载 functionfs 到 /dev/usb-ffs/adb
/// 必须等待挂载完成后才能启用 UDC，否则 UDC 绑定会失败
fn wait_for_functionfs_mount() -> Result<(), String> {
    const FFS_PATH: &str = "/dev/usb-ffs/adb";
    const MAX_RETRIES: u32 = 50;  // 最多等待 5 秒
    const RETRY_INTERVAL_MS: u64 = 100;
    
    for i in 0..MAX_RETRIES {
        // 检查 functionfs 是否已挂载
        // 通过检查 /dev/usb-ffs/adb 目录是否存在且可访问来判断
        if Path::new(FFS_PATH).exists() {
            // 额外检查：确保 ep0 文件存在（表示 functionfs 已完全就绪）
            let ep0_path = format!("{}/ep0", FFS_PATH);
            if Path::new(&ep0_path).exists() {
                // 再等待一小段时间确保 adbd 已打开 ep0
                std::thread::sleep(std::time::Duration::from_millis(200));
                return Ok(());
            }
        }
        
        if i < MAX_RETRIES - 1 {
            std::thread::sleep(std::time::Duration::from_millis(RETRY_INTERVAL_MS));
        }
    }
    
    // 即使超时也继续，不阻塞切换流程
    // 某些情况下 adbd 可能未启用，但其他功能仍可工作
    eprintln!("Warning: functionfs mount timeout, continuing anyway");
    Ok(())
}

/// 热切换 USB 模式（高级接口，立即生效无需重启）
///
/// 此函数实现完整的 USB 模式热切换，参考设备固件的 usbenum.sh 和 PRJ_SRT880.sh 脚本。
///
/// ## 切换流程
/// 1. 停止 adbd 服务
/// 2. 禁用 UDC (写入 "none")
/// 3. 删除所有配置链接和 CDC 功能
/// 4. 设置 IPA 硬件加速协议 (pamu3_protocol)
/// 5. 发送 AT 指令控制 USB 共享模式
/// 6. 配置 USB gadget (VID/PID/功能等)
/// 7. 创建功能符号链接
/// 8. 启动 adbd (如果是 multi_functions 模式)
/// 9. 启用 UDC
/// 10. 配置 USB 网络接口和 iptables 规则
///
/// ## 注意事项
/// - 热切换会导致 USB 连接短暂断开（约 1-2 秒）
/// - macOS 可能需要更长时间识别新设备
/// - 建议使用模式 1 (NCM) 以获得最佳兼容性
pub fn switch_usb_mode_advanced(mode: u8) -> Result<(), String> {
    let config = UsbModeConfig::get(mode)
        .ok_or_else(|| format!("Invalid USB mode: {}. Valid modes: 1=NCM, 2=ECM, 3=RNDIS, 4=NCM(no ADB)", mode))?;
    
    // **********************************************************
    // 提前读取 UDC 名称，避免禁用后 list 为空
    let udc_name_cached = get_udc_name();
    
    // 热切换不写入配置文件，仅临时生效
    // 如需永久保存，请使用 set_usb_mode_config() 函数
    
    // 1. 停止 adbd 服务
    let _ = stop_adbd();
    
    // 2. 禁用 UDC
    write_to_file(UDC_PATH, "none")
        .map_err(|e| format!("Failed to disable UDC: {}", e))?;
    
    // 等待 UDC 完全禁用
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    // 3. 删除所有链接和 CDC 功能
    remove_all_links()
        .map_err(|e| format!("Failed to remove links: {}", e))?;
    remove_all_cdc()
        .map_err(|e| format!("Failed to remove CDC functions: {}", e))?;
    
    // 4. 设置 IPA 硬件加速协议
    if let Some(protocol) = config.pamu3_protocol {
        if Path::new(PAMU3_PROTOCOL_PATH).exists() {
            write_to_file(PAMU3_PROTOCOL_PATH, protocol)
                .map_err(|e| format!("Failed to set pamu3_protocol: {}", e))?;
        }
    }
    
    // 5. 设置 max_dl_pkts (下行包批量数)
    if Path::new(PAMU3_MAX_DL_PKTS_PATH).exists() {
        let _ = write_to_file(PAMU3_MAX_DL_PKTS_PATH, "7");
    }
    
    // 6. 发送 AT 指令控制 USB 共享模式
    let _ = set_usb_share_mode(config.usb_share_enable);
    
    // 7. 确保 configfs 已挂载
    let _ = Command::new("mount")
        .args(["-t", "configfs", "none", "/sys/kernel/config"])
        .output();
    
    // 8. 设置 USB gadget 基本配置
    // 确保目录存在
    if !Path::new(GADGET_PATH).exists() {
        fs::create_dir_all(GADGET_PATH)
            .map_err(|e| format!("Failed to create gadget directory: {}", e))?;
    }
    
    write_to_file(&format!("{}/idVendor", GADGET_PATH), config.vid)
        .map_err(|e| format!("Failed to set VID: {}", e))?;
    write_to_file(&format!("{}/idProduct", GADGET_PATH), config.pid)
        .map_err(|e| format!("Failed to set PID: {}", e))?;
    write_to_file(&format!("{}/bcdDevice", GADGET_PATH), config.bcd_device)
        .map_err(|e| format!("Failed to set bcdDevice: {}", e))?;
    write_to_file(&format!("{}/bDeviceClass", GADGET_PATH), "0")
        .map_err(|e| format!("Failed to set bDeviceClass: {}", e))?;
    
    // 9. 设置字符串描述符
    let strings_path = format!("{}/strings/0x409", GADGET_PATH);
    if !Path::new(&strings_path).exists() {
        fs::create_dir_all(&strings_path)
            .map_err(|e| format!("Failed to create strings directory: {}", e))?;
    }
    
    let sn = read_serial_number();
    let product_name = generate_product_name();
    
    write_to_file(&format!("{}/serialnumber", strings_path), &sn)
        .map_err(|e| format!("Failed to set serial number: {}", e))?;
    write_to_file(&format!("{}/manufacturer", strings_path), "SOYEA")
        .map_err(|e| format!("Failed to set manufacturer: {}", e))?;
    write_to_file(&format!("{}/product", strings_path), &product_name)
        .map_err(|e| format!("Failed to set product name: {}", e))?;
    
    // 10. 设置配置描述符
    let config_strings_path = format!("{}/strings/0x409", CONFIG_PATH);
    if !Path::new(&config_strings_path).exists() {
        fs::create_dir_all(&config_strings_path)
            .map_err(|e| format!("Failed to create config strings directory: {}", e))?;
    }
    
    write_to_file(&format!("{}/configuration", config_strings_path), config.configuration)
        .map_err(|e| format!("Failed to set configuration: {}", e))?;
    write_to_file(&format!("{}/MaxPower", CONFIG_PATH), "500")
        .map_err(|e| format!("Failed to set MaxPower: {}", e))?;
    write_to_file(&format!("{}/bmAttributes", CONFIG_PATH), "0xc0")
        .map_err(|e| format!("Failed to set bmAttributes: {}", e))?;
    
    // 11. 创建主功能目录
    let function_path = format!("{}/{}", FUNCTIONS_PATH, config.functions);
    if !Path::new(&function_path).exists() {
        fs::create_dir_all(&function_path)
            .map_err(|e| format!("Failed to create function {}: {}", config.functions, e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            let _ = fs::set_permissions(&function_path, perms);
        }
    }
    
    // 12. 创建 gser/vser 功能
    create_gser_functions()
        .map_err(|e| format!("Failed to create gser functions: {}", e))?;
    
    // 13. 设置 MAC 地址
    let dev_addr_path = format!("{}/dev_addr", function_path);
    if Path::new(&dev_addr_path).exists() {
        let _ = write_to_file(&dev_addr_path, &USB_INTERFACE_MAC.to_lowercase());
    }
    // 设置 host_addr 以保证 RNDIS/NCM 正常枚举
    let host_addr_path = format!("{}/host_addr", function_path);
    if Path::new(&host_addr_path).exists() {
        // 为 host MAC 生成 01 后缀
        let mut parts: Vec<&str> = USB_INTERFACE_MAC.split(':').collect();
        if let Some(last) = parts.last_mut() {
            *last = "01";
        }
        let host_mac = parts.join(":").to_lowercase();
        let _ = write_to_file(&host_addr_path, &host_mac);
    }
    
    // 14. 创建符号链接（始终使用多功能模式，包含 ADB 和调试接口）
    create_multi_function_links(&config)?;

    // 15. 启动 adbd（始终启动）
    // adbd-init 会挂载 functionfs 到 /dev/usb-ffs/adb
    let _ = start_adbd();

    // 16. 等待 functionfs 挂载完成
    // adbd-init 是后台启动的，需要等待 functionfs 挂载完成后才能启用 UDC
    wait_for_functionfs_mount()?;

    // 17. 设置日志传输
    let _ = set_log_transport(true);
    
    // 18. 启用 UDC
    // 使用之前缓存的 UDC 名称写回，避免读取为空导致挂载失败
    write_to_file(UDC_PATH, &udc_name_cached)
        .map_err(|e| format!("Failed to enable UDC: {}", e))?;
    
    // 19. 等待 USB 设备被主机识别
    std::thread::sleep(std::time::Duration::from_millis(1000));
    
    // 20. 配置网络接口
    configure_usb_network()?;
    
    Ok(())
}

/// 创建多功能模式的符号链接
/// 
/// 包含：网络功能 + ADB + 多个串口 + vser
fn create_multi_function_links(config: &UsbModeConfig) -> Result<(), String> {
    // f1: 主网络功能 (ncm/ecm/rndis)
    std::os::unix::fs::symlink(
        format!("{}/{}", FUNCTIONS_PATH, config.functions),
        format!("{}/f1", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link main function: {}", e))?;
    
    // f2: gser.gs2 (AT 指令通道)
    std::os::unix::fs::symlink(
        format!("{}/gser.gs2", FUNCTIONS_PATH),
        format!("{}/f2", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link gser.gs2: {}", e))?;
    
    // f3: gser.gs0 (诊断通道)
    std::os::unix::fs::symlink(
        format!("{}/gser.gs0", FUNCTIONS_PATH),
        format!("{}/f3", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link gser.gs0: {}", e))?;
    
    // f4: vser.gs0 (虚拟串口/IQ 日志)
    std::os::unix::fs::symlink(
        format!("{}/vser.gs0", FUNCTIONS_PATH),
        format!("{}/f4", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link vser.gs0: {}", e))?;
    
    // f5: gser.gs3
    std::os::unix::fs::symlink(
        format!("{}/gser.gs3", FUNCTIONS_PATH),
        format!("{}/f5", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link gser.gs3: {}", e))?;
    
    // f6: ffs.adb (Android Debug Bridge)
    std::os::unix::fs::symlink(
        format!("{}/ffs.adb", FUNCTIONS_PATH),
        format!("{}/f6", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link ffs.adb: {}", e))?;
    
    // f7-f9: 更多串口通道
    std::os::unix::fs::symlink(
        format!("{}/gser.gs4", FUNCTIONS_PATH),
        format!("{}/f7", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link gser.gs4: {}", e))?;
    
    std::os::unix::fs::symlink(
        format!("{}/gser.gs5", FUNCTIONS_PATH),
        format!("{}/f8", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link gser.gs5: {}", e))?;
    
    std::os::unix::fs::symlink(
        format!("{}/gser.gs6", FUNCTIONS_PATH),
        format!("{}/f9", CONFIG_PATH)
    ).map_err(|e| format!("Failed to link gser.gs6: {}", e))?;
    
    Ok(())
}
fn get_udc_name() -> String {
    if let Ok(entries) = fs::read_dir("/sys/class/udc") {
        entries
            .filter_map(|e| e.ok())
            .next()
            .and_then(|e| e.file_name().into_string().ok())
            .unwrap_or_else(|| DEFAULT_UDC.to_string())
    } else {
        DEFAULT_UDC.to_string()
    }
}

/// 停止 adbd 服务
fn stop_adbd() -> io::Result<()> {
    let _ = Command::new("/bin/sh")
        .arg("/etc/init.d/adbd-init")
        .arg("stop")
        .output()?;
    Ok(())
}

/// 配置 USB 网络接口
/// 
/// 此函数实现完整的 USB 网络初始化，参考 /etc/route_test.sh 脚本。
/// 
/// ## 初始化流程
/// 1. 启用 connman gadget tethering
/// 2. 配置 usb0 接口 IP 和 MAC 地址
/// 3. 关闭 sipa_usb0 接口
/// 4. 禁用 SFP 硬件转发（保留 IPv6 软件转发，避免 sfp init mac fail）
/// 5. 标记配置完成
fn configure_usb_network() -> Result<(), String> {
    // 等待接口出现
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    // 1. 配置 connman gadget tethering
    // 先关闭再重新启用（避免 "Already enabled" 错误）
    let _ = Command::new("connmanctl")
        .args(["tether", "gadget", "off"])
        .output();
    
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    let _ = Command::new("connmanctl")
        .args(["disable", "gadget"])
        .output();
    
    std::thread::sleep(std::time::Duration::from_millis(200));
    
    // 重新启用
    let _ = Command::new("connmanctl")
        .args(["enable", "gadget"])
        .output();
    
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    let _ = Command::new("connmanctl")
        .args(["tether", "gadget", "on"])
        .output();
    
    std::thread::sleep(std::time::Duration::from_millis(300));
    
    // 2. 配置 usb0 接口
    // 等待接口出现并重试
    let max_retries = 5;
    for retry in 0..max_retries {
        // 检查接口是否存在
        let check = Command::new("ifconfig")
            .arg("-a")
            .output();
        
        if let Ok(output) = check {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if output_str.contains("usb0") || output_str.contains(USB_INTERFACE_IP) {
                break;
            }
        }
        
        if retry < max_retries - 1 {
            // 尝试添加 IP 地址
            let _ = Command::new("ifconfig")
                .args(["usb0", "add", USB_INTERFACE_IP])
                .output();
            
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    
    // 设置 IP 地址和子网掩码
    let _ = Command::new("ifconfig")
        .args(["usb0", USB_INTERFACE_IP, "netmask", "255.255.255.0"])
        .output();
    
    // 设置 MAC 地址
    let _ = Command::new("ifconfig")
        .args(["usb0", "hw", "ether", USB_INTERFACE_MAC])
        .output();
    
    // 启动接口（不要把默认路由改到 PC：手机壳应继续走蜂窝上网）
    let _ = Command::new("ip")
        .args(["link", "set", "dev", "usb0", "up"])
        .output();

    // 3-4. 关闭 sipa_usb0、禁用 SFP，并持久修补 route_test.sh
    if let Err(e) = ensure_usb_tether_health() {
        eprintln!("Warning: failed to ensure USB tether health: {}", e);
    }

    // 5. 标记配置完成
    let _ = fs::write("/tmp/sipa_usb0_ok", "");
    
    // 6. 输出当前网络配置到内核日志（用于调试）
    let _ = Command::new("sh")
        .args(["-c", "ifconfig > /dev/kmsg 2>/dev/null"])
        .output();
    
    Ok(())
}

/// USB 模式读取结果
pub struct UsbModeResult {
    pub mode: u8,
}

/// USB 模式配置文件路径
const USB_MODE_PERMANENT_FILE: &str = "/mnt/data/mode.cfg";
const USB_MODE_TEMPORARY_FILE: &str = "/mnt/data/mode_tmp.cfg";

/// 设置 USB 模式配置（写入配置文件，重启后生效）
///
/// # Arguments
/// * `mode` - USB 模式：
///   - 1: CDC-NCM
///   - 2: CDC-ECM
///   - 3: RNDIS
/// * `permanent` - true=永久模式（写入 mode.cfg），false=临时模式（写入 mode_tmp.cfg）
///
/// # Returns
/// 成功返回 Ok(())，失败返回错误信息
pub fn set_usb_mode_config(mode: u8, permanent: bool) -> Result<(), String> {
    // 验证模式值
    if !(1..=3).contains(&mode) {
        return Err(format!("Invalid USB mode: {}. Valid modes: 1=NCM, 2=ECM, 3=RNDIS", mode));
    }
    
    let config_file = if permanent {
        USB_MODE_PERMANENT_FILE
    } else {
        USB_MODE_TEMPORARY_FILE
    };
    
    // 写入配置文件（末尾添加换行符，与 echo 'x' > file 行为一致）
    fs::write(config_file, format!("{}\n", mode))
        .map_err(|e| format!("Failed to write USB mode config to {}: {}", config_file, e))?;
    
    Ok(())
}

/// 读取 USB 模式配置文件
///
/// # Returns
/// 返回一个包含当前硬件模式、永久模式和临时模式的结构
pub fn get_usb_mode_config() -> Result<UsbModeConfigResult, String> {
    // 1. 从 configfs 读取当前硬件实际运行的模式
    let current_hardware_mode = match get_current_usb_mode() {
        Ok(result) => Some(result.mode),
        Err(_) => None,
    };
    
    // 2. 读取永久配置文件
    let permanent_mode = fs::read_to_string(USB_MODE_PERMANENT_FILE)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .filter(|&m| (1..=3).contains(&m));
    
    // 3. 读取临时配置文件
    let temporary_mode = fs::read_to_string(USB_MODE_TEMPORARY_FILE)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .filter(|&m| (1..=3).contains(&m));
    
    Ok(UsbModeConfigResult {
        current_mode: current_hardware_mode,
        permanent_mode,
        temporary_mode,
    })
}

/// USB 模式配置读取结果
pub struct UsbModeConfigResult {
    /// 当前硬件实际运行的模式（从 configfs 读取）
    pub current_mode: Option<u8>,
    /// 永久配置的模式（从 mode.cfg 读取）
    pub permanent_mode: Option<u8>,
    /// 临时配置的模式（从 mode_tmp.cfg 读取）
    pub temporary_mode: Option<u8>,
}

/// 获取当前 USB 模式（从 configfs 读取实际配置）
/// 
/// # VID:PID 到模式的映射
/// - 0x1782:0x4040 -> 模式 1 (NCM)
/// - 0x1782:0x4039 -> 模式 2 (ECM)
/// - 0x1782:0x4038 -> 模式 3 (RNDIS)
pub fn get_current_usb_mode() -> Result<UsbModeResult, String> {
    // 尝试读取当前的 VID 和 PID
    let vid = fs::read_to_string(format!("{}/idVendor", GADGET_PATH))
        .map_err(|e| format!("Failed to read VID: {}", e))?
        .trim()
        .to_lowercase();
    let pid = fs::read_to_string(format!("{}/idProduct", GADGET_PATH))
        .map_err(|e| format!("Failed to read PID: {}", e))?
        .trim()
        .to_lowercase();

    // 根据 VID:PID 判断模式
    match (vid.as_str(), pid.as_str()) {
        ("0x1782", "0x4040") => Ok(UsbModeResult { mode: 1 }), // NCM
        ("0x1782", "0x4039") => Ok(UsbModeResult { mode: 2 }), // ECM
        ("0x1782", "0x4038") => Ok(UsbModeResult { mode: 3 }), // RNDIS
        // 其他可能的 NCM PID（从 usbenum.sh 分析）
        ("0x1782", "0x4107") => Ok(UsbModeResult { mode: 1 }), // NCM1
        ("0x1782", "0x4105") => Ok(UsbModeResult { mode: 1 }), // NCM2
        ("0x1782", "0x4103") => Ok(UsbModeResult { mode: 1 }), // NCM3
        ("0x1782", "0x4101") => Ok(UsbModeResult { mode: 1 }), // NCM4
        // 其他可能的 ECM PID
        ("0x1782", "0x4106") => Ok(UsbModeResult { mode: 2 }), // ECM1
        ("0x1782", "0x4104") => Ok(UsbModeResult { mode: 2 }), // ECM2
        ("0x1782", "0x4102") => Ok(UsbModeResult { mode: 2 }), // ECM3
        ("0x1782", "0x4100") => Ok(UsbModeResult { mode: 2 }), // ECM4
        _ => {
            // 如果无法识别，尝试从配置文件读取
            fs::read_to_string("/mnt/data/mode.cfg")
                .ok()
                .and_then(|s| s.trim().parse::<u8>().ok())
                .filter(|&m| (1..=3).contains(&m))
                .map(|mode| UsbModeResult { mode })
                .ok_or_else(|| format!("Unknown USB mode (VID={}, PID={})", vid, pid))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{expand_ipv6_hextets, ipv6_prefix_64};

    #[test]
    fn ipv6_prefix_from_compressed_sipa_addr() {
        assert_eq!(
            ipv6_prefix_64("2409:8d5c:240:47cf::1").as_deref(),
            Some("2409:8d5c:240:47cf")
        );
    }

    #[test]
    fn ipv6_prefix_from_expanded_addr() {
        assert_eq!(
            ipv6_prefix_64("2409:8d5c:240:47cf:0:0:0:1").as_deref(),
            Some("2409:8d5c:240:47cf")
        );
    }

    #[test]
    fn ipv6_prefix_from_eui64_usb_addr() {
        assert_eq!(
            ipv6_prefix_64("2409:8d5c:240:47cf:cee8:acff:fec0:0").as_deref(),
            Some("2409:8d5c:240:47cf")
        );
    }

    #[test]
    fn expand_loopback_and_leading_compress() {
        let hextets = expand_ipv6_hextets("::1").expect("expand ::1");
        assert_eq!(
            hextets,
            vec!["0", "0", "0", "0", "0", "0", "0", "1"]
        );
        assert_eq!(ipv6_prefix_64("::1").as_deref(), Some("0:0:0:0"));
    }
}

