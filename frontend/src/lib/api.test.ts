import { describe, it, expect } from 'vitest';
import { ApiError, parseApiError, formatApiError } from './api';

describe('ApiError', () => {
  it('has correct name and fields', () => {
    const err = new ApiError('not found', 404, 'NOT_FOUND');
    expect(err.name).toBe('ApiError');
    expect(err.message).toBe('not found');
    expect(err.status).toBe(404);
    expect(err.code).toBe('NOT_FOUND');
    expect(err).toBeInstanceOf(Error);
  });
});

describe('parseApiError', () => {
  it('extracts message from JSON body', async () => {
    const res = { status: 400, statusText: 'Bad Request', ok: false } as Response;
    const body = JSON.stringify({ status: 'error', message: 'Ship not found' });
    const err = await parseApiError(res, body);
    expect(err.message).toBe('Ship not found');
    expect(err.status).toBe(400);
    expect(err.code).toBe('VALIDATION');
  });

  it('falls back to body text when JSON has no message', async () => {
    const res = { status: 500, statusText: 'Internal Server Error', ok: false } as Response;
    const body = 'plain error text';
    const err = await parseApiError(res, body);
    expect(err.message).toBe('plain error text');
    expect(err.code).toBe('SERVER_ERROR');
  });

  it('falls back to statusText when body is empty', async () => {
    const res = { status: 404, statusText: 'Not Found', ok: false } as Response;
    const err = await parseApiError(res, '');
    expect(err.message).toBe('Not Found');
    expect(err.code).toBe('NOT_FOUND');
  });

  it('maps 401 to AUTH code', async () => {
    const res = { status: 401, statusText: 'Unauthorized', ok: false } as Response;
    const err = await parseApiError(res, '');
    expect(err.code).toBe('AUTH');
  });
});

describe('formatApiError', () => {
  it('adds retry hint for server errors', () => {
    const err = new ApiError('Internal error', 500, 'SERVER_ERROR');
    expect(formatApiError(err)).toBe('Internal error Try again later.');
  });

  it('returns plain message for non-server errors', () => {
    const err = new ApiError('Bad input', 400, 'VALIDATION');
    expect(formatApiError(err)).toBe('Bad input');
  });

  it('handles non-Error values', () => {
    expect(formatApiError('some string')).toBe('some string');
    expect(formatApiError(42)).toBe('42');
  });

  it('handles generic Error', () => {
    expect(formatApiError(new Error('generic'))).toBe('generic');
  });
});
