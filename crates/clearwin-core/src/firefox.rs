//! Firefox keeps history and bookmarks in the same `places.sqlite` database,
//! so history is removed row by row instead of deleting the file.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

pub fn count_visits(db: &Path) -> rusqlite::Result<u64> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.query_row("SELECT COUNT(*) FROM moz_historyvisits", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|n| n as u64)
}

/// Deletes every visit and every page that is not bookmarked. Returns visits removed.
pub fn clear_history(db: &Path) -> rusqlite::Result<u64> {
    let mut conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(Duration::from_secs(2))?;
    let tx = conn.transaction()?;
    let removed = tx.execute("DELETE FROM moz_historyvisits", [])?;
    tx.execute("DELETE FROM moz_inputhistory", [])?;
    // foreign_count counts bookmarks and keywords pointing at a page.
    tx.execute("DELETE FROM moz_places WHERE foreign_count = 0", [])?;
    tx.execute(
        "UPDATE moz_places SET visit_count = 0, last_visit_date = NULL",
        [],
    )?;
    tx.execute(
        "DELETE FROM moz_origins WHERE id NOT IN (SELECT origin_id FROM moz_places)",
        [],
    )?;
    tx.commit()?;
    // Reclaims the space; history is already gone if this fails.
    let _ = conn.execute_batch("VACUUM");
    Ok(removed as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_visits_and_keeps_bookmarks() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("places.sqlite");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE moz_origins (id INTEGER PRIMARY KEY, host TEXT);
             CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, origin_id INTEGER,
                 foreign_count INTEGER DEFAULT 0, visit_count INTEGER, last_visit_date INTEGER);
             CREATE TABLE moz_historyvisits (id INTEGER PRIMARY KEY, place_id INTEGER);
             CREATE TABLE moz_inputhistory (place_id INTEGER, input TEXT);
             INSERT INTO moz_origins VALUES (1, 'kept.example'), (2, 'gone.example');
             INSERT INTO moz_places VALUES (1, 'https://kept.example', 1, 1, 3, 100),
                                           (2, 'https://gone.example', 2, 0, 2, 200);
             INSERT INTO moz_historyvisits (place_id) VALUES (1), (1), (1), (2), (2);
             INSERT INTO moz_inputhistory VALUES (2, 'gon');",
        )
        .unwrap();
        drop(conn);

        assert_eq!(count_visits(&db).unwrap(), 5);
        assert_eq!(clear_history(&db).unwrap(), 5);
        assert_eq!(count_visits(&db).unwrap(), 0);

        let conn = Connection::open(&db).unwrap();
        let urls: Vec<String> = conn
            .prepare("SELECT url FROM moz_places")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(urls, vec!["https://kept.example"]);
        let origins: i64 = conn
            .query_row("SELECT COUNT(*) FROM moz_origins", [], |r| r.get(0))
            .unwrap();
        assert_eq!(origins, 1);
    }
}
