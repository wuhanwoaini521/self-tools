fn main() {
    let dir = std::env::temp_dir().join(format!("duckdb-syntax-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let live = dir.join("live.duckdb");
    let dest = dir.join("snap.duckdb.bak");
    {
        let c = duckdb::Connection::open(&live).expect("open");
        c.execute_batch("CREATE TABLE t(a INTEGER); INSERT INTO t VALUES (1),(2);").expect("seed");
        drop(c);
    }
    let c = duckdb::Connection::open(&live).expect("open live");
    let lit = dest.display().to_string().replace('\'', "''");
    for sql in [
        format!("COPY FROM DATABASE TO '{lit}'"),
    ] {
        let _ = c.execute_batch("SET external_access = true");
        let r = c.execute_batch(&sql);
        println!("SQL= {sql}\n  -> {:?}\n", r);
    }
    drop(c);
    // try ATTACH + COPY FROM DATABASE
    if !dest.exists() {
        let c2 = duckdb::Connection::open_in_memory().expect("mem");
        let _ = c2.execute_batch("SET external_access = true");
        let attach = format!("ATTACH '{lit}' AS snap");
        let r = c2.execute_batch(&attach);
        println!("ATTACH -> {r:?}");
        let r2 = c2.execute_batch("COPY FROM DATABASE live TO snap");
        println!("COPY FROM DATABASE live TO snap -> {r2:?}");
        let r3 = c2.execute_batch("USE live");
        println!("USE live -> {r3:?}");
        let r4 = c2.execute_batch(&format!("COPY FROM DATABASE TO '{lit}'"));
        println!("COPY FROM DATABASE TO (after USE) -> {r4:?}");
        drop(c2);
    }
    if !dest.exists() {
        let c3 = duckdb::Connection::open(&live).expect("live");
        let _ = c3.execute_batch("SET external_access = true");
        let r = c3.execute_batch(&format!("EXPORT DATABASE '{lit}'"));
        println!("EXPORT DATABASE -> {r:?}");
        drop(c3);
    }
    println!("dest exists: {}", dest.exists());
    if dest.exists() {
        let cc = duckdb::Connection::open(&dest).expect("open snap");
        let n: i64 = cc.query_row("SELECT count(*) FROM duckdb_tables()", [], |r| r.get(0)).expect("tables");
        println!("snap tables = {n}");
        let rows: i64 = cc.query_row("SELECT count(*) FROM t", [], |r| r.get(0)).expect("rows");
        println!("snap rows = {rows}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
