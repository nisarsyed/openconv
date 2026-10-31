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

fn admit(host: &mut Member, joiner: &mut Member) -> Vec<u8> {
    let commit = host.propose_add(&joiner.key_package().unwrap()).unwrap();
    let Event::Admitted { welcome, .. } = host.receive(&commit).unwrap() else {
        panic!("should have been admitted");
    };
    joiner.join(&welcome).unwrap();
    commit
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("openconv-measure-{}-{name}", std::process::id()))
}

fn main() {
    println!("== send/receive cost, in-memory (no vault) ==");
    let mut alice = Member::new("alice").unwrap();
    let mut bob = Member::new("bob").unwrap();
    alice.create_group().unwrap();
    admit(&mut alice, &mut bob);

    let n = 500;
    let t = Instant::now();
    for i in 0..n {
        let wire = alice.send(&format!("message {i}")).unwrap();
        bob.receive(&wire).unwrap();
    }
    let memory = t.elapsed();
    println!("  {n} round trips: {memory:?}  ({:?}/msg)", memory / n);

    println!("\n== send/receive cost, persisted to an encrypted vault ==");
    let dir = tmp("persisted");
    let mut alice =
        Member::restore(Vault::with_key(dir.join("a.vault"), [1u8; 32]), "alice").unwrap();
    let mut bob = Member::restore(Vault::with_key(dir.join("b.vault"), [2u8; 32]), "bob").unwrap();
    alice.create_group().unwrap();
    admit(&mut alice, &mut bob);

    let t = Instant::now();
    for i in 0..n {
        let wire = alice.send(&format!("message {i}")).unwrap();
        bob.receive(&wire).unwrap();
    }
    let persisted = t.elapsed();
    let size = std::fs::metadata(dir.join("a.vault")).unwrap().len();
    println!(
        "  {n} round trips: {persisted:?}  ({:?}/msg)",
        persisted / n
    );
    println!("  vault after {n} messages: {} KiB", size / 1024);
    println!(
        "  persistence overhead: {:.1}x",
        persisted.as_secs_f64() / memory.as_secs_f64()
    );
    std::fs::remove_dir_all(&dir).ok();

    println!("\n== group size scaling (in-memory) ==");
    println!("  members   add commit   vault KiB   send    broadcast-receive");
    for target in [2usize, 5, 10, 25, 50] {
        let dir = tmp(&format!("n{target}"));
        let mut host =
            Member::restore(Vault::with_key(dir.join("h.vault"), [3u8; 32]), "host").unwrap();
        host.create_group().unwrap();

        let mut others = Vec::new();
        let mut last_add = std::time::Duration::ZERO;
        let mut commit_size = 0;

        for i in 1..target {
            let mut m = Member::new(&format!("m{i}")).unwrap();
            let t = Instant::now();
            let commit = admit(&mut host, &mut m);
            last_add = t.elapsed();
            commit_size = commit.len();
            // Existing members must apply the commit.
            for other in &mut others {
                let _: &mut Member = other;
                other.receive(&commit).ok();
            }
            others.push(m);
        }

        let t = Instant::now();
        let wire = host.send("hello everyone").unwrap();
        let send = t.elapsed();

        let t = Instant::now();
        for other in &mut others {
            other.receive(&wire).unwrap();
        }
        let fanout = t.elapsed();

        let size = std::fs::metadata(dir.join("h.vault")).unwrap().len() / 1024;
        println!(
            "  {target:>7}   {last_add:>9.2?}   {size:>9}   {send:>5.2?}   {fanout:>8.2?}  (commit {commit_size}B)"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
