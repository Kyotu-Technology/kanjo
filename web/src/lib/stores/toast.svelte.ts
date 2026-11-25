export type ToastKind = 'info' | 'success' | 'warning' | 'error';

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  action?: { label: string; run: () => void };
}

class Toasts {
  items = $state<Toast[]>([]);
  private seq = 0;

  push(kind: ToastKind, message: string, opts: { action?: Toast['action']; timeoutMs?: number } = {}): number {
    const id = ++this.seq;
    this.items = [...this.items.slice(-3), { id, kind, message, action: opts.action }];
    const ms = opts.timeoutMs ?? (kind === 'error' ? 9000 : 5000);
    if (ms > 0) setTimeout(() => this.dismiss(id), ms);
    return id;
  }

  dismiss(id: number): void {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toasts = new Toasts();
