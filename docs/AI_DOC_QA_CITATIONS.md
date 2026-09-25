# AI Documentation Q&A: Citations and Grounding

StarForge provides grounded documentation Q&A via `starforge ai-doc-qa ask`. Answers are grounded in the local documentation repository and curated Stellar / Soroban references, with source citations linking directly to file paths, section anchors, and confidence scores.

## Citation Schema

When requesting answers or citations in JSON format (`--json` or `--citations-only`), citations conform to the following schema:

```json
{
  "source": "docs/DEPLOY_POLICY.md",
  "title": "## Checkpoints",
  "url": "https://nanle-code.github.io/StarForge/docs/DEPLOYMENT_CHECKPOINTS.html",
  "kind": "StarForge",
  "snippet": "Checkpoints allow resuming interrupted deployments safely.",
  "file_path": "docs/DEPLOY_POLICY.md",
  "section_anchor": "#checkpoints",
  "confidence_score": 0.85
}
```

### Fields

- `source`: The raw source identifier or display name.
- `title`: Header or title of the section retrieved.
- `url`: Online canonical documentation URL when available.
- `kind`: Source repository type (`StarForge`, `Stellar`, `SorobanSdk`, `Community`, `BestPractices`).
- `snippet`: Extracted text snippet used to ground the LLM prompt.
- `file_path`: Local workspace-relative file path (e.g., `docs/CONFIGURATION.md`).
- `section_anchor`: Markdown section slug (e.g., `#dry-run-mode`).
- `confidence_score`: Normalized relevance score between `0.0` and `1.0`.

---

## CLI Usage

### Ask with Grounding Citations

```bash
starforge ai-doc-qa ask "How do I configure deployment checkpoints?"
```

### IDE Integration Mode (`--citations-only`)

For IDE extensions, language servers, or CI tooling requiring structured citation objects:

```bash
starforge ai-doc-qa ask "How do I configure deployment checkpoints?" --citations-only
```

### Full JSON Answer (`--json`)

```bash
starforge ai-doc-qa ask "How do I configure deployment checkpoints?" --json
```

### Strict Citation Mode (`--citation-mode`)

```bash
starforge ai-doc-qa ask "What is the fee policy?" --citation-mode
```

---

## Confidence Grading & Low-Confidence Fallback

- Answers with confidence `< 60%` or with empty retrieved corpora are marked with `is_low_confidence: true`.
- In the CLI, low-confidence answers display a warning note.
- If no matching documentation exists, the engine degrades gracefully with suggestions to index custom directories using `starforge ai-doc-qa index --dir <path>`.
