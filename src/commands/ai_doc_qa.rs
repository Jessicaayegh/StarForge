//! AI Documentation Q&A commands (#512)
//!
//! `starforge ai-doc-qa` answers questions about StarForge, Stellar, and
//! Soroban documentation. Answers are grounded in an indexed knowledge base
//! (local docs + curated Stellar/Soroban/StarForge references), always cite
//! their sources, support follow-up sessions, and answer in multiple languages.

use crate::utils::{
    ai_doc_qa::{
        self, build_index, DocQaEngine, IndexOptions, QaLanguage, QaMessageRole, QaSessionStore,
    },
    print as p,
};
use anyhow::{Context, Result};
use clap::Subcommand;
use rustyline::Editor;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AiDocQaCommands {
    /// Build (or rebuild) the documentation index from local docs and the curated knowledge base
    Index {
        /// Additional directory containing documentation to index (repeatable)
        #[arg(long, short)]
        dir: Vec<PathBuf>,

        /// Skip seeding the curated Stellar/Soroban/StarForge knowledge base
        #[arg(long)]
        no_builtin: bool,
    },

    /// Ask a documentation question and get a cited, AI-generated answer
    Ask {
        /// The question (wrap in quotes)
        question: Vec<String>,

        /// Session ID for follow-up context (optional)
        #[arg(long)]
        session: Option<String>,

        /// Answer language (e.g. en, es, fr, de, zh, ja, ko, pt, ru, ar)
        #[arg(long)]
        language: Option<String>,

        /// Print the answer as JSON
        #[arg(long)]
        json: bool,

        /// Output only JSON citation objects for IDE integrations
        #[arg(long)]
        citations_only: bool,

        /// Strict citation verification mode
        #[arg(long)]
        citation_mode: bool,
    },

    /// Start an interactive chat session with follow-up support
    Chat {
        /// Session ID to resume (optional)
        #[arg(long)]
        session: Option<String>,

        /// Answer language (e.g. en, es, fr, de, zh, ja, ko, pt, ru, ar)
        #[arg(long)]
        language: Option<String>,
    },

    /// List documentation Q&A sessions
    Sessions,

    /// Show conversation history for a session
    SessionShow {
        /// Session ID
        session: String,
    },

    /// Delete a documentation Q&A session
    SessionDelete {
        /// Session ID
        session: String,
    },

    /// Show index statistics (chunks, sources, languages)
    Stats,

    /// List the supported answer languages
    Languages,

    /// Run a self-check against sample questions to verify accuracy
    Verify {
        /// Answer language for the self-check (e.g. es, fr, de)
        #[arg(long)]
        language: Option<String>,
    },
}

pub async fn handle(cmd: AiDocQaCommands) -> Result<()> {
    match cmd {
        AiDocQaCommands::Index { dir, no_builtin } => handle_index(&dir, no_builtin).await,
        AiDocQaCommands::Ask {
            question,
            session,
            language,
            json,
            citations_only,
            citation_mode,
        } => {
            handle_ask(
                &question.join(" "),
                session.as_deref(),
                language.as_deref(),
                json,
                citations_only,
                citation_mode,
            )
            .await
        }
        AiDocQaCommands::Chat { session, language } => {
            handle_chat(session.as_deref(), language.as_deref()).await
        }
        AiDocQaCommands::Sessions => handle_sessions().await,
        AiDocQaCommands::SessionShow { session } => handle_session_show(&session).await,
        AiDocQaCommands::SessionDelete { session } => handle_session_delete(&session).await,
        AiDocQaCommands::Stats => handle_stats().await,
        AiDocQaCommands::Languages => handle_languages().await,
        AiDocQaCommands::Verify { language } => handle_verify(language.as_deref()).await,
    }
}

fn parse_language(raw: Option<&str>) -> Option<QaLanguage> {
    raw.and_then(QaLanguage::parse)
}

fn build_engine(dir: &[PathBuf], no_builtin: bool) -> Result<DocQaEngine> {
    let options = IndexOptions {
        extra_dirs: dir.to_vec(),
        include_builtin: !no_builtin,
        ..IndexOptions::default()
    };
    let index = build_index(&options)?;
    Ok(DocQaEngine::new(index))
}

