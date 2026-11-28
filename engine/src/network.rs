use crate::errors::ServiceError;
use anyhow::Result;
use axum::http::StatusCode;
use serde_json::json;
use std::time::Duration;

#[derive(Clone, Default, Debug)]
pub struct ProxySettings {
    pub server: String,
    pub bypass: String,
    pub automatic: bool,
}
#[derive(Clone, Default)]
pub struct Routes {
    pub system: ProxySettings,
    pub http: String,
    pub https: String,
    pub all: String,
    pub bypass: String,
}
impl Routes {
    pub fn load() -> Self {
        Self {
            system: system_proxy(),
            http: environment("http_proxy"),
            https: environment("https_proxy"),
            all: environment("all_proxy"),
            bypass: environment("no_proxy"),
        }
    }
    pub fn fingerprint(&self) -> String {
        format!(
            "{:?}:{}:{}:{}:{}",
            self.system, self.http, self.https, self.all, self.bypass
        )
    }
    pub fn bypass(&self, url: &url::Url) -> bool {
        let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
        host.eq_ignore_ascii_case("localhost")
            || host.eq_ignore_ascii_case("loopback")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
            || bypass_matches(&self.bypass, url)
            || bypass_matches(&self.system.bypass, url)
    }
    pub fn proxy(&self, url: &url::Url) -> Option<String> {
        if self.bypass(url) {
            return None;
        }
        let env = if url.scheme() == "https" {
            &self.https
        } else {
            &self.http
        };
        let selected = if !env.is_empty() {
            env.clone()
        } else if !self.all.is_empty() {
            self.all.clone()
        } else {
            proxy_for_scheme(&self.system.server, url.scheme())?
        };
        Some(if selected.contains("://") {
            selected
        } else {
            format!("http://{selected}")
        })
    }
    pub fn client(&self, direct: bool, endpoint: &url::Url) -> Result<reqwest::Client> {
        if !direct
            && self.system.automatic
            && self.system.server.is_empty()
            && self.proxy(endpoint).is_none()
            && !self.bypass(endpoint)
        {
            return Err(ServiceError::new("proxy_auto_config","系统使用自动代理脚本；当前客户端不执行 PAC。可为此模型选择直连，或配置固定代理/代理环境变量",StatusCode::BAD_GATEWAY).into());
        }
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120));
        if !direct {
            let routes = self.clone();
            builder = builder.proxy(reqwest::Proxy::custom(move |url| routes.proxy(url)));
        }
        Ok(builder.build()?)
    }
    pub fn describe(&self, direct: bool, url: &url::Url) -> serde_json::Value {
        json!({"mode":if direct{"direct"}else{"system"},"route":if direct||self.bypass(url)||self.proxy(url).is_none(){"direct"}else{"proxy"},"system_proxy_configured":!self.system.server.is_empty(),"automatic_proxy_configured":self.system.automatic,"bypass_matched":self.bypass(url)})
    }
}
fn environment(name: &str) -> String {
    std::env::var(name)
        .or_else(|_| std::env::var(name.to_ascii_uppercase()))
        .unwrap_or_default()
}
fn proxy_for_scheme(server: &str, scheme: &str) -> Option<String> {
    if !server.contains('=') {
        return (!server.trim().is_empty()).then(|| server.trim().into());
    }
    server.split([';', ' ']).find_map(|part| {
        part.split_once('=')
            .filter(|(kind, _)| kind.eq_ignore_ascii_case(scheme))
            .map(|(_, value)| value.into())
    })
}
pub fn bypass_matches(patterns: &str, url: &url::Url) -> bool {
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    let host_port = format!("{host}:{}", url.port_or_known_default().unwrap_or(0));
    patterns
        .split([';', ',', ' '])
        .filter(|part| !part.is_empty())
        .any(|pattern| {
            let pattern = pattern.trim().to_ascii_lowercase();
            if pattern == "<local>" {
                return !host.contains('.') && host.parse::<std::net::IpAddr>().is_err();
            }
            if pattern == "*" {
                return true;
            }
            if let Some((address, bits)) = pattern.split_once('/') {
                if let (Ok(network), Ok(ip), Ok(bits)) = (
                    address.parse::<std::net::Ipv4Addr>(),
                    host.parse::<std::net::Ipv4Addr>(),
                    bits.parse::<u32>(),
                ) {
                    if bits <= 32 {
                        let mask = if bits == 0 {
                            0
                        } else {
                            u32::MAX << (32 - bits)
                        };
                        return u32::from(network) & mask == u32::from(ip) & mask;
                    }
                }
            }
            if pattern.contains('*') || pattern.contains('?') {
                return globset::GlobBuilder::new(&pattern)
                    .case_insensitive(true)
                    .build()
                    .is_ok_and(|glob| {
                        let matcher = glob.compile_matcher();
                        matcher.is_match(&host) || matcher.is_match(&host_port)
                    });
            }
            let domain = pattern.trim_start_matches('.');
            host == domain || host.ends_with(&format!(".{domain}")) || host_port == pattern
        })
}
#[cfg(not(windows))]
fn system_proxy() -> ProxySettings {
    ProxySettings::default()
}
#[cfg(windows)]
fn system_proxy() -> ProxySettings {
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        Networking::WinHttp::{
            WinHttpGetIEProxyConfigForCurrentUser, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
        },
    };
    unsafe fn take(pointer: *mut u16) -> String {
        if pointer.is_null() {
            return String::new();
        }
        let mut len = 0;
        while len < 65536 && unsafe { *pointer.add(len) } != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer, len) });
        unsafe { GlobalFree(pointer as _) };
        text
    }
    unsafe {
        let mut config: WINHTTP_CURRENT_USER_IE_PROXY_CONFIG = std::mem::zeroed();
        if WinHttpGetIEProxyConfigForCurrentUser(&mut config) == 0 {
            return ProxySettings::default();
        }
        let automatic = !config.lpszAutoConfigUrl.is_null();
        let _pac = take(config.lpszAutoConfigUrl);
        ProxySettings {
            server: take(config.lpszProxy),
            bypass: take(config.lpszProxyBypass),
            automatic,
        }
    }
}
