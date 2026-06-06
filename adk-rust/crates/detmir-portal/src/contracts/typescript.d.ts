export type ISODateTime = string;

export type PortalRole = "executive" | "manager" | "security" | "forensics" | "admin";
export type RiskLevel = "LOW" | "MEDIUM" | "HIGH" | "CRITICAL" | "UNKNOWN";
export type ReviewStatus =
  | "NEW"
  | "IN_REVIEW"
  | "CONFIRMED"
  | "FALSE_POSITIVE"
  | "POSTPONED";
export type CaseStatus = "OPEN" | "IN_PROGRESS" | "RESOLVED" | "REJECTED" | "ARCHIVED";

export interface JsonObject {
  [key: string]: unknown;
}

export interface EndpointDescriptor {
  method: string;
  path: string;
  purpose?: string;
  [key: string]: unknown;
}

export interface ContractIndex {
  ok: boolean;
  contract_version: string;
  generated_by?: string;
  api_base: "/api" | string;
  compatibility?: JsonObject;
  targets?: string[];
  artifacts: {
    openapi: string;
    typescript: string;
    [key: string]: unknown;
  };
  stable_endpoints: EndpointDescriptor[];
  [key: string]: unknown;
}

export interface RoleContext {
  role: PortalRole;
  role_label?: string;
  scope: string;
  allowed_scopes?: string[];
  server_enforced: boolean;
  [key: string]: unknown;
}

export interface ExecutiveDashboard {
  trust_kpi_score?: number;
  agent_coverage_pct?: number;
  high_risk_departments?: number;
  critical_candidates?: number;
  open_cases?: number;
  resolved_cases_30d?: number;
  forensics_readiness?: string;
  [key: string]: unknown;
}

export interface RiskNarrative {
  status?: "NORMAL" | "ATTENTION" | "HIGH_RISK" | "CRITICAL" | string;
  title?: string;
  summary?: string;
  main_reason?: string;
  recommendation?: string;
  supporting_layers?: JsonObject[];
  [key: string]: unknown;
}

export interface AgentQuality {
  collector_source?: string;
  collector_error?: string | null;
  sessions_collected_total?: number;
  active_sessions_total?: number;
  rdp_sessions_total?: number;
  quality_status?: string;
  [key: string]: unknown;
}

export interface AgentCoverageSla {
  expected_nodes?: number;
  reporting_nodes_24h?: number;
  stale_nodes?: number;
  missing_nodes?: number;
  coverage_pct?: number;
  freshness_pct?: number;
  sla_status?: "OK" | "WARNING" | "CRITICAL" | "UNKNOWN" | string;
  [key: string]: unknown;
}

export interface BusinessRiskItem {
  department?: string;
  trust_score?: number;
  activity_score?: number;
  trend?: string;
  risk_level?: RiskLevel;
  reasons?: string[];
  recommendation?: string;
  problem_nodes_count?: number;
  missing_nodes_count?: number;
  stale_nodes_count?: number;
  [key: string]: unknown;
}

export interface IncidentCandidate {
  id?: string;
  department?: string;
  owner?: string;
  hostname?: string;
  risk_level?: RiskLevel;
  reason?: string;
  evidence?: unknown;
  first_seen_utc?: ISODateTime;
  last_seen_utc?: ISODateTime;
  recommendation?: string;
  review?: IncidentReview;
  [key: string]: unknown;
}

export interface IncidentReview {
  candidate_id: string;
  status: ReviewStatus;
  reviewer?: string;
  comment?: string;
  updated_at?: ISODateTime;
  [key: string]: unknown;
}

export interface IncidentReviewRequest {
  candidate_id: string;
  status: ReviewStatus;
  reviewer?: string;
  comment?: string;
  [key: string]: unknown;
}

export interface CaseItem {
  case_id?: string;
  candidate_id?: string;
  title?: string;
  status?: CaseStatus;
  owner?: string;
  created_at_utc?: ISODateTime;
  updated_at_utc?: ISODateTime;
  summary?: string;
  decision?: string;
  [key: string]: unknown;
}