async fn handle_index(dir: &[PathBuf], no_builtin: bool) -> Result<()> {
    p::header("Indexing Documentation");
    p::separator();
    p::info(
        "Scanning StarForge docs, tutorials, and the curated Stellar/Soroban knowledge base...",
    );

    let engine = build_engine(dir, no_builtin)?;
    let stats = engine.index.stats();

    p::success(&format!(
        "Indexed {} chunks from {} sources.",
        stats.total_chunks, stats.total_sources
    ));

    let mut rows = Vec::new();
    for (kind, count) in stats.by_kind {
        rows.push(vec![kind, count.to_string()]);
    }
    p::table(&["Knowledge Base", "Chunks"], &rows);

    let mut lang_rows = Vec::new();
    for (lang, count) in stats.by_language {
        lang_rows.push(vec![lang, count.to_string()]);
    }
    p::table(&["Language", "Chunks"], &lang_rows);

    p::separator();
    p::info("The index is rebuilt in-memory on each invocation for freshness.");
    Ok(())
}

async fn handle_ask(
    question: &str,
    session_id: Option<&str>,
    language: Option<&str>,
    json: bool,
    citations_only: bool,
    citation_mode: bool,
) -> Result<()> {
    if question.trim().is_empty() {
        anyhow::bail!("Please provide a question, e.g. `starforge ai-doc-qa ask \"How do I deploy a contract?\"`");
    }

    let mut engine = build_engine(&[], false)?;
    let answer_language = parse_language(language);

    // Create a session if the user asked for follow-up context without one.
    let resolved_session = match session_id {
        Some(id) => Some(id.to_string()),
        None => {
            if answer_language.is_some() {
                let lang = answer_language.unwrap_or(QaLanguage::English);
                let session = engine.create_session(lang);
                Some(session.session_id)
            } else {
                None
            }
        }
    };

    let answer = engine
        .ask(
            question,
            resolved_session.as_deref(),
            answer_language.or(resolved_session
                .as_ref()
                .and_then(|id| engine.store.get(id).map(|s| s.preferred_language))),
        )
        .await?;

    if citations_only {
        println!("{}", serde_json::to_string_pretty(&answer.citations)?);
        return Ok(());
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&answer)?);
    } else {
        if answer.is_low_confidence {
            p::warn("Note: Low confidence score. This answer may not be fully grounded in documentation.");
        }
        if citation_mode && answer.citations.is_empty() {
            p::warn("Strict citation mode: No grounding sources were retrieved from the documentation index.");
        }
        p::header("Documentation Answer");
        p::separator();
        p::kv("Question", question);
        p::kv("Language", answer.language.display());
        p::kv(
            "Mode",
            match answer.mode {
                ai_doc_qa::AnswerMode::Generated => "AI-generated (grounded)",
                ai_doc_qa::AnswerMode::Extractive => "Extractive (LLM unavailable)",
            },
        );
        p::kv("Confidence", &format!("{:.0}%", answer.confidence * 100.0));
        p::separator();
        println!();
        println!("{}", answer.answer);
        println!();

        print_citations(&answer.citations);

        if let Some(id) = &resolved_session {
            p::separator();
            p::info(&format!("Follow-up session: {}", id));
        }
    }

    Ok(())
}

async fn handle_chat(session_id: Option<&str>, language: Option<&str>) -> Result<()> {
    let mut engine = build_engine(&[], false)?;
    let answer_language = parse_language(language);

    let session_id = match session_id {
        Some(id) => {
            engine
                .store
                .get(id)
                .context("Session not found — start a new chat or use `ai-doc-qa sessions`")?;
            id.to_string()
        }
        None => {
            engine
                .create_session(answer_language.unwrap_or(QaLanguage::English))
                .session_id
        }
    };

    p::header("AI Documentation Q&A Chat");
    p::separator();
    p::kv("Session", &session_id);
    p::info("Ask anything about StarForge, Stellar, or Soroban. Type 'exit' or 'quit' to end.");
    p::separator();
    println!();

    let mut rl: Editor<(), rustyline::history::DefaultHistory> = Editor::new()?;

    loop {
        let readline = rl.readline("Q: ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "exit" || line == "quit" {
                    p::info("Ending documentation session.");
                    break;
                }
                if line == "/session" {
                    p::kv("Session", &session_id);
                    continue;
                }

                p::info("AI: Thinking...");
                let answer = engine.ask(line, Some(&session_id), answer_language).await?;

                println!();
                println!("A: {}", answer.answer);
                println!();
                print_citations(&answer.citations);
                println!();
            }
            Err(_) => break,
        }
    }

    p::separator();
    p::info(&format!("Session saved: {}", session_id));
    p::info("Resume with: starforge ai-doc-qa chat --session <id>");
    Ok(())
}

async fn handle_sessions() -> Result<()> {
    p::header("Documentation Q&A Sessions");
    p::separator();

    let store = QaSessionStore::load();
    let sessions = store.list();
    if sessions.is_empty() {
        p::info("No sessions yet. Ask a question to create one.");
    } else {
        let mut rows = Vec::new();
        for session in sessions {
            rows.push(vec![
                session.session_id,
                session.preferred_language.display().to_string(),
                session.messages.len().to_string(),
                session.last_updated.format("%Y-%m-%d %H:%M").to_string(),
            ]);
        }
        p::table(
            &["Session ID", "Language", "Messages", "Last Updated"],
            &rows,
        );
    }
    p::separator();
    Ok(())
}

