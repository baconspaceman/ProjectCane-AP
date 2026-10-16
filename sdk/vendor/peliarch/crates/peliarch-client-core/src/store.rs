//! Optional durable journal for new generic adapters. Legacy game ledgers are untouched.
use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{DeliveryRecord, Host, HostSnapshot, SessionIdentity};

const VERSION: u32 = 1;

#[derive(Debug)]
pub struct StoreError(String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for StoreError {}
impl From<io::Error> for StoreError {
    fn from(value: io::Error) -> Self {
        Self(format!("durable host store: {value}"))
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope<P> {
    version: u32,
    snapshot: HostSnapshot<P>,
}

/// Owns an OS-managed exclusive lock for the journal's entire lifetime.
/// The sidecar is deliberately never unlinked: removing it would let a second
/// process lock a different inode while the first process still owns its lock.
pub struct DurableHostStore<P> {
    path: PathBuf,
    _lock: File,
    snapshot: HostSnapshot<P>,
    _payload: PhantomData<P>,
}

/// Defers opening a session-bound journal until the runner learns save identity.
pub struct JournalPath<P> {
    path: PathBuf,
    store: Option<DurableHostStore<P>>,
}

impl<P> JournalPath<P> {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            store: None,
        }
    }
}

impl<P: Clone + Eq + Serialize + DeserializeOwned> crate::runner::RunnerPersistence<P>
    for JournalPath<P>
{
    fn load(&mut self, identity: &SessionIdentity) -> Result<Option<HostSnapshot<P>>, String> {
        if self.store.is_none() {
            self.store = Some(
                DurableHostStore::open(&self.path, identity.clone())
                    .map_err(|error| error.to_string())?,
            );
        }
        crate::runner::RunnerPersistence::load(self.store.as_mut().unwrap(), identity)
    }

    fn checkpoint(
        &mut self,
        identity: &SessionIdentity,
        record: &DeliveryRecord<P>,
    ) -> Result<(), String> {
        self.store
            .as_mut()
            .ok_or_else(|| "journal has not been bound to a session".to_string())?
            .checkpoint(identity, record)
            .map_err(|error| error.to_string())
    }
}

impl<P: Clone + Eq + Serialize + DeserializeOwned> crate::runner::RunnerPersistence<P>
    for DurableHostStore<P>
{
    fn load(&mut self, identity: &SessionIdentity) -> Result<Option<HostSnapshot<P>>, String> {
        if identity != &self.snapshot.identity {
            return Err("journal session identity mismatch".into());
        }
        Ok(Some(self.snapshot()))
    }

    fn checkpoint(
        &mut self,
        identity: &SessionIdentity,
        record: &DeliveryRecord<P>,
    ) -> Result<(), String> {
        DurableHostStore::checkpoint(self, identity, record).map_err(|error| error.to_string())
    }
}

impl<P: Clone + Eq + Serialize + DeserializeOwned> DurableHostStore<P> {
    pub fn open(path: impl AsRef<Path>, identity: SessionIdentity) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let parent = journal_parent(&path);
        fs::create_dir_all(parent)?;
        let name = path
            .file_name()
            .ok_or_else(|| StoreError("journal needs a file name".into()))?;
        let path = fs::canonicalize(parent)?.join(name);
        let parent = journal_parent(&path);
        let name = path.file_name().unwrap();
        let mut lock_name = name.to_os_string();
        lock_name.push(".lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(parent.join(lock_name))?;
        lock.try_lock().map_err(|error| {
            StoreError(format!(
                "cannot exclusively lock {}: {error}",
                path.display()
            ))
        })?;
        let mut created = false;
        let snapshot = match fs::read(&path) {
            Ok(bytes) => {
                let envelope: Envelope<P> = json::from_slice(&bytes).map_err(|error| {
                    StoreError(format!("invalid journal {}: {error}", path.display()))
                })?;
                if envelope.version != VERSION {
                    return Err(StoreError(format!(
                        "unsupported host journal version {}",
                        envelope.version
                    )));
                }
                Host::restore(identity.clone(), envelope.snapshot)
                    .map_err(|error| StoreError(format!("invalid host journal: {error}")))?
                    .export()
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                created = true;
                Host::new(identity).export()
            }
            Err(error) => return Err(error.into()),
        };
        let mut store = Self {
            path,
            _lock: lock,
            snapshot,
            _payload: PhantomData,
        };
        if created {
            store.save(&store.snapshot())?;
        }
        Ok(store)
    }

    pub fn snapshot(&self) -> HostSnapshot<P> {
        self.snapshot.clone()
    }

    /// Writes and flushes a complete new generation before atomically replacing
    /// the old one. Failure never advances the in-memory durable watermark.
    pub fn save(&mut self, snapshot: &HostSnapshot<P>) -> Result<(), StoreError> {
        let validated = Host::restore(self.snapshot.identity.clone(), snapshot.clone())
            .map_err(|error| StoreError(format!("invalid host snapshot: {error}")))?
            .export();
        let bytes = json::to_vec(&Envelope {
            version: VERSION,
            snapshot: validated.clone(),
        })
        .map_err(|error| StoreError(format!("cannot serialize host journal: {error}")))?;
        write_file_atomic(&self.path, &bytes)?;
        self.snapshot = validated;
        Ok(())
    }

    /// Suitable for `Host::step`'s write-ahead callback. Other pending records
    /// should first be persisted with `save` after enqueueing them.
    pub fn checkpoint(
        &mut self,
        identity: &SessionIdentity,
        record: &DeliveryRecord<P>,
    ) -> Result<(), StoreError> {
        if identity != &self.snapshot.identity {
            return Err(StoreError("checkpoint session identity mismatch".into()));
        }
        let mut next = self.snapshot.clone();
        if let Some(existing) = next
            .records
            .iter_mut()
            .find(|entry| entry.effect.id == record.effect.id)
        {
            if existing.effect != record.effect {
                return Err(StoreError(format!(
                    "conflicting checkpoint effect {}",
                    record.effect.id.0
                )));
            }
            *existing = record.clone();
        } else {
            next.records.push(record.clone());
        }
        self.save(&next)
    }
}

/// Flush a complete file generation and atomically replace its predecessor.
/// This helper does not acquire a lock. The caller must own exclusive writing
/// rights to both `path` and its adjacent `<filename>.pending` staging file for
/// the entire operation. A successful return includes the platform's durability
/// flush; a failed return must not be treated as a committed game receipt.
/// An abandoned staging file is overwritten by the next write.
pub fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let name = path
        .file_name()
        .ok_or_else(|| StoreError("atomic file needs a file name".into()))?;
    fs::create_dir_all(journal_parent(path))?;
    let path = fs::canonicalize(journal_parent(path))?.join(name);
    let mut staging_name = name.to_os_string();
    staging_name.push(".pending");
    let staging = journal_parent(&path).join(staging_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&staging)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    replace(&staging, &path)?;
    Ok(())
}

