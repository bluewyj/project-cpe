# AGENTS.md — 给 AI 编程助手的项目说明

> 本文件面向 **AI Agent**。人类 README 见根目录 `README.md`。  
> 技术细则在 `.cursor/rules/*.mdc`，这里只写协作方式与硬约束。保持简短。

## 协作关系（最重要）

| 角色 | 是谁 | 负责什么 |
|------|------|----------|
| **指引者** | 仓库主人（中文开发者） | 定目标、选方案、拍板、验收；不必精通实现细节 |
| **执行者** | AI | 读代码、设计选项、写代码、跑检查、用白话汇报 |

- 默认用**简体中文**沟通：结论先行，少术语；必要时用一两句解释「为什么」。
- 主人说「做什么 / 选哪个」；AI 负责「怎么做」与落地。不要默认主人会改 Rust/前端细节。
- 拿不准就**先问再改**，不要悄悄扩大范围。

## 怎么跟 AI 下指令（示例）

你可以直接这样说（越具体越好）：

| 你想要的 | 可以这样说 |
|----------|------------|
| 只要方案 | 「先别改代码，给方案 A/B，并标明推荐」 |
| 做功能 | 「仪表盘加飞行模式开关，改完用白话说明怎么在设备上点验」 |
| 修问题 | 「数据开着上不了网，先查可能原因，再改；别动无关文件」 |
| 提交/推送 | 「可以提交并推 main」或「开 PR 到 main」 |
| 发版 | 「升到 3.4.1 并说明改了啥」（AI 会同步改三处版本号） |

一轮只交代**一个主目标**；需要真机时说明你是否已连上设备后台。

## 项目一句话

面向 5G CPE（如 UDX710）的后台：Rust 服务通过 ofono 控制调制解调器，React 管理界面，HTTP `/api` 对接。

目录：`backend/` 后端 · `frontend/` 前端 · `bruno-api/` API 测试集合。

## 必做 / 先问 / 禁止

### 必做（Always）

- 改代码前先对照 `.cursor/rules/`（尤其 `00-project-stack.mdc`）
- 新/改 API：后端 +（如需）前端 types/api + `bruno-api` 同步
- ofono / D-Bus / AT：**必须**走 `with_serial`
- 做完用白话说明：改了什么、怎么验证、主人需要在真机点哪几步
- 前端改完尽量跑 `pnpm lint`（在 `frontend/`）
- 涉及设备行为的改动：汇报里必须拆开「AI 已做」与「请你真机点验」

### 先问再做（Ask first）

- 新增大依赖、改构建/部署脚本、改默认端口或设备侧行为
- 删除功能、改 API 字段含义、影响设备射频/数据连接的默认策略
- Git 远端操作、发布与版本号变更（见「Git / GitHub」「版本管理」）
- 一次性大重构或跨很多无关文件的「顺手清理」

### 禁止（Never）

- 编造 ofono/AT 行为；不确定就查现有 `dbus.rs` 或问主人
- 跳过 `with_serial` 并发打 D-Bus
- 为系统指标再拆一堆 `/api/stats/...`（应扩展现有 `/api/stats`）
- 提交密钥、设备隐私明文、把调试后门留进默认路径
- 在回复里堆长英文术语墙；主人要的是可决策的说明
- 对 `main` force push、`--no-verify` 跳过 hook、擅自改 `git config`（除非主人写明允许）
- 只改一处版本号导致三处不一致（见「版本管理」）
- 用 PowerShell 默认编码、对含中文文件用 `StrReplace`、Shell 里写 bash HEREDOC/内联多行中文，或以「临时文件」为由不遵守 UTF-8（见 `.cursor/rules/text-encoding.mdc`）

## 技术规范去哪看

| 场景 | 文件 |
|------|------|
| 技术栈与硬约束 | `.cursor/rules/00-project-stack.mdc` |
| 后端 Rust/Axum | `.cursor/rules/rust-backend.mdc` |
| ofono / AT | `.cursor/rules/dbus-ofono.mdc` |
| 前端 React/MUI | `.cursor/rules/frontend-react.mdc` |
| 加 API 清单 | `.cursor/rules/api-feature-flow.mdc` |
| 文本编码 / 中文注释防乱码 | `.cursor/rules/text-encoding.mdc` |

