export class PromptScrubber {
  allowlist: string[] = [];
  scrubSecrets(prompt: string) { return prompt; }
}
