//! One process per connection-store directory, with no lock file or registry modification.
use sha2::{Digest, Sha256};
use std::ffi::{OsString, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(attributes: *const c_void, initial_owner: i32, name: *const u16) -> isize;
    fn CloseHandle(handle: isize) -> i32;
    fn GetLastError() -> u32;
    fn GetFullPathNameW(
        name: *const u16,
        size: u32,
        buffer: *mut u16,
        file_part: *mut *mut u16,
    ) -> u32;
}

pub struct InstanceGuard {
    handle: isize,
}
impl InstanceGuard {
    pub fn acquire(data_dir: &Path) -> Result<Self, String> {
        let name = mutex_name(data_dir)?;
        let encoded: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // Ownership is not requested: keeping the named kernel object alive is the lock.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, encoded.as_ptr()) };
        let error = unsafe { GetLastError() };
        if handle == 0 {
            return Err(format!(
                "Impossible de protéger le carnet de connexions contre une ouverture simultanée (erreur Windows {error})."
            ));
        }
        if error == 183 {
            unsafe {
                CloseHandle(handle);
            }
            return Err("Nexus Remote est déjà ouvert avec ce dossier de données. Utilisez la fenêtre existante.".into());
        }
        Ok(Self { handle })
    }
}
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

fn mutex_name(data_dir: &Path) -> Result<String, String> {
    let encoded: Vec<u16> = data_dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    if encoded[..encoded.len() - 1].contains(&0) {
        return Err("Le dossier de données contient un caractère nul.".into());
    }
    let mut full = vec![0u16; 32768];
    let count = unsafe {
        GetFullPathNameW(
            encoded.as_ptr(),
            full.len() as u32,
            full.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    } as usize;
    if count == 0 || count >= full.len() {
        return Err("Le chemin du dossier de données est invalide ou trop long.".into());
    }
    full.truncate(count);
    let path = PathBuf::from(String::from_utf16_lossy(&full));
    // Resolve the nearest existing ancestor so the name remains stable before and after
    // the first save creates the profile directory, including beneath a junction.
    let mut ancestor = path.as_path();
    let mut suffix: Vec<OsString> = Vec::new();
    let resolved = loop {
        if let Ok(mut existing) = std::fs::canonicalize(ancestor) {
            for component in suffix.iter().rev() {
                existing.push(component);
            }
            break existing;
        }
        let Some(name) = ancestor.file_name() else {
            break path.clone();
        };
        suffix.push(name.to_owned());
        let Some(parent) = ancestor.parent() else {
            break path.clone();
        };
        ancestor = parent;
    };
    let text = resolved.to_string_lossy();
    let text = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    };
    let normalized = text
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    Ok(format!("Local\\NexusRemoteRust-{:x}", digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_directory_and_releases_on_drop() {
        let directory =
            std::env::temp_dir().join(format!("nexus-instance-test-{}", uuid::Uuid::new_v4()));
        let first = InstanceGuard::acquire(&directory).unwrap();
        assert!(InstanceGuard::acquire(&directory).is_err());
        let other = InstanceGuard::acquire(&directory.with_extension("other")).unwrap();
        drop(other);
        drop(first);
        assert!(InstanceGuard::acquire(&directory).is_ok());
    }
    #[test]
    fn relative_components_and_case_share_mutex_name() {
        let directory = std::env::temp_dir().join("nexus-instance-case-test");
        let alias = directory
            .parent()
            .unwrap()
            .join(".")
            .join("NEXUS-INSTANCE-CASE-TEST");
        assert_eq!(mutex_name(&directory).unwrap(), mutex_name(&alias).unwrap());
    }
}
