use serde_json::{Value, json};

use crate::production::limits::parse_query_date;
use crate::{
    PortalRole, Snapshot, display_text_opt, query_param, role_envelope, trend_status,
    workforce_index, workforce_index_status, workforce_trend_json, worktime_totals,
};

struct KpiFactorInputs<'a> {
    users_count: usize,
    active_seconds: i64,
    apps_count: usize,
    kpi_score: u8,
    agent_coverage_percent: u8,
    missing_sources: &'a [String],
}

#[derive(Clone, Debug, Default)]
pub(crate) struct KpiExplainQuery {
    date: Option<String>,
    department: Option<String>,
    owner: Option<String>,
}

impl KpiExplainQuery {
    pub(crate) fn from_url(url: &str) -> Self {
        Self {
            date: query_param(url, "date").filter(|value| parse_query_date(value).is_some()),
            department: query_param(url, "department"),
            owner: query_param(url, "owner"),
        }
    }
}

pub(crate) fn build_workforce_kpi_explain(
    snapshot: &Snapshot,
    policy_explain: &Value,
    role: PortalRole,
    query: &KpiExplainQuery,
    anonymize: bool,
) -> Value {
    let (users_count, active_seconds, apps_count) = worktime_totals(snapshot);
    let base_index = workforce_index(users_count, active_seconds);
    let policy_index = policy_explain
        .get("index")
        .and_then(Value::as_u64)
        .map(|value| value.min(100) as u8);
    let kpi_score = policy_index.or(base_index).unwrap_or(0);
    let agent_coverage_percent = kpi_agent_coverage(snapshot);
    let data_freshness = kpi_data_freshness(snapshot);
    let missing_sources = kpi_missing_sources(snapshot, users_count, apps_count);
    let confidence = kpi_confidence(
        users_count,
        agent_coverage_percent,
        &data_freshness,
        &missing_sources,
    );
    let factors = kpi_explain_factors(
        snapshot,
        policy_explain,
        KpiFactorInputs {
            users_count,
            active_seconds,
            apps_count,
            kpi_score,
            agent_coverage_percent,
            missing_sources: &missing_sources,
        },
    );
    let top_applications = kpi_top_applications(snapshot, anonymize);
    let warnings = kpi_warnings(kpi_score, confidence, &data_freshness, &missing_sources);
    let recommendations = kpi_recommendations(confidence, &missing_sources, kpi_score);
    let mut payload = json!({
        "ok": true,
        "scope": "aggregate",
        "role_context": role_envelope(role, "workforce_kpi_explain"),
        "query": {
            "date": query.date.clone(),
            "department": query.department.clone(),
            "owner": query.owner.clone(),
            "employee_id_supported": false
        },
        "kpi_score": kpi_score,
        "kpi_status": workforce_index_status(Some(kpi_score)),
        "confidence": confidence,
        "coverage": {
            "agent_coverage_percent": agent_coverage_percent,
            "data_freshness": data_freshness,
            "missing_sources": missing_sources,
        },
        "factors": factors,
        "top_applications": top_applications,
        "warnings": warnings,
        "recommendations": recommendations,
        "formula": "rule_based: activity + business app usage - idle/afterhours/missing data with coverage confidence",
        "model": {
            "type": "rule_based",
            "ml": false,
            "llm": false,
            "version": "workforce-kpi-explain-v1"
        },
        "generated_at_utc": snapshot.generated_at_utc,
    });
    filter_kpi_explain_for_role(&mut payload, role);
    payload
}

fn kpi_agent_coverage(snapshot: &Snapshot) -> u8 {
    if snapshot.agent_coverage_sla.expected_nodes > 0 {
        snapshot.agent_coverage_sla.coverage_pct
    } else if snapshot.agent_quality.sessions_collected_total > 0 {
        75
    } else if snapshot.worktime.ok {
        60
    } else {
        0
    }
}

