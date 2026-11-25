//! Aggregate-only measurements for the backfill benchmark. Not a stable API.

use crate::discover;
use crate::pipeline::{self, Job};
use crate::record::{self, Kind, QueryRefs};
use crate::toplevel;
use kanjo_core::paths::Paths;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct ParseStats {
    pub files: u64,
    pub bytes: u64,
    pub read_errors: u64,
    /// Merged rows (one per message id and file).
    pub rows: u64,
    pub rows_with_query_source: u64,
    pub rows_with_tools: u64,
    /// Assistant records that produced a row before merging by message id.
    pub usage_records: u64,
    pub assistant_without_message: u64,
    pub assistant_without_usage: u64,
    pub synthetic: u64,
    pub synthetic_with_tokens: u64,
    pub tool_sets: u64,
    pub quarantined: u64,
    pub lines: u64,
    pub known_types: BTreeMap<&'static str, u64>,
    pub unknown_lines: u64,
    pub invalid_lines: u64,
    /// Lines the byte scanner rejected but serde accepted.
    pub scanner_fallbacks: u64,
    /// Lines where the byte scanner and serde disagree on the top-level type.
    pub scanner_mismatches: u64,
    /// Key signatures of the handled metadata record types, as "type signature".
    pub meta_signatures: BTreeMap<String, u64>,
}

impl ParseStats {
    fn merge(&mut self, o: ParseStats) {
        self.files += o.files;
        self.bytes += o.bytes;
        self.read_errors += o.read_errors;
        self.rows += o.rows;
        self.rows_with_query_source += o.rows_with_query_source;
        self.rows_with_tools += o.rows_with_tools;
        self.usage_records += o.usage_records;
        self.assistant_without_message += o.assistant_without_message;
        self.assistant_without_usage += o.assistant_without_usage;
        self.synthetic += o.synthetic;
        self.synthetic_with_tokens += o.synthetic_with_tokens;
        self.tool_sets += o.tool_sets;
        self.quarantined += o.quarantined;
        self.lines += o.lines;
        for (k, v) in o.known_types {
            *self.known_types.entry(k).or_default() += v;
        }
        self.unknown_lines += o.unknown_lines;
        self.invalid_lines += o.invalid_lines;
        self.scanner_fallbacks += o.scanner_fallbacks;
        self.scanner_mismatches += o.scanner_mismatches;
        for (k, v) in o.meta_signatures {
            *self.meta_signatures.entry(k).or_default() += v;
        }
    }
}

fn parallel(jobs: &[Job], f: impl Fn(&Job) -> ParseStats + Sync) -> ParseStats {
    let next = AtomicUsize::new(0);
    let total = Mutex::new(ParseStats::default());
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                let mut mine = ParseStats::default();
                while let Some(job) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                    mine.merge(f(job));
                }
                crate::lock(&total).merge(mine);
            });
        }
    });
    total.into_inner().unwrap_or_else(|e| e.into_inner())
}

fn jobs(paths: &Paths) -> Vec<Job> {
    let mut jobs = pipeline::plan(discover::walk(&paths.claude_projects()), &Default::default());
    jobs.sort_by_key(|j| std::cmp::Reverse(j.size));
    jobs
}

/// Parses every transcript exactly as a backfill would, without writing.
pub fn parse_only(paths: &Paths) -> ParseStats {
    parallel(&jobs(paths), |job| {
        let mut refs = QueryRefs::default();
        match pipeline::parse_job(job, &mut refs, false) {
            Ok(p) => ParseStats {
                files: 1,
                bytes: p.bytes,
                rows: p.chunk.rows.len() as u64,
                rows_with_query_source: p.chunk.rows.iter().filter(|r| r.query_source.is_some()).count() as u64,
                rows_with_tools: p.chunk.rows.iter().filter(|r| !r.tools.is_empty()).count() as u64,
                tool_sets: p.chunk.tools.len() as u64,
                usage_records: p.chunk.counts.usage_records,
                assistant_without_message: p.chunk.counts.no_message,
                assistant_without_usage: p.chunk.counts.no_usage,
                synthetic: p.chunk.counts.synthetic,
                synthetic_with_tokens: p.chunk.counts.synthetic_with_tokens,
                quarantined: p.chunk.quarantine.len() as u64,
                ..Default::default()
            },
            Err(_) => ParseStats { read_errors: 1, ..Default::default() },
        }
    })
}

/// Compares the byte scanner with a full serde parse on every line.
pub fn verify_scanner(paths: &Paths) -> ParseStats {
    parallel(&jobs(paths), |job| {
        let mut st = ParseStats { files: 1, ..Default::default() };
        let Ok(file) = std::fs::File::open(&job.target.path) else {
            st.read_errors = 1;
            return st;
        };
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        while let Ok(n) = reader.read_until(b'\n', &mut buf) {
            if n == 0 || buf[n - 1] != b'\n' {
                break;
            }
            st.lines += 1;
            st.bytes += n as u64;
            let line = &buf[..n - 1];
            let scanned = toplevel::scan(line, |_| false);
            let probed = toplevel::probe(line);
            match (&scanned, &probed) {
                (None, Some(_)) => st.scanner_fallbacks += 1,
                (Some(s), Some(p)) if s.kind.map(|k| k.to_vec()) != p.kind.as_ref().map(|k| k.as_bytes().to_vec()) => {
                    st.scanner_mismatches += 1
                }
                _ => {}
            }
            match record::kind_of(line) {
                Kind::Known(k) => {
                    *st.known_types.entry(k).or_default() += 1;
                    if matches!(k, "ai-title" | "session-owner" | "api-request" | "tools-used") {
                        if let Some(s) = &scanned {
                            let sig = toplevel::signature(s.keys.iter().copied());
                            *st.meta_signatures.entry(format!("{k} {sig}")).or_default() += 1;
                        }
                    }
                }
                Kind::Unknown { .. } => st.unknown_lines += 1,
                Kind::Invalid => st.invalid_lines += 1,
            }
            buf.clear();
        }
        st
    })
}
