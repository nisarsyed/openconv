//! Durable frame storage.
//!
//! Every frame the relay accepts is written here before anyone receives it,
//! keyed by its address and the sequence number the relay assigned. Payloads
//! are stored as the opaque bytes they arrived as; nothing here reads them.
//!
//! # Why SQLite
//!
//! The requirements are modest: opaque blobs, a monotonic key per address,
//! range reads for catch-up (#105), and deletes by age and size for retention
//! (#106). An append-only file per address would do the first two and make
//! the last two painful. SQLite does all four, is durable on commit, and is
//! the engine the client core is moving to for its MLS state (#107), so the
//! workspace keeps one storage dependency rather than two.
//!
//! `libsqlite3-sys` declares `links = "sqlite3"`, so the relay and the core
//! must agree on one `rusqlite` version. 0.40 is the one #107 plans for.
//!
//! # The sequence is durable state
//!
//! A restart that reset numbering to zero would be corruption, not
//! inconvenience: a client whose cursor sits at 500 would silently skip
//! everything until the relay counted back up to 500. So the next sequence
//! number lives on disk, advanced in the same transaction that stores the
//! frame.
//!
//! It lives in its own table rather than being derived from `MAX(seq)` over
//! the frames, because retention will delete frames. Deriving it would let an
//! address whose frames had all expired start again at zero.

use openconv_wire::{Address, Delivery};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

/// Anything that can go wrong reading or writing frames.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// SQLite itself failed: I/O, a full disk, a locked or corrupt file.
    #[error("storage: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The database was written by a newer relay, whose schema this one does
    /// not know. Refused rather than guessed at.
    #[error("store has schema version {found}; this relay understands up to {SCHEMA_VERSION}")]
    UnknownSchema {
        /// The version the file declares.
        found: i64,
    },
    /// A stored row cannot be what this relay wrote: a negative sequence or
    /// timestamp, or an address outside the wire format's bounds.
    #[error("stored frame is corrupt")]
    Corrupt,
    /// A value does not fit SQLite's signed 64-bit integers. Sequence numbers
    /// and millisecond timestamps would need centuries to get there.
    #[error("value out of range for storage")]
    OutOfRange,
}

/// Convenience alias for this module's fallible operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Bumped whenever the schema changes. A relay refuses a store with a higher
/// version than it knows, so an old binary cannot misread a newer layout.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    -- The next sequence number for each address that has ever had a frame.
    -- Never deleted, so numbering survives retention emptying an address.
    CREATE TABLE sequences (
        address  BLOB PRIMARY KEY,
        next_seq INTEGER NOT NULL
    ) WITHOUT ROWID;

    -- Payloads can be kilobytes, which is why this is a rowid table: SQLite
    -- recommends WITHOUT ROWID only for rows well under a page.
    CREATE TABLE frames (
        address     BLOB    NOT NULL,
        seq         INTEGER NOT NULL,
        accepted_ms INTEGER NOT NULL,
        payload     BLOB    NOT NULL,
        PRIMARY KEY (address, seq)
    );
";

