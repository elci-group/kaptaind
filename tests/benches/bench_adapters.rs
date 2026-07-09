//! Micro-benchmarks for the language adapters (Workstream A1).
//!
//! Parses representative Rust / TypeScript / Python snippets through the real
//! adapters and measures `parse_ast` throughput.

use kaptaind::diff::lang::adapter::LanguageAdapter;
use kaptaind::diff::lang::adapters::{PythonAdapter, RustAdapter, TypeScriptAdapter};
use std::path::PathBuf;
use tempfile::TempDir;

fn main() {
    divan::main();
}

const RUST_SRC: &str = r#"
use std::collections::HashMap;

pub struct Config {
    pub name: String,
}

pub fn build(name: String) -> Config {
    Config { name }
}

pub async fn fetch(id: u64) -> Result<u64, &'static str> {
    Ok(id + 1)
}

impl Config {
    pub fn label(&self) -> &str { &self.name }
    fn hidden(&self) {}
}
"#;

const TS_SRC: &str = r#"
export interface Config {
    name: string;
    version: number;
}

export function build(name: string): Config {
    return { name, version: 1 };
}

export class Runner {
    public run(): void {}
    private secret(): void {}
}

export const VALUE: number = 42;
"#;

const PY_SRC: &str = r#"
class Config:
    def __init__(self, name):
        self.name = name

    def label(self):
        return self.name

def build(name):
    return Config(name)

def fetch(item_id):
    return item_id + 1

VERSION = "0.1.0"
"#;

fn write_source(name: &str, src: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, src).unwrap();
    (dir, path)
}

#[divan::bench]
fn parse_rust(bencher: divan::Bencher) {
    let adapter = RustAdapter;
    bencher
        .with_inputs(|| write_source("lib.rs", RUST_SRC))
        .bench_values(|(dir, path)| {
            let _keep = dir;
            adapter.parse_ast(&path)
        });
}

#[divan::bench]
fn parse_typescript(bencher: divan::Bencher) {
    let adapter = TypeScriptAdapter;
    bencher
        .with_inputs(|| write_source("index.ts", TS_SRC))
        .bench_values(|(dir, path)| {
            let _keep = dir;
            adapter.parse_ast(&path)
        });
}

#[divan::bench]
fn parse_python(bencher: divan::Bencher) {
    let adapter = PythonAdapter;
    bencher
        .with_inputs(|| write_source("mod.py", PY_SRC))
        .bench_values(|(dir, path)| {
            let _keep = dir;
            adapter.parse_ast(&path)
        });
}
