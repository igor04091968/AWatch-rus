-- Шаблон для MS SQL-базы 1С.
-- ВНИМАНИЕ: замените dbo.onec_raw_* на реальные объекты вашей конфигурации 1С.

IF OBJECT_ID('dbo.onec_kpi_unposted_documents', 'V') IS NOT NULL DROP VIEW dbo.onec_kpi_unposted_documents;
GO
CREATE VIEW dbo.onec_kpi_unposted_documents AS
SELECT
  CAST('main' AS nvarchar(64)) AS company,
  CAST(COUNT(*) AS float) AS value
FROM dbo.onec_raw_documents d
WHERE d.posted = 0;
GO

IF OBJECT_ID('dbo.onec_kpi_sales_today', 'V') IS NOT NULL DROP VIEW dbo.onec_kpi_sales_today;
GO
CREATE VIEW dbo.onec_kpi_sales_today AS
SELECT
  CAST('main' AS nvarchar(64)) AS company,
  CAST(ISNULL(d.currency, 'RUB') AS nvarchar(16)) AS currency,
  CAST(ISNULL(SUM(d.amount), 0) AS float) AS value
FROM dbo.onec_raw_sales_documents d
WHERE d.posted = 1
  AND CAST(d.doc_date AS date) = CAST(GETDATE() AS date)
GROUP BY ISNULL(d.currency, 'RUB');
GO

IF OBJECT_ID('dbo.onec_kpi_overdue_receivables', 'V') IS NOT NULL DROP VIEW dbo.onec_kpi_overdue_receivables;
GO
CREATE VIEW dbo.onec_kpi_overdue_receivables AS
SELECT
  CAST('main' AS nvarchar(64)) AS company,
  CAST(
    CASE
      WHEN age_days BETWEEN 1 AND 7 THEN '1-7'
      WHEN age_days BETWEEN 8 AND 30 THEN '8-30'
      WHEN age_days BETWEEN 31 AND 90 THEN '31-90'
      ELSE '90+'
    END AS nvarchar(16)
  ) AS aging_bucket,
  CAST(ISNULL(currency, 'RUB') AS nvarchar(16)) AS currency,
  CAST(SUM(amount_due) AS float) AS value
FROM (
  SELECT
    r.currency,
    r.amount_due,
    CASE WHEN DATEDIFF(day, CAST(r.due_date AS date), CAST(GETDATE() AS date)) > 0
         THEN DATEDIFF(day, CAST(r.due_date AS date), CAST(GETDATE() AS date))
         ELSE 0 END AS age_days
  FROM dbo.onec_raw_receivables r
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
  ISNULL(currency, 'RUB');
GO

IF OBJECT_ID('dbo.onec_kpi_posting_errors_24h', 'V') IS NOT NULL DROP VIEW dbo.onec_kpi_posting_errors_24h;
GO
CREATE VIEW dbo.onec_kpi_posting_errors_24h AS
SELECT
  CAST('main' AS nvarchar(64)) AS company,
  CAST(COUNT(*) AS float) AS value
FROM dbo.onec_raw_posting_errors e
WHERE e.error_at >= DATEADD(hour, -24, GETDATE());
GO

IF OBJECT_ID('dbo.onec_kpi_data_freshness', 'V') IS NOT NULL DROP VIEW dbo.onec_kpi_data_freshness;
GO
CREATE VIEW dbo.onec_kpi_data_freshness AS
SELECT
  CAST('main' AS nvarchar(64)) AS company,
  CAST('documents' AS nvarchar(64)) AS source,
  CAST(
    CASE
      WHEN MAX(d.updated_at) IS NULL THEN 999999
      ELSE DATEDIFF(second, MAX(d.updated_at), GETDATE())
    END AS float
  ) AS value
FROM dbo.onec_raw_documents d;
GO
