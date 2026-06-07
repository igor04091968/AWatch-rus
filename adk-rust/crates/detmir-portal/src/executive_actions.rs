use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{PortalRole, role_envelope};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ExecutiveAction {
    pub priority: ActionPriority,
    pub title: String,
    pub summary: String,
    pub owner_role: ActionOwnerRole,
    pub recommended_deadline: String,
    pub reason_codes: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActionPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActionOwnerRole {
    Executive,
    Manager,
    Security,
    Forensics,
    Admin,
}

pub(crate) fn build_action_center_from_report(report: &Value, role: PortalRole) -> Value {
    let actions = generate_actions(report)
        .into_iter()
        .filter(|action| action_visible_for_role(action.owner_role, role))
        .collect::<Vec<_>>();
    json!({
        "ok": true,
        "role_context": role_envelope(role, "actions"),
        "actions": actions,
        "model": {
            "type": "rule_based",
            "version": "executive-action-center-v1",
            "ml": false,
            "llm": false,
            "auto_remediation": false
        },
        "generated_at_utc": Utc::now().to_rfc3339(),
        "limitations": [
            "Рекомендуемые действия не выполняются автоматически",
            "Action Center не блокирует пользователей и не меняет политики",
            "Все действия требуют ручного подтверждения ответственным контуром"
        ]
    })
}

pub(crate) fn filter_actions_for_role(actions: &Value, role: PortalRole) -> Value {
    let filtered = actions
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| {
            item.get("owner_role")
                .and_then(Value::as_str)
                .and_then(parse_owner_role)
                .is_some_and(|owner| action_visible_for_role(owner, role))
        })
        .cloned()
        .collect::<Vec<_>>();
    Value::Array(filtered)
}

pub(crate) fn actions_from_center(center: &Value) -> Value {
    center
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .map(Value::Array)
        .unwrap_or_else(|| Value::Array(Vec::new()))
}

fn generate_actions(report: &Value) -> Vec<ExecutiveAction> {
    let mut actions = Vec::new();
    add_workforce_kpi_action(report, &mut actions);
    add_coverage_action(report, &mut actions);
    add_ueba_action(report, &mut actions);
    add_security_correlation_action(report, &mut actions);
    add_incident_candidate_action(report, &mut actions);
    add_risk_narrative_action(report, &mut actions);
    if actions.is_empty() {
        actions.push(ExecutiveAction {
            priority: ActionPriority::Low,
            title: "Продолжить наблюдение".to_string(),
            summary: "Критичных управленческих действий по текущему срезу не требуется".to_string(),
            owner_role: ActionOwnerRole::Manager,
            recommended_deadline: "72h".to_string(),
            reason_codes: vec!["NORMAL_OBSERVATION".to_string()],
            evidence: vec!["Критичные сигналы не выявлены".to_string()],
        });
    }
    actions.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.owner_role.as_str().cmp(right.owner_role.as_str()))
            .then_with(|| left.title.cmp(&right.title))
    });
    actions
}

fn add_workforce_kpi_action(report: &Value, actions: &mut Vec<ExecutiveAction>) {
    let Some(score) = report
        .pointer("/workforce_kpi_explain/kpi_score")
        .and_then(Value::as_u64)
    else {
        return;
    };
    if score >= 70 {
        return;
    }
    let mut reason_codes = vec!["LOW_WORKFORCE_KPI".to_string()];
    let mut evidence = vec![format!("Workforce KPI ниже целевого уровня: {score}%")];
    if has_kpi_factor(report, "remote_session_activity") {
        reason_codes.push("HIGH_REMOTE_ACTIVITY".to_string());
        evidence.push("Рост удаленных сессий влияет на управленческий риск".to_string());
    }
    if let Some(confidence) = report
        .pointer("/workforce_kpi_explain/confidence")
        .and_then(Value::as_str)
        .filter(|value| *value == "low")
    {
        reason_codes.push("LOW_KPI_CONFIDENCE".to_string());
        evidence.push(format!("Доверие к KPI: {confidence}"));
    }
    actions.push(ExecutiveAction {
        priority: if score < 50 {
            ActionPriority::Critical
        } else {
            ActionPriority::High
        },
        title: "Проверить подразделение с низким индексом активности".to_string(),
        summary:
            "Индекс активности ниже управленческого порога; требуется проверка причины просадки"
                .to_string(),
        owner_role: ActionOwnerRole::Manager,
        recommended_deadline: if score < 50 { "4h" } else { "24h" }.to_string(),
        reason_codes,
        evidence,
    });
}

