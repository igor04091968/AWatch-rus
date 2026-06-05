export type ISODateTime = string;

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
  getReports(options?: { anonymize?: boolean }): Promise<ReportsResponse>;
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
