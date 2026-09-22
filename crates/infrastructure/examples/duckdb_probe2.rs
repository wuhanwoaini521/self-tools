fn main() {
    let dir = std::env::temp_dir().join(format!("duckdb-probe2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let live = dir.join("live.duckdb");
    let dest = dir.join("snap.duckdb.bak");
    {
        let c = duckdb::Connection::open(&live).expect("open");
        c.execute_batch("CREATE TABLE t(a INTEGER); INSERT INTO t VALUES (1),(2);").expect("seed");
        drop(c);
    }
    let lit = dest.display().to_string().replace('\'', "''");
    let mem = duckdb::Connection::open_in_memory().expect("mem");
    let _ = mem.execute_batch("SET external_access = true");
    let _ = mem.execute_batch(&format!("ATTACH '{lit}' AS snap"));
    // 打开 live 作为默认 (主) 库的副本：attach live 为另一个别名
    let live_lit = live.display().to_string().replace('\'', "''");
    let _ = mem.execute_batch(&format!("ATTACH '{live_lit}' AS src (READ_ONLY)"));
    for sql in [
        "COPY FROM DATABASE src TO snap".to_string(),
        format!("COPY FROM DATABASE \"{live_lit}\" TO snap"),
        format!("COPY FROM DATABASE '{live_lit}' TO snap"),
        "COPY FROM DATABASE live TO snap".to_string(),
    ] {
        let r = mem.execute_batch(&sql);
        println!("{sql}  -> {:?}", r);
        if dest.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            let cc = duckdb::Connection::open(&dest).expect("open snap");
            let n = cc.query_row("SELECT count(*) FROM duckdb_tables()", [], |r| r.get::<_, i64>(0));
            println!("   snap tables = {n:?}");
            let rr = cc.query_row("SELECT count(*) FROM t", [], |r| r.get::<_, i64>(0));
            println!("   snap rows   = {rr:?}");
            drop(cc);
            if n.is_ok() {
                let _ = std::fs::remove_file(&dest);
            }
        }
    }
    drop(mem);
    let _ = std::fs::remove_dir_all(&dir);
}