async fn handle_session_show(session_id: &str) -> Result<()> {
    p::header(&format!("Session History: {}", session_id));
    p::separator();

    let store = QaSessionStore::load();
    let session = store.get(session_id).context("Session not found")?;

    let messages: Vec<_> = session
        .messages
        .iter()
        .filter(|m| m.role != QaMessageRole::System)
        .collect();

    if messages.is_empty() {
        p::info("No messages in this session.");
    } else {
        for msg in messages {
            let role = match msg.role {
                QaMessageRole::User => "Q",
                QaMessageRole::Assistant => "A",
                QaMessageRole::System => "System",
            };
            println!("{}: {}", role, msg.content);
            println!();
        }
    }
    p::separator();
    Ok(())
}

async fn handle_session_delete(session_id: &str) -> Result<()> {
    p::header(&format!("Delete Session: {}", session_id));
    p::separator();

    let mut store = QaSessionStore::load();
    if store.delete(session_id) {
        store.save();
        p::success("Session deleted.");
    } else {
        anyhow::bail!("Session not found");
    }
    p::separator();
    Ok(())
}

async fn handle_stats() -> Result<()> {
    let engine = build_engine(&[], false)?;
    let stats = engine.index.stats();

    p::header("Documentation Index Statistics");
    p::separator();
    p::kv("Total chunks", &stats.total_chunks.to_string());
    p::kv("Total sources", &stats.total_sources.to_string());
    p::kv(
        "Built at",
        &stats.built_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    );
    p::separator();

    let mut rows = Vec::new();
    for (kind, count) in stats.by_kind {
        rows.push(vec![kind, count.to_string()]);
    }
    p::table(&["Knowledge Base", "Chunks"], &rows);

    let mut lang_rows = Vec::new();
    for (lang, count) in stats.by_language {
        lang_rows.push(vec![lang, count.to_string()]);
    }
    p::table(&["Language", "Chunks"], &lang_rows);
    p::separator();
    Ok(())
}

async fn handle_languages() -> Result<()> {
    p::header("Supported Answer Languages");
    p::separator();
    let mut rows = Vec::new();
    for lang in QaLanguage::all() {
        rows.push(vec![lang.as_str().to_string(), lang.display().to_string()]);
    }
    p::table(&["Code", "Language"], &rows);
    p::info("Pass --language <code> or let the question's language be detected automatically.");
    p::separator();
    Ok(())
}

async fn handle_verify(language: Option<&str>) -> Result<()> {
    p::header("AI Documentation Q&A Self-Check");
    p::separator();
    p::info("Running sample questions against the knowledge base...");

    let mut engine = build_engine(&[], false)?;
    let stats = engine.index.stats();
    p::kv("Indexed chunks", &stats.total_chunks.to_string());
    p::separator();

    let samples = [
        "How do I deploy a Soroban contract with StarForge?",
        "What is the Stellar native asset and what is it called?",
        "How does Soroban contract storage work?",
        "What is a trustline in Stellar?",
        "How does Soroban authentication work?",
    ];

    let answer_language = parse_language(language).unwrap_or(QaLanguage::English);
    let mut all_cited = true;
    let mut rows = Vec::new();

    for sample in samples {
        let answer = engine.ask(sample, None, Some(answer_language)).await?;
        let cited = !answer.citations.is_empty();
        all_cited &= cited;
        rows.push(vec![
            sample.chars().take(52).collect(),
            format!("{:.0}%", answer.confidence * 100.0),
            if cited { "yes" } else { "no" }.to_string(),
            match answer.mode {
                ai_doc_qa::AnswerMode::Generated => "generated",
                ai_doc_qa::AnswerMode::Extractive => "extractive",
            }
            .to_string(),
        ]);
    }

    p::table(&["Sample question", "Confidence", "Cited", "Mode"], &rows);
    p::separator();
    if all_cited {
        p::success("All sample answers include source citations.");
    } else {
        p::warn("Some sample answers are missing citations.");
    }
    Ok(())
}

fn print_citations(citations: &[ai_doc_qa::Citation]) {
    if citations.is_empty() {
        p::warn("No documentation sources were retrieved for this answer.");
        return;
    }
    println!("Sources:");
    for (i, c) in citations.iter().take(5).enumerate() {
        println!(
            "  [{i}] {} ({}) — {}",
            c.title,
            c.kind.as_str(),
            c.url.as_deref().unwrap_or(&c.source)
        );
    }
    println!();
}
