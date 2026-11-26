import type { AuditEntry, DoctorCheck, IngestStatus, LiveEvent, PurgeReport, PurgeRequest, QuarantineRow, Settings } from '../api/contract';
import type { Dataset } from './data';

/**
 * System-page parts of the mock: doctor checks in both languages (mirroring
 * crates/kanjo-setup/src/doctor.rs), quarantine rows, a seeded audit history
 * with a verifiable hash chain, backups, rescans and settings validation.
 */

const DAY = 86_400_000;
const HOUR = 3_600_000;
const MIN = 60_000;
const DATA_DIR = '/home/dev/.local/share/kanjo';
const CLAUDE_DIR = '/home/dev/.claude';
export const SAMPLE_CHAIN = 'bash ~/.claude/statusline-git.sh';

type L = { pl: boolean };
const tx = (l: L, en: string, pl: string) => (l.pl ? pl : en);

function plPlural(n: number, one: string, few: string, many: string): string {
  const a = Math.abs(n);
  if (a === 1) return one;
  return a % 10 >= 2 && a % 10 <= 4 && !(a % 100 >= 12 && a % 100 <= 14) ? few : many;
}

function ago(l: L, ms: number): string {
  const v = Math.max(0, ms);
  if (v < MIN) return tx(l, 'just now', 'przed chwilą');
  if (v < HOUR) return `${Math.floor(v / MIN)} ${tx(l, 'min ago', 'min temu')}`;
  if (v < DAY) return `${Math.floor(v / HOUR)} ${tx(l, 'h ago', 'godz. temu')}`;
  const d = Math.floor(v / DAY);
  return `${d} ${l.pl ? plPlural(d, 'dzień temu', 'dni temu', 'dni temu') : d === 1 ? 'day ago' : 'days ago'}`;
}

export interface CheckInput {
  sizeBytes: number;
  schema: number;
  pricingVersion: string;
  unpriced: string[];
  quarantined: number;
  ccVersion: string | null;
}

