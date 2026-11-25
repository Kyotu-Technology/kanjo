import type { ApiError } from './contract';

/** Codes from docs/api.md plus two the client produces itself. */
export type ErrorCode = 'unauthorized' | 'forbidden' | 'not_found' | 'invalid' | 'disabled' | 'internal' | 'network' | 'unknown';

const KNOWN: readonly ErrorCode[] = ['unauthorized', 'forbidden', 'not_found', 'invalid', 'disabled', 'internal'];

export class ApiRequestError extends Error {
  readonly status: number;
  readonly code: ErrorCode;

  constructor(status: number, code: ErrorCode, message: string) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = status;
    this.code = code;
  }

  get isUnauthorized(): boolean {
    return this.code === 'unauthorized';
  }
}

function isApiError(body: unknown): body is ApiError {
  return typeof body === 'object' && body !== null && typeof (body as ApiError).code === 'string' && typeof (body as ApiError).message === 'string';
}

/** Turns a non-2xx status and its parsed body (if any) into a typed error. */
export function errorFromResponse(status: number, body: unknown): ApiRequestError {
  if (isApiError(body)) {
    const code = (KNOWN as readonly string[]).includes(body.code) ? (body.code as ErrorCode) : 'unknown';
    return new ApiRequestError(status, code, body.message);
  }
  const fallback: ErrorCode = status === 401 ? 'unauthorized' : status === 403 ? 'forbidden' : status === 404 ? 'not_found' : status >= 500 ? 'internal' : 'unknown';
  return new ApiRequestError(status, fallback, `HTTP ${status}`);
}

export function networkError(cause: unknown): ApiRequestError {
  const message = cause instanceof Error ? cause.message : String(cause);
  return new ApiRequestError(0, 'network', message);
}

export function asApiError(e: unknown): ApiRequestError {
  return e instanceof ApiRequestError ? e : networkError(e);
}
