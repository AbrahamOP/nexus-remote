use crate::model::{AuthMode, Profile};
use std::path::Path;
/// Imports concrete Host entries. Includes and ProxyCommand are never executed.
pub fn parse(text: &str, home: &Path) -> Result<Vec<Profile>, String> {
    if text.len() > 5_000_000 {
        return Err("Configuration SSH trop volumineuse.".into());
    }
    #[derive(Default)]
    struct Block {
        aliases: Vec<String>,
        host: String,
        user: String,
        port: Option<u16>,
        key: String,
    }
    fn finish(b: Block, home: &Path, out: &mut Vec<Profile>) {
        for alias in b.aliases {
            if alias.contains(['*', '?', '!']) {
                continue;
            }
            let key = if let Some(suffix) = b
                .key
                .strip_prefix("~/")
                .or_else(|| b.key.strip_prefix("~\\"))
            {
                home.join(suffix).display().to_string()
            } else {
                b.key.clone()
            };
            out.push(Profile {
                name: alias.clone(),
                host: if b.host.is_empty() {
                    alias
                } else {
                    b.host.clone()
                },
                username: if b.user.is_empty() {
                    std::env::var("USERNAME").unwrap_or("user".into())
                } else {
                    b.user.clone()
                },
                port: b.port.unwrap_or(22),
                group: "Import SSH".into(),
                auth: if key.is_empty() {
                    AuthMode::Password
                } else {
                    AuthMode::PrivateKey
                },
                private_key_path: key,
                ..Default::default()
            });
        }
    }
    let mut current = Block::default();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(|c: char| c.is_whitespace() || c == '=') else {
            continue;
        };
        let value = value.trim().trim_start_matches('=').trim();
        let value = if value.starts_with('"') && value.ends_with('"') {
            value[1..value.len() - 1].to_string()
        } else {
            value.split(" #").next().unwrap_or(value).trim().to_string()
        };
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                finish(current, home, &mut out);
                current = Block {
                    aliases: value.split_whitespace().map(str::to_string).collect(),
                    ..Default::default()
                };
            }
            "hostname" => current.host = value,
            "user" => current.user = value,
            "port" => {
                current.port = Some(
                    value
                        .parse::<u16>()
                        .ok()
                        .filter(|p| *p > 0)
                        .ok_or("Port SSH invalide dans le fichier.")?,
                )
            }
            "identityfile" => current.key = value,
            _ => (),
        }
    }
    finish(current, home, &mut out);
    if out.len() > 10000 {
        return Err("Trop d’hôtes à importer.".into());
    }
    for p in &out {
        p.validate()?;
    }
    Ok(out)
}
pub fn quick_profile(input: &str) -> Result<Profile, String> {
    let input = input.trim();
    let (protocol, rest) = if let Some(s) = input
        .strip_prefix("ssh://")
        .or_else(|| input.strip_prefix("ssh "))
    {
        (crate::model::Protocol::Ssh, s)
    } else if let Some(s) = input.strip_prefix("sftp://") {
        (crate::model::Protocol::Sftp, s)
    } else if let Some(s) = input.strip_prefix("rdp://") {
        (crate::model::Protocol::Rdp, s)
    } else {
        (crate::model::Protocol::Ssh, input)
    };
    let (username, address) = rest
        .rsplit_once('@')
        .map(|(u, h)| (u.to_string(), h))
        .unwrap_or_else(|| {
            (
                if protocol == crate::model::Protocol::Rdp {
                    String::new()
                } else {
                    std::env::var("USERNAME").unwrap_or("user".into())
                },
                rest,
            )
        });
    if username.contains(':') {
        return Err("Saisissez le mot de passe dans le dialogue de connexion.".into());
    }
    let (host, port) = if let Some(bracket) = address.strip_prefix('[') {
        let (host, tail) = bracket.split_once(']').ok_or("Adresse IPv6 incomplète.")?;
        let port = if tail.is_empty() {
            protocol.port()
        } else {
            tail.strip_prefix(':')
                .ok_or("Adresse IPv6 invalide.")?
                .parse::<u16>()
                .map_err(|_| "Port invalide.")?
        };
        (host.to_string(), port)
    } else if address.matches(':').count() == 1 {
        let (host, port) = address.rsplit_once(':').unwrap();
        (
            host.to_string(),
            port.parse::<u16>().map_err(|_| "Port invalide.")?,
        )
    } else {
        (address.to_string(), protocol.port())
    };
    let p = Profile {
        name: host.clone(),
        host,
        username,
        port,
        protocol,
        ..Default::default()
    };
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_concrete_hosts_without_executing_directives() {
        let p=parse("Host *.prod\n ProxyCommand malicious\nHost test other\n HostName 192.0.2.1\n User root\n Port 2222\n IdentityFile \"~/clé privée\"",Path::new("C:/Users/test")).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].port, 2222);
        assert!(p[0].private_key_path.contains("clé privée"));
    }
    #[test]
    fn quick_connect_parses_ipv6_and_rejects_inline_password() {
        let p = quick_profile("sftp://admin@[::1]:2222").unwrap();
        assert_eq!(p.host, "::1");
        assert_eq!(p.port, 2222);
        assert!(quick_profile("ssh://user:secret@host").is_err());
        assert!(quick_profile("ssh://host:0").is_err());
    }
}