/** The sixteen doctor checks in output order, worded as the server words them. */
export function doctorChecks(ds: Dataset, input: CheckInput, lang: string | undefined): DoctorCheck[] {
  const l: L = { pl: (lang ?? '').toLowerCase().startsWith('pl') };
  const up = 'kanjo up';
  const now = ds.clock();
  const h = ds.hooks;
  const settings = `${CLAUDE_DIR}/settings.json`;
  const c = (id: string, status: DoctorCheck['status'], title: string, detail: string, fix: string | null = null): DoctorCheck => ({ id, status, title, detail, fix });
  const out: DoctorCheck[] = [];
  out.push(c('claude_dir', 'ok', tx(l, 'Claude Code directory', 'Katalog Claude Code'), CLAUDE_DIR));
  const files = ds.ingest.files_total;
  const projects = new Set(ds.sessions.map((s) => s.project)).size;
  if (files === 0) {
    out.push(
      c(
        'projects',
        'warn',
        tx(l, 'Session transcripts', 'Transkrypty sesji'),
        `${CLAUDE_DIR}/projects: ${tx(l, 'no transcripts yet', 'brak transkryptów')}`,
        tx(l, 'Run a Claude Code session; Kanjo picks up new transcripts automatically.', 'Przeprowadź sesję w Claude Code; Kanjo sam wczyta nowe transkrypty.'),
      ),
    );
  } else {
    const detail = l.pl
      ? `${files} ${plPlural(files, 'transkrypt', 'transkrypty', 'transkryptów')} w ${projects} ${projects === 1 ? 'projekcie' : 'projektach'}`
      : `${files} ${files === 1 ? 'transcript' : 'transcripts'} in ${projects} ${projects === 1 ? 'project' : 'projects'}`;
    out.push(c('projects', 'ok', tx(l, 'Session transcripts', 'Transkrypty sesji'), detail));
  }
  out.push(c('settings_json', 'ok', tx(l, 'Claude Code settings.json', 'Plik settings.json Claude Code'), settings));
  const chainTitle = tx(l, 'Statusline chain', 'Łańcuch statusline');
  if (h.statusline_installed) {
    out.push(c('statusline', 'ok', 'Statusline', tx(l, 'installed', 'zainstalowany')));
    out.push(
      h.statusline_chain
        ? c('chain', 'ok', chainTitle, l.pl ? `Kanjo przekazuje dane do Twojej poprzedniej komendy: ${h.statusline_chain}` : `Kanjo forwards the payload to your previous command: ${h.statusline_chain}`)
        : c('chain', 'ok', chainTitle, tx(l, 'no previous statusline command; Kanjo shows its own line', 'brak poprzedniej komendy statusline; Kanjo pokazuje własną linię')),
    );
  } else {
    out.push(
      c(
        'statusline',
        'warn',
        'Statusline',
        tx(l, 'not installed; limit percentages are estimates only', 'niezainstalowany; procenty limitów są tylko szacunkiem'),
        l.pl ? `Uruchom ${up}, aby zainstalować statusline.` : `Run ${up} to install the statusline.`,
      ),
    );
    out.push(c('chain', 'ok', chainTitle, tx(l, 'nothing to chain', 'nie ma czego łączyć')));
  }
  const hooksTitle = tx(l, 'Session hooks', 'Hooki sesji');
  out.push(
    h.session_hooks_installed
      ? c('session_hooks', 'ok', hooksTitle, 'SessionStart, Stop, SessionEnd')
      : c(
          'session_hooks',
          'warn',
          hooksTitle,
          `${tx(l, 'missing', 'brakuje')}: SessionStart, Stop, SessionEnd`,
          l.pl ? `Uruchom ${up}, aby dodać hooki SessionStart, Stop i SessionEnd.` : `Run ${up} to add the SessionStart, Stop and SessionEnd hooks.`,
        ),
  );
  const payloadTitle = tx(l, 'Last statusline payload', 'Ostatnie dane ze statusline');
  if (h.last_statusline_at !== null && now - h.last_statusline_at <= DAY) out.push(c('statusline_payload', 'ok', payloadTitle, ago(l, now - h.last_statusline_at)));
  else
    out.push(
      c(
        'statusline_payload',
        'warn',
        payloadTitle,
        tx(l, 'none received yet', 'jeszcze nic nie dotarło'),
        tx(
          l,
          'Start a Claude Code session and send one message; the payload arrives after the first response.',
          'Uruchom sesję Claude Code i wyślij jedną wiadomość; dane przychodzą po pierwszej odpowiedzi.',
        ),
      ),
    );
  out.push(c('data_dir', 'ok', tx(l, 'Data directory', 'Katalog danych'), `${DATA_DIR} (${tx(l, 'owner-only access', 'dostęp tylko dla właściciela')})`));
  out.push(c('database', 'ok', tx(l, 'Database', 'Baza danych'), `${DATA_DIR}/kanjo.db (${(input.sizeBytes / 1_048_576).toFixed(1)} MB)`));
  out.push(c('encryption', 'ok', tx(l, 'Encryption', 'Szyfrowanie'), `SQLCipher, ${tx(l, 'key in the system keyring', 'klucz w systemowym pęku kluczy')}`));
  out.push(c('schema', 'ok', tx(l, 'Schema version', 'Wersja schematu'), String(input.schema)));
  out.push(
    input.unpriced.length
      ? c(
          'pricing',
          'warn',
          tx(l, 'Price table', 'Cennik'),
          l.pl
            ? `${input.pricingVersion}; modele bez ceny (ich koszt nie jest liczony): ${input.unpriced.join(', ')}`
            : `${input.pricingVersion}; models without a price (their cost is not counted): ${input.unpriced.join(', ')}`,
          tx(l, 'Update Kanjo or import a signed price table that covers these models.', 'Zaktualizuj Kanjo albo zaimportuj podpisany cennik obejmujący te modele.'),
        )
      : c('pricing', 'ok', tx(l, 'Price table', 'Cennik'), input.pricingVersion),
  );
  const n = input.quarantined;
  out.push(
    n === 0
      ? c('quarantine', 'ok', tx(l, 'Unrecognised records', 'Nierozpoznane rekordy'), tx(l, 'none', 'brak'))
      : c(
          'quarantine',
          'warn',
          tx(l, 'Unrecognised records', 'Nierozpoznane rekordy'),
          l.pl
            ? `odłożono na bok ${n} ${plPlural(n, 'rekord', 'rekordy', 'rekordów')} o nieznanym kształcie (zapisano tylko typ i sygnaturę kluczy)`
            : `set aside ${n} ${n === 1 ? 'record' : 'records'} of unknown shape (only the type and key signature are kept)`,
          tx(l, 'Usually a newer Claude Code format; update Kanjo. The list is on this page.', 'Zwykle to nowszy format Claude Code; zaktualizuj Kanjo. Lista jest na tej stronie.'),
        ),
  );
  out.push(
    h.service_installed
      ? c('service', 'ok', tx(l, 'Background service', 'Usługa w tle'), tx(l, 'running', 'działa'))
      : c(
          'service',
          'warn',
          tx(l, 'Background service', 'Usługa w tle'),
          tx(l, 'not installed; data is collected only while kanjo serve runs', 'niezainstalowana; dane zbierane są tylko, gdy działa kanjo serve'),
          l.pl ? `Uruchom ${up}, aby Kanjo startował razem z systemem.` : `Run ${up} to start Kanjo with your session.`,
        ),
  );
  out.push(
    input.ccVersion
      ? c('claude_code_version', 'ok', tx(l, 'Claude Code version', 'Wersja Claude Code'), input.ccVersion)
      : c(
          'claude_code_version',
          'warn',
          tx(l, 'Claude Code version', 'Wersja Claude Code'),
          tx(l, 'not seen yet', 'jeszcze nieznana'),
          tx(l, 'Run a Claude Code session; the version is read from its data.', 'Przeprowadź sesję w Claude Code; wersja zostanie odczytana z jej danych.'),
        ),
  );
  out.push(
    c(
      'cleanup_period',
      'ok',
      tx(l, 'Transcript retention', 'Przechowywanie transkryptów'),
      l.pl
        ? 'Claude Code usuwa transkrypty starsze niż 30 dni (domyślnie w Claude Code); Kanjo trzyma własne agregaty, więc historia kosztów zostaje, ale podgląd transkryptów i ponowne skanowanie sięgają tylko 30 dni wstecz'
        : 'Claude Code deletes transcripts older than 30 days (Claude Code default); Kanjo keeps its own aggregates, so cost history stays, but the transcript viewer and rescans only reach 30 days back',
    ),
  );
  return out;
}

