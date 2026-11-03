import type { Provider } from '../types';
import type { ProviderAdapter } from './types';
import { codex } from './codex';
import { claude } from './claude';
import { executable } from './executable';

const adapters: Record<Provider, ProviderAdapter> = { codex, claude };

export function providerAdapter(id: Provider) {
  if (!Object.hasOwn(adapters, id)) {
    throw new Error('Choose Codex or Claude Code.');
  }
  return adapters[id];
}

export async function providers() {
  return Promise.all(
    Object.values(adapters).map(async (adapter) => ({
      id: adapter.id,
      transport: adapter.transport,
      available: await executable(adapter.id).then(
        () => true,
        () => false,
      ),
      capabilities: { resume: true, approvals: true, streaming: true },
    })),
  );
}

export { executable };
