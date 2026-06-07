use serde_json::{Value, json};

use crate::production::limits::parse_query_date;
use crate::{
    AgentCoverageSla, ExecutiveDashboard, PortalRole, RiskHeatmapItem, RiskIncidentCandidate,
    SecurityCorrelationItem, SecurityEventsSummary, Snapshot, query_param, role_envelope,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct RiskNarrativeQuery {
    date: Option<String>,
    department: Option<String>,
    module: Option<String>,
}

impl RiskNarrativeQuery {
    pub(crate) fn from_url(url: &str) -> Self {
        Self {
            date: query_param(url, "date").filter(|value| parse_query_date(value).is_some()),
            department: query_param(url, "department").filter(|value| !value.trim().is_empty()),
            module: query_param(url, "module").filter(|value| !value.trim().is_empty()),
        }
    }
}

pub(crate) struct RiskNarrativeInputs<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) workforce_kpi_explain: &'a Value,
    pub(crate) ueba_risk: &'a Value,
    pub(crate) agent_coverage_sla: &'a AgentCoverageSla,
    pub(crate) risk_heatmap: &'a [RiskHeatmapItem],
    pub(crate) security_correlation: &'a [SecurityCorrelationItem],
    pub(crate) risk_incident_candidates: &'a [RiskIncidentCandidate],
    pub(crate) executive_dashboard: &'a ExecutiveDashboard,
    pub(crate) security_events_summary: &'a SecurityEventsSummary,
}

#[derive(Clone, Debug)]
struct NarrativeSignal {
    score: u8,
    level: &'static str,
    confidence: String,
    classification: String,
    why: Vec<String>,
    evidence: Vec<Value>,
    recommended_actions: Vec<String>,
    limitations: Vec<String>,
    department: Option<String>,
    generated_at_utc: String,
}

pub(crate) fn build_risk_narrative(
    inputs: RiskNarrativeInputs<'_>,
    role: PortalRole,
    query: &RiskNarrativeQuery,
) -> Value {
    let selected_heatmap = select_heatmap_item(inputs.risk_heatmap, query.department.as_deref());
    let selected_correlation = select_correlation_item(
        inputs.security_correlation,
        selected_heatmap
            .map(|item| item.department.as_str())
            .or(query.department.as_deref()),
    );
    let mut signal = NarrativeSignal {
        score: 0,
        level: "low",
        confidence: "unknown".to_string(),
        classification: "insufficient_data".to_string(),
        why: Vec::new(),
        evidence: Vec::new(),
        recommended_actions: Vec::new(),
        limitations: risk_narrative_limitations(),
        department: selected_heatmap
            .map(|item| item.department.clone())
            .or_else(|| query.department.clone()),
        generated_at_utc: inputs.snapshot.generated_at_utc.clone(),
    };

    add_workforce_kpi_signal(&mut signal, inputs.workforce_kpi_explain);
    add_ueba_signal(&mut signal, inputs.ueba_risk);
    add_ueba_confidence_guardrails(&mut signal, inputs.ueba_risk);
    add_coverage_signal(
        &mut signal,
        inputs.agent_coverage_sla,
        inputs.workforce_kpi_explain,
    );
    add_heatmap_signal(&mut signal, selected_heatmap);
    add_security_correlation_signal(&mut signal, selected_correlation);
    let department_scope = signal.department.clone();
    add_incident_candidate_signal(
        &mut signal,
        inputs.risk_incident_candidates,
        department_scope.as_deref(),
    );
    add_security_events_signal(&mut signal, inputs.security_events_summary);
    add_remote_activity_signal(&mut signal, inputs.workforce_kpi_explain);
    add_pfsense_contract_signal(&mut signal);

    signal.score = signal.score.min(100);
    signal.level = risk_level(signal.score);
    finalize_recommendations(&mut signal, inputs.executive_dashboard);
    narrative_payload(signal, role, query)
}