/** Record shapes set aside by the importer; only the unpriced scenario has any. */
export function quarantineRows(ds: Dataset): QuarantineRow[] {
  if (ds.scenario !== 'unpriced') return [];
  const now = ds.clock();
  return [
    {
      record_type: 'assistant',
      signature: 'cwd,gitBranch,isSidechain,message,parentUuid,requestId,serverToolUse,sessionId,timestamp,type,uuid,version',
      reason: 'usage does not deserialize',
      cc_version: '2.4.1',
      count: 2,
      last_seen: now - DAY - 3 * HOUR,
    },
    {
      record_type: 'workspace-snapshot',
      signature: 'files,sessionId,snapshotId,timestamp,type,version',
      reason: 'unknown record type',
      cc_version: '2.4.1',
      count: 1,
      last_seen: now - 2 * DAY,
    },
  ];
}

function hashHex(input: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < input.length; i++) {
    const ch = input.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  return ((h2 >>> 0).toString(16).padStart(8, '0') + (h1 >>> 0).toString(16).padStart(8, '0')).repeat(4);
}

/** Recomputes the hash chain the way auditEntry() builds it, oldest first. */
export function verifyChain(ds: Dataset): boolean {
  let prev = '0'.repeat(64);
  for (let i = ds.audit.length - 1; i >= 0; i--) {
    const e = ds.audit[i];
    if (e.hash !== hashHex(prev + e.action + e.target)) return false;
    prev = e.hash;
  }
  return true;
}

