# TokLens 0.1.5

- 修复会话请求详情中文字较长时，来源、状态等列挤压重叠的问题。长文本自动换行，表格支持横向和纵向滚动，滚动时表头保持可见；数字溢出时可悬停查看完整值。
- 新增 WorkBuddy 本地用量统计，总览趋势、客户端分布、用量分析、会话明细与数据源管理均可查看。只读本机日志，使用客户端原生 Token 记录，不采集会话正文。
- WorkBuddy 按原生输出事件去重，usage 镜像仅补缺、不重复累加；正在写入的日志或读取失败时保留已有统计，下次同步重试。缺失字段继续显示未知，不补造请求或用量。

已有用户：升级后打开“数据源管理”，启用 WorkBuddy，再点击刷新。新安装默认启用八个客户端。WorkBuddy 默认读取 `~/.workbuddy/projects`；若配置过 `WORKBUDDY_CONFIG_DIR`，请确保 TokLens 与 WorkBuddy 使用相同目录及环境。

支持 Codex、Claude Code、OpenCode、DSH、Kimi Code、MiMoCode、Pi Desktop 与 WorkBuddy。
统计口径更新至 2.7：WorkBuddy 输入包含缓存读取，输出包含推理，统一分桶会扣除对应子集；总量保留客户端原生值，缓存写入关系未知且不计总量。客户端字段异常会保留原生总量并标记，不静默修正。其他客户端口径不变，数据库结构不变，保留已有统计和设置。

支持 Windows x64、macOS Intel 与 Apple Silicon。macOS 安装包尚未经过 Apple 公证。
默认不联网、不上传统计或会话正文；自动检查更新默认关闭，下载和安装均由用户触发。
