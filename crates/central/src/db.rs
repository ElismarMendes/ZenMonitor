use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::str::FromStr;

pub async fn open(path: &str) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
    for stmt in [
        "CREATE TABLE IF NOT EXISTS route_revisions (id TEXT NOT NULL, revision INTEGER NOT NULL, definition TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0, ts INTEGER NOT NULL, PRIMARY KEY(id,revision))",
        "CREATE TABLE IF NOT EXISTS route_runs (command_id INTEGER NOT NULL, machine TEXT NOT NULL, route_id TEXT NOT NULL, revision INTEGER NOT NULL, status TEXT NOT NULL, ts INTEGER NOT NULL, message TEXT, PRIMARY KEY(command_id,machine))",
        "CREATE TABLE IF NOT EXISTS zen_points (machine TEXT NOT NULL, char TEXT NOT NULL, ts INTEGER NOT NULL, zen INTEGER NOT NULL)",
        "CREATE INDEX IF NOT EXISTS zen_points_ts ON zen_points(ts)",
        "CREATE TABLE IF NOT EXISTS pk_events (ts INTEGER NOT NULL, killer TEXT NOT NULL, killer_kind TEXT NOT NULL, victim TEXT NOT NULL, machine TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS pk_events_ts ON pk_events(ts)",
        "CREATE TABLE IF NOT EXISTS audit (ts INTEGER NOT NULL, who TEXT NOT NULL, machine TEXT NOT NULL, command TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS deaths (id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, machine TEXT NOT NULL, char TEXT NOT NULL, hp_before INTEGER NOT NULL, screen_hp INTEGER, png BLOB)",
        "CREATE INDEX IF NOT EXISTS deaths_ts ON deaths(ts)",
    ] {
        sqlx::query(stmt).execute(&pool).await?;
    }
    migrate_route_runs(&pool).await?;
    sqlx::query("UPDATE route_runs SET status='interrompida',message='central reiniciado; chegada não confirmada',finished_at=? WHERE status='enviada'").bind(crate::now() as i64).execute(&pool).await?;
    Ok(pool)
}

async fn migrate_route_runs(pool:&SqlitePool)->anyhow::Result<()> {
    let columns=sqlx::query("PRAGMA table_info(route_runs)").fetch_all(pool).await?;
    if !columns.iter().any(|c|c.get::<String,_>("name")=="char_name") {
        sqlx::query("ALTER TABLE route_runs ADD COLUMN char_name TEXT NOT NULL DEFAULT ''").execute(pool).await?;
    }
    if !columns.iter().any(|c|c.get::<String,_>("name")=="finished_at") {
        sqlx::query("ALTER TABLE route_runs ADD COLUMN finished_at INTEGER").execute(pool).await?;
    }
    sqlx::query("CREATE INDEX IF NOT EXISTS route_runs_observer ON route_runs(machine,char_name,ts DESC)").execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod route_migration_tests {
    use super::*;
    #[tokio::test]
    async fn old_route_history_is_preserved_without_inventing_character_identity() {
        let pool=SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE route_runs(command_id INTEGER,machine TEXT,route_id TEXT,revision INTEGER,status TEXT,ts INTEGER,message TEXT,PRIMARY KEY(command_id,machine))").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO route_runs VALUES(7,'PC1','route',1,'concluída',100,'chegada confirmada')").execute(&pool).await.unwrap();
        migrate_route_runs(&pool).await.unwrap();migrate_route_runs(&pool).await.unwrap();
        let row=sqlx::query("SELECT * FROM route_runs WHERE command_id=7").fetch_one(&pool).await.unwrap();
        assert_eq!(row.get::<String,_>("char_name"),"");
        assert_eq!(row.get::<String,_>("message"),"chegada confirmada");
        assert_eq!(row.get::<i64,_>("ts"),100);
        assert_eq!(row.get::<Option<i64>,_>("finished_at"),None);
    }
}