/** Fills in a believable history: backups, past exports, setup, retention runs. */
export function seedSystem(ds: Dataset): void {
  if (ds.scenario === 'empty' || ds.scenario === 'onboarding') return;
  const now = ds.clock();
  if (ds.hooks.statusline_installed) ds.hooks = { ...ds.hooks, statusline_chain: SAMPLE_CHAIN };
  const existing = [...ds.audit].reverse();
  const plan: Omit<AuditEntry, 'id' | 'hash'>[] = [];
  const add = (ago: number, actor: string, action: string, target: string, detail: Record<string, unknown> = {}) =>
    plan.push({ ts: now - ago, actor, action, target, detail: detail as AuditEntry['detail'] });
  add(62 * DAY, 'cli', 'hooks.install', 'settings.json', { statusline: true, session_hooks: true, service: true });
  add(62 * DAY - 5 * MIN, 'local', 'settings.update', 'settings', { keys: ['language', 'privacy.session_titles'] });
  add(61 * DAY, 'local', 'settings.update', 'settings', { keys: ['plan'] });
  for (let w = 8; w >= 1; w--) {
    add(w * 7 * DAY + 3 * HOUR, 'service', 'retention.apply', 'requests', { requests: 0, alerts: w === 2 ? 3 : 0 });
    add(w * 7 * DAY + 2 * HOUR, 'service', 'backup.create', `kanjo-backup-week-${String(48 - w).padStart(2, '0')}.db`, { automatic: true });
    if (w % 2 === 0) add(w * 7 * DAY - 26 * HOUR, 'local', 'export', 'usage', { format: 'csv' });
    if (w % 3 === 0) add(w * 7 * DAY - 50 * HOUR, 'local', 'export', 'sessions', { format: 'csv' });
  }
  add(40 * DAY, 'local', 'settings.update', 'settings', { keys: ['currency'] });
  add(33 * DAY, 'local', 'data.purge', 'session', { requests: 41, sessions: 1, aggregates: 6 });
  add(20 * DAY, 'local', 'export', 'breakdown', { format: 'json' });
  add(12 * DAY, 'local', 'settings.update', 'settings', { keys: ['alerts.limit_thresholds'] });
  add(5 * DAY, 'local', 'export', 'alerts', { format: 'csv' });
  plan.sort((a, b) => a.ts - b.ts);
  const all = [...plan, ...existing.map(({ id: _id, hash: _h, ...rest }) => rest)];
  let prev = '0'.repeat(64);
  ds.audit = all
    .map((e, i) => {
      const hash = hashHex(prev + e.action + e.target);
      prev = hash;
      return { ...e, id: i + 1, hash };
    })
    .reverse();
  if (ds.scenario === 'tampered' && ds.audit.length > 10) ds.audit[9] = { ...ds.audit[9], target: 'sessions-edited' };
  for (let w = 1; w <= 3; w++) ds.backups.push({ file: `kanjo-backup-week-${String(48 - w).padStart(2, '0')}.db`, created_at: now - w * 7 * DAY + 2 * HOUR, size_bytes: 18_400_000 - w * 310_000 });
  ds.backups.sort((a, b) => b.created_at - a.created_at);
}

type Listener = (e: LiveEvent) => void;
const listeners = new WeakMap<Dataset, Set<Listener>>();

/** Lets the mock event stream forward events that endpoints cause (a rescan). */
export function subscribe(ds: Dataset, fn: Listener): () => void {
  let set = listeners.get(ds);
  if (!set) listeners.set(ds, (set = new Set()));
  set.add(fn);
  return () => set.delete(fn);
}

function emit(ds: Dataset, e: LiveEvent): void {
  for (const fn of listeners.get(ds) ?? []) fn(e);
}