旧版 `.cursorrules` 仅作指针；**以 `.cursor/rules/*.mdc` 为准**。

## 常见命令（AI 可自行执行）

```bash
# 前端
cd frontend && pnpm lint && pnpm build

# 后端（交叉编译等以 scripts/ 与 README 为准）
./scripts/build.sh
```

设备侧验证往往需要真机/局域网；本地改完后必须写出「还需你在设备上点哪几步」（见下节）。

## 真机 / 局域网验收（写给主人）

AI 本地能做 lint/编译时，在汇报的「验证」里分开写：

- **AI 已做**：例如 `pnpm lint`、代码自检、Bruno 文件已补
- **请你真机点验**（按改动勾选，说人话）：
  1. 电脑连上设备网络，浏览器打开后台（常见 `http://192.168.66.1`，以你实际为准）
  2. 打开相关页面，看状态/开关是否符合预期
  3. 涉及飞行模式 / 数据开关 / 射频：观察能否上网、信号/制式是否恢复（可能要等几秒重新注册）
  4. 涉及 OTA：确认版本号显示与包名一致后再升级
  5. 连不上后台时：先说「网络/地址不通」，不要让 AI 假装已在真机测过

## Git / GitHub（先问再动远端）

- **默认分支：`main`**。默认走「旁支 → PR → 合入 `main`」。
- **可授权直推 `main`**：若你明确说「直接推 main / 提交并推送到 main」，AI 可在该次任务内：`commit` → `push` 到 `main`（仍禁止 force push、跳过 hook）。未写明则不要直推。
- **commit / push / 开 PR / 合并 PR / 发布**：必须你先明确同意；未同意只改本地、用白话汇报。
- **开始任务前**：看 `git status`；若有未说明的本地改动或与远端不一致，先停下来问你，再 `pull`/`fetch`。
- **拉取**：用 `git pull`（或先 `fetch` 再合并）；出现冲突先说明冲突文件与选项，等你决定，禁止自行乱解后强推。
- **提交**：信息按下方「修改摘要」；只暂存相关文件；不提交密钥与大型无关产物。
- **PR**：用 `gh pr create`；正文按「修改摘要」；目标分支默认 `main`。
- **GitHub 编译 / CI**：推送或 PR 后用 `gh` / Checks 查看（如 `.github/workflows/build-ota.yml`）；失败时先白话摘要原因再改，不盲目空推重试。
- **CI 产物拉回本地**：放到仓库根目录 **`release/`**（已在 `.gitignore`，勿提交大包）。
  - 产物名与 CI 一致：`release/udx710-ota-{版本}.tar.gz`（如 `udx710-ota-3.4.0.tar.gz`）
  - 下载示例（主人同意后）：`gh run download <run-id> -D release/`；或先下到临时目录再挪进 `release/`，避免覆盖时先确认旧文件
  - **不要**把 CI 包塞进 `backend/target/`、`frontend/dist/` 或桌面长期存放
  - 拉回后在汇报里写清：文件路径、版本号；真机升级前请主人核对版本显示
- **禁止**：`git push --force` 到 `main`；改写已推送历史；跳过 hooks。

## 修改摘要怎么写

原则：**写清「为什么 / 对用户有什么影响」**，少堆文件清单；中文为主，类型标签可用英文前缀。

### 1. 对主人的聊天汇报（每次改完都要）

```text
结论：……（1 句）
改动：
- ……（功能/行为，不是文件名堆砌）
验证：已做 …… / 还需你 ……
请确认：……（若有）
```

### 2. Git commit（主人同意提交后）

格式：`<类型>: <一句话说明为什么>`

常用类型：`feat` 新功能 · `fix` 修复 · `docs` 文档 · `refactor` 重构 · `chore` 杂务（依赖/脚本）· `perf` 性能

```text
feat: 支持短信推送开关，避免未配置时误发
fix: 数据连接切换后清空残留 iptables，防止旧规则干扰
docs: 补充 AGENTS 版本与 Git 协作约定
```

- 一行能说清就一行；需要时第二段写要点（仍说「为什么」）。
- 不写无意义的 `update` / `fix` / `改了点东西`。

### 3. Pull Request 正文（开 PR 时）