pub async fn insert_zen(pool: &SqlitePool, rows: &[(String, String, u64, u64)]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for (m, c, ts, zen) in rows {
        sqlx::query("INSERT INTO zen_points (machine, char, ts, zen) VALUES (?, ?, ?, ?)")
            .bind(m)
            .bind(c)
            .bind(*ts as i64)
            .bind(*zen as i64)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn insert_pk(
    pool: &SqlitePool,
    ts: u64,
    killer: &str,
    kind: &str,
    victim: &str,
    machine: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO pk_events (ts, killer, killer_kind, victim, machine) VALUES (?, ?, ?, ?, ?)")
        .bind(ts as i64)
        .bind(killer)
        .bind(kind)
        .bind(victim)
        .bind(machine)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn audit(pool: &SqlitePool, ts: u64, who: &str, machine: &str, command: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO audit (ts, who, machine, command) VALUES (?, ?, ?, ?)")
        .bind(ts as i64)
        .bind(who)
        .bind(machine)
        .bind(command)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeathRow {
    pub id: i64,
    pub ts: u64,
    pub machine: String,
    pub char_name: String,
    pub hp_before: u32,
    pub screen_hp: Option<u32>,
    pub has_png: bool,
}

pub async fn insert_death(
    pool: &SqlitePool,
    ts: u64,
    machine: &str,
    char_name: &str,
    hp_before: u32,
    screen_hp: Option<u32>,
    png: Option<&[u8]>,
) -> anyhow::Result<i64> {
    let r = sqlx::query("INSERT INTO deaths (ts, machine, char, hp_before, screen_hp, png) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(ts as i64)
        .bind(machine)
        .bind(char_name)
        .bind(hp_before as i64)
        .bind(screen_hp.map(|v| v as i64))
        .bind(png)
        .execute(pool)
        .await?;
    Ok(r.last_insert_rowid())
}

/// Últimas mortes (sem a imagem).
pub async fn recent_deaths(pool: &SqlitePool, limit: i64) -> anyhow::Result<Vec<DeathRow>> {
    let rows = sqlx::query(
        "SELECT id, ts, machine, char, hp_before, screen_hp, png IS NOT NULL FROM deaths ORDER BY ts DESC, id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| DeathRow {
            id: r.get(0),
            ts: r.get::<i64, _>(1) as u64,
            machine: r.get(2),
            char_name: r.get(3),
            hp_before: r.get::<i64, _>(4) as u32,
            screen_hp: r.get::<Option<i64>, _>(5).map(|v| v as u32),
            has_png: r.get::<i64, _>(6) != 0,
        })
        .collect())
}

pub async fn death_png(pool: &SqlitePool, id: i64) -> anyhow::Result<Option<Vec<u8>>> {
    let row = sqlx::query("SELECT png FROM deaths WHERE id = ?").bind(id).fetch_optional(pool).await?;
    Ok(row.and_then(|r| r.get::<Option<Vec<u8>>, _>(0)))
}

/// Ranking de players que mais mataram (monstros fora). `since` = 0 para "total".
pub async fn pk_ranking(pool: &SqlitePool, since: u64) -> anyhow::Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        "SELECT killer, COUNT(*) AS n FROM pk_events WHERE killer_kind = 'Player' AND ts >= ? \
         GROUP BY killer ORDER BY n DESC, killer LIMIT 10",
    )
    .bind(since as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<String, _>(0), r.get::<i64, _>(1)))
        .collect())
}

/// Histórico de 30 dias (Zen) e 1 ano (PK/auditoria).
pub async fn prune(pool: &SqlitePool, now: u64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM zen_points WHERE ts < ?")
        .bind((now - 30 * 86400) as i64)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM pk_events WHERE ts < ?")
        .bind((now - 365 * 86400) as i64)
        .execute(pool)
        .await?;
    // Mortes: 30 dias (cada uma carrega um print).
    sqlx::query("DELETE FROM deaths WHERE ts < ?")
        .bind((now - 30 * 86400) as i64)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM audit WHERE ts < ?")
        .bind((now - 365 * 86400) as i64)
        .execute(pool)
        .await?;
    Ok(())
}

/// Cópia diária consistente (VACUUM INTO funciona com WAL e com o banco em uso).
pub async fn backup(pool: &SqlitePool, path: &str) -> anyhow::Result<()> {
    let _ = std::fs::remove_file(path);
    sqlx::query("VACUUM INTO ?").bind(path).execute(pool).await?;
    Ok(())
}