/** Re-reads every transcript: progress runs from zero to all files over a few seconds. */
export function startRescan(ds: Dataset, stepMs = 700): IngestStatus {
  if (ds.ingest.backfill_running) return ds.ingest;
  const total = ds.ingest.files_total;
  ds.ingest = { ...ds.ingest, backfill_running: total > 0, files_done: 0, bytes_done: 0 };
  if (total === 0) return ds.ingest;
  const step = () => {
    const done = Math.min(total, ds.ingest.files_done + Math.max(1, Math.ceil(total / 6)));
    ds.ingest = { ...ds.ingest, files_done: done, bytes_done: done * 412_000, backfill_running: done < total, last_event_at: ds.clock() };
    emit(ds, { type: 'ingest', data: ds.ingest });
    if (done < total) setTimeout(step, stepMs);
  };
  setTimeout(step, stepMs);
  return ds.ingest;
}

/** Deletion on request, following crates/kanjo-ops/src/retention.rs: settings, projects, budgets and the audit log survive. */
export function purgeDataset(ds: Dataset, r: PurgeRequest): PurgeReport | string {
  const selectors = Number(r.project != null) + Number(r.session != null) + Number(r.before != null) + Number(!!r.everything);
  if (selectors !== 1) return 'a purge needs exactly one of project, session, before or everything';
  const hit = (project: number, session: string, ts: number) =>
    r.everything || (r.project != null && project === r.project) || (r.session != null && session === r.session) || (r.before != null && ts < r.before);
  const gone = ds.records.filter((x) => hit(x.project, x.session, x.ts));
  ds.records = ds.records.filter((x) => !hit(x.project, x.session, x.ts));
  const sessionsBefore = ds.sessions.length;
  const goneSessions = new Set(ds.sessions.filter((s) => hit(s.project, s.id, s.last)).map((s) => s.id));
  ds.sessions = ds.sessions.filter((s) => !goneSessions.has(s.id));
  const hours = new Set(gone.map((x) => `${x.project}:${Math.floor(x.ts / HOUR)}`));
  const alertsBefore = ds.alerts.length;
  if (r.everything) ds.alerts = [];
  else if (r.before != null) ds.alerts = ds.alerts.filter((a) => a.ts >= (r.before as number));
  const timeScoped = r.everything || r.before != null;
  const report: PurgeReport = {
    requests: gone.reduce((n, x) => n + x.requests, 0),
    sessions: sessionsBefore - ds.sessions.length,
    aggregates: hours.size,
    limit_samples: timeScoped ? new Set(gone.map((x) => Math.floor(x.ts / HOUR))).size * 12 : 0,
    alerts: alertsBefore - ds.alerts.length,
    audit_entries: 0,
    quarantine: r.everything ? quarantineRows(ds).reduce((n, q) => n + q.count, 0) : 0,
  };
  ds.ingest = { ...ds.ingest, requests_stored: ds.records.reduce((n, x) => n + x.requests, 0) };
  return report;
}

/** The checks the real server applies to PUT /settings; returns the problem or null. */
export function validateSettings(s: Settings): string | null {
  if (!/^[A-Z]{3}$/.test(s.currency?.code ?? '')) return 'currency.code must be a three-letter ISO code';
  if (!(s.currency.usd_rate > 0) || s.currency.usd_rate > 1_000_000) return 'currency.usd_rate must be a positive number';
  if (!Number.isInteger(s.week_start) || s.week_start < 0 || s.week_start > 6) return 'week_start must be 0..6';
  if (s.plan.monthly_usd !== null && !(s.plan.monthly_usd >= 0)) return 'plan.monthly_usd must not be negative';
  const th = s.alerts.limit_thresholds;
  if (!th.length || th.some((x) => !Number.isFinite(x) || x < 1 || x > 100)) return 'alerts.limit_thresholds must hold values between 1 and 100';
  if (!(s.alerts.context_threshold >= 10 && s.alerts.context_threshold <= 100)) return 'alerts.context_threshold must be between 10 and 100';
  for (const [k, v] of Object.entries(s.retention)) if (!Number.isInteger(v) || v < 0 || v > 36_500) return `retention.${k} must be a whole number of days`;
  return null;
}
