use crate::utils::config;
use crate::utils::stream::SorobanEvent;
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventRoute {
    pub name: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Default)]
pub struct EventRouter {
    routes: Vec<EventRoute>,
}

impl EventRouter {
    pub fn new(routes: Vec<EventRoute>) -> Self {
        Self { routes }
    }

    pub fn from_specs(specs: &[String]) -> Result<Self> {
        let mut routes = Vec::new();
        for spec in specs {
            routes.push(parse_route(spec)?);
        }
        Ok(Self::new(routes))
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub fn route(&self, event: &SorobanEvent) -> Vec<String> {
        self.routes
            .iter()
            .filter(|route| event_matches_pattern(event, &route.pattern))
            .map(|route| route.name.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventAlertRule {
    pub id: String,
    pub severity: String,
    pub pattern: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventAlert {
    pub rule_id: String,
    pub severity: String,
    pub message: String,
}

/// Alert rule that fires when a pattern matches at least `count` events within
/// a sliding window of `window_ledgers` ledgers (e.g. a burst of transfers).
///
/// After firing, the rule stays quiet for one full window so a sustained burst
/// produces one alert per window instead of one per event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RateAlertRule {
    pub id: String,
    pub severity: String,
    pub pattern: String,
    pub count: usize,
    pub window_ledgers: u32,
    pub message: String,
    #[serde(skip)]
    matches: VecDeque<u32>,
    #[serde(skip)]
    last_fired: Option<u32>,
}

impl RateAlertRule {
    fn observe(&mut self, event: &SorobanEvent) -> Option<EventAlert> {
        if !event_matches_pattern(event, &self.pattern) {
            return None;
        }

        let ledger = event.ledger;
        self.matches.push_back(ledger);
        let window_start = ledger.saturating_sub(self.window_ledgers.saturating_sub(1));
        while self
            .matches
            .front()
            .is_some_and(|first| *first < window_start)
        {
            self.matches.pop_front();
        }

        let cooling_down = self
            .last_fired
            .is_some_and(|fired| ledger < fired.saturating_add(self.window_ledgers));
        if self.matches.len() < self.count || cooling_down {
            return None;
        }

        self.last_fired = Some(ledger);
        Some(EventAlert {
            rule_id: self.id.clone(),
            severity: self.severity.clone(),
            message: format!(
                "{} ({} matching events within {} ledgers)",
                self.message,
                self.matches.len(),
                self.window_ledgers
            ),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct AlertEngine {
    rules: Vec<EventAlertRule>,
    rate_rules: Vec<RateAlertRule>,
}

impl AlertEngine {
    pub fn new(rules: Vec<EventAlertRule>) -> Self {
        Self {
            rules,
            rate_rules: Vec::new(),
        }
    }

    pub fn from_specs(specs: &[String]) -> Result<Self> {
        Self::from_all_specs(specs, &[])
    }

    /// Build an engine from per-event alert specs (`--alert`) and rate-based
    /// alert specs (`--alert-rate`).
    pub fn from_all_specs(specs: &[String], rate_specs: &[String]) -> Result<Self> {
        let mut rules = Vec::new();
        for (index, spec) in specs.iter().enumerate() {
            rules.push(parse_alert_rule(spec, index)?);
        }
        let mut rate_rules = Vec::new();
        for (index, spec) in rate_specs.iter().enumerate() {
            rate_rules.push(parse_rate_alert_rule(spec, index)?);
        }
        Ok(Self { rules, rate_rules })
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len() + self.rate_rules.len()
    }

    /// Evaluate an event against every rule. Rate rules keep sliding-window
    /// state, so events must be fed in ledger order.
    pub fn evaluate(&mut self, event: &SorobanEvent) -> Vec<EventAlert> {
        let mut alerts: Vec<EventAlert> = self
            .rules
            .iter()
            .filter(|rule| event_matches_pattern(event, &rule.pattern))
            .map(|rule| EventAlert {
                rule_id: rule.id.clone(),
                severity: rule.severity.clone(),
                message: rule.message.clone(),
            })
            .collect();
        alerts.extend(
            self.rate_rules
                .iter_mut()
                .filter_map(|rule| rule.observe(event)),
        );
        alerts
    }
}

/// Rank of a severity label, used to compare against a minimum threshold.
pub fn severity_rank(severity: &str) -> u8 {
    match severity.to_lowercase().as_str() {
        "info" => 0,
        "low" => 1,
        "medium" => 2,
        "high" => 3,
        "critical" => 4,
        _ => 0,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventTrigger {
    pub pattern: String,
    pub command: String,
}

impl EventTrigger {
    pub fn from_specs(specs: &[String]) -> Result<Vec<Self>> {
        specs.iter().map(|spec| parse_trigger(spec)).collect()
    }

    pub fn matches(&self, event: &SorobanEvent) -> bool {
        event_matches_pattern(event, &self.pattern)
    }

    pub fn execute(&self, network: &str, contract_id: &str, event: &SorobanEvent) -> Result<()> {
        let topic = event.topic.join(",");
        let value = event.value.to_string();

        #[cfg(target_os = "windows")]
        let mut command = {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", &self.command]);
            cmd
        };

        #[cfg(not(target_os = "windows"))]
        let mut command = {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", &self.command]);
            cmd
        };

        let status = command
            .env("STARFORGE_NETWORK", network)
            .env("STARFORGE_CONTRACT_ID", contract_id)
            .env("STARFORGE_EVENT_ID", &event.id)
            .env("STARFORGE_EVENT_LEDGER", event.ledger.to_string())
            .env("STARFORGE_EVENT_TYPE", &event.event_type)
            .env("STARFORGE_EVENT_TOPIC", topic)
            .env("STARFORGE_EVENT_VALUE", value)
            .status()
            .with_context(|| format!("failed to execute trigger command '{}'", self.command))?;

        if !status.success() {
            anyhow::bail!(
                "trigger command '{}' exited with status {}",
                self.command,
                status
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedEvent {
    pub observed_at: String,
    pub network: String,
    pub contract_id: String,
    pub routes: Vec<String>,
    pub alerts: Vec<EventAlert>,
    pub event: SorobanEvent,
}

impl PersistedEvent {
    pub fn new(
        network: &str,
        contract_id: &str,
        event: SorobanEvent,
        routes: Vec<String>,
        alerts: Vec<EventAlert>,
    ) -> Self {
        Self {
            observed_at: Utc::now().to_rfc3339(),
            network: network.to_string(),
            contract_id: contract_id.to_string(),
            routes,
            alerts,
            event,
        }
    }

    pub fn identity(&self) -> String {
        format!("{}:{}:{}", self.network, self.contract_id, self.event.id)
    }
}

#[derive(Debug, Clone)]
pub struct EventStore {
    path: PathBuf,
}

impl EventStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path(network: &str, contract_id: &str) -> Result<PathBuf> {
        let dir = config::config_dir().join("events");
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create event store directory {}", dir.display()))?;
        Ok(dir.join(format!(
            "{}-{}.jsonl",
            sanitize_component(network),
            sanitize_component(contract_id)
        )))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn persist(&self, event: &PersistedEvent) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create event store directory {}",
                    parent.display()
                )
            })?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open event store {}", self.path.display()))?;
        serde_json::to_writer(&mut file, event)
            .with_context(|| format!("failed to serialize event into {}", self.path.display()))?;
        file.write_all(b"\n")?;
        Ok(())
    }

    pub fn replay(&self) -> Result<Vec<PersistedEvent>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&self.path)
            .with_context(|| format!("failed to open replay file {}", self.path.display()))?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        let mut identities = HashSet::new();
        for (index, line) in reader.lines().enumerate() {
            let line = line.with_context(|| {
                format!(
                    "failed to read line {} from {}",
                    index + 1,
                    self.path.display()
                )
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let event: PersistedEvent = match serde_json::from_str(trimmed) {
                Ok(event) => event,
                Err(error) => {
                    eprintln!(
                        "warning: skipping corrupt persisted event on line {} of {}: {}",
                        index + 1,
                        self.path.display(),
                        error
                    );
                    continue;
                }
            };
            if identities.insert(event.identity()) {
                events.push(event);
            }
        }

        Ok(events)
    }

    /// Replay persisted events restricted to an inclusive ledger range, in
    /// ledger order so rate-based alert rules see a consistent timeline.
    pub fn replay_range(
        &self,
        from_ledger: Option<u32>,
        to_ledger: Option<u32>,
    ) -> Result<Vec<PersistedEvent>> {
        if let (Some(from), Some(to)) = (from_ledger, to_ledger) {
            if from > to {
                anyhow::bail!(
                    "invalid replay range: --from-ledger {} is after --to-ledger {}",
                    from,
                    to
                );
            }
        }
        let mut events: Vec<PersistedEvent> = self
            .replay()?
            .into_iter()
            .filter(|event| from_ledger.map_or(true, |from| event.event.ledger >= from))
            .filter(|event| to_ledger.map_or(true, |to| event.event.ledger <= to))
            .collect();
        events.sort_by_key(|event| event.event.ledger);
        Ok(events)
    }
}

#[derive(Debug, Clone, Default)]
pub struct EventAnalytics {
    pub total_events: usize,
    pub alert_count: usize,
    pub first_ledger: Option<u32>,
    pub last_ledger: Option<u32>,
    pub by_type: HashMap<String, usize>,
    pub by_route: HashMap<String, usize>,
    pub by_alert_severity: HashMap<String, usize>,
    pub by_topic: HashMap<String, usize>,
    recent: Vec<String>,
}

impl EventAnalytics {
    pub fn record(&mut self, event: &PersistedEvent) {
        self.total_events += 1;
        self.alert_count += event.alerts.len();
        self.first_ledger = Some(
            self.first_ledger
                .map(|ledger| ledger.min(event.event.ledger))
                .unwrap_or(event.event.ledger),
        );
        self.last_ledger = Some(
            self.last_ledger
                .map(|ledger| ledger.max(event.event.ledger))
                .unwrap_or(event.event.ledger),
        );
        *self
            .by_type
            .entry(event.event.event_type.clone())
            .or_insert(0) += 1;

        for route in &event.routes {
            *self.by_route.entry(route.clone()).or_insert(0) += 1;
        }
        if let Some(topic) = event.event.topic.first() {
            *self.by_topic.entry(topic.clone()).or_insert(0) += 1;
        }
        for alert in &event.alerts {
            *self
                .by_alert_severity
                .entry(alert.severity.clone())
                .or_insert(0) += 1;
        }

        self.recent.push(format!(
            "ledger={} id={} type={} alerts={} routes={}",
            event.event.ledger,
            event.event.id,
            event.event.event_type,
            event.alerts.len(),
            if event.routes.is_empty() {
                "-".to_string()
            } else {
                event.routes.join(",")
            }
        ));
        if self.recent.len() > 10 {
            let excess = self.recent.len() - 10;
            self.recent.drain(0..excess);
        }
    }

    pub fn from_events(events: &[PersistedEvent]) -> Self {
        let mut analytics = Self::default();
        for event in events {
            analytics.record(event);
        }
        analytics
    }

    /// Average number of matching events per ledger across the observed range.
    pub fn events_per_ledger(&self) -> Option<f64> {
        match (self.first_ledger, self.last_ledger) {
            (Some(first), Some(last)) => {
                Some(self.total_events as f64 / f64::from(last - first + 1))
            }
            _ => None,
        }
    }

    /// Most frequent leading topics, highest count first (ties by name).
    pub fn top_topics(&self, limit: usize) -> Vec<(String, usize)> {
        let mut items: Vec<(String, usize)> = self
            .by_topic
            .iter()
            .map(|(topic, count)| (topic.clone(), *count))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        items.truncate(limit);
        items
    }

    pub fn render_dashboard(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "\nEvent Analytics Dashboard");
        let _ = writeln!(out, "-------------------------");
        let _ = writeln!(out, "Total events : {}", self.total_events);
        let _ = writeln!(out, "Alerts fired : {}", self.alert_count);
        let ledger_range = match (self.first_ledger, self.last_ledger) {
            (Some(first), Some(last)) => format!("{}..{}", first, last),
            _ => "n/a".to_string(),
        };
        let _ = writeln!(out, "Ledger range : {}", ledger_range);
        if let Some(rate) = self.events_per_ledger() {
            let _ = writeln!(out, "Event rate   : {:.2} events/ledger", rate);
        }
        let _ = writeln!(out, "Top topics:");
        let top = self.top_topics(5);
        if top.is_empty() {
            let _ = writeln!(out, "  - none");
        }
        for (topic, count) in top {
            let _ = writeln!(out, "  - {}: {}", topic, count);
        }
        write_counts(&mut out, "By type", &self.by_type);
        write_counts(&mut out, "By route", &self.by_route);
        write_counts(&mut out, "Alerts by severity", &self.by_alert_severity);
        let _ = writeln!(out, "Recent events:");
        if self.recent.is_empty() {
            let _ = writeln!(out, "  - none");
        } else {
            for event in &self.recent {
                let _ = writeln!(out, "  - {}", event);
            }
        }
        out
    }
}

/// Match an event against a pattern expression.
///
/// Grammar (case-insensitive):
/// - `*` or empty matches everything
/// - `text` matches a substring anywhere in the event (type, ledger, id, topics, value)
/// - `field~text` scopes the match to `type`, `topic`, `value`, or `id` (unknown fields fall back to a plain substring match)
/// - `ledger>N`, `ledger>=N`, `ledger<N`, `ledger<=N`, `ledger=N` compare the ledger number
/// - `!term` negates a term
/// - `a&b` requires every term; `a|b` accepts any alternative (`&` binds tighter)
pub fn event_matches_pattern(event: &SorobanEvent, pattern: &str) -> bool {
    let pattern = pattern.trim().to_lowercase();
    if pattern.is_empty() || pattern == "*" {
        return true;
    }
    pattern.split('|').any(|alternative| {
        let terms: Vec<&str> = alternative
            .split('&')
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .collect();
        !terms.is_empty() && terms.iter().all(|term| term_matches(event, term))
    })
}

fn term_matches(event: &SorobanEvent, term: &str) -> bool {
    if let Some(negated) = term.strip_prefix('!') {
        return !term_matches(event, negated.trim());
    }
    if let Some(rest) = term.strip_prefix("ledger") {
        if let Some(matched) = compare_ledger(event.ledger, rest.trim()) {
            return matched;
        }
    }
    if let Some((field, needle)) = term.split_once('~') {
        let needle = needle.trim();
        return match field.trim() {
            "type" => event.event_type.to_lowercase().contains(needle),
            "topic" => event
                .topic
                .iter()
                .any(|topic| topic.to_lowercase().contains(needle)),
            "value" => event.value.to_string().to_lowercase().contains(needle),
            "id" => event.id.to_lowercase().contains(needle),
            _ => event_search_text(event).to_lowercase().contains(term),
        };
    }
    event_search_text(event).to_lowercase().contains(term)
}

fn compare_ledger(ledger: u32, expr: &str) -> Option<bool> {
    let (op, number) = [">=", "<=", ">", "<", "="]
        .iter()
        .find_map(|op| expr.strip_prefix(op).map(|rest| (*op, rest.trim())))?;
    let number: u32 = number.parse().ok()?;
    Some(match op {
        ">=" => ledger >= number,
        "<=" => ledger <= number,
        ">" => ledger > number,
        "<" => ledger < number,
        _ => ledger == number,
    })
}

pub fn event_search_text(event: &SorobanEvent) -> String {
    format!(
        "{} {} {} {} {}",
        event.event_type,
        event.ledger,
        event.id,
        event.topic.join(" "),
        event.value
    )
}

fn parse_route(spec: &str) -> Result<EventRoute> {
    let (name, pattern) = spec.split_once('=').ok_or_else(|| {
        anyhow::anyhow!(
            "invalid route '{}'; expected name=pattern (example: swaps=swap)",
            spec
        )
    })?;
    let name = name.trim();
    let pattern = pattern.trim();
    if name.is_empty() || pattern.is_empty() {
        anyhow::bail!("invalid route '{}'; name and pattern cannot be empty", spec);
    }
    Ok(EventRoute {
        name: name.to_string(),
        pattern: pattern.to_string(),
    })
}

fn parse_alert_rule(spec: &str, index: usize) -> Result<EventAlertRule> {
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    let (severity, pattern, message) = match parts.as_slice() {
        [pattern] => ("high", pattern.trim(), None),
        [severity, pattern] if is_severity(severity.trim()) => {
            (severity.trim(), pattern.trim(), None)
        }
        [severity, pattern, message] if is_severity(severity.trim()) => {
            (severity.trim(), pattern.trim(), Some(message.trim()))
        }
        _ => ("high", spec.trim(), None),
    };

    if pattern.is_empty() {
        anyhow::bail!("invalid alert rule '{}'; pattern cannot be empty", spec);
    }

    Ok(EventAlertRule {
        id: format!("alert-{}", index + 1),
        severity: severity.to_lowercase(),
        pattern: pattern.to_string(),
        message: message
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("matched event pattern '{}'", pattern)),
    })
}

/// Parse `[severity:]pattern:COUNT/LEDGERS[:message]`, for example
/// `critical:topic~transfer:5/10:transfer burst`.
fn parse_rate_alert_rule(spec: &str, index: usize) -> Result<RateAlertRule> {
    let usage = || {
        anyhow::anyhow!(
            "invalid rate alert '{}'; expected [severity:]pattern:COUNT/LEDGERS[:message] \
             (example: critical:transfer:5/10:transfer burst)",
            spec
        )
    };
    let parts: Vec<&str> = spec.split(':').map(str::trim).collect();
    let rate_index = parts
        .iter()
        .position(|part| parse_rate(part).is_some())
        .ok_or_else(usage)?;
    let (count, window_ledgers) = parse_rate(parts[rate_index]).ok_or_else(usage)?;

    let (severity, pattern) = match &parts[..rate_index] {
        [pattern] => ("high", *pattern),
        [severity, pattern] if is_severity(severity) => (*severity, *pattern),
        _ => return Err(usage()),
    };
    if pattern.is_empty() {
        return Err(usage());
    }
    let message = parts[rate_index + 1..].join(":");
    let message = if message.trim().is_empty() {
        format!("event pattern '{}' exceeded rate threshold", pattern)
    } else {
        message.trim().to_string()
    };

    Ok(RateAlertRule {
        id: format!("rate-alert-{}", index + 1),
        severity: severity.to_lowercase(),
        pattern: pattern.to_string(),
        count,
        window_ledgers,
        message,
        matches: VecDeque::new(),
        last_fired: None,
    })
}

fn parse_rate(value: &str) -> Option<(usize, u32)> {
    let (count, window) = value.split_once('/')?;
    let count: usize = count.trim().parse().ok()?;
    let window: u32 = window.trim().parse().ok()?;
    (count > 0 && window > 0).then_some((count, window))
}

fn parse_trigger(spec: &str) -> Result<EventTrigger> {
    let (pattern, command) = spec.split_once('=').ok_or_else(|| {
        anyhow::anyhow!(
            "invalid trigger '{}'; expected pattern=command (example: mint=./on-mint.sh)",
            spec
        )
    })?;
    let pattern = pattern.trim();
    let command = command.trim();
    if pattern.is_empty() || command.is_empty() {
        anyhow::bail!(
            "invalid trigger '{}'; pattern and command cannot be empty",
            spec
        );
    }
    Ok(EventTrigger {
        pattern: pattern.to_string(),
        command: command.to_string(),
    })
}

fn is_severity(value: &str) -> bool {
    matches!(
        value.to_lowercase().as_str(),
        "info" | "low" | "medium" | "high" | "critical"
    )
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn write_counts(out: &mut String, title: &str, counts: &HashMap<String, usize>) {
    let _ = writeln!(out, "{}:", title);
    if counts.is_empty() {
        let _ = writeln!(out, "  - none");
        return;
    }

    let mut items: Vec<_> = counts.iter().collect();
    items.sort_by_key(|(left, _)| *left);
    for (key, count) in items {
        let _ = writeln!(out, "  - {}: {}", key, count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn sample_event() -> SorobanEvent {
        SorobanEvent {
            event_type: "contract".to_string(),
            ledger: 42,
            id: "0000000042-0000000001".to_string(),
            topic: vec!["swap".to_string(), "admin".to_string()],
            value: json!({ "amount": 100, "asset": "XLM" }),
        }
    }

    #[test]
    fn patterns_match_topics_and_values() {
        let event = sample_event();
        assert!(event_matches_pattern(&event, "swap"));
        assert!(event_matches_pattern(&event, "xlm"));
        assert!(!event_matches_pattern(&event, "missing"));
    }

    #[test]
    fn routes_are_parsed_and_applied() {
        let router = EventRouter::from_specs(&["dex=swap".to_string()]).unwrap();
        assert_eq!(router.route(&sample_event()), vec!["dex".to_string()]);
    }

    #[test]
    fn alerts_are_parsed_and_applied() {
        let mut engine =
            AlertEngine::from_specs(&["critical:admin:admin event".to_string()]).unwrap();
        let alerts = engine.evaluate(&sample_event());
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, "critical");
        assert_eq!(alerts[0].message, "admin event");
    }

    #[test]
    fn event_store_round_trips_persisted_events() {
        let dir = TempDir::new().unwrap();
        let store = EventStore::new(dir.path().join("events.jsonl"));
        let event = sample_event();
        let persisted = PersistedEvent::new(
            "testnet",
            "C123",
            event,
            vec!["dex".to_string()],
            vec![EventAlert {
                rule_id: "alert-1".to_string(),
                severity: "high".to_string(),
                message: "matched event pattern 'swap'".to_string(),
            }],
        );

        store.persist(&persisted).unwrap();

        let replayed = store.replay().unwrap();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].network, "testnet");
        assert_eq!(replayed[0].contract_id, "C123");
        assert_eq!(replayed[0].routes, vec!["dex".to_string()]);
        assert_eq!(replayed[0].alerts.len(), 1);
        assert_eq!(replayed[0].event.id, "0000000042-0000000001");
    }

    #[test]
    fn analytics_dashboard_includes_counts_and_recent_events() {
        let event = sample_event();
        let persisted = PersistedEvent::new(
            "testnet",
            "C123",
            event,
            vec!["dex".to_string()],
            vec![EventAlert {
                rule_id: "alert-1".to_string(),
                severity: "critical".to_string(),
                message: "admin event".to_string(),
            }],
        );

        let analytics = EventAnalytics::from_events(&[persisted]);
        let dashboard = analytics.render_dashboard();

        assert!(dashboard.contains("Event Analytics Dashboard"));
        assert!(dashboard.contains("Total events : 1"));
        assert!(dashboard.contains("Alerts fired : 1"));
        assert!(dashboard.contains("By route:"));
        assert!(dashboard.contains("Recent events:"));
    }

    #[test]
    fn replay_skips_corrupt_and_duplicate_records() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("events.jsonl");
        let store = EventStore::new(path.clone());
        let persisted =
            PersistedEvent::new("testnet", "C123", sample_event(), Vec::new(), Vec::new());
        store.persist(&persisted).unwrap();
        store.persist(&persisted).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"not-json\n")
            .unwrap();

        let replayed = store.replay().unwrap();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].identity(), "testnet:C123:0000000042-0000000001");
    }

    fn event_at(ledger: u32, topic: &str) -> SorobanEvent {
        SorobanEvent {
            event_type: "contract".to_string(),
            ledger,
            id: format!("{:010}-0000000001", ledger),
            topic: vec![topic.to_string()],
            value: json!({ "amount": 5 }),
        }
    }

    #[test]
    fn pattern_expressions_support_scopes_and_boolean_logic() {
        let event = sample_event();
        assert!(event_matches_pattern(&event, "topic~swap"));
        assert!(!event_matches_pattern(&event, "topic~xlm"));
        assert!(event_matches_pattern(&event, "value~xlm"));
        assert!(event_matches_pattern(&event, "type~contract & topic~admin"));
        assert!(!event_matches_pattern(&event, "topic~swap & !topic~admin"));
        assert!(event_matches_pattern(&event, "topic~mint | topic~swap"));
        assert!(event_matches_pattern(&event, "ledger>=42 & ledger<43"));
        assert!(!event_matches_pattern(&event, "ledger>42"));
    }

    #[test]
    fn rate_alert_spec_parsing() {
        let rule = parse_rate_alert_rule("critical:topic~transfer:3/10:transfer burst", 0).unwrap();
        assert_eq!(rule.severity, "critical");
        assert_eq!(rule.pattern, "topic~transfer");
        assert_eq!(rule.count, 3);
        assert_eq!(rule.window_ledgers, 10);
        assert_eq!(rule.message, "transfer burst");

        let rule = parse_rate_alert_rule("mint:2/5", 1).unwrap();
        assert_eq!(rule.severity, "high");
        assert_eq!(rule.id, "rate-alert-2");

        assert!(parse_rate_alert_rule("critical:transfer", 0).is_err());
        assert!(parse_rate_alert_rule("critical:transfer:0/5", 0).is_err());
    }

    #[test]
    fn rate_alerts_fire_on_bursts_and_respect_cooldown() {
        let mut engine =
            AlertEngine::from_all_specs(&[], &["critical:transfer:3/10:burst".to_string()])
                .unwrap();
        assert!(engine.evaluate(&event_at(100, "transfer")).is_empty());
        assert!(engine.evaluate(&event_at(101, "mint")).is_empty());
        assert!(engine.evaluate(&event_at(102, "transfer")).is_empty());
        let fired = engine.evaluate(&event_at(104, "transfer"));
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].severity, "critical");
        assert!(fired[0].message.starts_with("burst"));

        // Still inside the cooldown window: no duplicate alert.
        assert!(engine.evaluate(&event_at(105, "transfer")).is_empty());
        // Sparse matches outside the window do not accumulate.
        let mut sparse = AlertEngine::from_all_specs(&[], &["transfer:3/10".to_string()]).unwrap();
        for ledger in [100, 120, 140] {
            assert!(sparse.evaluate(&event_at(ledger, "transfer")).is_empty());
        }
    }

    #[test]
    fn replay_range_filters_and_orders_by_ledger() {
        let dir = TempDir::new().unwrap();
        let store = EventStore::new(dir.path().join("events.jsonl"));
        for ledger in [30, 10, 20] {
            let persisted = PersistedEvent::new(
                "testnet",
                "C123",
                event_at(ledger, "swap"),
                Vec::new(),
                Vec::new(),
            );
            store.persist(&persisted).unwrap();
        }

        let all = store.replay_range(None, None).unwrap();
        let ledgers: Vec<u32> = all.iter().map(|e| e.event.ledger).collect();
        assert_eq!(ledgers, vec![10, 20, 30]);

        let ranged = store.replay_range(Some(15), Some(30)).unwrap();
        let ledgers: Vec<u32> = ranged.iter().map(|e| e.event.ledger).collect();
        assert_eq!(ledgers, vec![20, 30]);

        assert!(store.replay_range(Some(40), Some(10)).is_err());
    }

    #[test]
    fn dashboard_reports_rate_and_top_topics() {
        let events: Vec<PersistedEvent> = [(10, "swap"), (11, "swap"), (12, "mint")]
            .iter()
            .map(|(ledger, topic)| {
                PersistedEvent::new(
                    "testnet",
                    "C123",
                    event_at(*ledger, topic),
                    Vec::new(),
                    Vec::new(),
                )
            })
            .collect();
        let analytics = EventAnalytics::from_events(&events);
        assert_eq!(analytics.events_per_ledger(), Some(1.0));
        assert_eq!(analytics.top_topics(1), vec![("swap".to_string(), 2usize)]);
        let dashboard = analytics.render_dashboard();
        assert!(dashboard.contains("Event rate   : 1.00 events/ledger"));
        assert!(dashboard.contains("Top topics:"));
    }

    #[test]
    fn severity_rank_orders_levels() {
        assert!(severity_rank("critical") > severity_rank("high"));
        assert!(severity_rank("high") > severity_rank("medium"));
        assert!(severity_rank("low") > severity_rank("info"));
    }
}