pub(crate) fn build_risk_narrative_from_report(
    report: &Value,
    role: PortalRole,
    query: &RiskNarrativeQuery,
) -> Value {
    let mut signal = NarrativeSignal {
        score: 0,
        level: "low",
        confidence: "unknown".to_string(),
        classification: "insufficient_data".to_string(),
        why: Vec::new(),
        evidence: Vec::new(),
        recommended_actions: Vec::new(),
        limitations: risk_narrative_limitations(),
        department: query.department.clone(),
        generated_at_utc: report
            .get("generated_at_utc")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
    };

    add_workforce_kpi_signal(
        &mut signal,
        report.get("workforce_kpi_explain").unwrap_or(&Value::Null),
    );
    add_ueba_signal(&mut signal, report.get("ueba_risk").unwrap_or(&Value::Null));
    add_ueba_confidence_guardrails(&mut signal, report.get("ueba_risk").unwrap_or(&Value::Null));
    add_coverage_from_report_signal(&mut signal, report);
    let selected_heatmap = select_heatmap_value(
        report.get("risk_heatmap").and_then(Value::as_array),
        query.department.as_deref(),
    );
    if let Some(item) = selected_heatmap {
        if signal.department.is_none() {
            signal.department = item
                .get("department")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        add_heatmap_value_signal(&mut signal, item);
    }
    let selected_correlation = select_correlation_value(
        report.get("security_correlation").and_then(Value::as_array),
        signal.department.as_deref().or(query.department.as_deref()),
    );
    if let Some(item) = selected_correlation {
        add_security_correlation_value_signal(&mut signal, item);
    }
    add_incident_candidate_value_signal(
        &mut signal,
        report
            .get("risk_incident_candidates")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );
    add_security_events_value_signal(
        &mut signal,
        report
            .get("security_events_summary")
            .unwrap_or(&Value::Null),
    );
    add_remote_activity_signal(
        &mut signal,
        report.get("workforce_kpi_explain").unwrap_or(&Value::Null),
    );
    add_pfsense_contract_signal(&mut signal);

    signal.score = signal.score.min(100);
    signal.level = risk_level(signal.score);
    finalize_recommendations_from_report(&mut signal, report);
    narrative_payload(signal, role, query)
}

fn select_heatmap_item<'a>(
    items: &'a [RiskHeatmapItem],
    department: Option<&str>,
) -> Option<&'a RiskHeatmapItem> {
    if let Some(department) = department {
        items
            .iter()
            .find(|item| item.department.eq_ignore_ascii_case(department))
    } else {
        items.first()
    }
}

fn select_correlation_item<'a>(
    items: &'a [SecurityCorrelationItem],
    department: Option<&str>,
) -> Option<&'a SecurityCorrelationItem> {
    department
        .and_then(|department| {
            items
                .iter()
                .find(|item| item.department.eq_ignore_ascii_case(department))
        })
        .or_else(|| items.iter().max_by_key(|item| item.correlation_score))
}

fn select_heatmap_value<'a>(
    items: Option<&'a Vec<Value>>,
    department: Option<&str>,
) -> Option<&'a Value> {
    let items = items?;
    if let Some(department) = department {
        items.iter().find(|item| {
            item.get("department")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case(department))
        })
    } else {
        items.first()
    }
}

fn select_correlation_value<'a>(
    items: Option<&'a Vec<Value>>,
    department: Option<&str>,
) -> Option<&'a Value> {
    let items = items?;
    department
        .and_then(|department| {
            items.iter().find(|item| {
                item.get("department")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case(department))
            })
        })
        .or_else(|| {
            items.iter().max_by_key(|item| {
                item.get("correlation_score")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            })
        })
}

