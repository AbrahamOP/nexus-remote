use crate::model::{AuthMode, Profile};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use sha2::{Digest, Sha256};
use ssh2::{KeyboardInteractivePrompt, Prompt, Session};
use std::io::Read;
use std::{
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub struct HostKeyRequest {
    pub endpoint: String,
    pub algorithm: String,
    pub fingerprint: String,
    pub reply: Sender<bool>,
}

#[derive(Clone)]
pub struct TrustBroker {
    pub requests: Sender<HostKeyRequest>,
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Connexion annulée.".into())
    } else {
        Ok(())
    }
}

/// Open a verified SSH connection. This function must only run on a worker thread.
/// The server identity is checked BEFORE any password/private-key authentication.
pub fn connect(
    profile: &Profile,
    secret: &str,
    trust: &TrustBroker,
    cancel: &AtomicBool,
) -> Result<Session, String> {
    check_cancel(cancel)?;
    if profile.host.trim().is_empty() || profile.username.trim().is_empty() || profile.port == 0 {
        return Err("Adresse, port ou utilisateur SSH invalide.".into());
    }
    let addresses: Vec<_> = (profile.host.as_str(), profile.port)
        .to_socket_addrs()
        .map_err(|e| format!("Résolution de l’adresse impossible : {e}"))?
        .collect();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last_error = "Aucune adresse réseau trouvée.".to_owned();
    let mut socket = None;
    for address in addresses {
        check_cancel(cancel)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match TcpStream::connect_timeout(&address, remaining.min(Duration::from_secs(5))) {
            Ok(stream) => {
                socket = Some(stream);
                break;
            }
            Err(error) => last_error = error.to_string(),
        }
    }
    let stream = socket.ok_or_else(|| format!("Connexion TCP impossible : {last_error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(20)))
        .map_err(|e| e.to_string())?;
    let _ = stream.set_nodelay(true);
    check_cancel(cancel)?;
    let mut session = Session::new().map_err(|e| e.to_string())?;
    session.set_tcp_stream(stream);
    session.set_timeout(20_000);
    session
        .handshake()
        .map_err(|e| format!("Négociation SSH impossible : {e}"))?;
    check_cancel(cancel)?;
    let (host_key, key_type) = session
        .host_key()
        .ok_or("Le serveur n’a pas fourni de clé SSH.")?;
    let fingerprint = format!(
        "SHA256:{}",
        STANDARD_NO_PAD.encode(Sha256::digest(host_key))
    );
    let (reply, response) = mpsc::channel();
    trust
        .requests
        .send(HostKeyRequest {
            endpoint: format!("[{}]:{}", profile.host.trim().to_lowercase(), profile.port),
            algorithm: format!("{key_type:?}"),
            fingerprint,
            reply,
        })
        .map_err(|_| "La vérification de la clé SSH est indisponible.")?;
    let trust_deadline = Instant::now() + Duration::from_secs(300);
    loop {
        check_cancel(cancel)?;
        match response.recv_timeout(Duration::from_millis(100)) {
            Ok(true) => break,
            Ok(false) => {
                return Err("Clé SSH refusée. Aucune authentification n’a été envoyée.".into());
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("Vérification de clé annulée.".into());
            }
            Err(RecvTimeoutError::Timeout) if Instant::now() >= trust_deadline => {
                return Err("La vérification de la clé SSH a expiré.".into());
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    check_cancel(cancel)?;
    match profile.auth {
        AuthMode::Password => {
            if let Err(password_error) = session.userauth_password(&profile.username, secret) {
                check_cancel(cancel)?;
                let available = session.auth_methods(&profile.username).unwrap_or_default();
                if !available.split(',').any(|m| m == "keyboard-interactive") {
                    return Err(format!(
                        "Authentification par mot de passe refusée : {password_error}"
                    ));
                }
                let mut prompter = PasswordOnly { secret };
                session.userauth_keyboard_interactive(&profile.username, &mut prompter)
                    .map_err(|_| "Authentification refusée. Vérifiez le mot de passe. Les défis MFA ne sont pas pris en charge dans cette version.".to_owned())?;
            }
        }
        AuthMode::PrivateKey => {
            let path = Path::new(&profile.private_key_path);
            let key = read_private_key(path)?;
            let text = std::str::from_utf8(&key).map_err(|_| "La clé privée doit être un fichier texte OpenSSH ou PEM en UTF-8. Les formats binaires ne sont pas pris en charge.".to_owned())?;
            if text.contains('\0') {
                return Err("La clé privée contient des données binaires. Choisissez une clé texte OpenSSH ou PEM.".into());
            }
            // Reading through Rust handles Unicode and long Windows paths; the
            // C library receives key bytes instead of using an ANSI file path.
            session
                .userauth_pubkey_memory(
                    &profile.username,
                    None,
                    text,
                    if secret.is_empty() {
                        None
                    } else {
                        Some(secret)
                    },
                )
                .map_err(|e| format!("Authentification par clé refusée : {e}"))?;
        }
    }
    check_cancel(cancel)?;
    if !session.authenticated() {
        return Err("Le serveur n’a pas accepté l’authentification SSH.".into());
    }
    session.set_keepalive(true, 25);
    Ok(session)
}

fn read_private_key(path: &Path) -> Result<Zeroizing<Vec<u8>>, String> {
    const LIMIT: u64 = 1024 * 1024;
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Impossible d’ouvrir le fichier de clé privée : {e}"))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Impossible de lire les informations de la clé privée : {e}"))?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err("La clé privée doit être un fichier texte de 1 Mo maximum.".into());
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(
        (metadata.len() as usize).min(LIMIT as usize),
    ));
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Impossible de lire le fichier de clé privée : {e}"))?;
    if bytes.len() > LIMIT as usize {
        return Err("Le fichier de clé privée dépasse la limite de 1 Mo.".into());
    }
    Ok(bytes)
}

// PAM password-only fallback. Unknown challenges and multi-prompt/MFA requests
// never receive the stored password automatically.
struct PasswordOnly<'a> {
    secret: &'a str,
}
impl KeyboardInteractivePrompt for PasswordOnly<'_> {
    fn prompt<'a>(&mut self, _: &str, _: &str, prompts: &[Prompt<'a>]) -> Vec<String> {
        if prompts.len() == 1 && !prompts[0].echo {
            let request = prompts[0]
                .text
                .trim()
                .trim_end_matches(':')
                .trim()
                .to_lowercase();
            if request == "password"
                || request == "mot de passe"
                || request.ends_with("'s password")
            {
                return vec![self.secret.to_owned()];
            }
        }
        prompts.iter().map(|_| String::new()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_fallback_never_answers_otp_or_multiple_prompts() {
        let mut callback = PasswordOnly {
            secret: "fixture-password",
        };
        assert_eq!(
            callback.prompt(
                "",
                "",
                &[Prompt {
                    text: "Password: ".into(),
                    echo: false
                }]
            ),
            vec!["fixture-password"]
        );
        assert_eq!(
            callback.prompt(
                "",
                "",
                &[Prompt {
                    text: "OTP code:".into(),
                    echo: false
                }]
            ),
            vec![""]
        );
        assert_eq!(
            callback.prompt(
                "",
                "",
                &[
                    Prompt {
                        text: "Password:".into(),
                        echo: false
                    },
                    Prompt {
                        text: "OTP:".into(),
                        echo: false
                    }
                ]
            ),
            vec!["", ""]
        );
    }

    #[test]
    fn private_key_loader_handles_unicode_paths_and_size_limit() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(format!("key-loader-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("clé-éphémère.pem");
        std::fs::write(&path, b"ASCII fixture text only").unwrap();
        assert_eq!(
            &**read_private_key(&path).unwrap(),
            b"ASCII fixture text only"
        );
        std::fs::write(&path, vec![b'a'; 1024 * 1024 + 1]).unwrap();
        assert!(read_private_key(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }

    #[test]
    #[ignore = "Requires the isolated loopback fixture with an ephemeral Ed25519 key."]
    fn ed25519_private_key_loopback_authentication() {
        let port: u16 = std::env::var("NEXUS_TEST_PORT")
            .expect("NEXUS_TEST_PORT missing")
            .parse()
            .unwrap();
        let expected_fingerprint =
            std::env::var("NEXUS_TEST_FINGERPRINT").expect("NEXUS_TEST_FINGERPRINT missing");
        let key_path = std::path::PathBuf::from(
            std::env::var("NEXUS_TEST_PRIVATE_KEY").expect("NEXUS_TEST_PRIVATE_KEY missing"),
        )
        .canonicalize()
        .expect("Fixture key file missing");
        let own_work = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        // An explicit test invocation must never read an existing user's SSH key.
        assert!(
            key_path.starts_with(&own_work),
            "Only generated keys under the task work directory are allowed"
        );
        let profile = Profile {
            name: "Generated Ed25519 fixture".into(),
            host: "127.0.0.1".into(),
            port,
            username: "testuser".into(),
            auth: AuthMode::PrivateKey,
            private_key_path: key_path.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let (requests, trust_rx) = mpsc::channel();
        let (completed, completed_rx) = mpsc::channel();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        struct CancelOnDrop(std::sync::Arc<AtomicBool>);
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let _guard = CancelOnDrop(cancel.clone());
        let worker_cancel = cancel.clone();
        let handle = std::thread::spawn(move || {
            let result =
                connect(&profile, "", &TrustBroker { requests }, &worker_cancel).map(|session| {
                    assert!(session.authenticated());
                    let channel = session
                        .channel_session()
                        .expect("Authenticated Ed25519 session cannot open a channel");
                    drop(channel);
                    let _ = session.disconnect(None, "Loopback test complete", None);
                });
            let _ = completed.send(result);
        });
        let request = trust_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("No host-key check before private-key authentication");
        assert_eq!(request.endpoint, format!("[127.0.0.1]:{port}"));
        assert_eq!(request.fingerprint, expected_fingerprint);
        request.reply.send(true).unwrap();
        completed_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("Ed25519 authentication timed out")
            .expect("Ed25519 authentication failed");
        handle.join().unwrap();
    }
}
