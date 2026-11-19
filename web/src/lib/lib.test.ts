import { describe, expect, it } from 'vitest';
import { HEAT_LUT, heat, unit } from './colormap';
import { backoffMs, streamUrl } from './connection.svelte';
import { duration } from './format';
import { parseMsg } from './protocol';

describe('colormap', () => {
  it('clamps the display range', () => {
    expect(unit(-200)).toBe(0);
    expect(unit(0)).toBe(1);
    expect(unit(-77.5)).toBeCloseTo(0.5);
  });

  it('runs from dark to bright', () => {
    const lum = ([r, g, b]: number[]) => r + g + b;
    expect(lum(heat(0))).toBeLessThan(lum(heat(0.5)));
    expect(lum(heat(0.5))).toBeLessThan(lum(heat(1)));
    expect(HEAT_LUT.length).toBe(768);
  });
});

describe('connection', () => {
  it('derives the stream url from the page', () => {
    expect(streamUrl({ protocol: 'http:', host: 'node.local:8787', search: '' })).toBe('ws://node.local:8787/ws');
    expect(streamUrl({ protocol: 'https:', host: 'x', search: '' })).toBe('wss://x/ws');
    expect(streamUrl({ protocol: 'http:', host: 'x', search: '?ws=ws://10.0.0.5:8787/ws' })).toBe('ws://10.0.0.5:8787/ws');
  });

  it('backs off up to 8 s', () => {
    expect(backoffMs(0)).toBe(500);
    expect(backoffMs(3)).toBe(4000);
    expect(backoffMs(10)).toBe(8000);
  });
});

describe('protocol', () => {
  it('accepts known messages only', () => {
    expect(parseMsg('{"type":"frame","seq":1}')?.type).toBe('frame');
    expect(parseMsg('{"type":"other"}')).toBeNull();
    expect(parseMsg('not json')).toBeNull();
  });
});

describe('format', () => {
  it('formats durations', () => {
    expect(duration(4200)).toBe('4 s');
    expect(duration(125_000)).toBe('2 min 05 s');
  });
});
