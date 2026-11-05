// Hermes has no crypto.getRandomValues; the Noise handshake's ephemeral keys need it.
// Set it up before anything imports @noble, then hand over to Expo Router.
import * as Crypto from 'expo-crypto';

const g = globalThis as { crypto?: { getRandomValues?: unknown } };
if (typeof g.crypto?.getRandomValues !== 'function') {
  g.crypto = { ...(g.crypto ?? {}), getRandomValues: Crypto.getRandomValues };
}

// Benchmark builds only; inlined to false otherwise, so the bench and its samples are dropped.
if (process.env.EXPO_PUBLIC_MD_BENCH === '1') require('./bench/app');
else require('expo-router/entry');
