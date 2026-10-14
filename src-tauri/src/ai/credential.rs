//! OS 凭据库窄接口（write-only）：只 `set` / `clear` / `has`。
//!
//! 平台原生后端：Windows Credential Manager / macOS Keychain / Linux Secret Service。
//! 明文只在 [`request_secret`] 内部读取，供 Rust 出网拼请求头；**绝不**返回给前端、写入日志、设置或导出。

const SERVICE: &str = "com.toklens.app.llm";
const ACCOUNT: &str = "api_key";
const STORE_ERROR: &str = "无法访问系统凭据库";

/// 凭据存储抽象；真实实现走 OS 凭据库，测试用内存实现。
pub trait CredentialStore {
    fn set(&self, secret: &str) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
    fn has(&self) -> Result<bool, String>;
}

/// 平台原生凭据库实现。
pub struct KeyringStore;

impl CredentialStore for KeyringStore {
    fn set(&self, secret: &str) -> Result<(), String> {
        let entry = entry()?;
        entry
            .set_password(secret)
            .map_err(|_| STORE_ERROR.to_string())
    }

    fn clear(&self) -> Result<(), String> {
        let entry = entry()?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(STORE_ERROR.to_string()),
        }
    }

    fn has(&self) -> Result<bool, String> {
        let entry = entry()?;
        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(_) => Err(STORE_ERROR.to_string()),
        }
    }
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|_| STORE_ERROR.to_string())
}

/// 保存 / 清除凭据：空串（或纯空白）表示清除，否则去除首尾空白后写入。
pub fn apply_secret(store: &dyn CredentialStore, secret: &str) -> Result<(), String> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        store.clear()
    } else {
        store.set(trimmed)
    }
}

/// 最佳努力判断是否存在密钥：凭据库不可用时返回 `false`，不阻塞配置读取。
pub fn has_secret(store: &dyn CredentialStore) -> bool {
    store.has().unwrap_or(false)
}

/// 仅供 Rust 出网使用：读取明文密钥作为请求头。
///
/// 不暴露为 command；返回值绝不出现在 DTO、日志或错误信息中。任何失败返回 `None`。
pub fn request_secret() -> Option<String> {
    entry().ok()?.get_password().ok()
}
