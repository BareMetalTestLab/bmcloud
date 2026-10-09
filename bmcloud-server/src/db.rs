use rusqlite::Connection;

pub fn init(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    create_schema(&conn)?;
    Ok(conn)
}

pub fn init_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    create_schema(&conn)?;
    Ok(conn)
}

fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            login TEXT NOT NULL UNIQUE,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES users(id),
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS devices (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id),
            name TEXT NOT NULL,
            token TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS device_state (
            device_id INTEGER PRIMARY KEY REFERENCES devices(id),
            state TEXT NOT NULL CHECK (state IN ('offline','online','error','busy')),
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;
    
    Ok(())
}