/// The relay's frame store. One connection, used under the relay's lock: a
/// single writer is all SQLite allows anyway, and that lock is also what
/// keeps numbering and delivery atomic.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open the store at `path`, creating it if it does not exist.
    ///
    /// The file is put in WAL mode with `synchronous = FULL`: a frame whose
    /// transaction has committed survives a crash of the relay or the
    /// machine. That is the guarantee delivery depends on, since subscribers
    /// only receive a frame once it is stored.
    ///
    /// # Errors
    /// If the file cannot be opened or created, or holds a schema this relay
    /// does not understand.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        // `journal_mode` reports the mode it settled on, so it is a query.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        Self::init(conn)
    }

    /// A store that lives only as long as this value. For tests, and for
    /// running a relay that should forget everything when it stops.
    ///
    /// # Errors
    /// If SQLite cannot allocate the database.
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        let tx = conn.transaction()?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        match version {
            0 => {
                tx.execute_batch(SCHEMA)?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            SCHEMA_VERSION => {}
            found => return Err(Error::UnknownSchema { found }),
        }
        tx.commit()?;
        Ok(Self { conn })
    }

    /// Number a frame and store it, returning it as subscribers should
    /// receive it.
    ///
    /// The sequence advances and the frame is written in one transaction, so
    /// a crash leaves both or neither. When this returns, the frame is on
    /// disk.
    ///
    /// # Errors
    /// If the write fails. Nothing is stored and the sequence does not
    /// advance, so the frame must not be delivered either.
    pub fn append(
        &mut self,
        address: Address,
        accepted_ms: u64,
        payload: Vec<u8>,
    ) -> Result<Delivery> {
        let accepted = i64::try_from(accepted_ms).map_err(|_| Error::OutOfRange)?;
        let tx = self.conn.transaction()?;
        let seq: i64 = tx
            .prepare_cached(
                "INSERT INTO sequences (address, next_seq) VALUES (?1, 1)
                 ON CONFLICT (address) DO UPDATE SET next_seq = next_seq + 1
                 RETURNING next_seq - 1",
            )?
            .query_row([address.as_bytes()], |r| r.get(0))?;
        tx.prepare_cached(
            "INSERT INTO frames (address, seq, accepted_ms, payload) VALUES (?1, ?2, ?3, ?4)",
        )?
        .execute(params![address.as_bytes(), seq, accepted, payload])?;
        tx.commit()?;
        Ok(Delivery {
            address,
            seq: u64::try_from(seq).map_err(|_| Error::Corrupt)?,
            accepted_ms,
            payload,
        })
    }

    /// Up to `limit` stored frames for `address`, starting at sequence
    /// `from`, in sequence order.
    ///
    /// What a returning client is sent to catch up (#105). Frames below
    /// `from` that retention has removed are simply absent.
    ///
    /// # Errors
    /// If the read fails or a stored row is corrupt.
    pub fn since(&self, address: &Address, from: u64, limit: usize) -> Result<Vec<Delivery>> {
        let from = i64::try_from(from).map_err(|_| Error::OutOfRange)?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut stmt = self.conn.prepare_cached(
            "SELECT seq, accepted_ms, payload FROM frames
             WHERE address = ?1 AND seq >= ?2
             ORDER BY seq LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![address.as_bytes(), from, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (seq, accepted_ms, payload) = row?;
            Ok(Delivery {
                address: address.clone(),
                seq: u64::try_from(seq).map_err(|_| Error::Corrupt)?,
                accepted_ms: u64::try_from(accepted_ms).map_err(|_| Error::Corrupt)?,
                payload,
            })
        })
        .collect()
    }

    /// The sequence number the next frame to `address` will get.
    ///
    /// # Errors
    /// If the read fails or the stored counter is corrupt.
    pub fn next_seq(&self, address: &Address) -> Result<u64> {
        let next: Option<i64> = self
            .conn
            .prepare_cached("SELECT next_seq FROM sequences WHERE address = ?1")?
            .query_row([address.as_bytes()], |r| r.get(0))
            .optional()?;
        u64::try_from(next.unwrap_or(0)).map_err(|_| Error::Corrupt)
    }

    /// Make every write fail from now on, so tests can check what the relay
    /// does when storage does.
    #[cfg(test)]
    pub(crate) fn fail_writes(&self) {
        self.conn
            .pragma_update(None, "query_only", true)
            .expect("set query_only");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(bytes: &[u8]) -> Address {
        Address::new(bytes.to_vec()).unwrap()
    }

    fn payloads(frames: &[Delivery]) -> Vec<(u64, &[u8])> {
        frames
            .iter()
            .map(|d| (d.seq, d.payload.as_slice()))
            .collect()
    }

    #[test]
    fn each_address_is_numbered_from_zero_independently() {
        let mut store = Store::in_memory().unwrap();
        let (red, blue) = (addr(b"red"), addr(b"blue"));
        assert_eq!(store.append(red.clone(), 1, b"r0".to_vec()).unwrap().seq, 0);
        assert_eq!(store.append(red.clone(), 2, b"r1".to_vec()).unwrap().seq, 1);
        assert_eq!(
            store.append(blue.clone(), 3, b"b0".to_vec()).unwrap().seq,
            0
        );
        assert_eq!(store.next_seq(&red).unwrap(), 2);
        assert_eq!(store.next_seq(&addr(b"never")).unwrap(), 0);
    }

    #[test]
    fn since_reads_in_order_from_a_cursor_and_respects_the_limit() {
        let mut store = Store::in_memory().unwrap();
        let room = addr(b"room");
        for i in 0..5u8 {
            store
                .append(room.clone(), 100 + u64::from(i), vec![i])
                .unwrap();
        }
        store.append(addr(b"other"), 0, b"x".to_vec()).unwrap();

        let all = store.since(&room, 0, 100).unwrap();
        assert_eq!(
            payloads(&all),
            [(0, &[0][..]), (1, &[1]), (2, &[2]), (3, &[3]), (4, &[4])]
        );
        assert_eq!(all[3].accepted_ms, 103);
        assert_eq!(
            payloads(&store.since(&room, 3, 100).unwrap()),
            [(3, &[3][..]), (4, &[4])]
        );
        assert_eq!(
            payloads(&store.since(&room, 1, 2).unwrap()),
            [(1, &[1][..]), (2, &[2])]
        );
        assert!(store.since(&room, 5, 100).unwrap().is_empty());
    }

    /// The failure #104 exists to prevent: a restarted relay handing out
    /// numbers it has already used.
    #[test]
    fn numbering_and_frames_survive_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.db");
        let room = addr(b"room");
        {
            let mut store = Store::open(&path).unwrap();
            store.append(room.clone(), 10, b"before".to_vec()).unwrap();
            store
                .append(room.clone(), 11, b"also before".to_vec())
                .unwrap();
        }
        let mut store = Store::open(&path).unwrap();
        assert_eq!(
            store
                .append(room.clone(), 12, b"after".to_vec())
                .unwrap()
                .seq,
            2
        );
        assert_eq!(
            payloads(&store.since(&room, 0, 10).unwrap()),
            [(0, &b"before"[..]), (1, b"also before"), (2, b"after")]
        );
    }

    /// Retention (#106) will delete frames. An address emptied by it must
    /// keep counting, not start again at zero.
    #[test]
    fn numbering_does_not_restart_when_frames_are_deleted() {
        let mut store = Store::in_memory().unwrap();
        let room = addr(b"room");
        store.append(room.clone(), 0, b"a".to_vec()).unwrap();
        store.append(room.clone(), 0, b"b".to_vec()).unwrap();
        store.conn.execute("DELETE FROM frames", []).unwrap();
        assert!(
            store.since(&room, 0, 10).unwrap().is_empty(),
            "delete did not apply"
        );
        assert_eq!(store.append(room, 0, b"c".to_vec()).unwrap().seq, 2);
    }

    #[test]
    fn a_failed_write_stores_nothing_and_does_not_advance_the_sequence() {
        let mut store = Store::in_memory().unwrap();
        let room = addr(b"room");
        store.append(room.clone(), 0, b"a".to_vec()).unwrap();
        store.fail_writes();
        assert!(store.append(room.clone(), 0, b"b".to_vec()).is_err());
        assert_eq!(store.next_seq(&room).unwrap(), 1);
        assert_eq!(store.since(&room, 0, 10).unwrap().len(), 1);
    }

    #[test]
    fn a_store_from_a_newer_relay_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.db");
        drop(Store::open(&path).unwrap());
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(
            Store::open(&path),
            Err(Error::UnknownSchema { found }) if found == SCHEMA_VERSION + 1
        ));
    }
}