fn add_workforce_kpi_signal(signal: &mut NarrativeSignal, explain: &Value) {
    let kpi_score = explain.get("kpi_score").and_then(Value::as_u64);
    let confidence = explain
        .get("confidence")
        .and_then(Value::as_str)
        .unwrap_or("low");
    if let Some(score) = kpi_score {
        let risk = 100_u64.saturating_sub(score.min(100));
        if risk >= 50 {
            add_score(
                signal,
                30,
                "Индекс активности существенно ниже рабочего ориентира",
            );
        } else if risk >= 30 {
            add_score(signal, 20, "Индекс активности ниже среднего уровня");
        } else if risk >= 15 {
            add_score(signal, 10, "Индекс активности немного ниже целевого уровня");
        }
        signal.evidence.push(evidence(
            "workforce_kpi",
            "Индекс активности",
            &format!("{score}%"),
            score_to_severity(score as u8),
        ));
    } else {
        add_score(signal, 12, "Индекс активности не рассчитан");
        signal.evidence.push(evidence(
            "workforce_kpi",
            "Индекс активности",
            "нет данных",
            "medium",
        ));
    }
    match confidence {
        "low" => add_score(signal, 20, "Низкое доверие к KPI"),
        "medium" => add_score(
            signal,
            10,
            "KPI рассчитан с частичными ограничениями данных",
        ),
        _ => {}
    }
    signal.evidence.push(evidence(
        "workforce_kpi",
        "Доверие к KPI",
        confidence,
        match confidence {
            "low" => "high",
            "medium" => "medium",
            _ => "low",
        },
    ));
    if let Some(missing) = explain
        .pointer("/coverage/missing_sources")
        .and_then(Value::as_array)
    {
        let count = missing.len();
        if count > 0 {
            add_score(
                signal,
                (count as u8).saturating_mul(6).min(18),
                "Есть пропуски источников данных",
            );
        }
    }
}

fn add_ueba_signal(signal: &mut NarrativeSignal, risk: &Value) {
    let level = risk
        .get("level")
        .and_then(Value::as_str)
        .unwrap_or("normal");
    let score = risk
        .get("score")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(100);
    match level {
        "critical" => add_score(signal, 30, "UEBA score находится на критическом уровне"),
        "high" => add_score(signal, 22, "UEBA score повышен до high"),
        "medium" => add_score(signal, 12, "UEBA score находится на среднем уровне"),
        "low" => add_score(signal, 5, "UEBA score показывает низкий, но ненулевой риск"),
        _ => {}
    }
    signal.evidence.push(evidence(
        "ueba",
        "UEBA score",
        &format!("{level} · {score}/100"),
        match level {
            "critical" => "critical",
            "high" => "high",
            "medium" => "medium",
            _ => "low",
        },
    ));
}

fn add_ueba_confidence_guardrails(signal: &mut NarrativeSignal, risk: &Value) {
    let confidence = risk
        .get("confidence_level")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let classification = risk
        .get("classification")
        .and_then(Value::as_str)
        .unwrap_or("insufficient_data");
    signal.confidence = confidence.to_string();
    signal.classification = classification.to_string();
    if matches!(confidence, "low" | "unknown") {
        push_unique(&mut signal.why, "Уверенность в выводе ниже целевого уровня");
        push_unique(
            &mut signal.recommended_actions,
            "Проверить полноту данных до управленческого вывода",
        );
        push_unique(
            &mut signal.limitations,
            "Низкая уверенность не подтверждает инцидент без ручной проверки",
        );
    }
    if classification == "needs_investigation" {
        push_unique(
            &mut signal.recommended_actions,
            "Зафиксировать статус Needs Investigation и передать на ручной разбор",
        );
    }
}

fn add_coverage_signal(
    signal: &mut NarrativeSignal,
    sla: &AgentCoverageSla,
    workforce_kpi_explain: &Value,
) {
    let coverage = if sla.expected_nodes > 0 {
        Some(sla.coverage_pct)
    } else {
        workforce_kpi_explain
            .pointer("/coverage/agent_coverage_percent")
            .and_then(Value::as_u64)
            .map(|value| value.min(100) as u8)
    };
    if let Some(value) = coverage {
        if value < 50 {
            add_score(
                signal,
                25,
                "Покрытие агентов критически ниже целевого уровня",
            );
        } else if value < 75 {
            add_score(signal, 18, "Покрытие агентов ниже целевого уровня");
        } else if value < 90 {
            add_score(signal, 8, "Покрытие агентов требует внимания");
        }
        signal.evidence.push(evidence(
            "coverage",
            "Покрытие агентов",
            &format!("{value}%"),
            if value < 50 {
                "high"
            } else if value < 75 {
                "medium"
            } else {
                "low"
            },
        ));
    } else {
        add_score(signal, 10, "Покрытие агентов не подтверждено");
    }
}

