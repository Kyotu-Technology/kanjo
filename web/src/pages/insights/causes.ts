const NAMES: Record<string, string> = {
  ttl_expired_5m: 'ttl_expired_5m',
  ttl_expired_1h: 'ttl_expired_1h',
  ttl_expired: 'ttl_expired',
  expired: 'ttl_expired',
  cache_expired: 'ttl_expired',
  tools_changed: 'tools_changed',
  system_prompt_changed: 'system_prompt_changed',
  system_changed: 'system_prompt_changed',
  model_changed: 'model_changed',
  claude_md_changed: 'claude_md_changed',
};

/** Dictionary leaf under pages.insights.cache_box.cause for a cause code, or null for an unknown code. */
export function causeKey(cause: string): string | null {
  return NAMES[cause] ?? null;
}

/** Dictionary leaf under pages.insights.cache_box.tip for a cause code. */
export function causeTip(cause: string): 'ttl' | 'tools' | 'system' | 'model' | null {
  const k = causeKey(cause);
  if (!k) return null;
  if (k.startsWith('ttl')) return 'ttl';
  if (k === 'tools_changed') return 'tools';
  if (k === 'model_changed') return 'model';
  return 'system';
}