fn kpi_data_freshness(snapshot: &Snapshot) -> String {
    if snapshot.worktime.status.eq_ignore_ascii_case("OK")
        && snapshot
            .worktime_management
            .status
            .eq_ignore_ascii_case("OK")
    {
        "fresh".to_string()
    } else if snapshot.worktime.status.eq_ignore_ascii_case("DEGRADED")
        || snapshot
            .worktime_management
            .status
            .eq_ignore_ascii_case("DEGRADED")
        || snapshot
            .worktime
            .summary
            .to_ascii_lowercase()
            .contains("stale")
        || snapshot
            .worktime_management
            .summary
            .to_ascii_lowercase()
            .contains("stale")
    {
        "stale".to_string()
    } else {
        "missing".to_string()
    }
}

fn kpi_missing_sources(snapshot: &Snapshot, users_count: usize, apps_count: usize) -> Vec<String> {
    let mut missing = Vec::new();
    if !snapshot.worktime.ok || users_count == 0 {
        missing.push("worktime".to_string());
    }
    if !snapshot.worktime_management.ok {
        missing.push("worktime_management".to_string());
    }
    if apps_count == 0 {
        missing.push("applications".to_string());
    }
    if snapshot.agent_coverage_sla.expected_nodes > 0
        && snapshot.agent_coverage_sla.coverage_pct < 50
    {
        missing.push("agent_coverage".to_string());
    }
    missing.sort();
    missing.dedup();
    missing
}

fn kpi_confidence(
    users_count: usize,
    agent_coverage_percent: u8,
    data_freshness: &str,
    missing_sources: &[String],
) -> &'static str {
    if users_count == 0
        || agent_coverage_percent < 50
        || data_freshness == "missing"
        || missing_sources.iter().any(|item| item == "worktime")
    {
        "low"
    } else if agent_coverage_percent < 80
        || data_freshness != "fresh"
        || !missing_sources.is_empty()
    {
        "medium"
    } else {
        "high"
    }
}

fn kpi_explain_factors(
    snapshot: &Snapshot,
    policy_explain: &Value,
    inputs: KpiFactorInputs<'_>,
) -> Vec<Value> {
    let planned_seconds = (inputs.users_count as i64).saturating_mul(8 * 3600);
    let idle_ratio = if planned_seconds > 0 {
        ((planned_seconds - inputs.active_seconds).max(0) as f64 / planned_seconds as f64)
            .clamp(0.0, 1.0)
    } else {
        1.0
    };
    let weighted_apps = policy_explain
        .get("matched_applications")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let afterhours_seconds = kpi_afterhours_seconds(snapshot);
    let remote_sessions = snapshot.agent_quality.rdp_sessions_total as u64;
    let trend = trend_status(&workforce_trend_json(snapshot));

    vec![
        kpi_factor(
            "productive_activity",
            "Полезная активность",
            positive_impact((inputs.kpi_score as i64 * 40) / 100),
            if inputs.kpi_score >= 80 {
                "Высокая доля активности относительно планового рабочего времени"
            } else if inputs.kpi_score >= 60 {
                "Активность близка к рабочему уровню, но есть просадка"
            } else {
                "Активность ниже ожидаемого рабочего уровня"
            },
        ),
        kpi_factor(
            "business_app_usage",
            "Рабочие приложения",
            positive_impact(
                ((weighted_apps.max(inputs.apps_count as u64).min(12) as i64) * 2).min(24),
            ),
            if weighted_apps > 0 {
                "В данных есть приложения, попавшие под рабочие правила"
            } else if inputs.apps_count > 0 {
                "Есть активность по приложениям, но правила рабочих приложений требуют настройки"
            } else {
                "Данных о рабочих приложениях нет"
            },
        ),
        kpi_factor(
            "idle_time",
            "Простой",
            negative_impact((idle_ratio * 30.0).round() as i64),
            if idle_ratio > 0.35 {
                "Есть значимые периоды неактивности в рабочее время"
            } else {
                "Простой не является основным фактором снижения индекса"
            },
        ),
        kpi_factor(
            "afterhours_activity",
            "Активность вне рабочего времени",
            negative_impact((afterhours_seconds / 3600).min(12)),
            if afterhours_seconds > 0 {
                "Есть признаки активности за пределами рабочего окна"
            } else {
                "Существенная активность вне рабочего времени не выявлена"
            },
        ),
        kpi_factor(
            "remote_session_activity",
            "Удаленные сессии",
            positive_impact((remote_sessions.min(5) * 2) as i64),
            if remote_sessions > 0 {
                "RDP/удаленные сессии подтверждают источник активности"
            } else {
                "Удаленные сессии не подтверждены текущим срезом"
            },
        ),
        kpi_factor(
            "data_coverage",
            "Полнота данных",
            if inputs.agent_coverage_percent >= 80 {
                positive_impact(12)
            } else {
                negative_impact(
                    ((80_u8.saturating_sub(inputs.agent_coverage_percent) as i64) / 4).max(1),
                )
            },
            if inputs.agent_coverage_percent >= 80 {
                "Покрытие данных достаточно для уверенного управленческого вывода"
            } else {
                "Покрытие данных снижает доверие к индексу"
            },
        ),
        kpi_factor(
            "missing_data",
            "Отсутствующие данные",
            negative_impact((inputs.missing_sources.len() as i64 * 8).min(32)),
            if inputs.missing_sources.is_empty() {
                "Критичных пропусков источников не выявлено"
            } else {
                "Есть пропуски источников, влияющие на надежность KPI"
            },
        ),
        kpi_factor(
            "trend_change",
            "Изменение тренда",
            match trend.as_str() {
                "monthly" | "weekly" => positive_impact(6),
                "daily_only" => negative_impact(2),
                _ => "0".to_string(),
            },
            match trend.as_str() {
                "monthly" => "Есть месячная история для оценки тренда",
                "weekly" => "Есть недельная история для оценки тренда",
                "daily_only" => "Доступен только дневной срез, исторический тренд ограничен",
                _ => "История тренда пока не накоплена",
            },
        ),
    ]
}