fn add_coverage_from_report_signal(signal: &mut NarrativeSignal, report: &Value) {
    let coverage = report
        .pointer("/agent_coverage_sla/coverage_pct")
        .and_then(Value::as_u64)
        .or_else(|| {
            report
                .pointer("/workforce_kpi_explain/coverage/agent_coverage_percent")
                .and_then(Value::as_u64)
        })
        .map(|value| value.min(100) as u8);
    if let Some(value) = coverage {
        if value < 50 {
            add_score(
                signal,
                25,
                "Покрытие агентов критически ниже целевого уровня",
            );
        } else if value < 75 {
            add_score(signal, 18, "Покрытие агентов ниже целевого уровня");
        } else if value < 90 {
            add_score(signal, 8, "Покрытие агентов требует внимания");
        }
        signal.evidence.push(evidence(
            "coverage",
            "Покрытие агентов",
            &format!("{value}%"),
            if value < 50 {
                "high"
            } else if value < 75 {
                "medium"
            } else {
                "low"
            },
        ));
    }
}

fn add_heatmap_signal(signal: &mut NarrativeSignal, item: Option<&RiskHeatmapItem>) {
    let Some(item) = item else {
        return;
    };
    match item.heat_level.as_str() {
        "CRITICAL" => add_score(signal, 25, "Карта рисков показывает критический уровень"),
        "HIGH" => add_score(signal, 18, "Карта рисков показывает высокий уровень"),
        "MEDIUM" => add_score(signal, 10, "Карта рисков показывает средний уровень"),
        _ => {}
    }
    signal.evidence.push(evidence(
        "risk_heatmap",
        "Карта рисков",
        &format!("{} · {}", item.department, item.heat_level),
        heatmap_severity(&item.heat_level),
    ));
}

fn add_heatmap_value_signal(signal: &mut NarrativeSignal, item: &Value) {
    let level = item
        .get("heat_level")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    match level {
        "CRITICAL" => add_score(signal, 25, "Карта рисков показывает критический уровень"),
        "HIGH" => add_score(signal, 18, "Карта рисков показывает высокий уровень"),
        "MEDIUM" => add_score(signal, 10, "Карта рисков показывает средний уровень"),
        _ => {}
    }
    let department = item
        .get("department")
        .and_then(Value::as_str)
        .unwrap_or("-");
    signal.evidence.push(evidence(
        "risk_heatmap",
        "Карта рисков",
        &format!("{department} · {level}"),
        heatmap_severity(level),
    ));
}

fn add_security_correlation_signal(
    signal: &mut NarrativeSignal,
    item: Option<&SecurityCorrelationItem>,
) {
    let Some(item) = item else {
        return;
    };
    let score = item.correlation_score;
    if score >= 85 {
        add_score(signal, 22, "Связь рисков и активности высокая");
    } else if score >= 60 {
        add_score(signal, 15, "Связь рисков и активности повышена");
    } else if score >= 35 {
        add_score(signal, 8, "Есть умеренная связь рисков и активности");
    }
    signal.evidence.push(evidence(
        "security_correlation",
        "Связь рисков и активности",
        &format!("{score}/100"),
        if score >= 85 {
            "high"
        } else if score >= 60 {
            "medium"
        } else {
            "low"
        },
    ));
}

