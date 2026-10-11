# 桌面发布与更新

## 平台与更新源

稳定版发布到 `MY-Final/TokLens` 的 GitHub Releases，清单为
`https://github.com/MY-Final/TokLens/releases/latest/download/latest.json`。
支持 Windows x64 NSIS、macOS Intel 与 Apple Silicon DMG；macOS 更新使用签名 `.app.tar.gz`。
首版没有 Windows Authenticode / Apple 签名与公证，更新签名不代表系统证书签名。

默认不联网。用户手动检查，或主动开启每天最多一次的自动检查；不会自动下载或安装。
开发模式禁用更新。尚未发布清单、网络失败、目标缺失和签名错误有独立提示；失败不安装。
清单只能指向本仓库 HTTPS release 下载地址；官方 updater 校验签名及签名中的版本。
安装前停止新采集并等待已有写入完成，保留统计和设置；安装失败恢复采集调度。

## 签名密钥

公钥保存于 `src-tauri/tauri.conf.json`；私钥放在仓库外。
本次本地生成路径为 `%USERPROFILE%\.toklens-signing\updater.key`，目录仅当前用户可访问。
不要将私钥或 signer 输出粘贴到日志、提交、聊天或 Release；保留安全备份。
已发布版本依赖此公钥，丢失私钥后不能直接换钥继续给旧版本提供可信更新。

GitHub Actions Secrets：

- `TAURI_SIGNING_PRIVATE_KEY`：私钥文件的完整内容（不要使用本机文件路径）。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：密钥密码；当前生成的 CI 密钥没有密码，此项可不设置。

本地构建可以将 `TAURI_SIGNING_PRIVATE_KEY` 指向仓库外私钥文件，再执行
`pnpm tauri build --bundles nsis`（Windows）或 `pnpm tauri build --bundles app,dmg`（macOS）。
不要在终端命令参数中展开私钥内容。此签名用于更新包，不用于系统签名。

## 发布步骤

日常 `.github/workflows/ci.yml` 在 main 推送、PR 和手动触发时检查前端、发布脚本，
并在 Windows x64、macOS Intel、Apple Silicon 上执行 Rust fmt / clippy / tests 与原生生产构建。
日常检查不打安装包、不使用签名 Secrets；PR 无写权限。重复提交取消同一分支的旧检查。
首次原生编译使用独立目标缓存，后续提交复用；锁文件必须保持不变。
工具链为 Node 22、pnpm 10、Python 3.12、Rust stable。
GitHub Actions 使用当前 Node 24 运行时版本（与构建项目的 Node 22 分开），避免已弃用 action 运行时。

`.github/workflows/release.yml` 仅由稳定版本标签触发，发布任务不会因普通 main 推送执行。

1. 同时修改 package.json、src-tauri/Cargo.toml 与 tauri.conf.json 的版本，并同步 Cargo.lock；更新 RELEASE_NOTES.md。
2. 运行前端与 Rust 校验；执行 `python -m unittest discover -s scripts -p "test_*.py"`。
3. 推送对应稳定标签 `vX.Y.Z`。流水线验证版本一致性并构建三个平台，各平台独立上传构建结果。
4. 每个构建使用嵌入公钥验证更新制品，并验证篡改后的字节被拒绝。全部成功后单个汇总任务生成完整 latest.json 和草稿 Release。
5. 验证三个平台的安装、托盘、关闭行为与旧版升级；确认后人工发布草稿。流水线不自动公开发布。

`scripts/release.py` 不联网：验证版本、收集指定平台制品、汇总带有版本化 URL 的清单。
缺少任一平台、签名不一致或目标重复均拒绝生成清单。重新运行已有草稿不会覆盖公开版本；
若草稿已存在，需要人工检查并清理失败草稿后重跑，不自动删除 Release。

撤回错误版本时先撤回公开 Release 并恢复上一份完整清单；客户端只接受比当前版本更高的稳定版本，
修复通常发布新的补丁版本，不通过更新清单强制降级。不对已发布数据库迁移做逆向修改。

## 验证范围

本地可验证 Windows 原生窗口、设置持久化、客户端启停、清除后暂停、签名与生产安装包构建。
macOS 原生运行、Gatekeeper 放行体验，以及两个连续已发布版本间的实际升级，需要对应机器与发布环境验收。
CI 的 macOS 构建不等同于这些手动验收。首个 Release 发布前，应用内不能取得可用更新清单。

前端生产构建仍有现有大 chunk 提示，更新功能未引入前端 updater 包。
原生 TLS / 签名库增加体积；实际安装包大小以发布制品为准。
本机 Windows NSIS 签名构建已通过，安装包约 3.39 MiB；嵌入公钥验证及篡改拒绝测试通过。

2026-10-11 Windows 隔离原生验收：主题保存与重载、七个客户端启停、清除确认及暂停、恢复采集、
首次关闭取消、记住缩小到托盘、再次启动恢复窗口、直接退出、重启后仍暂停均通过；
清除后的七客户端统计为 0，重启没有自动扫描；控制台无错误。未修改系统真实自启项。
