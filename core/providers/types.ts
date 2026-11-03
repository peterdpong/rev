import type { AgentApproval, AgentEvent, Provider } from '../types';

// The UI and review runner never handle provider wire messages or credentials.
export interface ProviderTurn {
  executable: string;
  cwd: string;
  mode: 'ask' | 'apply';
  prompt: string;
  sessionId?: string;
  signal: AbortSignal;
  emit(event: AgentEvent): void;
  saveSession(id: string): Promise<void>;
  approve(
    request: Omit<AgentApproval, 'id'>,
    signal?: AbortSignal,
  ): Promise<boolean>;
}

export interface ProviderAdapter {
  id: Provider;
  transport: string;
  run(turn: ProviderTurn): Promise<void>;
}
