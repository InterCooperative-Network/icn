/**
 * React mock for testing hooks outside React Native environment
 */

// Simple mock state implementation
let stateIndex = 0;
const states: Map<number, unknown> = new Map();

export function useState<T>(initial: T): [T, (value: T | ((prev: T) => T)) => void] {
  const index = stateIndex++;
  if (!states.has(index)) {
    states.set(index, initial);
  }
  const setState = (value: T | ((prev: T) => T)) => {
    const current = states.get(index) as T;
    const newValue = typeof value === 'function' ? (value as (prev: T) => T)(current) : value;
    states.set(index, newValue);
  };
  return [states.get(index) as T, setState];
}

export function useEffect(effect: () => void | (() => void), deps?: unknown[]): void {
  // In tests, we just run the effect synchronously
  const cleanup = effect();
  // Store cleanup for potential later use
  if (cleanup) {
    // Could track cleanups if needed
  }
}

export function useCallback<T extends (...args: unknown[]) => unknown>(
  callback: T,
  deps: unknown[]
): T {
  return callback;
}

export function useRef<T>(initial: T): { current: T } {
  return { current: initial };
}

export function useMemo<T>(factory: () => T, deps: unknown[]): T {
  return factory();
}

// Reset state between tests
export function __resetMockState(): void {
  stateIndex = 0;
  states.clear();
}

export default {
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
};
