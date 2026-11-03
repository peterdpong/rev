export type Provider = 'codex' | 'claude';

export type AgentEvent = {
  kind: 'message' | 'activity' | 'error';
  text: string;
  id?: string;
};

export type AgentApproval = { id: string; title: string; detail: string };
