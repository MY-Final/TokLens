pub(super) fn error_message(error: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => "更新签名无效，已拒绝安装，请重新检查更新".into(),
        Error::ReleaseNotFound => "更新源暂不可用，可能尚未发布签名版本，请稍后再试".into(),
        Error::TargetsNotFound(_) | Error::TargetNotFound(_) => {
            "此版本没有适用于当前平台的更新包".into()
        }
        Error::Reqwest(_) | Error::Network(_) => "无法连接更新服务，请检查网络后重试".into(),
        _ => "更新操作失败，请重试或从项目发布页手动安装".into(),
    }
}