```markdown
## 摘要
- ……（对使用者/设备可见的变化，3 条以内为佳）

## 测试
- [ ] ……（本地/lint/真机等；写清谁测、测什么）

## 备注
- 版本号：未改 / 将改为 x.y.z（须主人已同意）
- 风险：……（射频/数据/OTA 等相关时必写）
```

### 4. 发版说明（仅主人要求升版本 / 打 OTA / Release 时）

用白话列「本版新功能 / 修复 / 破坏性变更」；可写在 GitHub Release 说明里。  
仓库暂无强制维护 `CHANGELOG.md`；若主人要求再建文件并按版本追加。

## 版本管理

采用 **SemVer**：`主版本.次版本.修订号`（当前以根目录 `VERSION` 为准，如 `3.4.0`）。

### 单一事实来源（三者必须相同）

发版或升版本时，**同时**改且保持一致：

1. 根目录 `VERSION`（OTA 打包、`scripts/pack-ota.sh`、CI 读取）
2. `backend/Cargo.toml` 的 `version`
3. `frontend/package.json` 的 `version`

GitHub Actions（`build-ota.yml`）会校验上述三处；不一致则编译失败。  
运行时：后端 `build.rs`、前端 `vite.config.ts` 也会读 `VERSION` 注入显示/构建信息。

### 何时升版本（先问主人再改号）

| 变更类型 | 通常升 | 例子 |
|----------|--------|------|
| 修 bug、小改文案/不影响兼容 | 修订号 patch | `3.4.0` → `3.4.1` |
| 新功能、API 增补但可兼容 | 次版本 minor | `3.4.0` → `3.5.0` |
| 不兼容改动、重大行为变化 | 主版本 major | `3.4.0` → `4.0.0` |

日常功能开发**默认不改版本号**，除非主人说「要发版 / 打 OTA / 升版本」。

### AI 操作约定

- 升版本前用白话建议：升哪一位、新版本号是什么，等主人确认后再改三处文件。
- 不要只改 `Cargo.toml` 或只改 `package.json`。
- 不要擅自打 git tag、创建 GitHub Release、触发 OTA workflow（需主人明确要求）。
- OTA 包名形态：`udx710-ota-{version}.tar.gz`（见 `scripts/pack-ota.sh`）。

## 回复主人时的推荐格式

按「修改摘要 → 对主人的聊天汇报」执行（结论 / 改动 / 验证 / 请确认）。

<comet-ambient-resume>
<!-- Managed by Comet. Edits inside this block may be replaced by comet init/update. -->
<!-- Contract: comet.resume_probe.v2 -->

## Comet Ambient Resume

在这个仓库中，开始处理需要改动或调查的任务前，如果可能存在活跃 Comet workflow，把当前用户请求传入只读探针：`comet resume-probe . --stdin --json`。

- 如果用户通过宿主明确调用任意 Comet Skill（例如 `@comet`、`/comet`、`@comet-native` 或 `/comet-hotfix`），显式调用优先于本恢复协议；不要运行 resume probe，直接进入被调用的 Skill。
- 如果用户通过宿主明确调用的是非 Comet 的 Skill 或斜杠命令，任务意图已由该调用明确：不要运行 resume probe，直接执行该 Skill。
- 如果你正在 Comet 流程内（包括正在等待用户回复你在流程中提出的问题），不要运行 resume probe；把这类回复（例如方案/选项选择）当作当前 change 的继续，直接按用户的选择推进。
- 只信任返回的 `workflow`、`skill` 和 `entrySource`；它们只由项目配置或无配置兼容回退决定。不得扫描或切换另一套 workflow。
- 如果 probe 返回 `auto_resume`，简短说明选中的 active change，并进入 `nextCommand` 指向的永久入口。不要把状态命令当作恢复入口直接推进。
- 如果 probe 返回 `ask_user`，只问一个简短问题并等待用户回复。
- 如果当前请求未明确调用 Comet Skill，且 probe 返回 `out_of_scope` 或 `none`，不要进入 Comet workflow。
- `out_of_scope` 或 `none` 只表示不要因为这个新请求进入 Comet workflow；它绝不表示要暂停或退出一个已在进行的 Comet 流程。
- 如果配置或状态无效且没有 `nextCommand`，停止并报告原因；不要猜测另一个 workflow。
- 不能只因为存在 active change 就把无关任务挂到该 change。Native 的未提交改动由 Native 入口检查，不由探针自动归因。
</comet-ambient-resume>