export interface CreateCaseRequest {
  candidate_id: string;
  title?: string;
  owner?: string;
  summary?: string;
  decision?: string;
  [key: string]: unknown;
}

export interface CaseStatusRequest {
  status: CaseStatus;
  decision?: string;
  [key: string]: unknown;
}

export interface ReportsResponse {
  ok: boolean;
  role_context?: RoleContext;
  generated_at_utc?: ISODateTime;
  executive_points?: string[];
  executive_dashboard?: ExecutiveDashboard;
  risk_narrative?: RiskNarrative;
  agent_quality?: AgentQuality;
  agent_coverage_sla?: AgentCoverageSla;
  business_risk?: BusinessRiskItem[];
  risk_incident_candidates?: IncidentCandidate[];
  cases?: CaseItem[];
  [key: string]: unknown;
}

export interface UebaResponse {
  ok: boolean;
  role_context?: RoleContext;
  score: number | null;
  severity: "normal" | "low" | "medium" | "high" | "critical" | string;
  status?: string;
  score_components: {
    activity_anomaly: number;
    time_anomaly: number;
    application_anomaly: number;
    network_anomaly: number;
    history_anomaly: number;
  };
  reason_codes: string[];
  explanation: string;
  model: JsonObject;
  risk: JsonObject;
  [key: string]: unknown;
}

export interface PfsenseFirewallEvent {
  timestamp: ISODateTime;
  source_host: string;
  destination: string;
  action: string;
  rule_id?: string;
  protocol?: string;
  [key: string]: unknown;
}

export interface PfsenseVpnEvent {
  timestamp: ISODateTime;
  source_host: string;
  user_ref?: string;
  action: string;
  tunnel?: string;
  [key: string]: unknown;
}

export interface PfsenseReadinessResponse {
  ok: boolean;
  role_context?: RoleContext;
  contract_version: string;
  status: "contract_only" | "available" | string;
  siem: boolean;
  ingestion_available: boolean;
  firewall_events: PfsenseFirewallEvent[];
  vpn_events: PfsenseVpnEvent[];
  traffic_summary: JsonObject;
  top_destinations: JsonObject[];
  [key: string]: unknown;
}

export interface CaseListResponse {
  ok: boolean;
  cases: CaseItem[];
  [key: string]: unknown;
}

export interface DetMirPortalApi {
  getContracts(): Promise<ContractIndex>;
  getHealth(): Promise<JsonObject>;
  getOperator(): Promise<JsonObject>;
  getManager(): Promise<JsonObject>;
  getOwner(): Promise<JsonObject>;
  getReports(options?: { anonymize?: boolean; role?: PortalRole }): Promise<ReportsResponse>;
  getExecutive(options?: { role?: PortalRole }): Promise<ReportsResponse>;
  getWorkforce(options?: { role?: PortalRole }): Promise<ReportsResponse>;
  getSecurity(options?: { role?: PortalRole }): Promise<ReportsResponse>;
  getForensics(options?: { role?: PortalRole }): Promise<ReportsResponse>;
  getUeba(options?: { role?: PortalRole }): Promise<UebaResponse>;
  getPfsense(options?: { role?: PortalRole }): Promise<PfsenseReadinessResponse>;
  getIncidents(): Promise<JsonObject>;
  getCases(): Promise<CaseListResponse>;
  createCase(request: CreateCaseRequest): Promise<JsonObject>;
  setCaseStatus(caseId: string, request: CaseStatusRequest): Promise<JsonObject>;
  setIncidentReview(request: IncidentReviewRequest): Promise<JsonObject>;
  getInvestigationPack(
    candidateId: string,
    options?: { format?: "json" | "markdown" },
  ): Promise<JsonObject | string>;
  getDlpEvidence(): Promise<JsonObject>;
  getReadinessLatest(): Promise<JsonObject>;
  getReadinessBundle(): Promise<JsonObject>;
  verifyReadiness(): Promise<JsonObject>;
  getWorkforcePolicyExplain(options?: { anonymize?: boolean }): Promise<JsonObject>;
}
