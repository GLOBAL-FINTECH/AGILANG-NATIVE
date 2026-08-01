use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        #[cfg(windows)]
        {
            use std::ffi::c_void;
            #[link(name = "bcrypt")]
            extern "system" {
                fn BCryptGenRandom(
                    hAlgorithm: *mut c_void,
                    pbBuffer: *mut u8,
                    cbBuffer: u32,
                    dwFlags: u32,
                ) -> i32;
            }
            unsafe {
                let _ = BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), 32, 2);
            }
        }
        #[cfg(not(windows))]
        {
            if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
                use std::io::Read;
                let _ = f.read_exact(&mut bytes);
            }
        }
        let hex = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        SessionId(hex)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub user_id: String,
    pub created_at: SystemTime,
    pub last_seen_at: SystemTime,
    pub expires_at: SystemTime,
    pub csrf_secret: String,
    pub ip_hash: Option<String>,
    pub user_agent_hash: Option<String>,
}

impl Session {
    pub fn new(user_id: impl Into<String>, lifetime: Duration) -> Self {
        let now = SystemTime::now();
        Self {
            id: SessionId::generate(),
            user_id: user_id.into(),
            created_at: now,
            last_seen_at: now,
            expires_at: now + lifetime,
            csrf_secret: SessionId::generate().0,
            ip_hash: None,
            user_agent_hash: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }

    pub fn is_idle(&self, idle_timeout: Duration) -> bool {
        if let Ok(elapsed) = SystemTime::now().duration_since(self.last_seen_at) {
            elapsed > idle_timeout
        } else {
            false
        }
    }

    pub fn touch(&mut self) {
        self.last_seen_at = SystemTime::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieConfig {
    pub name: String,
    pub lifetime_seconds: u64,
    pub same_site: String,
    pub http_only: bool,
    pub secure: bool,
    pub path: String,
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            name: "agilang_session".to_string(),
            lifetime_seconds: 7200,
            same_site: "Lax".to_string(),
            http_only: true,
            secure: false,
            path: "/".to_string(),
        }
    }
}

impl CookieConfig {
    pub fn build_header(&self, session_id: &str) -> String {
        format!(
            "{}={}; Path={}; Max-Age={}; SameSite={}{}{}",
            self.name,
            session_id,
            self.path,
            self.lifetime_seconds,
            self.same_site,
            if self.http_only { "; HttpOnly" } else { "" },
            if self.secure { "; Secure" } else { "" }
        )
    }

    pub fn build_logout_header(&self) -> String {
        format!(
            "{}=; Path={}; Max-Age=0; SameSite={}{}{}",
            self.name,
            self.path,
            self.same_site,
            if self.http_only { "; HttpOnly" } else { "" },
            if self.secure { "; Secure" } else { "" }
        )
    }
}

pub trait SessionStore: Send + Sync {
    fn save(&self, session: Session) -> Result<()>;
    fn get(&self, id: &SessionId) -> Result<Option<Session>>;
    fn delete(&self, id: &SessionId) -> Result<()>;
    fn rotate(&self, old_id: &SessionId) -> Result<Session>;
    fn invalidate_all_for_user(&self, user_id: &str) -> Result<()>;
    fn prune_expired(&self) -> Result<usize>;
}

pub struct MemorySessionStore {
    sessions: Mutex<HashMap<SessionId, Session>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore for MemorySessionStore {
    fn save(&self, session: Session) -> Result<()> {
        let mut guard = self.sessions.lock().unwrap();
        guard.insert(session.id.clone(), session);
        Ok(())
    }

    fn get(&self, id: &SessionId) -> Result<Option<Session>> {
        let mut guard = self.sessions.lock().unwrap();
        if let Some(session) = guard.get_mut(id) {
            if session.is_expired() {
                guard.remove(id);
                return Ok(None);
            }
            session.touch();
            return Ok(Some(session.clone()));
        }
        Ok(None)
    }

    fn delete(&self, id: &SessionId) -> Result<()> {
        let mut guard = self.sessions.lock().unwrap();
        guard.remove(id);
        Ok(())
    }

    fn rotate(&self, old_id: &SessionId) -> Result<Session> {
        let mut guard = self.sessions.lock().unwrap();
        let mut session = match guard.remove(old_id) {
            Some(s) => s,
            None => bail!("session not found"),
        };
        let new_id = SessionId::generate();
        session.id = new_id.clone();
        session.csrf_secret = SessionId::generate().0;
        session.touch();
        guard.insert(new_id, session.clone());
        Ok(session)
    }

    fn invalidate_all_for_user(&self, user_id: &str) -> Result<()> {
        let mut guard = self.sessions.lock().unwrap();
        guard.retain(|_, session| session.user_id != user_id);
        Ok(())
    }

    fn prune_expired(&self) -> Result<usize> {
        let mut guard = self.sessions.lock().unwrap();
        let initial_len = guard.len();
        guard.retain(|_, session| !session.is_expired());
        Ok(initial_len - guard.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_entropy() {
        let s1 = SessionId::generate();
        let s2 = SessionId::generate();
        assert_eq!(s1.0.len(), 64);
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_session_store_lifecycle() {
        let store = MemorySessionStore::new();
        let session = Session::new("u100", Duration::from_secs(3600));
        let id = session.id.clone();

        store.save(session).unwrap();
        assert!(store.get(&id).unwrap().is_some());

        let rotated = store.rotate(&id).unwrap();
        assert_ne!(rotated.id, id);
        assert!(store.get(&id).unwrap().is_none());
        assert!(store.get(&rotated.id).unwrap().is_some());

        store.delete(&rotated.id).unwrap();
        assert!(store.get(&rotated.id).unwrap().is_none());
    }

    #[test]
    fn test_cookie_header_generation() {
        let config = CookieConfig::default();
        let header = config.build_header("test_session_id");
        assert!(header.contains("agilang_session=test_session_id"));
        assert!(header.contains("HttpOnly"));
        assert!(header.contains("SameSite=Lax"));
    }
}