fn kpi_factor(name: &str, label: &str, impact: String, explanation: &str) -> Value {
    json!({
        "name": name,
        "label": label,
        "impact": impact,
        "explanation": explanation,
    })
}

fn positive_impact(value: i64) -> String {
    format!("+{}", value.max(0))
}

fn negative_impact(value: i64) -> String {
    if value <= 0 {
        "0".to_string()
    } else {
        format!("-{value}")
    }
}

fn kpi_afterhours_seconds(snapshot: &Snapshot) -> i64 {
    snapshot
        .worktime_management
        .payload
        .as_ref()
        .and_then(|payload| payload.get("department_rollups"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let total = item
                        .get("calendar_total_active_seconds")
                        .or_else(|| item.get("total_active_seconds"))
                        .and_then(Value::as_i64)?;
                    let workday = item
                        .get("workday_total_active_seconds")
                        .or_else(|| item.get("active_seconds"))
                        .and_then(Value::as_i64)
                        .unwrap_or(total);
                    Some((total - workday).max(0))
                })
                .sum()
        })
        .unwrap_or(0)
}

fn kpi_top_applications(snapshot: &Snapshot, anonymize: bool) -> Vec<Value> {
    let Some(apps) = snapshot
        .worktime
        .payload
        .as_ref()
        .and_then(|payload| payload.get("true_active_apps"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut items = apps
        .iter()
        .enumerate()
        .filter_map(|(idx, app)| {
            let raw_name = app.get("application").and_then(Value::as_str)?;
            let seconds = app
                .get("proved_work_seconds")
                .or_else(|| app.get("active_seconds"))
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0);
            if seconds == 0 {
                return None;
            }
            let name = if anonymize {
                format!("Приложение {}", idx + 1)
            } else {
                display_text_opt(Some(raw_name), &format!("Приложение {}", idx + 1))
            };
            let category = kpi_application_category(raw_name);
            let contribution = if category == "business" {
                "positive"
            } else {
                "neutral"
            };
            Some(json!({
                "name": name,
                "category": category,
                "active_minutes": seconds / 60,
                "contribution": contribution,
            }))
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|item| {
        -item
            .get("active_minutes")
            .and_then(Value::as_i64)
            .unwrap_or(0)
    });
    items.truncate(8);
    items
}

fn kpi_application_category(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    let lower_ru = name.to_lowercase();
    if lower.contains("1c")
        || lower_ru.contains("1с")
        || lower.contains("erp")
        || lower.contains("sap")
        || lower.contains("excel")
        || lower.contains("office")
        || lower.contains("word")
    {
        "business"
    } else if lower.contains("browser")
        || lower.contains("chrome")
        || lower.contains("edge")
        || lower.contains("firefox")
    {
        "mixed"
    } else {
        "other"
    }
}

fn kpi_warnings(
    kpi_score: u8,
    confidence: &str,
    data_freshness: &str,
    missing_sources: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();
    if confidence == "low" {
        warnings
            .push("Низкое доверие к KPI: данных недостаточно для уверенного вывода.".to_string());
    }
    if data_freshness != "fresh" {
        warnings.push(format!("Свежесть данных: {data_freshness}."));
    }
    for source in missing_sources {
        warnings.push(format!("Не хватает источника данных: {source}."));
    }
    if kpi_score < 60 {
        warnings.push("Индекс активности ниже рабочего ориентира.".to_string());
    }
    warnings
}

fn kpi_recommendations(confidence: &str, missing_sources: &[String], kpi_score: u8) -> Vec<String> {
    let mut recommendations = Vec::new();
    if !missing_sources.is_empty() {
        recommendations
            .push("Проверить свежесть источников и восстановить пропущенные данные.".to_string());
    }
    if confidence != "high" {
        recommendations.push(
            "Перед управленческим выводом проверить покрытие данных рабочих мест.".to_string(),
        );
    }
    if kpi_score < 60 {
        recommendations
            .push("Проверить подразделения или ответственных с низкой активностью.".to_string());
    }
    if recommendations.is_empty() {
        recommendations.push("Использовать KPI как агрегированный управленческий индикатор, не как персональную HR-оценку.".to_string());
    }
    recommendations
}

fn filter_kpi_explain_for_role(payload: &mut Value, role: PortalRole) {
    match role {
        PortalRole::Executive | PortalRole::Manager => {
            payload["scope_note"] = json!("Агрегированный Workforce KPI без персональных деталей.");
        }
        PortalRole::Security => {
            payload["scope_note"] =
                json!("ИБ видит только факторы, релевантные риску и надежности данных.");
            filter_kpi_factors(
                payload,
                &[
                    "afterhours_activity",
                    "remote_session_activity",
                    "data_coverage",
                    "missing_data",
                    "trend_change",
                ],
            );
            payload["top_applications"] = Value::Array(Vec::new());
        }
        PortalRole::Forensics => {
            payload["scope_note"] = json!(
                "Расследования видят только контекст надежности данных и временных отклонений."
            );
            filter_kpi_factors(
                payload,
                &["afterhours_activity", "data_coverage", "missing_data"],
            );
            payload["top_applications"] = Value::Array(Vec::new());
        }
        PortalRole::Admin => {
            payload["scope_note"] =
                json!("Администратор видит техническое покрытие источников и rule-based факторы.");
        }
    }
}

fn filter_kpi_factors(payload: &mut Value, allowed: &[&str]) {
    if let Some(factors) = payload.get_mut("factors").and_then(Value::as_array_mut) {
        factors.retain(|item| {
            item.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| allowed.contains(&name))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AgentCoverageSla, AgentQuality, AgentQualityHistorySummary, AgentQualityNodesSummary,
        SecurityEventsSummary, SourceStatus,
    };

    fn kpi_snapshot(
        worktime_ok: bool,
        coverage_pct: u8,
        active_seconds: i64,
        apps: Vec<Value>,
    ) -> Snapshot {
        let rows = if active_seconds > 0 {
            json!([
                {"user": "USER-1", "user_id": "EMP-1", "active_seconds": active_seconds}
            ])
        } else {
            json!([])
        };
        Snapshot {
            generated_at_utc: "2026-06-07T10:00:00Z".to_string(),
            detmir_status: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: String::new(),
                error: None,
                payload: None,
            },
            detmir_check: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: String::new(),
                error: None,
                payload: None,
            },
            failed_units: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: String::new(),
                error: None,
                payload: None,
            },
            worktime: SourceStatus {
                ok: worktime_ok,
                status: if worktime_ok { "OK" } else { "FAIL" }.to_string(),
                summary: String::new(),
                error: None,
                payload: worktime_ok.then(|| {
                    json!({
                        "rows": rows,
                        "true_active_apps": apps
                    })
                }),
            },
            worktime_management: SourceStatus {
                ok: worktime_ok,
                status: if worktime_ok { "OK" } else { "FAIL" }.to_string(),
                summary: String::new(),
                error: None,
                payload: worktime_ok.then(|| {
                    json!({
                        "department_rollups": [
                            {
                                "name": "DEPT-1",
                                "users_count": 1,
                                "active_users": 1,
                                "portfolio_coverage_pct": coverage_pct,
                                "workday_total_active_seconds": active_seconds,
                                "calendar_total_active_seconds": active_seconds
                            }
                        ],
                        "trend": [
                            {"report_date": "2026-06-06", "portfolio_coverage_pct": coverage_pct},
                            {"report_date": "2026-06-07", "portfolio_coverage_pct": coverage_pct}
                        ]
                    })
                }),
            },
            one_c: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: String::new(),
                error: None,
                payload: None,
            },
            one_c_overview: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: String::new(),
                error: None,
                payload: None,
            },
            agent_quality: AgentQuality {
                collector_source: "awatch-agent-rs".to_string(),
                collector_error: None,
                sessions_collected_total: if worktime_ok { 1 } else { 0 },
                active_sessions_total: if worktime_ok { 1 } else { 0 },
                rdp_sessions_total: if worktime_ok { 1 } else { 0 },
                quality_status: if worktime_ok { "OK" } else { "UNKNOWN" }.to_string(),
            },
            agent_quality_history: Vec::new(),
            agent_quality_history_summary: AgentQualityHistorySummary::default(),
            agent_quality_nodes: Vec::new(),
            agent_quality_nodes_summary: AgentQualityNodesSummary::default(),
            agent_coverage_sla: AgentCoverageSla {
                expected_nodes: 1,
                reporting_nodes_24h: if worktime_ok { 1 } else { 0 },
                stale_nodes: 0,
                missing_nodes: if worktime_ok { 0 } else { 1 },
                coverage_pct,
                freshness_pct: coverage_pct,
                sla_status: if coverage_pct >= 80 { "OK" } else { "CRITICAL" }.to_string(),
                problem_nodes: Vec::new(),
            },
            security_events_summary: SecurityEventsSummary::disabled(),
        }
    }

    #[test]
    fn confidence_is_low_when_required_data_is_missing() {
        let snapshot = kpi_snapshot(false, 0, 0, Vec::new());
        let explain = build_workforce_kpi_explain(
            &snapshot,
            &json!({"configured": false}),
            PortalRole::Executive,
            &KpiExplainQuery::default(),
            false,
        );
        assert_eq!(explain["confidence"], "low");
        assert_eq!(explain["kpi_score"], 0);
        assert!(
            explain["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str().unwrap().contains("Низкое доверие"))
        );
    }

    #[test]
    fn factors_are_deterministic_and_role_filtered() {
        let snapshot = kpi_snapshot(
            true,
            95,
            8 * 3600,
            vec![json!({"application": "1C", "proved_work_seconds": 6 * 3600})],
        );
        let policy = json!({
            "configured": true,
            "index": 82,
            "matched_applications": 1
        });
        let explain = build_workforce_kpi_explain(
            &snapshot,
            &policy,
            PortalRole::Executive,
            &KpiExplainQuery::default(),
            false,
        );
        assert_eq!(explain["kpi_score"], 82);
        assert_eq!(explain["confidence"], "high");
        let factors = explain["factors"].as_array().unwrap();
        assert_eq!(factors.len(), 8);
        assert_eq!(factors[0]["name"], "productive_activity");
        assert!(
            factors
                .iter()
                .any(|item| item["name"] == "business_app_usage")
        );
        assert!(
            explain["top_applications"][0]["name"]
                .as_str()
                .unwrap()
                .contains("1C")
        );
        assert!(
            serde_json::to_string(&explain)
                .unwrap()
                .find("EMP-1")
                .is_none()
        );

        let security = build_workforce_kpi_explain(
            &snapshot,
            &policy,
            PortalRole::Security,
            &KpiExplainQuery::default(),
            false,
        );
        assert_eq!(security["top_applications"].as_array().unwrap().len(), 0);
        assert!(
            security["factors"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["name"] != "productive_activity")
        );
    }
}
