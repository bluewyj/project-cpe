# Release 成品

当前推荐 OTA 包：

- `udx710-ota-3.5.0.tar.gz`

相对原作者改动说明见仓库根目录 [`CHANGES-FROM-UPSTREAM.md`](../CHANGES-FROM-UPSTREAM.md)。

## 架构说明（相对原作者已变更）

| 项 | 原作者 | 本仓库成品 |
|----|--------|------------|
| CPU | aarch64 | aarch64（不变） |
| C 库 | **musl** | **glibc** |
| 构建 target | `aarch64-unknown-linux-musl` | **`aarch64-unknown-linux-gnu.2.27`** |
| OTA meta `arch` | `aarch64-unknown-linux-musl` | `aarch64-unknown-linux-gnu` |

请勿用默认 `aarch64-unknown-linux-gnu`（易链接到 glibc 2.28+）直接打包装到本设备；也勿再混用 musl 包（OTA `arch` 校验已改为 gnu）。

构建命令：

```bash
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.27
```

`meta.json` 必须为 UTF-8 **无 BOM**。