fn add_coverage_action(report: &Value, actions: &mut Vec<ExecutiveAction>) {
    let coverage = report
        .pointer("/agent_coverage_sla/coverage_pct")
        .or_else(|| report.pointer("/workforce_kpi_explain/coverage/agent_coverage_percent"))
        .and_then(Value::as_u64)
        .unwrap_or(100);
    let sla_status = report
        .pointer("/agent_coverage_sla/sla_status")
        .and_then(Value::as_str)
        .unwrap_or("OK");
    if coverage >= 80 && !matches!(sla_status, "WARNING" | "CRITICAL") {
        return;
    }
    actions.push(ExecutiveAction {
        priority: if coverage < 60 || sla_status == "CRITICAL" {
            ActionPriority::Critical
        } else {
            ActionPriority::High
        },
        title: "Проверить состояние агентов".to_string(),
        summary: "Полнота данных ниже целевого уровня; показатели могут быть нерепрезентативны"
            .to_string(),
        owner_role: ActionOwnerRole::Admin,
        recommended_deadline: if coverage < 60 { "4h" } else { "24h" }.to_string(),
        reason_codes: vec!["LOW_COVERAGE".to_string()],
        evidence: vec![
            format!("Покрытие агентов: {coverage}%"),
            format!("SLA полноты данных: {sla_status}"),
        ],
    });
}

fn add_ueba_action(report: &Value, actions: &mut Vec<ExecutiveAction>) {
    let score = report
        .pointer("/ueba_risk/score")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let level = report
        .pointer("/ueba_risk/level")
        .and_then(Value::as_str)
        .unwrap_or("low");
    if score < 70 && !matches!(level, "high" | "critical") {
        return;
    }
    actions.push(ExecutiveAction {
        priority: if score >= 90 || level == "critical" {
            ActionPriority::Critical
        } else {
            ActionPriority::High
        },
        title: "Передать данные в контур ИБ".to_string(),
        summary: "UEBA score повышен; требуется ручная проверка безопасности".to_string(),
        owner_role: ActionOwnerRole::Security,
        recommended_deadline: if score >= 90 { "4h" } else { "24h" }.to_string(),
        reason_codes: vec!["HIGH_UEBA".to_string()],
        evidence: vec![format!("UEBA score: {score}, уровень: {level}")],
    });
}

fn add_security_correlation_action(report: &Value, actions: &mut Vec<ExecutiveAction>) {
    let max_score = report
        .get("security_correlation")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("correlation_score").and_then(Value::as_u64))
        .max()
        .unwrap_or(0);
    if max_score < 60 {
        return;
    }
    actions.push(ExecutiveAction {
        priority: if max_score >= 80 {
            ActionPriority::Critical
        } else {
            ActionPriority::High
        },
        title: "Проверить связь активности и ИБ-событий".to_string(),
        summary: "Есть корреляция между операционным риском и событиями безопасности".to_string(),
        owner_role: ActionOwnerRole::Security,
        recommended_deadline: "24h".to_string(),
        reason_codes: vec!["HIGH_SECURITY_CORRELATION".to_string()],
        evidence: vec![format!("Security correlation score: {max_score}")],
    });
}

fn add_incident_candidate_action(report: &Value, actions: &mut Vec<ExecutiveAction>) {
    let Some(candidates) = report
        .get("risk_incident_candidates")
        .and_then(Value::as_array)
    else {
        return;
    };
    if candidates.is_empty() {
        return;
    }
    let critical = candidates.iter().any(|item| {
        item.get("risk_level")
            .and_then(Value::as_str)
            .is_some_and(|level| level.eq_ignore_ascii_case("critical"))
    });
    actions.push(ExecutiveAction {
        priority: if critical {
            ActionPriority::Critical
        } else {
            ActionPriority::High
        },
        title: "Провести расследование кандидатов".to_string(),
        summary: "В очереди есть кандидаты на проверку; требуется ручной разбор и фиксация решения"
            .to_string(),
        owner_role: ActionOwnerRole::Forensics,
        recommended_deadline: if critical { "4h" } else { "24h" }.to_string(),
        reason_codes: vec!["INCIDENT_CANDIDATE".to_string()],
        evidence: vec![format!("Кандидатов на проверку: {}", candidates.len())],
    });
}

