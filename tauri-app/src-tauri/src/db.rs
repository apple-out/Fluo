use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{
    AddressSuggestion, Bookmark, DownloadItem, GlossaryEntry, HistoryItem, SavedLogin,
    SearchHistoryItem, SegmentResult, TranslationHistory, UserScriptItem,
};

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub fn open(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA temp_store = MEMORY;
            PRAGMA cache_size = -8000;
            PRAGMA mmap_size = 67108864;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS translations (
                cache_key TEXT PRIMARY KEY,
                source_hash TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                style TEXT NOT NULL,
                target_language TEXT NOT NULL,
                translation TEXT NOT NULL,
                uncertain INTEGER NOT NULL DEFAULT 0,
                notes TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS snapshots (
                page_id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                article_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
             CREATE TABLE IF NOT EXISTS glossary (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                source TEXT NOT NULL,
                reading TEXT NOT NULL DEFAULT '',
                 translation TEXT NOT NULL DEFAULT '',
                 alternatives TEXT NOT NULL DEFAULT '',
                 aliases TEXT NOT NULL DEFAULT '',
                do_not_translate INTEGER NOT NULL DEFAULT 0,
                case_sensitive INTEGER NOT NULL DEFAULT 0,
                exact_match INTEGER NOT NULL DEFAULT 0,
                use_regex INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                note TEXT NOT NULL DEFAULT '',
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                folder TEXT NOT NULL DEFAULT '',
                favorite INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS translation_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                source TEXT NOT NULL,
                translation TEXT NOT NULL,
                model TEXT NOT NULL,
                translated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                visited_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS history_visited_at ON history(visited_at DESC);
            CREATE INDEX IF NOT EXISTS history_url ON history(url);
            CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS downloads_started_at ON downloads(started_at DESC);
            CREATE TABLE IF NOT EXISTS page_translations (
                url TEXT NOT NULL,
                source_hash TEXT NOT NULL,
                target_language TEXT NOT NULL,
                style TEXT NOT NULL,
                translation TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (url, source_hash, target_language, style)
            );
            CREATE INDEX IF NOT EXISTS page_translations_url ON page_translations(url);
            CREATE TABLE IF NOT EXISTS bookmark_folders (
                name TEXT PRIMARY KEY
            );
            CREATE TABLE IF NOT EXISTS search_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                query TEXT NOT NULL,
                engine TEXT NOT NULL,
                searched_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS search_history_at ON search_history(searched_at DESC);
            CREATE TABLE IF NOT EXISTS saved_logins (
                id TEXT PRIMARY KEY,
                origin TEXT NOT NULL,
                username TEXT NOT NULL,
                password_enc BLOB NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(origin, username)
            );
            CREATE INDEX IF NOT EXISTS saved_logins_origin ON saved_logins(origin);
            CREATE TABLE IF NOT EXISTS user_scripts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                match_pattern TEXT NOT NULL,
                code TEXT NOT NULL DEFAULT '',
                css TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                updated_at INTEGER NOT NULL
            );
            "#,
        )
        .map_err(|error| error.to_string())?;
    ensure_glossary_alias_column(&connection)?;
    Ok(connection)
}

fn ensure_glossary_alias_column(connection: &Connection) -> Result<(), String> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('glossary') WHERE name = 'aliases')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !exists {
        connection
            .execute(
                "ALTER TABLE glossary ADD COLUMN aliases TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn get_setting(connection: &Connection, key: &str) -> Option<String> {
    connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
}

pub fn save_session(connection: &Connection, json: &str) -> Result<(), String> {
    set_setting(connection, "tab_session", json)
}

pub fn load_session(connection: &Connection) -> Option<String> {
    get_setting(connection, "tab_session")
}

pub fn set_setting(connection: &Connection, key: &str, value: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CachedTranslation {
    pub translation: String,
    pub uncertain: bool,
    pub notes: Vec<String>,
}

pub fn get_translation(
    connection: &Connection,
    cache_key: &str,
) -> Result<Option<CachedTranslation>, String> {
    connection
        .query_row(
            "SELECT translation, uncertain, notes FROM translations WHERE cache_key = ?1",
            params![cache_key],
            |row| {
                let translation: String = row.get(0)?;
                let uncertain: i64 = row.get(1)?;
                let notes: String = row.get(2)?;
                Ok(CachedTranslation {
                    translation,
                    uncertain: uncertain != 0,
                    notes: serde_json::from_str(&notes).unwrap_or_default(),
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub fn put_translation(
    connection: &Connection,
    cache_key: &str,
    source_hash: &str,
    provider: &str,
    model: &str,
    style: &str,
    target_language: &str,
    result: &SegmentResult,
) -> Result<(), String> {
    let notes = serde_json::to_string(&result.notes).unwrap_or_else(|_| "[]".into());
    connection
        .execute(
            "INSERT INTO translations
                (cache_key, source_hash, provider, model, style, target_language, translation, uncertain, notes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(cache_key) DO UPDATE SET
                translation = excluded.translation,
                uncertain = excluded.uncertain,
                notes = excluded.notes,
                created_at = excluded.created_at",
            params![
                cache_key,
                source_hash,
                provider,
                model,
                style,
                target_language,
                result.translation,
                if result.uncertain { 1 } else { 0 },
                notes,
                now_secs()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn save_snapshot(
    connection: &Connection,
    page_id: &str,
    url: &str,
    title: &str,
    article_json: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO snapshots (page_id, url, title, article_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(page_id) DO UPDATE SET url = excluded.url, title = excluded.title,
                article_json = excluded.article_json, updated_at = excluded.updated_at",
            params![page_id, url, title, article_json, now_secs()],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_snapshot(connection: &Connection, page_id: &str) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT article_json FROM snapshots WHERE page_id = ?1",
            params![page_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub fn list_glossary(connection: &Connection) -> Result<Vec<GlossaryEntry>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, project_id, source, reading, translation, alternatives, aliases,
                    do_not_translate, case_sensitive, exact_match, use_regex, enabled, note
             FROM glossary ORDER BY source COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(GlossaryEntry {
                id: row.get(0)?,
                project_id: row.get(1)?,
                source: row.get(2)?,
                reading: row.get(3)?,
                translation: row.get(4)?,
                alternatives: row.get(5)?,
                aliases: row.get(6)?,
                do_not_translate: row.get::<_, i64>(7)? != 0,
                case_sensitive: row.get::<_, i64>(8)? != 0,
                exact_match: row.get::<_, i64>(9)? != 0,
                use_regex: row.get::<_, i64>(10)? != 0,
                enabled: row.get::<_, i64>(11)? != 0,
                note: row.get(12)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn upsert_glossary(connection: &Connection, entry: &GlossaryEntry) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO glossary
                (id, project_id, source, reading, translation, alternatives, aliases, do_not_translate,
                 case_sensitive, exact_match, use_regex, enabled, note, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                source = excluded.source,
                 reading = excluded.reading,
                 translation = excluded.translation,
                 alternatives = excluded.alternatives,
                 aliases = excluded.aliases,
                do_not_translate = excluded.do_not_translate,
                case_sensitive = excluded.case_sensitive,
                exact_match = excluded.exact_match,
                use_regex = excluded.use_regex,
                enabled = excluded.enabled,
                note = excluded.note,
                updated_at = excluded.updated_at",
            params![
                entry.id,
                entry.project_id,
                entry.source,
                entry.reading,
                entry.translation,
                entry.alternatives,
                entry.aliases,
                entry.do_not_translate as i64,
                entry.case_sensitive as i64,
                entry.exact_match as i64,
                entry.use_regex as i64,
                entry.enabled as i64,
                entry.note,
                now_secs()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_glossary(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM glossary WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_bookmarks(connection: &Connection) -> Result<Vec<Bookmark>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, url, title, folder, favorite, created_at, updated_at FROM bookmarks
         ORDER BY favorite DESC, folder COLLATE NOCASE, title COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(Bookmark {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                folder: row.get(3)?,
                favorite: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn save_bookmark(connection: &Connection, bookmark: &Bookmark) -> Result<(), String> {
    let updated_at = now_secs();
    let existing_id = connection
        .query_row(
            "SELECT id FROM bookmarks WHERE id = ?1",
            params![bookmark.id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if existing_id.is_some() {
        let url_owner = connection
            .query_row(
                "SELECT id FROM bookmarks WHERE url = ?1 AND id <> ?2",
                params![bookmark.url, bookmark.id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if url_owner.is_some() {
            return Err("이 URL은 이미 다른 북마크에 저장되어 있습니다.".into());
        }

        connection
            .execute(
                "UPDATE bookmarks SET url = ?1, title = ?2, folder = ?3, favorite = ?4,
                    updated_at = ?5 WHERE id = ?6",
                params![
                    bookmark.url,
                    bookmark.title,
                    bookmark.folder,
                    bookmark.favorite as i64,
                    updated_at,
                    bookmark.id
                ],
            )
            .map_err(|error| error.to_string())?;
    } else {
        connection
            .execute(
                "INSERT INTO bookmarks (id,url,title,folder,favorite,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(url) DO UPDATE SET title=excluded.title,
                    folder=excluded.folder,favorite=excluded.favorite,updated_at=excluded.updated_at",
                params![
                    bookmark.id,
                    bookmark.url,
                    bookmark.title,
                    bookmark.folder,
                    bookmark.favorite as i64,
                    bookmark.created_at,
                    updated_at
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn delete_bookmark(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM bookmarks WHERE id=?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn rename_bookmark_folder(
    connection: &Connection,
    old_name: &str,
    new_name: &str,
) -> Result<usize, String> {
    let old_name = old_name.trim();
    let new_name = new_name.trim();
    if old_name.is_empty() {
        return Err("이름을 변경할 폴더를 선택하세요.".into());
    }
    if new_name.is_empty() {
        return Err("새 폴더 이름을 입력하세요.".into());
    }
    if new_name.chars().count() > 100 {
        return Err("폴더 이름은 100자 이내로 입력하세요.".into());
    }
    if old_name == new_name {
        return Ok(0);
    }
    let updated_at = now_secs();
    connection
        .execute(
            "INSERT OR IGNORE INTO bookmark_folders (name) VALUES (?1)",
            params![new_name],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM bookmark_folders WHERE name = ?1",
            params![old_name],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE bookmarks SET folder = ?1, updated_at = ?2 WHERE folder = ?3",
            params![new_name, updated_at, old_name],
        )
        .map_err(|error| error.to_string())
}

pub fn list_bookmark_folders(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM bookmark_folders
             UNION
             SELECT DISTINCT folder AS name FROM bookmarks WHERE folder <> ''
             ORDER BY name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn create_bookmark_folder(connection: &Connection, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("폴더 이름을 입력하세요.".into());
    }
    if name.chars().count() > 100 {
        return Err("폴더 이름은 100자 이내로 입력하세요.".into());
    }
    connection
        .execute(
            "INSERT OR IGNORE INTO bookmark_folders (name) VALUES (?1)",
            params![name],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_bookmark_folder(connection: &Connection, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(());
    }
    let updated_at = now_secs();
    connection
        .execute(
            "DELETE FROM bookmark_folders WHERE name = ?1",
            params![name],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE bookmarks SET folder = '', updated_at = ?1 WHERE folder = ?2",
            params![updated_at, name],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn add_translation_history(
    connection: &Connection,
    url: &str,
    title: &str,
    source: &str,
    translation: &str,
    model: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO translation_history (url,title,source,translation,model,translated_at)
        VALUES (?1,?2,?3,?4,?5,?6)",
            params![url, title, source, translation, model, now_secs()],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM translation_history WHERE id NOT IN
        (SELECT id FROM translation_history ORDER BY translated_at DESC,id DESC LIMIT 5000)",
            [],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn escape_like(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub fn list_translation_history(
    connection: &Connection,
    query: &str,
) -> Result<Vec<TranslationHistory>, String> {
    let escaped_query = escape_like(query);
    let mut statement = connection.prepare("SELECT id,url,title,source,translation,model,translated_at
        FROM translation_history WHERE (?1='' OR source LIKE '%'||?1||'%' ESCAPE '\\' OR translation LIKE '%'||?1||'%' ESCAPE '\\' OR title LIKE '%'||?1||'%' ESCAPE '\\')
        ORDER BY translated_at DESC,id DESC LIMIT 500").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![escaped_query], |row| {
            Ok(TranslationHistory {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                source: row.get(3)?,
                translation: row.get(4)?,
                model: row.get(5)?,
                translated_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

/// Records a visit and keeps the table bounded. Titles update as pages load.
pub fn record_history(connection: &Connection, url: &str, title: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Ok(());
    }
    connection
        .execute(
            "INSERT INTO history (url,title,visited_at) VALUES (?1,?2,?3)",
            params![url, title, now_secs()],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM history WHERE id NOT IN
        (SELECT id FROM history ORDER BY visited_at DESC,id DESC LIMIT 10000)",
            [],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_history(
    connection: &Connection,
    query: &str,
    limit: u32,
) -> Result<Vec<HistoryItem>, String> {
    let escaped_query = escape_like(query);
    let limit = limit.clamp(1, 500);
    let mut statement = connection
        .prepare(
            "SELECT id,url,title,visited_at FROM history
         WHERE (?1='' OR url LIKE '%'||?1||'%' ESCAPE '\\' OR title LIKE '%'||?1||'%' ESCAPE '\\')
         ORDER BY visited_at DESC,id DESC LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![escaped_query, limit], |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                visited_at: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn update_history_title(connection: &Connection, url: &str, title: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Ok(());
    }
    connection
        .execute(
            "UPDATE history SET title=?2 WHERE id=
        (SELECT id FROM history WHERE url=?1 ORDER BY visited_at DESC,id DESC LIMIT 1)",
            params![url, title],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_history(connection: &Connection, id: i64) -> Result<(), String> {
    connection
        .execute("DELETE FROM history WHERE id=?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn clear_history(connection: &Connection) -> Result<(), String> {
    connection
        .execute("DELETE FROM history", [])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Imported entries keep their original timestamps so merged history stays ordered.
pub fn insert_history_item(connection: &Connection, item: &HistoryItem) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO history (url,title,visited_at) VALUES (?1,?2,?3)",
            params![item.url, item.title, item.visited_at],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn record_download_started(
    connection: &Connection,
    url: &str,
    file_name: &str,
    file_path: &str,
) -> Result<i64, String> {
    let now = now_secs();
    connection
        .execute(
            "INSERT INTO downloads (url, file_name, file_path, status, started_at, finished_at)
             VALUES (?1, ?2, ?3, 'in_progress', ?4, NULL)",
            params![url, file_name, file_path, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(connection.last_insert_rowid())
}

pub fn finish_download(
    connection: &Connection,
    url: &str,
    file_name: &str,
    file_path: &str,
    success: bool,
) -> Result<(), String> {
    let now = now_secs();
    let status = if success { "completed" } else { "failed" };
    let target_id = connection
        .query_row(
            "SELECT id FROM downloads
             WHERE status = 'in_progress' AND (url = ?1 OR (?2 <> '' AND file_path = ?2))
             ORDER BY started_at DESC, id DESC LIMIT 1",
            params![url, file_path],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(id) = target_id {
        connection
            .execute(
                "UPDATE downloads
                 SET status = ?1,
                     finished_at = ?2,
                     file_path = CASE WHEN ?3 <> '' THEN ?3 ELSE file_path END,
                     file_name = CASE WHEN ?4 <> '' THEN ?4 ELSE file_name END
                 WHERE id = ?5",
                params![status, now, file_path, file_name, id],
            )
            .map_err(|error| error.to_string())?;
    } else {
        connection
            .execute(
                "INSERT INTO downloads (url, file_name, file_path, status, started_at, finished_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![url, file_name, file_path, status, now],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn list_downloads(
    connection: &Connection,
    query: &str,
    limit: u32,
) -> Result<Vec<DownloadItem>, String> {
    let escaped_query = escape_like(query);
    let limit = limit.clamp(1, 500);
    let mut statement = connection
        .prepare(
            "SELECT id, url, file_name, file_path, status, started_at, finished_at
             FROM downloads
             WHERE (?1 = '' OR file_name LIKE '%'||?1||'%' ESCAPE '\\' OR url LIKE '%'||?1||'%' ESCAPE '\\')
             ORDER BY started_at DESC, id DESC
             LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![escaped_query, limit], |row| {
            Ok(DownloadItem {
                id: row.get(0)?,
                url: row.get(1)?,
                file_name: row.get(2)?,
                file_path: row.get(3)?,
                status: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn delete_download(connection: &Connection, id: i64) -> Result<(), String> {
    connection
        .execute("DELETE FROM downloads WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn clear_downloads(connection: &Connection) -> Result<(), String> {
    connection
        .execute("DELETE FROM downloads WHERE status <> 'in_progress'", [])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn save_page_translations(
    connection: &Connection,
    url: &str,
    target_language: &str,
    style: &str,
    entries: &[(String, String)],
) -> Result<(), String> {
    if url.trim().is_empty() || entries.is_empty() {
        return Ok(());
    }
    let now = now_secs();
    let mut statement = connection
        .prepare(
            "INSERT INTO page_translations (url, source_hash, target_language, style, translation, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(url, source_hash, target_language, style) DO UPDATE SET
                translation = excluded.translation,
                updated_at = excluded.updated_at",
        )
        .map_err(|error| error.to_string())?;
    for (source_hash, translation) in entries {
        statement
            .execute(params![
                url,
                source_hash,
                target_language,
                style,
                translation,
                now
            ])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn has_page_translations(
    connection: &Connection,
    url: &str,
    target_language: &str,
    style: &str,
) -> Result<bool, String> {
    if url.trim().is_empty() {
        return Ok(false);
    }
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM page_translations
                WHERE url = ?1 AND target_language = ?2 AND style = ?3
             )",
            params![url, target_language, style],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

pub fn load_page_translations(
    connection: &Connection,
    url: &str,
    target_language: &str,
    style: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut map = std::collections::HashMap::new();
    if url.trim().is_empty() {
        return Ok(map);
    }
    let mut statement = connection
        .prepare(
            "SELECT source_hash, translation FROM page_translations
             WHERE url = ?1 AND target_language = ?2 AND style = ?3",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![url, target_language, style], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (hash, translation) = row.map_err(|error| error.to_string())?;
        map.insert(hash, translation);
    }
    Ok(map)
}

/// Bookmarks first, then history, deduplicated by URL for the address bar.
pub fn search_address_suggestions(
    connection: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<AddressSuggestion>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let escaped_query = escape_like(query);
    let limit = limit.clamp(1, 20);
    let mut items: Vec<AddressSuggestion> = Vec::new();

    let mut bookmarks = connection
        .prepare(
            "SELECT 'bookmark',title,url FROM bookmarks
         WHERE title LIKE '%'||?1||'%' ESCAPE '\\' OR url LIKE '%'||?1||'%' ESCAPE '\\'
         ORDER BY favorite DESC,updated_at DESC LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let bookmark_rows = bookmarks
        .query_map(params![escaped_query, limit as u32], |row| {
            Ok(AddressSuggestion {
                kind: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in bookmark_rows {
        items.push(row.map_err(|error| error.to_string())?);
    }

    let mut history = connection
        .prepare(
            "SELECT 'history',title,url FROM history
         WHERE title LIKE '%'||?1||'%' ESCAPE '\\' OR url LIKE '%'||?1||'%' ESCAPE '\\'
         ORDER BY visited_at DESC,id DESC LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let history_rows = history
        .query_map(params![escaped_query, (limit * 2) as u32], |row| {
            Ok(AddressSuggestion {
                kind: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in history_rows {
        let row = row.map_err(|error| error.to_string())?;
        if !items.iter().any(|existing| existing.url == row.url) {
            items.push(row);
        }
        if items.len() >= limit {
            break;
        }
    }

    if items.len() < limit {
        let mut searches = connection
            .prepare(
                "SELECT 'search', query, engine FROM search_history
             WHERE query LIKE '%'||?1||'%' ESCAPE '\\'
             GROUP BY query
             ORDER BY MAX(searched_at) DESC LIMIT ?2",
            )
            .map_err(|error| error.to_string())?;
        let search_rows = searches
            .query_map(params![escaped_query, limit as u32], |row| {
                let query_str: String = row.get(1)?;
                Ok(AddressSuggestion {
                    kind: row.get(0)?,
                    title: format!("검색 기록: {query_str}"),
                    url: query_str,
                })
            })
            .map_err(|error| error.to_string())?;
        for row in search_rows {
            let row = row.map_err(|error| error.to_string())?;
            if !items.iter().any(|existing| existing.url == row.url) {
                items.push(row);
            }
            if items.len() >= limit {
                break;
            }
        }
    }

    items.truncate(limit);
    Ok(items)
}

pub fn record_search_history(
    connection: &Connection,
    query: &str,
    engine: &str,
) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(());
    }
    connection
        .execute(
            "INSERT INTO search_history (query, engine, searched_at) VALUES (?1, ?2, ?3)",
            params![query, engine, now_secs()],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM search_history WHERE id NOT IN
            (SELECT id FROM search_history ORDER BY searched_at DESC, id DESC LIMIT 5000)",
            [],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_search_history(
    connection: &Connection,
    query: &str,
    limit: u32,
) -> Result<Vec<SearchHistoryItem>, String> {
    let escaped_query = escape_like(query);
    let limit = limit.clamp(1, 500);
    let mut statement = connection
        .prepare(
            "SELECT id, query, engine, searched_at FROM search_history
             WHERE (?1 = '' OR query LIKE '%'||?1||'%' ESCAPE '\\')
             ORDER BY searched_at DESC, id DESC LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![escaped_query, limit], |row| {
            Ok(SearchHistoryItem {
                id: row.get(0)?,
                query: row.get(1)?,
                engine: row.get(2)?,
                searched_at: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn delete_search_history(connection: &Connection, id: i64) -> Result<(), String> {
    connection
        .execute("DELETE FROM search_history WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn clear_search_history(connection: &Connection) -> Result<(), String> {
    connection
        .execute("DELETE FROM search_history", [])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn save_login(
    connection: &Connection,
    id: &str,
    origin: &str,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let origin = origin.trim();
    let username = username.trim();
    if origin.is_empty() || username.is_empty() || password.is_empty() {
        return Err("사이트 주소, 아이디, 비밀번호를 모두 입력하세요.".into());
    }
    let encrypted = crate::credentials::encrypt_bytes(password.as_bytes())?;
    let now = now_secs();
    connection
        .execute(
            "INSERT INTO saved_logins (id, origin, username, password_enc, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(origin, username) DO UPDATE SET
                password_enc = excluded.password_enc,
                updated_at = excluded.updated_at",
            params![id, origin, username, encrypted, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_saved_logins(
    connection: &Connection,
    origin_filter: Option<&str>,
) -> Result<Vec<SavedLogin>, String> {
    let filter = origin_filter.unwrap_or("").trim();
    let mut statement = connection
        .prepare(
            "SELECT id, origin, username, password_enc, updated_at FROM saved_logins
             WHERE (?1 = '' OR origin = ?1)
             ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![filter], |row| {
            let blob: Vec<u8> = row.get(3)?;
            let password = crate::credentials::decrypt_bytes(&blob)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default();
            Ok(SavedLogin {
                id: row.get(0)?,
                origin: row.get(1)?,
                username: row.get(2)?,
                password,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn delete_login(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM saved_logins WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_user_scripts(connection: &Connection) -> Result<Vec<UserScriptItem>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, match_pattern, code, css, enabled, updated_at
             FROM user_scripts ORDER BY name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(UserScriptItem {
                id: row.get(0)?,
                name: row.get(1)?,
                match_pattern: row.get(2)?,
                code: row.get(3)?,
                css: row.get(4)?,
                enabled: row.get::<_, i64>(5)? != 0,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn save_user_script(connection: &Connection, item: &UserScriptItem) -> Result<(), String> {
    if item.name.trim().is_empty() {
        return Err("스크립트 이름을 입력하세요.".into());
    }
    connection
        .execute(
            "INSERT INTO user_scripts (id, name, match_pattern, code, css, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                match_pattern = excluded.match_pattern,
                code = excluded.code,
                css = excluded.css,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at",
            params![
                item.id,
                item.name.trim(),
                if item.match_pattern.trim().is_empty() {
                    "*"
                } else {
                    item.match_pattern.trim()
                },
                item.code,
                item.css,
                item.enabled as i64,
                now_secs()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_user_script(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM user_scripts WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmarks_are_upserted_by_url_and_removable() {
        let db = open(Path::new(":memory:")).unwrap();
        let bookmark = Bookmark {
            id: "one".into(),
            url: "https://example.org/".into(),
            title: "Example".into(),
            folder: "Docs".into(),
            favorite: true,
            created_at: 1,
            updated_at: 1,
        };
        save_bookmark(&db, &bookmark).unwrap();
        let mut edited = bookmark.clone();
        edited.title = "Updated".into();
        edited.folder = "Reference".into();
        save_bookmark(&db, &edited).unwrap();
        let rows = list_bookmarks(&db).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Updated");
        assert_eq!(rows[0].folder, "Reference");
        delete_bookmark(&db, "one").unwrap();
        assert!(list_bookmarks(&db).unwrap().is_empty());
    }

    #[test]
    fn editing_bookmark_url_updates_the_existing_row() {
        let db = open(Path::new(":memory:")).unwrap();
        let original = Bookmark {
            id: "one".into(),
            url: "https://example.org/old".into(),
            title: "Old title".into(),
            folder: "Docs".into(),
            favorite: false,
            created_at: 10,
            updated_at: 10,
        };
        save_bookmark(&db, &original).unwrap();

        let edited = Bookmark {
            url: "https://example.org/new".into(),
            title: "New title".into(),
            favorite: true,
            ..original.clone()
        };
        save_bookmark(&db, &edited).unwrap();

        let rows = list_bookmarks(&db).unwrap();
        assert_eq!(rows.len(), 1, "editing must not leave the old URL behind");
        assert_eq!(rows[0].id, "one");
        assert_eq!(rows[0].url, "https://example.org/new");
        assert_eq!(rows[0].title, "New title");
        assert!(rows[0].favorite);
        assert_eq!(rows[0].created_at, 10, "editing preserves creation time");
    }

    #[test]
    fn editing_bookmark_to_another_bookmarks_url_keeps_both_rows_unchanged() {
        let db = open(Path::new(":memory:")).unwrap();
        let original = Bookmark {
            id: "one".into(),
            url: "https://example.org/one".into(),
            title: "One".into(),
            folder: "".into(),
            favorite: false,
            created_at: 10,
            updated_at: 10,
        };
        let other = Bookmark {
            id: "two".into(),
            url: "https://example.org/two".into(),
            title: "Two".into(),
            created_at: 20,
            updated_at: 20,
            ..original.clone()
        };
        save_bookmark(&db, &original).unwrap();
        save_bookmark(&db, &other).unwrap();

        let conflicting_edit = Bookmark {
            url: other.url.clone(),
            title: "Changed".into(),
            ..original.clone()
        };
        assert!(save_bookmark(&db, &conflicting_edit).is_err());

        let rows = list_bookmarks(&db).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .any(|row| row.id == "one" && row.url == original.url)
        );
        assert!(rows.iter().any(|row| row.id == "two" && row.title == "Two"));
    }

    #[test]
    fn translation_history_is_global_and_searchable() {
        let db = open(Path::new(":memory:")).unwrap();
        add_translation_history(
            &db,
            "https://example.org",
            "Example",
            "source term",
            "번역",
            "model",
        )
        .unwrap();
        assert_eq!(list_translation_history(&db, "source").unwrap().len(), 1);
        assert!(list_translation_history(&db, "absent").unwrap().is_empty());
    }

    #[test]
    fn history_search_treats_like_wildcards_as_literal_characters() {
        let db = open(Path::new(":memory:")).unwrap();
        add_translation_history(&db, "https://one.test", "One", "100%", "백 퍼센트", "m").unwrap();
        add_translation_history(&db, "https://two.test", "Two", "100 units", "단위", "m").unwrap();
        assert_eq!(list_translation_history(&db, "%").unwrap().len(), 1);
        assert_eq!(list_translation_history(&db, "_").unwrap().len(), 0);
    }

    #[test]
    fn glossary_aliases_survive_database_round_trip() {
        let db = open(Path::new(":memory:")).unwrap();
        let entry = GlossaryEntry {
            id: "term-1".into(),
            source: "魔法".into(),
            reading: "まほう".into(),
            translation: "마법".into(),
            alternatives: "마술".into(),
            aliases: "まほう, 魔術".into(),
            do_not_translate: false,
            case_sensitive: false,
            exact_match: false,
            use_regex: false,
            enabled: true,
            project_id: None,
            note: String::new(),
        };
        upsert_glossary(&db, &entry).unwrap();
        let rows = list_glossary(&db).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].aliases, entry.aliases);
        assert_eq!(rows[0].alternatives, entry.alternatives);
    }

    #[test]
    fn glossary_schema_migrates_legacy_tables_without_aliases() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE glossary (id TEXT PRIMARY KEY, source TEXT NOT NULL);")
            .unwrap();
        ensure_glossary_alias_column(&db).unwrap();
        let aliases: String = db
            .query_row(
                "SELECT aliases FROM glossary WHERE id = 'absent'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap()
            .unwrap_or_default();
        assert!(aliases.is_empty());
    }

    #[test]
    fn history_records_searches_deletes_and_clears() {
        let db = open(Path::new(":memory:")).unwrap();
        record_history(&db, "https://example.org/docs", "Docs").unwrap();
        record_history(&db, "https://example.org/news", "News").unwrap();
        record_history(&db, "", "Empty").unwrap();

        let rows = list_history(&db, "", 50).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(list_history(&db, "docs", 50).unwrap().len(), 1);
        assert!(list_history(&db, "absent", 50).unwrap().is_empty());
        // Wildcards are matched literally, not as SQL patterns.
        assert!(list_history(&db, "%", 50).unwrap().is_empty());

        delete_history(&db, rows[0].id).unwrap();
        assert_eq!(list_history(&db, "", 50).unwrap().len(), 1);
        clear_history(&db).unwrap();
        assert!(list_history(&db, "", 50).unwrap().is_empty());
    }

    #[test]
    fn address_suggestions_prefer_bookmarks_and_dedupe_urls() {
        let db = open(Path::new(":memory:")).unwrap();
        let bookmark = Bookmark {
            id: "one".into(),
            url: "https://rust-lang.org/".into(),
            title: "Rust".into(),
            folder: String::new(),
            favorite: true,
            created_at: 1,
            updated_at: 1,
        };
        save_bookmark(&db, &bookmark).unwrap();
        record_history(&db, "https://rust-lang.org/", "Rust language").unwrap();
        record_history(&db, "https://doc.rust-lang.org/", "Rust docs").unwrap();

        let items = search_address_suggestions(&db, "rust", 10).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "bookmark");
        assert_eq!(items[0].url, "https://rust-lang.org/");

        let limited = search_address_suggestions(&db, "rust", 1).unwrap();
        assert_eq!(limited.len(), 1);
        assert!(
            search_address_suggestions(&db, "  ", 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn bookmark_folder_rename_updates_matching_rows() {
        let db = open(Path::new(":memory:")).unwrap();
        let make = |id: &str, url: &str, folder: &str| Bookmark {
            id: id.into(),
            url: url.into(),
            title: id.into(),
            folder: folder.into(),
            favorite: false,
            created_at: 1,
            updated_at: 1,
        };
        save_bookmark(&db, &make("a", "https://a.test", "Wiki")).unwrap();
        save_bookmark(&db, &make("b", "https://b.test", "Wiki")).unwrap();
        save_bookmark(&db, &make("c", "https://c.test", "Other")).unwrap();

        let count = rename_bookmark_folder(&db, "Wiki", "게임 위키").unwrap();
        assert_eq!(count, 2);

        let all = list_bookmarks(&db).unwrap();
        assert_eq!(all.iter().filter(|b| b.folder == "게임 위키").count(), 2);
        assert_eq!(all.iter().filter(|b| b.folder == "Other").count(), 1);
    }

    #[test]
    fn downloads_crud_and_completion_work() {
        let db = open(Path::new(":memory:")).unwrap();
        record_download_started(
            &db,
            "https://example.org/file.zip",
            "file.zip",
            "C:\\Downloads\\file.zip",
        )
        .unwrap();
        let items = list_downloads(&db, "", 50).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, "in_progress");

        finish_download(
            &db,
            "https://example.org/file.zip",
            "file.zip",
            "C:\\Downloads\\file.zip",
            true,
        )
        .unwrap();
        let items = list_downloads(&db, "file", 50).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, "completed");
        assert!(items[0].finished_at.is_some());

        delete_download(&db, items[0].id).unwrap();
        assert!(list_downloads(&db, "", 50).unwrap().is_empty());
    }

    #[test]
    fn page_translations_save_check_and_load_by_url_lang_style() {
        let db = open(Path::new(":memory:")).unwrap();
        let url = "https://example.org/wiki/Page";
        assert!(!has_page_translations(&db, url, "한국어", "natural").unwrap());

        save_page_translations(
            &db,
            url,
            "한국어",
            "natural",
            &[
                ("hash1".into(), "첫 번째 문단".into()),
                ("hash2".into(), "두 번째 문단".into()),
            ],
        )
        .unwrap();

        assert!(has_page_translations(&db, url, "한국어", "natural").unwrap());
        assert!(!has_page_translations(&db, url, "한국어", "literal").unwrap());

        let loaded = load_page_translations(&db, url, "한국어", "natural").unwrap();
        assert_eq!(
            loaded.get("hash1").map(String::as_str),
            Some("첫 번째 문단")
        );
        assert_eq!(
            loaded.get("hash2").map(String::as_str),
            Some("두 번째 문단")
        );
    }
}
