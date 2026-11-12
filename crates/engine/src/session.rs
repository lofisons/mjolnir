//! Local project safety: complete recovery files include embedded samples.
use crate::model::Project;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
pub struct Recovery {
    pub json: String,
    pub path: Option<String>,
    pub baseline: Project,
}

pub struct Session {
    pub path: Option<String>,
    pub baseline: Project,
    pub recovery_path: PathBuf,
}
impl Session {
    pub fn new(baseline: Project, recovery_path: PathBuf) -> Self {
        Self {
            path: None,
            baseline,
            recovery_path,
        }
    }
    pub fn dirty(&self, project: &Project) -> bool {
        project != &self.baseline
    }
    pub fn clean(&mut self, project: &Project, path: Option<String>) {
        self.baseline = project.clone();
        self.path = path;
    }
    pub fn autosave(&self, json: String) -> Result<(), String> {
        let recovery = Recovery {
            json,
            path: self.path.clone(),
            baseline: self.baseline.clone(),
        };
        atomic_write(
            &self.recovery_path,
            &serde_json::to_vec(&recovery).map_err(|e| e.to_string())?,
        )
    }
    pub fn recovery(&self) -> Result<Option<Recovery>, String> {
        match std::fs::read(&self.recovery_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| format!("Recovery file could not be read: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn discard_recovery(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.recovery_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Write alongside the destination then rename, preserving the last complete
/// project if serialization or writing fails. Never runs on the audio thread.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut name = path.file_name().ok_or("Invalid file name")?.to_os_string();
    name.push(".tmp");
    let tmp = parent.join(name);
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_preserves_document_samples_path_and_dirty_baseline() {
        let dir = std::env::temp_dir().join(format!("yggdrasil-session-{}", std::process::id()));
        let mut session = Session::new(Project::default(), dir.join("recovery.json"));
        session.path = Some("/music/song.yggdrasil".into());
        let json = r#"{"project":"complete", "samples":{"1":"embedded-wav"}}"#.to_string();
        session.autosave(json.clone()).unwrap();
        let recovered = session.recovery().unwrap().unwrap();
        assert_eq!(recovered.json, json);
        assert_eq!(recovered.path, session.path);
        let mut changed = session.baseline.clone();
        changed.bpm = 90.0;
        assert!(session.dirty(&changed));
        session.clean(&changed, None);
        assert!(!session.dirty(&changed));
        session.discard_recovery().unwrap();
        assert!(session.recovery().unwrap().is_none());
        std::fs::remove_dir(dir).unwrap();
    }
}
