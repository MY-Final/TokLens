# TokLens 0.1.7

- 用量分析支持按客户端浏览对应模型的 Token 用量，展开查看分项和来源；使用可横向滚动的下划线标签，侧边栏常驻显示。
- 改进 GitHub Copilot 采集，支持 CLI 与 VS Code Agent Host 用量，避免重复计数，并保留缺失字段的未知状态。
- 新增 Gemini CLI、Grok CLI、MCode 三个客户端的数据源、同步、筛选与统计展示，现支持十二个客户端。
- 明确新客户端统计口径：Gemini 保留客户端自报总量；Grok 请求数表示轮数；MCode 缓存读写独立计入总量，无法确认的推理关系和模型保持未知。

保留已有统计与设置，数据库结构不变；统计口径版本为 2.12，原有客户端历史规则保持不变。
支持 Windows x64、macOS Intel 与 Apple Silicon。macOS 安装包尚未经过 Apple 公证。
默认不联网，自动检查更新默认关闭；下载和安装均由用户触发。