fn journal_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)?;
    File::open(journal_parent(destination))?.sync_all()
}

#[cfg(windows)]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Both buffers remain alive and NUL-terminated throughout the call.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeliveryState, Effect, EffectId, EffectKind};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "peliarch-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn journal(&self) -> PathBuf {
            self.0.join("host.json")
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let root = fs::canonicalize(std::env::temp_dir()).unwrap();
            if let Ok(target) = fs::canonicalize(&self.0) {
                assert_eq!(target.parent(), Some(root.as_path()));
                assert!(
                    target
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("peliarch-store-")
                );
                let _ = fs::remove_dir_all(target);
            }
        }
    }
    fn identity() -> SessionIdentity {
        SessionIdentity {
            game: "test".into(),
            seed: "seed".into(),
            team: 1,
            slot: 2,
            save: "save".into(),
        }
    }
    fn record(state: DeliveryState) -> DeliveryRecord<u32> {
        DeliveryRecord {
            effect: Effect {
                id: EffectId::received_item(0),
                kind: EffectKind::Repeatable,
                payload: 7,
            },
            state,
        }
    }

    #[test]
    fn restart_preserves_uncertainty_and_completed_replacement() {
        let directory = Directory::new();
        let mut store = DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap();
        store
            .checkpoint(
                &identity(),
                &record(DeliveryState::Uncertain("started".into())),
            )
            .unwrap();
        drop(store);
        let mut store = DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap();
        assert_eq!(
            store.snapshot().records,
            vec![record(DeliveryState::Uncertain("started".into()))]
        );
        store
            .checkpoint(&identity(), &record(DeliveryState::Completed))
            .unwrap();
        drop(store);
        let store = DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap();
        assert_eq!(
            store.snapshot().records,
            vec![record(DeliveryState::Completed)]
        );
    }

    #[test]
    fn exclusive_lock_is_released_when_owner_drops() {
        let directory = Directory::new();
        let store = DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap();
        assert!(DurableHostStore::<u32>::open(directory.journal(), identity()).is_err());
        drop(store);
        assert!(DurableHostStore::<u32>::open(directory.journal(), identity()).is_ok());
    }

    #[test]
    fn empty_journal_binds_identity_across_restart() {
        let directory = Directory::new();
        drop(DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap());
        let mut wrong = identity();
        wrong.seed = "different seed".into();
        assert!(DurableHostStore::<u32>::open(directory.journal(), wrong).is_err());
        assert!(
            DurableHostStore::<u32>::open(directory.journal(), identity())
                .unwrap()
                .snapshot()
                .records
                .is_empty()
        );
    }

    #[test]
    #[ignore = "subprocess helper invoked by killed_writer_releases_lock"]
    fn child_writer() {
        let journal = std::env::var_os("PELIARCH_STORE_TEST_JOURNAL").unwrap();
        let mut store = DurableHostStore::<u32>::open(PathBuf::from(journal), identity()).unwrap();
        store
            .checkpoint(
                &identity(),
                &record(DeliveryState::Uncertain("child started".into())),
            )
            .unwrap();
        fs::write(
            std::env::var_os("PELIARCH_STORE_TEST_READY").unwrap(),
            b"ready",
        )
        .unwrap();
        loop {
            std::thread::park();
        }
    }

    #[test]
    fn killed_writer_releases_lock() {
        let directory = Directory::new();
        let ready = directory.0.join("ready");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "store::tests::child_writer", "--ignored"])
            .env("PELIARCH_STORE_TEST_JOURNAL", directory.journal())
            .env("PELIARCH_STORE_TEST_READY", &ready)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !ready.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let locked = DurableHostStore::<u32>::open(directory.journal(), identity()).is_err();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(ready.exists(), "child did not initialize durable journal");
        assert!(locked);
        let store = DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap();
        assert_eq!(
            store.snapshot().records,
            vec![record(DeliveryState::Uncertain("child started".into()))]
        );
    }

    #[test]
    fn rejects_wrong_session_corruption_and_future_version() {
        let directory = Directory::new();
        let mut store = DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap();
        store.save(&store.snapshot()).unwrap();
        let original = fs::read(directory.journal()).unwrap();
        let mut wrong = identity();
        wrong.save = "other".into();
        assert!(
            store
                .checkpoint(&wrong, &record(DeliveryState::Completed))
                .is_err()
        );
        drop(store);
        assert!(DurableHostStore::<u32>::open(directory.journal(), wrong).is_err());
        assert_eq!(fs::read(directory.journal()).unwrap(), original);
        fs::write(directory.journal(), b"{truncated").unwrap();
        assert!(DurableHostStore::<u32>::open(directory.journal(), identity()).is_err());
        let future = Envelope::<u32> {
            version: VERSION + 1,
            snapshot: Host::new(identity()).export(),
        };
        fs::write(directory.journal(), json::to_vec(&future).unwrap()).unwrap();
        assert!(DurableHostStore::<u32>::open(directory.journal(), identity()).is_err());
    }

    #[test]
    fn failed_staging_write_preserves_previous_generation() {
        let directory = Directory::new();
        let mut store = DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap();
        store
            .checkpoint(&identity(), &record(DeliveryState::Pending))
            .unwrap();
        let before = fs::read(directory.journal()).unwrap();
        fs::create_dir(directory.0.join("host.json.pending")).unwrap();
        assert!(
            store
                .checkpoint(&identity(), &record(DeliveryState::Completed))
                .is_err()
        );
        assert_eq!(
            store.snapshot().records,
            vec![record(DeliveryState::Pending)]
        );
        assert_eq!(fs::read(directory.journal()).unwrap(), before);
        drop(store);
        let store = DurableHostStore::<u32>::open(directory.journal(), identity()).unwrap();
        assert_eq!(
            store.snapshot().records,
            vec![record(DeliveryState::Pending)]
        );
    }
}
