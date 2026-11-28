//! Why a relay connection failed, in words a person can act on, not the library's.
use tokio_tungstenite::tungstenite::Error;

/// A dial that did not reach the relay, or a connection to it that ended.
pub(super) fn dial_failed(e: &Error) -> String {
    use std::io::ErrorKind as K;
    match e {
        Error::Io(io) => match io.kind() {
            K::ConnectionRefused => t!("连不上中继：它没在运行，或者地址、端口不对").into(),
            K::TimedOut => t!("连中继超时：网络不通，或者中继所在的机器挡掉了这个端口").into(),
            K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe | K::UnexpectedEof => {
                t!("和中继的连接被中途切断").into()
            }
            // Name lookups fail as `Other` with the resolver's words.
            _ if io.to_string().contains("lookup") || io.to_string().contains("resolve") => {
                t!("找不到中继地址：检查地址有没有写错").into()
            }
            _ => t!("连不上中继（{io}）", io = io.to_string()),
        },
        Error::Tls(_) => {
            t!("和中继建立加密连接失败：中继的证书有问题，或者地址填成了别的网站").into()
        }
        Error::Http(response) => t!(
            "中继拒绝了连接（HTTP {status}）：地址可能不对，或者它不是 Fibo 中继",
            status = response.status().as_u16()
        ),
        Error::Url(_) => t!("中继地址的格式不对").into(),
        Error::Protocol(_) | Error::ConnectionClosed | Error::AlreadyClosed => {
            t!("和中继的连接被中途切断").into()
        }
        other => t!("连不上中继（{other}）", other = other.to_string()),
    }
}

/// The relay turned this machine away at registration (`close` code, what it said).
pub(super) fn refused(code: u16, said: &str) -> String {
    match code {
        4003 => {
            t!("中继要求的注册密钥不对或没填：在 设置 › 手机与远程 填中继提供者给的密钥").into()
        }
        _ => t!("中继拒绝了这台电脑：{said}", said),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_errors_become_what_to_check() {
        let refused = Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        assert!(dial_failed(&refused).contains("没在运行"));
        let lookup = Error::Io(std::io::Error::other(
            "failed to lookup address information",
        ));
        assert!(dial_failed(&lookup).contains("找不到中继地址"));
        assert!(refused_words(4003).contains("设置 › 手机与远程"));
        assert!(refused_words(4002).contains("Invalid token hash"));
    }

    fn refused_words(code: u16) -> String {
        refused(
            code,
            if code == 4002 {
                "Invalid token hash"
            } else {
                ""
            },
        )
    }
}