fn add_security_correlation_value_signal(signal: &mut NarrativeSignal, item: &Value) {
    let score = item
        .get("correlation_score")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(100) as u8;
    if score >= 85 {
        add_score(signal, 22, "Связь рисков и активности высокая");
    } else if score >= 60 {
        add_score(signal, 15, "Связь рисков и активности повышена");
    } else if score >= 35 {
        add_score(signal, 8, "Есть умеренная связь рисков и активности");
    }
    signal.evidence.push(evidence(
        "security_correlation",
        "Связь рисков и активности",
        &format!("{score}/100"),
        if score >= 85 {
            "high"
        } else if score >= 60 {
            "medium"
        } else {
            "low"
        },
    ));
}

fn add_incident_candidate_signal(
    signal: &mut NarrativeSignal,
    candidates: &[RiskIncidentCandidate],
    department: Option<&str>,
) {
    let count = candidates
        .iter()
        .filter(|item| {
            department.is_none_or(|department| {
                item.department
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(department))
            })
        })
        .filter(|item| {
            matches!(
                item.risk_level.as_deref().unwrap_or("UNKNOWN"),
                "HIGH" | "CRITICAL"
            )
        })
        .count();
    if count > 0 {
        add_score(
            signal,
            (count as u8).saturating_mul(10).min(25),
            "Есть кандидаты на ручную проверку",
        );
    }
    signal.evidence.push(evidence(
        "incident_candidates",
        "Кандидаты на проверку",
        &count.to_string(),
        if count > 0 { "medium" } else { "low" },
    ));
}

fn add_incident_candidate_value_signal(signal: &mut NarrativeSignal, candidates: &[Value]) {
    let count = candidates
        .iter()
        .filter(|item| {
            matches!(
                item.get("risk_level")
                    .and_then(Value::as_str)
                    .unwrap_or("UNKNOWN"),
                "HIGH" | "CRITICAL"
            )
        })
        .count();
    if count > 0 {
        add_score(
            signal,
            (count as u8).saturating_mul(10).min(25),
            "Есть кандидаты на ручную проверку",
        );
    }
    signal.evidence.push(evidence(
        "incident_candidates",
        "Кандидаты на проверку",
        &count.to_string(),
        if count > 0 { "medium" } else { "low" },
    ));
}

fn add_security_events_signal(signal: &mut NarrativeSignal, summary: &SecurityEventsSummary) {
    if summary.fallback_used {
        add_score(
            signal,
            8,
            "Агрегированные события безопасности доступны в резервном режиме",
        );
    }
    if summary.events_24h > 0 {
        add_score(
            signal,
            summary.events_24h.saturating_mul(3).min(15) as u8,
            "Есть агрегированные события безопасности за 24 часа",
        );
    }
    signal.evidence.push(evidence(
        "security_events",
        "События безопасности",
        &summary.events_24h.to_string(),
        if summary.events_24h > 0 {
            "medium"
        } else {
            "low"
        },
    ));
}

fn add_security_events_value_signal(signal: &mut NarrativeSignal, summary: &Value) {
    let events = summary
        .get("events_24h")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if summary
        .get("fallback_used")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        add_score(
            signal,
            8,
            "Агрегированные события безопасности доступны в резервном режиме",
        );
    }
    if events > 0 {
        add_score(
            signal,
            events.saturating_mul(3).min(15) as u8,
            "Есть агрегированные события безопасности за 24 часа",
        );
    }
    signal.evidence.push(evidence(
        "security_events",
        "События безопасности",
        &events.to_string(),
        if events > 0 { "medium" } else { "low" },
    ));
}

fn add_remote_activity_signal(signal: &mut NarrativeSignal, explain: &Value) {
    let factors = explain
        .get("factors")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let afterhours = factors
        .iter()
        .find(|item| item.get("name").and_then(Value::as_str) == Some("afterhours_activity"));
    if let Some(item) = afterhours {
        let impact = item.get("impact").and_then(Value::as_str).unwrap_or("0");
        if impact.starts_with('-') && impact != "-0" {
            add_score(signal, 8, "Есть признаки активности вне рабочего времени");
            signal.evidence.push(evidence(
                "workforce_kpi",
                "Активность вне рабочего времени",
                impact,
                "medium",
            ));
        }
    }
    if factors.iter().any(|item| {
        item.get("name").and_then(Value::as_str) == Some("remote_session_activity")
            && item.get("impact").and_then(Value::as_str).unwrap_or("0") != "0"
    }) {
        signal.evidence.push(evidence(
            "workforce_kpi",
            "Удаленные сессии",
            "обнаружены",
            "low",
        ));
    }
}

