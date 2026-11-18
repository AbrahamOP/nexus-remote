use crate::model::{
    AuthMode, GroupConfig, Identity, Preferences, Profile, Protocol, Snippet, Workspace,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub version: u32,
    pub profiles: Vec<Profile>,
    pub known_hosts: BTreeMap<String, String>,
    pub identities: Vec<Identity>,
    pub snippets: Vec<Snippet>,
    pub groups: Vec<GroupConfig>,
    pub workspaces: Vec<Workspace>,
    pub tunnel_rules: Vec<crate::tunnels::TunnelRule>,
    pub preferences: Preferences,
}
impl State {
    pub fn resolved_profile(&self, profile: &Profile) -> Result<Profile, String> {
        profile.validate()?;
        let mut resolved = profile.clone();
        if let Some(id) = &profile.identity_id {
            let identity = self
                .identities
                .iter()
                .find(|identity| &identity.id == id)
                .ok_or_else(|| {
                    format!(
                        "L’identité de la connexion « {} » est introuvable.",
                        profile.name
                    )
                })?;
            identity.validate()?;
            if profile.protocol == Protocol::Rdp && identity.auth == AuthMode::PrivateKey {
                return Err("Une identité par clé SSH ne peut pas être utilisée pour RDP.".into());
            }
            resolved.username = identity.username.clone();
            resolved.auth = identity.auth;
            resolved.private_key_path = identity.private_key_path.clone();
            resolved.encrypted_secret = identity.encrypted_secret.clone();
            resolved.identity_id = None;
        }
        resolved.validate()?;
        Ok(resolved)
    }
    /// Called when creating/changing a group in the editor. Existing explicit ports
    /// and credentials are never silently changed when a group is later edited.
    pub fn apply_group_defaults(&self, profile: &mut Profile) {
        if let Some(group) = self.groups.iter().find(|group| group.name == profile.group) {
            if profile.username.is_empty() {
                profile.username = group.default_username.clone();
            }
            if profile.identity_id.is_none() {
                profile.identity_id = group.default_identity_id.clone();
            }
            if profile.port == 0 {
                profile.port = group
                    .default_port
                    .unwrap_or_else(|| profile.protocol.port());
            }
        }
    }
    pub fn mark_used(&mut self, profile_id: &str) {
        if let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
        {
            profile.last_used = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
        }
    }
    /// Remove dependencies as one transaction before Store::save.
    pub fn remove_profile(&mut self, profile_id: &str) {
        self.profiles.retain(|profile| profile.id != profile_id);
        self.tunnel_rules
            .retain(|rule| rule.profile_id != profile_id);
        for workspace in &mut self.workspaces {
            workspace.profile_ids.retain(|id| id != profile_id);
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Version du fichier de données non prise en charge.".into());
        }
        if self.profiles.len() > 10000
            || self.identities.len() > 10000
            || self.snippets.len() > 10000
            || self.groups.len() > 10000
            || self.workspaces.len() > 1000
            || self.tunnel_rules.len() > 1000
        {
            return Err("Le carnet contient trop d’éléments.".into());
        }
        validate_unique(self.profiles.iter().map(|p| p.id.as_str()), "connexions")?;
        validate_unique(self.identities.iter().map(|p| p.id.as_str()), "identités")?;
        validate_unique(self.snippets.iter().map(|p| p.id.as_str()), "commandes")?;
        validate_unique(self.groups.iter().map(|p| p.id.as_str()), "groupes")?;
        validate_unique(
            self.groups.iter().map(|p| p.name.as_str()),
            "noms de groupes",
        )?;
        validate_unique(
            self.workspaces.iter().map(|p| p.id.as_str()),
            "espaces de travail",
        )?;
        validate_unique(self.tunnel_rules.iter().map(|p| p.id.as_str()), "tunnels")?;
        for identity in &self.identities {
            identity.validate()?;
        }
        for profile in &self.profiles {
            self.resolved_profile(profile)?;
        }
        for snippet in &self.snippets {
            snippet.validate()?;
        }
        for group in &self.groups {
            group.validate()?;
            if group
                .default_identity_id
                .as_ref()
                .is_some_and(|id| !self.identities.iter().any(|identity| &identity.id == id))
            {
                return Err(format!(
                    "L’identité par défaut du groupe « {} » est introuvable.",
                    group.name
                ));
            }
        }
        for workspace in &self.workspaces {
            workspace.validate()?;
            if workspace
                .profile_ids
                .iter()
                .any(|id| !self.profiles.iter().any(|profile| &profile.id == id))
            {
                return Err(format!(
                    "L’espace de travail « {} » référence une connexion absente.",
                    workspace.name
                ));
            }
        }
        for rule in &self.tunnel_rules {
            rule.validate()?;
            let profile = self
                .profiles
                .iter()
                .find(|profile| profile.id == rule.profile_id)
                .ok_or_else(|| {
                    format!(
                        "Le tunnel « {} » référence une connexion absente.",
                        rule.name
                    )
                })?;
            if profile.protocol == Protocol::Rdp {
                return Err("Un tunnel SSH doit utiliser une connexion SSH ou SFTP.".into());
            }
        }
        self.preferences.validate()
    }
}
fn validate_unique<'a>(ids: impl Iterator<Item = &'a str>, kind: &str) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    if ids.into_iter().any(|id| !seen.insert(id)) {
        return Err(format!("Identifiants de {kind} dupliqués."));
    }
    Ok(())
}
pub struct Store {
    pub state: State,
    pub data_dir: PathBuf,
}
fn user_local_data_dir(
    local: Option<OsString>,
    profile: Option<OsString>,
    roaming: Option<OsString>,
) -> Option<PathBuf> {
    let absolute = |value: Option<OsString>| value.map(PathBuf::from).filter(|p| p.is_absolute());
    absolute(local)
        .or_else(|| absolute(profile).map(|p| p.join("AppData").join("Local")))
        .or_else(|| absolute(roaming))
}
impl Store {
    pub fn default_dir() -> PathBuf {
        user_local_data_dir(
            std::env::var_os("LOCALAPPDATA"),
            std::env::var_os("USERPROFILE"),
            std::env::var_os("APPDATA"),
        )
        .map(|base| base.join("NexusRemoteRust"))
        .unwrap_or_default()
    }
    pub fn empty() -> Self {
        Self {
            state: State {
                version: 1,
                ..Default::default()
            },
            data_dir: Self::default_dir(),
        }
    }
    pub fn load() -> Result<Self, String> {
        let data_dir = Self::default_dir();
        if data_dir.as_os_str().is_empty() {
            return Err("Dossier utilisateur Windows introuvable : LOCALAPPDATA, USERPROFILE et APPDATA sont absents ou invalides.".into());
        }
        Self::load_at(data_dir)
    }
    pub fn load_at(data_dir: PathBuf) -> Result<Self, String> {
        let file = data_dir.join("connections.json");
        let state = if file.exists() {
            if fs::metadata(&file).map_err(|e| e.to_string())?.len() > 10_000_000 {
                return Err("Fichier de données trop volumineux.".into());
            }
            let bytes = fs::read(&file).map_err(|e| e.to_string())?;
            if bytes.len() > 10_000_000 {
                return Err("Fichier de données trop volumineux.".into());
            }
            let state: State = serde_json::from_slice(&bytes)
                .map_err(|e| format!("Fichier de connexions invalide : {e}"))?;
            state.validate()?;
            state
        } else {
            State {
                version: 1,
                ..Default::default()
            }
        };
        Ok(Self { state, data_dir })
    }
    pub fn save(&self) -> Result<(), String> {
        if self.data_dir.as_os_str().is_empty() {
            return Err(
                "Dossier utilisateur Windows introuvable. Le carnet n’a pas été écrit.".into(),
            );
        }
        self.state.validate()?;
        let bytes = serde_json::to_vec_pretty(&self.state).map_err(|e| e.to_string())?;
        if bytes.len() > 10_000_000 {
            return Err(
                "Le carnet dépasse 10 Mo. Retirez des éléments avant de l’enregistrer.".into(),
            );
        }
        self.backup_legacy_data()?;
        atomic_write(&self.data_dir.join("connections.json"), &bytes)
    }
    /// The first v0.2 save keeps an unchanged v0.1 copy. It never overwrites an
    /// existing backup, and never decrypts a secret to migrate the data.
    fn backup_legacy_data(&self) -> Result<(), String> {
        let current = self.data_dir.join("connections.json");
        if !current.exists() {
            return Ok(());
        }
        if fs::metadata(&current).map_err(|e| e.to_string())?.len() > 10_000_000 {
            return Err("Fichier de données trop volumineux.".into());
        }
        let bytes = fs::read(&current).map_err(|e| e.to_string())?;
        let previous: serde_json::Value = serde_json::from_slice(&bytes).map_err(
            |_| "Le fichier de données a été modifié et n’est plus valide. Il a été conservé.",
        )?;
        let is_legacy = previous.as_object().is_some_and(|object| {
            !object.contains_key("identities") || !object.contains_key("preferences")
        });
        if !is_legacy {
            return Ok(());
        }
        let backup = self.data_dir.join("connections.v0.1-backup.json");
        match File::options().write(true).create_new(true).open(&backup) {
            Ok(mut file) => {
                let result = file.write_all(&bytes).and_then(|_| file.sync_all());
                drop(file);
                if let Err(error) = result {
                    let _ = fs::remove_file(&backup);
                    return Err(format!(
                        "Impossible de sauvegarder l’ancien carnet : {error}"
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(format!(
                    "Impossible de sauvegarder l’ancien carnet : {error}"
                ));
            }
        }
        Ok(())
    }
    pub fn export(&self, path: &Path) -> Result<(), String> {
        let current = self.data_dir.join("connections.json");
        let target = if path.exists() {
            fs::canonicalize(path).ok()
        } else {
            None
        };
        let live = if current.exists() {
            fs::canonicalize(&current).ok()
        } else {
            None
        };
        if path == current || (target.is_some() && target == live) {
            return Err("Choisissez un autre fichier : vous ne pouvez pas remplacer la base de connexions par un export.".into());
        }
        let profiles: Vec<Profile> = self
            .state
            .profiles
            .iter()
            .map(|profile| {
                let mut resolved = self.state.resolved_profile(profile)?;
                resolved.encrypted_secret = None;
                resolved.identity_id = None;
                Ok(resolved)
            })
            .collect::<Result<_, String>>()?;
        atomic_write(
            path,
            &serde_json::to_vec_pretty(&profiles).map_err(|e| e.to_string())?,
        )
    }
    pub fn import(&mut self, path: &Path) -> Result<usize, String> {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() > 5_000_000 {
            return Err("Le fichier dépasse 5 Mo.".into());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 5_000_000 {
            return Err("Le fichier dépasse 5 Mo.".into());
        }
        let mut profiles: Vec<Profile> =
            serde_json::from_slice(&bytes).map_err(|e| format!("Import JSON invalide : {e}"))?;
        if profiles.len() > 10000 {
            return Err("Trop de connexions.".into());
        }
        for p in &mut profiles {
            // An imported file cannot refer to a local identity by guessing its ID.
            p.identity_id = None;
            p.validate()?;
            p.id = uuid::Uuid::new_v4().to_string();
            p.encrypted_secret = None;
        }
        let count = profiles.len();
        let original_len = self.state.profiles.len();
        self.state.profiles.extend(profiles);
        if let Err(e) = self.save() {
            self.state.profiles.truncate(original_len);
            return Err(e);
        }
        Ok(count)
    }
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Dossier invalide")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".nexus-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = File::options().write(true).create_new(true).open(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(e.to_string());
    }
    Ok(())
}
pub fn protect(secret: &str) -> Result<String, String> {
    if secret.len() > 65536 {
        return Err("Le secret dépasse 64 Ko.".into());
    }
    use windows::{
        Win32::{
            Foundation::{HLOCAL, LocalFree},
            Security::Cryptography::{
                CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
            },
        },
        core::PCWSTR,
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|e| format!("Chiffrement Windows indisponible : {e}"))?;
        let encoded = STANDARD.encode(std::slice::from_raw_parts(
            output.pbData,
            output.cbData as usize,
        ));
        LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(encoded)
    }
}
pub fn unprotect(encoded: &str) -> Result<String, String> {
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
        },
    };
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Secret chiffré invalide")?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|_| {
            "Le secret est illisible sur ce compte Windows. Saisissez-le à nouveau.".to_string()
        })?;
        let secret = if output.cbData == 0 {
            Ok(String::new())
        } else {
            String::from_utf8(
                std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec(),
            )
            .map_err(|e| e.to_string())
        };
        if output.cbData > 0 {
            std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        }
        LocalFree(Some(HLOCAL(output.pbData.cast())));
        secret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_data_location_never_uses_a_shared_temp_directory() {
        let profile = PathBuf::from(r"C:\Users\fixture");
        let local = profile.join("AppData").join("Local");
        let roaming = profile.join("AppData").join("Roaming");
        assert_eq!(user_local_data_dir(None, None, None), None);
        assert_eq!(
            user_local_data_dir(None, Some(profile.clone().into_os_string()), None),
            Some(local.clone())
        );
        assert_eq!(
            user_local_data_dir(
                Some(local.clone().into_os_string()),
                Some(profile.into_os_string()),
                Some(roaming.into_os_string()),
            ),
            Some(local)
        );
        assert_eq!(
            user_local_data_dir(Some("relative\\temp".into()), None, None),
            None
        );
        let mut store = Store::empty();
        store.data_dir = PathBuf::new();
        assert!(store.save().is_err());
    }
    fn test_dir() -> PathBuf {
        std::env::temp_dir().join(format!("nexus-rust-test-{}", uuid::Uuid::new_v4()))
    }
    fn p() -> Profile {
        Profile {
            name: "Test".into(),
            host: "127.0.0.1".into(),
            username: "tester".into(),
            ..Default::default()
        }
    }
    #[test]
    fn dpapi_roundtrip() {
        let encoded = protect("é-test-secret-✓").unwrap();
        assert!(!encoded.contains("test-secret"));
        assert_eq!(unprotect(&encoded).unwrap(), "é-test-secret-✓");
        assert!(unprotect("corrupted").is_err());
    }
    #[test]
    fn persistence_export_import_and_invalid_import_are_atomic() {
        let root = test_dir();
        let mut s = Store::load_at(root.clone()).unwrap();
        let mut profile = p();
        profile.encrypted_secret = Some(protect("do-not-export").unwrap());
        let id = profile.id.clone();
        s.state.profiles.push(profile);
        s.save().unwrap();
        let reloaded = Store::load_at(root.clone()).unwrap();
        assert_eq!(reloaded.state.profiles[0].name, "Test");
        assert!(
            !fs::read_to_string(root.join("connections.json"))
                .unwrap()
                .contains("do-not-export")
        );
        let export = root.join("export.json");
        s.export(&export).unwrap();
        let data = fs::read_to_string(&export).unwrap();
        let exported: Vec<Profile> = serde_json::from_str(&data).unwrap();
        assert!(exported[0].encrypted_secret.is_none());
        assert_eq!(s.import(&export).unwrap(), 1);
        assert_ne!(s.state.profiles[1].id, id);
        assert!(s.state.profiles[1].encrypted_secret.is_none());
        let mut invalid = vec![p(), p()];
        invalid[1].host = "\ninjected".into();
        let invalid_path = root.join("invalid.json");
        fs::write(&invalid_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        let previous = fs::read(root.join("connections.json")).unwrap();
        assert!(s.import(&invalid_path).is_err());
        assert_eq!(s.state.profiles.len(), 2);
        assert_eq!(fs::read(root.join("connections.json")).unwrap(), previous);
    }
    #[test]
    fn export_cannot_corrupt_live_store() {
        let dir = test_dir();
        let s = Store::load_at(dir.clone()).unwrap();
        s.save().unwrap();
        let path = dir.join("connections.json");
        let before = fs::read(&path).unwrap();
        assert!(s.export(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), before);
    }
    #[test]
    fn import_discards_untrusted_encrypted_secret() {
        let dir = test_dir();
        let mut s = Store::load_at(dir.clone()).unwrap();
        fs::create_dir_all(&dir).unwrap();
        let mut profile = p();
        profile.encrypted_secret = Some("untrusted opaque data".into());
        let path = dir.join("import.json");
        fs::write(&path, serde_json::to_vec(&vec![profile]).unwrap()).unwrap();
        s.import(&path).unwrap();
        assert!(s.state.profiles[0].encrypted_secret.is_none());
    }
    #[test]
    fn old_v1_data_keeps_secrets_and_creates_exact_backup_only_on_save() {
        let dir = test_dir();
        fs::create_dir_all(&dir).unwrap();
        let encrypted = protect("old-fixture-secret").unwrap();
        let legacy = serde_json::json!({"version":1,"profiles":[{"id":"legacy","name":"Legacy host","host":"127.0.0.1","port":22,"username":"test","encrypted_secret":encrypted}],"known_hosts":{"[127.0.0.1]:22":"SHA256:fixture"}});
        let bytes = serde_json::to_vec(&legacy).unwrap();
        fs::write(dir.join("connections.json"), &bytes).unwrap();
        let mut store = Store::load_at(dir.clone()).unwrap();
        assert!(store.state.identities.is_empty());
        assert!(store.state.profiles[0].tags.is_empty());
        assert_eq!(store.state.preferences.terminal_font_size, 14.0);
        assert_eq!(
            unprotect(store.state.profiles[0].encrypted_secret.as_ref().unwrap()).unwrap(),
            "old-fixture-secret"
        );
        assert!(!dir.join("connections.v0.1-backup.json").exists());
        store.state.profiles[0].description = "Updated description".into();
        store.save().unwrap();
        assert_eq!(
            fs::read(dir.join("connections.v0.1-backup.json")).unwrap(),
            bytes
        );
        store.save().unwrap();
        assert_eq!(
            fs::read(dir.join("connections.v0.1-backup.json")).unwrap(),
            bytes
        );
    }
    #[test]
    fn reusable_identity_exports_portable_connection_without_secret_or_reference() {
        let dir = test_dir();
        let mut store = Store::load_at(dir.clone()).unwrap();
        let identity = Identity {
            name: "Administration".into(),
            username: "identity-user".into(),
            encrypted_secret: Some(protect("identity-secret").unwrap()),
            ..Default::default()
        };
        let identity_id = identity.id.clone();
        let ciphertext = identity.encrypted_secret.clone().unwrap();
        store.state.identities.push(identity);
        let mut profile = p();
        profile.identity_id = Some(identity_id);
        profile.username.clear();
        store.state.profiles.push(profile);
        store.save().unwrap();
        let resolved = store
            .state
            .resolved_profile(&store.state.profiles[0])
            .unwrap();
        assert_eq!(resolved.username, "identity-user");
        assert_eq!(
            unprotect(resolved.encrypted_secret.as_ref().unwrap()).unwrap(),
            "identity-secret"
        );
        assert!(resolved.identity_id.is_none());
        let path = dir.join("export.json");
        store.export(&path).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(&ciphertext));
        let exported: Vec<Profile> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(exported[0].username, "identity-user");
        assert!(exported[0].identity_id.is_none());
        assert!(exported[0].encrypted_secret.is_none());
    }
    #[test]
    fn workspace_and_tunnel_references_are_removed_with_profile() {
        let mut state = State {
            version: 1,
            ..Default::default()
        };
        let profile = p();
        let id = profile.id.clone();
        state.profiles.push(profile);
        state.workspaces.push(Workspace {
            name: "Monitoring".into(),
            profile_ids: vec![id.clone()],
            ..Default::default()
        });
        state.tunnel_rules.push(crate::tunnels::TunnelRule {
            name: "Web tunnel".into(),
            profile_id: id.clone(),
            ..Default::default()
        });
        state.validate().unwrap();
        state.remove_profile(&id);
        assert!(state.profiles.is_empty());
        assert!(state.tunnel_rules.is_empty());
        assert!(state.workspaces[0].profile_ids.is_empty());
        state.validate().unwrap();
    }
    #[test]
    fn group_defaults_only_fill_new_unset_values() {
        let mut state = State {
            version: 1,
            ..Default::default()
        };
        state.groups.push(GroupConfig {
            name: "Personnel".into(),
            default_username: "group-user".into(),
            default_port: Some(2222),
            ..Default::default()
        });
        let mut existing = p();
        state.apply_group_defaults(&mut existing);
        assert_eq!(existing.username, "tester");
        assert_eq!(existing.port, 22);
        let mut new = p();
        new.username.clear();
        new.port = 0;
        state.apply_group_defaults(&mut new);
        assert_eq!(new.username, "group-user");
        assert_eq!(new.port, 2222);
    }
    #[test]
    fn snippets_and_preferences_persist_and_validate_references() {
        let dir = test_dir();
        let mut store = Store::load_at(dir.clone()).unwrap();
        store.state.snippets.push(Snippet {
            name: "Disk usage".into(),
            command: "df -h\nlsblk".into(),
            group: "Maintenance".into(),
            ..Default::default()
        });
        store.state.preferences.terminal_theme = "dracula".into();
        store.state.preferences.host_view_list = true;
        store.save().unwrap();
        let reloaded = Store::load_at(dir).unwrap();
        assert_eq!(reloaded.state.snippets[0].command, "df -h\nlsblk");
        assert_eq!(reloaded.state.preferences.terminal_theme, "dracula");
        assert!(reloaded.state.preferences.host_view_list);
        store.state.workspaces.push(Workspace {
            name: "Broken".into(),
            profile_ids: vec!["unknown-profile".into()],
            ..Default::default()
        });
        assert!(store.state.validate().is_err());
    }
}
