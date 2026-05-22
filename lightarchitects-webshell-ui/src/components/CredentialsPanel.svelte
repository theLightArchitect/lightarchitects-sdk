<script lang="ts">
  import CredentialRow from './CredentialRow.svelte';

  const providers = [
    {
      provider: 'google',
      label: 'Google (Cloud Platform)',
      flow: 'oauth' as const,
      prompt: '',
    },
    {
      provider: 'github',
      label: 'GitHub',
      flow: 'device' as const,
      prompt: '',
    },
    {
      provider: 'anthropic',
      label: 'Anthropic (Claude)',
      flow: 'apikey' as const,
      prompt: 'Paste your Anthropic API key (sk-ant-…)',
    },
    {
      provider: 'openai',
      label: 'OpenAI',
      flow: 'apikey' as const,
      prompt: 'Paste your OpenAI project key (sk-proj-…)',
    },
    {
      provider: 'mistral',
      label: 'Mistral',
      flow: 'apikey' as const,
      prompt: 'Paste your Mistral API key',
    },
    {
      provider: 'ollama',
      label: 'Ollama (local)',
      flow: 'cli' as const,
      prompt: '',
    },
  ] as const;
</script>

<div class="flex flex-col gap-1 py-2">
  <p class="text-[10px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider px-1 pb-2">
    Provider Credentials
  </p>
  <div class="flex flex-col divide-y divide-[var(--la-drawer-border)]/40">
    {#each providers as p (p.provider)}
      <CredentialRow
        provider={p.provider}
        label={p.label}
        flow={p.flow}
        prompt={p.prompt}
      />
    {/each}
  </div>
  <p class="text-[10px] text-[var(--la-text-dim)] px-1 pt-3 leading-relaxed">
    Credentials are stored in the macOS Keychain and never logged.
    Google and GitHub require browser authorization; Anthropic, OpenAI, and Mistral accept API keys directly.
  </p>
</div>