fn add_pfsense_contract_signal(signal: &mut NarrativeSignal) {
    add_score(signal, 3, "pfSense находится в contract_only режиме");
    signal.evidence.push(evidence(
        "pfsense",
        "pfSense readiness",
        "contract_only",
        "low",
    ));
}

fn add_score(signal: &mut NarrativeSignal, points: u8, reason: &str) {
    signal.score = signal.score.saturating_add(points);
    if !signal.why.iter().any(|item| item == reason) {
        signal.why.push(reason.to_string());
    }
}

fn finalize_recommendations(signal: &mut NarrativeSignal, dashboard: &ExecutiveDashboard) {
    finalize_common_recommendations(signal);
    if dashboard
        .critical_candidates
        .as_ref()
        .is_some_and(|items| !items.is_empty())
    {
        push_unique(
            &mut signal.recommended_actions,
            "Передать кандидатов на проверку в контур ИБ",
        );
    }
}

fn finalize_recommendations_from_report(signal: &mut NarrativeSignal, report: &Value) {
    finalize_common_recommendations(signal);
    if report
        .get("risk_incident_candidates")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
    {
        push_unique(
            &mut signal.recommended_actions,
            "Передать кандидатов на проверку в контур ИБ",
        );
    }
}

fn finalize_common_recommendations(signal: &mut NarrativeSignal) {
    if signal
        .why
        .iter()
        .any(|item| item.contains("Покрытие") || item.contains("пропуски"))
    {
        push_unique(
            &mut signal.recommended_actions,
            "Проверить подразделения с низким покрытием данных",
        );
    }
    if signal.why.iter().any(|item| item.contains("UEBA")) {
        push_unique(
            &mut signal.recommended_actions,
            "Передать security-события в контур ИБ для анализа",
        );
    }
    if signal
        .evidence
        .iter()
        .any(|item| item.get("label").and_then(Value::as_str) == Some("Удаленные сессии"))
    {
        push_unique(
            &mut signal.recommended_actions,
            "Проверить рост удаленных сессий",
        );
    }
    if signal.recommended_actions.is_empty() {
        push_unique(
            &mut signal.recommended_actions,
            "Продолжить наблюдение и контролировать полноту данных",
        );
    }
}

fn push_unique(items: &mut Vec<String>, value: &str) {
    if !items.iter().any(|item| item == value) {
        items.push(value.to_string());
    }
}

fn risk_narrative_limitations() -> Vec<String> {
    vec![
        "pfSense находится в contract_only режиме".to_string(),
        "Risk Narrative не является ML-прогнозом".to_string(),
        "Risk Narrative не подтверждает нарушение без ручной проверки".to_string(),
    ]
}

fn narrative_payload(
    signal: NarrativeSignal,
    role: PortalRole,
    query: &RiskNarrativeQuery,
) -> Value {
    json!({
        "ok": true,
        "role_context": role_envelope(role, "risk_narrative"),
        "scope": if signal.department.is_some() { "department" } else { "aggregate" },
        "query": {
            "date": query.date,
            "department": query.department,
            "module": query.module,
            "employee_id_supported": false
        },
        "risk_level": signal.level,
        "risk_score": signal.score,
        "confidence": signal.confidence,
        "classification": signal.classification,
        "title": risk_title(signal.level),
        "summary": risk_summary(signal.level, signal.department.as_deref(), &signal.why),
        "why": signal.why,
        "evidence": signal.evidence,
        "recommended_actions": signal.recommended_actions,
        "limitations": signal.limitations,
        "model": {
            "type": "rule_based",
            "ml": false,
            "llm": false,
            "predictive": false,
            "version": "risk-narrative-v1"
        },
        "generated_at_utc": signal.generated_at_utc,
    })
}

