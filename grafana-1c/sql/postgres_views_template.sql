-- Шаблон для PostgreSQL-базы 1С.
-- ВНИМАНИЕ: замените onec_raw.* на реальные таблицы/представления вашей конфигурации 1С.

CREATE OR REPLACE VIEW onec_kpi_unposted_documents AS
SELECT
  'main'::text AS company,
  COUNT(*)::double precision AS value
FROM onec_raw.documents d
WHERE d.posted = false;

CREATE OR REPLACE VIEW onec_kpi_sales_today AS
SELECT
  'main'::text AS company,
  COALESCE(d.currency, 'RUB')::text AS currency,
  COALESCE(SUM(d.amount), 0)::double precision AS value
FROM onec_raw.sales_documents d
WHERE d.posted = true
  AND d.doc_date::date = CURRENT_DATE
GROUP BY COALESCE(d.currency, 'RUB');

CREATE OR REPLACE VIEW onec_kpi_overdue_receivables AS
SELECT
  'main'::text AS company,
  CASE
    WHEN age_days BETWEEN 1 AND 7 THEN '1-7'
    WHEN age_days BETWEEN 8 AND 30 THEN '8-30'
    WHEN age_days BETWEEN 31 AND 90 THEN '31-90'
    ELSE '90+'
  END::text AS aging_bucket,
  COALESCE(currency, 'RUB')::text AS currency,
  COALESCE(SUM(amount_due), 0)::double precision AS value
FROM (
  SELECT
    r.currency,
    r.amount_due,
    GREATEST((CURRENT_DATE - r.due_date::date), 0) AS age_days
  FROM onec_raw.receivables r
  WHERE r.amount_due > 0
) x
WHERE age_days > 0
GROUP BY
  CASE
    WHEN age_days BETWEEN 1 AND 7 THEN '1-7'
    WHEN age_days BETWEEN 8 AND 30 THEN '8-30'
    WHEN age_days BETWEEN 31 AND 90 THEN '31-90'
    ELSE '90+'
  END,
  COALESCE(currency, 'RUB');

CREATE OR REPLACE VIEW onec_kpi_posting_errors_24h AS
SELECT
  'main'::text AS company,
  COUNT(*)::double precision AS value
FROM onec_raw.posting_errors e
WHERE e.error_at >= NOW() - INTERVAL '24 hours';

CREATE OR REPLACE VIEW onec_kpi_data_freshness AS
SELECT
  'main'::text AS company,
  'documents'::text AS source,
  COALESCE(EXTRACT(EPOCH FROM (NOW() - MAX(d.updated_at))), 999999)::double precision AS value
FROM onec_raw.documents d;
