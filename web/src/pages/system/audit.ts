import type { AuditEntry } from '../../lib/api/contract';

/** Dictionary key under system.audit.actions for an action such as "hooks.uninstall". */
export function actionKey(action: string): string {
  return action.replace(/[^a-z0-9]+/gi, '_');
}

const ROLES = ['local', 'cli', 'service'] as const;
type Role = (typeof ROLES)[number];

function fields(e: Pick<AuditEntry, 'detail'>): Record<string, unknown> {
  const d = e.detail;
  return d && typeof d === 'object' && !Array.isArray(d) ? (d as Record<string, unknown>) : {};
}

/**
 * Who did it. Newer entries name the role in the actor field; older ones and the CLI
 * put the OS user name there, so the role is read from how the entry was written:
 * the CLI marks its entries with via "cli", the HTTP API records the browser's
 * address as client, and automatic jobs say so.
 */
export function actorKey(e: Pick<AuditEntry, 'detail' | 'actor'>): Role | null {
  if ((ROLES as readonly string[]).includes(e.actor)) return e.actor as Role;
  const o = fields(e);
  if (o.via === 'cli') return 'cli';
  if ('client' in o) return 'local';
  if (o.automatic === true) return 'service';
  return null;
}

/** The OS user behind an entry, when it is recorded. */
export function actorUser(e: Pick<AuditEntry, 'detail' | 'actor'>): string | null {
  const u = fields(e).user;
  if (typeof u === 'string' && u) return u;
  return (ROLES as readonly string[]).includes(e.actor) || e.actor === 'demo' ? null : e.actor;
}

const HIDDEN = new Set(['via', 'client', 'user']);

/** The detail object as "key: value" pairs, without the fields shown as the actor. */
export function detailText(e: Pick<AuditEntry, 'detail'>): string {
  const v = e.detail;
  if (!v || typeof v !== 'object' || Array.isArray(v)) return v === null || v === undefined ? '' : String(v);
  return Object.entries(v)
    .filter(([k]) => !HIDDEN.has(k))
    .map(([k, x]) => `${k}: ${Array.isArray(x) ? x.join(', ') : typeof x === 'object' && x !== null ? JSON.stringify(x) : String(x)}`)
    .join('; ');
}