fn risk_level(score: u8) -> &'static str {
    match score {
        0..=24 => "low",
        25..=49 => "guarded",
        50..=74 => "medium",
        75..=89 => "high",
        _ => "critical",
    }
}

fn risk_title(level: &str) -> &'static str {
    match level {
        "critical" => "Критический рост операционного риска",
        "high" => "Высокий операционный риск",
        "medium" => "Умеренный рост операционного риска",
        "guarded" => "Риск требует наблюдения",
        _ => "Риск низкий",
    }
}

fn risk_summary(level: &str, department: Option<&str>, why: &[String]) -> String {
    let scope = department
        .map(|value| format!("в зоне {value}"))
        .unwrap_or_else(|| "по текущему срезу".to_string());
    let reason = why
        .first()
        .cloned()
        .unwrap_or_else(|| "существенных негативных признаков не выявлено".to_string());
    match level {
        "critical" | "high" => {
            format!("{scope} риск повышен: {reason}. Требуется ручная проверка.")
        }
        "medium" => format!(
            "{scope} есть умеренный риск: {reason}. Нужно проверить причины и полноту данных."
        ),
        "guarded" => format!("{scope} риск требует наблюдения: {reason}."),
        _ => format!("{scope} риск низкий: {reason}."),
    }
}

fn evidence(source: &str, label: &str, value: &str, severity: &str) -> Value {
    json!({
        "source": source,
        "label": label,
        "value": value,
        "severity": severity,
    })
}

fn score_to_severity(score: u8) -> &'static str {
    if score < 50 {
        "high"
    } else if score < 75 {
        "medium"
    } else {
        "low"
    }
}

fn heatmap_severity(level: &str) -> &'static str {
    match level {
        "CRITICAL" => "critical",
        "HIGH" => "high",
        "MEDIUM" => "medium",
        _ => "low",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_level_thresholds_are_stable() {
        assert_eq!(risk_level(0), "low");
        assert_eq!(risk_level(24), "low");
        assert_eq!(risk_level(25), "guarded");
        assert_eq!(risk_level(50), "medium");
        assert_eq!(risk_level(75), "high");
        assert_eq!(risk_level(90), "critical");
    }

    #[test]
    fn report_builder_is_rule_based_and_does_not_support_employee_scope() {
        let report = json!({
            "generated_at_utc": "2026-06-07T10:00:00Z",
            "workforce_kpi_explain": {
                "kpi_score": 62,
                "confidence": "medium",
                "coverage": {
                    "agent_coverage_percent": 70,
                    "missing_sources": ["applications"]
                },
                "factors": [
                    {"name": "afterhours_activity", "impact": "-2"},
                    {"name": "remote_session_activity", "impact": "+2"}
                ]
            },
            "ueba_risk": {"level": "high", "score": 72},
            "agent_coverage_sla": {"coverage_pct": 70},
            "risk_heatmap": [{"department": "DEPT-1", "heat_level": "HIGH"}],
            "security_correlation": [{"department": "DEPT-1", "correlation_score": 65}],
            "risk_incident_candidates": [{"risk_level": "HIGH"}],
            "security_events_summary": {"events_24h": 2, "fallback_used": false}
        });
        let query = RiskNarrativeQuery {
            department: Some("DEPT-1".to_string()),
            ..RiskNarrativeQuery::default()
        };
        let narrative = build_risk_narrative_from_report(&report, PortalRole::Executive, &query);
        assert_eq!(narrative["ok"], true);
        assert_eq!(narrative["query"]["employee_id_supported"], false);
        assert_eq!(narrative["model"]["ml"], false);
        assert_eq!(narrative["model"]["llm"], false);
        assert!(
            narrative["risk_score"].as_u64().unwrap() >= 50,
            "expected elevated risk narrative"
        );
        assert!(
            narrative["why"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str().unwrap().contains("UEBA"))
        );
    }
}