fn add_risk_narrative_action(report: &Value, actions: &mut Vec<ExecutiveAction>) {
    let score = report
        .pointer("/risk_narrative/risk_score")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if score < 75 {
        return;
    }
    let level = report
        .pointer("/risk_narrative/risk_level")
        .and_then(Value::as_str)
        .unwrap_or("high");
    actions.push(ExecutiveAction {
        priority: if score >= 90 {
            ActionPriority::Critical
        } else {
            ActionPriority::High
        },
        title: "Назначить владельца корректирующих действий".to_string(),
        summary: "Риск-нарратив показывает высокий управленческий риск; нужен ответственный и срок контроля"
            .to_string(),
        owner_role: ActionOwnerRole::Executive,
        recommended_deadline: if score >= 90 { "4h" } else { "24h" }.to_string(),
        reason_codes: vec!["RISK_NARRATIVE_HIGH".to_string()],
        evidence: vec![format!("Risk Narrative: {score}/100, уровень: {level}")],
    });
}

fn has_kpi_factor(report: &Value, factor_name: &str) -> bool {
    report
        .pointer("/workforce_kpi_explain/factors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|item| item.get("name").and_then(Value::as_str) == Some(factor_name))
}

fn action_visible_for_role(owner_role: ActionOwnerRole, role: PortalRole) -> bool {
    match role {
        PortalRole::Admin => true,
        PortalRole::Executive => matches!(
            owner_role,
            ActionOwnerRole::Executive | ActionOwnerRole::Manager | ActionOwnerRole::Admin
        ),
        PortalRole::Manager => matches!(owner_role, ActionOwnerRole::Manager),
        PortalRole::Security => {
            matches!(
                owner_role,
                ActionOwnerRole::Security | ActionOwnerRole::Forensics
            )
        }
        PortalRole::Forensics => {
            matches!(
                owner_role,
                ActionOwnerRole::Forensics | ActionOwnerRole::Security
            )
        }
    }
}

fn parse_owner_role(value: &str) -> Option<ActionOwnerRole> {
    match value {
        "executive" => Some(ActionOwnerRole::Executive),
        "manager" => Some(ActionOwnerRole::Manager),
        "security" => Some(ActionOwnerRole::Security),
        "forensics" => Some(ActionOwnerRole::Forensics),
        "admin" => Some(ActionOwnerRole::Admin),
        _ => None,
    }
}

impl ActionOwnerRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Executive => "executive",
            Self::Manager => "manager",
            Self::Security => "security",
            Self::Forensics => "forensics",
            Self::Admin => "admin",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> Value {
        json!({
            "workforce_kpi_explain": {
                "kpi_score": 48,
                "confidence": "low",
                "coverage": {"agent_coverage_percent": 58},
                "factors": [{"name": "remote_session_activity"}]
            },
            "ueba_risk": {"score": 91, "level": "critical"},
            "agent_coverage_sla": {"coverage_pct": 58, "sla_status": "CRITICAL"},
            "security_correlation": [{"correlation_score": 81}],
            "risk_incident_candidates": [{"risk_level": "CRITICAL"}],
            "risk_narrative": {"risk_score": 92, "risk_level": "critical"}
        })
    }

    #[test]
    fn generates_rule_based_actions_with_priorities() {
        let payload = build_action_center_from_report(&sample_report(), PortalRole::Admin);
        let actions = payload["actions"].as_array().unwrap();
        assert!(actions.len() >= 5);
        assert_eq!(payload["model"]["type"], "rule_based");
        assert_eq!(payload["model"]["ml"], false);
        assert_eq!(payload["model"]["llm"], false);
        assert!(actions.iter().any(|item| {
            item["reason_codes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|code| code == "LOW_WORKFORCE_KPI")
        }));
        assert!(actions.iter().any(|item| item["priority"] == "critical"));
    }

    #[test]
    fn filters_actions_by_role() {
        let admin = build_action_center_from_report(&sample_report(), PortalRole::Admin);
        let security = filter_actions_for_role(&actions_from_center(&admin), PortalRole::Security);
        let security_actions = security.as_array().unwrap();
        assert!(!security_actions.is_empty());
        assert!(
            security_actions
                .iter()
                .all(|item| item["owner_role"] == "security" || item["owner_role"] == "forensics")
        );

        let manager = filter_actions_for_role(&actions_from_center(&admin), PortalRole::Manager);
        let manager_actions = manager.as_array().unwrap();
        assert!(
            manager_actions
                .iter()
                .all(|item| item["owner_role"] == "manager")
        );
    }

    #[test]
    fn emits_observation_when_no_rule_matches() {
        let payload = build_action_center_from_report(&json!({}), PortalRole::Executive);
        let actions = payload["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["priority"], "low");
        assert_eq!(actions[0]["reason_codes"][0], "NORMAL_OBSERVATION");
    }
}
