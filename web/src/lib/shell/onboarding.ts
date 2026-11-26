/** What the Connect step of the first-run guide offers: a demo server refuses to change Claude Code's settings, so it only explains. */
export type ConnectState = 'demo' | 'connected' | 'open';

export function connectState(demo: boolean, hooksOn: boolean): ConnectState {
  if (demo) return 'demo';
  return hooksOn ? 'connected' : 'open';
}
