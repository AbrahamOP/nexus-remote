use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    Ssh,
    Sftp,
    Rdp,
}
impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ssh => "SSH",
            Self::Sftp => "SFTP",
            Self::Rdp => "RDP",
        }
    }
    pub fn port(self) -> u16 {
        if self == Self::Rdp { 3389 } else { 22 }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMode {
    #[default]
    Password,
    PrivateKey,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group: String,
    pub protocol: Protocol,
    pub auth: AuthMode,
    pub private_key_path: String,
    pub remote_path: String,
    pub domain: String,
    pub redirect_clipboard: bool,
    pub allow_unverified_rdp_certificate: bool,
    pub favorite: bool,
    pub encrypted_secret: Option<String>,
    pub tags: Vec<String>,
    pub description: String,
    pub identity_id: Option<String>,
    pub last_used: Option<u64>,
    pub color: String,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            host: String::new(),
            port: 22,
            username: String::new(),
            group: "Personnel".into(),
            protocol: Protocol::Ssh,
            auth: AuthMode::Password,
            private_key_path: String::new(),
            remote_path: ".".into(),
            domain: String::new(),
            redirect_clipboard: false,
            allow_unverified_rdp_certificate: false,
            favorite: false,
            encrypted_secret: None,
            tags: Vec::new(),
            description: String::new(),
            identity_id: None,
            last_used: None,
            color: String::new(),
        }
    }
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 100 || self.id.chars().any(char::is_control) {
            return Err("Identifiant de connexion invalide.".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 200
            || self.name.chars().any(char::is_control)
        {
            return Err("Donnez un nom à la connexion (200 caractères maximum).".into());
        }
        if self.host.trim().is_empty()
            || self.host.len() > 253
            || self
                .host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '/' | '\\' | '"'))
        {
            return Err("Indiquez une adresse IP ou un nom d’hôte, sans protocole ni port.".into());
        }
        if self.port == 0 {
            return Err("Le port doit être compris entre 1 et 65535.".into());
        }
        if (self.username.trim().is_empty()
            && self.protocol != Protocol::Rdp
            && self.identity_id.is_none())
            || self.username.len() > 256
            || self.username.chars().any(char::is_control)
        {
            return Err("Indiquez un utilisateur valide.".into());
        }
        if self.domain.len() > 253
            || self.domain.chars().any(char::is_control)
            || self.group.len() > 100
            || self.group.chars().any(char::is_control)
        {
            return Err("Groupe ou domaine invalide.".into());
        }
        if self.protocol != Protocol::Rdp
            && self.auth == AuthMode::PrivateKey
            && self.private_key_path.is_empty()
            && self.identity_id.is_none()
        {
            return Err("Sélectionnez une clé privée SSH.".into());
        }
        if self.remote_path.len() > 4096 || self.remote_path.contains('\0') {
            return Err("Chemin SFTP invalide.".into());
        }
        if self.private_key_path.len() > 32768 || self.private_key_path.contains('\0') {
            return Err("Chemin de clé privée invalide.".into());
        }
        if self
            .encrypted_secret
            .as_ref()
            .is_some_and(|s| s.len() > 100_000)
        {
            return Err("Secret de connexion trop volumineux.".into());
        }
        validate_optional_id(self.identity_id.as_deref(), "Identité")?;
        validate_description(&self.description)?;
        validate_color(&self.color)?;
        if self.tags.len() > 30
            || self.tags.iter().any(|tag| {
                tag.trim().is_empty() || tag.len() > 50 || tag.chars().any(char::is_control)
            })
        {
            return Err("Utilisez au maximum 30 étiquettes de 50 caractères.".into());
        }
        Ok(())
    }
}

