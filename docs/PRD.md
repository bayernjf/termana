# termana — 产品需求文档（PRD）

## 一句话定义
termana 是一个 **agent 中立的项目控制层**：在一个面板里管理多个开发项目，每个项目绑定一个 coding agent（Claude Code / Codex / ...），点一下拉起终端直接开干；并在此基础上提供跨 agent 的上下文归一化与可观测性。

## 背景
2026 年，开发者同时使用多个 CLI coding agent（Claude Code、Codex、Aider、Gemini CLI…）。每个 agent 有自己的上下文文件格式、配置模型、权限和 MCP 接入方式。这些碎片化目前全靠手动维护，且没有任何一个 agent 厂商愿意管理竞品的配置。

## 目标用户
- 终端爱好者 / 重度 CLI 用户
- 同时维护多个项目、且在项目间使用不同 coding agent 的开发者
- 追求可复现、可版本化的 agent 工作环境的人

## 核心问题（痛点）
1. **上下文碎片化**：同一项目要维护 `CLAUDE.md` / `AGENTS.md` / `.codex` / `.cursorrules`，内容重叠且 drift。
2. **配置不迁移**：项目用哪个 agent、model、MCP、权限，全靠记忆，换机器 / 换人即丢失。
3. **跨 agent 无感知**：多个 agent 会话分散在终端窗口，看不到全局状态和 token 消耗。
4. **agent 切换无接力**：任务卡住想换 agent，context 带不过去。

## 核心价值主张
**agent 中立。** 每个 agent 厂商只管自己，termana 占据"多 agent 之间的缝隙"——这是唯一不会被单一厂商吃掉的位置。

## 非目标（明确不做）
- 不做自己的终端模拟器（drive 系统 tmux / 终端，不嵌入）。
- 不做云托管 SaaS（本地优先）。
- 不替代任何 agent 本身的能力（不自己跑模型）。
- v1 不做团队 / 多人协作。

## 范围分期
| 阶段 | 内容 | 状态 |
|---|---|---|
| **v0** | 项目注册表 + agent 绑定（binary 级） + 一键拉起终端 + agent 安装检测 + built-in 预设（agents.toml，不可改）+ 自定义 agent（增删改）。入口层。 | 已建 |
| **v1** | 上下文归一化（写一次，派生各 agent 格式）；配置绑定深化（model / 权限 / MCP）。 | 规划 |
| **v2** | 跨 agent 可观测面板；agent 间带 context 接力。 | 规划 |

## 成功标准
- **v0**：用户能在面板里配 5 个项目、绑定不同 agent、一键拉起终端跑起来。
- **v1**：用户为一个项目写一份 context，sync 后各 agent 格式文件正确生成；换 agent 不需要重写 context。
- **留存信号**：用户持续用 termana 启动项目（而非 alias / tmuxinator），且依赖 sync 维护 context。

## 风险
- **停在 launcher**：免费替代品（alias / tmuxinator）够用，无留存。→ 必须推进到 context 归一化层。
- **被 agent 厂商 commoditize**：单 agent 配置管理厂商会自己做。→ 押注多 agent 中立，不押注单 agent 功能。
- **adapter 维护成本**：CLI agent 演进快。→ 薄 adapter 层 + 社区贡献。
