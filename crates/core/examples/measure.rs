//! Rough measurements to find where the current design falls over.
//!
//! Deliberately not a benchmark suite: one run, no statistics, no regression
//! tracking. It exists to answer "which design decision should we revisit
//! next", and its numbers are orders of magnitude, not budgets. A real
//! benchmark suite belongs with CI to run it and a stable design to measure.
//!
//! `cargo run --release --example measure`

// Reporting numbers to a human is this binary's entire job.
#![allow(clippy::print_stdout)]

use openconv_core::{Event, Member, Vault};
use std::time::Instant;

fn admit(host: &mut Member, joiner: &mut Member, channel: &[u8]) -> Vec<u8> {
    let commit = host
        .propose_add(channel, &joiner.key_package().unwrap())
        .unwrap();
    let Event::Admitted { welcome, .. } = host.receive(channel, &commit).unwrap() else {
        panic!("should have been admitted");
    };
    joiner.join(&welcome).unwrap();
    commit
}

/// The store's size on disk, as (database, WAL) in KiB. The WAL holds
/// writes not yet checkpointed into the database.
fn store_kib(db: &std::path::Path) -> (u64, u64) {
    let size = |p: &std::path::Path| std::fs::metadata(p).map_or(0, |m| m.len()) / 1024;
    (size(db), size(&db.with_extension("db-wal")))
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("openconv-measure-{}-{name}", std::process::id()))
}

fn main() {
    println!("== send/receive cost, in-memory (no vault) ==");
    let mut alice = Member::new("alice").unwrap();
    let mut bob = Member::new("bob").unwrap();
    let channel = alice.create_group().unwrap();
    admit(&mut alice, &mut bob, &channel);

    let n = 500;
    let t = Instant::now();
    for i in 0..n {
        let wire = alice.send(&channel, &format!("message {i}")).unwrap();
        bob.receive(&channel, &wire).unwrap();
    }
    let memory = t.elapsed();
    println!("  {n} round trips: {memory:?}  ({:?}/msg)", memory / n);

    println!("\n== send/receive cost, both members persisted to the encrypted store ==");
    let dir = tmp("persisted");
    let mut alice = Member::restore(Vault::with_key(dir.join("a.db"), [1u8; 32]), "alice").unwrap();
    let mut bob = Member::restore(Vault::with_key(dir.join("b.db"), [2u8; 32]), "bob").unwrap();
    let channel = alice.create_group().unwrap();
    admit(&mut alice, &mut bob, &channel);

    let t = Instant::now();
    for i in 0..n {
        let wire = alice.send(&channel, &format!("message {i}")).unwrap();
        bob.receive(&channel, &wire).unwrap();
    }
    let persisted = t.elapsed();
    let size = store_kib(&dir.join("a.db"));
    println!(
        "  {n} round trips: {persisted:?}  ({:?}/msg)",
        persisted / n
    );
    println!(
        "  store after {n} messages: {} KiB database + {} KiB WAL",
        size.0, size.1
    );
    println!(
        "  persistence overhead: {:.1}x",
        persisted.as_secs_f64() / memory.as_secs_f64()
    );
    std::fs::remove_dir_all(&dir).ok();

    // The host is persisted, so its add and send columns include the store.
    // The other members are in memory, so broadcast-receive does not.
    println!("\n== group size scaling (host persisted, others in memory) ==");
    println!("  members   add commit   db+WAL KiB   send    broadcast-receive");
    for target in [2usize, 5, 10, 25, 50] {
        let dir = tmp(&format!("n{target}"));
        let mut host =
            Member::restore(Vault::with_key(dir.join("h.db"), [3u8; 32]), "host").unwrap();
        let channel = host.create_group().unwrap();

        let mut others = Vec::new();
        let mut last_add = std::time::Duration::ZERO;
        let mut commit_size = 0;

        for i in 1..target {
            let mut m = Member::new(&format!("m{i}")).unwrap();
            let t = Instant::now();
            let commit = admit(&mut host, &mut m, &channel);
            last_add = t.elapsed();
            commit_size = commit.len();
            // Existing members must apply the commit.
            for other in &mut others {
                let _: &mut Member = other;
                other.receive(&channel, &commit).ok();
            }
            others.push(m);
        }

        let t = Instant::now();
        let wire = host.send(&channel, "hello everyone").unwrap();
        let send = t.elapsed();

        let t = Instant::now();
        for other in &mut others {
            other.receive(&channel, &wire).unwrap();
        }
        let fanout = t.elapsed();

        let size = store_kib(&dir.join("h.db"));
        println!(
            "  {target:>7}   {last_add:>9.2?}   {:>4}+{:<5}   {send:>5.2?}   {fanout:>8.2?}  (commit {commit_size}B)",
            size.0, size.1
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