/// A reusable login. Secrets are DPAPI ciphertext, never plain text.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub username: String,
    pub auth: AuthMode,
    pub private_key_path: String,
    pub encrypted_secret: Option<String>,
}
impl Default for Identity {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            username: String::new(),
            auth: AuthMode::Password,
            private_key_path: String::new(),
            encrypted_secret: None,
        }
    }
}
impl Identity {
    pub fn validate(&self) -> Result<(), String> {
        validate_named(&self.id, &self.name, "identité")?;
        if self.username.trim().is_empty()
            || self.username.len() > 256
            || self.username.chars().any(char::is_control)
        {
            return Err("Indiquez un utilisateur valide pour l’identité.".into());
        }
        if self.auth == AuthMode::PrivateKey && self.private_key_path.trim().is_empty() {
            return Err("Sélectionnez la clé privée de l’identité.".into());
        }
        if self.private_key_path.len() > 32768 || self.private_key_path.contains('\0') {
            return Err("Chemin de clé privée invalide.".into());
        }
        if self
            .encrypted_secret
            .as_ref()
            .is_some_and(|s| s.len() > 100_000)
        {
            return Err("Secret d’identité trop volumineux.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub command: String,
    pub group: String,
}
impl Default for Snippet {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            command: String::new(),
            group: "Personnel".into(),
        }
    }
}
impl Snippet {
    pub fn validate(&self) -> Result<(), String> {
        validate_named(&self.id, &self.name, "commande")?;
        if self.command.trim().is_empty()
            || self.command.len() > 65536
            || self.command.contains('\0')
        {
            return Err(
                "La commande doit contenir entre 1 et 65 536 octets, sans caractère nul.".into(),
            );
        }
        if self.group.len() > 100 || self.group.chars().any(char::is_control) {
            return Err("Groupe de commande invalide.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub default_username: String,
    pub default_port: Option<u16>,
    pub default_identity_id: Option<String>,
}
impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            description: String::new(),
            color: String::new(),
            default_username: String::new(),
            default_port: None,
            default_identity_id: None,
        }
    }
}
impl GroupConfig {
    pub fn validate(&self) -> Result<(), String> {
        validate_named(&self.id, &self.name, "groupe")?;
        if self.name.len() > 100
            || self.default_username.len() > 256
            || self.default_username.chars().any(char::is_control)
            || self.default_port == Some(0)
        {
            return Err("Paramètres par défaut du groupe invalides.".into());
        }
        validate_description(&self.description)?;
        validate_color(&self.color)?;
        validate_optional_id(self.default_identity_id.as_deref(), "Identité du groupe")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub profile_ids: Vec<String>,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            profile_ids: Vec::new(),
        }
    }
}
impl Workspace {
    pub fn validate(&self) -> Result<(), String> {
        validate_named(&self.id, &self.name, "espace de travail")?;
        if self.profile_ids.len() > 100
            || self
                .profile_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 100)
        {
            return Err("Un espace de travail peut contenir au maximum 100 connexions.".into());
        }
        let mut unique = std::collections::HashSet::new();
        if self.profile_ids.iter().any(|id| !unique.insert(id)) {
            return Err("Une connexion apparaît plusieurs fois dans cet espace de travail.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub terminal_font_size: f32,
    pub terminal_theme: String,
    pub host_view_list: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            terminal_font_size: 14.0,
            terminal_theme: "nexus".into(),
            host_view_list: false,
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if !self.terminal_font_size.is_finite() || !(8.0..=32.0).contains(&self.terminal_font_size)
        {
            return Err("La taille du terminal doit être comprise entre 8 et 32.".into());
        }
        if self.terminal_theme.is_empty()
            || self.terminal_theme.len() > 50
            || self.terminal_theme.chars().any(char::is_control)
        {
            return Err("Thème de terminal invalide.".into());
        }
        Ok(())
    }
}

fn validate_named(id: &str, name: &str, kind: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 100 || id.chars().any(char::is_control) {
        return Err(format!("Identifiant de {kind} invalide."));
    }
    if name.trim().is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
        return Err(format!(
            "Donnez un nom valide au {kind} (200 caractères maximum)."
        ));
    }
    Ok(())
}
fn validate_description(description: &str) -> Result<(), String> {
    if description.len() > 8192 || description.contains('\0') {
        return Err("La description dépasse 8 Ko ou contient un caractère nul.".into());
    }
    Ok(())
}
fn validate_optional_id(id: Option<&str>, field: &str) -> Result<(), String> {
    if id.is_some_and(|id| id.is_empty() || id.len() > 100 || id.chars().any(char::is_control)) {
        return Err(format!("{field} invalide."));
    }
    Ok(())
}
fn validate_color(color: &str) -> Result<(), String> {
    if !color.is_empty()
        && (color.len() != 7
            || !color.starts_with('#')
            || !color[1..].bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("La couleur doit être vide ou au format #RRGGBB.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> Profile {
        Profile {
            name: "Test".into(),
            host: "127.0.0.1".into(),
            username: "test".into(),
            ..Default::default()
        }
    }
    #[test]
    fn reject_injected_host() {
        let mut p = valid();
        p.host = "server\nusername:s:attacker".into();
        assert!(p.validate().is_err());
    }
    #[test]
    fn reject_invalid_port_and_empty_user() {
        let mut p = valid();
        p.port = 0;
        assert!(p.validate().is_err());
        p.port = 22;
        p.username.clear();
        assert!(p.validate().is_err());
    }
    #[test]
    fn accept_ipv6_and_rdp_prompt() {
        let mut p = valid();
        p.host = "2001:db8::1".into();
        assert!(p.validate().is_ok());
        p.protocol = Protocol::Rdp;
        p.username.clear();
        assert!(p.validate().is_ok());
    }

    #[test]
    fn legacy_rdp_profile_requires_server_authentication_until_explicitly_enabled() {
        let mut profile = valid();
        profile.protocol = Protocol::Rdp;
        let mut legacy = serde_json::to_value(&profile).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("allow_unverified_rdp_certificate");
        let loaded: Profile = serde_json::from_value(legacy).unwrap();
        assert!(!loaded.allow_unverified_rdp_certificate);

        profile.allow_unverified_rdp_certificate = true;
        let loaded: Profile =
            serde_json::from_value(serde_json::to_value(profile).unwrap()).unwrap();
        assert!(loaded.allow_unverified_rdp_certificate);
    }
}
