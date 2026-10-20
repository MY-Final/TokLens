# TokLens 0.1.13

## 新增客户端

- 接入 Cline：读取会话清单与逐轮用量明细，按轮计入会话与请求统计；输入已包含缓存读写，不会重复计入。

## 采集稳定性

- 修复 OpenCode 本地库超过 2 GB 后同步持续失败的问题：客户端运行时改为就地只读打开，峰值内存由 2 078 MB 降到 75 MB。
- 修复 Kimi Code 与 Pi Desktop 迁移数据目录与文件格式后完全采不到数据的问题：
  Kimi Code 支持 `~/.kimi/sessions` 与协议 1.1，Pi Desktop 支持 `~/.pi/agent/sessions` 与版本 3 会话文件；旧格式仍可读取。
- Kimi Code 与 Pi Desktop 的解析器版本提升，此前导入的空会话会在下次同步时重新解析。

## 正确性

- Kimi Code 的用量按消息 id 去重，同一 id 重复出现只计一次。
- Cline 会话总量取客户端累计值，与逐轮明细不一致时标记为明细不完整，不静默改数。

## 工程

- 新增 `md5` 依赖（MIT、无传递依赖），仅用于反查 Kimi Code 的工作目录摘要，不参与统计口径。
- 统计数据库结构保持不变，统计口径扩展到 2.15，更新签名公钥不变。

支持 Windows x64、macOS Intel 与 Apple Silicon。macOS 安装包尚未经过 Apple 公证。
默认不联网，自动检查更新默认关闭；下载和安装均由用户触发。
